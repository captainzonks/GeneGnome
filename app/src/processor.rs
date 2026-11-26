// ==============================================================================
// processor.rs - Core Genetic Data Processing Logic
// ==============================================================================
// Description: Merges 23andMe data with imputed VCF files and 50-sample reference panel
// Author: Matt Barham
// Created: 2025-10-31
// Modified: 2026-11-26
// Version: 2.0.1
// ==============================================================================

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use uuid::Uuid;

use crate::audit;
use crate::merge::merge_variant;
use crate::models::{MultiSampleVariant, QualityThreshold};
use crate::parsers::{Genome23Parser, Genome23Record, PgsDataset, PgsParser, VCFParser};
use crate::reference_panel::ReferencePanelReader;
use crate::secure_delete;

// Re-export for backward compatibility with worker
pub use crate::models::DataSource;

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
        info!(
            "Starting 51-sample genetic data processing for job {}",
            self.job_id
        );
        info!("Quality threshold: {:?}", self.quality_threshold);

        // 1. Locate input files
        let processing_dir = self.get_processing_dir();
        let files = self.locate_input_files(&processing_dir).await?;

        // 2. Validate all files are present
        self.validate_file_set(&files)?;

        // 3. Open reference panel database
        info!(
            "Opening reference panel database: {:?}",
            self.reference_path
        );
        let reference_panel = ReferencePanelReader::open(&self.reference_path)
            .context("Failed to open reference panel database")?;

        reference_panel
            .validate()
            .context("Reference panel validation failed")?;

        // 4. Parse 23andMe data
        info!("Parsing 23andMe data");
        self.check_23andme(&files.genome_file).await?;

        // 5. Process each chromosome (50 reference + 1 user = 51 samples)
        info!("Processing 22 chromosomes with 51-sample merge");
        let mut merged_chromosomes: HashMap<u8, Vec<MultiSampleVariant>> = HashMap::new();

        for chr in 1..=22 {
            let merged = self
                .process_chromosome(chr, &files, &reference_panel)
                .await?;
            merged_chromosomes.insert(chr, merged);
        }

        // Calculate total statistics
        let total_variants: usize = merged_chromosomes.values().map(|v| v.len()).sum();

        // Count how many variants have user data as "Genotyped"
        let user_genotyped: usize = merged_chromosomes
            .values()
            .flat_map(|v| v.iter())
            .filter(|variant| {
                variant
                    .samples
                    .iter()
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
        let result_path = self
            .generate_output_file(&merged_chromosomes, &pgs_data)
            .await?;

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
            genome_file: genome_file
                .ok_or_else(|| anyhow::anyhow!("23andMe genome file not found"))?,
            vcf_files,
            pgs_file: pgs_file.ok_or_else(|| anyhow::anyhow!("PGS scores file not found"))?,
        })
    }

    fn validate_file_set(&self, files: &InputFiles) -> Result<()> {
        // Must have exactly 22 VCF files (one per chromosome)
        if files.vcf_files.len() != 22 {
            anyhow::bail!("Expected 22 VCF files, found {}", files.vcf_files.len());
        }

        // Check each chromosome VCF exists
        for chr in 1..=22 {
            let expected = format!("chr{}.dose.vcf.gz", chr);
            let found = files
                .vcf_files
                .iter()
                .any(|p| p.file_name().unwrap().to_string_lossy().contains(&expected));

            if !found {
                anyhow::bail!("Missing VCF file for chromosome {}", chr);
            }
        }

        Ok(())
    }

    /// Parse the whole file up front so a bad upload fails before chromosome work
    async fn check_23andme(&self, path: &Path) -> Result<()> {
        info!("Parsing 23andMe genome file: {:?}", path);

        // Create parser that only includes autosomal chromosomes (1-22)
        let parser = Genome23Parser::autosomal_only();

        // Parse the file
        let records = parser
            .parse(path)
            .context("Failed to parse 23andMe genome file")?;

        info!("Parsed {} SNPs from 23andMe file", records.len());

        Ok(())
    }

    async fn process_chromosome(
        &self,
        chr: u8,
        files: &InputFiles,
        reference_panel: &ReferencePanelReader,
    ) -> Result<Vec<MultiSampleVariant>> {
        info!("Processing chromosome {} with 51-sample merge", chr);

        // 1. Load reference panel variants for this chromosome (50 samples)
        let ref_variants = reference_panel
            .get_chromosome_variants(chr)
            .context(format!("Failed to load reference panel for chr{}", chr))?;

        info!(
            "Loaded {} reference panel variants for chr{}",
            ref_variants.len(),
            chr
        );

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

        info!(
            "Parsed {} user imputed variants for chr{}",
            user_vcf_records.len(),
            chr
        );

        // 3. Parse user's 23andMe data (genotyped data)
        let user_genome_records = self.load_23andme_for_chr(chr, &files.genome_file).await?;

        info!(
            "Loaded {} user genotyped variants for chr{}",
            user_genome_records.len(),
            chr
        );

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

        // 23andMe records by position; alleles are checked during the merge
        let mut user_genome_lookup: HashMap<u64, Vec<&Genome23Record>> = HashMap::new();
        for record in &user_genome_records {
            user_genome_lookup
                .entry(record.position)
                .or_default()
                .push(record);
        }

        // 5. Merge all variants
        let mut merged = Vec::new();
        let mut filtered_by_quality = 0;

        for ref_variant in ref_variants {
            // Apply quality filtering
            if !self
                .quality_threshold
                .passes(ref_variant.imputation_quality)
            {
                filtered_by_quality += 1;
                continue;
            }

            let key = (
                ref_variant.position,
                ref_variant.ref_allele.clone(),
                ref_variant.alt_allele.clone(),
            );
            let genotyped = user_genome_lookup
                .get(&ref_variant.position)
                .map_or(&[][..], Vec::as_slice);
            merged.push(merge_variant(
                chr,
                &ref_variant,
                genotyped,
                user_vcf_lookup.get(&key).copied(),
            ));
        }

        info!(
            "Merged chr{}: {} variants ({} filtered by quality)",
            chr,
            merged.len(),
            filtered_by_quality
        );

        Ok(merged)
    }

    async fn load_23andme_for_chr(
        &self,
        chr: u8,
        genome_file: &Path,
    ) -> Result<Vec<Genome23Record>> {
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
        let dataset = PgsParser::parse(path).context("Failed to parse PGS scores file")?;

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
                        v.samples
                            .iter()
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

// Helper functions
