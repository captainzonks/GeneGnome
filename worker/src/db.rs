// ==============================================================================
// db.rs - Row-Level-Security-Scoped Database Access
// ==============================================================================
// Description: Helper for opening a transaction with app.current_user_id
//              set via a bound parameter, replacing the format!()-built
//              `SET LOCAL app.current_user_id = '...'` pattern used
//              throughout this crate. Worker always already knows which
//              user it's acting on behalf of (from the job payload, or from
//              genetics.list_stuck_jobs / genetics.list_jobs_for_cleanup),
//              so unlike api-gateway/src/db.rs there's no job_id-only
//              lookup case to handle here.
// Author: Matt Barham
// Created: 2026-09-08
// Modified: 2026-09-08
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use sqlx::{PgPool, Postgres, Transaction};

/// Open a transaction scoped to `user_id`.
pub async fn scoped_tx_for_user(
    pool: &PgPool,
    user_id: &str,
) -> Result<Transaction<'static, Postgres>> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start RLS-scoped transaction")?;

    // set_config(..., true) is the parameterized equivalent of SET LOCAL -
    // unlike SET, it accepts a bind parameter, so there's no string
    // interpolation / manual quote-escaping involved.
    sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .context("Failed to set RLS context")?;

    Ok(tx)
}
