// ==============================================================================
// queue.rs - Redis Job Queue Management
// ==============================================================================
// Description: Job queue operations for genetics processing
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use redis::{Client, Commands};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{OutputFormat, QualityThreshold};

const QUEUE_KEY: &str = "genetics:job_queue";
const JOB_PREFIX: &str = "genetics:job:";

/// Job payload for Redis queue
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
}

/// Job queue manager
pub struct JobQueue {
    client: Client,
}

impl JobQueue {
    /// Create new job queue manager
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Enqueue a new job
    pub fn enqueue(&self, payload: &JobPayload) -> Result<()> {
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        // Serialize job payload
        let payload_json = serde_json::to_string(payload)
            .context("Failed to serialize job payload")?;

        // Push to queue (LPUSH for FIFO with BRPOP)
        conn.lpush::<_, _, ()>(QUEUE_KEY, &payload_json)
            .context("Failed to push job to queue")?;

        // Store job data with expiry (24 hours)
        let job_key = format!("{}{}", JOB_PREFIX, payload.job_id);
        conn.set_ex::<_, _, ()>(&job_key, &payload_json, 86400)
            .context("Failed to store job data")?;

        Ok(())
    }

    /// Publish progress update to pub/sub channel
    pub fn publish_progress(&self, job_id: Uuid, message: &str) -> Result<()> {
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let channel = format!("genetics:progress:{}", job_id);
        conn.publish::<_, _, ()>(channel, message)
            .context("Failed to publish progress update")?;

        Ok(())
    }

    /// Get pub/sub channel name for a job
    pub fn progress_channel(job_id: Uuid) -> String {
        format!("genetics:progress:{}", job_id)
    }

    /// Create a new PubSub connection (caller owns the connection)
    pub fn create_pubsub_connection(&self) -> Result<redis::Connection> {
        self.client.get_connection()
            .context("Failed to get Redis connection for pub/sub")
    }

    /// Get job data
    pub fn get_job(&self, job_id: Uuid) -> Result<Option<JobPayload>> {
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let job_key = format!("{}{}", JOB_PREFIX, job_id);
        let payload_json: Option<String> = conn.get(&job_key)
            .context("Failed to get job data")?;

        match payload_json {
            Some(json) => {
                let payload = serde_json::from_str(&json)
                    .context("Failed to deserialize job payload")?;
                Ok(Some(payload))
            }
            None => Ok(None),
        }
    }

    /// Delete job data from Redis
    pub fn delete_job(&self, job_id: Uuid) -> Result<()> {
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let job_key = format!("{}{}", JOB_PREFIX, job_id);
        conn.del::<_, ()>(&job_key)
            .context("Failed to delete job data")?;

        Ok(())
    }

    /// Get queue length
    pub fn queue_length(&self) -> Result<usize> {
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let len: usize = conn.llen(QUEUE_KEY)
            .context("Failed to get queue length")?;

        Ok(len)
    }
}
