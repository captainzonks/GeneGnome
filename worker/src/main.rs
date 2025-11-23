// ==============================================================================
// main.rs - Genetics Worker Process
// ==============================================================================
// Description: Background worker that processes genetics jobs from Redis queue
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use chrono::Utc;
use redis::Client as RedisClient;
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn, Level};
use uuid::Uuid;
use zip::{ZipWriter, write::SimpleFileOptions};

mod email;
mod job_processor;
mod queue;
mod security;

use email::{EmailConfig, EmailSender};
use job_processor::JobProcessor;
use queue::{JobPayload, JobQueue};
use security::{generate_download_token, generate_download_password, hash_password};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    info!("Starting Genetics Worker v1.0.0");

    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize database connection
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set")?;

    let db_pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    info!("Connected to PostgreSQL");

    // Initialize Redis connection
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let redis_client = RedisClient::open(redis_url)
        .context("Failed to create Redis client")?;

    // Create async connection manager
    let redis_conn = ConnectionManager::new(redis_client)
        .await
        .context("Failed to create Redis connection manager")?;

    info!("Connected to Redis");

    // Get encrypted volume path
    let encrypted_volume_path = PathBuf::from(
        std::env::var("ENCRYPTED_VOLUME_PATH")
            .unwrap_or_else(|_| "/mnt/genetics-encrypted".to_string())
    );

    if !encrypted_volume_path.exists() {
        error!("Encrypted volume not mounted at {:?}", encrypted_volume_path);
        return Err(anyhow::anyhow!("Encrypted volume not accessible"));
    }

    info!("Encrypted volume accessible at {:?}", encrypted_volume_path);

    // Get reference panel path
    let reference_panel_path = encrypted_volume_path.join("reference_panel.db");

    if !reference_panel_path.exists() {
        error!("Reference panel database not found at {:?}", reference_panel_path);
        return Err(anyhow::anyhow!("Reference panel database not accessible"));
    }

    info!("Reference panel database accessible at {:?}", reference_panel_path);

    // Create worker instance
    let worker = Worker::new(db_pool, redis_conn, encrypted_volume_path, reference_panel_path);

    // Recover stuck jobs from previous worker instance
    info!("Checking for stuck jobs from previous worker instance...");
    if let Err(e) = worker.recover_stuck_jobs().await {
        error!("Failed to recover stuck jobs: {}", e);
    }

    // Start cleanup task (runs every hour)
    let cleanup_worker = worker.clone();
    tokio::spawn(async move {
        cleanup_worker.cleanup_loop().await;
    });

    // Start main processing loop
    info!("Worker ready, waiting for jobs...");
    worker.run().await
}

/// Main worker struct
#[derive(Clone)]
struct Worker {
    db_pool: PgPool,
    redis_conn: ConnectionManager,
    encrypted_volume_path: PathBuf,
    reference_panel_path: PathBuf,
}

impl Worker {
    fn new(db_pool: PgPool, redis_conn: ConnectionManager, encrypted_volume_path: PathBuf, reference_panel_path: PathBuf) -> Self {
        Self {
            db_pool,
            redis_conn,
            encrypted_volume_path,
            reference_panel_path,
        }
    }

    /// Main processing loop - polls Redis queue for jobs
    async fn run(&self) -> Result<()> {
        let mut job_queue = JobQueue::new(self.redis_conn.clone());

        loop {
            match job_queue.dequeue().await {
                Ok(Some(payload)) => {
                    info!("Received job: {}", payload.job_id);

                    // Process job in background (don't block queue)
                    let worker = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = worker.process_job(payload).await {
                            error!("Job processing failed: {}", e);
                        }
                    });
                }
                Ok(None) => {
                    // No jobs in queue, wait a bit
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(e) => {
                    error!("Failed to dequeue job: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Process a single job
    async fn process_job(&self, payload: JobPayload) -> Result<()> {
        let job_id = payload.job_id;

        info!("Processing job {}", job_id);

        // Update job status to processing
        self.update_job_status(job_id, &payload.user_id, "processing", None, None, None, None, None).await?;
        self.publish_progress(job_id, 0.0, "Starting processing").await?;

        // Phase 7.1: Reassemble chunks if this is a chunked upload
        let upload_dir = PathBuf::from(&payload.upload_dir);
        if payload.chunked_upload {
            if let Some(upload_session_id) = &payload.upload_session_id {
                info!("Reassembling chunked upload for job {}", job_id);
                self.publish_progress(job_id, 0.0, "Assembling uploaded files").await?;

                let chunks_dir = self.encrypted_volume_path.join("uploads").join("chunks").join(upload_session_id);
                self.reassemble_chunks(&chunks_dir, &upload_dir, job_id).await
                    .context("Failed to reassemble chunks")?;

                info!("Chunk reassembly complete for job {}", job_id);
            } else {
                return Err(anyhow::anyhow!("Chunked upload specified but no upload_session_id provided"));
            }
        }

        // Create job processor
        let processor = JobProcessor::new(
            job_id,
            payload.user_id.clone(),
            upload_dir,
            PathBuf::from(&payload.output_dir),
            self.reference_panel_path.clone(),
            self.db_pool.clone(),
            self.redis_conn.clone(),
        );

        // Execute processing
        match processor.process(&payload.output_formats, payload.quality_threshold).await {
            Ok(_) => {
                info!("Job {} completed successfully", job_id);
                let completed_at = Utc::now();
                let expires_at = completed_at + chrono::Duration::hours(24);

                // Phase 7.2: Create ZIP archive of results
                let output_dir = PathBuf::from(&payload.output_dir);
                let zip_path = self.create_results_zip(&output_dir, job_id).await
                    .context("Failed to create results ZIP archive")?;
                info!("Created results ZIP: {:?}", zip_path);

                // Phase 4: Generate download token and password
                let (token, password, password_hash) = if payload.user_email.is_some() {
                    match (generate_download_token(), generate_download_password()) {
                        (Ok(token), Ok(password)) => {
                            match hash_password(&password) {
                                Ok(hash) => {
                                    info!("Generated download credentials for job {}", job_id);
                                    (Some(token), Some(password), Some(hash))
                                }
                                Err(e) => {
                                    warn!("Failed to hash password for job {}: {}", job_id, e);
                                    (None, None, None)
                                }
                            }
                        }
                        (Err(e1), Err(e2)) => {
                            warn!("Failed to generate credentials for job {}: token={}, password={}", job_id, e1, e2);
                            (None, None, None)
                        }
                        (Err(e), _) => {
                            warn!("Failed to generate token for job {}: {}", job_id, e);
                            (None, None, None)
                        }
                        (_, Err(e)) => {
                            warn!("Failed to generate password for job {}: {}", job_id, e);
                            (None, None, None)
                        }
                    }
                } else {
                    (None, None, None)
                };

                // Update database with credentials and result path (ZIP file)
                let zip_path_str = zip_path.to_str().unwrap_or("");
                self.update_job_status(
                    job_id,
                    &payload.user_id,
                    "completed",
                    None,
                    payload.user_email.as_deref(),
                    token.as_deref(),
                    password_hash.as_deref(),
                    Some(zip_path_str),
                ).await?;

                // Phase 5: Send email notification
                if let (Some(user_email), Some(token_str), Some(password_str)) =
                    (payload.user_email.as_ref(), token.as_ref(), password.as_ref())
                {
                    // Load email configuration
                    match EmailConfig::from_env() {
                        Ok(email_config) => {
                            let email_sender = EmailSender::new(email_config);

                            match email_sender.send_download_notification(
                                job_id,
                                user_email,
                                token_str,
                                password_str,
                                &completed_at,
                                &expires_at,
                            ) {
                                Ok(_) => {
                                    info!("Email notification sent for job {}", job_id);

                                    // Update emailed_at timestamp (with RLS context)
                                    let mut tx = match self.db_pool.begin().await {
                                        Ok(tx) => tx,
                                        Err(e) => {
                                            warn!("Failed to start transaction for emailed_at update: {}", e);
                                            return Ok(());
                                        }
                                    };

                                    let set_query = format!("SET LOCAL app.current_user_id = '{}'", payload.user_id.replace("'", "''"));
                                    if let Err(e) = sqlx::query(&set_query)
                                        .execute(&mut *tx)
                                        .await
                                    {
                                        warn!("Failed to set RLS context for emailed_at: {}", e);
                                        let _ = tx.rollback().await;
                                        return Ok(());
                                    }

                                    if let Err(e) = sqlx::query(
                                        "UPDATE genetics.genetics_jobs SET emailed_at = NOW() WHERE id = $1"
                                    )
                                    .bind(job_id)
                                    .execute(&mut *tx)
                                    .await
                                    {
                                        warn!("Failed to update emailed_at for job {}: {}", job_id, e);
                                        let _ = tx.rollback().await;
                                    } else if let Err(e) = tx.commit().await {
                                        warn!("Failed to commit emailed_at update: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to send email for job {}: {}", job_id, e);
                                    // Don't fail the job if email fails
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to load email config for job {}: {}", job_id, e);
                            // Don't fail the job if email config is missing
                        }
                    }
                }

                self.publish_progress(job_id, 100.0, "Processing complete").await?;
            }
            Err(e) => {
                error!("Job {} failed: {}", job_id, e);
                let error_msg = format!("{:#}", e);
                self.update_job_status(job_id, &payload.user_id, "failed", Some(&error_msg), None, None, None, None).await?;
                self.publish_progress(job_id, 0.0, &format!("Failed: {}", error_msg)).await?;
            }
        }

        Ok(())
    }

    /// Update job status in database (with RLS context)
    async fn update_job_status(
        &self,
        job_id: Uuid,
        user_id: &str,
        status: &str,
        error_message: Option<&str>,
        user_email: Option<&str>,
        download_token: Option<&str>,
        download_password_hash: Option<&str>,
        result_path: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();

        // Use transaction to set RLS context
        let mut tx = self.db_pool.begin().await
            .context("Failed to start transaction for job status update")?;

        // Set RLS context (SET command doesn't support placeholders, must use format!)
        let set_query = format!("SET LOCAL app.current_user_id = '{}'", user_id.replace("'", "''"));
        sqlx::query(&set_query)
            .execute(&mut *tx)
            .await
            .context("Failed to set RLS context")?;

        if status == "processing" {
            sqlx::query(
                "UPDATE genetics.genetics_jobs SET status = $1, started_at = $2 WHERE id = $3"
            )
            .bind(status)
            .bind(now)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .context("Failed to update job status to processing")?;
        } else if status == "completed" {
            // Phase 4: Update with email, download credentials, and result path
            sqlx::query(
                "UPDATE genetics.genetics_jobs
                 SET status = $1,
                     completed_at = $2,
                     user_email = $3,
                     download_token = $4,
                     download_password_hash = $5,
                     result_path = $6
                 WHERE id = $7"
            )
            .bind(status)
            .bind(now)
            .bind(user_email)
            .bind(download_token)
            .bind(download_password_hash)
            .bind(result_path)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .context("Failed to update job status to completed")?;
        } else if status == "failed" {
            sqlx::query(
                "UPDATE genetics.genetics_jobs SET status = $1, completed_at = $2, error_message = $3 WHERE id = $4"
            )
            .bind(status)
            .bind(now)
            .bind(error_message)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .context("Failed to update job status to failed")?;
        }

        tx.commit().await
            .context("Failed to commit job status update")?;

        Ok(())
    }

    /// Publish progress update via Redis pub/sub
    async fn publish_progress(&self, job_id: Uuid, progress: f32, message: &str) -> Result<()> {
        let mut job_queue = JobQueue::new(self.redis_conn.clone());

        let progress_msg = serde_json::json!({
            "job_id": job_id,
            "progress_pct": progress,
            "message": message,
            "timestamp": Utc::now().to_rfc3339(),
        });

        job_queue.publish_progress(job_id, &progress_msg.to_string()).await?;

        Ok(())
    }

    /// Create ZIP archive of all result files in output directory
    /// Uses STORE method (no compression) since files are already compressed
    async fn create_results_zip(&self, output_dir: &PathBuf, job_id: Uuid) -> Result<PathBuf> {
        let zip_filename = format!("results_{}.zip", job_id);
        let zip_path = output_dir.join(&zip_filename);

        info!("Creating ZIP archive: {:?}", zip_path);

        // Use blocking task for synchronous ZIP operations
        let output_dir_clone = output_dir.clone();
        let zip_path_clone = zip_path.clone();

        tokio::task::spawn_blocking(move || {
            let zip_file = File::create(&zip_path_clone)
                .context("Failed to create ZIP file")?;

            let mut zip = ZipWriter::new(zip_file);

            // Use STORE method (no compression) since files are already compressed (.gz, .parquet)
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            // Read all files in the output directory
            let entries = std::fs::read_dir(&output_dir_clone)
                .context("Failed to read output directory")?;

            let mut file_count = 0;
            for entry in entries {
                let entry = entry.context("Failed to read directory entry")?;
                let path = entry.path();

                // Skip the ZIP file itself and directories
                if path.is_file() && path != zip_path_clone {
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .context("Invalid filename")?;

                    info!("Adding to ZIP: {}", filename);

                    zip.start_file(filename, options)
                        .context("Failed to start ZIP file entry")?;

                    let mut file = File::open(&path)
                        .context("Failed to open result file")?;

                    std::io::copy(&mut file, &mut zip)
                        .context("Failed to copy file to ZIP")?;

                    file_count += 1;
                }
            }

            zip.finish().context("Failed to finalize ZIP archive")?;

            info!("ZIP archive created successfully with {} files", file_count);

            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("ZIP creation task failed")??;

        Ok(zip_path)
    }

    /// Recover jobs that were stuck in "processing" state from previous worker instance
    async fn recover_stuck_jobs(&self) -> Result<()> {
        // Find jobs stuck in processing state for more than 10 minutes
        let cutoff = Utc::now() - chrono::Duration::minutes(10);

        let stuck_jobs: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, user_id FROM genetics.genetics_jobs
             WHERE status = 'processing'
             AND started_at < $1"
        )
        .bind(cutoff)
        .fetch_all(&self.db_pool)
        .await
        .context("Failed to query stuck jobs")?;

        if stuck_jobs.is_empty() {
            info!("No stuck jobs found");
            return Ok(());
        }

        info!("Found {} stuck job(s), marking as failed", stuck_jobs.len());

        for (job_id, user_id) in stuck_jobs {
            warn!("Marking stuck job as failed: {} (user: {})", job_id, user_id);

            // Mark job as failed with explanation
            let error_msg = "Job interrupted by worker restart. Please resubmit your data.";
            self.update_job_status(job_id, &user_id, "failed", Some(error_msg), None, None, None, None).await?;
            self.publish_progress(job_id, 0.0, error_msg).await?;
        }

        Ok(())
    }

    /// Cleanup loop - runs every hour to delete old jobs
    async fn cleanup_loop(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await; // 1 hour

            info!("Running cleanup task");

            if let Err(e) = self.cleanup_old_jobs().await {
                error!("Cleanup task failed: {}", e);
            }
        }
    }

    /// Delete jobs older than 24 hours
    async fn cleanup_old_jobs(&self) -> Result<()> {
        let cutoff = Utc::now() - chrono::Duration::hours(24);

        // Find old completed jobs
        let old_jobs: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, user_id FROM genetics.genetics_jobs
             WHERE status IN ('completed', 'failed')
             AND completed_at < $1"
        )
        .bind(cutoff)
        .fetch_all(&self.db_pool)
        .await
        .context("Failed to query old jobs")?;

        for (job_id, user_id) in old_jobs {
            info!("Cleaning up old job: {} (user: {})", job_id, user_id);

            // Delete files from encrypted volume
            let upload_dir = self.encrypted_volume_path.join("uploads").join(job_id.to_string());
            let results_dir = self.encrypted_volume_path.join("results").join(job_id.to_string());

            if upload_dir.exists() {
                // TODO: Use secure_delete from genetics-processor
                match tokio::fs::remove_dir_all(&upload_dir).await {
                    Ok(_) => info!("Deleted upload directory for job {}", job_id),
                    Err(e) => warn!("Failed to delete upload directory: {}", e),
                }
            }

            if results_dir.exists() {
                match tokio::fs::remove_dir_all(&results_dir).await {
                    Ok(_) => info!("Deleted results directory for job {}", job_id),
                    Err(e) => warn!("Failed to delete results directory: {}", e),
                }
            }

            // Use transaction to set RLS context and delete job
            let mut tx = match self.db_pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    error!("Failed to start transaction for job {}: {}", job_id, e);
                    continue;
                }
            };

            // Set app.current_user_id for RLS policy (SET doesn't support placeholders)
            let set_query = format!("SET LOCAL app.current_user_id = '{}'", user_id.replace("'", "''"));
            if let Err(e) = sqlx::query(&set_query)
                .execute(&mut *tx)
                .await
            {
                error!("Failed to set RLS context for job {}: {}", job_id, e);
                let _ = tx.rollback().await;
                continue;
            }

            // Delete from database
            match sqlx::query("DELETE FROM genetics.genetics_jobs WHERE id = $1")
                .bind(job_id)
                .execute(&mut *tx)
                .await
            {
                Ok(_) => {
                    if let Err(e) = tx.commit().await {
                        error!("Failed to commit deletion for job {}: {}", job_id, e);
                    } else {
                        info!("Cleaned up job {}", job_id);
                    }
                }
                Err(e) => {
                    error!("Failed to delete job {} from database: {}", job_id, e);
                    let _ = tx.rollback().await;
                }
            }
        }

        Ok(())
    }

    /// Phase 7.1: Reassemble chunked files from upload session
    async fn reassemble_chunks(
        &self,
        chunks_dir: &PathBuf,
        target_dir: &PathBuf,
        job_id: Uuid,
    ) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        if !chunks_dir.exists() {
            return Err(anyhow::anyhow!("Chunks directory not found: {:?}", chunks_dir));
        }

        // Get all chunk files and group by original filename
        let mut entries = tokio::fs::read_dir(chunks_dir).await
            .context("Failed to read chunks directory")?;

        let mut chunks_by_file: std::collections::HashMap<String, Vec<(usize, PathBuf)>> =
            std::collections::HashMap::new();

        while let Some(entry) = entries.next_entry().await
            .context("Failed to read directory entry")?
        {
            let path = entry.path();
            let filename = entry.file_name().to_string_lossy().to_string();

            // Parse filename: "original_name_####"
            if let Some(last_underscore) = filename.rfind('_') {
                let original_name = &filename[..last_underscore];
                let chunk_index_str = &filename[last_underscore + 1..];

                if let Ok(chunk_index) = chunk_index_str.parse::<usize>() {
                    chunks_by_file.entry(original_name.to_string())
                        .or_insert_with(Vec::new)
                        .push((chunk_index, path));
                }
            }
        }

        info!("Found {} files to reassemble for job {}", chunks_by_file.len(), job_id);

        // Reassemble each file
        for (original_filename, mut chunks) in chunks_by_file {
            // Sort chunks by index
            chunks.sort_by_key(|(idx, _)| *idx);

            // Create reassembled file
            let output_path = target_dir.join(&original_filename);
            let mut output_file = tokio::fs::File::create(&output_path).await
                .context(format!("Failed to create output file: {:?}", output_path))?;

            // Write chunks in order
            for (chunk_index, chunk_path) in chunks {
                let chunk_data = tokio::fs::read(&chunk_path).await
                    .context(format!("Failed to read chunk: {:?}", chunk_path))?;

                output_file.write_all(&chunk_data).await
                    .context(format!("Failed to write to output file: {:?}", output_path))?;

                info!("Reassembled chunk {} for file {}", chunk_index, original_filename);
            }

            output_file.flush().await
                .context(format!("Failed to flush output file: {:?}", output_path))?;

            info!("Reassembled file: {}", original_filename);
        }

        // Clean up chunks directory
        tokio::fs::remove_dir_all(chunks_dir).await
            .context("Failed to clean up chunks directory")?;

        info!("Cleaned up chunks directory for job {}", job_id);

        Ok(())
    }
}
