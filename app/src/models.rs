// ==============================================================================
// models.rs - Multi-Sample Data Models
// ==============================================================================
// Description: Data structures for 51-sample genomic data processing
// Author: Matt Barham
// Created: 2025-11-12
// Modified: 2025-11-12
// Version: 2.0.0
// ==============================================================================

use serde::{Deserialize, Serialize};

/// Source of genomic data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataSource {
    /// Directly genotyped from 23andMe or similar service
    Genotyped,
    /// Imputed with high quality (R2 >= threshold)
    Imputed,
    /// Imputed with low quality (R2 < 0.3)
    ImputedLowQual,
}

impl DataSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataSource::Genotyped => "Genotyped",
            DataSource::Imputed => "Imputed",
            DataSource::ImputedLowQual => "ImputedLowQual",
        }
    }
}

/// Sample-specific genomic data at a variant position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleData {
    /// Sample identifier (e.g., "samp1", "samp2", ..., "samp50", "samp51")
    pub sample_id: String,

    /// Phased genotype (e.g., "0|0", "0|1", "1|0", "1|1")
    pub genotype: String,

    /// Allele dosage (0.0 to 2.0)
    pub dosage: f64,

    /// Source of this sample's data
    pub source: DataSource,

    /// Imputation quality (R2) if imputed, None if genotyped
    pub imputation_quality: Option<f64>,
}

/// Multi-sample variant data (51 samples: 50 reference + 1 user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSampleVariant {
    /// rsID (e.g., "rs12345")
    pub rsid: String,

    /// Chromosome (1-22)
    pub chromosome: u8,

    /// Position (GRCh37/hg19)
    pub position: u64,

    /// Reference allele
    pub ref_allele: String,

    /// Alternate allele
    pub alt_allele: String,

    /// Allele frequency from reference panel
    pub allele_freq: Option<f64>,

    /// Minor allele frequency from reference panel
    pub minor_allele_freq: Option<f64>,

    /// Whether this variant was typed (genotyped) in reference panel
    pub is_typed: bool,

    /// Data for all 51 samples
    pub samples: Vec<SampleData>,
}

/// Reference panel variant (50 samples only)
#[derive(Debug, Clone)]
pub struct ReferencePanelVariant {
    pub chromosome: u8,
    pub position: u64,
    pub rsid: Option<String>,
    pub ref_allele: String,
    pub alt_allele: String,
    pub phased: bool,
    pub allele_freq: Option<f64>,
    pub minor_allele_freq: Option<f64>,
    pub imputation_quality: Option<f64>,
    pub is_typed: bool,
    /// Sample genotypes: Vec of 50 genotype strings ("0|0", etc.)
    pub sample_genotypes: Vec<String>,
}

/// Quality threshold for filtering imputed variants
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum QualityThreshold {
    /// Filter variants with R2 < 0.8
    R08,
    /// Filter variants with R2 < 0.9
    R09,
    /// No quality filtering
    NoFilter,
}

/// Single-sample merged variant (backward compatibility for worker)
/// TODO: Migrate worker to use MultiSampleVariant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedVariant {
    /// SNP identifier (rsID)
    pub rsid: String,
    /// Chromosome number (1-22)
    pub chromosome: u8,
    /// Base pair position (GRCh37/hg19)
    pub position: u64,
    /// Reference allele
    pub ref_allele: String,
    /// Alternate allele
    pub alt_allele: String,
    /// Final dosage value (0.0-2.0)
    pub dosage: f64,
    /// Source of dosage value
    pub source: DataSource,
    /// Imputation quality (R²) if from VCF
    pub imputation_quality: Option<f64>,
}

impl QualityThreshold {
    pub fn threshold_value(&self) -> Option<f64> {
        match self {
            QualityThreshold::R08 => Some(0.8),
            QualityThreshold::R09 => Some(0.9),
            QualityThreshold::NoFilter => None,
        }
    }

    pub fn passes(&self, r2: Option<f64>) -> bool {
        match (self.threshold_value(), r2) {
            (None, _) => true, // No filter
            (Some(_), None) => true, // Genotyped data passes
            (Some(threshold), Some(r2_value)) => r2_value >= threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_threshold_passes() {
        let r09 = QualityThreshold::R09;
        assert!(r09.passes(Some(0.95))); // Passes
        assert!(!r09.passes(Some(0.85))); // Fails
        assert!(r09.passes(None)); // Genotyped passes

        let no_filter = QualityThreshold::NoFilter;
        assert!(no_filter.passes(Some(0.1))); // All pass with no filter
    }

    #[test]
    fn test_data_source_str() {
        assert_eq!(DataSource::Genotyped.as_str(), "Genotyped");
        assert_eq!(DataSource::Imputed.as_str(), "Imputed");
        assert_eq!(DataSource::ImputedLowQual.as_str(), "ImputedLowQual");
    }
}
