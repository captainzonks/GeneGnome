// ==============================================================================
// job_processor.rs - Genetics Data Processing Logic
// ==============================================================================
// Description: Execute genetics processor on uploaded files
// Author: Matt Barham
// Created: 2025-11-06
// Modified: 2026-11-26
// Version: 1.2.0
// ==============================================================================

use anyhow::{Context, Result};
use chrono::Utc;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

// Import from genetics-processor library
use genetics_processor::merge::merge_variant;
use genetics_processor::models::{MultiSampleVariant, QualityThreshold as ModelQualityThreshold};
use genetics_processor::parsers::{
    genome23andme::{Genome23Parser, Genome23Record},
    pgs::{PgsParser, is_scores_filename},
    vcf::{VCFParser, VCFRecord},
};
use genetics_processor::reference_panel::ReferencePanelReader;

use crate::db;
use crate::queue::{JobQueue, OutputFormat, QualityThreshold};

/// What an uploaded file is, judged by its name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadKind {
    Genome,
    Vcf,
    Scores,
}

fn classify_upload(name: &str) -> Option<UploadKind> {
    if is_scores_filename(name) {
        Some(UploadKind::Scores)
    } else if name.ends_with(".txt") {
        Some(UploadKind::Genome)
    } else if name.ends_with(".vcf.gz") || name.ends_with(".vcf") {
        Some(UploadKind::Vcf)
    } else {
        None
    }
}

/// Job processor that executes genetics data merging
pub struct JobProcessor {
    job_id: Uuid,
    user_id: String,
    upload_dir: PathBuf,
    output_dir: PathBuf,
    reference_panel_path: PathBuf,
    db_pool: PgPool,
    redis_conn: ConnectionManager,
}

impl JobProcessor {
    pub fn new(
        job_id: Uuid,
        user_id: String,
        upload_dir: PathBuf,
        output_dir: PathBuf,
        reference_panel_path: PathBuf,
        db_pool: PgPool,
        redis_conn: ConnectionManager,
    ) -> Self {
        Self {
            job_id,
            user_id,
            upload_dir,
            output_dir,
            reference_panel_path,
            db_pool,
            redis_conn,
        }
    }

    /// Get VCF format preference from job metadata
    async fn get_vcf_format_preference(&self) -> Result<genetics_processor::output::VcfFormat> {
        use genetics_processor::output::VcfFormat;

        // Query database for job metadata (RLS-scoped: this worker already
        // knows the job's owner from the queue payload, no lookup needed)
        let mut tx = db::scoped_tx_for_user(&self.db_pool, &self.user_id)
            .await
            .context("Failed to open RLS-scoped transaction for job metadata")?;
        let row: Option<(serde_json::Value,)> =
            sqlx::query_as("SELECT metadata FROM genetics_jobs WHERE id = $1")
                .bind(self.job_id)
                .fetch_optional(&mut *tx)
                .await
                .context("Failed to query job metadata")?;
        tx.commit().await.context("Failed to commit")?;

        // Parse VCF format preference from metadata
        if let Some((metadata,)) = row
            && let Some(vcf_format_str) = metadata.get("vcf_format").and_then(|v| v.as_str())
        {
            let format = match vcf_format_str {
                "per_chromosome" => VcfFormat::PerChromosome,
                _ => VcfFormat::Merged, // Default to merged for unknown values
            };
            info!(
                "Job {} VCF format preference from metadata: {:?}",
                self.job_id, format
            );
            return Ok(format);
        }

        // No metadata or no vcf_format field - default to Merged
        info!(
            "Job {} has no VCF format preference in metadata, defaulting to Merged",
            self.job_id
        );
        Ok(VcfFormat::Merged)
    }

    /// Main processing function
    pub async fn process(
        &self,
        output_formats: &[OutputFormat],
        quality_threshold: QualityThreshold,
    ) -> Result<genetics_processor::output::VisualizationSummary> {
        info!(
            "Starting multi-sample genetics processing (51 samples) for job {} with quality threshold: {:?}",
            self.job_id, quality_threshold
        );

        // Step 1: Verify reference panel database exists
        self.publish_progress(5.0, "Verifying reference panel database (50 samples)")
            .await?;
        if !self.reference_panel_path.exists() {
            return Err(anyhow::anyhow!(
                "Reference panel database not found at {:?}",
                self.reference_panel_path
            ));
        }
        info!(
            "Reference panel database verified: {:?}",
            self.reference_panel_path
        );
        self.publish_progress(
            8.0,
            "Reference panel ready (will load per-chromosome to manage memory)",
        )
        .await?;

        // Step 2: Find uploaded files
        self.publish_progress(10.0, "Locating uploaded files")
            .await?;
        let files = self.find_uploaded_files().await?;
        self.publish_progress(
            12.0,
            &format!(
                "Found {} VCF file(s), genome file, and {} PGS file",
                files.vcf_files.len(),
                if files.pgs_file.is_some() { "1" } else { "0" }
            ),
        )
        .await?;

        // Step 3: Parse 23andMe genome file
        self.publish_progress(20.0, "Parsing 23andMe genome data")
            .await?;
        let genome_data = self.parse_genome_file(&files.genome_file).await?;
        info!("Parsed {} genome records", genome_data.len());
        self.publish_progress(
            25.0,
            &format!(
                "Loaded {} genotyped variants from 23andMe",
                genome_data.len()
            ),
        )
        .await?;

        // Step 4: Parse VCF files
        self.publish_progress(
            30.0,
            &format!("Parsing {} VCF file(s)...", files.vcf_files.len()),
        )
        .await?;
        let vcf_data = self.parse_vcf_files(&files.vcf_files).await?;
        info!("Parsed VCF data for {} chromosomes", vcf_data.len());
        let total_vcf_variants: usize = vcf_data.values().map(|v| v.len()).sum();
        self.publish_progress(
            40.0,
            &format!(
                "Loaded {} imputed variants across {} chromosomes",
                total_vcf_variants,
                vcf_data.len()
            ),
        )
        .await?;

        // Step 5: Parse PGS scores (optional)
        self.publish_progress(45.0, "Parsing polygenic scores file")
            .await?;
        let pgs_data = self.parse_pgs_file(&files.pgs_file).await?;
        if let Some(ref data) = pgs_data {
            info!("Parsed {} PGS records", data.unscaled.len());
            let trait_count = data
                .unscaled
                .iter()
                .map(|r| r.label.clone())
                .collect::<std::collections::HashSet<_>>()
                .len();
            self.publish_progress(
                50.0,
                &format!(
                    "Loaded {} polygenic scores for {} traits",
                    data.unscaled.len(),
                    trait_count
                ),
            )
            .await?;
        } else {
            info!("No PGS data available, continuing without polygenic scores");
            self.publish_progress(50.0, "No PGS data - continuing without polygenic scores")
                .await?;
        }

        // Step 6 & 7: Merge and stream output chromosome-by-chromosome (memory-efficient)
        self.publish_progress(
            55.0,
            "Starting streaming multi-sample processing (51 samples × 22 autosomes)",
        )
        .await?;
        let (output_paths, viz_summary) = self
            .merge_and_stream_chromosomes(
                &genome_data,
                &vcf_data,
                pgs_data.as_ref(),
                quality_threshold,
                output_formats,
            )
            .await?;
        info!(
            "Streaming processing complete: {} output files generated",
            output_paths.len()
        );

        // Step 8: Record file metadata in database
        self.publish_progress(95.0, "Recording output metadata")
            .await?;
        self.record_output_files(&output_paths).await?;

        self.publish_progress(100.0, "Multi-sample processing complete (51 samples)")
            .await?;

        Ok(viz_summary)
    }

    /// Find uploaded files in upload directory
    async fn find_uploaded_files(&self) -> Result<UploadedFiles> {
        let mut genome_file: Option<PathBuf> = None;
        let mut vcf_files: Vec<PathBuf> = Vec::new();
        let mut pgs_file: Option<PathBuf> = None;

        let mut entries = tokio::fs::read_dir(&self.upload_dir)
            .await
            .context("Failed to read upload directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            match classify_upload(&filename_str) {
                Some(UploadKind::Genome) => genome_file = Some(path),
                Some(UploadKind::Vcf) => vcf_files.push(path),
                Some(UploadKind::Scores) => pgs_file = Some(path),
                None => {}
            }
        }

        Ok(UploadedFiles {
            genome_file: genome_file.context("No genome file found")?,
            vcf_files,
            pgs_file,
        })
    }

    /// Parse 23andMe genome file
    async fn parse_genome_file(&self, path: &PathBuf) -> Result<Vec<Genome23Record>> {
        let parser = Genome23Parser::new();
        let records = parser
            .parse(path)
            .context("Failed to parse 23andMe genome file")?;
        Ok(records)
    }

    /// Parse VCF files
    async fn parse_vcf_files(&self, paths: &[PathBuf]) -> Result<HashMap<u8, Vec<VCFRecord>>> {
        let mut all_records = HashMap::new();
        let total_files = paths.len();

        for (idx, path) in paths.iter().enumerate() {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            self.publish_progress(
                30.0 + ((idx + 1) as f32 / total_files as f32) * 10.0,
                &format!("Parsing VCF file {}/{}: {}", idx + 1, total_files, filename),
            )
            .await?;

            let mut parser = VCFParser::new();
            let records = parser
                .parse(path)
                .context(format!("Failed to parse VCF file: {:?}", path))?;

            let record_count = records.len();

            // Group by chromosome
            for record in records {
                all_records
                    .entry(record.chromosome)
                    .or_insert_with(Vec::new)
                    .push(record);
            }

            info!("Parsed {} variants from {}", record_count, filename);
        }

        Ok(all_records)
    }

    /// Parse the uploaded PGS scores file, if there is one. A file that cannot
    /// be parsed fails the job: results must not silently omit uploaded scores.
    async fn parse_pgs_file(
        &self,
        path: &Option<PathBuf>,
    ) -> Result<Option<genetics_processor::parsers::pgs::PgsDataset>> {
        let Some(path) = path else {
            info!("No PGS scores file provided, continuing without PGS data");
            return Ok(None);
        };

        let dataset = PgsParser::parse(path).context("Failed to parse PGS scores file")?;
        info!("Successfully parsed PGS scores file");
        Ok(Some(dataset))
    }

    /// Merge and stream output chromosome-by-chromosome (memory-efficient)
    ///
    /// This method processes chromosomes one at a time, writing output immediately
    /// to avoid accumulating all 22 chromosomes in memory (~31GB).
    async fn merge_and_stream_chromosomes(
        &self,
        genome_data: &[Genome23Record],
        vcf_data: &HashMap<u8, Vec<VCFRecord>>,
        pgs_data: Option<&genetics_processor::parsers::pgs::PgsDataset>,
        quality_threshold: QualityThreshold,
        output_formats: &[OutputFormat],
    ) -> Result<(
        HashMap<String, PathBuf>,
        genetics_processor::output::VisualizationSummary,
    )> {
        use genetics_processor::output::OutputGenerator;

        info!("════════════════════════════════════════════════════════════════");
        info!("Starting TRUE STREAMING multi-sample chromosome merge");
        info!("Memory-efficient: Process one chromosome at a time");
        info!("Quality threshold: {:?}", quality_threshold);
        info!("Output formats requested: {:?}", output_formats);
        info!("════════════════════════════════════════════════════════════════");

        let mut total_variants = 0usize;

        // Convert QualityThreshold
        let model_threshold = match quality_threshold {
            QualityThreshold::None => ModelQualityThreshold::NoFilter,
            QualityThreshold::R080 => ModelQualityThreshold::R08,
            QualityThreshold::R090 => ModelQualityThreshold::R09,
        };

        // Convert queue::OutputFormat to processor::OutputFormat
        use genetics_processor::output::OutputFormat as ProcessorOutputFormat;
        let processor_formats: Vec<ProcessorOutputFormat> = output_formats
            .iter()
            .map(|f| match f {
                OutputFormat::Parquet => ProcessorOutputFormat::Parquet,
                OutputFormat::Sqlite => ProcessorOutputFormat::Sqlite,
                OutputFormat::Vcf => ProcessorOutputFormat::Vcf,
            })
            .collect();

        // Initialize streaming output BEFORE processing any chromosomes
        info!(
            "Initializing streaming output for {} format(s)...",
            processor_formats.len()
        );
        let mut output_gen = OutputGenerator::new(
            self.job_id.to_string(),
            self.user_id.clone(),
            self.output_dir.clone(),
        );

        // Get VCF format preference from job metadata

        let vcf_format = self.get_vcf_format_preference().await?;
        info!(
            "Using VCF format preference from job metadata: {:?}",
            vcf_format
        );

        output_gen
            .initialize_streaming_output(&processor_formats, vcf_format)
            .await?;
        info!("✓ Streaming output initialized (files created, headers written)");

        let pgs_csv_path = match pgs_data {
            Some(scores) => Some(output_gen.write_pgs_scores(scores)?),
            None => None,
        };

        // Process each chromosome and stream output immediately
        for chr in 1..=22u8 {
            info!("════════════════════════════════════════════════════════════════");
            info!("▶ CHROMOSOME {} / 22", chr);
            info!("════════════════════════════════════════════════════════════════");

            // Load reference panel for this chromosome
            info!("  [1/4] Loading reference panel for chromosome {}...", chr);
            let ref_variants = tokio::task::spawn_blocking({
                let path = self.reference_panel_path.clone();
                let chr_num = chr;
                move || -> Result<Vec<genetics_processor::models::ReferencePanelVariant>> {
                    let reference_panel = ReferencePanelReader::open(&path)?;
                    let variants = reference_panel.get_chromosome_variants(chr_num)?;
                    Ok(variants)
                }
            })
            .await??;

            let ref_panel_size_mb = (ref_variants.len() * 50 * 50) / 1_048_576; // Conservative estimate
            info!(
                "  ✓ Loaded {} reference variants (~{} MB estimated)",
                ref_variants.len(),
                ref_panel_size_mb
            );

            // Get user data for this chromosome
            info!("  [2/4] Extracting user data for chromosome {}...", chr);
            let chr_str = chr.to_string();
            let chr_genome: Vec<_> = genome_data
                .iter()
                .filter(|r| r.chromosome == chr_str)
                .cloned()
                .collect();
            let chr_vcf = vcf_data.get(&chr).map(|v| v.as_slice()).unwrap_or(&[]);
            info!(
                "  ✓ User data: {} genome records, {} VCF variants",
                chr_genome.len(),
                chr_vcf.len()
            );

            // Merge this chromosome
            info!(
                "  [3/4] Merging chromosome {} (50 reference + 1 user = 51 samples)...",
                chr
            );
            let merged = self.merge_single_chromosome_multi_sample(
                chr,
                &ref_variants,
                &chr_genome,
                chr_vcf,
                model_threshold,
            )?;

            let variant_count = merged.len();
            total_variants += variant_count;
            let merged_size_mb = (variant_count * 51 * 80) / 1_048_576; // More conservative estimate (~80 bytes per sample)
            info!(
                "  ✓ Merged: {} variants × 51 samples (~{} MB)",
                variant_count, merged_size_mb
            );

            // Explicitly drop reference variants to free memory
            drop(ref_variants);
            info!("  ✓ Reference panel memory freed");

            // IMMEDIATELY write to output files - do NOT accumulate in memory
            info!("  [4/4] Writing chromosome {} to output files...", chr);
            output_gen.append_chromosome(chr, &merged).await?;
            info!("  ✓ Chromosome {} written to all output formats", chr);

            // Drop merged data - no longer needed!
            drop(merged);
            info!("  ✓ Chromosome {} memory freed (peak memory released)", chr);

            // Publish progress
            let progress = 60.0 + (chr as f32 / 22.0) * 25.0; // 60-85% range
            self.publish_progress(
                progress,
                &format!(
                    "Processed and wrote chromosome {}/22 ({} total variants)",
                    chr, total_variants
                ),
            )
            .await?;

            info!(
                "✓ Chromosome {}/22 complete ({} accumulated variants)",
                chr, total_variants
            );
        }

        info!("════════════════════════════════════════════════════════════════");
        info!("All 22 chromosomes processed successfully!");
        info!("Total: {} variants × 51 samples", total_variants);
        info!("Peak memory: ~2-3GB (one chromosome at a time)");
        info!("════════════════════════════════════════════════════════════════");

        // Finalize streaming output (close files, write metadata, create indexes)
        self.publish_progress(90.0, "Finalizing output files (metadata, indexes)...")
            .await?;
        info!("Finalizing streaming output (closing files, writing metadata, creating indexes)...");
        let (output_paths_map, viz_summary) = output_gen.finalize_streaming_output().await?;
        info!("✓ Output finalization complete!");

        // Convert HashMap<OutputFormat, PathBuf> to HashMap<String, PathBuf>
        let mut output_paths: HashMap<String, PathBuf> = output_paths_map
            .into_iter()
            .map(|(fmt, path)| (format!("{:?}", fmt), path))
            .collect();
        if let Some(path) = pgs_csv_path {
            output_paths.insert("PgsScores".to_string(), path);
        }

        info!("════════════════════════════════════════════════════════════════");
        info!("✓ STREAMING PROCESSING COMPLETE!");
        info!("Output files generated: {}", output_paths.len());
        for (format, path) in &output_paths {
            info!("  {} -> {:?}", format, path);
        }
        info!(
            "Visualization summary: {} variants across {} chromosomes",
            viz_summary.total_variants, viz_summary.chromosomes_processed
        );
        info!("Memory efficient: Never held more than 1 chromosome in memory");
        info!("════════════════════════════════════════════════════════════════");

        Ok((output_paths, viz_summary))
    }

    /// Merge a single chromosome's multi-sample data (50 reference + 1 user = 51 samples)
    fn merge_single_chromosome_multi_sample(
        &self,
        chr: u8,
        ref_variants: &[genetics_processor::models::ReferencePanelVariant],
        genome_records: &[Genome23Record],
        vcf_records: &[VCFRecord],
        quality_threshold: ModelQualityThreshold,
    ) -> Result<Vec<MultiSampleVariant>> {
        // 23andMe records by position (alleles are checked during the merge),
        // imputed records by position, REF and ALT
        let mut user_genotyped_lookup: HashMap<u64, Vec<&Genome23Record>> = HashMap::new();
        for record in genome_records {
            user_genotyped_lookup
                .entry(record.position)
                .or_default()
                .push(record);
        }

        let mut user_vcf_lookup: HashMap<(u64, String, String), &VCFRecord> = HashMap::new();
        for record in vcf_records {
            let key = (
                record.position,
                record.ref_allele.clone(),
                record.alt_allele.clone(),
            );
            user_vcf_lookup.insert(key, record);
        }

        let mut merged = Vec::new();
        let mut filtered_count = 0;

        for ref_variant in ref_variants {
            // Apply quality threshold filtering
            if !quality_threshold.passes(ref_variant.imputation_quality) {
                filtered_count += 1;
                continue;
            }

            let key = (
                ref_variant.position,
                ref_variant.ref_allele.clone(),
                ref_variant.alt_allele.clone(),
            );
            let genotyped = user_genotyped_lookup
                .get(&ref_variant.position)
                .map_or(&[][..], Vec::as_slice);
            merged.push(merge_variant(
                chr,
                ref_variant,
                genotyped,
                user_vcf_lookup.get(&key).copied(),
            ));
        }

        info!(
            "Chromosome {} multi-sample merge: {} variants × 51 samples ({} filtered by quality)",
            chr,
            merged.len(),
            filtered_count
        );

        Ok(merged)
    }

    /// Record output file metadata in database
    async fn record_output_files(&self, output_paths: &HashMap<String, PathBuf>) -> Result<()> {
        // RLS-scoped: genetics_files has the same isolation policy as
        // genetics_jobs (see database/init.sql). This insert previously
        // ran on the bare pool with no transaction/context at all - missed
        // by the original diagnosis of this bug, found while verifying it.
        let mut tx = db::scoped_tx_for_user(&self.db_pool, &self.user_id)
            .await
            .context("Failed to open RLS-scoped transaction for output file metadata")?;

        for (format, path) in output_paths {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            let file_size = tokio::fs::metadata(path)
                .await
                .ok()
                .map(|m| m.len() as i64)
                .unwrap_or(0);

            // Map format to database file_type enum
            let file_type = match format.as_str() {
                "Vcf" => "vcf",
                "Parquet" => "result",
                _ => "result",
            };

            // Compute SHA256 hash of the file
            let hash_sha256 = "pending-hash-computation".to_string(); // TODO: Implement actual hash computation

            sqlx::query(
                "INSERT INTO genetics_files (job_id, user_id, file_name, file_type, file_size, hash_sha256, uploaded_at, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
            )
            .bind(self.job_id)
            .bind(&self.user_id)
            .bind(file_name)
            .bind(file_type)
            .bind(file_size)
            .bind(hash_sha256)
            .bind(Utc::now())
            .bind(serde_json::json!({}))
            .execute(&mut *tx)
            .await
            .context("Failed to record output file metadata")?;
        }

        tx.commit()
            .await
            .context("Failed to commit output file metadata")?;

        Ok(())
    }

    /// Publish progress update via Redis pub/sub
    async fn publish_progress(&self, progress_pct: f32, message: &str) -> Result<()> {
        let mut job_queue = JobQueue::new(self.redis_conn.clone());

        let progress_msg = serde_json::json!({
            "job_id": self.job_id,
            "progress_pct": progress_pct,
            "message": message,
            "timestamp": Utc::now().to_rfc3339(),
        });

        job_queue
            .publish_progress(self.job_id, &progress_msg.to_string())
            .await?;

        Ok(())
    }
}

/// Uploaded files structure
struct UploadedFiles {
    genome_file: PathBuf,
    vcf_files: Vec<PathBuf>,
    pgs_file: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_uploads_by_name() {
        assert_eq!(classify_upload("scores.txt"), Some(UploadKind::Scores));
        assert_eq!(classify_upload("My_Scores.csv"), Some(UploadKind::Scores));
        assert_eq!(classify_upload("traits.pgs"), Some(UploadKind::Scores));
        assert_eq!(classify_upload("genome_v5.txt"), Some(UploadKind::Genome));
        assert_eq!(classify_upload("export.txt"), Some(UploadKind::Genome));
        assert_eq!(classify_upload("chr1.dose.vcf.gz"), Some(UploadKind::Vcf));
        assert_eq!(classify_upload("chr1.dose.vcf.gz.tbi"), None);
    }
}
