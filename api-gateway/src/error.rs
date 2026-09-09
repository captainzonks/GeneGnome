// ==============================================================================
// error.rs - Shared API Error Type
// ==============================================================================
// Description: Application error type shared by handlers.rs and db.rs
// Author: Matt Barham
// Created: 2026-09-08
// Modified: 2026-09-08
// Version: 1.0.0
// ==============================================================================

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use tracing::error;

use crate::models::ErrorResponse;

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
