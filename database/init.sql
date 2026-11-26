-- ==============================================================================
-- init.sql - Genetics Database Schema
-- ==============================================================================
-- Description: PostgreSQL schema for genetic data processing service
-- Author: Matt Barham
-- Created: 2025-10-31
-- Modified: 2026-11-25
-- Version: 1.3.1
-- Security: Row-level security (FORCED, non-owner runtime role), append-only
--           audit log, token-based downloads. See
--           docs/adr/0001-rls-role-separation.md and
--           database/migrations/004_separate_runtime_role_from_owner.sql for
--           why this schema assumes two distinct roles (genetics_owner,
--           genetics_app) instead of one. On a fresh deployment,
--           database/00-create-app-role.sh runs before this file (via
--           docker-entrypoint-initdb.d ordering) and creates genetics_app;
--           POSTGRES_USER for a fresh deployment should be genetics_owner.
--
-- This file is the complete schema for a fresh deployment: it includes
-- everything migrations 001-004 add. Do NOT run those migrations after it.
-- They exist to upgrade older volumes. A schema change needs both a new
-- migration and the same change here.
-- ==============================================================================

-- Create genetics database schema
CREATE SCHEMA IF NOT EXISTS genetics;

-- Set search path
SET search_path TO genetics, public;

-- genetics_owner (POSTGRES_USER, the bootstrap role running this script) is
-- the table owner for everything below and is PERMANENTLY a superuser -
-- Postgres refuses to let the bootstrap superuser ever drop SUPERUSER
-- ("permission denied to alter role ... The bootstrap superuser must have
-- the SUPERUSER attribute"), confirmed empirically against Postgres 18.3.
-- That's fine: superuser already implies bypassing row security
-- unconditionally, so the explicit BYPASSRLS below is redundant in
-- practice but kept for clarity/documentation. The actual security
-- boundary was never "genetics_owner isn't superuser" - it's that
-- genetics_owner's credentials never appear in api-gateway or worker's
-- DATABASE_URL (see docker-compose.yml). See
-- docs/adr/0001-rls-role-separation.md.
--
-- This only works because a fresh deployment's POSTGRES_USER is
-- genetics_owner directly (see .env.example) - the role already exists by
-- the time this script runs. An existing deployment upgrading in place
-- never re-runs this file (docker-entrypoint-initdb.d only fires on an
-- empty volume); it gets to the same end state via
-- database/migrations/004_separate_runtime_role_from_owner.sql instead,
-- targeting CURRENT_USER rather than the literal name genetics_owner -
-- Postgres refuses to let a role rename itself, so the older "genetics_api"
-- bootstrap role is never renamed and keeps that name permanently on an
-- upgraded deployment (cosmetic difference only, not a security one - see
-- the migration file for the full explanation).
ALTER ROLE genetics_owner BYPASSRLS;
ALTER ROLE genetics_owner SET search_path TO genetics, public;
ALTER ROLE genetics_app SET search_path TO genetics, public;

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
    metadata JSONB DEFAULT '{}'::jsonb,

    -- Email delivery and token-based download (migration 001)
    user_email TEXT,
    download_token TEXT UNIQUE,
    download_password_hash TEXT, -- Argon2id
    download_attempts INTEGER DEFAULT 0 NOT NULL,
    last_download_attempt TIMESTAMPTZ,
    max_download_attempts INTEGER DEFAULT 5 NOT NULL,
    emailed_at TIMESTAMPTZ,

    -- Email resend rate limit (migration 003)
    last_email_resent_at TIMESTAMPTZ,

    CONSTRAINT genetics_jobs_email_format CHECK (
        user_email IS NULL OR user_email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
    ),
    CONSTRAINT genetics_jobs_download_attempts_valid CHECK (
        download_attempts >= 0 AND download_attempts <= max_download_attempts
    )
);

-- Indexes for jobs table
CREATE INDEX idx_genetics_jobs_user_id ON genetics_jobs(user_id);
CREATE INDEX idx_genetics_jobs_status ON genetics_jobs(status);
CREATE INDEX idx_genetics_jobs_created_at ON genetics_jobs(created_at DESC);
CREATE INDEX idx_genetics_jobs_expires_at ON genetics_jobs(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX idx_genetics_jobs_status_started_at ON genetics_jobs(status, started_at); -- Composite index for stuck jobs query
CREATE INDEX idx_genetics_jobs_download_token ON genetics_jobs(download_token)
    WHERE download_token IS NOT NULL;
CREATE INDEX idx_genetics_jobs_user_email ON genetics_jobs(user_email)
    WHERE user_email IS NOT NULL;
CREATE INDEX idx_genetics_jobs_needs_email ON genetics_jobs(status, emailed_at)
    WHERE status = 'completed' AND emailed_at IS NULL;
CREATE INDEX idx_genetics_jobs_download_rate_limit ON genetics_jobs(
    download_token,
    download_attempts,
    last_download_attempt
) WHERE download_token IS NOT NULL;

-- Enable row-level security
ALTER TABLE genetics_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE genetics_jobs FORCE ROW LEVEL SECURITY;

-- Policy: Users can only see their own jobs
CREATE POLICY genetics_jobs_isolation ON genetics_jobs
    FOR ALL
    TO genetics_app
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
ALTER TABLE genetics_files FORCE ROW LEVEL SECURITY;

-- Policy: Users can only see their own files
CREATE POLICY genetics_files_isolation ON genetics_files
    FOR ALL
    TO genetics_app
    USING (user_id = current_setting('app.current_user_id', TRUE)::TEXT)
    WITH CHECK (user_id = current_setting('app.current_user_id', TRUE)::TEXT);

-- ==============================================================================
-- DOWNLOAD ATTEMPTS TABLE (append-only; rows leave only with their job)
-- ==============================================================================

CREATE TABLE genetics_download_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES genetics_jobs(id) ON DELETE CASCADE,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ip_address TEXT,
    user_agent TEXT,
    token_provided BOOLEAN NOT NULL,
    token_valid BOOLEAN,
    password_provided BOOLEAN NOT NULL,
    password_valid BOOLEAN,
    attempt_result TEXT NOT NULL CHECK (
        attempt_result IN (
            'success',
            'invalid_token',
            'invalid_password',
            'max_attempts_exceeded',
            'job_expired',
            'job_not_found',
            'rate_limited'
        )
    ),
    details JSONB DEFAULT '{}'::jsonb
);

CREATE INDEX idx_genetics_download_attempts_job_id ON genetics_download_attempts(job_id);
CREATE INDEX idx_genetics_download_attempts_timestamp ON genetics_download_attempts(attempted_at DESC);
CREATE INDEX idx_genetics_download_attempts_result ON genetics_download_attempts(attempt_result);
CREATE INDEX idx_genetics_download_attempts_ip ON genetics_download_attempts(ip_address);

CREATE RULE genetics_download_attempts_no_update AS
    ON UPDATE TO genetics_download_attempts DO INSTEAD NOTHING;

CREATE OR REPLACE FUNCTION audit_download_attempt_deletion()
RETURNS TRIGGER AS $$
BEGIN
    -- Log the deletion to genetics_audit
    INSERT INTO genetics_audit (
        event_type,
        user_id,
        action,
        result,
        details,
        severity
    ) VALUES (
        'job_cleanup',
        'system',
        'download_attempt_deleted',
        'success',
        jsonb_build_object(
            'download_attempt_id', OLD.id,
            'job_id', OLD.job_id,
            'attempted_at', OLD.attempted_at,
            'attempt_result', OLD.attempt_result
        ),
        'info'
    );
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

-- Create trigger to audit deletions (fires BEFORE delete)
CREATE TRIGGER trigger_audit_download_attempt_deletion
    BEFORE DELETE ON genetics_download_attempts
    FOR EACH ROW
    EXECUTE FUNCTION audit_download_attempt_deletion();

-- ==============================================================================
-- RECOVERY CODES TABLE (self-service deletion)
-- ==============================================================================

CREATE TABLE IF NOT EXISTS genetics_recovery_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES genetics_jobs(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used BOOLEAN DEFAULT false NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recovery_codes_job_id ON genetics_recovery_codes(job_id);
CREATE INDEX idx_recovery_codes_unused ON genetics_recovery_codes(job_id)
    WHERE used = false;

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
GRANT INSERT ON genetics_audit TO genetics_app;

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
-- CROSS-USER ACCESS FUNCTIONS (SECURITY DEFINER, pinned search_path)
-- ==============================================================================
-- Owned by genetics_owner (BYPASSRLS). GeneGnome has no login session -
-- most api-gateway handlers only ever receive a job_id (itself a bearer
-- capability), never the owning user's email - so app.current_user_id has
-- to be resolved from the row before the real, RLS-restricted query can
-- run. See docs/adr/0001-rls-role-separation.md.

CREATE OR REPLACE FUNCTION genetics.resolve_job_owner(p_job_id UUID)
RETURNS TEXT
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = genetics, pg_catalog
AS $$
    SELECT user_id FROM genetics.genetics_jobs WHERE id = p_job_id;
$$;

COMMENT ON FUNCTION genetics.resolve_job_owner(UUID) IS
    'SECURITY DEFINER (bypasses RLS as genetics_owner). Returns only the owning user_id for a job_id, nothing else. Used to seed app.current_user_id before RLS-scoped queries in api-gateway.';

CREATE OR REPLACE FUNCTION genetics.resolve_job_owner_by_token(p_token TEXT)
RETURNS TABLE(id UUID, user_id TEXT)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = genetics, pg_catalog
AS $$
    SELECT id, user_id FROM genetics.genetics_jobs WHERE download_token = p_token;
$$;

COMMENT ON FUNCTION genetics.resolve_job_owner_by_token(TEXT) IS
    'SECURITY DEFINER (bypasses RLS as genetics_owner). Resolves id/user_id from a download_token, nothing else. Used to seed app.current_user_id before RLS-scoped download/visualization queries in api-gateway.';

-- Cross-user sweeps for worker's stuck-job recovery and 24h cleanup loops.
-- Each returned job is then acted on inside its own RLS-scoped, per-owner
-- transaction - these functions only replace the initial cross-user
-- discovery SELECT, not the mutations.
CREATE OR REPLACE FUNCTION genetics.list_stuck_jobs(p_started_before TIMESTAMPTZ)
RETURNS TABLE(id UUID, user_id TEXT)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = genetics, pg_catalog
AS $$
    SELECT id, user_id FROM genetics.genetics_jobs
    WHERE status = 'processing' AND started_at < p_started_before;
$$;

COMMENT ON FUNCTION genetics.list_stuck_jobs(TIMESTAMPTZ) IS
    'SECURITY DEFINER (bypasses RLS as genetics_owner). Cross-user discovery for worker stuck-job recovery; returns only id/user_id, no job content.';

CREATE OR REPLACE FUNCTION genetics.list_jobs_for_cleanup(p_completed_before TIMESTAMPTZ)
RETURNS TABLE(id UUID, user_id TEXT)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = genetics, pg_catalog
AS $$
    SELECT id, user_id FROM genetics.genetics_jobs
    WHERE (status IN ('completed', 'failed') AND completed_at < p_completed_before)
       OR status = 'user_deleted';
$$;

COMMENT ON FUNCTION genetics.list_jobs_for_cleanup(TIMESTAMPTZ) IS
    'SECURITY DEFINER (bypasses RLS as genetics_owner). Cross-user discovery for worker''s 24h job cleanup sweep; returns only id/user_id, no job content.';

-- ==============================================================================
-- DOWNLOAD RATE LIMIT FUNCTIONS (operator use)
-- ==============================================================================

CREATE OR REPLACE FUNCTION check_download_allowed(job_uuid UUID)
RETURNS BOOLEAN AS $$
DECLARE
    job_record RECORD;
    rate_limit_window INTERVAL := '1 minute';
    max_attempts_per_window INTEGER := 3;
    recent_attempts INTEGER;
BEGIN
    -- Get job record
    SELECT * INTO job_record
    FROM genetics_jobs
    WHERE id = job_uuid;

    -- Job must exist
    IF NOT FOUND THEN
        RETURN FALSE;
    END IF;

    -- Job must be completed
    IF job_record.status != 'completed' THEN
        RETURN FALSE;
    END IF;

    -- Job must not be expired
    IF job_record.expires_at IS NOT NULL AND job_record.expires_at < NOW() THEN
        RETURN FALSE;
    END IF;

    -- Check max attempts not exceeded
    IF job_record.download_attempts >= job_record.max_download_attempts THEN
        RETURN FALSE;
    END IF;

    -- Check rate limiting (max 3 attempts per minute)
    IF job_record.last_download_attempt IS NOT NULL THEN
        SELECT COUNT(*) INTO recent_attempts
        FROM genetics_download_attempts
        WHERE job_id = job_uuid
            AND attempted_at > NOW() - rate_limit_window;

        IF recent_attempts >= max_attempts_per_window THEN
            RETURN FALSE;
        END IF;
    END IF;

    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION check_download_allowed(UUID) IS 'Validates if a download attempt should be allowed (rate limiting + max attempts)';

-- Function to increment download attempts
CREATE OR REPLACE FUNCTION increment_download_attempt(job_uuid UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE genetics_jobs
    SET download_attempts = download_attempts + 1,
        last_download_attempt = NOW()
    WHERE id = job_uuid;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION increment_download_attempt(UUID) IS 'Increments download attempt counter and updates timestamp';

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

CREATE OR REPLACE VIEW genetics_download_stats AS
SELECT
    j.user_email,
    COUNT(DISTINCT j.id) as total_jobs_with_downloads,
    COUNT(DISTINCT CASE WHEN j.download_attempts > 0 THEN j.id END) as jobs_with_attempts,
    SUM(j.download_attempts) as total_download_attempts,
    COUNT(DISTINCT da.id) FILTER (WHERE da.attempt_result = 'success') as successful_downloads,
    COUNT(DISTINCT da.id) FILTER (WHERE da.attempt_result = 'invalid_password') as failed_password_attempts,
    COUNT(DISTINCT da.id) FILTER (WHERE da.attempt_result = 'max_attempts_exceeded') as locked_out_jobs,
    MAX(j.emailed_at) as last_email_sent
FROM genetics_jobs j
LEFT JOIN genetics_download_attempts da ON j.id = da.job_id
WHERE j.user_email IS NOT NULL
GROUP BY j.user_email;

COMMENT ON VIEW genetics_download_stats IS 'Download security statistics per user email';

-- View for failed download attempts (security monitoring)
CREATE OR REPLACE VIEW genetics_failed_downloads AS
SELECT
    da.attempted_at,
    da.ip_address,
    da.user_agent,
    da.attempt_result,
    j.user_email,
    j.id as job_id,
    j.download_attempts,
    j.max_download_attempts
FROM genetics_download_attempts da
JOIN genetics_jobs j ON da.job_id = j.id
WHERE da.attempt_result != 'success'
ORDER BY da.attempted_at DESC;

COMMENT ON VIEW genetics_failed_downloads IS 'All failed download attempts for security monitoring';

-- ==============================================================================
-- GRANTS
-- ==============================================================================

-- Grant necessary permissions to the runtime role. Re-derived from the
-- actual query inventory in api-gateway and worker, not a blanket grant -
-- notably genetics_app does NOT get genetics_job_stats / genetics_recent_audit
-- / genetics_security_events (ops-only views, queried manually via
-- genetics_owner, never by application code) and does NOT get DELETE or
-- SELECT/UPDATE beyond what each table actually needs.
GRANT USAGE ON SCHEMA genetics TO genetics_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON genetics_jobs TO genetics_app;
GRANT SELECT, INSERT ON genetics_files TO genetics_app;
GRANT SELECT, INSERT, UPDATE ON genetics_recovery_codes TO genetics_app;
GRANT INSERT ON genetics_download_attempts TO genetics_app;

GRANT EXECUTE ON FUNCTION genetics.resolve_job_owner(UUID) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.resolve_job_owner_by_token(TEXT) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.list_stuck_jobs(TIMESTAMPTZ) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.list_jobs_for_cleanup(TIMESTAMPTZ) TO genetics_app;

-- Grant sequence usage
GRANT USAGE ON ALL SEQUENCES IN SCHEMA genetics TO genetics_app;

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
    '{"schema_version": "1.3.0", "created_at": "2025-10-31"}'::jsonb,
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
