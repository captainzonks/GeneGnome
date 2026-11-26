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
    /// Recovery codes (only present on initial submission, never returned again)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_codes: Option<Vec<String>>,
}

/// Public job status response (no sensitive fields exposed)
#[derive(Debug, Serialize)]
pub struct JobPublicStatusResponse {
    pub job_id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub has_download: bool,
}

/// Email resend response
#[derive(Debug, Serialize)]
pub struct ResendEmailResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_resend_available_at: Option<DateTime<Utc>>,
}

/// Delete job request (recovery code in body)
#[derive(Debug, Deserialize)]
pub struct DeleteJobRequest {
    pub recovery_code: String,
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
#[derive(Default)]
pub enum QualityThreshold {
    None, // No filtering - include all variants
    R080, // R² ≥ 0.8
    #[default]
    R090, // R² ≥ 0.9 (default, matches R script)
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
}
