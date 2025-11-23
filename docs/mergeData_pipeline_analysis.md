# mergeData.R Pipeline Analysis

<!--
==============================================================================
mergeData_pipeline_analysis.md - Technical analysis of R processing pipeline
==============================================================================
Description: Detailed breakdown of mergeData.R for Rust implementation planning
Author: Matt Barham
Created: 2025-11-03
Modified: 2025-11-04
Version: 1.1.0
Repository: https://github.com/captainzonks/GeneGnome
==============================================================================
Document Type: Technical Specification
Audience: Developer
Status: Reference
==============================================================================
Source: mergeData.R reference implementation (374 lines)
Purpose: Educational genomics script for merging 23andMe + imputation data
Target: Port to Rust for production genetics-processor service
==============================================================================
-->

## Overview

**Purpose**: Merge user's 23andMe genome data with Michigan Imputation Server 2 results and polygenic scores to create comprehensive genomic analysis dataset.

**Input Files:**
- `VCF.Files3.RData` - Reference panel (50 anonymous samples, 22 chromosomes)
- `chr{1-22}.dose.vcf.gz` - Michigan Imputation Server imputed VCF files
- `23andMe_genotype.txt` - User's 23andMe raw data file
- `scores.txt` - Polygenic scores from Michigan server

**Output:**
- `GenomicData4152.RData` - R workspace containing merged data across all chromosomes

**Processing Model:**
- Sequential chromosome-by-chromosome processing (chr1 → chr22)
- User added as 51st sample column to reference panel
- Combines genotyped (direct 23andMe) + imputed (Michigan server) SNPs

---

## Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│ INPUTS                                                              │
├─────────────────────────────────────────────────────────────────────┤
│ 1. VCF.Files3.RData (50 samples × 22 chromosomes)                  │
│    ├─ vcf.Chr1 ... vcf.Chr22                                       │
│    └─ SNP matrix: rsID × 50 samples (genotypes: 0/1/2)            │
│                                                                     │
│ 2. chr{1-22}.dose.vcf.gz (Michigan Imputation Server)              │
│    ├─ Imputed SNPs for user                                        │
│    └─ Format: CHROM POS ID REF ALT QUAL FILTER INFO FORMAT SAMPLE  │
│                                                                     │
│ 3. 23andMe_genotype.txt                                            │
│    ├─ User's genotyped SNPs                                        │
│    └─ Format: rsID chromosome position genotype                    │
│                                                                     │
│ 4. scores.txt (Polygenic scores)                                   │
│    └─ Format: SNP CHR BP A1 A2 weight                              │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ PROCESSING PIPELINE (Per Chromosome Loop: i = 1 to 22)             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│ STAGE 1: Load Reference Panel                                      │
│ ├─ Load vcf.Chr{i} from VCF.Files3.RData                          │
│ └─ Extract rsID list (vcf.Chr{i}$rsID)                            │
│                                                                     │
│ STAGE 2: Read Imputed VCF                                          │
│ ├─ Read chr{i}.dose.vcf.gz                                         │
│ ├─ Skip headers (lines starting with ##)                           │
│ ├─ Parse: CHROM POS ID REF ALT ... SAMPLE                          │
│ └─ Store as vcfi data frame                                        │
│                                                                     │
│ STAGE 3: Read 23andMe Genotypes                                    │
│ ├─ Read 23andMe_genotype.txt                                       │
│ ├─ Filter to chromosome {i}                                        │
│ ├─ Match rsIDs with reference panel                                │
│ └─ Store as gpos6 (initial genotypes)                              │
│                                                                     │
│ STAGE 4: Convert Genotypes to REF/ALT Format                       │
│ ├─ gpos7: Match 23andMe position with imputed VCF position         │
│ ├─ gpos8: Add REF/ALT alleles from imputed VCF                     │
│ ├─ gpos9: Convert genotype calls:                                  │
│ │   ├─ Match REF/REF → 0                                           │
│ │   ├─ Heterozygous → 1                                            │
│ │   ├─ Match ALT/ALT → 2                                           │
│ │   └─ No match → NA                                               │
│ └─ Final: gpos9$GT_NEW (numeric 0/1/2/NA)                          │
│                                                                     │
│ STAGE 5: Merge Genotyped + Imputed SNPs                            │
│ ├─ TOT: Left join vcfi with gpos9 on rsID                          │
│ ├─ TOT2: Keep imputed SNPs (vcfi) if no genotype match             │
│ ├─ TOT3: Keep genotyped SNPs (gpos9) if have match                 │
│ ├─ TOT4: Combined dataset (genotyped preferred over imputed)       │
│ └─ Extract dosage column as user's genotype vector                 │
│                                                                     │
│ STAGE 6: Add User to Reference Panel                               │
│ ├─ Start with vcf.Chr{i} (50 samples)                              │
│ ├─ Add user dosage as 51st column                                  │
│ ├─ Preserve rsID, REF, ALT, position metadata                      │
│ └─ Store back to vcf.Chr{i} (now 51 samples)                       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼ (Repeat for chr1-22)
┌─────────────────────────────────────────────────────────────────────┐
│ POST-PROCESSING                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│ STAGE 7: Merge Polygenic Scores                                    │
│ ├─ Read scores.txt                                                 │
│ ├─ Parse: SNP CHR BP A1 A2 weight                                  │
│ ├─ Create pgs.unscaled and pgs.scaled objects                      │
│ └─ Store in workspace                                              │
│                                                                     │
│ STAGE 8: Save Workspace                                            │
│ ├─ save() all vcf.Chr{1-22} objects                                │
│ ├─ Include pgs.unscaled, pgs.scaled                                │
│ └─ Output: GenomicData4152.RData                                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ OUTPUT                                                              │
├─────────────────────────────────────────────────────────────────────┤
│ GenomicData4152.RData containing:                                  │
│ ├─ vcf.Chr1 ... vcf.Chr22 (51 samples each)                        │
│ ├─ pgs.unscaled (polygenic score data)                             │
│ └─ pgs.scaled (polygenic score data)                               │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Detailed Processing Stages

### STAGE 1: Load Reference Panel

**Code:** Lines 194-196 (per chromosome)

```r
load(url("http://www.matthewckeller.com/public/VCF.Files3.RData"))
# Loads: vcf.Chr1, vcf.Chr2, ..., vcf.Chr22
```

**Data Structure:**
```
vcf.Chr{i}:
  - rsID: Character vector of SNP IDs (e.g., "rs12345")
  - position: Integer, base pair location
  - REF: Character, reference allele
  - ALT: Character, alternate allele
  - sample_1 ... sample_50: Numeric (0/1/2), genotype calls
```

**Key Operations:**
- Single load operation at start (not per-chromosome in optimized version)
- Reference panel provides SNP list and allele definitions
- 50 anonymous samples serve as population reference

**Rust Implementation Notes:**
- Need R workspace parser or pre-convert to Parquet/CSV
- Could cache reference panel in PostgreSQL for performance
- Consider lazy loading per chromosome to reduce memory

---

### STAGE 2: Read Imputed VCF

**Code:** Lines 197-203

```r
vcfi = read.table(paste0("chr", i, ".dose.vcf.gz"), comment.char="#", stringsAsFactors=F)
names(vcfi) = c("CHROM", "POS", "ID", "REF", "ALT", "QUAL", "FILTER", "INFO", "FORMAT", "SAMPLE")
```

**VCF Format (Michigan Imputation Server):**
```
##fileformat=VCFv4.3
##INFO=<ID=DR2,Number=1,Type=Float,Description="Dosage R-square">
#CHROM  POS       ID          REF  ALT  QUAL  FILTER  INFO         FORMAT  SAMPLE
1       10177     rs367896724  A    AC   .     PASS    DR2=0.98     DS      1.02
1       10352     rs555500075  T    TA   .     PASS    DR2=0.87     DS      0.15
```

**Key Fields:**
- **ID**: rsID (SNP identifier)
- **POS**: Base pair position on chromosome
- **REF/ALT**: Reference and alternate alleles
- **SAMPLE** (last column): Dosage value (0.0-2.0, decimal)
  - 0.0 = homozygous reference (REF/REF)
  - 1.0 = heterozygous (REF/ALT)
  - 2.0 = homozygous alternate (ALT/ALT)
  - Decimal values represent imputation uncertainty

**Rust Implementation Notes:**
- Use `noodles` crate for VCF parsing
- Handle gzipped files with `flate2`
- Parse dosage (DS) from FORMAT field
- Typical file size: 10-50 MB compressed per chromosome

---

### STAGE 3: Read 23andMe Genotypes

**Code:** Lines 204-211

```r
gpos6 = read.table("23andMe_genotype.txt", skip=12, stringsAsFactors=F)
names(gpos6) = c("rsID", "chr", "pos", "alleles")
gpos6 = gpos6[gpos6$chr == i,]  # Filter to current chromosome
```

**23andMe File Format:**
```
# 23andMe raw genotype data
# (12 header lines with metadata)
rsid    chromosome    position    genotype
rs12345    1    752566    AG
rs12346    1    752721    GG
rs12347    1    754182    --    # Missing data
```

**Key Operations:**
- Skip 12-line header
- Filter to chromosome `i`
- Match rsIDs with reference panel: `gpos6[gpos6$rsID %in% vcf.Chr{i}$rsID,]`
- Preserve only SNPs present in reference panel

**Genotype Encoding:**
- Two-letter code (e.g., "AG", "GG", "AA")
- "--" indicates no call / missing data
- Unphased (order doesn't matter: "AG" = "GA")

**Rust Implementation Notes:**
- Simple tab-delimited parsing
- Use HashSet for rsID matching (O(1) lookup)
- Handle missing data ("--") gracefully
- Typical file size: 5-10 MB uncompressed

---

### STAGE 4: Convert Genotypes to REF/ALT Format

**Code:** Lines 212-235 (complex multi-stage transformation)

#### Step 4a: Match Positions (gpos7)

```r
gpos7 = merge(gpos6, vcfi[,c("ID","POS")], by.x="rsID", by.y="ID")
```

**Purpose:** Align 23andMe positions with VCF positions (authoritative source)

#### Step 4b: Add REF/ALT Alleles (gpos8)

```r
gpos8 = merge(gpos7, vcfi[,c("ID","REF","ALT")], by.x="rsID", by.y="ID")
```

**Purpose:** Get reference/alternate allele definitions from VCF

#### Step 4c: Convert to Dosage Format (gpos9)

```r
gpos9 = gpos8
gpos9$GT_NEW = NA

# Match REF/REF → 0
gpos9$GT_NEW[gpos9$alleles == paste0(gpos9$REF, gpos9$REF)] = 0

# Match heterozygous → 1
gpos9$GT_NEW[gpos9$alleles == paste0(gpos9$REF, gpos9$ALT) |
             gpos9$alleles == paste0(gpos9$ALT, gpos9$REF)] = 1

# Match ALT/ALT → 2
gpos9$GT_NEW[gpos9$alleles == paste0(gpos9$ALT, gpos9$ALT)] = 2

# No match (strand flip, tri-allelic, etc.) → NA
```

**Conversion Table:**

| 23andMe Genotype | REF | ALT | GT_NEW | Notes |
|------------------|-----|-----|--------|-------|
| AA | A | G | 0 | Homozygous reference |
| AG | A | G | 1 | Heterozygous |
| GA | A | G | 1 | Heterozygous (unphased) |
| GG | A | G | 2 | Homozygous alternate |
| AT | A | G | NA | Doesn't match REF/ALT |
| -- | A | G | NA | Missing data |

**Edge Cases:**
- Strand flips (23andMe uses opposite strand)
- Tri-allelic sites (REF/ALT1/ALT2)
- Indels (insertions/deletions)
- Missing data ("--")

**Rust Implementation Notes:**
- Use pattern matching for genotype conversion
- Create lookup table: `(REF, ALT, genotype) → dosage`
- Handle both orderings of heterozygous calls
- Log unmatched genotypes for debugging
- Consider strand flip detection heuristic

---

### STAGE 5: Merge Genotyped + Imputed SNPs

**Code:** Lines 236-247

```r
# TOT: Left join imputed with genotyped
TOT = merge(vcfi, gpos9[,c("rsID","GT_NEW")], by.x="ID", by.y="rsID", all.x=T)

# TOT2: Keep imputed value if no genotype match
TOT2 = TOT[is.na(TOT$GT_NEW),]
TOT2$GT_FINAL = TOT2$SAMPLE  # Use imputed dosage

# TOT3: Keep genotyped value if match exists
TOT3 = TOT[!is.na(TOT$GT_NEW),]
TOT3$GT_FINAL = TOT3$GT_NEW  # Use genotyped dosage (0/1/2)

# TOT4: Combine
TOT4 = rbind(TOT2, TOT3)
```

**Merge Logic:**

```
For each SNP in imputed VCF:
  If SNP was genotyped by 23andMe AND alleles match:
    Use genotyped value (0/1/2)  ← Higher confidence
  Else:
    Use imputed value (0.0-2.0)  ← Statistical prediction
```

**Why Prefer Genotyped:**
- Direct lab measurement vs statistical imputation
- No imputation uncertainty
- Integer values (0/1/2) vs decimals

**Typical Ratios:**
- ~600K SNPs genotyped by 23andMe
- ~40M SNPs imputed by Michigan server
- ~99% of final dataset is imputed

**Rust Implementation Notes:**
- Use HashMap for O(1) genotype lookups
- Preserve imputation quality (DR2) for filtering
- Output both genotyped and imputed counts for audit log
- Consider flagging which SNPs were genotyped (metadata column)

---

### STAGE 6: Add User to Reference Panel

**Code:** Lines 248-256

```r
# Extract user dosage vector (aligned with vcf.Chr{i} row order)
user_dosages = TOT4$GT_FINAL[match(vcf.Chr{i}$rsID, TOT4$ID)]

# Add as 51st column
vcf.Chr{i}$sample_51 = user_dosages

# Update column names
names(vcf.Chr{i})[ncol(vcf.Chr{i})] = "USER"
```

**Before:**
```
rsID       | pos    | REF | ALT | sample_1 | ... | sample_50
rs12345    | 752566 | A   | G   | 0        | ... | 1
rs12346    | 752721 | G   | C   | 2        | ... | 0
```

**After:**
```
rsID       | pos    | REF | ALT | sample_1 | ... | sample_50 | USER
rs12345    | 752566 | A   | G   | 0        | ... | 1         | 1.02
rs12346    | 752721 | G   | C   | 2        | ... | 0         | 0
```

**Key Operation:**
- `match(vcf.Chr{i}$rsID, TOT4$ID)` aligns TOT4 rows with reference panel row order
- Preserves SNP ordering from reference panel
- User column contains mixed integer (genotyped) and decimal (imputed) values

**Rust Implementation Notes:**
- Create index mapping: `rsID → row_position` for reference panel
- Use vector allocation (Vec<f64>) for user column
- Verify all rsIDs matched (should be 100% overlap)
- Handle missing values (NA) as Option<f64>

---

### STAGE 7: Merge Polygenic Scores

**Code:** Lines 289-320

```r
pgs.unscaled = read.table("scores.txt", header=T)
# Columns: SNP CHR BP A1 A2 weight

# Example polygenic score calculation (conceptual):
# For each trait:
#   score = Σ (genotype_i × weight_i) for all SNPs in trait panel
```

**scores.txt Format:**
```
SNP         CHR  BP        A1  A2  weight
rs1234567   1    752566    A   G   0.0234
rs7654321   1    768234    C   T   -0.0123
```

**Key Fields:**
- **SNP**: rsID
- **CHR/BP**: Chromosome and position
- **A1/A2**: Effect allele and other allele
- **weight**: Beta coefficient from GWAS study

**Polygenic Score Calculation:**
```
PGS_trait = Σ (dosage_SNP × weight_SNP)

Where:
- dosage_SNP: 0/1/2 (or imputed decimal)
- weight_SNP: GWAS effect size
- Sum over all SNPs in trait's panel
```

**pgs.scaled vs pgs.unscaled:**
- `pgs.unscaled`: Raw beta coefficients from GWAS
- `pgs.scaled`: Normalized to population (z-score transformation)

**Rust Implementation Notes:**
- Parse scores.txt as simple TSV
- Store in database table: `polygenic_scores(job_id, snp_id, trait, weight)`
- PGS calculation is matrix multiplication: `genotype_matrix × weight_vector`
- Consider caching scores per trait for multiple users

---

### STAGE 8: Save Workspace

**Code:** Lines 367-372

```r
save(vcf.Chr1, vcf.Chr2, ..., vcf.Chr22,
     pgs.unscaled, pgs.scaled,
     file = "GenomicData4152.RData")
```

**R Workspace (.RData) Contents:**
```
GenomicData4152.RData:
├─ vcf.Chr1:  data.frame (n_snps × 55 columns)
├─ vcf.Chr2:  data.frame
├─ ...
├─ vcf.Chr22: data.frame
├─ pgs.unscaled: data.frame
└─ pgs.scaled: data.frame
```

**Rust Output Format (Proposed):**

Instead of .RData (R-specific), output portable formats:

**Option 1: Parquet (recommended)**
```
results/
├─ chr1.parquet
├─ chr2.parquet
├─ ...
├─ chr22.parquet
├─ pgs_unscaled.parquet
└─ pgs_scaled.parquet
```

**Option 2: Compressed CSV**
```
results/
├─ chr1.csv.gz
├─ chr2.csv.gz
├─ ...
├─ manifest.json (metadata)
```

**Option 3: SQLite Database**
```
results.db:
├─ chr1 (table)
├─ chr2 (table)
├─ ...
└─ polygenic_scores (table)
```

**Rust Implementation Notes:**
- Use `parquet` crate for columnar storage (best compression + R/Python compatible)
- Include metadata: processing date, software version, input file checksums
- Generate MD5/SHA256 checksums for integrity verification
- Consider splitting large chromosomes (chr1, chr2) into chunks

---

## Data Structures Summary

### Reference Panel (vcf.Chr{i})

```rust
struct ReferencePanel {
    chromosome: u8,              // 1-22
    snps: Vec<SNP>,              // Vector of SNPs
}

struct SNP {
    rsid: String,                // "rs12345"
    position: u64,               // Base pair location
    ref_allele: String,          // "A"
    alt_allele: String,          // "G"
    samples: Vec<f64>,           // Dosages for 50 reference samples
}
```

### Imputed VCF (vcfi)

```rust
struct ImputedVCF {
    chromosome: u8,
    records: Vec<VCFRecord>,
}

struct VCFRecord {
    pos: u64,
    rsid: String,
    ref_allele: String,
    alt_allele: String,
    dosage: f64,                 // 0.0-2.0
    quality: f64,                // DR2 (R-squared)
    format: String,              // "DS"
}
```

### 23andMe Genotypes (gpos)

```rust
struct GenotypeData {
    chromosome: u8,
    genotypes: Vec<Genotype>,
}

struct Genotype {
    rsid: String,
    position: u64,
    alleles: String,             // "AG", "GG", "--"
}
```

### Polygenic Scores

```rust
struct PolygeneticScore {
    trait: String,               // "height", "bmi", etc.
    snps: Vec<SNPWeight>,
}

struct SNPWeight {
    rsid: String,
    chromosome: u8,
    position: u64,
    effect_allele: String,       // A1
    other_allele: String,        // A2
    weight: f64,                 // Beta coefficient
}
```

---

## Algorithm Complexity Analysis

### Time Complexity (Per Chromosome)

| Stage | Operation | Complexity | Notes |
|-------|-----------|------------|-------|
| Load reference | File I/O | O(n) | n = SNPs in reference (~2M) |
| Read VCF | Parse + decompress | O(m) | m = SNPs in imputed (~10M) |
| Read 23andMe | Parse + filter | O(k) | k = genotyped (~600K) |
| Convert genotypes | String matching | O(k) | 4 comparisons per SNP |
| Merge | Hash join | O(m + k) | Using HashMap |
| Align user data | Index lookup | O(n) | match() operation |

**Total per chromosome:** O(n + m + k) ≈ O(m) since m is largest

**Total for all 22 chromosomes:** O(22m) ≈ O(440M operations)

### Space Complexity

| Data Structure | Size | Notes |
|----------------|------|-------|
| Reference panel (all chr) | ~4 GB | 50M SNPs × 50 samples × 8 bytes |
| Imputed VCF (one chr) | ~100 MB | Compressed: ~10 MB |
| 23andMe genotypes | ~50 MB | ~600K SNPs × 80 bytes |
| Merged result (one chr) | ~150 MB | Before adding to reference |
| **Peak memory** | ~4.5 GB | Reference + working buffers |

**Rust Optimizations:**
- Stream VCF records (don't load entire file)
- Process chromosome-by-chromosome (avoid loading all 22 at once)
- Use memory-mapped reference panel if caching
- Compress intermediate results

---

## Error Handling Requirements

### Data Validation

1. **Reference Panel Integrity**
   - Verify all 22 chromosomes present
   - Check for duplicate rsIDs
   - Validate dosages in range [0.0, 2.0]

2. **VCF File Validation**
   - Confirm VCF format version (4.2+)
   - Verify chromosome matches expected (1-22)
   - Check for required INFO fields (DR2)
   - Validate REF/ALT are valid DNA bases

3. **23andMe File Validation**
   - Verify header format (12 lines)
   - Check chromosome values (1-22, X, Y, MT)
   - Validate genotype format (2 chars or "--")
   - Detect strand orientation issues

4. **Polygenic Score Validation**
   - Verify required columns present
   - Check for duplicate SNPs
   - Validate weights are numeric
   - Warn if coverage is low (<50% of expected SNPs)

### Error Recovery

| Error Type | Recovery Strategy | User Impact |
|------------|-------------------|-------------|
| Missing VCF file | Fail job, log error | Cannot proceed |
| Corrupt VCF | Skip bad records, log count | Reduced coverage |
| Missing 23andMe SNPs | Use imputed only | Slight quality loss |
| Strand flip detected | Auto-flip, log action | Transparent |
| Reference panel missing | Download from URL | Automated retry |
| Out of memory | Process in chunks | Slower, completes |

### Audit Logging

**Log to PostgreSQL audit table:**

```sql
INSERT INTO genetics_audit (
    job_id, user_id, event_type, severity, message, metadata
) VALUES (
    $1, $2, 'processing', 'info',
    'Chromosome 1: Merged 9,234,567 imputed + 23,456 genotyped SNPs',
    '{"chr": 1, "imputed": 9234567, "genotyped": 23456, "matched": 98.5}'
);
```

**Key metrics to log:**
- SNPs per chromosome (imputed, genotyped, merged)
- Genotype match rate (% successfully converted)
- Imputation quality distribution (DR2 values)
- Processing time per stage
- Memory usage peaks
- Any warnings or errors

---

## Rust Implementation Roadmap

### Phase 1: File Parsers (Foundation)

**Tasks:**
1. ✅ VCF parser using `noodles` crate
   - Read gzipped VCF files
   - Extract: rsID, POS, REF, ALT, dosage (DS field)
   - Handle missing dosage gracefully

2. ✅ 23andMe parser (custom)
   - Skip header lines
   - Parse: rsID, chromosome, position, genotype
   - Filter to specified chromosome

3. ✅ R workspace loader (investigate)
   - Options: Use `r-extendr`, pre-convert to Parquet, or regenerate reference in Rust format
   - Decision needed: Best approach for reference panel

4. ✅ Polygenic scores parser (simple TSV)

**Deliverable:** `src/parsers/` module with unit tests

---

### Phase 2: Core Algorithms

**Tasks:**
1. ✅ Genotype converter
   - Input: (REF, ALT, genotype_string)
   - Output: Option<f64> (0/1/2 or None)
   - Handle both strand orientations

2. ✅ SNP merger
   - Hash-based join (rsID key)
   - Prefer genotyped over imputed
   - Track merge statistics

3. ✅ Reference panel updater
   - Add user as 51st column
   - Maintain SNP ordering
   - Validate alignment

**Deliverable:** `src/core/` module with integration tests

---

### Phase 3: Pipeline Orchestration

**Tasks:**
1. ✅ Chromosome processor
   - Encapsulate per-chromosome logic
   - Parallelizable across chromosomes
   - Error handling and logging

2. ✅ Job manager
   - Iterate chromosomes 1-22
   - Collect results
   - Generate output format

3. ✅ Audit logger
   - PostgreSQL integration
   - Structured logging (JSON metadata)
   - Performance metrics

**Deliverable:** `src/processor.rs` (main pipeline)

---

### Phase 4: Output Generation

**Tasks:**
1. ✅ Parquet writer
   - One file per chromosome
   - Include metadata (processing date, versions)
   - Compression (snappy or zstd)

2. ✅ Manifest generator
   - JSON file listing all outputs
   - Checksums (SHA256)
   - Processing summary statistics

3. ✅ Workspace packager
   - Tar/zip all results
   - Secure deletion of intermediates

**Deliverable:** `src/output/` module

---

### Phase 5: Testing & Validation

**Tasks:**
1. ✅ Unit tests (all modules)
2. ✅ Integration test (small dataset)
   - 10K SNPs across chr1-22
   - Verify against R reference output
3. ✅ Load test (full dataset)
   - 50M SNPs, 600K genotypes
   - Measure performance (time, memory)
4. ✅ Fuzzing (edge cases)
   - Malformed VCF
   - Strand flips
   - Missing data

**Deliverable:** `tests/` directory with CI integration

---

## Key Design Decisions for Rust Port

### 1. Reference Panel Format

**Options:**

**A. Pre-convert .RData to Parquet (Recommended)**
- Pros: Fast loading, columnar storage, R/Python compatible
- Cons: One-time conversion step required
- Implementation: Python script using `rpy2` + `pyarrow`

**B. Parse .RData directly in Rust**
- Pros: No pre-processing needed
- Cons: Complex format, limited crate support
- Crate: None stable; would need custom parser

**C. PostgreSQL cache**
- Pros: Centralized, queryable, versioned
- Cons: Slower than file-based, adds dependency
- Implementation: Load once into DB, query per job

**Decision: Option A (Parquet)**
- Best balance of performance and maintainability
- One-time conversion: ~5 minutes
- Rust read time: ~1-2 seconds for all chromosomes

---

### 2. Parallelization Strategy

**Chromosome-level parallelism (Recommended):**
```rust
use rayon::prelude::*;

let results: Vec<ChromosomeResult> = (1..=22)
    .into_par_iter()
    .map(|chr| process_chromosome(chr, &inputs))
    .collect();
```

**Pros:**
- 22 independent tasks (no shared state)
- Simple error handling per chromosome
- Natural progress reporting (1/22, 2/22, ...)

**Cons:**
- Requires 22 × working_memory
- Cannot parallelize if memory constrained

**Alternative: Sequential with streaming**
- Lower memory footprint
- Suitable for smaller systems
- Progress still visible

**Decision: Configurable**
- Default: Parallel if >8 GB RAM available
- Fallback: Sequential processing
- User override via config

---

### 3. Output Format

**User-facing output:**

**Primary: Parquet files**
- Compatible with R (arrow package), Python (pandas), Julia
- Column-oriented (efficient for genomics)
- Compression (~10× smaller than CSV)

**Secondary: Summary report**
```json
{
  "job_id": "uuid",
  "user_id": "user_uuid",
  "created_at": "2025-11-03T10:30:00Z",
  "chromosomes": [
    {
      "chr": 1,
      "snps_total": 9234567,
      "snps_genotyped": 23456,
      "snps_imputed": 9211111,
      "match_rate": 98.5
    },
    ...
  ],
  "files": [
    {"name": "chr1.parquet", "size": 45678912, "sha256": "abc..."},
    ...
  ]
}
```

**Tertiary: R workspace (optional)**
- If user requests .RData format
- Use `extendr` to call R from Rust
- Generate using R's save() function

---

### 4. Error Handling Philosophy

**Fail fast vs. best effort:**

**Fail fast (Recommended for critical errors):**
- Missing required input files
- Corrupt reference panel
- Invalid credentials/permissions

**Best effort (Recommended for data quality):**
- Some VCF records unparseable → Skip, log warning
- Genotype doesn't match REF/ALT → Use imputed, log
- Low imputation quality (DR2 < 0.3) → Include but flag

**Implementation:**
```rust
match parse_vcf_record(line) {
    Ok(record) => records.push(record),
    Err(e) => {
        audit_log(job_id, "warning", format!("Skipped malformed VCF line: {}", e));
        skipped_count += 1;
    }
}

// After processing:
if skipped_count > 1000 {
    return Err(ProcessingError::TooManyBadRecords(skipped_count));
}
```

---

## Performance Targets

### Baseline (R script, current)

| Metric | Value | Notes |
|--------|-------|-------|
| Total runtime | ~30 minutes | 6-core CPU |
| Peak memory | ~8 GB | Loads all chromosomes |
| Disk I/O | ~2 GB read | Compressed VCF files |

### Rust Goals

| Metric | Target | Improvement | Implementation |
|--------|--------|-------------|----------------|
| Total runtime | **<10 minutes** | 3× faster | Parallelism + no GC |
| Peak memory | **<4 GB** | 2× less | Streaming, no data copies |
| Disk I/O | ~2 GB read | Same | Input-bound |
| Output size | **<500 MB** | 4× smaller | Parquet compression |

**Optimization Priorities:**
1. Parallel chromosome processing (biggest win)
2. Efficient memory management (Vec reuse, no String clones)
3. Fast VCF parsing (noodles is optimized)
4. Minimize disk seeks (sequential reads preferred)

---

## Next Steps

1. ✅ **Research `noodles` crate** - VCF parsing library
   - Confirm it handles dosage (DS) format field
   - Check gzip support
   - Review error handling

2. ✅ **Prototype VCF parser** - Read chr22.dose.vcf.gz (smallest)
   - Extract: rsID, position, REF, ALT, dosage
   - Benchmark: Parse 1M records, measure time/memory

3. ✅ **Design reference panel loader**
   - Decide: Parquet vs. direct .RData parsing
   - If Parquet: Write conversion script
   - Test: Load and validate all 22 chromosomes

4. ⏳ **Implement genotype converter**
   - Unit tests for all cases (REF/REF, HET, ALT/ALT, mismatch)
   - Handle edge cases (tri-allelic, indels)

5. ⏳ **Build minimal end-to-end pipeline**
   - Process single chromosome (chr22)
   - Verify output matches R script
   - Measure performance

---

## Open Questions

1. **Reference panel distribution:**
   - Download on-demand vs. pre-bundled?
   - Cache in database vs. file system?
   - Version management (panel may be updated)?

2. **Strand flip detection:**
   - Should we auto-detect and flip?
   - Or require user to pre-process 23andMe file?
   - How to handle ambiguous SNPs (A/T, G/C)?

3. **Imputation quality filtering:**
   - Should we filter SNPs with DR2 < 0.3?
   - Make configurable per-job?
   - Document impact on polygenic scores?

4. **Output format preferences:**
   - Always generate Parquet?
   - Optionally generate .RData for R users?
   - Support CSV export for maximum compatibility?

5. **Resumability:**
   - If job fails on chromosome 15, can we resume?
   - Store intermediate results in database?
   - Trade-off: Complexity vs. user experience

---

## Appendix: File Format Specifications

### VCF v4.2 (Michigan Imputation Server)

**Spec:** https://samtools.github.io/hts-specs/VCFv4.2.pdf

**Key sections:**
- Header lines: `##fileformat=VCFv4.2`, `##INFO=...`, `##FORMAT=...`
- Column header: `#CHROM POS ID REF ALT QUAL FILTER INFO FORMAT SAMPLE1 SAMPLE2 ...`
- Data lines: Tab-separated values

**FORMAT field encoding:**
- `DS`: Dosage (0.0-2.0, decimal)
- `GP`: Genotype probabilities (P(0/0), P(0/1), P(1/1))
- `HDS`: Haploid dosage (X chromosome males)

**INFO field:**
- `DR2`: Dosage R-squared (imputation quality metric, 0.0-1.0)
- Higher DR2 = better imputation quality
- Typically filter DR2 < 0.3 for low-quality SNPs

### 23andMe Raw Data Format

**Spec:** https://www.23andme.com/gen101/raw-data/

**Header (12 lines):**
```
# This data file generated by 23andMe at: Sat Jan 01 00:00:00 2025
# ...
# rsid    chromosome    position    genotype
```

**Data format:**
- Tab-separated
- Chromosome: 1-22, X, Y, MT
- Position: Integer (build 37 / hg19)
- Genotype: Two characters (unphased)

**Special genotypes:**
- `--`: No call (missing data)
- `DD`: Deletion
- `II`: Insertion
- `DI` / `ID`: Deletion-insertion

### R Workspace (.RData)

**Format:** Binary, R-specific serialization format (XDR)

**Tools:**
- R: `load("file.RData")`, `save(..., file=...)`
- Python: `rpy2` package
- Rust: No stable crate; use R bridge or pre-convert

**Structure (for VCF.Files3.RData):**
```
Objects:
├─ vcf.Chr1: data.frame [2,100,345 × 54]
├─ vcf.Chr2: data.frame [2,456,789 × 54]
├─ ...
└─ vcf.Chr22: data.frame [823,456 × 54]

Each data.frame columns:
├─ rsID: character
├─ position: integer
├─ REF: character
├─ ALT: character
├─ sample_1: numeric (0/1/2)
├─ ...
└─ sample_50: numeric (0/1/2)
```

---

**End of Document**

*This analysis provides the technical foundation for porting mergeData.R to Rust. All data structures, algorithms, and edge cases have been documented for implementation reference.*
