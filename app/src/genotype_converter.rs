// ==============================================================================
// genotype_converter.rs - Genotype to Dosage Conversion
// ==============================================================================
// Description: Converts 23andMe genotypes to allele dosage values for VCF merging
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================
// Algorithm:
//   Given REF allele and ALT allele from VCF:
//   - REF/REF (e.g., TT where REF=T) → 0.0 (no ALT alleles)
//   - REF/ALT or ALT/REF (e.g., AG where REF=A, ALT=G) → 1.0 (one ALT allele)
//   - ALT/ALT (e.g., GG where ALT=G) → 2.0 (two ALT alleles)
//   - --/-- (no call) → None (use imputed dosage from VCF)
// ==============================================================================

use thiserror::Error;

/// Errors that can occur during genotype conversion
#[derive(Error, Debug, Clone, PartialEq)]
pub enum GenotypeConversionError {
    #[error("Invalid genotype format: '{0}' (expected 2 characters)")]
    InvalidFormat(String),

    #[error("Genotype '{genotype}' does not match REF '{ref_allele}' or ALT '{alt_allele}' alleles")]
    AllelesMismatch {
        genotype: String,
        ref_allele: String,
        alt_allele: String,
    },

    #[error("Multi-allelic site detected: genotype '{genotype}' with REF '{ref_allele}', ALT '{alt_allele}'")]
    MultiAllelicSite {
        genotype: String,
        ref_allele: String,
        alt_allele: String,
    },
}

/// Convert 23andMe genotype to dosage given REF and ALT alleles
///
/// # Arguments
/// * `genotype` - Two-character genotype string (e.g., "TT", "AG", "--")
/// * `ref_allele` - Reference allele from VCF (e.g., "T")
/// * `alt_allele` - Alternate allele from VCF (e.g., "C")
///
/// # Returns
/// * `Ok(Some(dosage))` - Successfully converted to dosage (0.0, 1.0, or 2.0)
/// * `Ok(None)` - Missing genotype ("--"), use imputed dosage
/// * `Err(GenotypeConversionError)` - Invalid genotype or allele mismatch
///
/// # Dosage Scale
/// - 0.0 = Homozygous reference (REF/REF)
/// - 1.0 = Heterozygous (REF/ALT or ALT/REF)
/// - 2.0 = Homozygous alternate (ALT/ALT)
///
/// # Examples
/// ```
/// use genetics_processor::genotype_converter::genotype_to_dosage;
///
/// // Homozygous reference
/// assert_eq!(genotype_to_dosage("TT", "T", "C").unwrap(), Some(0.0));
///
/// // Heterozygous
/// assert_eq!(genotype_to_dosage("TC", "T", "C").unwrap(), Some(1.0));
/// assert_eq!(genotype_to_dosage("CT", "T", "C").unwrap(), Some(1.0));
///
/// // Homozygous alternate
/// assert_eq!(genotype_to_dosage("CC", "T", "C").unwrap(), Some(2.0));
///
/// // Missing genotype - use imputed
/// assert_eq!(genotype_to_dosage("--", "T", "C").unwrap(), None);
/// ```
pub fn genotype_to_dosage(
    genotype: &str,
    ref_allele: &str,
    alt_allele: &str,
) -> Result<Option<f64>, GenotypeConversionError> {
    // Handle missing genotype (no-call)
    if genotype == "--" || genotype.is_empty() {
        return Ok(None); // Use imputed dosage
    }

    // Validate genotype format (must be exactly 2 characters)
    if genotype.len() != 2 {
        return Err(GenotypeConversionError::InvalidFormat(genotype.to_string()));
    }

    // Validate that REF and ALT are single characters (SNPs only)
    // 23andMe genotypes are 2 characters and cannot represent indels properly
    // For indels, we should return an error to fall back to imputed dosage
    if ref_allele.len() != 1 || alt_allele.len() != 1 {
        return Err(GenotypeConversionError::AllelesMismatch {
            genotype: genotype.to_string(),
            ref_allele: ref_allele.to_string(),
            alt_allele: alt_allele.to_string(),
        });
    }

    // Extract two alleles from genotype
    let mut chars = genotype.chars();
    let allele1 = chars.next().unwrap().to_string();
    let allele2 = chars.next().unwrap().to_string();

    // Count ALT alleles
    let mut alt_count = 0;
    let mut ref_count = 0;

    // Check allele1
    if allele1 == alt_allele {
        alt_count += 1;
    } else if allele1 == ref_allele {
        ref_count += 1;
    } else {
        // Allele doesn't match REF or ALT (possible multi-allelic site or strand issue)
        return Err(GenotypeConversionError::AllelesMismatch {
            genotype: genotype.to_string(),
            ref_allele: ref_allele.to_string(),
            alt_allele: alt_allele.to_string(),
        });
    }

    // Check allele2
    if allele2 == alt_allele {
        alt_count += 1;
    } else if allele2 == ref_allele {
        ref_count += 1;
    } else {
        // Allele doesn't match REF or ALT
        return Err(GenotypeConversionError::AllelesMismatch {
            genotype: genotype.to_string(),
            ref_allele: ref_allele.to_string(),
            alt_allele: alt_allele.to_string(),
        });
    }

    // Validate: should have exactly 2 alleles total
    if alt_count + ref_count != 2 {
        return Err(GenotypeConversionError::MultiAllelicSite {
            genotype: genotype.to_string(),
            ref_allele: ref_allele.to_string(),
            alt_allele: alt_allele.to_string(),
        });
    }

    // Return dosage (count of ALT alleles)
    Ok(Some(alt_count as f64))
}

/// Convert genotype to dosage with strand flipping support
///
/// This function attempts to convert the genotype, and if it fails due to
/// allele mismatch, it tries the reverse complement (strand flip).
///
/// # Arguments
/// * `genotype` - Two-character genotype string
/// * `ref_allele` - Reference allele from VCF
/// * `alt_allele` - Alternate allele from VCF
///
/// # Returns
/// * `Ok(Some(dosage))` - Successfully converted
/// * `Ok(None)` - Missing genotype
/// * `Err(GenotypeConversionError)` - Cannot convert even with strand flip
pub fn genotype_to_dosage_with_flip(
    genotype: &str,
    ref_allele: &str,
    alt_allele: &str,
) -> Result<Option<f64>, GenotypeConversionError> {
    // Try direct conversion first
    match genotype_to_dosage(genotype, ref_allele, alt_allele) {
        Ok(result) => Ok(result),
        Err(GenotypeConversionError::AllelesMismatch { .. }) => {
            // Try reverse complement (strand flip)
            let flipped_genotype = flip_strand(genotype);
            genotype_to_dosage(&flipped_genotype, ref_allele, alt_allele)
        }
        Err(e) => Err(e),
    }
}

/// Flip genotype to reverse complement (strand flip)
///
/// # Mapping
/// - A ↔ T
/// - C ↔ G
/// - G ↔ C
/// - T ↔ A
///
/// # Arguments
/// * `genotype` - Two-character genotype string
///
/// # Returns
/// * Reverse complement genotype string
fn flip_strand(genotype: &str) -> String {
    genotype
        .chars()
        .map(|c| match c {
            'A' => 'T',
            'T' => 'A',
            'C' => 'G',
            'G' => 'C',
            '-' => '-',
            _ => c, // Unknown character, keep as-is
        })
        .collect()
}

/// Batch convert multiple genotypes to dosages
///
/// This is useful for converting all genotypes at positions that match VCF records.
///
/// # Arguments
/// * `genotypes` - Vector of (genotype, ref_allele, alt_allele) tuples
///
/// # Returns
/// * Vector of conversion results
pub fn batch_convert_genotypes(
    genotypes: Vec<(&str, &str, &str)>,
) -> Vec<Result<Option<f64>, GenotypeConversionError>> {
    genotypes
        .into_iter()
        .map(|(gt, ref_a, alt_a)| genotype_to_dosage(gt, ref_a, alt_a))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_homozygous_reference() {
        // TT with REF=T, ALT=C → 0.0
        assert_eq!(genotype_to_dosage("TT", "T", "C").unwrap(), Some(0.0));
        assert_eq!(genotype_to_dosage("AA", "A", "G").unwrap(), Some(0.0));
        assert_eq!(genotype_to_dosage("GG", "G", "A").unwrap(), Some(0.0));
        assert_eq!(genotype_to_dosage("CC", "C", "T").unwrap(), Some(0.0));
    }

    #[test]
    fn test_heterozygous() {
        // TC with REF=T, ALT=C → 1.0
        assert_eq!(genotype_to_dosage("TC", "T", "C").unwrap(), Some(1.0));
        // CT with REF=T, ALT=C → 1.0 (order doesn't matter)
        assert_eq!(genotype_to_dosage("CT", "T", "C").unwrap(), Some(1.0));

        // Other combinations
        assert_eq!(genotype_to_dosage("AG", "A", "G").unwrap(), Some(1.0));
        assert_eq!(genotype_to_dosage("GA", "A", "G").unwrap(), Some(1.0));
    }

    #[test]
    fn test_homozygous_alternate() {
        // CC with REF=T, ALT=C → 2.0
        assert_eq!(genotype_to_dosage("CC", "T", "C").unwrap(), Some(2.0));
        assert_eq!(genotype_to_dosage("GG", "A", "G").unwrap(), Some(2.0));
        assert_eq!(genotype_to_dosage("AA", "T", "A").unwrap(), Some(2.0));
        assert_eq!(genotype_to_dosage("TT", "C", "T").unwrap(), Some(2.0));
    }

    #[test]
    fn test_missing_genotype() {
        // -- (no call) → None (use imputed)
        assert_eq!(genotype_to_dosage("--", "T", "C").unwrap(), None);
        assert_eq!(genotype_to_dosage("", "T", "C").unwrap(), None);
    }

    #[test]
    fn test_invalid_format() {
        // Single character
        let result = genotype_to_dosage("T", "T", "C");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::InvalidFormat(_))
        ));

        // Three characters
        let result = genotype_to_dosage("TTC", "T", "C");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_allele_mismatch() {
        // Genotype TG doesn't match REF=T, ALT=C
        let result = genotype_to_dosage("TG", "T", "C");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::AllelesMismatch { .. })
        ));

        // Genotype AG doesn't match REF=T, ALT=C
        let result = genotype_to_dosage("AG", "T", "C");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::AllelesMismatch { .. })
        ));
    }

    #[test]
    fn test_strand_flip() {
        assert_eq!(flip_strand("AT"), "TA");
        assert_eq!(flip_strand("CG"), "GC");
        assert_eq!(flip_strand("AC"), "TG");
        assert_eq!(flip_strand("--"), "--");
    }

    #[test]
    fn test_genotype_with_flip() {
        // Direct match should work
        assert_eq!(
            genotype_to_dosage_with_flip("TT", "T", "C").unwrap(),
            Some(0.0)
        );

        // Strand flip: AT genotype with REF=A, ALT=T should fail direct but succeed with flip
        // AT flips to TA, which is REF/ALT → 1.0
        assert_eq!(
            genotype_to_dosage_with_flip("AT", "T", "A").unwrap(),
            Some(1.0)
        );
    }

    #[test]
    fn test_batch_conversion() {
        let genotypes = vec![
            ("TT", "T", "C"), // 0.0
            ("TC", "T", "C"), // 1.0
            ("CC", "T", "C"), // 2.0
            ("--", "T", "C"), // None
        ];

        let results = batch_convert_genotypes(genotypes);

        assert_eq!(results[0].as_ref().unwrap(), &Some(0.0));
        assert_eq!(results[1].as_ref().unwrap(), &Some(1.0));
        assert_eq!(results[2].as_ref().unwrap(), &Some(2.0));
        assert_eq!(results[3].as_ref().unwrap(), &None);
    }

    #[test]
    fn test_all_nucleotide_combinations() {
        // Test all possible valid combinations
        let test_cases = vec![
            // REF=A combinations
            ("AA", "A", "T", 0.0),
            ("AT", "A", "T", 1.0),
            ("TA", "A", "T", 1.0),
            ("TT", "A", "T", 2.0),
            // REF=C combinations
            ("CC", "C", "G", 0.0),
            ("CG", "C", "G", 1.0),
            ("GC", "C", "G", 1.0),
            ("GG", "C", "G", 2.0),
            // REF=G combinations
            ("GG", "G", "C", 0.0),
            ("GC", "G", "C", 1.0),
            ("CG", "G", "C", 1.0),
            ("CC", "G", "C", 2.0),
            // REF=T combinations
            ("TT", "T", "A", 0.0),
            ("TA", "T", "A", 1.0),
            ("AT", "T", "A", 1.0),
            ("AA", "T", "A", 2.0),
        ];

        for (genotype, ref_allele, alt_allele, expected_dosage) in test_cases {
            let result = genotype_to_dosage(genotype, ref_allele, alt_allele);
            assert_eq!(
                result.unwrap(),
                Some(expected_dosage),
                "Failed for genotype={}, REF={}, ALT={}",
                genotype,
                ref_allele,
                alt_allele
            );
        }
    }

    #[test]
    fn test_indels() {
        // Indels should be rejected since 23andMe genotypes cannot represent them
        // REF=A, ALT=AG (insertion) with genotype "AA" should return AllelesMismatch
        let result = genotype_to_dosage("AA", "A", "AG");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::AllelesMismatch { .. })
        ));

        // Same for deletions
        let result = genotype_to_dosage("AA", "AG", "A");
        assert!(matches!(
            result,
            Err(GenotypeConversionError::AllelesMismatch { .. })
        ));
    }
}
