use std::io::{BufRead, BufReader};
use std::fs::File;
use std::env;
use flate2::read::MultiGzDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <vcf_file>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run --example minimal_gz_test -- /path/to/chr22.dose.vcf.gz");
        std::process::exit(1);
    }

    let vcf_path = &args[1];

    println!("Opening gzipped file: {}", vcf_path);
    let file = File::open(vcf_path)?;
    let decoder = MultiGzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let mut count = 0;
    for line_result in reader.lines() {
        match line_result {
            Ok(_) => count += 1,
            Err(e) => {
                eprintln!("Error at line {}: {}", count + 1, e);
                break;
            }
        }

        if count % 50000 == 0 {
            println!("Read {} lines so far...", count);
        }
    }

    println!("\nTotal lines read: {}", count);
    println!("Expected: 152207");

    Ok(())
}
