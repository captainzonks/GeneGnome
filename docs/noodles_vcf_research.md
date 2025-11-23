# noodles VCF Parsing Research

<!--
==============================================================================
noodles VCF Parsing Research
==============================================================================
Description: Research findings for using noodles-vcf crate for VCF parsing
Author: Matt Barham
Created: 2025-11-04
Modified: 2025-11-04
Version: 1.0.0
==============================================================================
Document Type: Research Notes
Audience: Developer
Status: Draft
==============================================================================
-->

## Overview

Research into the `noodles` Rust crate ecosystem for parsing VCF (Variant Call Format) files in the Stisty-Server genetic data processing pipeline.

**Primary Crate**: `noodles-vcf`
**Current Version**: 0.81.0 (as of 2025-11-04)
**Repository**: https://github.com/zaeleus/noodles
**Documentation**: https://docs.rs/noodles-vcf/latest/noodles_vcf/
**License**: MIT

## Noodles Ecosystem

`noodles` is a comprehensive bioinformatics I/O library ecosystem in Rust that supports multiple formats:

**Supported Formats**:
- BAM 1.6
- BCF 2.2 (Binary VCF)
- BED
- BGZF (Blocked GNU Zip Format)
- CRAM 3.0/3.1
- CSI (Coordinate-Sorted Index)
- FASTA
- FASTQ
- GFF3
- GTF 2.2
- htsget 1.3
- refget 2.0
- SAM 1.6
- Tabix
- **VCF 4.3/4.4** ← Our primary interest

**Maturity**:
- 627 stars on GitHub
- 8,542 commits
- Actively maintained
- Production-ready (used in bioinformatics community)
- API still considered experimental (subject to change)

## noodles-vcf Features

### Core Capabilities

1. **VCF Reading and Writing**
   - Sequential VCF file reading
   - VCF file writing
   - Header parsing and manipulation
   - Record-by-record processing

2. **Compressed File Support**
   - BGZF compression via `noodles-bgzf` dependency
   - Native support for `.vcf.gz` files (standard Michigan Imputation Server output)
   - Indexed access via `noodles-csi` and `noodles-tabix`

3. **Asynchronous I/O**
   - Tokio integration for async operations
   - `AsyncReader` and `AsyncWriter` structs
   - Suitable for concurrent chromosome processing

4. **Performance Optimizations**
   - libdeflate for DEFLATE stream encoding/decoding
   - Zero-copy parsing where possible
   - Memory-efficient streaming

### Basic Usage Pattern

```rust
use noodles_vcf as vcf;

// Build a reader from a file path
let mut reader = vcf::io::reader::Builder::default()
    .build_from_path("chr1.dose.vcf.gz")?;

// Read the VCF header
let header = reader.read_header()?;

// Iterate through records
for result in reader.records() {
    let record = result?;
    // Process record...
}
```

### Key Structs

1. **`Reader` / `AsyncReader`**
   - Sequential access to VCF records
   - Handles header parsing
   - Provides record iterator

2. **`Header`**
   - VCF metadata (##INFO, ##FORMAT, ##FILTER, etc.)
   - Sample names
   - File format version

3. **`Record`**
   - Represents a single variant
   - Fields: CHROM, POS, ID, REF, ALT, QUAL, FILTER, INFO, FORMAT, samples
   - Access methods for each field

4. **`Writer` / `AsyncWriter`**
   - Write VCF files
   - Header and record writing

## VCF FORMAT Field Access

### Background: DS (Dosage) Field

From Michigan Imputation Server 2, we receive VCF files with dosage data in the FORMAT field:

```
#CHROM  POS     ID          REF  ALT  QUAL  FILTER  INFO            FORMAT  SAMPLE
1       69869   rs548049170 T    C    .     PASS    R2=0.99;DS=1.95 DS:GP   1.95:0,0,1
```

**Format Structure**:
- `FORMAT` column: Lists field names (e.g., "DS:GP")
- Sample columns: Contain values for each sample (e.g., "1.95:0,0,1")
- `DS`: Dosage value (0-2 continuous scale, posterior mean)
- `GP`: Genotype probabilities (optional)

### Accessing Sample Data in noodles-vcf

**API Pattern** (based on noodles structure):
```rust
// Read a VCF record
let record: vcf::Record = ...;

// Access sample data
let samples = record.samples();  // or similar method

// For a specific sample (e.g., first sample, index 0):
let sample = samples.get(0)?;

// Get FORMAT field value by key
// Method 1: Direct field access
let ds_value = sample.get("DS")?;  // Returns Option or Result

// Method 2: Typed access
let ds: f32 = sample.get_dosage()?;  // Hypothetical typed method

// Convert to numeric value
let dosage: f32 = ds_value.parse()?;
```

**Note**: Exact API needs verification from docs.rs. The noodles API may use:
- `record.genotypes()` or `record.samples()`
- Index-based or key-based FORMAT field access
- Type-safe conversions for common fields (GT, DS, GP, etc.)

### Our Requirements

For Michigan Imputation Server 2 VCF files:
1. Extract `DS` (dosage) field from FORMAT column
2. Single sample per file (column 10)
3. Read compressed `.vcf.gz` files
4. Process 22 files (chr1-chr22)
5. Extract: CHROM, POS, ID (rsid), REF, ALT, DS value

## Implementation Plan for VCF Parsing

### Dependencies

```toml
[dependencies]
noodles = { version = "0.81", features = ["vcf", "bgzf", "async"] }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

### Proposed Architecture

```rust
use noodles_vcf as vcf;
use std::path::Path;

/// VCF variant record for Michigan Imputation Server output
#[derive(Debug, Clone)]
pub struct ImputedVariant {
    pub chrom: u8,          // 1-22
    pub position: u32,      // bp37 position
    pub rsid: String,       // SNP identifier (e.g., "rs548049170")
    pub ref_allele: char,   // Reference allele
    pub alt_allele: char,   // Alternate allele
    pub dosage: f32,        // DS field (0-2)
    pub r2: f32,            // Imputation quality (from INFO field)
}

/// Parse a single chromosome VCF file
pub async fn parse_vcf_file<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<ImputedVariant>, Box<dyn std::error::Error>> {
    let mut reader = vcf::io::reader::Builder::default()
        .build_from_path(path)?;

    let header = reader.read_header()?;
    let mut variants = Vec::new();

    for result in reader.records() {
        let record = result?;

        let variant = ImputedVariant {
            chrom: parse_chrom(&record)?,
            position: record.position()?,
            rsid: extract_rsid(&record),
            ref_allele: record.reference()?,
            alt_allele: extract_alt(&record)?,
            dosage: extract_dosage(&record, &header)?,
            r2: extract_r2(&record)?,
        };

        variants.push(variant);
    }

    Ok(variants)
}

/// Extract dosage (DS) value from FORMAT field
fn extract_dosage(
    record: &vcf::Record,
    header: &vcf::Header,
) -> Result<f32, Box<dyn std::error::Error>> {
    // Implementation depends on noodles API
    // Pseudocode:
    // let samples = record.samples();
    // let sample = samples.get(0)?;  // First sample
    // let ds_value = sample.get_field("DS", header)?;
    // Ok(ds_value.parse()?)
    todo!("Implement using noodles API")
}
```

### Processing Strategy

**Sequential Processing** (Initial Implementation):
```rust
pub async fn process_all_chromosomes(
    vcf_dir: &Path,
) -> Result<HashMap<u8, Vec<ImputedVariant>>, Box<dyn std::error::Error>> {
    let mut results = HashMap::new();

    for chrom in 1..=22 {
        let path = vcf_dir.join(format!("chr{}.dose.vcf.gz", chrom));
        let variants = parse_vcf_file(&path).await?;
        results.insert(chrom, variants);
    }

    Ok(results)
}
```

**Parallel Processing** (Optimized):
```rust
use tokio::task;

pub async fn process_all_chromosomes_parallel(
    vcf_dir: &Path,
) -> Result<HashMap<u8, Vec<ImputedVariant>>, Box<dyn std::error::Error>> {
    let mut handles = vec![];

    for chrom in 1..=22 {
        let path = vcf_dir.join(format!("chr{}.dose.vcf.gz", chrom));
        let handle = task::spawn(async move {
            parse_vcf_file(&path).await
        });
        handles.push((chrom, handle));
    }

    let mut results = HashMap::new();
    for (chrom, handle) in handles {
        let variants = handle.await??;
        results.insert(chrom, variants);
    }

    Ok(results)
}
```

## Next Steps

1. **Verify noodles API**:
   - Access docs.rs/noodles-vcf/latest for exact API
   - Check examples in GitHub repo: `noodles/noodles-vcf/examples/`
   - Test with sample VCF file from Michigan Imputation Server

2. **Prototype VCF Parser**:
   - Create minimal Rust project
   - Add noodles dependency
   - Implement `ImputedVariant` extraction
   - Test with chr22.dose.vcf.gz (smallest chromosome)

3. **Benchmark Performance**:
   - Measure sequential vs parallel processing
   - Compare memory usage vs R implementation
   - Validate output against R mergeData.R results

4. **Handle Edge Cases**:
   - Missing DS field (shouldn't happen with MIS2)
   - Multiple ALT alleles (biallelic vs multiallelic)
   - Missing rsid (use "." placeholder)
   - Malformed VCF records

5. **Integration**:
   - Connect VCF parser to PostgreSQL schema
   - Implement caching in Redis
   - Add API endpoints for data access

## Open Questions

1. **Format Field API**: What is the exact method to access FORMAT fields in noodles 0.81?
   - Need to check: `record.genotypes()`, `record.samples()`, or other?
   - Type-safe access for DS field?

2. **Performance**: How does noodles compare to:
   - R's `read.table()` with colClasses
   - bcftools query
   - Other Rust VCF parsers (rust-bio)?

3. **Memory Usage**: For chromosome 1 (~9M variants):
   - Load all into memory (Vec<ImputedVariant>)?
   - Stream to database?
   - Chunked processing?

4. **Error Handling**:
   - How strict should validation be?
   - Log and skip malformed records?
   - Fail fast on any error?

## References

- **noodles GitHub**: https://github.com/zaeleus/noodles
- **noodles-vcf docs**: https://docs.rs/noodles-vcf/latest/noodles_vcf/
- **VCF 4.3 Spec**: https://samtools.github.io/hts-specs/VCFv4.3.pdf
- **Michigan Imputation Server**: https://imputationserver.sph.umich.edu/
- **23andMe Format**: https://customercare.23andme.com/hc/en-us/articles/212196868

---

**Status**: Research phase complete. Ready for prototyping and API verification.
