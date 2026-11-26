// ==============================================================================
// schema_contract.rs - Fresh Install Schema Integration Test
// ==============================================================================
// Description: Walks one job through the SQL the services run, as the runtime
//              role (genetics_app), against a database initialized from
//              database/init.sql alone. Fails when init.sql lacks a table,
//              column, function or grant that exists only in a migration.
//
//              Requires TEST_DATABASE_URL, like rls_enforcement.rs. Run both
//              with scripts/ci_rls_test.sh.
// Author: Matt Barham
// Created: 2026-11-25
// Modified: 2026-11-25
// Version: 1.0.0
// ==============================================================================

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Returns None (with a printed skip reason) if TEST_DATABASE_URL isn't set.
async fn connect() -> Option<PgPool> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIPPED: TEST_DATABASE_URL not set");
        return None;
    };
    Some(
        PgPool::connect(&url)
            .await
            .expect("Failed to connect to TEST_DATABASE_URL"),
    )
}

/// Transaction scoped to one job owner, as the services open it.
async fn scoped<'a>(pool: &PgPool, user_id: &str) -> Transaction<'a, Postgres> {
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .expect("set RLS context");
    tx
}

#[tokio::test]
async fn runtime_role_can_run_a_whole_job_lifecycle() {
    let Some(pool) = connect().await else { return };
    let owner = format!("schema-{}@example.com", Uuid::new_v4());
    let job_id = Uuid::new_v4();
    let token = Uuid::new_v4().to_string();

    // Submission: job row plus recovery codes
    let mut tx = scoped(&pool, &owner).await;
    sqlx::query(
        "INSERT INTO genetics_jobs (id, user_id, status, created_at, metadata)
         VALUES ($1, $2, 'pending', NOW(), '{}'::jsonb)",
    )
    .bind(job_id)
    .bind(&owner)
    .execute(&mut *tx)
    .await
    .expect("insert job");
    sqlx::query("INSERT INTO genetics.genetics_recovery_codes (job_id, code_hash) VALUES ($1, $2)")
        .bind(job_id)
        .bind("hash")
        .execute(&mut *tx)
        .await
        .expect("insert recovery code");

    // Completion: output file record and download credentials
    sqlx::query(
        "INSERT INTO genetics_files
             (job_id, user_id, file_name, file_type, file_size, hash_sha256, uploaded_at, metadata)
         VALUES ($1, $2, 'out.vcf.gz', 'vcf', 1, 'hash', NOW(), '{}'::jsonb)",
    )
    .bind(job_id)
    .bind(&owner)
    .execute(&mut *tx)
    .await
    .expect("insert file");
    sqlx::query(
        "UPDATE genetics.genetics_jobs
         SET status = 'completed', completed_at = NOW(), user_email = $1,
             download_token = $2, download_password_hash = 'hash', emailed_at = NOW()
         WHERE id = $3",
    )
    .bind(&owner)
    .bind(&token)
    .bind(job_id)
    .execute(&mut *tx)
    .await
    .expect("complete job");
    tx.commit().await.expect("commit");

    // Download: token lookup, job read, attempt counter and attempt log
    let (resolved_id, resolved_owner): (Uuid, String) =
        sqlx::query_as("SELECT id, user_id FROM genetics.resolve_job_owner_by_token($1)")
            .bind(&token)
            .fetch_one(&pool)
            .await
            .expect("resolve owner by token");
    assert_eq!((resolved_id, resolved_owner), (job_id, owner.clone()));

    let mut tx = scoped(&pool, &owner).await;
    sqlx::query(
        "SELECT id, user_id, status, result_path, metadata, expires_at,
                download_password_hash, download_attempts,
                max_download_attempts, last_download_attempt, last_email_resent_at
         FROM genetics_jobs WHERE download_token = $1",
    )
    .bind(&token)
    .fetch_one(&mut *tx)
    .await
    .expect("read job for download");
    sqlx::query(
        "UPDATE genetics.genetics_jobs
         SET download_attempts = download_attempts + 1, last_download_attempt = NOW(),
             last_email_resent_at = NOW()
         WHERE id = $1",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await
    .expect("count download attempt");
    sqlx::query(
        "INSERT INTO genetics.genetics_download_attempts
             (job_id, ip_address, user_agent, token_provided, token_valid,
              password_provided, password_valid, attempt_result)
         VALUES ($1, '192.0.2.1', 'test', true, true, true, true, 'success')",
    )
    .bind(job_id)
    .execute(&mut *tx)
    .await
    .expect("log download attempt");

    // Self-service deletion, then the cleanup sweep removes the row
    let code_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM genetics.genetics_recovery_codes WHERE job_id = $1 AND used = false",
    )
    .bind(job_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read recovery code");
    sqlx::query(
        "UPDATE genetics.genetics_recovery_codes SET used = true, used_at = NOW() WHERE id = $1",
    )
    .bind(code_id)
    .execute(&mut *tx)
    .await
    .expect("consume recovery code");
    sqlx::query("UPDATE genetics.genetics_jobs SET status = 'user_deleted' WHERE id = $1")
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .expect("mark job deleted");
    // Cascades to files, recovery codes and download attempts
    let deleted = sqlx::query("DELETE FROM genetics.genetics_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .expect("delete job");
    assert_eq!(deleted.rows_affected(), 1);
    tx.commit().await.expect("commit");
}
