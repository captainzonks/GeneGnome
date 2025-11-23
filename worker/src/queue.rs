// ==============================================================================
// queue.rs - Redis Job Queue Management (Worker Side)
// ==============================================================================
// Description: Job queue operations for consuming jobs from Redis
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
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
pub enum QualityThreshold {
    None,       // No filtering - include all variants
    R080,       // R² ≥ 0.8
    R090,       // R² ≥ 0.9 (default, matches R script)
}

impl QualityThreshold {
    /// Get the numeric threshold value
    pub fn value(&self) -> Option<f64> {
        match self {
            QualityThreshold::None => None,
            QualityThreshold::R080 => Some(0.8),
            QualityThreshold::R090 => Some(0.9),
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            QualityThreshold::None => "No filtering (all variants)",
            QualityThreshold::R080 => "R² ≥ 0.8 (good quality)",
            QualityThreshold::R090 => "R² ≥ 0.9 (high quality, matches R script)",
        }
    }
}

impl Default for QualityThreshold {
    fn default() -> Self {
        QualityThreshold::R090  // Default to R² ≥ 0.9 to match R script behavior
    }
}

/// Job payload from Redis queue (must match API gateway)
#[derive(Debug, Serialize, Deserialize)]
pub struct JobPayload {
    pub job_id: Uuid,
    pub user_id: String,
    pub user_email: Option<String>,  // Phase 4: Email for download notifications
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
        // BRPOP with 1 second timeout
        let result: Option<(String, String)> = self.conn
            .brpop(QUEUE_KEY, 1.0)
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
        self.conn.publish::<_, _, ()>(channel, message)
            .await
            .context("Failed to publish progress update")?;

        Ok(())
    }
}
