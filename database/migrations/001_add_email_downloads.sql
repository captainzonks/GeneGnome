-- ==============================================================================
-- 001_add_email_downloads.sql - Email-Based Download Security Migration
-- ==============================================================================
-- Description: Adds email notification and secure download token functionality
-- Author: Matt Barham
-- Created: 2025-11-18
-- Modified: 2025-11-18
-- Version: 1.1.0
-- Schema Version: 1.0.2 → 1.1.0
-- Security: Token-based downloads with Argon2id password hashing
-- ==============================================================================
-- Migration Path: Phase 1 of 8 (Database Schema)
-- Dependencies: init.sql (v1.0.2)
-- Rollback: See rollback section at end of file
-- ==============================================================================

SET search_path TO genetics, public;

-- ==============================================================================
-- JOBS TABLE MODIFICATIONS
-- ==============================================================================

-- Add email and download security columns to genetics_jobs
ALTER TABLE genetics_jobs
    ADD COLUMN user_email TEXT,
    ADD COLUMN download_token TEXT UNIQUE,
    ADD COLUMN download_password_hash TEXT,
    ADD COLUMN download_attempts INTEGER DEFAULT 0 NOT NULL,
    ADD COLUMN last_download_attempt TIMESTAMPTZ,
    ADD COLUMN max_download_attempts INTEGER DEFAULT 5 NOT NULL,
    ADD COLUMN emailed_at TIMESTAMPTZ;

-- Add check constraint for valid email format
ALTER TABLE genetics_jobs
    ADD CONSTRAINT genetics_jobs_email_format CHECK (
        user_email IS NULL OR user_email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
    );

-- Add check constraint for download attempts logic
ALTER TABLE genetics_jobs
    ADD CONSTRAINT genetics_jobs_download_attempts_valid CHECK (
        download_attempts >= 0 AND download_attempts <= max_download_attempts
    );

-- Add comment explaining new columns
COMMENT ON COLUMN genetics_jobs.user_email IS 'User email address for download link notifications';
COMMENT ON COLUMN genetics_jobs.download_token IS 'Unique 256-bit secure token for download URL (URL-safe base64)';
COMMENT ON COLUMN genetics_jobs.download_password_hash IS 'Argon2id hash of download password (emailed separately)';
COMMENT ON COLUMN genetics_jobs.download_attempts IS 'Number of download attempts made (rate limiting)';
COMMENT ON COLUMN genetics_jobs.last_download_attempt IS 'Timestamp of most recent download attempt';
COMMENT ON COLUMN genetics_jobs.max_download_attempts IS 'Maximum allowed download attempts before lockout';
COMMENT ON COLUMN genetics_jobs.emailed_at IS 'Timestamp when download link was emailed to user';

-- ==============================================================================
-- DOWNLOAD ATTEMPTS AUDIT TABLE
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

-- Indexes for download attempts audit table
CREATE INDEX idx_genetics_download_attempts_job_id ON genetics_download_attempts(job_id);
CREATE INDEX idx_genetics_download_attempts_timestamp ON genetics_download_attempts(attempted_at DESC);
CREATE INDEX idx_genetics_download_attempts_result ON genetics_download_attempts(attempt_result);
CREATE INDEX idx_genetics_download_attempts_ip ON genetics_download_attempts(ip_address);

-- Comments for download attempts table
COMMENT ON TABLE genetics_download_attempts IS 'Audit log for all download attempts (security monitoring)';
COMMENT ON COLUMN genetics_download_attempts.token_provided IS 'Whether a token was provided in the request';
COMMENT ON COLUMN genetics_download_attempts.token_valid IS 'Whether the provided token matched (NULL if not provided)';
COMMENT ON COLUMN genetics_download_attempts.password_provided IS 'Whether a password was provided in the request';
COMMENT ON COLUMN genetics_download_attempts.password_valid IS 'Whether the provided password matched (NULL if not provided)';
COMMENT ON COLUMN genetics_download_attempts.attempt_result IS 'Final result of the download attempt';

-- Grant insert-only access to download attempts table
GRANT INSERT ON genetics_download_attempts TO genetics_api;
REVOKE UPDATE, DELETE ON genetics_download_attempts FROM genetics_api;

-- Prevent updates and deletes on download attempts table (append-only)
CREATE RULE genetics_download_attempts_no_update AS
    ON UPDATE TO genetics_download_attempts DO INSTEAD NOTHING;

CREATE RULE genetics_download_attempts_no_delete AS
    ON DELETE TO genetics_download_attempts DO INSTEAD NOTHING;

-- ==============================================================================
-- PERFORMANCE INDEXES
-- ==============================================================================

-- Index for download token lookups (primary access pattern)
CREATE INDEX idx_genetics_jobs_download_token ON genetics_jobs(download_token)
    WHERE download_token IS NOT NULL;

-- Index for email lookups (user may want to check their jobs)
CREATE INDEX idx_genetics_jobs_user_email ON genetics_jobs(user_email)
    WHERE user_email IS NOT NULL;

-- Index for identifying jobs that need email notifications
CREATE INDEX idx_genetics_jobs_needs_email ON genetics_jobs(status, emailed_at)
    WHERE status = 'completed' AND emailed_at IS NULL;

-- Composite index for download attempt rate limiting
CREATE INDEX idx_genetics_jobs_download_rate_limit ON genetics_jobs(
    download_token,
    download_attempts,
    last_download_attempt
) WHERE download_token IS NOT NULL;

-- ==============================================================================
-- FUNCTIONS
-- ==============================================================================

-- Function to check if download is allowed (rate limiting)
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

-- View for download security statistics
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

-- Grant permissions for new table and views
GRANT SELECT ON genetics_download_attempts TO genetics_api;
GRANT SELECT ON genetics_download_stats TO genetics_api;
GRANT SELECT ON genetics_failed_downloads TO genetics_api;

-- Grant execute on new functions
GRANT EXECUTE ON FUNCTION check_download_allowed(UUID) TO genetics_api;
GRANT EXECUTE ON FUNCTION increment_download_attempt(UUID) TO genetics_api;

-- ==============================================================================
-- SCHEMA VERSION UPDATE
-- ==============================================================================

-- Record migration in audit log
INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'schema_migration',
    'success',
    jsonb_build_object(
        'migration', '001_add_email_downloads',
        'old_version', '1.0.2',
        'new_version', '1.1.0',
        'description', 'Added email notification and secure download token functionality',
        'tables_modified', jsonb_build_array('genetics_jobs'),
        'tables_created', jsonb_build_array('genetics_download_attempts'),
        'timestamp', NOW()
    ),
    'info'
);

-- ==============================================================================
-- ROLLBACK SCRIPT (for reference only - do not execute)
-- ==============================================================================

/*
-- ROLLBACK: Revert to schema version 1.0.2

SET search_path TO genetics, public;

-- Drop new views
DROP VIEW IF EXISTS genetics_failed_downloads;
DROP VIEW IF EXISTS genetics_download_stats;

-- Drop new functions
DROP FUNCTION IF EXISTS increment_download_attempt(UUID);
DROP FUNCTION IF EXISTS check_download_allowed(UUID);

-- Drop new indexes
DROP INDEX IF EXISTS idx_genetics_jobs_download_rate_limit;
DROP INDEX IF EXISTS idx_genetics_jobs_needs_email;
DROP INDEX IF EXISTS idx_genetics_jobs_user_email;
DROP INDEX IF EXISTS idx_genetics_jobs_download_token;

-- Drop download attempts table
DROP TABLE IF EXISTS genetics_download_attempts;

-- Remove columns from genetics_jobs
ALTER TABLE genetics_jobs
    DROP COLUMN IF EXISTS emailed_at,
    DROP COLUMN IF EXISTS max_download_attempts,
    DROP COLUMN IF EXISTS last_download_attempt,
    DROP COLUMN IF EXISTS download_attempts,
    DROP COLUMN IF EXISTS download_password_hash,
    DROP COLUMN IF EXISTS download_token,
    DROP COLUMN IF EXISTS user_email;

-- Record rollback in audit log
INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'schema_rollback',
    'success',
    jsonb_build_object(
        'migration', '001_add_email_downloads',
        'old_version', '1.1.0',
        'new_version', '1.0.2',
        'timestamp', NOW()
    ),
    'warning'
);
*/
