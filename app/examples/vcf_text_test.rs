// ==============================================================================
// examples/vcf_text_test.rs - Simple text-based VCF parser test
// ==============================================================================
// Description: Test DS/R2 extraction using simple text parsing
// Author: Matt Barham
// Created: 2025-11-03
// ==============================================================================

use std::io::{BufRead, BufReader};
use std::fs::File;
use std::env;
use flate2::read::MultiGzDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get file path from command line
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <vcf_file>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run --example vcf_text_test -- /path/to/chr22.dose.vcf.gz");
        std::process::exit(1);
    }

    let vcf_path = &args[1];

    println!("Opening file: {}", vcf_path);
    let file = File::open(vcf_path)?;
    println!("File opened successfully");

    let decoder = MultiGzDecoder::new(file);
    let reader = BufReader::new(decoder);

    println!("Parsing VCF file (text mode)...\n");

    let mut count = 0;
    let mut ds_sum = 0.0;
    let mut r2_sum = 0.0;
    let mut r2_count = 0;

    let mut header_count = 0;
    let mut line_num = 0;

    for line_result in reader.lines() {
        line_num += 1;

        if line_num % 10000 == 0 {
            println!("Processing line {}...", line_num);
        }

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading line {}: {}", line_num, e);
                eprintln!("Stopping due to error.");
                break;
            }
        };

        // Skip header lines
        if line.starts_with('#') {
            header_count += 1;
            if header_count % 10 == 0 {
                println!("Read {} header lines so far...", header_count);
            }
            continue;
        }

        if count == 0 {
            println!("\nSkipped {} header lines total\n", header_count);
            println!("First data line (first 100 chars): {}\n", &line[..std::cmp::min(100, line.len())]);
        }

        count += 1;

        // Parse line
        let fields: Vec<&str> = line.split('\t').collect();

        if fields.len() < 10 {
            eprintln!("Warning: Line has only {} fields", fields.len());
            continue;
        }

        // Extract rsID (field 2)
        let rsid = fields[2];

        // Extract R2 from INFO (field 7)
        let info = fields[7];
        let mut r2 = None;
        for part in info.split(';') {
            if part.starts_with("R2=") {
                if let Ok(val) = part[3..].parse::<f64>() {
                    r2 = Some(val);
                    r2_sum += val;
                    r2_count += 1;
                }
                break;
            }
        }

        // Extract DS from last sample
        let format = fields[8];
        let format_keys: Vec<&str> = format.split(':').collect();
        let ds_index = format_keys.iter().position(|&k| k == "DS");

        if let Some(ds_idx) = ds_index {
            let last_sample = fields[fields.len() - 1];
            let sample_values: Vec<&str> = last_sample.split(':').collect();

            if ds_idx < sample_values.len() {
                if let Ok(ds) = sample_values[ds_idx].parse::<f64>() {
                    ds_sum += ds;

                    if count <= 10 {
                        println!("{:<15} DS={:.4}  R2={}", rsid, ds,
                            r2.map(|v| format!("{:.4}", v)).unwrap_or("N/A".to_string()));
                    }
                }
            }
        }

        if count == 10 {
            println!("...\n");
        }

        // Process all records (no limit for full benchmark)
    }

    println!("\nParsing complete!");
    println!("  - Total lines processed: {}", line_num);
    println!("  - Header lines: {}", header_count);
    println!("  - Data records: {}", count);
    println!("\nStatistics (first {} records):", count);
    println!("  - Mean DS: {:.4}", ds_sum / count as f64);
    println!("  - Mean R2: {:.4}", r2_sum / r2_count as f64);
    println!("  - Records with R2: {}/{}", r2_count, count);

    Ok(())
}
