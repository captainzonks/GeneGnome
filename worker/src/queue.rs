// ==============================================================================
// queue.rs - Redis Job Queue Management (Worker Side)
// ==============================================================================
// Description: Job queue operations for consuming jobs from Redis
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2026-11-26
// Version: 1.0.1
// ==============================================================================

use anyhow::{Context, Result};
use redis::AsyncCommands;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

const QUEUE_KEY: &str = "genetics:job_queue";

/// Output format selection (must match API gateway)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    // Json, // DISABLED: 29GB JSON file causes OOM. Users can convert from SQLite/Parquet/VCF
    Parquet,
    Sqlite,
    Vcf,
}

/// Quality threshold for imputation filtering
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum QualityThreshold {
    None, // No filtering - include all variants
    R080, // R² ≥ 0.8
    #[default]
    R090, // R² ≥ 0.9 (default, matches R script)
}

/// Job payload from Redis queue (must match API gateway)
#[derive(Debug, Serialize, Deserialize)]
pub struct JobPayload {
    pub job_id: Uuid,
    pub user_id: String,
    pub user_email: Option<String>, // Phase 4: Email for download notifications
    pub upload_dir: String,
    pub output_dir: String,
    pub output_formats: Vec<OutputFormat>,
    #[serde(default)]
    pub quality_threshold: QualityThreshold,
    /// Phase 7.1: Indicates if files are chunked and need reassembly by worker
    #[serde(default)]
    pub chunked_upload: bool,
    /// Phase 7.1: Upload session ID for chunk reassembly (if chunked_upload=true)
    pub upload_session_id: Option<String>,
    /// VCF format preference: "merged" or "per_chromosome" (defaults to "merged")
    #[serde(default = "default_vcf_format")]
    pub vcf_format: String,
}

fn default_vcf_format() -> String {
    "merged".to_string()
}

/// How long `BRPOP` blocks on an empty queue
const DEQUEUE_BLOCK: Duration = Duration::from_secs(1);

/// Client-side reply timeout. It must exceed `DEQUEUE_BLOCK`: a shorter one (the
/// library default is 500 ms) fails every idle poll and can drop a job that
/// Redis pops after the client has given up.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Open the worker's auto-reconnecting Redis connection
pub async fn connect(client: redis::Client) -> Result<ConnectionManager> {
    let config = ConnectionManagerConfig::new().set_response_timeout(Some(RESPONSE_TIMEOUT));

    ConnectionManager::new_with_config(client, config)
        .await
        .context("Failed to create Redis connection manager")
}

/// Job queue manager
pub struct JobQueue {
    conn: ConnectionManager,
}

impl JobQueue {
    /// Create new job queue manager
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// Dequeue a job (blocking pop with timeout)
    pub async fn dequeue(&mut self) -> Result<Option<JobPayload>> {
        let result: Option<(String, String)> = self
            .conn
            .brpop(QUEUE_KEY, DEQUEUE_BLOCK.as_secs_f64())
            .await
            .context("Failed to pop from queue")?;

        match result {
            Some((_, payload_json)) => {
                let payload: JobPayload = serde_json::from_str(&payload_json)
                    .context("Failed to deserialize job payload")?;
                Ok(Some(payload))
            }
            None => Ok(None),
        }
    }

    /// Publish progress update to pub/sub channel
    pub async fn publish_progress(&mut self, job_id: Uuid, message: &str) -> Result<()> {
        let channel = format!("genetics:progress:{}", job_id);
        self.conn
            .publish::<_, _, ()>(channel, message)
            .await
            .context("Failed to publish progress update")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty queue must yield `Ok(None)` once the blocking pop expires, not a
    /// client-side timeout. Needs a disposable Redis in TEST_REDIS_URL.
    #[tokio::test]
    async fn dequeue_on_empty_queue_waits_out_the_blocking_pop() {
        let Ok(url) = std::env::var("TEST_REDIS_URL") else {
            eprintln!("SKIPPED: TEST_REDIS_URL not set");
            return;
        };
        let client = redis::Client::open(url).expect("valid TEST_REDIS_URL");
        let conn = connect(client).await.expect("connect to Redis");

        let job = JobQueue::new(conn).dequeue().await;

        assert!(matches!(job, Ok(None)), "unexpected result: {job:?}");
    }
}
