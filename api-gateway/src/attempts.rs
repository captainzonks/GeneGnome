// ==============================================================================
// attempts.rs - Failed Credential Attempt Limiter
// ==============================================================================
// Description: Caps password and recovery-code guesses per job, counted in Redis
// Author: Matt Barham
// Created: 2026-11-26
// Modified: 2026-11-26
// Version: 1.0.0
// ==============================================================================

use redis::{Client, Commands, RedisResult};
use uuid::Uuid;

/// Wrong guesses allowed per job and scope within one window
pub const MAX_ATTEMPTS: i64 = 10;

/// Length of a window, starting at its first attempt
const WINDOW_SECONDS: i64 = 3600;

/// What is being guessed. Each scope counts separately, so that someone holding
/// only a job ID cannot lock the owner out of the emailed download link.
#[derive(Debug, Clone, Copy)]
pub enum Scope {
    /// Token and password, from the emailed link
    Download,
    /// Job ID and password
    JobPassword,
    /// Job ID and recovery code
    RecoveryCode,
}

fn key(scope: Scope, job_id: Uuid) -> String {
    format!("genetics:attempts:{scope:?}:{job_id}")
}

/// Count an attempt before the credential is checked, so that parallel requests
/// cannot all slip under the cap. Returns false once the cap is exceeded.
pub fn register(client: &Client, scope: Scope, job_id: Uuid) -> RedisResult<bool> {
    let mut conn = client.get_connection()?;
    let key = key(scope, job_id);
    let (count,): (i64,) = redis::pipe()
        .atomic()
        .incr(&key, 1)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(WINDOW_SECONDS)
        .arg("NX")
        .ignore()
        .query(&mut conn)?;
    Ok(count <= MAX_ATTEMPTS)
}

/// Forget a job's attempts, after a correct credential or a new password
pub fn clear(client: &Client, scope: Scope, job_id: Uuid) -> RedisResult<()> {
    client.get_connection()?.del(key(scope, job_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs TEST_REDIS_URL; prints a skip reason without it.
    #[test]
    fn attempts_are_capped_per_job_and_scope_until_cleared() {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("SKIPPED: TEST_REDIS_URL not set");
            return;
        };
        let client = Client::open(url).expect("Redis URL");
        let job = Uuid::new_v4();

        for attempt in 1..=MAX_ATTEMPTS {
            assert!(
                register(&client, Scope::Download, job).unwrap(),
                "{attempt}"
            );
        }
        assert!(!register(&client, Scope::Download, job).unwrap());

        // Other scopes and other jobs are unaffected
        assert!(register(&client, Scope::JobPassword, job).unwrap());
        assert!(register(&client, Scope::Download, Uuid::new_v4()).unwrap());

        // The window is set to expire
        let ttl: i64 = client
            .get_connection()
            .unwrap()
            .ttl(key(Scope::Download, job))
            .unwrap();
        assert!(ttl > 0 && ttl <= WINDOW_SECONDS);

        clear(&client, Scope::Download, job).unwrap();
        assert!(register(&client, Scope::Download, job).unwrap());
    }
}
