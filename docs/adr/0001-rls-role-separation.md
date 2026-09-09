<!-- =============================================================================
0001-rls-role-separation.md -- ADR: separate the runtime role from the schema
                                owner so row-level security actually enforces

Description: Architecture decision record for database/migrations/
             004_separate_runtime_role_from_owner.sql. Documents what was
             wrong with the original single-role model, why the RLS
             policies never evaluated despite being syntactically correct,
             what changed, and what it costs.
Author:      Matt Barham
Created:     2026-09-08
Modified:    2026-09-08
Version:     1.0.0

Document Type: Architecture Decision Record
Status:        Accepted
============================================================================= -->

# ADR 0001: Separate the runtime role from the schema owner so RLS enforces

## Status

Accepted. Implemented in `database/migrations/004_separate_runtime_role_from_owner.sql`
(fresh deployments get the same end state directly from `database/init.sql` and
`database/00-create-app-role.sh`).

## Context

`genetics_jobs` and `genetics_files` have had row-level security policies
(`genetics_jobs_isolation`, `genetics_files_isolation`) since the original
schema. Those policies are syntactically correct: they compare
`user_id = current_setting('app.current_user_id', TRUE)`. They were also
completely inert, for two independent reasons, both rooted in a single
design choice: `docker-compose.yml` set `POSTGRES_USER=genetics_api`, and
that same role — `genetics_api` — was used for everything: it is the
Postgres bootstrap superuser, it owns every table `docker-entrypoint-initdb.d`
creates, and it is what both `api-gateway` and `worker` authenticate as in
their `DATABASE_URL`.

**1. Superusers unconditionally bypass row security.** Per the PostgreSQL
`CREATE POLICY` documentation: "Superusers and roles with the `BYPASSRLS`
attribute always bypass the row security system when accessing a table."
This cannot be overridden by any table-level setting. Verified empirically
against the live Rome deployment (2026-09-08):

```sql
SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolname = 'genetics_api';
--  rolname    | rolsuper | rolbypassrls
-- genetics_api | t        | t
```

**2. Table owners bypass row security unless `FORCE ROW LEVEL SECURITY` is
set.** Neither `database/init.sql` nor any prior migration set it. `genetics_api`
owned every table it created via `docker-entrypoint-initdb.d`, so even if (1)
were somehow not true, ownership alone would have made the policies inert.

Either defect alone was sufficient to defeat the isolation the policies were
written to provide; the deployment had both. The `GRANT ... TO genetics_api`
statements throughout `database/init.sql` and migrations 001–003 were
consequently no-ops — a role cannot meaningfully grant privileges to itself
as the object owner. There was also no application-layer RLS context at
all on the `api-gateway` side: `SET LOCAL app.current_user_id` appeared only
in `worker/src/main.rs`, never in `api-gateway/src/handlers.rs`.

**The actual security boundary, before this change, was never the
database.** It was the API key required to reach the service at all, plus
application-layer ownership checks in handler code (e.g. comparing a
supplied email to a job's `user_id` column before returning it). The RLS
policies were dead code that happened to be syntactically valid SQL.

## Decision

Split the single role into two:

- **Owner role** — runs `docker-entrypoint-initdb.d`, owns every object,
  remains the Postgres bootstrap superuser. On a fresh deployment this role
  is named `genetics_owner` directly (`POSTGRES_USER=genetics_owner` in
  `.env.example`). **On an existing deployment it keeps the name
  `genetics_api` permanently** — see "Rename is impossible" below. Neither
  name is ever used in `api-gateway` or `worker`'s `DATABASE_URL`.

- **Runtime role (`genetics_app`)** — `LOGIN`, not superuser, not
  `BYPASSRLS`, owns nothing. Both services' `DATABASE_URL` point at this
  role. Grants are re-derived from the actual query inventory in
  `api-gateway` and `worker` (grepped across both crates), not copied
  wholesale from the old `genetics_api` grant list — notably narrower: no
  `SELECT` on `genetics_download_attempts`, no access to the ops-only
  views (`genetics_job_stats`, `genetics_security_events`, etc.).

`FORCE ROW LEVEL SECURITY` is set on both tables. With a non-owner runtime
role this is defense in depth rather than strictly required, but it means
the policies still hold if table ownership ever shifts in a future
migration.

### Rename is impossible on an existing deployment

An earlier draft of the migration renamed `genetics_api` to `genetics_owner`
so both paths converged on one name. That fails, verified empirically
against Postgres 18.6 by running the migration the way its own runbook
specifies (`psql -U genetics_api`):

```
ERROR:  session user cannot be renamed
```

Postgres will not let a role rename itself, and there is no other superuser
to connect as — `initdb` only ever creates the one role named by
`POSTGRES_USER`. The migration therefore never attempts the rename; every
statement that needs to affect the owner role's attributes targets
`CURRENT_USER` instead of a hardcoded name. The practical effect: an
upgraded deployment's owner role is named `genetics_api` forever, a fresh
deployment's is named `genetics_owner` forever, and this is cosmetic, not
security-relevant — ownership is tracked by OID, not name, and the actual
boundary is that `DATABASE_URL` never references either name.

### Establishing the RLS context on the API path

Before this change, `api-gateway` queried RLS-protected tables without ever
setting `app.current_user_id`. Fixing the role model alone would have
broken every one of those code paths outright (`USING` matches no rows
against a NULL setting, `WITH CHECK` rejects every insert), so the role
change and this had to land together.

`api-gateway/src/db.rs` adds three helpers (`scoped_tx_for_job`,
`scoped_tx_for_token`, `scoped_tx_for_user`) that open a transaction,
establish `app.current_user_id`, and hand the transaction back — making it
structurally hard for a handler to query these tables without first going
through one of them. GeneGnome has no login session, so most handlers only
ever receive a `job_id` or a `download_token` (both effectively bearer
capabilities), not the owner's identity directly; `scoped_tx_for_job` and
`scoped_tx_for_token` resolve the owner first via a `SECURITY DEFINER`
function, then scope the transaction to it — so a handler bug that forgets
a `WHERE id = $1` clause still cannot leak another user's row, because the
connection was never in a context where it could see one.

Both `api-gateway` and the three existing call sites in
`worker/src/main.rs` now set the context with:

```sql
SELECT set_config('app.current_user_id', $1, true)
```

`set_config(..., true)` is the parameterized equivalent of `SET LOCAL` —
unlike `SET`, it accepts a bind parameter. The worker's previous
`format!`-built `SET LOCAL` statements with manual `'` doubling were not
found to be exploitable, but the bind parameter removes the class of bug
entirely rather than relying on the escaping being correct forever.

### Legitimate cross-user access

Two classes of code legitimately need to see rows across users, and both
are now explicit, narrow, `SECURITY DEFINER` functions with a pinned
`search_path` (fixed SQL, no dynamic queries, minimal columns returned) —
owned by the owner role, so they run with RLS bypassed by design, not by
accident:

- **Identity resolution** (`resolve_job_owner`, `resolve_job_owner_by_token`) —
  the one deliberate cross-user *lookup*, used by `api-gateway/src/db.rs` to
  seed `app.current_user_id` before the real, RLS-scoped query runs.
- **Cross-user discovery sweeps** (`list_stuck_jobs`, `list_jobs_for_cleanup`) —
  used by `worker`'s stuck-job recovery and 24h cleanup loops to decide
  *which* jobs to act on. Each discovered job is still acted on inside its
  own RLS-scoped, per-owner transaction; these functions only replace the
  initial cross-user `SELECT`, not the mutations.

## Consequences

**What this costs:**

- An existing deployment must be upgraded in a specific order (provision
  the `genetics_app` password secret → run `database/00-create-app-role.sh` →
  apply the migration → redeploy `api-gateway`/`worker` pointed at
  `genetics_app`). Doing this out of order either leaves the app connecting
  as a role that still bypasses RLS, or breaks it against a role that
  doesn't exist yet.
- The owner role's name is permanently inconsistent across deployment ages
  (`genetics_api` vs `genetics_owner`) — cosmetic, documented above, but a
  future contributor reading only one deployment's state could be misled
  without this ADR.
- Four additional `SECURITY DEFINER` functions are now part of the schema's
  trusted computing base. Each is deliberately minimal (fixed SQL, no
  dynamic query construction, no caller-controlled column selection) to
  keep that surface small and auditable.

**What this buys:**

- Row-level security is now the actual enforcement boundary for
  `genetics_jobs` and `genetics_files`, not the API key and
  application-layer ownership checks alone. See
  `api-gateway/tests/rls_enforcement.rs` for the integration test proving
  this against a live `genetics_app` connection — inserting rows for two
  distinct users and asserting a scoped `SELECT` sees only its own user's
  row, a context-less `SELECT` sees zero rows (not an error, not
  everything), and a mismatched `WITH CHECK` insert is rejected.
- A future handler bug that forgets a `WHERE user_id = $1` clause fails
  closed (denied by RLS) instead of failing open (returns every user's
  data), because the connection making the query is never a role that can
  see across users in the first place.

## Verified acceptance criteria (2026-09-08)

Run live against both a fresh deployment (`database/init.sql` +
`database/00-create-app-role.sh` only) and a reconstructed pre-fix upgrade
path (old `genetics_api`-owns-everything schema → `00-create-app-role.sh` →
this migration), not just reviewed as SQL:

1. `api-gateway/tests/rls_enforcement.rs` passes (4/4) against a real
   `genetics_app` connection on both paths.
2. `SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user;`
   returns `f, f` through `genetics_app`'s own connection.
3. `relforcerowsecurity` is `t` for both `genetics_jobs` and `genetics_files`.
4. `pg_policy.polroles` for both isolation policies resolves to
   `{genetics_app}` after the migration, on the upgrade path where the
   original policies were created `TO genetics_api`.
5. `cargo clippy` on the touched code (`api-gateway/src/db.rs`,
   `api-gateway/src/error.rs`, the new test file, and the modified
   `handlers.rs`/`main.rs`/`job_processor.rs`/`worker/src/main.rs`/
   `worker/src/db.rs`) introduces zero new warnings versus the pre-change
   baseline (40 warnings in `api-gateway`, 30 in `worker`, unchanged by this
   PR — both crates carry pre-existing, unrelated dead-code/style warnings
   that were out of scope for this change).

## Also noted, not fixed here

`api-gateway` and `worker` have roughly 92 non-test `unwrap()`/`expect()`
calls between them. Out of scope for this change; flagged separately for
the ones on request-handling paths.
