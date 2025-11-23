use noodles_vcf as vcf;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <vcf_file>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run --example noodles_debug_test -- /path/to/chr22.dose.vcf.gz");
        std::process::exit(1);
    }

    let vcf_path = &args[1];

    let mut reader = vcf::io::reader::Builder::default()
        .build_from_path(vcf_path)?;

    let _header = reader.read_header()?;

    if let Some(Ok(record)) = reader.records().next() {
        let debug_str = format!("{:?}", record);
        println!("Debug output length: {}", debug_str.len());
        println!("\nFirst 500 chars:\n{}", &debug_str[..std::cmp::min(500, debug_str.len())]);
        println!("\n---");

        // Try to find various patterns
        if let Some(pos) = debug_str.find("RecordBuf") {
            println!("\nFound 'RecordBuf' at position: {}", pos);
        } else {
            println!("\n'RecordBuf' NOT found");
        }

        if let Some(pos) = debug_str.find("Record") {
            println!("Found 'Record' at position: {}", pos);
            println!("Context: {}", &debug_str[pos..std::cmp::min(pos+100, debug_str.len())]);
        }
    }

    Ok(())
}
