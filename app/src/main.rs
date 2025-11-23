// ==============================================================================
// main.rs - Genetics Processor Entry Point
// ==============================================================================
// Description: Main entry point for secure genetic data processing service
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2025-10-31
// Version: 1.0.0
// ==============================================================================

use anyhow::Result;
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod processor;
mod validator;
mod secure_delete;
mod audit;
mod parsers;
mod genotype_converter;
mod models;
mod reference_panel;
mod output;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Job ID to process
    #[arg(short, long)]
    job_id: uuid::Uuid,

    /// User ID (owner of the job)
    #[arg(short, long)]
    user_id: String,

    /// Data directory path
    #[arg(short, long, default_value = "/data/genetics")]
    data_dir: String,

    /// Reference panel path
    #[arg(short, long, default_value = "/reference/VCF.Files3.RData")]
    reference: String,

    /// Database URL (or use DATABASE_URL_FILE env var)
    #[arg(long, env)]
    database_url: Option<String>,

    /// Quality threshold for filtering (r08, r09, or no-filter)
    #[arg(long, default_value = "r09")]
    quality_threshold: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "genetics_processor=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Genetics Processor starting...");

    // Parse command line arguments
    let args = Args::parse();

    // Load database URL from file if DATABASE_URL_FILE is set
    let database_url = if let Some(url) = args.database_url {
        url
    } else if let Ok(file_path) = std::env::var("DATABASE_URL_FILE") {
        std::fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read DATABASE_URL_FILE: {}", e))?
            .trim()
            .to_string()
    } else {
        anyhow::bail!("DATABASE_URL or DATABASE_URL_FILE must be provided");
    };

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    info!("Connected to database");

    // Parse quality threshold
    let quality_threshold = match args.quality_threshold.to_lowercase().as_str() {
        "r08" => models::QualityThreshold::R08,
        "r09" => models::QualityThreshold::R09,
        "no-filter" | "nofilter" => models::QualityThreshold::NoFilter,
        _ => {
            warn!("Invalid quality threshold '{}', using R09", args.quality_threshold);
            models::QualityThreshold::R09
        }
    };

    // Create processor
    let processor = processor::GeneticsProcessor::new(
        args.job_id,
        args.user_id.clone(),
        args.data_dir.into(),
        args.reference.into(),
        pool.clone(),
        quality_threshold,
    );

    // Audit: Job started
    audit::log_event(
        &pool,
        audit::AuditEventType::JobStarted,
        &args.user_id,
        Some(args.job_id.to_string()),
        serde_json::json!({
            "job_id": args.job_id,
            "user_id": args.user_id,
        }),
    )
    .await?;

    // Process genetic data
    match processor.process().await {
        Ok(result_path) => {
            info!("Processing completed successfully: {:?}", result_path);

            // Audit: Job completed
            audit::log_event(
                &pool,
                audit::AuditEventType::JobCompleted,
                &args.user_id,
                Some(args.job_id.to_string()),
                serde_json::json!({
                    "job_id": args.job_id,
                    "result_path": result_path.to_str(),
                    "success": true,
                }),
            )
            .await?;

            Ok(())
        }
        Err(e) => {
            warn!("Processing failed: {}", e);

            // Audit: Job failed
            audit::log_event(
                &pool,
                audit::AuditEventType::JobFailed,
                &args.user_id,
                Some(args.job_id.to_string()),
                serde_json::json!({
                    "job_id": args.job_id,
                    "error": e.to_string(),
                    "success": false,
                }),
            )
            .await?;

            Err(e)
        }
    }
}
