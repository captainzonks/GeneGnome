// ==============================================================================
// main.rs - Genetics API Gateway Entry Point
// ==============================================================================
// Description: Axum web server for genetics data processing API
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2026-01-17
// Version: 1.1.0
// ==============================================================================

use anyhow::{Context, Result};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};
use tracing::{info, Level};

mod handlers;
mod middleware;
mod models;
mod queue;
mod security;
mod state;
mod validator;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let server_port = 8099;
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .init();

    info!("Starting Genetics API Gateway v1.0.0");

    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize application state
    let state = AppState::new()
        .await
        .context("Failed to initialize application state")?;

    // Build router with all endpoints
    let app = build_router(state);

    // Bind server
    let addr = SocketAddr::from(([0, 0, 0, 0], server_port));
    info!("API Gateway listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind to address")?;

    // Run server
    axum::serve(listener, app)
        .await
        .context("Server error")?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    // API routes
    let api_routes = Router::new()
        // Job submission (file upload)
        .route("/jobs", post(handlers::submit_job))
        // Job status
        .route("/jobs/{job_id}", get(handlers::get_job_status))
        // Job deletion
        .route("/jobs/{job_id}", delete(handlers::delete_job))
        // WebSocket progress updates
        .route("/jobs/{job_id}/ws", get(handlers::job_progress_ws))
        // Download results (Phase 6: secure token-based download with password)
        .route("/download", get(handlers::download_results))
        // Chunked upload endpoints (for files >50MB, Cloudflare bypass)
        .route("/upload/chunks", post(handlers::upload_chunk))
        .route("/upload/finalize", post(handlers::finalize_upload))
        // Health checks (nested under /api/genetics for consistency)
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::readiness_check));

    // Configure CORS for public genetics platform
    // Origins are configured via CORS_ALLOWED_ORIGINS env var (comma-separated)
    // Example: CORS_ALLOWED_ORIGINS=https://genetics.example.com,https://test.example.com
    let cors_origins = std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    let allowed_origins: Vec<_> = cors_origins
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let cors = CorsLayer::new()
        // Allow specific origins from environment
        .allow_origin(AllowOrigin::list(allowed_origins))
        // PUBLIC PLATFORM: No credentials needed (token+password in request body/query)
        .allow_credentials(false)
        // Allow standard methods
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        // Allow standard headers (no authentication headers needed)
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
        ])
        // Expose headers for client access
        .expose_headers([header::CONTENT_TYPE]);

    // Combine all routes
    Router::new()
        .route("/", get(handlers::root))
        .nest("/api/genetics", api_routes)
        .layer(
            ServiceBuilder::new()
                // Request tracing
                .layer(TraceLayer::new_for_http())
                // CORS for public platform (no authentication required)
                .layer(cors)
                // Request body size limit (500MB for large VCF files)
                .layer(DefaultBodyLimit::max(500 * 1024 * 1024)),
        )
        .with_state(state)
}

// Timeout error handler
async fn handle_timeout_error(err: tower::BoxError) -> impl IntoResponse {
    if err.is::<tower::timeout::error::Elapsed>() {
        (StatusCode::REQUEST_TIMEOUT, "Request timed out".to_string())
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unhandled error: {}", err),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_builds() {
        // Smoke test to ensure router compiles
        let state = AppState::mock();
        let _router = build_router(state);
    }
}
