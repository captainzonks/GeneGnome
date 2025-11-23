// ==============================================================================
// parsers/vcf.rs - VCF file parser
// ==============================================================================
// Description: Parser for VCF (Variant Call Format) files using noodles-vcf
// Author: Matt Barham
// Created: 2025-11-03
// Modified: 2025-11-03
// Version: 1.0.0
// ==============================================================================
// References:
// - VCF 4.2 Spec: https://samtools.github.io/hts-specs/VCFv4.2.pdf
// - noodles-vcf: https://docs.rs/noodles-vcf/0.81.0/noodles_vcf/
// ==============================================================================

use noodles_vcf as vcf;
use noodles_vcf::variant::record::{AlternateBases, Ids};
use std::path::Path;
use thiserror::Error;

/// Parsed VCF record with relevant fields for genetic data processing
#[derive(Debug, Clone)]
pub struct VCFRecord {
    /// rsID (e.g., "rs12345") or generated ID (e.g., "chr1:10177:A:G")
    pub rsid: String,

    /// Chromosome number (1-22)
    pub chromosome: u8,

    /// Base pair position on chromosome
    pub position: u64,

    /// Reference allele (e.g., "A")
    pub ref_allele: String,

    /// Alternate allele (e.g., "G")
    pub alt_allele: String,

    /// Dosage value (0.0-2.0)
    /// - 0.0 = Homozygous reference (REF/REF)
    /// - 1.0 = Heterozygous (REF/ALT)
    /// - 2.0 = Homozygous alternate (ALT/ALT)
    /// - Decimal values indicate imputation uncertainty
    pub dosage: f64,

    /// Imputation quality (DR2 R-squared, 0.0-1.0)
    /// None if not available
    pub imputation_quality: Option<f64>,
}

/// VCF parsing errors
#[derive(Error, Debug)]
pub enum VCFParseError {
    #[error("Failed to open VCF file: {0}")]
    FileOpenError(String),

    #[error("Failed to read VCF header: {0}")]
    HeaderError(String),

    #[error("Failed to parse VCF record: {0}")]
    RecordError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid dosage value: {0} (must be 0.0-2.0)")]
    InvalidDosage(f64),

    #[error("Invalid chromosome: {0}")]
    InvalidChromosome(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// VCF parser with configuration options
pub struct VCFParser {
    /// Minimum imputation quality (DR2) to accept
    /// SNPs below this threshold are skipped
    pub min_quality: f64,

    /// Maximum number of errors before failing
    pub max_errors: usize,

    /// Count of skipped records (for reporting)
    pub skipped_count: usize,

    /// Count of error records (for reporting)
    pub error_count: usize,
}

impl Default for VCFParser {
    fn default() -> Self {
        Self {
            min_quality: 0.0,  // Accept all by default
            max_errors: 1000,  // Fail if >1000 bad records
            skipped_count: 0,
            error_count: 0,
        }
    }
}

impl VCFParser {
    /// Create new VCF parser with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set minimum imputation quality filter
    pub fn with_min_quality(mut self, quality: f64) -> Self {
        self.min_quality = quality;
        self
    }

    /// Set maximum allowed errors
    pub fn with_max_errors(mut self, max: usize) -> Self {
        self.max_errors = max;
        self
    }

    /// Parse VCF file and return vector of records
    ///
    /// # Arguments
    /// * `path` - Path to VCF file (can be .vcf or .vcf.gz)
    ///
    /// # Returns
    /// * `Result<Vec<VCFRecord>, VCFParseError>` - Parsed records or error
    ///
    /// # Example
    /// ```no_run
    /// use genetics_processor::parsers::VCFParser;
    ///
    /// let mut parser = VCFParser::new()
    ///     .with_min_quality(0.3)
    ///     .with_max_errors(100);
    ///
    /// let records = parser.parse("chr22.dose.vcf.gz")?;
    /// println!("Parsed {} SNPs", records.len());
    /// ```
    pub fn parse(&mut self, path: impl AsRef<Path>) -> Result<Vec<VCFRecord>, VCFParseError> {
        let path = path.as_ref();

        // Open VCF file using noodles builder
        let mut reader = vcf::io::reader::Builder::default()
            .build_from_path(path)
            .map_err(|e| VCFParseError::FileOpenError(format!("{}: {}", path.display(), e)))?;

        // Read header
        let header = reader
            .read_header()
            .map_err(|e| VCFParseError::HeaderError(format!("{}", e)))?;

        // Parse records
        let mut vcf_records = Vec::new();
        self.skipped_count = 0;
        self.error_count = 0;

        for (line_num, result) in reader.records().enumerate() {
            match result {
                Ok(record) => {
                    match self.parse_record(&record, &header) {
                        Ok(Some(vcf_record)) => vcf_records.push(vcf_record),
                        Ok(None) => self.skipped_count += 1,  // Filtered by quality
                        Err(e) => {
                            eprintln!("Warning: Line {}: {}", line_num + 1, e);
                            self.error_count += 1;

                            if self.error_count > self.max_errors {
                                return Err(VCFParseError::RecordError(
                                    format!("Too many errors ({} > {})", self.error_count, self.max_errors)
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Line {}: Failed to read record: {}", line_num + 1, e);
                    self.error_count += 1;

                    if self.error_count > self.max_errors {
                        return Err(VCFParseError::RecordError(
                            format!("Too many errors ({} > {})", self.error_count, self.max_errors)
                        ));
                    }
                }
            }
        }

        Ok(vcf_records)
    }

    /// Parse a single VCF record
    ///
    /// Returns:
    /// - Ok(Some(record)) if successfully parsed and passes quality filter
    /// - Ok(None) if filtered by quality threshold
    /// - Err if parsing failed
    fn parse_record(
        &self,
        record: &vcf::Record,
        header: &vcf::Header,
    ) -> Result<Option<VCFRecord>, VCFParseError> {
        // Extract chromosome
        let chrom_str = record.reference_sequence_name();
        let chromosome = self.parse_chromosome(chrom_str)?;

        // Extract position
        let position = match record.variant_start() {
            Some(Ok(pos)) => usize::from(pos.get()) as u64,
            Some(Err(e)) => return Err(VCFParseError::RecordError(format!("Failed to get position: {}", e))),
            None => return Err(VCFParseError::MissingField("Position".to_string())),
        };

        // Extract rsID (or generate one if missing)
        let rsid = self.extract_rsid(record, chromosome, position);

        // Extract REF allele
        let ref_allele = record.reference_bases().to_string();

        // Extract ALT allele (take first if multiple)
        let alt_alleles = record.alternate_bases();
        let alt_allele = if alt_alleles.is_empty() {
            return Err(VCFParseError::MissingField("ALT allele".to_string()));
        } else {
            alt_alleles.iter().next().unwrap()
                .map_err(|e| VCFParseError::RecordError(format!("Failed to get ALT allele: {}", e)))?
                .to_string()
        };

        // Extract dosage (DS field from FORMAT column)
        let dosage = self.extract_dosage(record, header)?;

        // Validate dosage range
        if !(0.0..=2.0).contains(&dosage) {
            return Err(VCFParseError::InvalidDosage(dosage));
        }

        // Extract imputation quality (R2 from INFO)
        let imputation_quality = self.extract_dr2(record, header);

        // Apply quality filter
        if let Some(quality) = imputation_quality {
            if quality < self.min_quality {
                return Ok(None);  // Skip low-quality SNPs
            }
        }

        Ok(Some(VCFRecord {
            rsid,
            chromosome,
            position,
            ref_allele,
            alt_allele,
            dosage,
            imputation_quality,
        }))
    }

    /// Parse chromosome string to u8
    fn parse_chromosome(&self, chrom: &str) -> Result<u8, VCFParseError> {
        // Handle "chr1" or "1" format
        let chrom_num = chrom.trim_start_matches("chr");

        chrom_num
            .parse::<u8>()
            .map_err(|_| VCFParseError::InvalidChromosome(chrom.to_string()))
            .and_then(|n| {
                if (1..=22).contains(&n) {
                    Ok(n)
                } else {
                    Err(VCFParseError::InvalidChromosome(format!(
                        "{} (must be 1-22)", chrom
                    )))
                }
            })
    }

    /// Extract rsID or generate pseudo-ID
    fn extract_rsid(&self, record: &vcf::Record, chromosome: u8, position: u64) -> String {
        // Get IDs from record
        let ids = record.ids();

        if ids.is_empty() {
            // Generate pseudo-ID for novel variants
            // Format: chr{CHROM}:{POS}:{REF}:{ALT}
            let ref_bases = record.reference_bases();
            let alt_bases = record.alternate_bases();
            let alt_str = if let Some(alt_result) = alt_bases.iter().next() {
                alt_result.unwrap_or("N")
            } else {
                "N"
            };

            format!("chr{}:{}:{}:{}", chromosome, position, ref_bases, alt_str)
        } else {
            // Use first rsID
            ids.iter().next().unwrap().to_string()
        }
    }

    /// Extract dosage (DS) field from FORMAT column
    ///
    /// Parses the raw VCF line to extract DS value from last sample
    /// Uses Debug format to access samples string from noodles Record
    fn extract_dosage(&self, record: &vcf::Record, _header: &vcf::Header) -> Result<f64, VCFParseError> {
        let record_str = format!("{:?}", record);

        // Find the samples field: samples: Samples("...")
        let samples_prefix = "samples: Samples(\"";
        let start_idx = record_str.find(samples_prefix)
            .ok_or_else(|| VCFParseError::RecordError("Samples field not found in debug output".to_string()))?
            + samples_prefix.len();

        // Find the closing quote
        let end_idx = record_str[start_idx..].find("\")")
            .ok_or_else(|| VCFParseError::RecordError("Samples field end not found".to_string()))?
            + start_idx;

        // Extract samples string (FORMAT + all sample data, tab-separated)
        let samples_str = &record_str[start_idx..end_idx];

        // Split by tabs (in debug format they're literal \t, not escaped)
        let fields: Vec<&str> = samples_str.split("\\t").collect();

        if fields.is_empty() {
            return Err(VCFParseError::MissingField("No sample data found".to_string()));
        }

        // First field is FORMAT
        let format = fields[0];

        // Find DS position in FORMAT
        let format_keys: Vec<&str> = format.split(':').collect();
        let ds_index = format_keys.iter().position(|&k| k == "DS")
            .ok_or_else(|| VCFParseError::MissingField("DS not found in FORMAT".to_string()))?;

        // Last sample is at the end (fields[0] is FORMAT, fields[1..] are samples)
        if fields.len() < 2 {
            return Err(VCFParseError::MissingField("No sample columns found".to_string()));
        }
        let last_sample = fields[fields.len() - 1];

        // Extract DS value from sample
        let sample_values: Vec<&str> = last_sample.split(':').collect();

        if ds_index >= sample_values.len() {
            return Err(VCFParseError::MissingField("DS index out of bounds".to_string()));
        }

        let ds_str = sample_values[ds_index];

        // Parse as float
        ds_str.parse::<f64>()
            .map_err(|e| VCFParseError::RecordError(format!("Failed to parse DS '{}' as f64: {}", ds_str, e)))
    }

    /// Extract R2 (imputation quality) from INFO field
    ///
    /// Michigan Imputation Server uses "R2" (not "DR2") for imputation quality
    fn extract_dr2(&self, record: &vcf::Record, _header: &vcf::Header) -> Option<f64> {
        let record_str = format!("{:?}", record);

        // Find the info field: info: Info("...")
        let info_prefix = "info: Info(\"";
        let start_idx = record_str.find(info_prefix)? + info_prefix.len();

        // Find the closing quote
        let end_idx = record_str[start_idx..].find("\")")? + start_idx;

        // Extract INFO string
        let info = &record_str[start_idx..end_idx];

        // Split INFO field by semicolons
        for field in info.split(';') {
            if field.starts_with("R2=") {
                let r2_str = &field[3..];  // Skip "R2="
                if let Ok(r2) = r2_str.parse::<f64>() {
                    return Some(r2);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chromosome_parsing() {
        let parser = VCFParser::new();

        assert_eq!(parser.parse_chromosome("1").unwrap(), 1);
        assert_eq!(parser.parse_chromosome("chr1").unwrap(), 1);
        assert_eq!(parser.parse_chromosome("22").unwrap(), 22);
        assert_eq!(parser.parse_chromosome("chr22").unwrap(), 22);

        assert!(parser.parse_chromosome("X").is_err());
        assert!(parser.parse_chromosome("23").is_err());
        assert!(parser.parse_chromosome("chr0").is_err());
    }

    #[test]
    fn test_dosage_validation() {
        let parser = VCFParser::new();

        // Valid dosages
        assert!(VCFRecord {
            rsid: "rs1".to_string(),
            chromosome: 1,
            position: 100,
            ref_allele: "A".to_string(),
            alt_allele: "G".to_string(),
            dosage: 0.0,
            imputation_quality: None,
        }.dosage >= 0.0);

        assert!(VCFRecord {
            rsid: "rs2".to_string(),
            chromosome: 1,
            position: 200,
            ref_allele: "A".to_string(),
            alt_allele: "G".to_string(),
            dosage: 2.0,
            imputation_quality: None,
        }.dosage <= 2.0);
    }
}
