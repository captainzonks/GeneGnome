# mergeData.R Processing Pipeline Specification

<!--
==============================================================================
R Processing Pipeline Specification
==============================================================================
Description: Technical specification for porting mergeData.R to Rust
Author: Matt Barham
Created: 2025-11-04
Modified: 2025-11-04
Version: 1.0.0
Repository: https://github.com/captainzonks/GeneGnome
==============================================================================
Document Type: Technical Specification
Audience: Developer
Status: Draft
==============================================================================
-->

## Overview

This document provides a detailed technical specification of the `mergeData.R` processing pipeline to guide Rust implementation. The pipeline merges personal genomic data (23andMe), imputed variants (Michigan Imputation Server 2 VCF files), and polygenic scores (PGS) into a single R workspace file.

**Source File**: `mergeData.R` (374 lines)
**Target Implementation**: Rust (stisty-server)
**Reference Panel**: 50 anonymous samples from openSNP

## Processing Pipeline Overview

```
Input Files:
├── genome_Full_20180110025702.txt    (23andMe genotyped data)
├── chr1.dose.vcf.gz ... chr22.dose.vcf.gz    (Michigan Imputation Server 2 output)
├── scores.txt                        (Polygenic score data)
└── VCF.Files3.RData                  (Reference panel - 50 samples)

Processing Steps:
1. Setup and Configuration
2. Load Reference Panel
3. For each chromosome (1-22):
   a. Load 23andMe genotyped SNPs
   b. Load VCF imputed SNPs
   c. Merge genotyped and imputed data
   d. Handle REF/ALT allele orientation
4. Load and Scale PGS Data
5. Merge PGS with Genomic Data

Output:
└── GenomicData4152.RData             (R workspace with all merged data)
```

## Input File Formats

### 1. 23andMe Genome File

**File**: `genome_Full_20180110025702.txt`
**Format**: Tab-delimited text file with header comments

**Structure**:
```
# rsid    chromosome    position    genotype
rs548049170    1    69869    TT
rs13328684    1    74792    --
rs9283150    1    565508    AA
```

**Column Names** (as read by R):
- `rsid`: SNP identifier (e.g., "rs548049170")
- `chrom`: Chromosome number (1-22, X, Y, MT)
- `bp37`: Base pair position (GRCh37/hg19 assembly)
- `gt51`: Genotype (e.g., "TT", "AA", "AG", "--" for missing)

**R Code**:
```r
gpos6 <- read.table(mygenomefile_loc, header = FALSE)
names(gpos6) <- c("rsid", "chrom", "bp37", "gt51")
```

**Key Characteristics**:
- Missing genotypes represented as "--"
- Autosomal chromosomes: 1-22
- Sex chromosomes: X, Y
- Mitochondrial: MT
- Build: GRCh37 (hg19)

### 2. VCF Imputed Files (Michigan Imputation Server 2)

**Files**: `chr1.dose.vcf.gz` through `chr22.dose.vcf.gz`
**Format**: Compressed VCF (Variant Call Format) with dosage data

**Structure** (simplified):
```
#CHROM    POS    ID    REF    ALT    QUAL    FILTER    INFO    FORMAT    SAMPLE
1    69869    rs548049170    T    C    .    PASS    R2=0.99;DS=1.95    DS:GP    1.95:0,0,1
```

**Column Names** (as read by R):
- `chrom`: Chromosome number (1-22)
- `bp37`: Base pair position (GRCh37/hg19)
- `REF`: Reference allele (e.g., "T")
- `ALT`: Alternate allele (e.g., "C")
- `dat51`: Dosage data for sample (numeric 0-2)

**R Code**:
```r
fields <- c("integer", "integer", "character", "character", rep("NULL", 4), "numeric")
vcfi <- read.table(paste0("chr", i, ".dose.vcf.gz"), header = FALSE, colClasses = fields)
names(vcfi) <- c("chrom", "bp37", "REF", "ALT", "dat51")
```

**Key Characteristics**:
- Only reads columns 1, 2, 3, 4, and 10 (skips 5-9)
- Column 10 contains dosage value (0-2 scale)
- R² (imputation quality) in INFO field (not extracted in R code)
- Missing data: Handled by dosage values

### 3. Polygenic Score File

**File**: `scores.txt`
**Format**: Comma-delimited CSV with header

**Structure**:
```
ID,PGS_label,score_value
sample1,Height,1.234
sample1,BMI,0.456
sample2,Height,1.567
```

**Column Names**:
- `ID`: Sample identifier
- `PGS_label`: Polygenic score name/label
- `score_value`: Raw PGS value (numeric)

**R Code**:
```r
pgs <- read.table("scores.txt", header = TRUE, sep = ",")
```

### 4. Reference Panel

**File**: `VCF.Files3.RData`
**Format**: R workspace file containing reference panel data

**Expected Contents**:
- `vcf1` through `vcf22`: Data frames for each chromosome
- Each data frame contains VCF data for 50 reference samples
- Column structure similar to imputed VCF files

**R Code**:
```r
load("VCF.Files3.RData")  # Loads vcf1, vcf2, ..., vcf22
```

## Processing Steps (Detailed)

### Step 1: Setup and Configuration (Lines 1-63)

**Purpose**: Initialize working directory and file paths

**R Code**:
```r
BIGDIR <- "/tmp/MichiganImputation2"
setwd(BIGDIR)
mygenomefile_loc <- "/path/to/your/genome_Full_YYYYMMDDXXXXXX.txt"
```

**Rust Equivalent**:
- Configuration struct with file paths
- Working directory setup
- Validation that all required files exist

### Step 2: Load Reference Panel (Lines 153-171)

**Purpose**: Load reference panel data (50 samples) and helper functions

**R Code**:
```r
# Load reference panel
load("VCF.Files3.RData")  # Creates vcf1, vcf2, ..., vcf22

# Load helper functions
source("helper_fns.R")
```

**Key Data**:
- `vcf1` through `vcf22`: Reference panel data for each chromosome
- Helper functions for data manipulation (not in provided R file)

**Rust Equivalent**:
- Parse VCF.Files3.RData (requires R workspace parser or pre-conversion)
- Implement equivalent helper functions in Rust
- Store reference data in memory-efficient structures

### Step 3: Merge Own Genomic Data (Lines 188-249)

**Purpose**: For each chromosome, merge 23andMe genotyped data with imputed VCF data

#### Step 3a: Load 23andMe Data

**R Code**:
```r
gpos6 <- read.table(mygenomefile_loc, header = FALSE)
names(gpos6) <- c("rsid", "chrom", "bp37", "gt51")
```

**Key Points**:
- Read entire 23andMe file once
- Filter by chromosome in loop
- ~600,000-1,000,000 SNPs total

#### Step 3b: Process Each Chromosome (Loop i=1 to 22)

**R Code**:
```r
for (i in 1:22) {
  # Filter 23andMe data for this chromosome
  gpos6i <- gpos6[gpos6$chrom == i, ]

  # Load imputed VCF for this chromosome
  fields <- c("integer", "integer", "character", "character",
              rep("NULL", 4), "numeric")
  vcfi <- read.table(paste0("chr", i, ".dose.vcf.gz"),
                     header = FALSE, colClasses = fields)
  names(vcfi) <- c("chrom", "bp37", "REF", "ALT", "dat51")

  # Merge datasets
  # ... merging logic ...
}
```

#### Step 3c: Merging Algorithm

**Key Operations**:

1. **Match by Position**: Merge on `chrom` and `bp37` (base pair position)

2. **Handle Genotype Orientation**:
   - 23andMe genotypes: Two-letter codes (e.g., "TT", "AG")
   - VCF REF/ALT: Single letters (e.g., REF="T", ALT="C")
   - Dosage: 0 = REF/REF, 1 = REF/ALT, 2 = ALT/ALT

3. **Conversion Logic**:
   ```r
   # If 23andMe genotype matches VCF REF allele:
   if (gpos6i$gt51 == paste0(vcfi$REF, vcfi$REF)) {
     dosage <- 0  # Homozygous reference
   }
   # If heterozygous (one REF, one ALT):
   else if (gpos6i$gt51 == paste0(vcfi$REF, vcfi$ALT) ||
            gpos6i$gt51 == paste0(vcfi$ALT, vcfi$REF)) {
     dosage <- 1  # Heterozygous
   }
   # If homozygous alternate:
   else if (gpos6i$gt51 == paste0(vcfi$ALT, vcfi$ALT)) {
     dosage <- 2  # Homozygous alternate
   }
   ```

4. **Handle Missing Data**:
   - 23andMe: "--" for no call
   - VCF: Use imputed dosage value (0-2 continuous)

5. **Priority**:
   - Use 23andMe genotyped data when available (higher quality)
   - Use VCF imputed data for missing positions
   - Imputed data provides coverage for ungenotyped SNPs

**Output per Chromosome**:
- Merged data frame with columns:
  - `chrom`: Chromosome number
  - `bp37`: Position
  - `rsid`: SNP identifier
  - `REF`: Reference allele
  - `ALT`: Alternate allele
  - `dosage`: Allele dosage (0-2)
  - `source`: "genotyped" or "imputed"

### Step 4: Merge PGS Data (Lines 271-296)

**Purpose**: Load polygenic scores and merge with genomic data

#### Step 4a: Load PGS Data

**R Code**:
```r
pgs <- read.table("scores.txt", header = TRUE, sep = ",")
```

#### Step 4b: Scale PGS Values

**Scaling Algorithm**:
```r
# For each PGS label:
pgs.scaled <- pgs
for (label in unique(pgs$PGS_label)) {
  subset <- pgs[pgs$PGS_label == label, ]

  # Z-score normalization
  mean_val <- mean(subset$score_value)
  sd_val <- sd(subset$score_value)
  pgs.scaled[pgs.scaled$PGS_label == label, "score_value"] <-
    (subset$score_value - mean_val) / sd_val
}
```

**Key Points**:
- Z-score normalization per PGS trait
- Preserves original unscaled values
- Creates scaled version for analysis

#### Step 4c: Merge with Genomic Data

**R Code** (conceptual):
```r
# Add PGS data to genomic workspace
# (Actual implementation may vary)
```

### Step 5: Save Output (Line 315)

**Purpose**: Save all merged data to R workspace

**R Code**:
```r
save(vcf1, vcf2, ..., vcf22, pgs.unscaled, pgs.scaled,
     file = "GenomicData4152.RData")
```

**Output Contents**:
- `vcf1` through `vcf22`: Merged genomic data per chromosome
- `pgs.unscaled`: Original PGS values
- `pgs.scaled`: Z-score normalized PGS values

## Data Structure Requirements

### Chromosome Data (vcf1 - vcf22)

**Schema**:
```rust
struct ChromosomeData {
    chrom: u8,                    // 1-22
    positions: Vec<u32>,          // bp37 positions
    rsids: Vec<String>,           // SNP identifiers
    ref_alleles: Vec<char>,       // Reference alleles
    alt_alleles: Vec<char>,       // Alternate alleles
    dosages: Vec<f32>,            // Allele dosages (0-2)
    sources: Vec<DataSource>,     // Genotyped or Imputed
}

enum DataSource {
    Genotyped,
    Imputed,
}
```

### PGS Data

**Schema**:
```rust
struct PgsData {
    sample_id: String,
    scores: HashMap<String, f32>,  // PGS_label -> score_value
}

struct PgsDataset {
    unscaled: Vec<PgsData>,
    scaled: Vec<PgsData>,
}
```

## Performance Considerations

**R Script Characteristics**:
- Sequential chromosome processing (1-22)
- In-memory operations (all data loaded at once)
- File I/O for each chromosome VCF
- Memory usage: ~2-4 GB for full dataset

**Rust Optimization Opportunities**:
1. **Parallel Processing**: Process chromosomes concurrently
2. **Streaming**: Read VCF files incrementally, not all at once
3. **Memory Efficiency**: Use indexed structures, avoid full data copies
4. **Type Safety**: Compile-time validation of data structures
5. **Error Handling**: Robust handling of malformed input files

## Implementation Recommendations

### Phase 1: Core Data Structures
- Define Rust structs for genomic data
- Implement serialization/deserialization
- Create test data for validation

### Phase 2: File Parsers
- 23andMe parser (simple TSV)
- VCF parser using `noodles` crate
- PGS CSV parser
- R workspace loader (may require external tool)

### Phase 3: Merging Logic
- Position-based merge algorithm
- Genotype-to-dosage conversion
- Missing data handling
- Source tracking (genotyped vs imputed)

### Phase 4: PGS Processing
- Z-score normalization
- Trait-specific scaling
- Integration with genomic data

### Phase 5: Output Generation
- R workspace writer (or alternative format)
- JSON/binary serialization for web API
- Validation and checksums

## Open Questions

1. **R Workspace Format**:
   - Can we parse `VCF.Files3.RData` directly in Rust?
   - Alternative: Pre-convert to JSON/CSV/Parquet?

2. **Helper Functions**:
   - What does `helper_fns.R` contain?
   - Are these functions critical or optional?

3. **Output Format**:
   - Must output be R workspace, or can we use modern format?
   - JSON for API, Parquet for analytics?

4. **Validation**:
   - How do we validate Rust output matches R output?
   - Test dataset for comparison?

5. **Reference Panel**:
   - How often is reference panel updated?
   - Should we download/parse this as part of setup?

## Next Steps

1. Research `noodles` crate VCF parsing capabilities
2. Investigate R workspace parsing in Rust (or conversion tools)
3. Design Rust data structures matching this specification
4. Create test harness with sample data
5. Implement Phase 1 (core data structures)

---

**References**:
- Original R script: `mergeData.R`
- VCF specification: https://samtools.github.io/hts-specs/VCFv4.3.pdf
- noodles crate: https://docs.rs/noodles/latest/noodles/
- 23andMe raw data format: https://customercare.23andme.com/hc/en-us/articles/212196868-Accessing-Your-Raw-Genetic-Data
