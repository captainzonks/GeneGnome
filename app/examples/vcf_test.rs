// ==============================================================================
// examples/vcf_test.rs - VCF Parser Test
// ==============================================================================
// Description: Test harness for VCF parser prototype
// Author: Matthew Barham
// Created: 2025-11-03
// ==============================================================================
// Usage:
//   cargo run --example vcf_test -- /path/to/file.vcf.gz
// ==============================================================================

use genetics_processor::parsers::VCFParser;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get file path from command line
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <vcf_file>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run --example vcf_test -- /path/to/chr22.dose.vcf.gz");
        std::process::exit(1);
    }

    let vcf_path = &args[1];

    println!("{}", "=".repeat(80));
    println!("VCF Parser Prototype Test");
    println!("{}", "=".repeat(80));
    println!("File: {}", vcf_path);
    println!();

    // Create parser with quality filter
    let mut parser = VCFParser::new()
        .with_min_quality(0.3)   // Filter SNPs with DR2 < 0.3
        .with_max_errors(100);   // Fail if >100 bad records

    println!("Parser configuration:");
    println!("  - Minimum quality (DR2): {}", parser.min_quality);
    println!("  - Maximum errors: {}", parser.max_errors);
    println!();

    // Parse VCF file
    println!("Parsing VCF file...");
    let start = std::time::Instant::now();

    match parser.parse(vcf_path) {
        Ok(records) => {
            let elapsed = start.elapsed();

            println!();
            println!("{}", "=".repeat(80));
            println!("Parsing completed successfully!");
            println!("{}", "=".repeat(80));
            println!();

            println!("Statistics:");
            println!("  - Total records parsed: {}", records.len());
            println!("  - Records skipped (quality): {}", parser.skipped_count);
            println!("  - Errors encountered: {}", parser.error_count);
            println!("  - Parse time: {:.2}s", elapsed.as_secs_f64());
            println!("  - Records/sec: {:.0}", records.len() as f64 / elapsed.as_secs_f64());
            println!();

            // Show first 10 records as sample
            if !records.is_empty() {
                println!("Sample records (first 10):");
                println!("{}", "-".repeat(80));
                println!("{:<15} {:<6} {:<12} {:<4} {:<4} {:<8} {:<6}",
                    "rsID", "Chr", "Position", "REF", "ALT", "Dosage", "DR2");
                println!("{}", "-".repeat(80));

                for (i, record) in records.iter().take(10).enumerate() {
                    let dr2_str = record.imputation_quality
                        .map(|q| format!("{:.4}", q))
                        .unwrap_or_else(|| "N/A".to_string());

                    println!("{:<15} {:<6} {:<12} {:<4} {:<4} {:<8.4} {:<6}",
                        if record.rsid.len() > 14 {
                            format!("{}...", &record.rsid[..11])
                        } else {
                            record.rsid.clone()
                        },
                        record.chromosome,
                        record.position,
                        record.ref_allele,
                        record.alt_allele,
                        record.dosage,
                        dr2_str
                    );

                    if i == 9 && records.len() > 10 {
                        println!("  ... and {} more records", records.len() - 10);
                    }
                }
                println!("{}", "-".repeat(80));
            }

            // Quality distribution (if available)
            let with_quality: Vec<_> = records.iter()
                .filter_map(|r| r.imputation_quality)
                .collect();

            if !with_quality.is_empty() {
                let min_q = with_quality.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let max_q = with_quality.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                let mean_q = with_quality.iter().sum::<f64>() / with_quality.len() as f64;

                println!();
                println!("Imputation Quality (DR2) Distribution:");
                println!("  - Min: {:.4}", min_q);
                println!("  - Max: {:.4}", max_q);
                println!("  - Mean: {:.4}", mean_q);
                println!("  - SNPs with DR2: {} / {} ({:.1}%)",
                    with_quality.len(),
                    records.len(),
                    100.0 * with_quality.len() as f64 / records.len() as f64
                );
            }

            println!();
            println!("Test completed successfully!");

            Ok(())
        }
        Err(e) => {
            eprintln!();
            eprintln!("{}", "=".repeat(80));
            eprintln!("ERROR: Parsing failed!");
            eprintln!("{}", "=".repeat(80));
            eprintln!("{}", e);
            eprintln!();
            eprintln!("Statistics before failure:");
            eprintln!("  - Records skipped: {}", parser.skipped_count);
            eprintln!("  - Errors encountered: {}", parser.error_count);

            Err(e.into())
        }
    }
}
