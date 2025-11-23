// ==============================================================================
// state.rs - Application State Management
// ==============================================================================
// Description: Shared application state for API gateway
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use redis::Client as RedisClient;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    /// PostgreSQL connection pool
    pub db_pool: PgPool,

    /// Redis client for job queue
    pub redis_client: RedisClient,

    /// Path to encrypted volume
    pub encrypted_volume_path: PathBuf,

    /// Upload directory (within encrypted volume)
    pub upload_dir: PathBuf,

    /// Processing directory (within encrypted volume)
    pub processing_dir: PathBuf,

    /// Results directory (within encrypted volume)
    pub results_dir: PathBuf,
}

impl AppState {
    /// Create new application state from environment
    pub async fn new() -> Result<Self> {
        // Get database URL from environment
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be set")?;

        // Create PostgreSQL pool
        let db_pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;

        // Get Redis URL from environment
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        // Create Redis client
        let redis_client = RedisClient::open(redis_url)
            .context("Failed to create Redis client")?;

        // Test Redis connection
        let mut conn = redis_client.get_connection()
            .context("Failed to connect to Redis")?;
        redis::cmd("PING")
            .query::<String>(&mut conn)
            .context("Redis PING failed")?;

        // Get encrypted volume path from environment
        let encrypted_volume_path = PathBuf::from(
            std::env::var("ENCRYPTED_VOLUME_PATH")
                .unwrap_or_else(|_| "/mnt/genetics-encrypted".to_string())
        );

        // Setup directories within encrypted volume
        let upload_dir = encrypted_volume_path.join("uploads");
        let processing_dir = encrypted_volume_path.join("processing");
        let results_dir = encrypted_volume_path.join("results");

        // Create directories if they don't exist
        tokio::fs::create_dir_all(&upload_dir)
            .await
            .context("Failed to create upload directory")?;
        tokio::fs::create_dir_all(&processing_dir)
            .await
            .context("Failed to create processing directory")?;
        tokio::fs::create_dir_all(&results_dir)
            .await
            .context("Failed to create results directory")?;

        Ok(Self {
            inner: Arc::new(AppStateInner {
                db_pool,
                redis_client,
                encrypted_volume_path,
                upload_dir,
                processing_dir,
                results_dir,
            }),
        })
    }

    /// Get database pool
    pub fn db_pool(&self) -> &PgPool {
        &self.inner.db_pool
    }

    /// Get Redis client
    pub fn redis_client(&self) -> &RedisClient {
        &self.inner.redis_client
    }

    /// Get encrypted volume path
    pub fn encrypted_volume_path(&self) -> &PathBuf {
        &self.inner.encrypted_volume_path
    }

    /// Get upload directory
    pub fn upload_dir(&self) -> &PathBuf {
        &self.inner.upload_dir
    }

    /// Get processing directory
    pub fn processing_dir(&self) -> &PathBuf {
        &self.inner.processing_dir
    }

    /// Get results directory
    pub fn results_dir(&self) -> &PathBuf {
        &self.inner.results_dir
    }

    /// Create mock state for testing
    #[cfg(test)]
    pub fn mock() -> Self {
        use std::sync::OnceLock;
        static MOCK_STATE: OnceLock<AppState> = OnceLock::new();

        MOCK_STATE.get_or_init(|| {
            // This is a placeholder - in real tests you'd use test containers
            panic!("Mock state not yet implemented - use integration tests with test containers");
        }).clone()
    }
}
