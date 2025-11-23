// ==============================================================================
// handlers.rs - API Request Handlers
// ==============================================================================
// Description: HTTP request handlers for genetics API endpoints
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Multipart, Path, Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use futures_util::sink::SinkExt;
use redis::Commands;
use serde::Deserialize;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    // PUBLIC PLATFORM: No authentication middleware needed
    models::*,
    queue::{JobPayload, JobQueue},
    security::verify_password,
    state::AppState,
    validator::FileValidator,
};

/// Root endpoint - API information
pub async fn root() -> Json<ApiInfoResponse> {
    Json(ApiInfoResponse {
        service: "Genetics API Gateway",
        version: "1.0.0",
        endpoints: vec![
            "/health - Health check",
            "/ready - Readiness check",
            "/api/genetics/jobs - Submit job (POST)",
            "/api/genetics/jobs/{job_id} - Get status (GET) or delete (DELETE)",
            "/api/genetics/jobs/{job_id}/ws - WebSocket progress updates",
            "/api/genetics/results/{job_id} - Download results (GET)",
            "/api/genetics/upload/chunks - Upload file chunk (POST, for files >50MB)",
            "/api/genetics/upload/finalize - Finalize chunked upload (POST)",
        ],
    })
}

/// Health check endpoint
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "1.0.0",
        timestamp: Utc::now(),
    })
}

/// Readiness check endpoint
pub async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    // Check database connection
    let db_ready = sqlx::query("SELECT 1")
        .fetch_one(state.db_pool())
        .await
        .is_ok();

    // Check Redis connection
    let redis_ready = state.redis_client().get_connection().is_ok();

    // Check encrypted volume exists
    let volume_ready = state.encrypted_volume_path().exists();

    let ready = db_ready && redis_ready && volume_ready;

    let response = ReadinessResponse {
        ready,
        database: db_ready,
        redis: redis_ready,
        encrypted_volume: volume_ready,
    };

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(response))
}

/// Submit job endpoint (file upload)
pub async fn submit_job(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<JobSubmitResponse>, AppError> {
    info!("Received job submission request");

    // Initialize file validator
    let validator = FileValidator::new();

    // Generate job ID
    let job_id = Uuid::new_v4();

    // Create job-specific upload directory
    let job_upload_dir = state.upload_dir().join(job_id.to_string());
    tokio::fs::create_dir_all(&job_upload_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create upload directory: {}", e)))?;

    let mut genome_file: Option<PathBuf> = None;
    let mut vcf_files: Vec<PathBuf> = Vec::new();
    let mut pgs_file: Option<PathBuf> = None;
    let mut output_formats = vec![OutputFormat::Parquet, OutputFormat::Vcf]; // Default formats (Parquet for analytics, VCF for bioinformatics)
    let mut quality_threshold = QualityThreshold::default(); // Default R² ≥ 0.9
    let mut user_email: Option<String> = None; // REQUIRED: Email for job ownership and notifications
    let mut vcf_format = "merged".to_string(); // Default to merged VCF

    // Process multipart form fields
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "genome_file" => {
                let filename = field.file_name().unwrap_or("genome.txt").to_string();
                let data = field.bytes().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read genome file: {}", e)))?;

                // SECURITY: Validate file before writing to disk
                let validated = validator.validate_upload(&filename, &data, "genome")
                    .map_err(|e| AppError::BadRequest(format!("Invalid genome file: {}", e)))?;

                info!("Genome file validated: {} ({} bytes, SHA256: {})",
                    validated.safe_name, validated.size, &validated.hash_sha256[..16]);

                // Save file using sanitized filename
                let file_path = job_upload_dir.join(&validated.safe_name);
                let mut file = tokio::fs::File::create(&file_path)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;
                file.write_all(&data)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

                genome_file = Some(file_path);
                info!("Saved genome file: {}", validated.safe_name);
            }

            "vcf_file" => {
                let filename = field.file_name().unwrap_or("chr.vcf.gz").to_string();
                let data = field.bytes().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read VCF file: {}", e)))?;

                // SECURITY: Validate file before writing to disk
                let validated = validator.validate_upload(&filename, &data, "vcf")
                    .map_err(|e| AppError::BadRequest(format!("Invalid VCF file: {}", e)))?;

                info!("VCF file validated: {} ({} bytes, SHA256: {})",
                    validated.safe_name, validated.size, &validated.hash_sha256[..16]);

                // Save file using sanitized filename
                let file_path = job_upload_dir.join(&validated.safe_name);
                let mut file = tokio::fs::File::create(&file_path)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;
                file.write_all(&data)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

                vcf_files.push(file_path);
                info!("Saved VCF file: {}", validated.safe_name);
            }

            "pgs_file" => {
                let filename = field.file_name().unwrap_or("scores.txt").to_string();
                let data = field.bytes().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read PGS file: {}", e)))?;

                // SECURITY: Validate file before writing to disk
                let validated = validator.validate_upload(&filename, &data, "pgs")
                    .map_err(|e| AppError::BadRequest(format!("Invalid PGS file: {}", e)))?;

                info!("PGS file validated: {} ({} bytes, SHA256: {})",
                    validated.safe_name, validated.size, &validated.hash_sha256[..16]);

                // Save file using sanitized filename
                let file_path = job_upload_dir.join(&validated.safe_name);
                let mut file = tokio::fs::File::create(&file_path)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;
                file.write_all(&data)
                    .await
                    .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

                pgs_file = Some(file_path);
                info!("Saved PGS file: {}", validated.safe_name);
            }

            "output_formats" => {
                let data = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read output formats: {}", e)))?;

                // Parse format value (can be single format or comma-separated)
                let formats: Vec<OutputFormat> = data
                    .split(',')
                    .filter_map(|s| match s.trim().to_lowercase().as_str() {
                        "parquet" => Some(OutputFormat::Parquet),
                        "sqlite" => Some(OutputFormat::Sqlite),
                        "vcf" => Some(OutputFormat::Vcf),
                        _ => None,
                    })
                    .collect();

                // Accumulate formats (multiple fields with same name)
                output_formats.extend(formats);
            }

            "quality_threshold" => {
                let data = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read quality threshold: {}", e)))?;

                // Parse quality threshold value
                quality_threshold = match data.trim().to_lowercase().as_str() {
                    "none" => QualityThreshold::None,
                    "r080" | "0.8" => QualityThreshold::R080,
                    "r090" | "0.9" => QualityThreshold::R090,
                    _ => {
                        warn!("Unknown quality threshold '{}', using default (r090)", data);
                        QualityThreshold::R090
                    }
                };
            }

            "user_email" => {
                let email = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read user email: {}", e)))?;

                // Basic email validation
                if !email.trim().is_empty() && email.contains('@') {
                    user_email = Some(email.trim().to_string());
                    info!("Job {} will send download notification to {}", job_id, email.trim());
                } else {
                    warn!("Invalid email provided: {}", email);
                }
            }

            "vcf_format" => {
                let format = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read VCF format: {}", e)))?;

                // Validate and store VCF format preference
                vcf_format = match format.trim().to_lowercase().as_str() {
                    "merged" => "merged".to_string(),
                    "per_chromosome" => "per_chromosome".to_string(),
                    _ => {
                        warn!("Unknown VCF format '{}', using default (merged)", format);
                        "merged".to_string()
                    }
                };
                info!("Job {} VCF format preference: {}", job_id, vcf_format);
            }

            _ => {
                warn!("Unknown multipart field: {}", name);
            }
        }
    }

    // Deduplicate formats (in case same format sent multiple times)
    output_formats.sort();
    output_formats.dedup();

    // Default to Parquet + VCF if no formats specified
    if output_formats.is_empty() {
        output_formats.push(OutputFormat::Parquet);
        output_formats.push(OutputFormat::Vcf);
    }

    info!("Job output formats: {:?}", output_formats);
    info!("Job quality threshold: {:?}", quality_threshold);

    // Validate required files
    let genome_file = genome_file
        .ok_or_else(|| AppError::BadRequest("Missing genome_file".to_string()))?;

    if vcf_files.is_empty() {
        return Err(AppError::BadRequest("Missing vcf_file(s)".to_string()));
    }

    let pgs_file = pgs_file
        .ok_or_else(|| AppError::BadRequest("Missing pgs_file".to_string()))?;

    // PUBLIC PLATFORM: Require email for job ownership and notifications
    let user_email = user_email
        .ok_or_else(|| AppError::BadRequest("Email address is required for job submission".to_string()))?;

    // Create job in database with VCF format metadata
    let created_at = Utc::now();
    let metadata = serde_json::json!({
        "vcf_format": vcf_format
    });

    // PUBLIC PLATFORM: Use email as user_id (no RLS/authentication needed)
    sqlx::query(
        "INSERT INTO genetics_jobs (id, user_id, status, created_at, metadata) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(job_id)
    .bind(&user_email)
    .bind("pending")
    .bind(created_at)
    .bind(&metadata)
    .execute(state.db_pool())
    .await
    .map_err(|e| AppError::Internal(format!("Failed to create job in database: {}", e)))?;

    // Create results directory for job
    let job_results_dir = state.results_dir().join(job_id.to_string());
    tokio::fs::create_dir_all(&job_results_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create results directory: {}", e)))?;

    // Enqueue job
    let job_queue = JobQueue::new(state.redis_client().clone());
    let payload = JobPayload {
        job_id,
        user_id: user_email.clone(),  // PUBLIC PLATFORM: Email is the user identifier
        user_email: Some(user_email.clone()),  // Also store in email field for notifications
        upload_dir: job_upload_dir.to_string_lossy().to_string(),
        output_dir: job_results_dir.to_string_lossy().to_string(),
        output_formats: output_formats.clone(),
        quality_threshold,
        chunked_upload: false,  // Phase 7.1: Standard upload, no reassembly needed
        upload_session_id: None,  // Phase 7.1: Only for chunked uploads
    };

    job_queue.enqueue(&payload)
        .map_err(|e| AppError::Internal(format!("Failed to enqueue job: {}", e)))?;

    info!("Job {} queued successfully", job_id);

    Ok(Json(JobSubmitResponse {
        job_id,
        status: JobStatus::Queued,
        created_at,
        estimated_completion: None, // TODO: Calculate based on queue length
    }))
}

/// Get job status endpoint
pub async fn get_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<JobStatusResponse>, AppError> {
    // PUBLIC PLATFORM: Anyone with job_id can check status (no authentication required)
    // Query job from database
    let job = sqlx::query_as::<_, (uuid::Uuid, String, String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>, Option<String>)>(
        "SELECT id, user_id, status, created_at, started_at, completed_at, error_message FROM genetics_jobs WHERE id = $1"
    )
    .bind(job_id)
    .fetch_optional(state.db_pool())
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?
    .ok_or(AppError::NotFound)?;

    let (job_id_db, user_id_db, status_str, created_at_db, started_at_db, completed_at_db, error_message_db) = job;

    let status = match status_str.as_str() {
        "queued" => JobStatus::Queued,
        "processing" => JobStatus::Processing,
        "complete" => JobStatus::Complete,
        "failed" => JobStatus::Failed,
        _ => JobStatus::Queued,
    };

    // Calculate progress (simplified)
    let progress = match status {
        JobStatus::Queued => 0.0,
        JobStatus::Processing => 50.0, // TODO: Get actual progress from Redis
        JobStatus::Complete => 100.0,
        JobStatus::Failed => 0.0,
    };

    // Query output formats from genetics_files table
    let output_formats: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT LOWER(file_type) FROM genetics_files WHERE job_id = $1 ORDER BY LOWER(file_type)"
    )
    .bind(job_id)
    .fetch_all(state.db_pool())
    .await
    .unwrap_or_default();

    Ok(Json(JobStatusResponse {
        job_id: job_id_db,
        user_id: user_id_db,
        status,
        progress,
        created_at: created_at_db,
        started_at: started_at_db,
        completed_at: completed_at_db,
        error_message: error_message_db,
        output_formats,
        files: JobFiles {
            genome_file: "genome.txt".to_string(),
            vcf_files: vec!["chr1-22.vcf.gz".to_string()],
            pgs_file: "scores.txt".to_string(),
        },
    }))
}

/// Delete job endpoint
pub async fn delete_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    info!("Deleting job {}", job_id);

    // PUBLIC PLATFORM: Anyone with job_id can delete (no authentication required)
    // Delete from database
    let result = sqlx::query("DELETE FROM genetics_jobs WHERE id = $1")
        .bind(job_id)
        .execute(state.db_pool())
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    // Delete from Redis
    let job_queue = JobQueue::new(state.redis_client().clone());
    job_queue.delete_job(job_id)
        .map_err(|e| AppError::Internal(format!("Failed to delete from Redis: {}", e)))?;

    // Delete files from encrypted volume
    let encrypted_volume = state.encrypted_volume_path();
    let upload_dir = encrypted_volume.join("uploads").join(job_id.to_string());
    let results_dir = encrypted_volume.join("results").join(job_id.to_string());

    // Remove upload directory if it exists
    if upload_dir.exists() {
        tokio::fs::remove_dir_all(&upload_dir)
            .await
            .map_err(|e| {
                warn!("Failed to delete upload directory {:?}: {}", upload_dir, e);
                AppError::Internal(format!("Failed to delete upload directory: {}", e))
            })?;
        info!("Deleted upload directory: {:?}", upload_dir);
    }

    // Remove results directory if it exists
    if results_dir.exists() {
        tokio::fs::remove_dir_all(&results_dir)
            .await
            .map_err(|e| {
                warn!("Failed to delete results directory {:?}: {}", results_dir, e);
                AppError::Internal(format!("Failed to delete results directory: {}", e))
            })?;
        info!("Deleted results directory: {:?}", results_dir);
    }

    info!("Successfully deleted job {} and all associated files", job_id);
    Ok(StatusCode::NO_CONTENT)
}

/// WebSocket progress updates endpoint
pub async fn job_progress_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| handle_progress_socket(socket, state, job_id))
}

async fn handle_progress_socket(socket: WebSocket, state: AppState, job_id: Uuid) {
    use futures::stream::StreamExt;
    use tokio::sync::mpsc;

    info!("WebSocket connected for job {}", job_id);

    // Split socket into sender and receiver early so we can send initial state
    let (mut sender, mut receiver) = socket.split();

    // PUBLIC PLATFORM: No authentication required - anyone with job_id can watch progress
    // Query current job status from database and send immediately
    match sqlx::query_as::<_, (String,)>("SELECT status FROM genetics_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(state.db_pool())
        .await
    {
        Ok(Some((status,))) => {
            let initial_msg = serde_json::json!({
                "type": "status",
                "status": status,
                "message": format!("Current status: {}", status)
            });
            if let Ok(msg_str) = serde_json::to_string(&initial_msg) {
                let _ = sender.send(Message::Text(msg_str.into())).await;
            }
        }
        Ok(None) => {
            error!("Job {} not found", job_id);
            let error_msg = serde_json::json!({
                "type": "error",
                "error": "job_not_found",
                "message": "Job does not exist or has been deleted"
            });
            if let Ok(msg_str) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(msg_str.into())).await;
            }
            return;
        }
        Err(e) => {
            error!("Failed to query job status: {}", e);
            let error_msg = serde_json::json!({
                "type": "error",
                "error": "database_error",
                "message": "Failed to query job status"
            });
            if let Ok(msg_str) = serde_json::to_string(&error_msg) {
                let _ = sender.send(Message::Text(msg_str.into())).await;
            }
            return;
        }
    }

    // Get dedicated Redis connection for pub/sub
    let job_queue = JobQueue::new(state.redis_client().clone());
    let mut conn = match job_queue.create_pubsub_connection() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create pub/sub connection: {}", e);
            return;
        }
    };

    let channel = JobQueue::progress_channel(job_id);

    // Create channel for communicating between blocking Redis thread and async event loop
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Spawn blocking task to poll Redis pub/sub
    // This prevents the blocking get_message() from holding up the async event loop
    // We move conn into the closure and create pubsub inside to satisfy lifetime requirements
    let redis_handle = tokio::task::spawn_blocking(move || {
        let mut pubsub = conn.as_pubsub();

        if let Err(e) = pubsub.subscribe(&channel) {
            error!("Failed to subscribe to channel: {}", e);
            return;
        }

        loop {
            // get_message() blocks until a message arrives (no timeout variant available)
            match pubsub.get_message() {
                Ok(msg) => {
                    if let Ok(payload) = msg.get_payload::<String>() {
                        if tx.send(payload).is_err() {
                            // Channel closed, stop polling
                            break;
                        }
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        // Timeout is expected, continue polling
                        continue;
                    }
                    // Other errors indicate connection issues
                    error!("Redis pub/sub error in blocking task: {}", e);
                    break;
                }
            }
        }
    });

    // Ping interval for keepalive
    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Main event loop - now fully async!
    loop {
        tokio::select! {
            // Handle Redis pub/sub messages from channel
            Some(payload) = rx.recv() => {
                if sender.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }

            // Send periodic pings for keepalive
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }

            // Handle incoming client messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to ping
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Client initiated close for job {}", job_id);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        // Connection closed
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Clean up: dropping tx will cause redis_handle to exit
    drop(rx);
    let _ = redis_handle.await;

    info!("WebSocket disconnected for job {}", job_id);
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    format: Option<String>,
}
// Old insecure download endpoint removed - replaced with secure token-based download (Phase 6)
// See download_results() below for the new implementation with password verification

/// Upload chunk endpoint (for chunked uploads >50MB)
pub async fn upload_chunk(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    info!("Chunk upload request received");

    // Initialize file validator
    let validator = FileValidator::new();

    let mut upload_id: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut file_type: Option<String> = None;
    let mut chunk_index: Option<usize> = None;
    let mut total_chunks: Option<usize> = None;
    let mut chunk_data: Option<Vec<u8>> = None;

    // Process multipart form fields
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "upload_id" => {
                upload_id = Some(field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read upload_id: {}", e)))?);
            }
            "filename" => {
                filename = Some(field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read filename: {}", e)))?);
            }
            "file_type" => {
                file_type = Some(field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read file_type: {}", e)))?);
            }
            "chunk_index" => {
                let text = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read chunk_index: {}", e)))?;
                chunk_index = Some(text.parse()
                    .map_err(|e| AppError::BadRequest(format!("Invalid chunk_index: {}", e)))?);
            }
            "total_chunks" => {
                let text = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read total_chunks: {}", e)))?;
                total_chunks = Some(text.parse()
                    .map_err(|e| AppError::BadRequest(format!("Invalid total_chunks: {}", e)))?);
            }
            "chunk" => {
                chunk_data = Some(field.bytes().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read chunk data: {}", e)))?
                    .to_vec());
            }
            _ => {
                warn!("Unknown chunk upload field: {}", name);
            }
        }
    }

    // Validate required fields
    let upload_id = upload_id.ok_or_else(|| AppError::BadRequest("Missing upload_id".to_string()))?;
    let filename = filename.ok_or_else(|| AppError::BadRequest("Missing filename".to_string()))?;
    let file_type = file_type.ok_or_else(|| AppError::BadRequest("Missing file_type".to_string()))?;
    let chunk_index = chunk_index.ok_or_else(|| AppError::BadRequest("Missing chunk_index".to_string()))?;
    let total_chunks = total_chunks.ok_or_else(|| AppError::BadRequest("Missing total_chunks".to_string()))?;
    let chunk_data = chunk_data.ok_or_else(|| AppError::BadRequest("Missing chunk data".to_string()))?;

    // SECURITY: Validate chunk before writing to disk
    let chunk_bytes = axum::body::Bytes::from(chunk_data.clone());
    validator.validate_chunk(&filename, &chunk_bytes, chunk_index, total_chunks)
        .map_err(|e| AppError::BadRequest(format!("Invalid chunk: {}", e)))?;

    info!("Chunk validated: {} ({}/{}, {} bytes)", filename, chunk_index + 1, total_chunks, chunk_data.len());

    // Create upload session directory
    let upload_session_dir = state.upload_dir().join("chunks").join(&upload_id);
    tokio::fs::create_dir_all(&upload_session_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create upload session directory: {}", e)))?;

    // Save chunk to disk
    let chunk_filename = format!("{}_{:04}", filename, chunk_index);
    let chunk_path = upload_session_dir.join(&chunk_filename);

    let mut file = tokio::fs::File::create(&chunk_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create chunk file: {}", e)))?;
    file.write_all(&chunk_data)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write chunk: {}", e)))?;

    info!("Saved chunk {}/{} for file {} (upload_id: {})",
        chunk_index + 1, total_chunks, filename, upload_id);

    // Store chunk metadata in Redis for tracking
    let metadata_key = format!("chunk:{}:{}:{}", upload_id, filename, chunk_index);
    let metadata = serde_json::json!({
        "filename": filename,
        "file_type": file_type,
        "chunk_index": chunk_index,
        "total_chunks": total_chunks,
        "size": chunk_data.len(),
    }).to_string();

    let mut conn = state.redis_client()
        .get_connection()
        .map_err(|e| AppError::Internal(format!("Failed to get Redis connection: {}", e)))?;

    conn.set_ex::<_, _, ()>(&metadata_key, metadata, 3600) // 1 hour expiry
        .map_err(|e| AppError::Internal(format!("Failed to store chunk metadata: {}", e)))?;

    Ok(StatusCode::OK)
}

/// Finalize chunked upload endpoint
pub async fn finalize_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<JobSubmitResponse>, AppError> {
    info!("Finalizing chunked upload");
    let mut upload_id: Option<String> = None;
    let mut output_formats = Vec::new();
    let mut quality_threshold = QualityThreshold::default(); // Default R² ≥ 0.9
    let mut user_email: Option<String> = None; // REQUIRED: Email for job ownership and notifications
    let mut vcf_format = "merged".to_string(); // Default to merged VCF

    // Process multipart form fields
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "upload_id" => {
                upload_id = Some(field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read upload_id: {}", e)))?);
            }
            "output_formats" => {
                let data = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read output formats: {}", e)))?;

                // Parse format value (can be single format or comma-separated)
                let formats: Vec<OutputFormat> = data
                    .split(',')
                    .filter_map(|s| match s.trim().to_lowercase().as_str() {
                        "parquet" => Some(OutputFormat::Parquet),
                        "sqlite" => Some(OutputFormat::Sqlite),
                        "vcf" => Some(OutputFormat::Vcf),
                        _ => None,
                    })
                    .collect();

                // Accumulate formats (multiple fields with same name)
                output_formats.extend(formats);
            }
            "quality_threshold" => {
                let data = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read quality threshold: {}", e)))?;

                // Parse quality threshold value
                quality_threshold = match data.trim().to_lowercase().as_str() {
                    "none" => QualityThreshold::None,
                    "r080" | "0.8" => QualityThreshold::R080,
                    "r090" | "0.9" => QualityThreshold::R090,
                    _ => {
                        warn!("Unknown quality threshold '{}', using default (r090)", data);
                        QualityThreshold::R090
                    }
                };
            }
            "user_email" => {
                let email = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read user email: {}", e)))?;

                // Basic email validation
                if !email.trim().is_empty() && email.contains('@') {
                    user_email = Some(email.trim().to_string());
                    info!("Chunked upload will send download notification to {}", email.trim());
                } else {
                    warn!("Invalid email provided: {}", email);
                }
            }
            "vcf_format" => {
                let format = field.text().await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read VCF format: {}", e)))?;

                // Validate and store VCF format preference
                vcf_format = match format.trim().to_lowercase().as_str() {
                    "merged" => "merged".to_string(),
                    "per_chromosome" => "per_chromosome".to_string(),
                    _ => {
                        warn!("Unknown VCF format '{}', using default (merged)", format);
                        "merged".to_string()
                    }
                };
                info!("Chunked upload VCF format preference: {}", vcf_format);
            }
            _ => {
                warn!("Unknown finalize field: {}", name);
            }
        }
    }

    // Deduplicate formats (in case same format sent multiple times)
    output_formats.sort();
    output_formats.dedup();

    // Default to Parquet + VCF if no formats specified
    if output_formats.is_empty() {
        output_formats.push(OutputFormat::Parquet);
        output_formats.push(OutputFormat::Vcf);
    }

    let upload_id = upload_id.ok_or_else(|| AppError::BadRequest("Missing upload_id".to_string()))?;

    // PUBLIC PLATFORM: Require email for job ownership and notifications
    let user_email = user_email
        .ok_or_else(|| AppError::BadRequest("Email address is required for job submission".to_string()))?;

    info!("Finalizing chunked upload: {}", upload_id);
    info!("Job output formats: {:?}", output_formats);
    info!("Job quality threshold: {:?}", quality_threshold);

    // Generate job ID
    let job_id = Uuid::new_v4();

    // Create job-specific upload directory
    let job_upload_dir = state.upload_dir().join(job_id.to_string());
    tokio::fs::create_dir_all(&job_upload_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create upload directory: {}", e)))?;

    // Phase 7.1: Skip reassembly - let worker handle it to avoid HTTP timeout
    // Verify upload session exists
    let upload_session_dir = state.upload_dir().join("chunks").join(&upload_id);
    if !upload_session_dir.exists() {
        return Err(AppError::BadRequest("Upload session not found".to_string()));
    }

    info!("Upload session verified, deferring chunk reassembly to worker");

    // Create job in database with VCF format metadata
    let created_at = Utc::now();
    let metadata = serde_json::json!({
        "vcf_format": vcf_format
    });

    // PUBLIC PLATFORM: Use email as user_id (no RLS/authentication needed)
    sqlx::query(
        "INSERT INTO genetics_jobs (id, user_id, status, created_at, metadata) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(job_id)
    .bind(&user_email)
    .bind("pending")
    .bind(created_at)
    .bind(&metadata)
    .execute(state.db_pool())
    .await
    .map_err(|e| AppError::Internal(format!("Failed to create job in database: {}", e)))?;

    // Create results directory for job
    let job_results_dir = state.results_dir().join(job_id.to_string());
    tokio::fs::create_dir_all(&job_results_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create results directory: {}", e)))?;

    // Enqueue job
    let job_queue = JobQueue::new(state.redis_client().clone());
    let payload = JobPayload {
        job_id,
        user_id: user_email.clone(),  // PUBLIC PLATFORM: Email is the user identifier
        user_email: Some(user_email.clone()),  // Also store in email field for notifications
        upload_dir: job_upload_dir.to_string_lossy().to_string(),
        output_dir: job_results_dir.to_string_lossy().to_string(),
        output_formats: output_formats.clone(),
        quality_threshold,
        chunked_upload: true,  // Phase 7.1: Worker will reassemble chunks
        upload_session_id: Some(upload_id.clone()),  // Phase 7.1: For chunk reassembly
    };

    job_queue.enqueue(&payload)
        .map_err(|e| AppError::Internal(format!("Failed to enqueue job: {}", e)))?;

    info!("Job {} queued successfully (from chunked upload)", job_id);

    Ok(Json(JobSubmitResponse {
        job_id,
        status: JobStatus::Queued,
        created_at,
        estimated_completion: None,
    }))
}

// ==============================================================================
// PHASE 6: SECURE DOWNLOAD ENDPOINT
// ==============================================================================

/// Download request parameters (token from query, password from body or query)
#[derive(Debug, Clone, Deserialize)]
pub struct DownloadRequest {
    token: String,
    password: Option<String>,
}

/// Database record for job download
#[derive(Debug, sqlx::FromRow)]
struct JobDownloadRecord {
    id: Uuid,
    user_id: String,
    status: String,
    result_path: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    download_password_hash: Option<String>,
    download_attempts: i32,
    max_download_attempts: i32,
    last_download_attempt: Option<chrono::DateTime<chrono::Utc>>,
}

/// Download a completed job's results with token and password
///
/// PUBLIC PLATFORM: Token+password based download (no authentication required)
/// Security is enforced through cryptographically secure tokens and Argon2id password hashing
#[axum::debug_handler]
pub async fn download_results(
    Query(query_params): Query<DownloadRequest>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let token = query_params.token;
    let password = query_params.password
        .ok_or_else(|| AppError::BadRequest("Password required".to_string()))?;

    info!("Download attempt with token: {}...", &token[..8.min(token.len())]);

    // Get client IP for audit logging (from headers if behind proxy)
    let client_ip = "unknown".to_string(); // TODO: Extract from headers

    // Query database for job with matching token
    let job: Option<JobDownloadRecord> = sqlx::query_as(
        r#"
        SELECT id, user_id, status, result_path, expires_at,
               download_password_hash, download_attempts,
               max_download_attempts, last_download_attempt
        FROM genetics_jobs
        WHERE download_token = $1
        "#,
    )
    .bind(&token)
    .fetch_optional(state.db_pool())
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    let job = job.ok_or(AppError::NotFound)?;

    let job_id = job.id;
    info!("Download attempt for job {}", job_id);

    // PUBLIC PLATFORM: No user ownership check needed
    // Token+password provides sufficient security (only job owner has these credentials)

    // Check 1: Job must be completed
    if job.status != "completed" {
        let _ = record_download_attempt(
            state.db_pool(),
            job_id,
            &client_ip,
            "unknown",
            true,
            false,
            true,
            false,
            "job_not_found",
        ).await;
        return Err(AppError::BadRequest("Job not completed".to_string()));
    }

    // Check 2: Job must not be expired
    if let Some(expires_at) = job.expires_at {
        if expires_at < Utc::now() {
            let _ = record_download_attempt(
                state.db_pool(),
                job_id,
                &client_ip,
                "unknown",
                true,
                true,
                true,
                false,
                "job_expired",
            ).await;
            return Err(AppError::BadRequest("Download link expired".to_string()));
        }
    }

    // Check 3: Max attempts not exceeded
    if job.download_attempts >= job.max_download_attempts {
        let _ = record_download_attempt(
            state.db_pool(),
            job_id,
            &client_ip,
            "unknown",
            true,
            true,
            true,
            false,
            "max_attempts_exceeded",
        ).await;
        return Err(AppError::BadRequest("Maximum download attempts exceeded".to_string()));
    }

    // Check 4: Rate limiting (max 3 attempts per minute)
    if let Some(last_attempt) = job.last_download_attempt {
        let since_last = Utc::now().signed_duration_since(last_attempt);
        if since_last.num_seconds() < 20 {  // 20 seconds between attempts
            let _ = record_download_attempt(
                state.db_pool(),
                job_id,
                &client_ip,
                "unknown",
                true,
                true,
                true,
                false,
                "rate_limited",
            ).await;
            return Err(AppError::BadRequest("Too many attempts, please wait".to_string()));
        }
    }

    // Check 5: Verify password
    let password_hash = job.download_password_hash
        .as_ref()
        .ok_or_else(|| AppError::Internal("No password hash found".to_string()))?;

    let password_valid = verify_password(&password, password_hash)
        .map_err(|e| AppError::Internal(format!("Password verification failed: {}", e)))?;

    // Increment download attempts
    sqlx::query(
        "UPDATE genetics.genetics_jobs
         SET download_attempts = download_attempts + 1,
             last_download_attempt = NOW()
         WHERE id = $1"
    )
    .bind(job_id)
    .execute(state.db_pool())
    .await
    .map_err(|e| AppError::Internal(format!("Failed to update attempts: {}", e)))?;

    if !password_valid {
        let _ = record_download_attempt(
            state.db_pool(),
            job_id,
            &client_ip,
            "unknown",
            true,
            true,
            true,
            false,
            "invalid_password",
        ).await;
        return Err(AppError::BadRequest("Invalid password".to_string()));
    }

    // Password valid - proceed with download
    let _ = record_download_attempt(
        state.db_pool(),
        job_id,
        &client_ip,
        "unknown",
        true,
        true,
        true,
        true,
        "success",
    ).await;

    // Get result file path
    let result_path = job.result_path
        .ok_or_else(|| AppError::Internal("No result path found".to_string()))?;

    let file_path = PathBuf::from(&result_path);
    if !file_path.exists() {
        error!("Result file not found: {:?}", file_path);
        return Err(AppError::Internal("Result file not found".to_string()));
    }

    // Get file metadata for Content-Length
    let file_metadata = tokio::fs::metadata(&file_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to get file metadata: {}", e)))?;

    let file_size = file_metadata.len();

    // Read file for download
    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to open file: {}", e)))?;

    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("results.zip");

    let stream = ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/zip".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        file_size.to_string().parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", file_name)
            .parse()
            .unwrap(),
    );

    info!("Serving download for job {}: {} ({} bytes)", job_id, file_name, file_size);

    Ok((headers, body).into_response())
}

/// Record download attempt in audit table
async fn record_download_attempt(
    pool: &sqlx::PgPool,
    job_id: Uuid,
    ip_address: &str,
    user_agent: &str,
    token_provided: bool,
    token_valid: bool,
    password_provided: bool,
    password_valid: bool,
    attempt_result: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO genetics.genetics_download_attempts
            (job_id, ip_address, user_agent, token_provided, token_valid,
             password_provided, password_valid, attempt_result)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(job_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(token_provided)
    .bind(token_valid)
    .bind(password_provided)
    .bind(password_valid)
    .bind(attempt_result)
    .execute(pool)
    .await?;

    Ok(())
}

/// Application error type
#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(String),
    Forbidden,
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Resource not found".to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Access denied".to_string()),
            AppError::Internal(msg) => {
                error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        };

        let body = Json(ErrorResponse::new(error_message));
        (status, body).into_response()
    }
}
