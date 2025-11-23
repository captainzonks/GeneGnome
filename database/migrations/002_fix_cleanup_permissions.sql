-- ==============================================================================
-- 002_fix_cleanup_permissions.sql - Fix cleanup permissions for worker
-- ==============================================================================
-- Description: Grant DELETE permission and allow cascade deletes for cleanup
-- Author: Matthew Barham
-- Created: 2025-11-22
-- Modified: 2025-11-22
-- Version: 1.1.1
-- Issue: Worker cleanup fails due to missing DELETE permission and rule conflicts
-- ==============================================================================

SET search_path TO genetics, public;

-- ==============================================================================
-- GRANT DELETE PERMISSION
-- ==============================================================================

-- Allow genetics_api (worker) to delete old jobs
GRANT DELETE ON genetics_jobs TO genetics_api;

-- ==============================================================================
-- FIX DOWNLOAD ATTEMPTS RULE
-- ==============================================================================

-- Drop the original no-delete rule
DROP RULE IF EXISTS genetics_download_attempts_no_delete ON genetics_download_attempts;

-- Create a new rule that allows CASCADE deletes but blocks direct deletes
-- This maintains audit integrity while allowing cleanup
CREATE OR REPLACE FUNCTION block_direct_download_attempts_delete()
RETURNS TRIGGER AS $$
BEGIN
    -- Allow CASCADE deletes from genetics_jobs deletion
    -- Block direct DELETE commands
    IF TG_OP = 'DELETE' THEN
        -- Check if this is a cascading delete by examining the context
        -- Cascading deletes don't raise errors, direct deletes do
        RAISE EXCEPTION 'Direct deletion from genetics_download_attempts is not allowed (append-only table for audit)'
            USING HINT = 'Download attempts are automatically deleted when their parent job is deleted';
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Actually, PostgreSQL rules are better for this - let's use a rule that allows
-- CASCADE but denies direct deletes by checking the trigger depth

-- Drop function approach, use rule approach instead
DROP FUNCTION IF EXISTS block_direct_download_attempts_delete();

-- The issue is that rules fire BEFORE constraint checks
-- We need to allow the CASCADE to succeed
-- The best approach is to remove the rule entirely and rely on permissions

-- For audit purposes: Document deletions via a trigger that logs to genetics_audit
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
-- COMMENTS
-- ==============================================================================

COMMENT ON FUNCTION audit_download_attempt_deletion() IS 'Logs download attempt deletions to genetics_audit for compliance';
COMMENT ON TRIGGER trigger_audit_download_attempt_deletion ON genetics_download_attempts IS 'Audit trail for deleted download attempts (cascade cleanup)';

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
        'migration', '002_fix_cleanup_permissions',
        'old_version', '1.1.0',
        'new_version', '1.1.1',
        'description', 'Grant DELETE permission to genetics_api and fix cascade delete rule conflict',
        'tables_modified', jsonb_build_array('genetics_jobs', 'genetics_download_attempts'),
        'changes', jsonb_build_array(
            'Granted DELETE permission on genetics_jobs to genetics_api',
            'Removed genetics_download_attempts_no_delete rule',
            'Added audit trigger for download attempt deletions'
        ),
        'timestamp', NOW()
    ),
    'info'
);

-- ==============================================================================
-- ROLLBACK SCRIPT (for reference only - do not execute)
-- ==============================================================================

/*
-- ROLLBACK: Revert to schema version 1.1.0

SET search_path TO genetics, public;

-- Remove trigger and function
DROP TRIGGER IF EXISTS trigger_audit_download_attempt_deletion ON genetics_download_attempts;
DROP FUNCTION IF EXISTS audit_download_attempt_deletion();

-- Restore no-delete rule
CREATE RULE genetics_download_attempts_no_delete AS
    ON DELETE TO genetics_download_attempts DO INSTEAD NOTHING;

-- Revoke DELETE permission
REVOKE DELETE ON genetics_jobs FROM genetics_api;

-- Record rollback in audit log
INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'schema_rollback',
    'success',
    jsonb_build_object(
        'migration', '002_fix_cleanup_permissions',
        'from_version', '1.1.1',
        'to_version', '1.1.0',
        'timestamp', NOW()
    ),
    'warning'
);
*/
