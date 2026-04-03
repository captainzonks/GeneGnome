-- ==============================================================================
-- 003_add_recovery_codes_and_resend.sql - Recovery Codes & Email Resend
-- ==============================================================================
-- Description: Adds recovery codes table for self-service deletion and
--              email resend rate limiting column
-- Author: Matt Barham
-- Created: 2026-04-02
-- Modified: 2026-04-02
-- Version: 1.2.0
-- Schema Version: 1.1.0 → 1.2.0
-- ==============================================================================
-- Migration Path: Recovery codes + email resend support
-- Dependencies: 001_add_email_downloads.sql
-- Rollback: See rollback section at end of file
-- ==============================================================================

SET search_path TO genetics, public;

-- ==============================================================================
-- RECOVERY CODES TABLE
-- ==============================================================================
-- 8 single-use recovery codes per job, Argon2id-hashed.
-- Used for self-service data deletion without email access.

CREATE TABLE IF NOT EXISTS genetics_recovery_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES genetics_jobs(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used BOOLEAN DEFAULT false NOT NULL,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_job_id
    ON genetics_recovery_codes(job_id);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_unused
    ON genetics_recovery_codes(job_id)
    WHERE used = false;

-- ==============================================================================
-- JOBS TABLE MODIFICATIONS
-- ==============================================================================
-- Track email resend timing separately from initial emailed_at

ALTER TABLE genetics_jobs
    ADD COLUMN IF NOT EXISTS last_email_resent_at TIMESTAMPTZ;

-- ==============================================================================
-- PERMISSIONS
-- ==============================================================================

GRANT SELECT, INSERT, UPDATE ON genetics_recovery_codes TO genetics_api;

-- ==============================================================================
-- ROLLBACK
-- ==============================================================================
-- To undo this migration:
--   DROP TABLE IF EXISTS genetics_recovery_codes;
--   ALTER TABLE genetics_jobs DROP COLUMN IF EXISTS last_email_resent_at;
