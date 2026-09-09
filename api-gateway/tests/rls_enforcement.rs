// ==============================================================================
// rls_enforcement.rs - RLS Enforcement Integration Test
// ==============================================================================
// Description: Proves genetics_jobs row-level security actually enforces
//              when queried as the application's own runtime role
//              (genetics_app), not as the schema owner. A test that only
//              exercises the policy SQL as the owner proves nothing - see
//              docs/adr/0001-rls-role-separation.md for why.
//
//              Requires a live Postgres reachable via TEST_DATABASE_URL,
//              authenticated as genetics_app (NOT genetics_owner/
//              genetics_api - that would defeat the point of this test).
//              Skipped with a clear message if the env var isn't set.
//
//              Run against the dev stack:
//                TEST_DATABASE_URL="postgresql://genetics_app:<password>@localhost:<port>/genetics" \
//                  cargo test --test rls_enforcement -- --test-threads=1
//
//              --test-threads=1 matters: tests share one genetics_jobs
//              table and rely on the app.current_user_id GUC being
//              transaction-local (SET LOCAL / set_config(..., true)) -
//              concurrent tests on separate connections don't interfere
//              with each other, but each test commits real rows, so
//              serializing keeps failures easy to read.
// Author: Matt Barham
// Created: 2026-09-08
// Modified: 2026-09-08
// Version: 1.0.0
// ==============================================================================

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Returns None (with a printed skip reason) if TEST_DATABASE_URL isn't set,
/// so this test suite doesn't fail CI environments that don't have a
/// database wired up - but it never silently passes on a real failure once
/// it does connect.
async fn connect() -> Option<PgPool> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIPPED: TEST_DATABASE_URL not set - see api-gateway/tests/rls_enforcement.rs header for how to run this test");
        return None;
    };
    Some(
        PgPool::connect(&url)
            .await
            .expect("Failed to connect to TEST_DATABASE_URL"),
    )
}

async fn insert_job_as(pool: &PgPool, user_id: &str) -> Uuid {
    let job_id = Uuid::new_v4();
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("set RLS context");
    sqlx::query(
        "INSERT INTO genetics_jobs (id, user_id, status, created_at) VALUES ($1, $2, 'pending', NOW())",
    )
    .bind(job_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("insert job as its own owner must succeed");
    tx.commit().await.expect("commit");
    job_id
}

/// Acceptance criterion #2: the application's own connection is not a
/// superuser and does not bypass row security. If this fails, every other
/// assertion in this file is meaningless - the connection would see
/// everything regardless of policy.
#[tokio::test]
async fn app_connection_is_not_superuser_and_does_not_bypass_rls() {
    let Some(pool) = connect().await else { return };

    let row = sqlx::query("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = current_user")
        .fetch_one(&pool)
        .await
        .expect("query pg_roles for current_user");

    let rolsuper: bool = row.get("rolsuper");
    let rolbypassrls: bool = row.get("rolbypassrls");

    assert!(!rolsuper, "genetics_app must not be a superuser (found rolsuper=true - is TEST_DATABASE_URL pointed at genetics_owner instead?)");
    assert!(!rolbypassrls, "genetics_app must not have BYPASSRLS (found rolbypassrls=true)");
}

/// Acceptance criterion #3: FORCE ROW LEVEL SECURITY is actually set.
#[tokio::test]
async fn genetics_jobs_has_force_row_level_security() {
    let Some(pool) = connect().await else { return };

    let row = sqlx::query(
        "SELECT relforcerowsecurity FROM pg_class WHERE oid = 'genetics.genetics_jobs'::regclass",
    )
    .fetch_one(&pool)
    .await
    .expect("query pg_class for genetics_jobs");

    let forced: bool = row.get("relforcerowsecurity");
    assert!(forced, "genetics_jobs must have FORCE ROW LEVEL SECURITY set");
}

/// Acceptance criterion #1 (core enforcement test): insert jobs for two
/// distinct user_id values, then assert a SELECT scoped to user A's context
/// returns only A's rows, and a SELECT with no context set returns zero
/// rows - not an error, not everything, zero.
#[tokio::test]
async fn rls_isolates_rows_by_current_user_id_and_denies_by_default() {
    let Some(pool) = connect().await else { return };

    let user_a = format!("rls-test-a-{}@example.invalid", Uuid::new_v4());
    let user_b = format!("rls-test-b-{}@example.invalid", Uuid::new_v4());

    let job_a = insert_job_as(&pool, &user_a).await;
    let job_b = insert_job_as(&pool, &user_b).await;

    // Scoped to A: sees only A's job, never B's.
    {
        let mut tx = pool.begin().await.expect("begin");
        sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
            .bind(&user_a)
            .execute(&mut *tx)
            .await
            .expect("set RLS context to A");

        let rows: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM genetics_jobs WHERE id IN ($1, $2)")
            .bind(job_a)
            .bind(job_b)
            .fetch_all(&mut *tx)
            .await
            .expect("select as A");

        assert_eq!(rows, vec![job_a], "user A's context must see exactly A's job, not B's");
        tx.commit().await.expect("commit");
    }

    // No context set at all: sees nothing, for either job. This is the
    // exact failure mode that made the original policies dead in practice
    // - current_setting(..., TRUE) is NULL, and NULL never equals a
    // user_id, so USING matches no rows.
    {
        let rows: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM genetics_jobs WHERE id IN ($1, $2)")
            .bind(job_a)
            .bind(job_b)
            .fetch_all(&pool)
            .await
            .expect("select with no RLS context set");

        assert!(rows.is_empty(), "a connection with no app.current_user_id set must see zero rows, saw {:?}", rows);
    }

    // Cleanup: each delete needs its own owner's context, same as the app does.
    for (job_id, owner) in [(job_a, &user_a), (job_b, &user_b)] {
        let mut tx = pool.begin().await.expect("begin");
        sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
            .bind(owner)
            .execute(&mut *tx)
            .await
            .expect("set RLS context for cleanup");
        sqlx::query("DELETE FROM genetics_jobs WHERE id = $1")
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .expect("cleanup delete");
        tx.commit().await.expect("commit");
    }
}

/// A WITH CHECK companion to the USING test above: inserting a row under
/// one user's context with another user's user_id must be rejected, not
/// silently reassigned or silently allowed.
#[tokio::test]
async fn rls_rejects_insert_with_mismatched_user_id() {
    let Some(pool) = connect().await else { return };

    let user_a = format!("rls-test-mismatch-a-{}@example.invalid", Uuid::new_v4());
    let user_b = format!("rls-test-mismatch-b-{}@example.invalid", Uuid::new_v4());

    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(&user_a)
        .execute(&mut *tx)
        .await
        .expect("set RLS context to A");

    let result = sqlx::query(
        "INSERT INTO genetics_jobs (id, user_id, status, created_at) VALUES ($1, $2, 'pending', NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(&user_b) // mismatched: context is A, row claims to be B's
    .execute(&mut *tx)
    .await;

    assert!(
        result.is_err(),
        "inserting a row for user B while scoped to user A's context must be rejected by WITH CHECK"
    );

    let _ = tx.rollback().await;
}
