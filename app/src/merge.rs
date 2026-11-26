// ==============================================================================
// merge.rs - Per-Variant Merge of User and Reference Data
// ==============================================================================
// Description: Builds one 51-sample variant from the reference panel, the
//              user's 23andMe calls and the user's imputed VCF record
// Author: Matt Barham
// Created: 2026-11-26
// Modified: 2026-11-26
// Version: 1.0.0
// ==============================================================================

use crate::genotype_converter::genotype_to_dosage;
use crate::models::{DataSource, MultiSampleVariant, ReferencePanelVariant, SampleData};
use crate::parsers::genome23andme::Genome23Record;
use crate::parsers::vcf::VCFRecord;

/// Sample id of the user in the 51-sample merge
pub const USER_SAMPLE_ID: &str = "samp51";

/// Imputed calls below this R2 are labelled low quality
const LOW_QUALITY_R2: f64 = 0.3;

/// Genotype written when a sample has no call
pub const MISSING_GENOTYPE: &str = ".|.";

/// Merge one panel variant with the user's data into a 51-sample variant
pub fn merge_variant(
    chromosome: u8,
    reference: &ReferencePanelVariant,
    genotyped: &[&Genome23Record],
    imputed: Option<&VCFRecord>,
) -> MultiSampleVariant {
    let mut samples = reference_samples(reference);
    samples.push(user_sample(reference, genotyped, imputed));

    MultiSampleVariant {
        rsid: reference
            .rsid
            .clone()
            .unwrap_or_else(|| format!("{chromosome}:{}", reference.position)),
        chromosome,
        position: reference.position,
        ref_allele: reference.ref_allele.clone(),
        alt_allele: reference.alt_allele.clone(),
        allele_freq: reference.allele_freq,
        minor_allele_freq: reference.minor_allele_freq,
        is_typed: reference.is_typed,
        samples,
    }
}

/// The user's call at a panel variant.
///
/// `genotyped` holds the 23andMe records at the variant's position, and
/// `imputed` the VCF record matching its position, REF and ALT. A 23andMe call
/// made of this variant's alleles wins, then the imputed record. With neither,
/// the sample is missing: nothing is assumed about the user's genotype.
///
/// Genotypes are not strand-flipped. 23andMe reports the positive strand of
/// GRCh37, the panel's strand, so other alleles indicate a different variant.
pub fn user_sample(
    reference: &ReferencePanelVariant,
    genotyped: &[&Genome23Record],
    imputed: Option<&VCFRecord>,
) -> SampleData {
    let genotyped_dosage = genotyped.iter().find_map(|record| {
        genotype_to_dosage(
            &record.genotype,
            &reference.ref_allele,
            &reference.alt_allele,
        )
        .ok()
        .flatten()
    });

    let (genotype, dosage, source, imputation_quality) = match (genotyped_dosage, imputed) {
        (Some(dosage), _) => (
            format_dosage_as_genotype(dosage),
            Some(dosage),
            DataSource::Genotyped,
            None,
        ),
        (None, Some(record)) => (
            format_dosage_as_genotype(record.dosage),
            Some(record.dosage),
            if record.imputation_quality.unwrap_or(1.0) < LOW_QUALITY_R2 {
                DataSource::ImputedLowQual
            } else {
                DataSource::Imputed
            },
            record.imputation_quality,
        ),
        (None, None) => (
            MISSING_GENOTYPE.to_string(),
            None,
            DataSource::Missing,
            None,
        ),
    };

    SampleData {
        sample_id: USER_SAMPLE_ID.to_string(),
        genotype,
        dosage,
        source,
        imputation_quality,
    }
}

fn reference_samples(reference: &ReferencePanelVariant) -> Vec<SampleData> {
    reference
        .sample_genotypes
        .iter()
        .enumerate()
        .map(|(index, genotype)| {
            let dosage = dosage_from_genotype(genotype);
            SampleData {
                sample_id: format!("samp{}", index + 1),
                genotype: genotype.clone(),
                dosage,
                source: match dosage {
                    None => DataSource::Missing,
                    Some(_) if reference.is_typed => DataSource::Genotyped,
                    Some(_) => DataSource::Imputed,
                },
                imputation_quality: reference.imputation_quality,
            }
        })
        .collect()
}

fn format_dosage_as_genotype(dosage: f64) -> String {
    if dosage < 0.5 {
        "0|0".to_string()
    } else if dosage < 1.5 {
        "0|1".to_string()
    } else {
        "1|1".to_string()
    }
}

/// ALT allele count of a panel genotype such as `0|1` or `1/1`, or None if
/// either allele is unreadable
fn dosage_from_genotype(genotype: &str) -> Option<f64> {
    let (first, second) = genotype.split_once(['|', '/'])?;
    let total = first.parse::<u8>().ok()? + second.parse::<u8>().ok()?;
    Some(f64::from(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(ref_allele: &str, alt_allele: &str) -> ReferencePanelVariant {
        ReferencePanelVariant {
            chromosome: 1,
            position: 1000,
            rsid: Some("rs1".into()),
            ref_allele: ref_allele.into(),
            alt_allele: alt_allele.into(),
            phased: true,
            allele_freq: Some(0.5),
            minor_allele_freq: Some(0.5),
            imputation_quality: Some(0.99),
            is_typed: false,
            sample_genotypes: vec!["0|1".into(); 50],
        }
    }

    fn call(genotype: &str) -> Genome23Record {
        Genome23Record {
            rsid: "rs1".into(),
            chromosome: "1".into(),
            position: 1000,
            genotype: genotype.into(),
        }
    }

    fn imputed(dosage: f64, r2: f64) -> VCFRecord {
        VCFRecord {
            rsid: "rs1".into(),
            chromosome: 1,
            position: 1000,
            ref_allele: "A".into(),
            alt_allele: "G".into(),
            dosage,
            imputation_quality: Some(r2),
        }
    }

    #[test]
    fn every_allele_order_and_zygosity_counts_as_genotyped() {
        for (genotype, dosage, expected) in [
            ("AA", 0.0, "0|0"),
            ("AG", 1.0, "0|1"),
            ("GA", 1.0, "0|1"),
            ("GG", 2.0, "1|1"),
        ] {
            let record = call(genotype);
            // The imputed value disagrees, so it is visible which one was used
            let vcf = imputed(0.4, 0.9);
            let sample = user_sample(&panel("A", "G"), &[&record], Some(&vcf));
            assert_eq!(sample.source, DataSource::Genotyped, "{genotype}");
            assert_eq!(sample.dosage, Some(dosage), "{genotype}");
            assert_eq!(sample.genotype, expected, "{genotype}");
        }
    }

    #[test]
    fn uses_whichever_record_at_a_position_fits_the_alleles() {
        let unusable = call("--");
        let usable = call("GG");
        let sample = user_sample(&panel("A", "G"), &[&unusable, &usable], None);
        assert_eq!(sample.source, DataSource::Genotyped);
        assert_eq!(sample.dosage, Some(2.0));
    }

    #[test]
    fn unusable_genotypes_fall_back_to_the_imputed_record() {
        // No-call, other alleles, opposite strand, indel code, haploid call
        for genotype in ["--", "CC", "TC", "II", "A"] {
            let record = call(genotype);
            let vcf = imputed(1.2, 0.2);
            let sample = user_sample(&panel("A", "G"), &[&record], Some(&vcf));
            assert_eq!(sample.source, DataSource::ImputedLowQual, "{genotype}");
            assert_eq!(sample.dosage, Some(1.2), "{genotype}");
        }
    }

    #[test]
    fn a_variant_with_no_usable_call_is_missing() {
        let no_call = call("--");
        for genotyped in [vec![], vec![&no_call]] {
            let sample = user_sample(&panel("A", "G"), &genotyped, None);
            assert_eq!(sample.source, DataSource::Missing);
            assert_eq!(sample.dosage, None);
            assert_eq!(sample.genotype, ".|.");
            assert_eq!(sample.imputation_quality, None);
        }
    }

    #[test]
    fn merged_variant_holds_the_panel_then_the_user() {
        let mut reference = panel("A", "G");
        reference.sample_genotypes[0] = "1|1".into();
        reference.sample_genotypes[1] = ".|.".into();
        reference.rsid = None;

        let variant = merge_variant(7, &reference, &[], None);

        assert_eq!(variant.rsid, "7:1000");
        assert_eq!(variant.samples.len(), 51);
        assert_eq!(variant.samples[0].dosage, Some(2.0));
        assert_eq!(variant.samples[1].dosage, None);
        assert_eq!(variant.samples[1].source, DataSource::Missing);
        assert_eq!(variant.samples[2].dosage, Some(1.0));
        assert_eq!(variant.samples[50].sample_id, USER_SAMPLE_ID);
    }
}
