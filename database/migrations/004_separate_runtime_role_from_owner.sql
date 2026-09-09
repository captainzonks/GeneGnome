-- ==============================================================================
-- 004_separate_runtime_role_from_owner.sql - Enforce RLS via role separation
-- ==============================================================================
-- Description: Splits the single "genetics_api" role (bootstrap superuser +
--              table owner + application role, all at once) into an owner
--              role and a non-superuser, non-owning runtime role. Adds
--              FORCE ROW LEVEL SECURITY and a narrow, audited path for the
--              two classes of legitimate cross-user access the app needs.
-- Author: Matt Barham
-- Created: 2026-09-08
-- Modified: 2026-09-08
-- Version: 1.3.0
-- Schema Version: 1.2.0 -> 1.3.0
-- Security: See docs/adr/0001-rls-role-separation.md for the full writeup of
--           what was wrong and why. Summary: genetics_api is the Postgres
--           bootstrap superuser (POSTGRES_USER) AND owns every table it
--           created via docker-entrypoint-initdb.d, so it silently bypassed
--           row_security on two independent grounds (rolsuper AND table
--           ownership without FORCE). The genetics_jobs_isolation /
--           genetics_files_isolation policies were syntactically correct
--           and semantically dead: verified empirically against the live
--           Rome deployment (2026-09-08) that genetics_api has
--           rolsuper=t, rolbypassrls=t.
-- ==============================================================================
-- Migration Path: Phase 4 of 4 (Database Schema)
-- Dependencies: 003_add_recovery_codes_and_resend.sql
-- Rollback: See rollback section at end of file
--
-- NOT IDEMPOTENT AGAINST AN EXISTING VOLUME WITHOUT PREREQUISITE SETUP.
-- Before applying this migration to an already-deployed instance:
--   1. Provision the runtime role's password as a Docker secret and deploy
--      the updated docker-compose.yml (adds the genetics_app_db_password
--      secret and mounts database/00-create-app-role.sh).
--   2. Run database/00-create-app-role.sh inside the running container
--      (it is idempotent - IF NOT EXISTS - and is also what fresh
--      deployments run automatically via docker-entrypoint-initdb.d):
--        docker exec postgres18-genetics bash /docker-entrypoint-initdb.d/00-create-app-role.sh
--   3. Only then apply this migration:
--        docker exec -i postgres18-genetics psql -U genetics_api -d genetics \
--          < database/migrations/004_separate_runtime_role_from_owner.sql
--   4. Redeploy api-gateway and worker so DATABASE_URL points at genetics_app.
-- This migration itself does not create genetics_app or set its password -
-- it only fails loudly (RAISE EXCEPTION) if step 2 was skipped, rather than
-- silently doing nothing.
-- ==============================================================================

SET search_path TO genetics, public;

-- ==============================================================================
-- PRECONDITION CHECK
-- ==============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'genetics_app') THEN
        RAISE EXCEPTION 'genetics_app role does not exist. Run database/00-create-app-role.sh inside the postgres container first (see this migration''s header).';
    END IF;
END
$$;

-- ==============================================================================
-- THE OWNER ROLE KEEPS ITS EXISTING NAME - IT IS NOT RENAMED
-- ==============================================================================
-- An earlier draft of this migration renamed the bootstrap role to
-- genetics_owner here, to match the name a fresh deployment gets directly
-- from initdb (see database/init.sql, .env.example). That rename is
-- IMPOSSIBLE on an existing deployment and was removed after verifying the
-- failure empirically (Postgres 18.6, migration run the intended way, as
-- `psql -U genetics_api`):
--
--   ERROR:  session user cannot be renamed
--
-- Postgres refuses to let a role rename itself, and this migration must
-- run as the bootstrap role - there is no other superuser to connect as;
-- initdb only ever creates the one role named by POSTGRES_USER. So on an
-- upgraded deployment the owner role permanently stays named genetics_api;
-- only a genuinely fresh deployment (POSTGRES_USER=genetics_owner from
-- .env.example, nothing to rename) gets the tidier name. This is a
-- cosmetic asymmetry, not a security-relevant one: ownership is tracked by
-- OID, not name, and the actual security boundary is that neither
-- api-gateway nor worker's DATABASE_URL ever references this role, under
-- either name (see docker-compose.yml / .env.example changes in the same
-- PR). Do NOT edit POSTGRES18_GENETICS_USER to genetics_owner after
-- applying this migration on an existing deployment - the role was never
-- renamed, and doing so breaks the postgres healthcheck and every
-- subsequent bootstrap connection.
--
-- Everything below targets CURRENT_USER instead of a hardcoded role name,
-- so this migration applies identically no matter what the owner role is
-- actually called.

-- ==============================================================================
-- DELIBERATE, NARROW RLS BYPASS FOR THE OWNER ROLE
-- ==============================================================================
-- The owner role is - and, empirically against Postgres 18.6, PERMANENTLY
-- must remain - the bootstrap superuser: Postgres refuses to let the
-- original bootstrap superuser ever drop SUPERUSER ("permission denied to
-- alter role ... The bootstrap superuser must have the SUPERUSER
-- attribute"). Superuser already implies bypassing row security
-- unconditionally, so the explicit BYPASSRLS below is redundant in
-- practice but kept for documentation clarity, and so this role's
-- privileges are stated explicitly rather than left implicit in "well,
-- it's a superuser". The one deliberate bypass surface this schema
-- actually relies on is functional, not attribute-based: the four
-- SECURITY DEFINER functions below (fixed SQL, no dynamic queries, minimal
-- columns returned). The real security boundary was never "the owner role
-- isn't superuser" - it's that neither api-gateway nor worker's
-- DATABASE_URL will ever reference the owner role after this migration
-- (see docker-compose.yml / .env.example changes in the same PR).
-- Acceptance criterion #2 (rolsuper=f, rolbypassrls=f) is checked against
-- the APPLICATION's connection, i.e. genetics_app, not against the owner.
ALTER ROLE CURRENT_USER WITH BYPASSRLS;

-- Preserve the search_path this role has carried as untracked manual
-- configuration (confirmed via `SELECT rolconfig FROM pg_roles` against the
-- live Rome deployment prior to this migration) so unqualified table names
-- keep resolving. Made explicit and applied to the new runtime role too,
-- below.
ALTER ROLE CURRENT_USER SET search_path TO genetics, public;
ALTER ROLE genetics_app SET search_path TO genetics, public;

-- ==============================================================================
-- FORCE ROW LEVEL SECURITY
-- ==============================================================================
-- Defense in depth. genetics_app is not the table owner, so RLS already
-- applies to it without FORCE; FORCE additionally protects against a future
-- migration accidentally reassigning table ownership to a role that isn't
-- meant to bypass isolation. The owner role remains exempt only because of
-- the explicit BYPASSRLS above, not because of ownership.

ALTER TABLE genetics_jobs FORCE ROW LEVEL SECURITY;
ALTER TABLE genetics_files FORCE ROW LEVEL SECURITY;

-- Rebind the existing policies from the owner role to the new runtime
-- role. This is the step that actually matters: the original policies
-- were created "TO genetics_api", which now bypasses RLS entirely via
-- BYPASSRLS above, making that binding inert. Without this rebind,
-- genetics_app (the role that actually needs the policy) would match no
-- policy at all on a FORCE-RLS table, which denies all access outright
-- rather than just failing to isolate.
ALTER POLICY genetics_jobs_isolation ON genetics_jobs TO genetics_app;
ALTER POLICY genetics_files_isolation ON genetics_files TO genetics_app;

-- ==============================================================================
-- CROSS-USER ACCESS FUNCTIONS (SECURITY DEFINER, pinned search_path)
-- ==============================================================================
-- Every function below is owned by the owner role (BYPASSRLS), so it runs
-- with RLS bypassed regardless of the caller's app.current_user_id - by
-- design. Each returns only the minimum columns the caller needs and takes
-- no caller-controlled SQL, only bound scalar parameters.

-- Resolve which user owns a job, given only its id. This is the one
-- deliberate cross-user *identity* lookup: GeneGnome has no login session,
-- so most api-gateway handlers only ever receive a job_id (itself a
-- capability - a UUIDv4 the caller can only have via the original job
-- response or the emailed download link), never the owner's email. The
-- api-gateway helper that wraps this (api-gateway/src/db.rs) calls it once
-- per request, at the top of a transaction, then sets app.current_user_id
-- to the result before running the real, RLS-restricted query - so a bug
-- that forgets a `WHERE id = $1` clause still can't leak another user's rows.
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
    'SECURITY DEFINER (bypasses RLS as the owner role). Returns only the owning user_id for a job_id, nothing else. Used to seed app.current_user_id before RLS-scoped queries in api-gateway.';

-- Same problem, different capability: the download/visualization endpoints
-- identify a job by its download_token (emailed to the user), not job_id.
-- Resolve id + owner from the token so the rest of that request can run
-- inside a normal RLS-scoped transaction.
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
    'SECURITY DEFINER (bypasses RLS as the owner role). Resolves id/user_id from a download_token, nothing else. Used to seed app.current_user_id before RLS-scoped download/visualization queries in api-gateway.';

-- Cross-user sweeps: worker's stuck-job recovery and 24h cleanup loops
-- (worker/src/main.rs) legitimately need to see jobs across every user to
-- decide what to act on. Each returned job is then acted on inside its own
-- RLS-scoped, per-owner transaction (unchanged) - these functions only
-- replace the initial cross-user discovery SELECT, not the mutations.
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
    'SECURITY DEFINER (bypasses RLS as the owner role). Cross-user discovery for worker stuck-job recovery; returns only id/user_id, no job content.';

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
    'SECURITY DEFINER (bypasses RLS as the owner role). Cross-user discovery for worker''s 24h job cleanup sweep; returns only id/user_id, no job content.';

-- ==============================================================================
-- GRANTS TO THE RUNTIME ROLE
-- ==============================================================================
-- Re-derived from the actual query inventory in api-gateway and worker
-- (grepped across both crates 2026-09-08), not copied from the old
-- genetics_api grant list. Notably narrower than 001-003's grants: no
-- SELECT on genetics_download_attempts (nothing in the app ever reads it
-- back) and no access to genetics_job_stats / genetics_security_events /
-- genetics_download_stats / genetics_failed_downloads (ops-only views,
-- queried manually via the owner role, never by application code).

GRANT USAGE ON SCHEMA genetics TO genetics_app;

GRANT SELECT, INSERT, UPDATE, DELETE ON genetics_jobs TO genetics_app;
GRANT SELECT, INSERT ON genetics_files TO genetics_app;
GRANT SELECT, INSERT, UPDATE ON genetics_recovery_codes TO genetics_app;
GRANT INSERT ON genetics_download_attempts TO genetics_app;
GRANT INSERT ON genetics_audit TO genetics_app;

GRANT USAGE ON ALL SEQUENCES IN SCHEMA genetics TO genetics_app;

GRANT EXECUTE ON FUNCTION genetics.resolve_job_owner(UUID) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.resolve_job_owner_by_token(TEXT) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.list_stuck_jobs(TIMESTAMPTZ) TO genetics_app;
GRANT EXECUTE ON FUNCTION genetics.list_jobs_for_cleanup(TIMESTAMPTZ) TO genetics_app;

-- ==============================================================================
-- SCHEMA VERSION UPDATE
-- ==============================================================================

INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'schema_migration',
    'success',
    jsonb_build_object(
        'migration', '004_separate_runtime_role_from_owner',
        'old_version', '1.2.0',
        'new_version', '1.3.0',
        'description', 'Split genetics_api into an owner role (unchanged name - Postgres cannot rename a role to itself as session user, and remains the permanently-superuser bootstrap role since Postgres does not allow stripping SUPERUSER from it either) and genetics_app (non-superuser, non-bypassrls, non-owning runtime role that api-gateway/worker actually connect as). Added FORCE ROW LEVEL SECURITY on genetics_jobs and genetics_files.',
        'tables_modified', jsonb_build_array('genetics_jobs', 'genetics_files'),
        'functions_created', jsonb_build_array('resolve_job_owner', 'resolve_job_owner_by_token', 'list_stuck_jobs', 'list_jobs_for_cleanup'),
        'roles_modified', jsonb_build_array('genetics_api (kept its name; BYPASSRLS + FORCE RLS make it explicit rather than relying on ownership - see comments above)', 'genetics_app (new runtime role, non-superuser, non-bypassrls)'),
        'timestamp', NOW()
    ),
    'critical'
);

-- ==============================================================================
-- ROLLBACK (for reference only - do not execute without understanding the
-- consequences: this restores the pre-fix, RLS-dead-in-practice state)
-- ==============================================================================

/*
SET search_path TO genetics, public;

-- The owner role was never renamed and was never actually stripped of
-- SUPERUSER (Postgres disallows both for the bootstrap role) - nothing to
-- restore on either front. Substitute the owner role's actual name below
-- (genetics_api on an upgraded deployment, genetics_owner on a fresh one).

ALTER POLICY genetics_jobs_isolation ON genetics_jobs TO genetics_api;
ALTER POLICY genetics_files_isolation ON genetics_files TO genetics_api;

DROP FUNCTION IF EXISTS genetics.list_jobs_for_cleanup(TIMESTAMPTZ);
DROP FUNCTION IF EXISTS genetics.list_stuck_jobs(TIMESTAMPTZ);
DROP FUNCTION IF EXISTS genetics.resolve_job_owner_by_token(TEXT);
DROP FUNCTION IF EXISTS genetics.resolve_job_owner(UUID);

ALTER TABLE genetics_files NO FORCE ROW LEVEL SECURITY;
ALTER TABLE genetics_jobs NO FORCE ROW LEVEL SECURITY;

REVOKE ALL ON ALL SEQUENCES IN SCHEMA genetics FROM genetics_app;
REVOKE ALL ON genetics_audit FROM genetics_app;
REVOKE ALL ON genetics_download_attempts FROM genetics_app;
REVOKE ALL ON genetics_recovery_codes FROM genetics_app;
REVOKE ALL ON genetics_files FROM genetics_app;
REVOKE ALL ON genetics_jobs FROM genetics_app;
REVOKE USAGE ON SCHEMA genetics FROM genetics_app;

-- No rename to undo - the owner role's name was never changed.

-- genetics_app is intentionally left in place; DROP ROLE genetics_app
-- manually once you've confirmed nothing still authenticates as it.

INSERT INTO genetics_audit (event_type, user_id, action, result, details, severity)
VALUES (
    'configuration_changed',
    'system',
    'schema_rollback',
    'success',
    jsonb_build_object(
        'migration', '004_separate_runtime_role_from_owner',
        'from_version', '1.3.0',
        'to_version', '1.2.0',
        'timestamp', NOW()
    ),
    'warning'
);
*/
