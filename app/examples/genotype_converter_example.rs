// ==============================================================================
// genotype_converter_example.rs - Genotype to Dosage Conversion Example
// ==============================================================================
// Description: Demonstrates converting 23andMe genotypes to VCF dosages
// Author: Matthew Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use genetics_processor::genotype_converter::{
    genotype_to_dosage, genotype_to_dosage_with_flip, batch_convert_genotypes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Genotype to Dosage Conversion Example ===\n");

    // Example 1: Basic conversions
    println!("--- Example 1: Basic Conversions ---");
    println!("Converting 23andMe genotypes to dosage values (0.0-2.0)\n");

    let conversions = vec![
        ("TT", "T", "C", "Homozygous Reference"),
        ("TC", "T", "C", "Heterozygous (REF/ALT)"),
        ("CT", "T", "C", "Heterozygous (ALT/REF)"),
        ("CC", "T", "C", "Homozygous Alternate"),
        ("--", "T", "C", "No Call (missing)"),
    ];

    println!("{:<12} {:<6} {:<6} {:<10} {}", "Genotype", "REF", "ALT", "Dosage", "Type");
    println!("{:-<60}", "");

    for (genotype, ref_allele, alt_allele, description) in conversions {
        match genotype_to_dosage(genotype, ref_allele, alt_allele)? {
            Some(dosage) => {
                println!(
                    "{:<12} {:<6} {:<6} {:<10} {}",
                    genotype, ref_allele, alt_allele, dosage, description
                );
            }
            None => {
                println!(
                    "{:<12} {:<6} {:<6} {:<10} {}",
                    genotype, ref_allele, alt_allele, "None", description
                );
            }
        }
    }

    // Example 2: All nucleotide combinations
    println!("\n--- Example 2: All Nucleotide Combinations ---");
    println!("Testing all A/T and C/G combinations\n");

    let nucleotide_tests = vec![
        // A/T combinations
        ("AA", "A", "T"),
        ("AT", "A", "T"),
        ("TA", "A", "T"),
        ("TT", "A", "T"),
        // C/G combinations
        ("CC", "C", "G"),
        ("CG", "C", "G"),
        ("GC", "C", "G"),
        ("GG", "C", "G"),
    ];

    for (genotype, ref_allele, alt_allele) in nucleotide_tests {
        let dosage = genotype_to_dosage(genotype, ref_allele, alt_allele)?;
        println!(
            "  {} with REF={}, ALT={} → dosage = {}",
            genotype,
            ref_allele,
            alt_allele,
            dosage.unwrap()
        );
    }

    // Example 3: Error handling
    println!("\n--- Example 3: Error Handling ---");
    println!("Demonstrating allele mismatch detection\n");

    let error_cases = vec![
        ("TG", "T", "C", "Allele G doesn't match REF=T or ALT=C"),
        ("AG", "T", "C", "Neither allele matches REF=T or ALT=C"),
        ("T", "T", "C", "Invalid format (1 character)"),
        ("TTC", "T", "C", "Invalid format (3 characters)"),
    ];

    for (genotype, ref_allele, alt_allele, description) in error_cases {
        match genotype_to_dosage(genotype, ref_allele, alt_allele) {
            Ok(_) => println!("  ✗ {} - Expected error but got success", genotype),
            Err(e) => println!("  ✓ {} - {}: {}", genotype, description, e),
        }
    }

    // Example 4: Strand flipping
    println!("\n--- Example 4: Strand Flipping ---");
    println!("Handling genotypes on the opposite strand\n");

    let flip_cases = vec![
        ("AT", "T", "A", "Flips to TA"),
        ("CG", "G", "C", "Flips to GC"),
        ("AC", "T", "G", "Flips to TG"),
    ];

    for (genotype, ref_allele, alt_allele, description) in flip_cases {
        // Try without flip (should fail)
        let direct_result = genotype_to_dosage(genotype, ref_allele, alt_allele);

        // Try with flip (should succeed)
        let flip_result = genotype_to_dosage_with_flip(genotype, ref_allele, alt_allele)?;

        println!(
            "  {} with REF={}, ALT={} - {}",
            genotype, ref_allele, alt_allele, description
        );
        println!(
            "    Direct: {}",
            if direct_result.is_err() {
                "Failed (as expected)"
            } else {
                "Succeeded"
            }
        );
        println!(
            "    With flip: dosage = {}",
            flip_result.unwrap_or(-1.0)
        );
    }

    // Example 5: Batch conversion
    println!("\n--- Example 5: Batch Conversion ---");
    println!("Converting multiple genotypes at once\n");

    let batch = vec![
        ("TT", "T", "C"),
        ("TC", "T", "C"),
        ("CC", "T", "C"),
        ("--", "T", "C"),
        ("AA", "A", "G"),
        ("AG", "A", "G"),
    ];

    let results = batch_convert_genotypes(batch.clone());

    println!("{:<12} {:<6} {:<6} {:<10}", "Genotype", "REF", "ALT", "Dosage");
    println!("{:-<40}", "");

    for (i, (genotype, ref_allele, alt_allele)) in batch.iter().enumerate() {
        match &results[i] {
            Ok(Some(dosage)) => {
                println!(
                    "{:<12} {:<6} {:<6} {:<10}",
                    genotype, ref_allele, alt_allele, dosage
                );
            }
            Ok(None) => {
                println!(
                    "{:<12} {:<6} {:<6} {:<10}",
                    genotype, ref_allele, alt_allele, "None"
                );
            }
            Err(e) => {
                println!("{:<12} {:<6} {:<6} Error: {}", genotype, ref_allele, alt_allele, e);
            }
        }
    }

    // Example 6: Realistic scenario
    println!("\n--- Example 6: Realistic VCF Merging Scenario ---");
    println!("Simulating merging 23andMe data with VCF imputed data\n");

    // Simulated VCF records at specific positions
    let vcf_records = vec![
        (69869, "rs548049170", "T", "C", 1.95), // Imputed dosage
        (74792, "rs13328684", "A", "G", 0.05),
        (565508, "rs9283150", "A", "G", 1.50),
    ];

    // Simulated 23andMe genotypes at the same positions
    let genotypes_23andme = vec![
        (69869, "TT"), // User has homozygous ref
        (74792, "--"), // Missing - use imputed
        (565508, "AG"), // User has heterozygous
    ];

    println!("{:<10} {:<15} {:<6} {:<6} {:<15} {:<10}",
             "Position", "rsID", "REF", "ALT", "23andMe GT", "Final Dosage");
    println!("{:-<80}", "");

    for (vcf_pos, rsid, ref_a, alt_a, imputed_dosage) in vcf_records {
        // Find matching 23andMe genotype
        let gt_23andme = genotypes_23andme
            .iter()
            .find(|(pos, _)| *pos == vcf_pos)
            .map(|(_, gt)| *gt);

        let final_dosage = if let Some(gt) = gt_23andme {
            match genotype_to_dosage(gt, ref_a, alt_a)? {
                Some(dosage) => {
                    println!(
                        "{:<10} {:<15} {:<6} {:<6} {:<15} {:<10} ← from genotype",
                        vcf_pos, rsid, ref_a, alt_a, gt, dosage
                    );
                    dosage
                }
                None => {
                    println!(
                        "{:<10} {:<15} {:<6} {:<6} {:<15} {:<10} ← from VCF (imputed)",
                        vcf_pos, rsid, ref_a, alt_a, gt, imputed_dosage
                    );
                    imputed_dosage
                }
            }
        } else {
            println!(
                "{:<10} {:<15} {:<6} {:<6} {:<15} {:<10} ← from VCF (imputed)",
                vcf_pos, rsid, ref_a, alt_a, "not in 23andMe", imputed_dosage
            );
            imputed_dosage
        };

        let _ = final_dosage; // Use the variable
    }

    println!("\n=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("  • Genotyped data (23andMe) takes priority over imputed data");
    println!("  • Missing genotypes (--) use imputed dosages from VCF");
    println!("  • Dosage scale: 0.0=REF/REF, 1.0=REF/ALT, 2.0=ALT/ALT");
    println!("  • Strand flipping can resolve some allele mismatches");
    println!("  • Batch conversion is efficient for large datasets");

    Ok(())
}
