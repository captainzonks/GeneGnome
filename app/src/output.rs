// ==============================================================================
// output.rs - Multi-Format Output Generation
// ==============================================================================
// Description: Generate genetic analysis results in multiple formats for web delivery
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2025-11-06
// Version: 1.0.0
// ==============================================================================

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

// Apache Arrow/Parquet for columnar data
use arrow::array::{
    ArrayRef, Float64Array, StringArray, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::{RecordBatch, RecordBatchReader};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

// SQLite for queryable database
use rusqlite::{params, Connection};

use crate::parsers::PgsDataset;
use crate::models::{DataSource, MergedVariant, MultiSampleVariant, SampleData};

/// Supported output formats for web delivery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Apache Parquet (best for data science: Python, R, Spark)
    Parquet,
    /// JSON (best for web APIs and JavaScript)
    Json,
    /// SQLite database (best for querying and exploration)
    Sqlite,
    /// VCF with dosages (best for bioinformatics tools)
    Vcf,
    /// R workspace (for R users - requires conversion script)
    RData,
}

impl OutputFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Parquet => "parquet",
            OutputFormat::Json => "json",
            OutputFormat::Sqlite => "db",
            OutputFormat::Vcf => "vcf.gz",
            OutputFormat::RData => "RData",
        }
    }

    /// Get MIME type for HTTP downloads
    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Parquet => "application/vnd.apache.parquet",
            OutputFormat::Json => "application/json",
            OutputFormat::Sqlite => "application/vnd.sqlite3",
            OutputFormat::Vcf => "text/x-vcf",
            OutputFormat::RData => "application/octet-stream",
        }
    }

    /// Check if format is implemented
    pub fn is_implemented(&self) -> bool {
        matches!(
            self,
            OutputFormat::Json | OutputFormat::Parquet | OutputFormat::Sqlite | OutputFormat::Vcf
        )
        // RData requires external R conversion script
    }
}

/// Complete genetic analysis output (single-sample)
/// DEPRECATED: Use MultiSampleGeneticOutput for new code
#[derive(Debug, Serialize, Deserialize)]
pub struct GeneticAnalysisOutput {
    /// Metadata about the analysis
    pub metadata: OutputMetadata,

    /// Merged variants per chromosome (chr1 - chr22)
    pub chromosomes: HashMap<u8, Vec<MergedVariantOutput>>,

    /// Polygenic scores (unscaled)
    pub pgs_unscaled: Vec<PgsRecordOutput>,

    /// Polygenic scores (z-score normalized)
    pub pgs_scaled: Vec<PgsRecordOutput>,
}

/// Complete genetic analysis output (multi-sample: 51 samples)
#[derive(Debug, Serialize, Deserialize)]
pub struct MultiSampleGeneticOutput {
    /// Metadata about the analysis
    pub metadata: OutputMetadata,

    /// Multi-sample variants per chromosome (chr1 - chr22)
    pub chromosomes: HashMap<u8, Vec<MultiSampleVariantOutput>>,

    /// Polygenic scores (unscaled) - for all 51 samples
    pub pgs_unscaled: Vec<PgsRecordOutput>,

    /// Polygenic scores (z-score normalized) - for all 51 samples
    pub pgs_scaled: Vec<PgsRecordOutput>,
}

/// Analysis metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct OutputMetadata {
    pub job_id: String,
    pub user_id: String,
    pub processing_date: String,
    pub genome_file: String,
    pub imputation_server: String,
    pub reference_panel: String,
    pub total_snps: usize,
    pub genotyped_snps: usize,
    pub imputed_snps: usize,
    pub low_quality_snps: usize,
    pub pgs_traits: Vec<String>,
}

/// Merged variant for output (simplified from internal representation)
/// DEPRECATED: Use MultiSampleVariantOutput for new code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedVariantOutput {
    pub rsid: String,
    pub position: u64,
    pub ref_allele: String,
    pub alt_allele: String,
    pub dosage: f64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imputation_quality: Option<f64>,
}

/// Multi-sample variant for output (51 samples: 50 reference + 1 user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSampleVariantOutput {
    pub rsid: String,
    pub chromosome: u8,
    pub position: u64,
    pub ref_allele: String,
    pub alt_allele: String,
    pub allele_freq: Option<f64>,
    pub minor_allele_freq: Option<f64>,
    pub is_typed: bool,
    pub samples: Vec<SampleDataOutput>,
}

/// Sample data for output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleDataOutput {
    pub sample_id: String,
    pub genotype: String,
    pub dosage: f64,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imputation_quality: Option<f64>,
}

/// PGS record for output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgsRecordOutput {
    pub sample_id: String,
    pub trait_label: String,
    pub value: f64,
}

/// Multi-format output generator
pub struct OutputGenerator {
    job_id: String,
    user_id: String,
    output_dir: PathBuf,
    // Streaming state (None if not in streaming mode)
    streaming_state: Option<StreamingState>,
}

/// VCF output format preference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcfFormat {
    /// Single merged VCF file for all 22 chromosomes
    Merged,
    /// Separate VCF files per chromosome (chr1.vcf.gz, chr2.vcf.gz, etc.)
    PerChromosome,
}

/// Streaming output state for incremental chromosome processing
struct StreamingState {
    formats: Vec<OutputFormat>,
    // VCF format preference (merged or per-chromosome)
    vcf_format: VcfFormat,
    // SQLite connection (kept open across chromosomes)
    sqlite_conn: Option<Connection>,
    sqlite_path: Option<PathBuf>,
    // JSON file handle and state
    json_file: Option<std::fs::File>,
    json_path: Option<PathBuf>,
    json_first_chromosome: bool,
    // VCF file handle (gzip-compressed) - for merged format
    vcf_file: Option<flate2::write::GzEncoder<std::fs::File>>,
    vcf_path: Option<PathBuf>,
    vcf_header_written: bool,
    // VCF per-chromosome files - for per-chromosome format
    vcf_files: Vec<PathBuf>,
    vcf_base_path: Option<PathBuf>,
    // Parquet per-chromosome files
    parquet_files: Vec<PathBuf>,
    parquet_base_path: Option<PathBuf>,
    // Accumulated metadata
    total_variants: usize,
    genotyped_variants: usize,
    low_quality_variants: usize,
    chromosomes_processed: u8,
}

impl OutputGenerator {
    pub fn new(job_id: String, user_id: String, output_dir: PathBuf) -> Self {
        Self {
            job_id,
            user_id,
            output_dir,
            streaming_state: None,
        }
    }

    /// Generate output in specified formats (single-sample, deprecated)
    ///
    /// # Arguments
    /// * `formats` - List of formats to generate
    /// * `merged_chromosomes` - Merged genetic variants per chromosome
    /// * `pgs_data` - Polygenic scores (unscaled and scaled), optional
    ///
    /// # Returns
    /// * HashMap of format -> file path
    pub async fn generate(
        &self,
        formats: &[OutputFormat],
        merged_chromosomes: &HashMap<u8, Vec<MergedVariant>>,
        pgs_data: Option<&PgsDataset>,
    ) -> Result<HashMap<OutputFormat, PathBuf>> {
        // Create output directory
        std::fs::create_dir_all(&self.output_dir)?;

        // Build complete output structure
        let output = self.build_output(merged_chromosomes, pgs_data);

        // Generate each requested format
        let mut result = HashMap::new();

        for format in formats {
            if !format.is_implemented() {
                info!("Skipping unimplemented format: {:?}", format);
                continue;
            }

            let path = self.generate_format(format, &output).await?;
            result.insert(*format, path);
        }

        Ok(result)
    }

    /// Generate output in specified formats (multi-sample: 51 samples)
    ///
    /// # Arguments
    /// * `formats` - List of formats to generate
    /// * `multi_sample_chromosomes` - Multi-sample genetic variants per chromosome
    /// * `pgs_data` - Polygenic scores (unscaled and scaled), optional
    ///
    /// # Returns
    /// * HashMap of format -> file path
    pub async fn generate_multi_sample(
        &self,
        formats: &[OutputFormat],
        multi_sample_chromosomes: &HashMap<u8, Vec<MultiSampleVariant>>,
        pgs_data: Option<&PgsDataset>,
    ) -> Result<HashMap<OutputFormat, PathBuf>> {
        // Create output directory
        std::fs::create_dir_all(&self.output_dir)?;

        // Build complete multi-sample output structure
        let output = self.build_multi_sample_output(multi_sample_chromosomes, pgs_data);

        // Generate each requested format
        let mut result = HashMap::new();

        for format in formats {
            if !format.is_implemented() {
                info!("Skipping unimplemented format: {:?}", format);
                continue;
            }

            let path = self.generate_multi_sample_format(format, &output).await?;
            result.insert(*format, path);
        }

        Ok(result)
    }

    /// Build complete output structure
    fn build_output(
        &self,
        merged_chromosomes: &HashMap<u8, Vec<MergedVariant>>,
        pgs_data: Option<&PgsDataset>,
    ) -> GeneticAnalysisOutput {
        // Convert internal representation to output representation
        let chromosomes: HashMap<u8, Vec<MergedVariantOutput>> = merged_chromosomes
            .iter()
            .map(|(chr, variants)| {
                let output_variants = variants
                    .iter()
                    .map(|v| MergedVariantOutput {
                        rsid: v.rsid.clone(),
                        position: v.position,
                        ref_allele: v.ref_allele.clone(),
                        alt_allele: v.alt_allele.clone(),
                        dosage: v.dosage,
                        source: format!("{:?}", v.source),
                        imputation_quality: v.imputation_quality,
                    })
                    .collect();
                (*chr, output_variants)
            })
            .collect();

        // Convert PGS data (if available)
        let (pgs_unscaled, pgs_scaled, pgs_traits) = match pgs_data {
            Some(data) => {
                let unscaled: Vec<PgsRecordOutput> = data
                    .unscaled
                    .iter()
                    .map(|r| PgsRecordOutput {
                        sample_id: r.sample_id.clone(),
                        trait_label: r.label.clone(),
                        value: r.value,
                    })
                    .collect();

                let scaled: Vec<PgsRecordOutput> = data
                    .scaled
                    .iter()
                    .map(|r| PgsRecordOutput {
                        sample_id: r.sample_id.clone(),
                        trait_label: r.label.clone(),
                        value: r.value,
                    })
                    .collect();

                let traits: Vec<String> = data
                    .unscaled
                    .iter()
                    .map(|r| r.label.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                (unscaled, scaled, traits)
            }
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        // Calculate statistics
        let total_snps: usize = chromosomes.values().map(|v| v.len()).sum();
        let genotyped_snps: usize = merged_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|m| matches!(m.source, DataSource::Genotyped))
            .count();
        let low_quality_snps: usize = merged_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|m| matches!(m.source, DataSource::ImputedLowQual))
            .count();

        GeneticAnalysisOutput {
            metadata: OutputMetadata {
                job_id: self.job_id.clone(),
                user_id: self.user_id.clone(),
                processing_date: chrono::Utc::now().to_rfc3339(),
                genome_file: "23andMe genome data".to_string(),
                imputation_server: "Michigan Imputation Server 2".to_string(),
                reference_panel: "openSNP (50 samples)".to_string(),
                total_snps,
                genotyped_snps,
                imputed_snps: total_snps - genotyped_snps,
                low_quality_snps,
                pgs_traits,
            },
            chromosomes,
            pgs_unscaled,
            pgs_scaled,
        }
    }

    /// Build complete multi-sample output structure (51 samples)
    fn build_multi_sample_output(
        &self,
        multi_sample_chromosomes: &HashMap<u8, Vec<MultiSampleVariant>>,
        pgs_data: Option<&PgsDataset>,
    ) -> MultiSampleGeneticOutput {
        // Convert internal multi-sample representation to output representation
        let chromosomes: HashMap<u8, Vec<MultiSampleVariantOutput>> = multi_sample_chromosomes
            .iter()
            .map(|(chr, variants)| {
                let output_variants = variants
                    .iter()
                    .map(|v| {
                        // Convert samples
                        let samples: Vec<SampleDataOutput> = v
                            .samples
                            .iter()
                            .map(|s| SampleDataOutput {
                                sample_id: s.sample_id.clone(),
                                genotype: s.genotype.clone(),
                                dosage: s.dosage,
                                source: format!("{:?}", s.source),
                                imputation_quality: s.imputation_quality,
                            })
                            .collect();

                        MultiSampleVariantOutput {
                            rsid: v.rsid.clone(),
                            chromosome: v.chromosome,
                            position: v.position,
                            ref_allele: v.ref_allele.clone(),
                            alt_allele: v.alt_allele.clone(),
                            allele_freq: v.allele_freq,
                            minor_allele_freq: v.minor_allele_freq,
                            is_typed: v.is_typed,
                            samples,
                        }
                    })
                    .collect();
                (*chr, output_variants)
            })
            .collect();

        // Convert PGS data (if available) - same as single-sample
        let (pgs_unscaled, pgs_scaled, pgs_traits) = match pgs_data {
            Some(data) => {
                let unscaled: Vec<PgsRecordOutput> = data
                    .unscaled
                    .iter()
                    .map(|r| PgsRecordOutput {
                        sample_id: r.sample_id.clone(),
                        trait_label: r.label.clone(),
                        value: r.value,
                    })
                    .collect();

                let scaled: Vec<PgsRecordOutput> = data
                    .scaled
                    .iter()
                    .map(|r| PgsRecordOutput {
                        sample_id: r.sample_id.clone(),
                        trait_label: r.label.clone(),
                        value: r.value,
                    })
                    .collect();

                let traits: Vec<String> = data
                    .unscaled
                    .iter()
                    .map(|r| r.label.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                (unscaled, scaled, traits)
            }
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        // Calculate statistics across all 51 samples
        // For multi-sample, we count unique variants (not total samples * variants)
        let total_snps: usize = chromosomes.values().map(|v| v.len()).sum();

        // Count typed variants (variants that have is_typed = true)
        let genotyped_snps: usize = multi_sample_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|m| m.is_typed)
            .count();

        // Count low quality SNPs (variants with R² < 0.3 for the user sample)
        let low_quality_snps: usize = multi_sample_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|m| {
                // Check user sample (last sample, index 50)
                if let Some(user_sample) = m.samples.get(50) {
                    matches!(user_sample.source, DataSource::ImputedLowQual)
                } else {
                    false
                }
            })
            .count();

        MultiSampleGeneticOutput {
            metadata: OutputMetadata {
                job_id: self.job_id.clone(),
                user_id: self.user_id.clone(),
                processing_date: chrono::Utc::now().to_rfc3339(),
                genome_file: "23andMe genome data".to_string(),
                imputation_server: "Michigan Imputation Server 2".to_string(),
                reference_panel: "openSNP (50 samples) + user (1 sample) = 51 total".to_string(),
                total_snps,
                genotyped_snps,
                imputed_snps: total_snps - genotyped_snps,
                low_quality_snps,
                pgs_traits,
            },
            chromosomes,
            pgs_unscaled,
            pgs_scaled,
        }
    }

    /// Generate specific format (single-sample, deprecated)
    async fn generate_format(
        &self,
        format: &OutputFormat,
        output: &GeneticAnalysisOutput,
    ) -> Result<PathBuf> {
        let filename = format!("GenomicData_{}.{}", self.job_id, format.extension());
        let path = self.output_dir.join(&filename);

        match format {
            OutputFormat::Json => self.generate_json(&path, output).await,
            OutputFormat::Parquet => self.generate_parquet(&path, output).await,
            OutputFormat::Sqlite => self.generate_sqlite(&path, output).await,
            OutputFormat::Vcf => self.generate_vcf(&path, output).await,
            OutputFormat::RData => {
                // RData requires external R conversion script
                // Users can convert JSON/Parquet to RData using R
                Err(anyhow::anyhow!(
                    "RData format requires external R conversion. Use JSON or Parquet as input."
                ))
            }
        }
    }

    /// Generate specific format (multi-sample: 51 samples)
    async fn generate_multi_sample_format(
        &self,
        format: &OutputFormat,
        output: &MultiSampleGeneticOutput,
    ) -> Result<PathBuf> {
        let filename = format!("GenomicData_{}_51samples.{}", self.job_id, format.extension());
        let path = self.output_dir.join(&filename);

        match format {
            OutputFormat::Json => self.generate_multi_sample_json(&path, output).await,
            OutputFormat::Parquet => self.generate_multi_sample_parquet(&path, output).await,
            OutputFormat::Sqlite => self.generate_multi_sample_sqlite(&path, output).await,
            OutputFormat::Vcf => self.generate_multi_sample_vcf(&path, output).await,
            OutputFormat::RData => {
                // RData requires external R conversion script
                // Users can convert JSON/Parquet to RData using R
                Err(anyhow::anyhow!(
                    "RData format requires external R conversion. Use JSON or Parquet as input."
                ))
            }
        }
    }

    /// Generate JSON output (single-sample, deprecated)
    async fn generate_json(
        &self,
        path: &Path,
        output: &GeneticAnalysisOutput,
    ) -> Result<PathBuf> {
        info!("Generating JSON output: {:?}", path);

        let file = std::fs::File::create(path)
            .context("Failed to create JSON output file")?;

        serde_json::to_writer_pretty(file, output)
            .context("Failed to write JSON output")?;

        info!(
            "JSON output complete: {} SNPs, {} PGS traits",
            output.metadata.total_snps,
            output.metadata.pgs_traits.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate JSON output (multi-sample: 51 samples)
    async fn generate_multi_sample_json(
        &self,
        path: &Path,
        output: &MultiSampleGeneticOutput,
    ) -> Result<PathBuf> {
        info!("Generating multi-sample JSON output (51 samples): {:?}", path);

        let file = std::fs::File::create(path)
            .context("Failed to create JSON output file")?;

        serde_json::to_writer_pretty(file, output)
            .context("Failed to write JSON output")?;

        info!(
            "Multi-sample JSON output complete: {} SNPs, 51 samples, {} PGS traits",
            output.metadata.total_snps,
            output.metadata.pgs_traits.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate Parquet output (columnar format for data science)
    async fn generate_parquet(
        &self,
        path: &Path,
        output: &GeneticAnalysisOutput,
    ) -> Result<PathBuf> {
        info!("Generating Parquet output: {:?}", path);

        // Flatten all chromosomes into a single dataset
        let mut all_variants: Vec<&MergedVariantOutput> = Vec::new();
        for variants in output.chromosomes.values() {
            all_variants.extend(variants.iter());
        }

        // Create Arrow schema for variants
        let variant_schema = Arc::new(Schema::new(vec![
            Field::new("rsid", DataType::Utf8, false),
            Field::new("position", DataType::UInt64, false),
            Field::new("ref_allele", DataType::Utf8, false),
            Field::new("alt_allele", DataType::Utf8, false),
            Field::new("dosage", DataType::Float64, false),
            Field::new("source", DataType::Utf8, false),
            Field::new("imputation_quality", DataType::Float64, true),
        ]));

        // Build Arrow arrays for variants
        let rsid_array: ArrayRef = Arc::new(StringArray::from(
            all_variants.iter().map(|v| v.rsid.as_str()).collect::<Vec<_>>(),
        ));
        let position_array: ArrayRef = Arc::new(UInt64Array::from(
            all_variants.iter().map(|v| v.position).collect::<Vec<_>>(),
        ));
        let ref_array: ArrayRef = Arc::new(StringArray::from(
            all_variants.iter().map(|v| v.ref_allele.as_str()).collect::<Vec<_>>(),
        ));
        let alt_array: ArrayRef = Arc::new(StringArray::from(
            all_variants.iter().map(|v| v.alt_allele.as_str()).collect::<Vec<_>>(),
        ));
        let dosage_array: ArrayRef = Arc::new(Float64Array::from(
            all_variants.iter().map(|v| v.dosage).collect::<Vec<_>>(),
        ));
        let source_array: ArrayRef = Arc::new(StringArray::from(
            all_variants.iter().map(|v| v.source.as_str()).collect::<Vec<_>>(),
        ));
        let quality_array: ArrayRef = Arc::new(Float64Array::from(
            all_variants
                .iter()
                .map(|v| v.imputation_quality)
                .collect::<Vec<_>>(),
        ));

        // Create RecordBatch
        let variant_batch = RecordBatch::try_new(
            variant_schema.clone(),
            vec![
                rsid_array,
                position_array,
                ref_array,
                alt_array,
                dosage_array,
                source_array,
                quality_array,
            ],
        )
        .context("Failed to create Arrow RecordBatch")?;

        // Write to Parquet file with compression
        let file = std::fs::File::create(path).context("Failed to create Parquet file")?;
        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, variant_schema, Some(props))
            .context("Failed to create Parquet writer")?;

        writer
            .write(&variant_batch)
            .context("Failed to write Parquet data")?;
        writer.close().context("Failed to close Parquet writer")?;

        info!(
            "Parquet output complete: {} variants",
            all_variants.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate Parquet output (multi-sample: 51 samples, columnar format)
    async fn generate_multi_sample_parquet(
        &self,
        path: &Path,
        output: &MultiSampleGeneticOutput,
    ) -> Result<PathBuf> {
        info!("Generating multi-sample Parquet output (51 samples): {:?}", path);

        // Flatten all chromosomes and all samples into a single dataset
        // Each row represents one sample's data for one variant
        let mut all_rows: Vec<(&MultiSampleVariantOutput, &SampleDataOutput)> = Vec::new();
        for variants in output.chromosomes.values() {
            for variant in variants {
                for sample in &variant.samples {
                    all_rows.push((variant, sample));
                }
            }
        }

        // Create Arrow schema for multi-sample variants
        let variant_schema = Arc::new(Schema::new(vec![
            Field::new("rsid", DataType::Utf8, false),
            Field::new("chromosome", DataType::UInt64, false),
            Field::new("position", DataType::UInt64, false),
            Field::new("ref_allele", DataType::Utf8, false),
            Field::new("alt_allele", DataType::Utf8, false),
            Field::new("allele_freq", DataType::Float64, true),
            Field::new("minor_allele_freq", DataType::Float64, true),
            Field::new("is_typed", DataType::UInt64, false),
            Field::new("sample_id", DataType::Utf8, false),
            Field::new("genotype", DataType::Utf8, false),
            Field::new("dosage", DataType::Float64, false),
            Field::new("source", DataType::Utf8, false),
            Field::new("imputation_quality", DataType::Float64, true),
        ]));

        // Build Arrow arrays for multi-sample variants
        let rsid_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(v, _)| v.rsid.as_str()).collect::<Vec<_>>(),
        ));
        let chromosome_array: ArrayRef = Arc::new(UInt64Array::from(
            all_rows.iter().map(|(v, _)| v.chromosome as u64).collect::<Vec<_>>(),
        ));
        let position_array: ArrayRef = Arc::new(UInt64Array::from(
            all_rows.iter().map(|(v, _)| v.position).collect::<Vec<_>>(),
        ));
        let ref_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(v, _)| v.ref_allele.as_str()).collect::<Vec<_>>(),
        ));
        let alt_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(v, _)| v.alt_allele.as_str()).collect::<Vec<_>>(),
        ));
        let allele_freq_array: ArrayRef = Arc::new(Float64Array::from(
            all_rows.iter().map(|(v, _)| v.allele_freq).collect::<Vec<_>>(),
        ));
        let minor_allele_freq_array: ArrayRef = Arc::new(Float64Array::from(
            all_rows.iter().map(|(v, _)| v.minor_allele_freq).collect::<Vec<_>>(),
        ));
        let is_typed_array: ArrayRef = Arc::new(UInt64Array::from(
            all_rows.iter().map(|(v, _)| if v.is_typed { 1u64 } else { 0u64 }).collect::<Vec<_>>(),
        ));
        let sample_id_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(_, s)| s.sample_id.as_str()).collect::<Vec<_>>(),
        ));
        let genotype_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(_, s)| s.genotype.as_str()).collect::<Vec<_>>(),
        ));
        let dosage_array: ArrayRef = Arc::new(Float64Array::from(
            all_rows.iter().map(|(_, s)| s.dosage).collect::<Vec<_>>(),
        ));
        let source_array: ArrayRef = Arc::new(StringArray::from(
            all_rows.iter().map(|(_, s)| s.source.as_str()).collect::<Vec<_>>(),
        ));
        let quality_array: ArrayRef = Arc::new(Float64Array::from(
            all_rows.iter().map(|(_, s)| s.imputation_quality).collect::<Vec<_>>(),
        ));

        // Create RecordBatch
        let variant_batch = RecordBatch::try_new(
            variant_schema.clone(),
            vec![
                rsid_array,
                chromosome_array,
                position_array,
                ref_array,
                alt_array,
                allele_freq_array,
                minor_allele_freq_array,
                is_typed_array,
                sample_id_array,
                genotype_array,
                dosage_array,
                source_array,
                quality_array,
            ],
        )
        .context("Failed to create Arrow RecordBatch")?;

        // Write to Parquet file with compression
        let file = std::fs::File::create(path).context("Failed to create Parquet file")?;
        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, variant_schema, Some(props))
            .context("Failed to create Parquet writer")?;

        writer
            .write(&variant_batch)
            .context("Failed to write Parquet data")?;
        writer.close().context("Failed to close Parquet writer")?;

        info!(
            "Multi-sample Parquet output complete: {} variants × 51 samples = {} rows",
            output.metadata.total_snps,
            all_rows.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate SQLite output (queryable database)
    async fn generate_sqlite(
        &self,
        path: &Path,
        output: &GeneticAnalysisOutput,
    ) -> Result<PathBuf> {
        info!("Generating SQLite output: {:?}", path);

        let mut conn = Connection::open(path).context("Failed to create SQLite database")?;

        // Create variants table
        conn.execute(
            "CREATE TABLE variants (
                rsid TEXT NOT NULL,
                chromosome INTEGER NOT NULL,
                position INTEGER NOT NULL,
                ref_allele TEXT NOT NULL,
                alt_allele TEXT NOT NULL,
                dosage REAL NOT NULL,
                source TEXT NOT NULL,
                imputation_quality REAL,
                PRIMARY KEY (chromosome, position, ref_allele, alt_allele)
            )",
            [],
        )
        .context("Failed to create variants table")?;

        // Create PGS tables
        conn.execute(
            "CREATE TABLE pgs_unscaled (
                sample_id TEXT NOT NULL,
                trait_label TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (sample_id, trait_label)
            )",
            [],
        )
        .context("Failed to create pgs_unscaled table")?;

        conn.execute(
            "CREATE TABLE pgs_scaled (
                sample_id TEXT NOT NULL,
                trait_label TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (sample_id, trait_label)
            )",
            [],
        )
        .context("Failed to create pgs_scaled table")?;

        // Create metadata table
        conn.execute(
            "CREATE TABLE metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )
        .context("Failed to create metadata table")?;

        // Insert metadata
        // Convert numeric metadata to strings (must live long enough for the loop)
        let total_snps_str = output.metadata.total_snps.to_string();
        let genotyped_snps_str = output.metadata.genotyped_snps.to_string();
        let imputed_snps_str = output.metadata.imputed_snps.to_string();
        let low_quality_snps_str = output.metadata.low_quality_snps.to_string();

        let metadata_items = vec![
            ("job_id", &output.metadata.job_id),
            ("user_id", &output.metadata.user_id),
            ("processing_date", &output.metadata.processing_date),
            ("genome_file", &output.metadata.genome_file),
            ("imputation_server", &output.metadata.imputation_server),
            ("reference_panel", &output.metadata.reference_panel),
            ("total_snps", &total_snps_str),
            ("genotyped_snps", &genotyped_snps_str),
            ("imputed_snps", &imputed_snps_str),
            ("low_quality_snps", &low_quality_snps_str),
        ];

        for (key, value) in metadata_items {
            conn.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .context("Failed to insert metadata")?;
        }

        // Insert variants in batches
        let tx = conn.transaction().context("Failed to start transaction")?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO variants (rsid, chromosome, position, ref_allele, alt_allele, dosage, source, imputation_quality)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .context("Failed to prepare variants insert statement")?;

            for (chr, variants) in &output.chromosomes {
                for variant in variants {
                    stmt.execute(params![
                        variant.rsid,
                        chr,
                        variant.position,
                        variant.ref_allele,
                        variant.alt_allele,
                        variant.dosage,
                        variant.source,
                        variant.imputation_quality,
                    ])
                    .context("Failed to insert variant")?;
                }
            }
        }
        tx.commit().context("Failed to commit variants")?;

        // Insert PGS data
        let tx = conn.transaction().context("Failed to start PGS transaction")?;
        {
            let mut stmt_unscaled = tx
                .prepare("INSERT INTO pgs_unscaled (sample_id, trait_label, value) VALUES (?1, ?2, ?3)")
                .context("Failed to prepare pgs_unscaled insert")?;

            for record in &output.pgs_unscaled {
                stmt_unscaled
                    .execute(params![
                        record.sample_id,
                        record.trait_label,
                        record.value,
                    ])
                    .context("Failed to insert PGS unscaled record")?;
            }

            let mut stmt_scaled = tx
                .prepare("INSERT INTO pgs_scaled (sample_id, trait_label, value) VALUES (?1, ?2, ?3)")
                .context("Failed to prepare pgs_scaled insert")?;

            for record in &output.pgs_scaled {
                stmt_scaled
                    .execute(params![record.sample_id, record.trait_label, record.value])
                    .context("Failed to insert PGS scaled record")?;
            }
        }
        tx.commit().context("Failed to commit PGS data")?;

        // Create indexes for efficient queries (rsid index removed - too expensive for large TEXT columns)
        conn.execute(
            "CREATE INDEX idx_variants_position ON variants(chromosome, position)",
            [],
        )
        .context("Failed to create position index")?;
        conn.execute(
            "CREATE INDEX idx_pgs_trait ON pgs_unscaled(trait_label)",
            [],
        )
        .context("Failed to create PGS trait index")?;

        info!(
            "SQLite output complete: {} variants, {} PGS traits",
            output.metadata.total_snps,
            output.metadata.pgs_traits.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate SQLite output (multi-sample: 51 samples)
    async fn generate_multi_sample_sqlite(
        &self,
        path: &Path,
        output: &MultiSampleGeneticOutput,
    ) -> Result<PathBuf> {
        info!("Generating multi-sample SQLite output (51 samples): {:?}", path);

        let mut conn = Connection::open(path).context("Failed to create SQLite database")?;

        // Create variants table with sample_id column
        // This stores 51 rows per variant (one per sample)
        conn.execute(
            "CREATE TABLE variants (
                rsid TEXT NOT NULL,
                chromosome INTEGER NOT NULL,
                position INTEGER NOT NULL,
                ref_allele TEXT NOT NULL,
                alt_allele TEXT NOT NULL,
                allele_freq REAL,
                minor_allele_freq REAL,
                is_typed INTEGER NOT NULL,
                sample_id TEXT NOT NULL,
                genotype TEXT NOT NULL,
                dosage REAL NOT NULL,
                source TEXT NOT NULL,
                imputation_quality REAL,
                PRIMARY KEY (chromosome, position, ref_allele, alt_allele, sample_id)
            )",
            [],
        )
        .context("Failed to create variants table")?;

        // Create PGS tables (same as single-sample)
        conn.execute(
            "CREATE TABLE pgs_unscaled (
                sample_id TEXT NOT NULL,
                trait_label TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (sample_id, trait_label)
            )",
            [],
        )
        .context("Failed to create pgs_unscaled table")?;

        conn.execute(
            "CREATE TABLE pgs_scaled (
                sample_id TEXT NOT NULL,
                trait_label TEXT NOT NULL,
                value REAL NOT NULL,
                PRIMARY KEY (sample_id, trait_label)
            )",
            [],
        )
        .context("Failed to create pgs_scaled table")?;

        // Create metadata table
        conn.execute(
            "CREATE TABLE metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )
        .context("Failed to create metadata table")?;

        // Insert metadata
        let total_snps_str = output.metadata.total_snps.to_string();
        let genotyped_snps_str = output.metadata.genotyped_snps.to_string();
        let imputed_snps_str = output.metadata.imputed_snps.to_string();
        let low_quality_snps_str = output.metadata.low_quality_snps.to_string();

        let metadata_items = vec![
            ("job_id", &output.metadata.job_id),
            ("user_id", &output.metadata.user_id),
            ("processing_date", &output.metadata.processing_date),
            ("genome_file", &output.metadata.genome_file),
            ("imputation_server", &output.metadata.imputation_server),
            ("reference_panel", &output.metadata.reference_panel),
            ("total_snps", &total_snps_str),
            ("genotyped_snps", &genotyped_snps_str),
            ("imputed_snps", &imputed_snps_str),
            ("low_quality_snps", &low_quality_snps_str),
        ];

        for (key, value) in metadata_items {
            conn.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .context("Failed to insert metadata")?;
        }

        // Insert variants in batches (51 rows per variant)
        let tx = conn.transaction().context("Failed to start transaction")?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO variants
                     (rsid, chromosome, position, ref_allele, alt_allele, allele_freq,
                      minor_allele_freq, is_typed, sample_id, genotype, dosage, source, imputation_quality)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                )
                .context("Failed to prepare variants insert statement")?;

            for (chr, variants) in &output.chromosomes {
                for variant in variants {
                    // Insert one row for each of the 51 samples
                    for sample in &variant.samples {
                        stmt.execute(params![
                            variant.rsid,
                            chr,
                            variant.position,
                            variant.ref_allele,
                            variant.alt_allele,
                            variant.allele_freq,
                            variant.minor_allele_freq,
                            if variant.is_typed { 1 } else { 0 },
                            sample.sample_id,
                            sample.genotype,
                            sample.dosage,
                            sample.source,
                            sample.imputation_quality,
                        ])
                        .context("Failed to insert variant sample")?;
                    }
                }
            }
        }
        tx.commit().context("Failed to commit variants")?;

        // Insert PGS data
        let tx = conn.transaction().context("Failed to start PGS transaction")?;
        {
            let mut stmt_unscaled = tx
                .prepare("INSERT INTO pgs_unscaled (sample_id, trait_label, value) VALUES (?1, ?2, ?3)")
                .context("Failed to prepare pgs_unscaled insert")?;

            for record in &output.pgs_unscaled {
                stmt_unscaled
                    .execute(params![
                        record.sample_id,
                        record.trait_label,
                        record.value,
                    ])
                    .context("Failed to insert PGS unscaled record")?;
            }

            let mut stmt_scaled = tx
                .prepare("INSERT INTO pgs_scaled (sample_id, trait_label, value) VALUES (?1, ?2, ?3)")
                .context("Failed to prepare pgs_scaled insert")?;

            for record in &output.pgs_scaled {
                stmt_scaled
                    .execute(params![record.sample_id, record.trait_label, record.value])
                    .context("Failed to insert PGS scaled record")?;
            }
        }
        tx.commit().context("Failed to commit PGS data")?;

        // Create indexes for efficient queries (rsid index removed - too expensive for large TEXT columns)
        conn.execute(
            "CREATE INDEX idx_variants_position ON variants(chromosome, position)",
            [],
        )
        .context("Failed to create position index")?;
        conn.execute(
            "CREATE INDEX idx_variants_sample ON variants(sample_id)",
            [],
        )
        .context("Failed to create sample_id index")?;
        conn.execute(
            "CREATE INDEX idx_pgs_trait ON pgs_unscaled(trait_label)",
            [],
        )
        .context("Failed to create PGS trait index")?;

        info!(
            "Multi-sample SQLite output complete: {} variants × 51 samples = {} rows, {} PGS traits",
            output.metadata.total_snps,
            output.metadata.total_snps * 51,
            output.metadata.pgs_traits.len()
        );

        Ok(path.to_path_buf())
    }

    /// Generate VCF output (bioinformatics standard)
    async fn generate_vcf(
        &self,
        path: &Path,
        output: &GeneticAnalysisOutput,
    ) -> Result<PathBuf> {
        info!("Generating VCF output: {:?}", path);

        use std::io::Write;

        // Create VCF file
        let mut file = std::fs::File::create(path).context("Failed to create VCF file")?;

        // Write VCF header manually (simpler than noodles VCF writer API)
        writeln!(file, "##fileformat=VCFv4.3")?;
        writeln!(file, "##fileDate={}", chrono::Utc::now().format("%Y%m%d"))?;
        writeln!(file, "##source=genetics-processor-v1.0.0")?;
        writeln!(file, "##INFO=<ID=DS,Number=1,Type=Float,Description=\"Dosage\">")?;
        writeln!(file, "##INFO=<ID=IQ,Number=1,Type=Float,Description=\"Imputation Quality (R²)\">")?;
        writeln!(file, "##INFO=<ID=SRC,Number=1,Type=String,Description=\"Data Source (Genotyped/Imputed/ImputedLowQual)\">")?;
        writeln!(file, "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO")?;

        // Write variants chromosome by chromosome
        for chr_num in 1..=22u8 {
            if let Some(variants) = output.chromosomes.get(&chr_num) {
                for variant in variants {
                    // Build INFO field with dosage, quality, and source
                    let mut info_string = format!("DS={:.3}", variant.dosage);
                    if let Some(qual) = variant.imputation_quality {
                        info_string.push_str(&format!(";IQ={:.3}", qual));
                    }
                    info_string.push_str(&format!(";SRC={}", variant.source));

                    // Write VCF record
                    // Format: CHROM POS ID REF ALT QUAL FILTER INFO
                    writeln!(
                        file,
                        "chr{}\t{}\t{}\t{}\t{}\t.\t.\t{}",
                        chr_num,
                        variant.position,
                        variant.rsid,
                        variant.ref_allele,
                        variant.alt_allele,
                        info_string
                    )?;
                }
            }
        }

        info!(
            "VCF output complete: {} variants across 22 chromosomes",
            output.metadata.total_snps
        );

        Ok(path.to_path_buf())
    }

    /// Generate VCF output (multi-sample: 51 samples, bioinformatics standard)
    async fn generate_multi_sample_vcf(
        &self,
        path: &Path,
        output: &MultiSampleGeneticOutput,
    ) -> Result<PathBuf> {
        info!("Generating multi-sample VCF output (51 samples): {:?}", path);

        use std::io::Write;

        // Create BGZF-compressed VCF file
        let file = std::fs::File::create(path).context("Failed to create VCF file")?;
        let mut writer = flate2::write::GzEncoder::new(file, flate2::Compression::default());

        // Write VCF header manually (simpler than noodles VCF writer API)
        writeln!(writer, "##fileformat=VCFv4.3")?;
        writeln!(writer, "##fileDate={}", chrono::Utc::now().format("%Y%m%d"))?;
        writeln!(writer, "##source=genetics-processor-v1.0.0")?;
        writeln!(writer, "##INFO=<ID=AF,Number=A,Type=Float,Description=\"Allele Frequency\">")?;
        writeln!(writer, "##INFO=<ID=MAF,Number=1,Type=Float,Description=\"Minor Allele Frequency\">")?;
        writeln!(writer, "##INFO=<ID=TYPED,Number=0,Type=Flag,Description=\"Variant was genotyped (not imputed)\">")?;
        writeln!(writer, "##FORMAT=<ID=GT,Number=1,Type=String,Description=\"Genotype\">")?;
        writeln!(writer, "##FORMAT=<ID=DS,Number=1,Type=Float,Description=\"Dosage\">")?;
        writeln!(writer, "##FORMAT=<ID=IQ,Number=1,Type=Float,Description=\"Imputation Quality (R²)\">")?;

        // Build sample list from first variant (all variants have same samples)
        let sample_ids: Vec<String> = if let Some(first_chr_variants) = output.chromosomes.values().next() {
            if let Some(first_variant) = first_chr_variants.first() {
                first_variant.samples.iter().map(|s| s.sample_id.clone()).collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Write header line with sample IDs
        write!(writer, "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT")?;
        for sample_id in &sample_ids {
            write!(writer, "\t{}", sample_id)?;
        }
        writeln!(writer)?;

        // Write variants chromosome by chromosome
        for chr_num in 1..=22u8 {
            if let Some(variants) = output.chromosomes.get(&chr_num) {
                for variant in variants {
                    // Build INFO field with allele frequencies
                    let mut info_parts = Vec::new();
                    if let Some(af) = variant.allele_freq {
                        info_parts.push(format!("AF={:.4}", af));
                    }
                    if let Some(maf) = variant.minor_allele_freq {
                        info_parts.push(format!("MAF={:.4}", maf));
                    }
                    if variant.is_typed {
                        info_parts.push("TYPED".to_string());
                    }
                    let info_string = if info_parts.is_empty() {
                        ".".to_string()
                    } else {
                        info_parts.join(";")
                    };

                    // Write VCF record: CHROM POS ID REF ALT QUAL FILTER INFO FORMAT [SAMPLES...]
                    write!(
                        writer,
                        "chr{}\t{}\t{}\t{}\t{}\t.\t.\t{}\tGT:DS:IQ",
                        chr_num,
                        variant.position,
                        variant.rsid,
                        variant.ref_allele,
                        variant.alt_allele,
                        info_string
                    )?;

                    // Write sample genotypes
                    for sample in &variant.samples {
                        let iq_str = sample
                            .imputation_quality
                            .map(|q| format!("{:.3}", q))
                            .unwrap_or_else(|| ".".to_string());

                        write!(
                            writer,
                            "\t{}:{:.3}:{}",
                            sample.genotype,
                            sample.dosage,
                            iq_str
                        )?;
                    }
                    writeln!(writer)?;
                }
            }
        }

        // Finalize gzip stream
        writer.finish().context("Failed to finalize gzip compression")?;

        info!(
            "Multi-sample VCF output complete: {} variants × 51 samples across 22 chromosomes",
            output.metadata.total_snps
        );

        Ok(path.to_path_buf())
    }

    // ========================================================================
    // STREAMING OUTPUT METHODS
    // ========================================================================
    // These methods support incremental chromosome processing to avoid
    // accumulating all 22 chromosomes in memory at once.
    //
    // Usage:
    //   1. Call initialize_streaming_output() with desired formats
    //   2. For each chromosome 1-22:
    //      - Process chromosome data
    //      - Call append_chromosome() immediately
    //      - Drop chromosome data from memory
    //   3. Call finalize_streaming_output() to close files and get paths
    // ========================================================================

    /// Initialize streaming output for incremental chromosome processing
    ///
    /// This creates output files and writes headers/schemas but doesn't
    /// write any variant data yet.
    ///
    /// # Arguments
    /// * `formats` - List of output formats to generate
    /// * `vcf_format` - VCF format preference (merged or per-chromosome)
    ///
    /// # Returns
    /// * Result indicating success or failure
    pub async fn initialize_streaming_output(
        &mut self,
        formats: &[OutputFormat],
        vcf_format: VcfFormat,
    ) -> Result<()> {
        use std::io::Write;

        info!("Initializing streaming output for {} formats", formats.len());

        // Create output directory
        std::fs::create_dir_all(&self.output_dir)?;

        // Initialize streaming state
        let mut state = StreamingState {
            formats: formats.to_vec(),
            vcf_format,
            sqlite_conn: None,
            sqlite_path: None,
            json_file: None,
            json_path: None,
            json_first_chromosome: true,
            vcf_file: None,
            vcf_path: None,
            vcf_header_written: false,
            vcf_files: Vec::new(),
            vcf_base_path: None,
            parquet_files: Vec::new(),
            parquet_base_path: None,
            total_variants: 0,
            genotyped_variants: 0,
            low_quality_variants: 0,
            chromosomes_processed: 0,
        };

        // Initialize each format
        for format in formats {
            if !format.is_implemented() {
                info!("Skipping unimplemented format: {:?}", format);
                continue;
            }

            match format {
                OutputFormat::Sqlite => {
                    let filename = format!("GenomicData_{}_51samples.{}", self.job_id, format.extension());
                    let path = self.output_dir.join(&filename);

                    info!("Initializing SQLite database: {:?}", path);
                    let conn = Connection::open(&path)
                        .context("Failed to create SQLite database")?;

                    // Optimize SQLite settings for large dataset
                    // Note: Using execute_batch for PRAGMA statements (handles return values automatically)
                    conn.execute_batch(
                        "PRAGMA page_size = 32768;        -- 32KB pages (vs 4KB default) reduces fragmentation
                         PRAGMA journal_mode = OFF;       -- Disable WAL journal for faster bulk insert (one-time write)
                         PRAGMA synchronous = OFF;        -- Disable fsync for speed (safe for one-time write)
                         PRAGMA cache_size = -2000000;    -- 2GB cache (negative = KB)
                         PRAGMA locking_mode = EXCLUSIVE; -- Exclusive mode for better write performance
                         PRAGMA temp_store = MEMORY;"     // Keep temp tables in RAM
                    ).context("Failed to set SQLite optimizations")?;

                    // Create variants table WITHOUT PRIMARY KEY to save space
                    // PRIMARY KEY creates huge B-tree index with TEXT fields
                    conn.execute(
                        "CREATE TABLE variants (
                            rsid TEXT NOT NULL,
                            chromosome INTEGER NOT NULL,
                            position INTEGER NOT NULL,
                            ref_allele TEXT NOT NULL,
                            alt_allele TEXT NOT NULL,
                            allele_freq REAL,
                            minor_allele_freq REAL,
                            is_typed INTEGER NOT NULL,
                            sample_id TEXT NOT NULL,
                            genotype TEXT NOT NULL,
                            dosage REAL NOT NULL,
                            source TEXT NOT NULL,
                            imputation_quality REAL
                        )",
                        [],
                    )
                    .context("Failed to create variants table")?;

                    // Create PGS tables (empty for now)
                    conn.execute(
                        "CREATE TABLE pgs_unscaled (
                            sample_id TEXT NOT NULL,
                            trait_label TEXT NOT NULL,
                            value REAL NOT NULL,
                            PRIMARY KEY (sample_id, trait_label)
                        )",
                        [],
                    )
                    .context("Failed to create pgs_unscaled table")?;

                    conn.execute(
                        "CREATE TABLE pgs_scaled (
                            sample_id TEXT NOT NULL,
                            trait_label TEXT NOT NULL,
                            value REAL NOT NULL,
                            PRIMARY KEY (sample_id, trait_label)
                        )",
                        [],
                    )
                    .context("Failed to create pgs_scaled table")?;

                    // Create metadata table (will populate in finalize)
                    conn.execute(
                        "CREATE TABLE metadata (
                            key TEXT PRIMARY KEY,
                            value TEXT NOT NULL
                        )",
                        [],
                    )
                    .context("Failed to create metadata table")?;

                    state.sqlite_conn = Some(conn);
                    state.sqlite_path = Some(path);
                }
                OutputFormat::Json => {
                    // JSON format disabled - 29GB JSON file causes OOM during finalization
                    // Users can generate JSON from SQLite/Parquet/VCF if needed
                    info!("Skipping JSON format (too large for memory-efficient streaming)");
                    continue;
                }
                OutputFormat::Vcf => {
                    match state.vcf_format {
                        VcfFormat::Merged => {
                            // Single merged VCF file for all chromosomes
                            let filename = format!("GenomicData_{}_51samples.{}", self.job_id, format.extension());
                            let path = self.output_dir.join(&filename);

                            info!("Initializing merged VCF file (gzip-compressed): {:?}", path);
                            let file = std::fs::File::create(&path)
                                .context("Failed to create VCF file")?;
                            let mut writer = flate2::write::GzEncoder::new(file, flate2::Compression::default());

                            // Write VCF header
                            writeln!(writer, "##fileformat=VCFv4.3")?;
                            writeln!(writer, "##fileDate={}", chrono::Utc::now().format("%Y%m%d"))?;
                            writeln!(writer, "##source=genetics-processor-v1.0.0")?;
                            writeln!(writer, "##INFO=<ID=AF,Number=A,Type=Float,Description=\"Allele Frequency\">")?;
                            writeln!(writer, "##INFO=<ID=MAF,Number=1,Type=Float,Description=\"Minor Allele Frequency\">")?;
                            writeln!(writer, "##INFO=<ID=TYPED,Number=0,Type=Flag,Description=\"Variant was genotyped (not imputed)\">")?;
                            writeln!(writer, "##FORMAT=<ID=GT,Number=1,Type=String,Description=\"Genotype\">")?;
                            writeln!(writer, "##FORMAT=<ID=DS,Number=1,Type=Float,Description=\"Dosage\">")?;
                            writeln!(writer, "##FORMAT=<ID=IQ,Number=1,Type=Float,Description=\"Imputation Quality (R²)\">")?;

                            // Write header line with sample IDs (samp1-samp50 + user)
                            write!(writer, "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT")?;
                            for i in 1..=50 {
                                write!(writer, "\tsamp{}", i)?;
                            }
                            writeln!(writer, "\tuser")?;

                            state.vcf_file = Some(writer);
                            state.vcf_path = Some(path);
                            state.vcf_header_written = true;
                        }
                        VcfFormat::PerChromosome => {
                            // Per-chromosome VCF files will be created on-the-fly in append_chromosome()
                            let base_name = format!("GenomicData_{}_51samples", self.job_id);
                            let base_path = self.output_dir.join(&base_name);

                            info!("Initializing per-chromosome VCF files (will create chr1.vcf.gz, chr2.vcf.gz, etc.)");
                            state.vcf_base_path = Some(base_path);
                        }
                    }
                }
                OutputFormat::Parquet => {
                    // For Parquet, we'll create per-chromosome files and concatenate later
                    let base_name = format!("GenomicData_{}_51samples", self.job_id);
                    let base_path = self.output_dir.join(&base_name);

                    info!("Initializing Parquet streaming (per-chromosome files): {:?}", base_path);
                    state.parquet_base_path = Some(base_path);
                }
                OutputFormat::RData => {
                    // Not implemented
                    continue;
                }
            }
        }

        self.streaming_state = Some(state);
        info!("Streaming output initialized successfully");
        Ok(())
    }

    /// Append one chromosome's variants to streaming output
    ///
    /// This writes variant data immediately to output files/databases.
    /// After this call, the chromosome data can be dropped from memory.
    ///
    /// # Arguments
    /// * `chromosome` - Chromosome number (1-22)
    /// * `variants` - Variants for this chromosome
    ///
    /// # Returns
    /// * Result indicating success or failure
    pub async fn append_chromosome(
        &mut self,
        chromosome: u8,
        variants: &[MultiSampleVariant],
    ) -> Result<()> {
        use std::io::Write;

        let state = self.streaming_state.as_mut()
            .ok_or_else(|| anyhow::anyhow!("Streaming not initialized. Call initialize_streaming_output() first."))?;

        info!("Appending chromosome {} ({} variants) to streaming output", chromosome, variants.len());

        // Update metadata
        state.total_variants += variants.len();
        state.genotyped_variants += variants.iter().filter(|v| v.is_typed).count();
        state.low_quality_variants += variants
            .iter()
            .filter(|v| {
                // Check user sample (last sample, index 50)
                if let Some(user_sample) = v.samples.get(50) {
                    matches!(user_sample.source, DataSource::ImputedLowQual)
                } else {
                    false
                }
            })
            .count();
        state.chromosomes_processed += 1;

        // Append to each format
        for format in state.formats.clone() {
            match format {
                OutputFormat::Sqlite => {
                    if let Some(conn) = &mut state.sqlite_conn {
                        info!("  Appending chromosome {} to SQLite ({} variants × 51 samples = {} rows)",
                              chromosome, variants.len(), variants.len() * 51);

                        let tx = conn.transaction()
                            .context("Failed to start SQLite transaction")?;
                        {
                            let mut stmt = tx.prepare(
                                "INSERT OR REPLACE INTO variants
                                 (rsid, chromosome, position, ref_allele, alt_allele, allele_freq,
                                  minor_allele_freq, is_typed, sample_id, genotype, dosage, source, imputation_quality)
                                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                            )
                            .context("Failed to prepare variants insert statement")?;

                            for variant in variants {
                                // Insert one row for each of the 51 samples
                                for sample in &variant.samples {
                                    stmt.execute(params![
                                        variant.rsid,
                                        chromosome,
                                        variant.position,
                                        variant.ref_allele,
                                        variant.alt_allele,
                                        variant.allele_freq,
                                        variant.minor_allele_freq,
                                        if variant.is_typed { 1 } else { 0 },
                                        sample.sample_id,
                                        sample.genotype,
                                        sample.dosage,
                                        format!("{:?}", sample.source),
                                        sample.imputation_quality,
                                    ])
                                    .context("Failed to insert variant sample")?;
                                }
                            }
                        }
                        tx.commit().context("Failed to commit variants")?;
                        info!("  ✓ SQLite chromosome {} committed", chromosome);
                    }
                }
                OutputFormat::Json => {
                    // JSON format disabled - skipping
                    continue;
                }
                OutputFormat::Vcf => {
                    match state.vcf_format {
                        VcfFormat::Merged => {
                            // Append to single merged VCF file
                            if let Some(file) = &mut state.vcf_file {
                                info!("  Appending chromosome {} to merged VCF", chromosome);

                                for variant in variants {
                                    // Build INFO field
                                    let mut info_parts = Vec::new();
                                    if let Some(af) = variant.allele_freq {
                                        info_parts.push(format!("AF={:.4}", af));
                                    }
                                    if let Some(maf) = variant.minor_allele_freq {
                                        info_parts.push(format!("MAF={:.4}", maf));
                                    }
                                    if variant.is_typed {
                                        info_parts.push("TYPED".to_string());
                                    }
                                    let info_string = if info_parts.is_empty() {
                                        ".".to_string()
                                    } else {
                                        info_parts.join(";")
                                    };

                                    // Write VCF record
                                    write!(
                                        file,
                                        "chr{}\t{}\t{}\t{}\t{}\t.\t.\t{}\tGT:DS:IQ",
                                        chromosome,
                                        variant.position,
                                        variant.rsid,
                                        variant.ref_allele,
                                        variant.alt_allele,
                                        info_string
                                    )?;

                                    // Write sample genotypes
                                    for sample in &variant.samples {
                                        let iq_str = sample
                                            .imputation_quality
                                            .map(|q| format!("{:.3}", q))
                                            .unwrap_or_else(|| ".".to_string());

                                        write!(
                                            file,
                                            "\t{}:{:.3}:{}",
                                            sample.genotype,
                                            sample.dosage,
                                            iq_str
                                        )?;
                                    }
                                    writeln!(file)?;
                                }
                                info!("  ✓ VCF chromosome {} written to merged file", chromosome);
                            }
                        }
                        VcfFormat::PerChromosome => {
                            // Create separate VCF file for this chromosome
                            if let Some(base_path) = &state.vcf_base_path {
                                info!("  Writing chromosome {} to separate VCF file", chromosome);

                                // Extract filename stem (without .vcf.gz double extension)
                                let full_name = base_path.file_name().unwrap().to_str().unwrap();
                                let base_filename = full_name.trim_end_matches(".vcf.gz");
                                let chr_filename = format!("{}_chr{}.vcf.gz", base_filename, chromosome);
                                let chr_path = base_path.parent().unwrap().join(&chr_filename);

                                // Create chromosome-specific VCF file
                                let file = std::fs::File::create(&chr_path)
                                    .context("Failed to create per-chromosome VCF file")?;
                                let mut writer = flate2::write::GzEncoder::new(file, flate2::Compression::default());

                                // Write VCF header
                                writeln!(writer, "##fileformat=VCFv4.3")?;
                                writeln!(writer, "##fileDate={}", chrono::Utc::now().format("%Y%m%d"))?;
                                writeln!(writer, "##source=genetics-processor-v1.0.0")?;
                                writeln!(writer, "##INFO=<ID=AF,Number=A,Type=Float,Description=\"Allele Frequency\">")?;
                                writeln!(writer, "##INFO=<ID=MAF,Number=1,Type=Float,Description=\"Minor Allele Frequency\">")?;
                                writeln!(writer, "##INFO=<ID=TYPED,Number=0,Type=Flag,Description=\"Variant was genotyped (not imputed)\">")?;
                                writeln!(writer, "##FORMAT=<ID=GT,Number=1,Type=String,Description=\"Genotype\">")?;
                                writeln!(writer, "##FORMAT=<ID=DS,Number=1,Type=Float,Description=\"Dosage\">")?;
                                writeln!(writer, "##FORMAT=<ID=IQ,Number=1,Type=Float,Description=\"Imputation Quality (R²)\">")?;

                                // Write header line with sample IDs (samp1-samp50 + user)
                                write!(writer, "#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT")?;
                                for i in 1..=50 {
                                    write!(writer, "\tsamp{}", i)?;
                                }
                                writeln!(writer, "\tuser")?;

                                // Write variants for this chromosome
                                for variant in variants {
                                    // Build INFO field
                                    let mut info_parts = Vec::new();
                                    if let Some(af) = variant.allele_freq {
                                        info_parts.push(format!("AF={:.4}", af));
                                    }
                                    if let Some(maf) = variant.minor_allele_freq {
                                        info_parts.push(format!("MAF={:.4}", maf));
                                    }
                                    if variant.is_typed {
                                        info_parts.push("TYPED".to_string());
                                    }
                                    let info_string = if info_parts.is_empty() {
                                        ".".to_string()
                                    } else {
                                        info_parts.join(";")
                                    };

                                    // Write VCF record
                                    write!(
                                        writer,
                                        "chr{}\t{}\t{}\t{}\t{}\t.\t.\t{}\tGT:DS:IQ",
                                        chromosome,
                                        variant.position,
                                        variant.rsid,
                                        variant.ref_allele,
                                        variant.alt_allele,
                                        info_string
                                    )?;

                                    // Write sample genotypes
                                    for sample in &variant.samples {
                                        let iq_str = sample
                                            .imputation_quality
                                            .map(|q| format!("{:.3}", q))
                                            .unwrap_or_else(|| ".".to_string());

                                        write!(
                                            writer,
                                            "\t{}:{:.3}:{}",
                                            sample.genotype,
                                            sample.dosage,
                                            iq_str
                                        )?;
                                    }
                                    writeln!(writer)?;
                                }

                                // Finalize gzip compression
                                writer.finish().context("Failed to finalize per-chromosome VCF gzip compression")?;

                                state.vcf_files.push(chr_path.clone());
                                info!("  ✓ VCF chromosome {} written to {:?}", chromosome, chr_path);
                            }
                        }
                    }
                }
                OutputFormat::Parquet => {
                    if let Some(base_path) = &state.parquet_base_path {
                        info!("  Writing chromosome {} to Parquet file", chromosome);

                        let chr_filename = format!("{}_chr{}.parquet",
                            base_path.file_name().unwrap().to_str().unwrap(),
                            chromosome);
                        let chr_path = base_path.parent().unwrap().join(&chr_filename);

                        // Create Arrow schema
                        let variant_schema = Arc::new(Schema::new(vec![
                            Field::new("rsid", DataType::Utf8, false),
                            Field::new("chromosome", DataType::UInt64, false),
                            Field::new("position", DataType::UInt64, false),
                            Field::new("ref_allele", DataType::Utf8, false),
                            Field::new("alt_allele", DataType::Utf8, false),
                            Field::new("allele_freq", DataType::Float64, true),
                            Field::new("minor_allele_freq", DataType::Float64, true),
                            Field::new("is_typed", DataType::UInt64, false),
                            Field::new("sample_id", DataType::Utf8, false),
                            Field::new("genotype", DataType::Utf8, false),
                            Field::new("dosage", DataType::Float64, false),
                            Field::new("source", DataType::Utf8, false),
                            Field::new("imputation_quality", DataType::Float64, true),
                        ]));

                        // Create Parquet writer once
                        let file = std::fs::File::create(&chr_path)
                            .context("Failed to create Parquet file")?;
                        let props = WriterProperties::builder()
                            .set_compression(parquet::basic::Compression::SNAPPY)
                            .build();

                        let mut writer = ArrowWriter::try_new(file, variant_schema.clone(), Some(props))
                            .context("Failed to create Parquet writer")?;

                        // Write in batches to avoid OOM (10,000 variants at a time)
                        const BATCH_SIZE: usize = 10_000;
                        let total_variants = variants.len();
                        let mut batches_written = 0;

                        for chunk_start in (0..total_variants).step_by(BATCH_SIZE) {
                            let chunk_end = std::cmp::min(chunk_start + BATCH_SIZE, total_variants);
                            let variant_chunk = &variants[chunk_start..chunk_end];

                            // Flatten chunk variants and samples into rows
                            let mut chunk_rows: Vec<(&MultiSampleVariant, &SampleData)> = Vec::new();
                            for variant in variant_chunk {
                                for sample in &variant.samples {
                                    chunk_rows.push((variant, sample));
                                }
                            }

                            // Build Arrow arrays for this chunk only
                            let rsid_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(v, _)| v.rsid.as_str()).collect::<Vec<_>>(),
                            ));
                            let chromosome_array: ArrayRef = Arc::new(UInt64Array::from(
                                chunk_rows.iter().map(|(v, _)| v.chromosome as u64).collect::<Vec<_>>(),
                            ));
                            let position_array: ArrayRef = Arc::new(UInt64Array::from(
                                chunk_rows.iter().map(|(v, _)| v.position).collect::<Vec<_>>(),
                            ));
                            let ref_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(v, _)| v.ref_allele.as_str()).collect::<Vec<_>>(),
                            ));
                            let alt_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(v, _)| v.alt_allele.as_str()).collect::<Vec<_>>(),
                            ));
                            let allele_freq_array: ArrayRef = Arc::new(Float64Array::from(
                                chunk_rows.iter().map(|(v, _)| v.allele_freq).collect::<Vec<_>>(),
                            ));
                            let minor_allele_freq_array: ArrayRef = Arc::new(Float64Array::from(
                                chunk_rows.iter().map(|(v, _)| v.minor_allele_freq).collect::<Vec<_>>(),
                            ));
                            let is_typed_array: ArrayRef = Arc::new(UInt64Array::from(
                                chunk_rows.iter().map(|(v, _)| if v.is_typed { 1u64 } else { 0u64 }).collect::<Vec<_>>(),
                            ));
                            let sample_id_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(_, s)| s.sample_id.as_str()).collect::<Vec<_>>(),
                            ));
                            let genotype_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(_, s)| s.genotype.as_str()).collect::<Vec<_>>(),
                            ));
                            let dosage_array: ArrayRef = Arc::new(Float64Array::from(
                                chunk_rows.iter().map(|(_, s)| s.dosage).collect::<Vec<_>>(),
                            ));
                            let source_array: ArrayRef = Arc::new(StringArray::from(
                                chunk_rows.iter().map(|(_, s)| format!("{:?}", s.source)).collect::<Vec<_>>(),
                            ));
                            let quality_array: ArrayRef = Arc::new(Float64Array::from(
                                chunk_rows.iter().map(|(_, s)| s.imputation_quality).collect::<Vec<_>>(),
                            ));

                            // Create RecordBatch for this chunk
                            let variant_batch = RecordBatch::try_new(
                                variant_schema.clone(),
                                vec![
                                    rsid_array,
                                    chromosome_array,
                                    position_array,
                                    ref_array,
                                    alt_array,
                                    allele_freq_array,
                                    minor_allele_freq_array,
                                    is_typed_array,
                                    sample_id_array,
                                    genotype_array,
                                    dosage_array,
                                    source_array,
                                    quality_array,
                                ],
                            )
                            .context("Failed to create Arrow RecordBatch")?;

                            // Write this batch immediately
                            writer.write(&variant_batch)
                                .context("Failed to write Parquet batch")?;

                            batches_written += 1;
                            // Arrays and chunk_rows will be dropped here, freeing memory
                        }

                        // Close writer
                        writer.close()
                            .context("Failed to close Parquet writer")?;

                        state.parquet_files.push(chr_path.clone());
                        info!("  ✓ Parquet chromosome {} written to {:?} ({} batches, {} total rows)",
                              chromosome, chr_path, batches_written, variants.len() * 51);
                    }
                }
                OutputFormat::RData => {
                    // Not implemented
                    continue;
                }
            }
        }

        info!("✓ Chromosome {} appended to all formats ({} total variants accumulated)",
              chromosome, state.total_variants);
        Ok(())
    }

    /// Finalize streaming output and return file paths
    ///
    /// This closes all file handles, writes metadata, creates indexes, and
    /// returns the paths to the completed output files.
    ///
    /// # Returns
    /// * HashMap of format -> file path
    pub async fn finalize_streaming_output(&mut self) -> Result<HashMap<OutputFormat, PathBuf>> {
        let mut state = self.streaming_state.take()
            .ok_or_else(|| anyhow::anyhow!("Streaming not initialized."))?;

        info!("Finalizing streaming output...");
        info!("Total accumulated: {} variants across {} chromosomes",
              state.total_variants, state.chromosomes_processed);

        let mut result = HashMap::new();

        // Finalize each format
        for format in &state.formats {
            match format {
                OutputFormat::Sqlite => {
                    if let (Some(mut conn), Some(path)) = (state.sqlite_conn.take(), state.sqlite_path.take()) {
                        info!("Finalizing SQLite database...");

                        // Insert metadata
                        let total_snps_str = state.total_variants.to_string();
                        let genotyped_snps_str = state.genotyped_variants.to_string();
                        let imputed_snps_str = (state.total_variants - state.genotyped_variants).to_string();
                        let low_quality_snps_str = state.low_quality_variants.to_string();
                        let processing_date = chrono::Utc::now().to_rfc3339();
                        let genome_file = "23andMe genome data".to_string();
                        let imputation_server = "Michigan Imputation Server 2".to_string();
                        let reference_panel = "openSNP (50 samples) + user (1 sample) = 51 total".to_string();

                        let metadata_items = vec![
                            ("job_id", &self.job_id),
                            ("user_id", &self.user_id),
                            ("processing_date", &processing_date),
                            ("genome_file", &genome_file),
                            ("imputation_server", &imputation_server),
                            ("reference_panel", &reference_panel),
                            ("total_snps", &total_snps_str),
                            ("genotyped_snps", &genotyped_snps_str),
                            ("imputed_snps", &imputed_snps_str),
                            ("low_quality_snps", &low_quality_snps_str),
                        ];

                        for (key, value) in metadata_items {
                            conn.execute(
                                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                                params![key, value],
                            )
                            .context("Failed to insert metadata")?;
                        }

                        // Create indexes (rsid index removed - too expensive for 300M+ TEXT rows)
                        info!("Creating SQLite indexes...");
                        conn.execute(
                            "CREATE INDEX idx_variants_position ON variants(chromosome, position)",
                            [],
                        )
                        .context("Failed to create position index")?;
                        conn.execute(
                            "CREATE INDEX idx_variants_sample ON variants(sample_id)",
                            [],
                        )
                        .context("Failed to create sample_id index")?;

                        // Re-enable safety features and reclaim free space
                        info!("Optimizing SQLite database (VACUUM)...");
                        conn.execute_batch(
                            "PRAGMA journal_mode = DELETE;  -- Re-enable WAL journal
                             PRAGMA synchronous = FULL;     -- Re-enable fsync for durability
                             VACUUM;"                       // Reclaim free space and apply page_size
                        ).context("Failed to optimize database")?;

                        // Close connection
                        drop(conn);

                        info!("✓ SQLite finalized: {} variants × 51 samples = {} rows",
                              state.total_variants, state.total_variants * 51);
                        result.insert(*format, path);
                    }
                }
                OutputFormat::Json => {
                    // JSON format disabled - skipping finalization
                    info!("Skipping JSON finalization (format disabled)");
                    continue;
                }
                OutputFormat::Vcf => {
                    match state.vcf_format {
                        VcfFormat::Merged => {
                            // Finalize single merged VCF file
                            if let (Some(writer), Some(path)) = (state.vcf_file.take(), state.vcf_path.take()) {
                                info!("Finalizing merged VCF file (flushing gzip compression)...");

                                // Finalize gzip compression
                                writer.finish().context("Failed to finalize VCF gzip compression")?;

                                info!("✓ VCF finalized: {} variants × 51 samples in single merged file", state.total_variants);
                                result.insert(*format, path);
                            }
                        }
                        VcfFormat::PerChromosome => {
                            // Per-chromosome VCF files are already finalized during append_chromosome()
                            info!("✓ VCF finalized: Keeping {} per-chromosome VCF files", state.vcf_files.len());

                            for (idx, chr_file) in state.vcf_files.iter().enumerate() {
                                info!("  chr{}: {:?}", idx + 1, chr_file.file_name().unwrap());
                            }

                            // All chromosome files will be included in ZIP archive automatically
                            // Return the first file path as the representative path
                            if let Some(first_file) = state.vcf_files.first() {
                                result.insert(*format, first_file.clone());
                            }
                        }
                    }
                }
                OutputFormat::Parquet => {
                    if let Some(base_path) = &state.parquet_base_path {
                        info!("Finalizing Parquet files ({} chromosome files)...", state.parquet_files.len());

                        // Keep per-chromosome Parquet files (partitioned format)
                        // This improves query performance for chromosome-specific analyses
                        // Users can filter by chromosome column without scanning all data
                        info!("✓ Parquet finalized: Keeping {} partitioned chromosome files for optimal query performance",
                              state.parquet_files.len());

                        for (idx, chr_file) in state.parquet_files.iter().enumerate() {
                            info!("  chr{}: {:?}", idx + 1, chr_file.file_name().unwrap());
                        }

                        // All chromosome files will be included in ZIP archive automatically
                        // Return the first file path as the representative path
                        let first_file = state.parquet_files.first()
                            .context("No Parquet files generated")?;
                        result.insert(*format, first_file.clone());
                    }
                }
                OutputFormat::RData => {
                    // Not implemented
                    continue;
                }
            }
        }

        info!("✓ Streaming output finalized successfully");
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_extension() {
        assert_eq!(OutputFormat::Parquet.extension(), "parquet");
        assert_eq!(OutputFormat::Json.extension(), "json");
        assert_eq!(OutputFormat::Sqlite.extension(), "db");
        assert_eq!(OutputFormat::Vcf.extension(), "vcf.gz");
        assert_eq!(OutputFormat::RData.extension(), "RData");
    }

    #[test]
    fn test_output_format_mime_type() {
        assert_eq!(
            OutputFormat::Json.mime_type(),
            "application/json"
        );
        assert_eq!(
            OutputFormat::Parquet.mime_type(),
            "application/vnd.apache.parquet"
        );
    }

    #[test]
    fn test_output_format_serde() {
        // Test JSON serialization
        let format = OutputFormat::Json;
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, "\"json\"");

        // Test deserialization
        let parsed: OutputFormat = serde_json::from_str("\"parquet\"").unwrap();
        assert_eq!(parsed, OutputFormat::Parquet);
    }
}
