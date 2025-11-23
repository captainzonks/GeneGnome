// ==============================================================================
// pgs_parser_example.rs - Example of PGS Parser Usage
// ==============================================================================
// Description: Demonstrates parsing PGS scores with z-score normalization
// Author: Matthew Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use genetics_processor::parsers::pgs::{PgsParser, PgsRecord};
use std::io::Write;
use tempfile::NamedTempFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PGS Parser Example ===\n");

    // Create a temporary CSV file with sample PGS data
    let mut temp_file = NamedTempFile::new()?;
    writeln!(temp_file, "ID,PGS_label,score_value")?;
    writeln!(temp_file, "user1,Height,165.5")?;
    writeln!(temp_file, "user1,BMI,22.3")?;
    writeln!(temp_file, "user1,Intelligence,105.0")?;
    writeln!(temp_file, "user2,Height,180.2")?;
    writeln!(temp_file, "user2,BMI,25.1")?;
    writeln!(temp_file, "user2,Intelligence,115.5")?;
    writeln!(temp_file, "user3,Height,172.8")?;
    writeln!(temp_file, "user3,BMI,23.7")?;
    writeln!(temp_file, "user3,Intelligence,110.2")?;
    temp_file.flush()?;

    println!("Created temporary PGS file: {:?}\n", temp_file.path());

    // Parse the PGS file
    println!("Parsing PGS file...");
    let dataset = PgsParser::parse(temp_file.path())?;

    println!("✓ Successfully parsed {} records\n", dataset.unscaled.len());

    // Display unscaled values
    println!("--- Unscaled PGS Values ---");
    println!("{:<8} {:<15} {:>12}", "ID", "PGS Label", "Value");
    println!("{:-<40}", "");
    for record in &dataset.unscaled {
        println!(
            "{:<8} {:<15} {:>12.4}",
            record.sample_id, record.label, record.value
        );
    }

    // Display statistics per label
    println!("\n--- PGS Statistics (Unscaled) ---");
    let labels = ["Height", "BMI", "Intelligence"];
    for label in labels {
        if let Some(stats) = PgsParser::get_stats(&dataset.unscaled, label) {
            println!(
                "{}: n={}, mean={:.4}, sd={:.4}, range=[{:.4}, {:.4}]",
                stats.label, stats.count, stats.mean, stats.std_dev, stats.min, stats.max
            );
        }
    }

    // Display scaled values (z-scores)
    println!("\n--- Scaled PGS Values (Z-scores) ---");
    println!("{:<8} {:<15} {:>12}", "ID", "PGS Label", "Z-score");
    println!("{:-<40}", "");
    for record in &dataset.scaled {
        println!(
            "{:<8} {:<15} {:>12.4}",
            record.sample_id, record.label, record.value
        );
    }

    // Verify z-score properties
    println!("\n--- Z-score Verification ---");
    for label in labels {
        let scaled_values: Vec<f64> = dataset
            .scaled
            .iter()
            .filter(|r| r.label == label)
            .map(|r| r.value)
            .collect();

        let mean: f64 = scaled_values.iter().sum::<f64>() / scaled_values.len() as f64;
        let variance: f64 = scaled_values
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / scaled_values.len() as f64;
        let std_dev = variance.sqrt();

        println!(
            "{}: scaled_mean={:.6}, scaled_sd={:.6}",
            label, mean, std_dev
        );
        println!("  ✓ Mean ≈ 0: {}", mean.abs() < 1e-10);
        println!("  ✓ SD ≈ 1: {}", (std_dev - 1.0).abs() < 1e-10);
    }

    println!("\n=== Example Complete ===");

    Ok(())
}
