// ==============================================================================
// db.rs - Row-Level-Security-Scoped Database Access
// ==============================================================================
// Description: Helpers that make it structurally hard to query
//              genetics_jobs / genetics_files without first establishing
//              the RLS context (app.current_user_id). GeneGnome has no
//              login session - most handlers only ever receive a job_id, so
//              the owning user has to be resolved from the row itself
//              before the real, RLS-restricted query can run. See
//              docs/adr/0001-rls-role-separation.md.
// Author: Matt Barham
// Created: 2026-09-08
// Modified: 2026-09-08
// Version: 1.0.0
// ==============================================================================

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::AppError;

/// Open a transaction scoped to a job's owner.
///
/// Resolves the owning `user_id` via the `genetics.resolve_job_owner`
/// SECURITY DEFINER function (the one deliberate RLS bypass in this
/// schema - see database/migrations/004_separate_runtime_role_from_owner.sql)
/// and sets `app.current_user_id` for the lifetime of the returned
/// transaction before handing it back.
///
/// Returns `AppError::NotFound` if the job doesn't exist, without the
/// (still context-less) connection ever having read a row from
/// genetics_jobs directly.
pub async fn scoped_tx_for_job(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<(Transaction<'static, Postgres>, String), AppError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to start transaction: {}", e)))?;

    let owner: Option<String> = sqlx::query_scalar("SELECT genetics.resolve_job_owner($1)")
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to resolve job owner: {}", e)))?;

    let owner = owner.ok_or(AppError::NotFound)?;

    set_rls_context(&mut tx, &owner).await?;

    Ok((tx, owner))
}

/// Open a transaction scoped to the owner of the job identified by a
/// download token (the download/visualization endpoints identify a job by
/// its emailed download_token, not job_id). Same shape as
/// `scoped_tx_for_job`, resolving through `genetics.resolve_job_owner_by_token`
/// instead.
pub async fn scoped_tx_for_token(
    pool: &PgPool,
    token: &str,
) -> Result<(Transaction<'static, Postgres>, Uuid, String), AppError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to start transaction: {}", e)))?;

    let resolved: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, user_id FROM genetics.resolve_job_owner_by_token($1)",
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to resolve job owner: {}", e)))?;

    let (job_id, owner) = resolved.ok_or(AppError::NotFound)?;

    set_rls_context(&mut tx, &owner).await?;

    Ok((tx, job_id, owner))
}

/// Open a transaction scoped directly to `user_id` - for inserting a brand
/// new row (job submission) where there is no existing row to resolve an
/// owner from. The caller already knows the owner because the requester
/// supplied it themselves (the email address they're submitting the job
/// under).
pub async fn scoped_tx_for_user(
    pool: &PgPool,
    user_id: &str,
) -> Result<Transaction<'static, Postgres>, AppError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to start transaction: {}", e)))?;

    set_rls_context(&mut tx, user_id).await?;

    Ok(tx)
}

async fn set_rls_context(
    tx: &mut Transaction<'static, Postgres>,
    user_id: &str,
) -> Result<(), AppError> {
    // set_config(..., true) is the parameterized equivalent of SET LOCAL -
    // unlike SET, it accepts a bind parameter, so there's no string
    // interpolation / manual quote-escaping involved.
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to set RLS context: {}", e)))?;

    Ok(())
}
