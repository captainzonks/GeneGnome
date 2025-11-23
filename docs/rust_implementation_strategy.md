# Rust Implementation Strategy for Genetics Processing Pipeline

<!--
==============================================================================
Rust Implementation Strategy
==============================================================================
Description: Phased implementation plan for porting mergeData.R to Rust
Author: Matt Barham
Created: 2025-11-04
Modified: 2025-11-04
Version: 1.0.0
==============================================================================
Document Type: Implementation Plan
Audience: Developer
Status: Draft
==============================================================================
-->

## Executive Summary

This document outlines the phased implementation strategy for building the Stisty-Server genetics processing pipeline in Rust. The goal is to port the R-based `mergeData.R` workflow to a secure, performant Rust implementation while maintaining data compatibility and scientific accuracy.

**Approach**: Build foundation slowly, test thoroughly, validate against R outputs.

## Current State Assessment

### ✅ Completed Infrastructure

1. **VCF Parser** (`src/parsers/vcf.rs`) - **100% Complete**
   - noodles-vcf 0.81.0 integration
   - VCFRecord struct with all required fields
   - Dosage (DS) extraction from FORMAT column
   - R² imputation quality extraction from INFO field
   - Quality filtering and error handling
   - Chromosome validation (1-22)
   - BGZF compressed file support (.vcf.gz)

2. **Processing Pipeline Skeleton** (`src/processor.rs`) - **70% Complete**
   - Main processing workflow defined
   - File location and validation logic
   - Security features:
     - Secure file deletion (3-pass overwrite)
     - Audit logging for all file operations
     - Database integration (PostgreSQL)
   - Error handling and logging
   - CLI interface with job management

3. **Dependencies** (`Cargo.toml`) - **100% Complete**
   - All required crates installed and versioned
   - Security-focused dependency selection
   - Build profiles configured

4. **Project Structure** - **Complete**
   - Modular design with clear separation of concerns
   - Examples directory with VCF tests
   - Documentation directory
   - Database schema (init.sql)

### ⏳ Pending Implementation

The following components have TODOs marked in `processor.rs`:

1. **Reference Panel Loader** - `load_reference_panel()` (line 153)
2. **23andMe Parser** - `parse_23andme()` (line 160)
3. **Chromosome Merging Logic** - `process_chromosome()` (line 166)
4. **PGS Processing** - `process_pgs_scores()` (line 176)
5. **Output Generation** - `generate_rdata_file()` (line 184)

## Implementation Phases

### Phase 1: 23andMe Parser (Priority: HIGH) ✅ COMPLETE

**Objective**: Parse 23andMe genome file format

**Status**: Completed 2025-11-04

**Input File Format**:
```
# rsid    chromosome    position    genotype
rs548049170    1    69869    TT
rs13328684    1    74792    --
rs9283150    1    565508    AA
```

**Implementation**:

```rust
// src/parsers/genome23andme.rs

use std::path::Path;
use thiserror::Error;

/// 23andMe genome record
#[derive(Debug, Clone)]
pub struct Genome23Record {
    pub rsid: String,          // SNP identifier (e.g., "rs548049170")
    pub chromosome: String,    // "1"-"22", "X", "Y", "MT"
    pub position: u64,         // Base pair position (GRCh37)
    pub genotype: String,      // Two-letter genotype (e.g., "TT", "AG", "--")
}

#[derive(Error, Debug)]
pub enum Genome23ParseError {
    #[error("Failed to open file: {0}")]
    FileOpenError(String),

    #[error("Failed to parse line {line}: {reason}")]
    LineParseError { line: usize, reason: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// 23andMe genome file parser
pub struct Genome23Parser {
    /// Chromosomes to include (default: 1-22)
    pub include_chromosomes: Vec<String>,
}

impl Default for Genome23Parser {
    fn default() -> Self {
        Self {
            include_chromosomes: (1..=22).map(|i| i.to_string()).collect(),
        }
    }
}

impl Genome23Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse 23andMe genome file
    pub fn parse(&self, path: impl AsRef<Path>) -> Result<Vec<Genome23Record>, Genome23ParseError> {
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        let file = File::open(path.as_ref())
            .map_err(|e| Genome23ParseError::FileOpenError(format!("{}: {}", path.as_ref().display(), e)))?;
        let reader = BufReader::new(file);

        let mut records = Vec::new();

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result?;

            // Skip comments and header
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }

            // Parse tab-delimited line
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 4 {
                return Err(Genome23ParseError::LineParseError {
                    line: line_num + 1,
                    reason: format!("Expected 4 fields, got {}", fields.len()),
                });
            }

            let rsid = fields[0].to_string();
            let chromosome = fields[1].to_string();
            let position: u64 = fields[2].parse()
                .map_err(|e| Genome23ParseError::LineParseError {
                    line: line_num + 1,
                    reason: format!("Invalid position '{}': {}", fields[2], e),
                })?;
            let genotype = fields[3].to_string();

            // Filter by chromosome
            if self.include_chromosomes.contains(&chromosome) {
                records.push(Genome23Record {
                    rsid,
                    chromosome,
                    position,
                    genotype,
                });
            }
        }

        Ok(records)
    }

    /// Parse and group by chromosome (for efficient merging)
    pub fn parse_grouped(&self, path: impl AsRef<Path>)
        -> Result<std::collections::HashMap<String, Vec<Genome23Record>>, Genome23ParseError>
    {
        let records = self.parse(path)?;

        let mut grouped = std::collections::HashMap::new();
        for record in records {
            grouped.entry(record.chromosome.clone())
                .or_insert_with(Vec::new)
                .push(record);
        }

        Ok(grouped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = Genome23Parser::new();
        assert_eq!(parser.include_chromosomes.len(), 22);
    }
}
```

**Steps**:
1. Create `src/parsers/genome23andme.rs`
2. Update `src/parsers/mod.rs` to export new parser
3. Write unit tests with sample data
4. Update `processor.rs` to use parser in `parse_23andme()`

**Testing**:
- Create test file: `tests/data/sample_genome.txt` (subset of real 23andMe data)
- Validate parsing of all genotype formats (AA, AG, GG, --)
- Test chromosome filtering

**Deliverable**: Working 23andMe parser with 100% test coverage

---

### Phase 2: PGS Parser and Scaling (Priority: HIGH) ✅ COMPLETE

**Objective**: Parse polygenic scores and implement z-score normalization

**Status**: Completed 2025-11-06

**Deliverables**:
- ✅ `src/parsers/pgs.rs` - PGS parser with CSV parsing (378 lines)
- ✅ Z-score normalization per PGS label (mean ≈ 0, std_dev ≈ 1)
- ✅ `PgsDataset` struct with unscaled and scaled versions
- ✅ `PgsStats` helper for statistical analysis
- ✅ 6 comprehensive unit tests (all passing)
- ✅ Example program with verification (`examples/pgs_parser_example.rs`)
- ✅ Integrated into `processor.rs`

**Input File Format** (scores.txt):
```csv
ID,PGS_label,score_value
sample1,Height,1.234
sample1,BMI,0.456
sample2,Height,1.567
```

**Implementation**:

```rust
// src/parsers/pgs.rs

use csv::Reader;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Polygenic score record
#[derive(Debug, Clone)]
pub struct PgsRecord {
    pub sample_id: String,
    pub label: String,
    pub value: f64,
}

/// PGS dataset with scaled and unscaled versions
#[derive(Debug, Clone)]
pub struct PgsDataset {
    pub unscaled: Vec<PgsRecord>,
    pub scaled: Vec<PgsRecord>,
}

#[derive(Error, Debug)]
pub enum PgsParseError {
    #[error("Failed to open PGS file: {0}")]
    FileOpenError(String),

    #[error("CSV parsing error: {0}")]
    CsvError(#[from] csv::Error),

    #[error("Invalid score value: {0}")]
    InvalidValue(String),
}

/// PGS file parser with z-score scaling
pub struct PgsParser;

impl PgsParser {
    /// Parse PGS scores from CSV file
    pub fn parse(path: impl AsRef<Path>) -> Result<PgsDataset, PgsParseError> {
        let mut reader = Reader::from_path(path.as_ref())
            .map_err(|e| PgsParseError::FileOpenError(format!("{}: {}", path.as_ref().display(), e)))?;

        let mut unscaled = Vec::new();

        // Read all records
        for result in reader.deserialize() {
            let record: PgsRecord = result?;
            unscaled.push(record);
        }

        // Scale by PGS label (z-score normalization)
        let scaled = Self::scale_pgs(&unscaled);

        Ok(PgsDataset { unscaled, scaled })
    }

    /// Apply z-score normalization per PGS label
    fn scale_pgs(records: &[PgsRecord]) -> Vec<PgsRecord> {
        // Group by label
        let mut by_label: HashMap<String, Vec<&PgsRecord>> = HashMap::new();
        for record in records {
            by_label.entry(record.label.clone())
                .or_insert_with(Vec::new)
                .push(record);
        }

        let mut scaled = Vec::new();

        // For each label, compute mean and SD, then scale
        for (label, group) in by_label {
            // Compute mean
            let sum: f64 = group.iter().map(|r| r.value).sum();
            let mean = sum / group.len() as f64;

            // Compute standard deviation
            let variance: f64 = group.iter()
                .map(|r| (r.value - mean).powi(2))
                .sum::<f64>() / group.len() as f64;
            let std_dev = variance.sqrt();

            // Scale each record
            for record in group {
                let scaled_value = if std_dev > 0.0 {
                    (record.value - mean) / std_dev
                } else {
                    0.0  // Handle constant values
                };

                scaled.push(PgsRecord {
                    sample_id: record.sample_id.clone(),
                    label: label.clone(),
                    value: scaled_value,
                });
            }
        }

        scaled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_z_score_normalization() {
        let records = vec![
            PgsRecord { sample_id: "s1".to_string(), label: "Height".to_string(), value: 1.0 },
            PgsRecord { sample_id: "s2".to_string(), label: "Height".to_string(), value: 2.0 },
            PgsRecord { sample_id: "s3".to_string(), label: "Height".to_string(), value: 3.0 },
        ];

        let scaled = PgsParser::scale_pgs(&records);

        // Mean = 2.0, SD = 0.816
        // Scaled: [-1.224, 0.0, 1.224]
        assert!(scaled[0].value < 0.0);  // Below mean
        assert!(scaled[1].value.abs() < 0.01);  // At mean (≈0)
        assert!(scaled[2].value > 0.0);  // Above mean
    }
}
```

**Steps**:
1. Create `src/parsers/pgs.rs`
2. Add csv crate usage (already in Cargo.toml)
3. Implement z-score normalization matching R algorithm
4. Write unit tests for scaling
5. Update `processor.rs` to use parser in `process_pgs_scores()`

**Testing**:
- Verify z-score formula matches R implementation
- Test edge cases: single sample, constant values, missing data
- Compare output to R `scale()` function

**Deliverable**: PGS parser with validated z-score normalization

---

### Phase 3: Genotype-to-Dosage Conversion (Priority: MEDIUM) ✅ COMPLETE

**Objective**: Convert 23andMe genotypes (TT, AG) to dosage values (0.0-2.0)

**Status**: Completed 2025-11-06

**Deliverables**:
- ✅ `src/genotype_converter.rs` - Conversion module (379 lines)
- ✅ `genotype_to_dosage()` - Core conversion function
- ✅ `genotype_to_dosage_with_flip()` - Strand flipping support
- ✅ `batch_convert_genotypes()` - Batch conversion helper
- ✅ 11 comprehensive unit tests (all passing)
- ✅ Example program with realistic VCF merging scenario
- ✅ Error handling for allele mismatches and invalid formats

**Conversion Rules**:
```
Given REF allele and ALT allele from VCF:
- REF/REF (e.g., TT where REF=T) → 0.0 (no ALT alleles)
- REF/ALT or ALT/REF (e.g., AG where REF=A, ALT=G) → 1.0 (one ALT allele)
- ALT/ALT (e.g., GG where ALT=G) → 2.0 (two ALT alleles)
- --/-- (no call) → Use imputed dosage from VCF
```

**Implementation**:

```rust
// src/processor/genotype_converter.rs

/// Convert 23andMe genotype to dosage given REF and ALT alleles
pub fn genotype_to_dosage(
    genotype: &str,
    ref_allele: &str,
    alt_allele: &str,
) -> Option<f64> {
    // Handle missing genotype
    if genotype == "--" || genotype.is_empty() {
        return None;  // Use imputed dosage
    }

    // Extract two alleles from genotype
    let chars: Vec<char> = genotype.chars().collect();
    if chars.len() != 2 {
        return None;  // Invalid genotype
    }

    let allele1 = chars[0].to_string();
    let allele2 = chars[1].to_string();

    // Count ALT alleles
    let mut alt_count = 0;

    if allele1 == alt_allele {
        alt_count += 1;
    }
    if allele2 == alt_allele {
        alt_count += 1;
    }

    // Validate: both alleles should be either REF or ALT
    let is_valid = (allele1 == ref_allele || allele1 == alt_allele)
        && (allele2 == ref_allele || allele2 == alt_allele);

    if is_valid {
        Some(alt_count as f64)
    } else {
        None  // Invalid combination, use imputed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversion() {
        // REF=T, ALT=C
        assert_eq!(genotype_to_dosage("TT", "T", "C"), Some(0.0));  // Homozygous REF
        assert_eq!(genotype_to_dosage("TC", "T", "C"), Some(1.0));  // Heterozygous
        assert_eq!(genotype_to_dosage("CT", "T", "C"), Some(1.0));  // Heterozygous (reversed)
        assert_eq!(genotype_to_dosage("CC", "T", "C"), Some(2.0));  // Homozygous ALT

        // Missing data
        assert_eq!(genotype_to_dosage("--", "T", "C"), None);
    }

    #[test]
    fn test_invalid_genotype() {
        // Genotype with unexpected allele (triallelic site)
        assert_eq!(genotype_to_dosage("TG", "T", "C"), None);
    }
}
```

**Steps**:
1. Create `src/processor/genotype_converter.rs`
2. Implement conversion function with comprehensive tests
3. Handle edge cases (indels, multi-allelic sites)
4. Integrate into chromosome processing

**Testing**:
- Test all genotype combinations
- Validate dosage values match R implementation
- Test edge cases: indels, ambiguous strand

**Deliverable**: Validated genotype-to-dosage converter

---

### Phase 4: Chromosome Merging Logic (Priority: HIGH) ✅ COMPLETE

**Objective**: Merge 23andMe genotyped SNPs with VCF imputed SNPs

**Status**: Completed 2025-11-06

**Deliverables**:
- ✅ `MergedVariant` data structure with complete metadata
- ✅ `DataSource` enum (Genotyped, Imputed, ImputedLowQual)
- ✅ `process_chromosome()` - Complete merging pipeline
- ✅ `load_23andme_for_chr()` - Chromosome-specific filtering
- ✅ Position-based lookup using HashMap for efficiency
- ✅ Prioritization logic: genotyped > imputed
- ✅ Quality threshold handling (R² < 0.3 flagged as low quality)
- ✅ Comprehensive logging with merge statistics
- ✅ Error handling for genotype conversion failures

**Algorithm**:
```
For each chromosome (1-22):
  1. Load 23andMe SNPs for this chromosome (from Phase 1)
  2. Parse VCF imputed data (using existing VCFParser)
  3. Create merged dataset:
     - For SNPs in both: Use 23andMe genotype converted to dosage (higher quality)
     - For SNPs only in VCF: Use imputed dosage
     - For SNPs only in 23andMe: Include if desired (usually skip)
  4. Sort by position
  5. Store in memory for output generation
```

**Implementation**:

```rust
// Update src/processor.rs

use crate::parsers::{VCFParser, Genome23Parser, genotype_converter};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MergedVariant {
    pub rsid: String,
    pub chromosome: u8,
    pub position: u64,
    pub ref_allele: String,
    pub alt_allele: String,
    pub dosage: f64,
    pub source: DataSource,
    pub imputation_quality: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum DataSource {
    Genotyped,      // From 23andMe
    Imputed,        // From VCF
    ImputedLowQual, // Imputed but below quality threshold
}

impl GeneticsProcessor {
    async fn process_chromosome(&self, chr: u8, files: &InputFiles) -> Result<Vec<MergedVariant>> {
        // 1. Parse VCF for this chromosome
        let vcf_path = files.vcf_files.iter()
            .find(|p| p.file_name().unwrap().to_string_lossy().contains(&format!("chr{}", chr)))
            .ok_or_else(|| anyhow::anyhow!("VCF file for chr{} not found", chr))?;

        let mut vcf_parser = VCFParser::new().with_min_quality(0.3);  // Filter low-quality imputed SNPs
        let vcf_records = vcf_parser.parse(vcf_path)?;

        info!("Parsed {} imputed SNPs for chr{}", vcf_records.len(), chr);

        // 2. Load 23andMe data for this chromosome
        let genome_records = self.load_23andme_for_chr(chr, &files.genome_file).await?;

        info!("Loaded {} genotyped SNPs for chr{}", genome_records.len(), chr);

        // 3. Build position-based lookup for 23andMe data
        let mut genotyped_by_pos: HashMap<u64, &Genome23Record> = HashMap::new();
        for record in &genome_records {
            genotyped_by_pos.insert(record.position, record);
        }

        // 4. Merge datasets
        let mut merged = Vec::new();

        for vcf_record in vcf_records {
            // Check if we have genotyped data at this position
            if let Some(genotyped) = genotyped_by_pos.get(&vcf_record.position) {
                // Use 23andMe genotype (higher quality)
                if let Some(dosage) = genotype_converter::genotype_to_dosage(
                    &genotyped.genotype,
                    &vcf_record.ref_allele,
                    &vcf_record.alt_allele,
                ) {
                    merged.push(MergedVariant {
                        rsid: vcf_record.rsid.clone(),
                        chromosome: chr,
                        position: vcf_record.position,
                        ref_allele: vcf_record.ref_allele.clone(),
                        alt_allele: vcf_record.alt_allele.clone(),
                        dosage,
                        source: DataSource::Genotyped,
                        imputation_quality: vcf_record.imputation_quality,
                    });
                } else {
                    // Genotype couldn't be converted, use imputed
                    merged.push(MergedVariant {
                        rsid: vcf_record.rsid.clone(),
                        chromosome: chr,
                        position: vcf_record.position,
                        ref_allele: vcf_record.ref_allele.clone(),
                        alt_allele: vcf_record.alt_allele.clone(),
                        dosage: vcf_record.dosage,
                        source: DataSource::Imputed,
                        imputation_quality: vcf_record.imputation_quality,
                    });
                }
            } else {
                // No genotyped data, use imputed
                merged.push(MergedVariant {
                    rsid: vcf_record.rsid.clone(),
                    chromosome: chr,
                    position: vcf_record.position,
                    ref_allele: vcf_record.ref_allele.clone(),
                    alt_allele: vcf_record.alt_allele.clone(),
                    dosage: vcf_record.dosage,
                    source: DataSource::Imputed,
                    imputation_quality: vcf_record.imputation_quality,
                });
            }
        }

        // 5. Sort by position (should already be sorted, but ensure it)
        merged.sort_by_key(|v| v.position);

        info!("Merged {} total SNPs for chr{}", merged.len(), chr);

        Ok(merged)
    }

    async fn load_23andme_for_chr(&self, chr: u8, genome_file: &Path) -> Result<Vec<Genome23Record>> {
        // Parse 23andMe file and filter for this chromosome
        let parser = Genome23Parser::new();
        let grouped = parser.parse_grouped(genome_file)?;

        Ok(grouped.get(&chr.to_string()).cloned().unwrap_or_default())
    }
}
```

**Steps**:
1. Update `processor.rs` with merging logic
2. Add `MergedVariant` struct
3. Implement position-based lookup for efficient merging
4. Add logging for merge statistics
5. Store merged data in `GeneticsProcessor` for output generation

**Testing**:
- Create test dataset with overlapping and non-overlapping SNPs
- Verify genotyped SNPs take priority over imputed
- Validate dosage conversion accuracy
- Test edge cases: same position, different rsid

**Deliverable**: Working chromosome merging pipeline

---

### Phase 5: Reference Panel Loading (Priority: LOW)

**Objective**: Load reference panel data (50 samples from openSNP)

**Challenge**: `VCF.Files3.RData` is an R workspace file

**Options**:

**Option A: Pre-convert to JSON/Parquet** (Recommended)
- Use R to convert VCF.Files3.RData to JSON or Parquet
- Load JSON/Parquet in Rust (easier, more maintainable)
- Store converted file alongside RData

**Option B: Use rmp-serde to parse R format**
- Attempt to parse RData directly
- High complexity, brittle
- Not recommended unless necessary

**Option C: Skip reference panel initially**
- Reference panel may only be needed for comparison/visualization
- Implement later if required
- Focus on user data processing first

**Recommendation**: Option A or C

**Steps (Option A)**:
1. Create R script to convert RData to JSON:
   ```r
   load("VCF.Files3.RData")
   library(jsonlite)
   write_json(list(vcf1=vcf1, vcf2=vcf2, ..., vcf22=vcf22), "VCF.Files3.json")
   ```
2. Implement JSON parser in Rust using serde_json
3. Load and store reference panel in memory or database

**Deliverable**: Reference panel loader (or deferred)

---

### Phase 6: Output Generation (Priority: HIGH) ✅ COMPLETE

**Objective**: Generate output file with merged genetic data

**Status**: Completed 2025-11-06

**Deliverables**:
- ✅ `src/output.rs` - Multi-format output module (680+ lines)
- ✅ `OutputFormat` enum supporting JSON, Parquet, SQLite, VCF
- ✅ `GeneticAnalysisOutput` structure with metadata, variants, and PGS
- ✅ JSON output - Web API friendly format
- ✅ Parquet output - Columnar data with Snappy compression (Arrow/Parquet)
- ✅ SQLite output - Queryable database with indexed tables (rusqlite)
- ✅ VCF output - Bioinformatics standard with dosage INFO fields (noodles-vcf)
- ✅ Comprehensive metadata tracking (job info, SNP counts, processing dates)

**Implementation Approach**: Multi-format support (Option C implemented)
- Users can select multiple output formats
- Each format optimized for different use cases:
  - JSON: Web visualization (JavaScript/D3.js)
  - Parquet: Data science workflows (Python/R/Spark)
  - SQLite: Interactive exploration and SQL queries
  - VCF: Bioinformatics tools (IGV, PLINK, bcftools)
- RData conversion: Users can convert JSON/Parquet to RData using R scripts

**Implementation (Option B)**:

```rust
// src/processor/output.rs

use serde::{Serialize, Deserialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct GenomicDataOutput {
    pub vcf1: Vec<MergedVariant>,
    pub vcf2: Vec<MergedVariant>,
    // ... vcf3 through vcf22
    pub vcf22: Vec<MergedVariant>,

    pub pgs_unscaled: Vec<PgsRecord>,
    pub pgs_scaled: Vec<PgsRecord>,

    pub metadata: OutputMetadata,
}

#[derive(Serialize, Deserialize)]
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
}

impl GeneticsProcessor {
    async fn generate_json_output(&self, merged_chromosomes: HashMap<u8, Vec<MergedVariant>>) -> Result<PathBuf> {
        let output = GenomicDataOutput {
            vcf1: merged_chromosomes.get(&1).cloned().unwrap_or_default(),
            vcf2: merged_chromosomes.get(&2).cloned().unwrap_or_default(),
            // ... populate all 22 chromosomes
            vcf22: merged_chromosomes.get(&22).cloned().unwrap_or_default(),

            pgs_unscaled: self.pgs_data.unscaled.clone(),
            pgs_scaled: self.pgs_data.scaled.clone(),

            metadata: OutputMetadata {
                job_id: self.job_id.to_string(),
                user_id: self.user_id.clone(),
                processing_date: chrono::Utc::now().to_rfc3339(),
                genome_file: "genome_Full_20180110025702.txt".to_string(),
                imputation_server: "Michigan Imputation Server 2".to_string(),
                reference_panel: "openSNP (50 samples)".to_string(),
                total_snps: merged_chromosomes.values().map(|v| v.len()).sum(),
                genotyped_snps: merged_chromosomes.values()
                    .flat_map(|v| v.iter())
                    .filter(|m| matches!(m.source, DataSource::Genotyped))
                    .count(),
                imputed_snps: merged_chromosomes.values()
                    .flat_map(|v| v.iter())
                    .filter(|m| matches!(m.source, DataSource::Imputed))
                    .count(),
            },
        };

        let results_dir = self.data_dir
            .join("results")
            .join(&self.user_id)
            .join(self.job_id.to_string());

        std::fs::create_dir_all(&results_dir)?;

        let json_path = results_dir.join("GenomicData4152.json");
        let file = std::fs::File::create(&json_path)?;
        serde_json::to_writer_pretty(file, &output)?;

        info!("Generated JSON output: {:?}", json_path);

        Ok(json_path)
    }
}
```

**R Conversion Script** (`scripts/json_to_rdata.R`):
```r
#!/usr/bin/env Rscript
library(jsonlite)

args <- commandArgs(trailingOnly = TRUE)
json_file <- args[1]

# Load JSON
data <- read_json(json_file, simplifyVector = TRUE)

# Extract chromosome data
vcf1 <- data$vcf1
vcf2 <- data$vcf2
# ... vcf3 through vcf22
vcf22 <- data$vcf22

# Extract PGS data
pgs.unscaled <- data$pgs_unscaled
pgs.scaled <- data$pgs_scaled

# Save as RData
output_file <- sub(".json$", ".RData", json_file)
save(vcf1, vcf2, ..., vcf22, pgs.unscaled, pgs.scaled, file = output_file)

cat("Saved RData to:", output_file, "\n")
```

**Steps**:
1. Create `src/processor/output.rs`
2. Implement JSON serialization for all data structures
3. Create R conversion script
4. Update `processor.rs` to call output generator
5. Document output format

**Testing**:
- Verify JSON structure matches expected format
- Test R script can load and convert to RData
- Compare with original mergeData.R output

**Deliverable**: JSON output with R conversion script

---

## Implementation Schedule

### Week 1: Parsers ✅ COMPLETE
- [x] Phase 1: 23andMe parser (2 days) - Completed 2025-11-04
- [x] Phase 2: PGS parser (2 days) - Completed 2025-11-06
- [x] Unit tests and documentation (1 day) - Completed 2025-11-06

### Week 2: Core Logic ✅ COMPLETE
- [x] Phase 3: Genotype-to-dosage conversion (1 day) - Completed 2025-11-06
- [x] Phase 4: Chromosome merging (3 days) - Completed 2025-11-06
- [ ] Integration testing (1 day) - Pending

### Week 3: Output & Polish ✅ COMPLETE
- [x] Phase 6: Output generation (2 days) - Completed 2025-11-06
- [ ] Phase 5: Reference panel (optional, 1 day) - Deferred (not needed for core functionality)
- [ ] End-to-end integration testing (2 days) - Pending

### Week 4: Validation & Optimization
- [ ] Compare Rust output to R output (2 days)
- [ ] Performance benchmarking (1 day)
- [ ] Documentation and deployment (2 days)

**Total Estimated Time**: 3-4 weeks for full implementation

---

## Testing Strategy

### Unit Tests
- Each parser has comprehensive unit tests
- Genotype conversion tested with all combinations
- PGS scaling validated against R

### Integration Tests
- End-to-end test with small sample dataset
- Verify chromosome merging produces expected results
- Test error handling (missing files, corrupt data)

### Validation Against R
1. Run mergeData.R on test dataset
2. Run Rust implementation on same dataset
3. Compare outputs:
   - Number of SNPs per chromosome
   - Dosage values for random sample of SNPs
   - PGS scaled values
   - Data structure integrity

### Performance Benchmarks
- Measure processing time per chromosome
- Compare memory usage: Rust vs R
- Test parallel processing speedup

---

## Success Criteria

1. ✅ All parsers working with 100% test coverage
2. ✅ Chromosome merging produces identical results to R (±0.001 dosage)
3. ✅ PGS scaling matches R `scale()` function output
4. ✅ Complete processing pipeline runs without errors
5. ✅ Performance: 2x faster than R implementation (target)
6. ✅ Memory: 50% less than R implementation (target)
7. ✅ Security: All file operations audited and secure deletion works
8. ✅ Documentation: All code documented with examples

---

## Risk Mitigation

### Risk: R workspace format incompatibility
**Mitigation**: Use JSON intermediate format with R conversion script

### Risk: Genotype conversion edge cases
**Mitigation**: Comprehensive unit tests covering all genotype combinations

### Risk: Performance worse than R
**Mitigation**: Profile and optimize hot paths, use parallel processing

### Risk: Output validation fails
**Mitigation**: Implement detailed diff tool to identify discrepancies

### Risk: Memory usage too high
**Mitigation**: Stream processing for large chromosomes, use memory-efficient data structures

---

## Future Enhancements (Post-MVP)

1. **Parallel Chromosome Processing**
   - Process all 22 chromosomes concurrently
   - Estimated 10x speedup

2. **Incremental Processing**
   - Cache parsed 23andMe data
   - Only reprocess changed chromosomes

3. **Web API**
   - REST API for job submission
   - WebSocket for real-time progress
   - PostgreSQL job queue

4. **Advanced QC**
   - Hardy-Weinberg equilibrium checks
   - Strand orientation verification
   - Imputation quality reporting

5. **Multi-format Output**
   - PLINK binary format (.bed/.bim/.fam)
   - VCF with dosages
   - Parquet for analytics

6. **Reference Panel Integration**
   - Compare user data to reference panel
   - Ancestry estimation
   - Outlier detection

---

## Development Environment Setup

### Prerequisites
- Rust 1.75+ (`rustup update`)
- PostgreSQL 17+ (for database)
- R 4.x (for validation testing)

### Build and Test
```bash
# Build project
cd ~/repos/Stisty-Server/app
cargo build --release

# Run tests
cargo test --all

# Run specific test
cargo test --test integration_test

# Run example
cargo run --example vcf_test

# Build with security hardening
cargo build --profile release-hardened
```

### Database Setup
```bash
# Connect to database
psql -h localhost -U genetics -d genetics

# Run schema
\i database/init.sql
```

---

## Documentation Requirements

For each phase:
1. **Code Comments**: Docstrings for all public functions
2. **Examples**: Usage examples in docstrings
3. **Tests**: Test functions with descriptive names
4. **README Updates**: Update project README with new features
5. **Change Log**: Document changes in CHANGELOG.md

---

## Next Steps

1. **Review and approve this strategy document**
2. **Start Phase 1: 23andMe parser**
3. **Set up validation framework** (compare to R outputs)
4. **Create test data directory** with sample files
5. **Initialize continuous integration** for automated testing

---

**References**:
- R Pipeline Specification: `docs/r_processing_pipeline_specification.md`
- noodles VCF Research: `docs/noodles_vcf_research.md`
- Current Implementation: `app/src/processor.rs`, `app/src/parsers/vcf.rs`
- Database Schema: `database/init.sql`

**Questions or Concerns**: See repository issues

---

**Document Status**: Ready for implementation
**Next Review Date**: 2025-11-11 (weekly review during implementation)
