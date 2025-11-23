// ==============================================================================
// processor.rs - Core Genetic Data Processing Logic
// ==============================================================================
// Description: Merges 23andMe data with imputed VCF files and 50-sample reference panel
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2025-11-12
// Version: 2.0.0
// ==============================================================================

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use sqlx::PgPool;
use tracing::{info, debug};
use uuid::Uuid;

use crate::secure_delete;
use crate::audit;
use crate::parsers::{Genome23Parser, Genome23Record, PgsParser, PgsDataset, VCFParser};
use crate::genotype_converter::genotype_to_dosage;
use crate::models::{MultiSampleVariant, SampleData, QualityThreshold};
use crate::reference_panel::ReferencePanelReader;

// Re-export for backward compatibility with worker
pub use crate::models::{DataSource, MergedVariant};

pub struct GeneticsProcessor {
    job_id: Uuid,
    user_id: String,
    data_dir: PathBuf,
    reference_path: PathBuf,
    db_pool: PgPool,
    quality_threshold: QualityThreshold,
}

impl GeneticsProcessor {
    pub fn new(
        job_id: Uuid,
        user_id: String,
        data_dir: PathBuf,
        reference_path: PathBuf,
        db_pool: PgPool,
        quality_threshold: QualityThreshold,
    ) -> Self {
        Self {
            job_id,
            user_id,
            data_dir,
            reference_path,
            db_pool,
            quality_threshold,
        }
    }

    /// Main processing pipeline
    pub async fn process(&self) -> Result<PathBuf> {
        info!("Starting 51-sample genetic data processing for job {}", self.job_id);
        info!("Quality threshold: {:?}", self.quality_threshold);

        // 1. Locate input files
        let processing_dir = self.get_processing_dir();
        let files = self.locate_input_files(&processing_dir).await?;

        // 2. Validate all files are present
        self.validate_file_set(&files)?;

        // 3. Open reference panel database
        info!("Opening reference panel database: {:?}", self.reference_path);
        let reference_panel = ReferencePanelReader::open(&self.reference_path)
            .context("Failed to open reference panel database")?;

        reference_panel.validate()
            .context("Reference panel validation failed")?;

        // 4. Parse 23andMe data
        info!("Parsing 23andMe data");
        let _user_genome = self.parse_23andme(&files.genome_file).await?;

        // 5. Process each chromosome (50 reference + 1 user = 51 samples)
        info!("Processing 22 chromosomes with 51-sample merge");
        let mut merged_chromosomes: HashMap<u8, Vec<MultiSampleVariant>> = HashMap::new();

        for chr in 1..=22 {
            let merged = self.process_chromosome(chr, &files, &reference_panel).await?;
            merged_chromosomes.insert(chr, merged);
        }

        // Calculate total statistics
        let total_variants: usize = merged_chromosomes.values().map(|v| v.len()).sum();

        // Count how many variants have user data as "Genotyped"
        let user_genotyped: usize = merged_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|variant| {
                variant.samples.iter()
                    .find(|s| s.sample_id == "samp51")
                    .map(|s| s.source == DataSource::Genotyped)
                    .unwrap_or(false)
            })
            .count();

        info!(
            "Chromosome processing complete: {} total variants ({} user genotyped)",
            total_variants, user_genotyped
        );

        // 6. Process PGS scores
        info!("Processing polygenic scores");
        let pgs_data = self.process_pgs_scores(&files.pgs_file).await?;

        // 7. Generate output file
        info!("Generating output file");
        let result_path = self.generate_output_file(&merged_chromosomes, &pgs_data).await?;

        // 8. Securely delete all input files
        info!("Securely deleting input files");
        self.secure_delete_inputs(&files).await?;

        // 9. Clean up processing directory
        info!("Cleaning up processing directory");
        std::fs::remove_dir_all(&processing_dir)
            .context("Failed to remove processing directory")?;

        info!("Processing complete, result: {:?}", result_path);
        Ok(result_path)
    }

    fn get_processing_dir(&self) -> PathBuf {
        self.data_dir
            .join("processing")
            .join(&self.user_id)
            .join(self.job_id.to_string())
    }

    async fn locate_input_files(&self, dir: &Path) -> Result<InputFiles> {
        debug!("Locating input files in {:?}", dir);

        let mut genome_file = None;
        let mut vcf_files = Vec::new();
        let mut pgs_file = None;

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_string_lossy();

            if file_name.starts_with("genome_") && file_name.ends_with(".txt") {
                genome_file = Some(path.clone());
            } else if file_name.starts_with("chr") && file_name.ends_with(".dose.vcf.gz") {
                vcf_files.push(path.clone());
            } else if file_name == "scores.txt" {
                pgs_file = Some(path.clone());
            }
        }

        Ok(InputFiles {
            genome_file: genome_file.ok_or_else(|| anyhow::anyhow!("23andMe genome file not found"))?,
            vcf_files,
            pgs_file: pgs_file.ok_or_else(|| anyhow::anyhow!("PGS scores file not found"))?,
        })
    }

    fn validate_file_set(&self, files: &InputFiles) -> Result<()> {
        // Must have exactly 22 VCF files (one per chromosome)
        if files.vcf_files.len() != 22 {
            anyhow::bail!(
                "Expected 22 VCF files, found {}",
                files.vcf_files.len()
            );
        }

        // Check each chromosome VCF exists
        for chr in 1..=22 {
            let expected = format!("chr{}.dose.vcf.gz", chr);
            let found = files.vcf_files.iter().any(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(&expected)
            });

            if !found {
                anyhow::bail!("Missing VCF file for chromosome {}", chr);
            }
        }

        Ok(())
    }


    async fn parse_23andme(&self, path: &Path) -> Result<UserGenomeData> {
        info!("Parsing 23andMe genome file: {:?}", path);

        // Create parser that only includes autosomal chromosomes (1-22)
        let parser = Genome23Parser::autosomal_only();

        // Parse the file
        let records = parser.parse(path)
            .context("Failed to parse 23andMe genome file")?;

        info!("Parsed {} SNPs from 23andMe file", records.len());

        Ok(UserGenomeData { records })
    }

    async fn process_chromosome(
        &self,
        chr: u8,
        files: &InputFiles,
        reference_panel: &ReferencePanelReader,
    ) -> Result<Vec<MultiSampleVariant>> {
        info!("Processing chromosome {} with 51-sample merge", chr);

        // 1. Load reference panel variants for this chromosome (50 samples)
        let ref_variants = reference_panel.get_chromosome_variants(chr)
            .context(format!("Failed to load reference panel for chr{}", chr))?;

        info!("Loaded {} reference panel variants for chr{}", ref_variants.len(), chr);

        // 2. Parse user's VCF file (imputed data)
        let vcf_path = files
            .vcf_files
            .iter()
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(&format!("chr{}.dose.vcf.gz", chr))
            })
            .ok_or_else(|| anyhow::anyhow!("VCF file for chr{} not found", chr))?;

        debug!("Parsing user VCF file: {:?}", vcf_path);
        let mut vcf_parser = VCFParser::new();
        let user_vcf_records = vcf_parser
            .parse(vcf_path)
            .context(format!("Failed to parse VCF for chromosome {}", chr))?;

        info!("Parsed {} user imputed variants for chr{}", user_vcf_records.len(), chr);

        // 3. Parse user's 23andMe data (genotyped data)
        let user_genome_records = self.load_23andme_for_chr(chr, &files.genome_file).await?;

        info!("Loaded {} user genotyped variants for chr{}", user_genome_records.len(), chr);

        // 4. Build lookups for user data
        // Key: (position, ref_allele, alt_allele)
        let mut user_vcf_lookup: HashMap<(u64, String, String), _> = HashMap::new();
        for record in &user_vcf_records {
            let key = (
                record.position,
                record.ref_allele.clone(),
                record.alt_allele.clone(),
            );
            user_vcf_lookup.insert(key, record);
        }

        // Build lookup for 23andMe data (only by position, since we'll check alleles during matching)
        let mut user_genome_lookup: HashMap<u64, &Genome23Record> = HashMap::new();
        for record in &user_genome_records {
            user_genome_lookup.insert(record.position, record);
        }

        // 5. Merge all variants
        let mut merged = Vec::new();
        let mut user_genotyped_count = 0;
        let mut user_imputed_count = 0;
        let mut filtered_by_quality = 0;

        for ref_variant in ref_variants {
            // Check if user has VCF data for this variant (match by position + REF + ALT)
            let key = (
                ref_variant.position,
                ref_variant.ref_allele.clone(),
                ref_variant.alt_allele.clone(),
            );

            // Apply quality filtering
            if !self.quality_threshold.passes(ref_variant.imputation_quality) {
                filtered_by_quality += 1;
                continue;
            }

            let user_sample = if let Some(user_vcf) = user_vcf_lookup.get(&key) {
                // User has imputed VCF data for this variant
                // Check if we also have genotyped data
                if let Some(user_genome) = user_genome_lookup.get(&ref_variant.position) {
                    // Try to use genotyped data
                    match genotype_to_dosage(
                        &user_genome.genotype,
                        &ref_variant.ref_allele,
                        &ref_variant.alt_allele,
                    ) {
                        Ok(Some(dosage)) => {
                            // Successfully converted genotype
                            user_genotyped_count += 1;
                            SampleData {
                                sample_id: "samp51".to_string(),
                                genotype: user_genome.genotype.clone(),
                                dosage,
                                source: DataSource::Genotyped,
                                imputation_quality: user_vcf.imputation_quality,
                            }
                        }
                        Ok(None) | Err(_) => {
                            // Missing genotype or conversion error, use imputed
                            user_imputed_count += 1;
                            let source = if let Some(qual) = user_vcf.imputation_quality {
                                if qual < 0.3 {
                                    DataSource::ImputedLowQual
                                } else {
                                    DataSource::Imputed
                                }
                            } else {
                                DataSource::Imputed
                            };

                            SampleData {
                                sample_id: "samp51".to_string(),
                                genotype: format_dosage_as_genotype(user_vcf.dosage),
                                dosage: user_vcf.dosage,
                                source,
                                imputation_quality: user_vcf.imputation_quality,
                            }
                        }
                    }
                } else {
                    // No genotyped data, use imputed from VCF
                    user_imputed_count += 1;
                    let source = if let Some(qual) = user_vcf.imputation_quality {
                        if qual < 0.3 {
                            DataSource::ImputedLowQual
                        } else {
                            DataSource::Imputed
                        }
                    } else {
                        DataSource::Imputed
                    };

                    SampleData {
                        sample_id: "samp51".to_string(),
                        genotype: format_dosage_as_genotype(user_vcf.dosage),
                        dosage: user_vcf.dosage,
                        source,
                        imputation_quality: user_vcf.imputation_quality,
                    }
                }
            } else {
                // User has no VCF data for this variant
                // Mark as missing data (dosage 0.0, genotype "./.")
                SampleData {
                    sample_id: "samp51".to_string(),
                    genotype: "./.".to_string(),
                    dosage: 0.0,
                    source: DataSource::ImputedLowQual,
                    imputation_quality: None,
                }
            };

            // Build samples vector: 50 reference samples + 1 user sample
            let mut samples = Vec::with_capacity(51);

            // Add 50 reference samples
            for (idx, genotype) in ref_variant.sample_genotypes.iter().enumerate() {
                let sample_id = format!("samp{}", idx + 1);
                let dosage = calculate_dosage_from_genotype(genotype);

                samples.push(SampleData {
                    sample_id,
                    genotype: genotype.clone(),
                    dosage,
                    source: if ref_variant.is_typed {
                        DataSource::Genotyped
                    } else {
                        DataSource::Imputed
                    },
                    imputation_quality: ref_variant.imputation_quality,
                });
            }

            // Add user sample (sample 51)
            samples.push(user_sample);

            // Create multi-sample variant
            merged.push(MultiSampleVariant {
                rsid: ref_variant.rsid.unwrap_or_else(|| format!("chr{}:{}", chr, ref_variant.position)),
                chromosome: chr,
                position: ref_variant.position,
                ref_allele: ref_variant.ref_allele.clone(),
                alt_allele: ref_variant.alt_allele.clone(),
                allele_freq: ref_variant.allele_freq,
                minor_allele_freq: ref_variant.minor_allele_freq,
                is_typed: ref_variant.is_typed,
                samples,
            });
        }

        info!(
            "Merged chr{}: {} variants ({} user genotyped, {} user imputed, {} filtered by quality)",
            chr,
            merged.len(),
            user_genotyped_count,
            user_imputed_count,
            filtered_by_quality
        );

        Ok(merged)
    }

    async fn load_23andme_for_chr(&self, chr: u8, genome_file: &Path) -> Result<Vec<Genome23Record>> {
        debug!("Loading 23andMe data for chromosome {}", chr);

        // Parse 23andMe file (parser caches internally for efficiency)
        let parser = Genome23Parser::autosomal_only();
        let all_records = parser
            .parse(genome_file)
            .context("Failed to parse 23andMe genome file")?;

        // Filter for this chromosome
        let chr_str = chr.to_string();
        let chr_records: Vec<Genome23Record> = all_records
            .into_iter()
            .filter(|r| r.chromosome == chr_str)
            .collect();

        debug!(
            "Filtered {} records for chromosome {}",
            chr_records.len(),
            chr
        );

        Ok(chr_records)
    }

    async fn process_pgs_scores(&self, path: &Path) -> Result<PgsDataset> {
        info!("Parsing PGS scores from {:?}", path);

        // Parse PGS file with automatic z-score normalization
        let dataset = PgsParser::parse(path)
            .context("Failed to parse PGS scores file")?;

        info!(
            "Parsed {} PGS records (unscaled: {}, scaled: {})",
            dataset.unscaled.len(),
            dataset.unscaled.len(),
            dataset.scaled.len()
        );

        // Log statistics for each unique PGS label
        let labels: std::collections::HashSet<_> =
            dataset.unscaled.iter().map(|r| r.label.as_str()).collect();

        for label in labels {
            if let Some(stats) = PgsParser::get_stats(&dataset.unscaled, label) {
                debug!(
                    "PGS '{}': n={}, mean={:.4}, sd={:.4}, range=[{:.4}, {:.4}]",
                    stats.label, stats.count, stats.mean, stats.std_dev, stats.min, stats.max
                );
            }
        }

        Ok(dataset)
    }

    async fn generate_output_file(
        &self,
        merged_chromosomes: &HashMap<u8, Vec<MultiSampleVariant>>,
        pgs_data: &PgsDataset,
    ) -> Result<PathBuf> {
        // TODO: Implement actual output generation (JSON or RData)
        // For now, just create the output directory structure

        let results_dir = self
            .data_dir
            .join("results")
            .join(&self.user_id)
            .join(self.job_id.to_string());

        std::fs::create_dir_all(&results_dir)?;

        let output_path = results_dir.join("GenomicData4152.json");

        info!(
            "Output generation not yet implemented. Would write {} chromosomes and {} PGS records to {:?}",
            merged_chromosomes.len(),
            pgs_data.unscaled.len(),
            output_path
        );

        // Log summary statistics
        for chr in 1..=22 {
            if let Some(variants) = merged_chromosomes.get(&chr) {
                let user_genotyped = variants
                    .iter()
                    .filter(|v| {
                        v.samples.iter()
                            .find(|s| s.sample_id == "samp51")
                            .map(|s| s.source == DataSource::Genotyped)
                            .unwrap_or(false)
                    })
                    .count();
                debug!(
                    "  chr{}: {} variants (51 samples each, {} user genotyped)",
                    chr,
                    variants.len(),
                    user_genotyped
                );
            }
        }

        // Log PGS statistics
        let pgs_labels: std::collections::HashSet<_> =
            pgs_data.unscaled.iter().map(|r| r.label.as_str()).collect();
        debug!("PGS data includes {} unique traits", pgs_labels.len());

        Ok(output_path)
    }

    async fn secure_delete_inputs(&self, files: &InputFiles) -> Result<()> {
        // Securely delete genome file
        secure_delete::secure_delete_file(&files.genome_file).await?;

        audit::log_event(
            &self.db_pool,
            audit::AuditEventType::FileDeleted,
            &self.user_id,
            Some(self.job_id.to_string()),
            serde_json::json!({
                "file": files.genome_file.to_str(),
                "reason": "secure_deletion_after_processing",
            }),
        )
        .await?;

        // Securely delete all VCF files
        for vcf in &files.vcf_files {
            secure_delete::secure_delete_file(vcf).await?;

            audit::log_event(
                &self.db_pool,
                audit::AuditEventType::FileDeleted,
                &self.user_id,
                Some(self.job_id.to_string()),
                serde_json::json!({
                    "file": vcf.to_str(),
                    "reason": "secure_deletion_after_processing",
                }),
            )
            .await?;
        }

        // Securely delete PGS file
        secure_delete::secure_delete_file(&files.pgs_file).await?;

        audit::log_event(
            &self.db_pool,
            audit::AuditEventType::FileDeleted,
            &self.user_id,
            Some(self.job_id.to_string()),
            serde_json::json!({
                "file": files.pgs_file.to_str(),
                "reason": "secure_deletion_after_processing",
            }),
        )
        .await?;

        Ok(())
    }
}

// Data structures
struct InputFiles {
    genome_file: PathBuf,
    vcf_files: Vec<PathBuf>,
    pgs_file: PathBuf,
}

struct UserGenomeData {
    /// All parsed 23andMe records
    records: Vec<Genome23Record>,
}

// Helper functions

/// Convert dosage (0.0-2.0) to genotype string for display
fn format_dosage_as_genotype(dosage: f64) -> String {
    // Round to nearest integer for simple display
    // 0.0 -> "0|0", 1.0 -> "0|1", 2.0 -> "1|1"
    if dosage < 0.5 {
        "0|0".to_string()
    } else if dosage < 1.5 {
        "0|1".to_string()
    } else {
        "1|1".to_string()
    }
}

/// Calculate dosage from phased genotype string
fn calculate_dosage_from_genotype(genotype: &str) -> f64 {
    // Parse genotypes like "0|0", "0|1", "1|0", "1|1", "./."
    let parts: Vec<&str> = if genotype.contains('|') {
        genotype.split('|').collect()
    } else if genotype.contains('/') {
        genotype.split('/').collect()
    } else {
        return 0.0; // Invalid format
    };

    if parts.len() != 2 {
        return 0.0;
    }

    let allele1 = parts[0].parse::<i32>().unwrap_or(0);
    let allele2 = parts[1].parse::<i32>().unwrap_or(0);

    (allele1 + allele2) as f64
}
