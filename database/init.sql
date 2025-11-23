-- ==============================================================================
-- init.sql - Genetics Database Schema
-- ==============================================================================
-- Description: PostgreSQL schema for genetic data processing service
-- Author: Matt Barham
-- Created: 2025-10-31
-- Modified: 2025-11-18
-- Version: 1.1.0
-- Security: Row-level security, append-only audit log, token-based downloads
-- ==============================================================================

-- Create genetics database schema
CREATE SCHEMA IF NOT EXISTS genetics;

-- Set search path
SET search_path TO genetics, public;

-- ==============================================================================
-- JOBS TABLE
-- ==============================================================================

CREATE TABLE IF NOT EXISTS genetics_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'completed', 'failed', 'expired', 'user_deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    error_message TEXT,
    result_path TEXT,
    result_hash_sha256 TEXT,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Indexes for jobs table
CREATE INDEX idx_genetics_jobs_user_id ON genetics_jobs(user_id);
CREATE INDEX idx_genetics_jobs_status ON genetics_jobs(status);
CREATE INDEX idx_genetics_jobs_created_at ON genetics_jobs(created_at DESC);
CREATE INDEX idx_genetics_jobs_expires_at ON genetics_jobs(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX idx_genetics_jobs_status_started_at ON genetics_jobs(status, started_at); -- Composite index for stuck jobs query

-- Enable row-level security
ALTER TABLE genetics_jobs ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own jobs
CREATE POLICY genetics_jobs_isolation ON genetics_jobs
    FOR ALL
    TO genetics_api
    USING (user_id = current_setting('app.current_user_id', TRUE)::TEXT)
    WITH CHECK (user_id = current_setting('app.current_user_id', TRUE)::TEXT);

-- ==============================================================================
-- FILES TABLE
-- ==============================================================================

CREATE TABLE IF NOT EXISTS genetics_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES genetics_jobs(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_type TEXT NOT NULL CHECK (file_type IN ('genome', 'vcf', 'vcf_index', 'pgs', 'result')),
    file_size BIGINT NOT NULL,
    hash_sha256 TEXT NOT NULL,
    uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Indexes for files table
CREATE INDEX idx_genetics_files_job_id ON genetics_files(job_id);
CREATE INDEX idx_genetics_files_user_id ON genetics_files(user_id);
CREATE INDEX idx_genetics_files_uploaded_at ON genetics_files(uploaded_at DESC);
CREATE INDEX idx_genetics_files_hash ON genetics_files(hash_sha256);

-- Enable row-level security
ALTER TABLE genetics_files ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own files
CREATE POLICY genetics_files_isolation ON genetics_files
    FOR ALL
    TO genetics_api
    USING (user_id = current_setting('app.current_user_id', TRUE)::TEXT)
    WITH CHECK (user_id = current_setting('app.current_user_id', TRUE)::TEXT);

-- ==============================================================================
-- AUDIT LOG TABLE
-- ==============================================================================

CREATE TABLE IF NOT EXISTS genetics_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_type TEXT NOT NULL,
    user_id TEXT,
    session_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    resource TEXT,
    action TEXT NOT NULL,
    result TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error', 'critical'))
);

-- Indexes for audit table
CREATE INDEX idx_genetics_audit_user_id ON genetics_audit(user_id);
CREATE INDEX idx_genetics_audit_timestamp ON genetics_audit(timestamp DESC);
CREATE INDEX idx_genetics_audit_event_type ON genetics_audit(event_type);
CREATE INDEX idx_genetics_audit_severity ON genetics_audit(severity);
CREATE INDEX idx_genetics_audit_details ON genetics_audit USING GIN(details);

-- Grant insert-only access to audit table
GRANT INSERT ON genetics_audit TO genetics_api;
REVOKE UPDATE, DELETE ON genetics_audit FROM genetics_api;

-- Prevent updates and deletes on audit table (append-only)
CREATE RULE genetics_audit_no_update AS
    ON UPDATE TO genetics_audit DO INSTEAD NOTHING;

CREATE RULE genetics_audit_no_delete AS
    ON DELETE TO genetics_audit DO INSTEAD NOTHING;

-- ==============================================================================
-- FUNCTIONS
-- ==============================================================================

-- Function to automatically set expires_at when job completes
CREATE OR REPLACE FUNCTION set_job_expiration()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'completed' AND OLD.status != 'completed' THEN
        NEW.expires_at := NOW() + INTERVAL '24 hours';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger for auto-expiration
CREATE TRIGGER trigger_set_job_expiration
    BEFORE UPDATE ON genetics_jobs
    FOR EACH ROW
    EXECUTE FUNCTION set_job_expiration();

-- Function to clean up expired jobs
CREATE OR REPLACE FUNCTION cleanup_expired_jobs()
RETURNS INTEGER AS $$
DECLARE
    expired_count INTEGER;
BEGIN
    -- Update status of expired jobs
    UPDATE genetics_jobs
    SET status = 'expired'
    WHERE status = 'completed'
        AND expires_at IS NOT NULL
        AND expires_at < NOW();

    GET DIAGNOSTICS expired_count = ROW_COUNT;

    RETURN expired_count;
END;
$$ LANGUAGE plpgsql;

-- ==============================================================================
-- VIEWS
-- ==============================================================================

-- View for job statistics
CREATE OR REPLACE VIEW genetics_job_stats AS
SELECT
    user_id,
    COUNT(*) as total_jobs,
    COUNT(*) FILTER (WHERE status = 'completed') as completed_jobs,
    COUNT(*) FILTER (WHERE status = 'failed') as failed_jobs,
    COUNT(*) FILTER (WHERE status = 'processing') as processing_jobs,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) FILTER (WHERE status = 'completed') as avg_processing_time_seconds,
    MAX(created_at) as last_job_created_at
FROM genetics_jobs
GROUP BY user_id;

-- View for recent audit events (last 24 hours)
CREATE OR REPLACE VIEW genetics_recent_audit AS
SELECT *
FROM genetics_audit
WHERE timestamp > NOW() - INTERVAL '24 hours'
ORDER BY timestamp DESC;

-- View for security events
CREATE OR REPLACE VIEW genetics_security_events AS
SELECT *
FROM genetics_audit
WHERE severity IN ('warning', 'critical')
    OR event_type LIKE '%failure'
    OR event_type LIKE '%denied'
ORDER BY timestamp DESC;

-- ==============================================================================
-- GRANTS
-- ==============================================================================

-- Grant necessary permissions to API role
GRANT USAGE ON SCHEMA genetics TO genetics_api;
GRANT SELECT, INSERT, UPDATE ON genetics_jobs TO genetics_api;
GRANT SELECT, INSERT, UPDATE ON genetics_files TO genetics_api;
GRANT INSERT ON genetics_audit TO genetics_api;
GRANT SELECT ON genetics_job_stats TO genetics_api;
GRANT SELECT ON genetics_recent_audit TO genetics_api;

-- Grant sequence usage
GRANT USAGE ON ALL SEQUENCES IN SCHEMA genetics TO genetics_api;

-- ==============================================================================
-- INITIAL DATA
-- ==============================================================================

-- Insert initial audit event (database initialized)
INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'database_initialized',
    'success',
    '{"schema_version": "1.0.0", "created_at": "2025-10-31"}'::jsonb,
    'info'
);

-- ==============================================================================
-- COMMENTS
-- ==============================================================================

COMMENT ON TABLE genetics_jobs IS 'Job tracking for genetic data processing';
COMMENT ON TABLE genetics_files IS 'File metadata for uploaded and generated files';
COMMENT ON TABLE genetics_audit IS 'Append-only audit log (7-year retention required)';
COMMENT ON FUNCTION cleanup_expired_jobs() IS 'Cleanup function to mark expired jobs (run via cron)';
COMMENT ON VIEW genetics_job_stats IS 'Aggregated job statistics per user';
COMMENT ON VIEW genetics_security_events IS 'View of security-related audit events';
