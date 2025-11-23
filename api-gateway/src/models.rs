// ==============================================================================
// models.rs - API Data Models
// ==============================================================================
// Description: Request/response models for genetics API
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Job submission response
#[derive(Debug, Serialize)]
pub struct JobSubmitResponse {
    pub job_id: Uuid,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub estimated_completion: Option<DateTime<Utc>>,
}

/// Job status response
#[derive(Debug, Serialize)]
pub struct JobStatusResponse {
    pub job_id: Uuid,
    pub user_id: String,
    pub status: JobStatus,
    pub progress: f32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub output_formats: Vec<String>,
    pub files: JobFiles,
}

/// Job files information
#[derive(Debug, Serialize, Deserialize)]
pub struct JobFiles {
    pub genome_file: String,
    pub vcf_files: Vec<String>,
    pub pgs_file: String,
}

/// Job status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Processing,
    Complete,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Processing => "processing",
            JobStatus::Complete => "complete",
            JobStatus::Failed => "failed",
        }
    }
}

/// Output format selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    // Json, // DISABLED: 29GB JSON file causes OOM. Users can convert from SQLite/Parquet/VCF
    Parquet,
    Sqlite,
    Vcf,
}

/// Quality threshold for imputation filtering (must match worker queue.rs)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QualityThreshold {
    None,       // No filtering - include all variants
    R080,       // R² ≥ 0.8
    R090,       // R² ≥ 0.9 (default, matches R script)
}

impl Default for QualityThreshold {
    fn default() -> Self {
        QualityThreshold::R090  // Default to R² ≥ 0.9 to match R script behavior
    }
}

impl OutputFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Parquet => "parquet",
            OutputFormat::Sqlite => "db",
            OutputFormat::Vcf => "vcf.gz",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Parquet => "application/octet-stream",
            OutputFormat::Sqlite => "application/octet-stream",
            OutputFormat::Vcf => "application/gzip",
        }
    }
}

/// Job creation request (from multipart form)
#[derive(Debug)]
pub struct JobCreateRequest {
    pub user_id: String,
    pub genome_file: Vec<u8>,
    pub genome_filename: String,
    pub vcf_files: Vec<(String, Vec<u8>)>,
    pub pgs_file: Vec<u8>,
    pub pgs_filename: String,
    pub output_formats: Vec<OutputFormat>,
    pub quality_threshold: QualityThreshold,
}

/// Progress update message (for WebSocket)
#[derive(Debug, Serialize)]
pub struct ProgressUpdate {
    pub job_id: Uuid,
    pub progress_pct: f32,
    pub current_step: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

/// API information response
#[derive(Debug, Serialize)]
pub struct ApiInfoResponse {
    pub service: &'static str,
    pub version: &'static str,
    pub endpoints: Vec<&'static str>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub timestamp: DateTime<Utc>,
}

/// Readiness check response
#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub database: bool,
    pub redis: bool,
    pub encrypted_volume: bool,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: None,
        }
    }

    pub fn with_details(error: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: Some(details.into()),
        }
    }
}
