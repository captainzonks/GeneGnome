# Reference Data Guide

This document explains the reference databases used by GeneGnome, where to obtain them, and how to prepare them for use.

## Overview

GeneGnome uses two types of reference data:

1. **Genotyped Reference Panel** (`genotyped.anon.RData`) - For VCF file generation
2. **Imputed Reference Panel** (`VCF.Files3.RData` or `reference_panel.db`) - For merging imputation results

Both contain 50 anonymous genome samples originally from openSNP.org (now closed), freely uploaded by users for research purposes.

---

## 1. Genotyped Reference Panel

### Purpose

Used by the browser-based VCF generator to create multi-sample VCF files required by Michigan Imputation Server.

### Why Multi-Sample?

Michigan Imputation Server requires VCF files with **multiple samples** to perform accurate imputation. Single-sample VCFs are rejected. The solution is to merge your 23andMe data with 50 anonymous genotyped samples.

### Download Location

```bash
# Download genotyped reference panel (14 MB)
wget http://www.matthewckeller.com/public/genotyped.anon.RData
```

### File Details

- **Format**: R binary data file (.RData)
- **Size**: ~14 MB
- **Content**: 50 anonymous samples with ~700,000 genotyped SNPs per sample
- **Build**: GRCh37 (hg19)
- **Chromosomes**: 22 autosomal chromosomes (1-22)
- **Objects**: `chr1`, `chr2`, ..., `chr22` (one data frame per chromosome)

### Data Structure

Each chromosome object (e.g., `chr1`) is a data frame with columns:

```r
# Example structure
str(chr1)
# 'data.frame':   ~700000 obs. of 59 variables:
#  $ CHROM  : chr  "1" "1" "1" ...
#  $ POS    : int  12345 23456 34567 ...
#  $ ID     : chr  "rs12345" "rs23456" "rs34567" ...
#  $ REF    : chr  "A" "G" "C" ...
#  $ ALT    : chr  "G" "A" "T" ...
#  $ QUAL   : chr  "." "." "." ...
#  $ FILTER : chr  "PASS" "PASS" "PASS" ...
#  $ INFO   : chr  "." "." "." ...
#  $ FORMAT : chr  "GT" "GT" "GT" ...
#  $ samp1  : chr  "0/1" "1/1" "0/0" ...
#  $ samp2  : chr  "0/0" "0/1" "1/1" ...
#  ...
#  $ samp50 : chr  "0/1" "0/0" "0/1" ...
```

### Usage in GeneGnome

This reference panel is **currently NOT used** by the Rust-based GeneGnome processor. It was used by the original R script (`vcf_gen.R`) to generate VCF files for Michigan Imputation Server submission.

**For the WebAssembly VCF generator**: We would need to convert this RData file to a WASM-compatible format (JSON, SQLite, or embedded in the binary). This is a **future enhancement** not yet implemented.

---

## 2. Imputed Reference Panel

### Purpose

Used by the Rust processor to merge Michigan Imputation Server results with your 23andMe data for polygenic score calculations and comprehensive genomic analysis.

### Download Location

```bash
# Download imputed reference panel (167 MB)
wget http://www.matthewckeller.com/public/VCF.Files3.RData
```

### File Details

- **Format**: R binary data file (.RData)
- **Size**: ~167 MB
- **Content**: 50 anonymous samples with ~6 million imputed variants per sample
- **Build**: GRCh37 (hg19)
- **Chromosomes**: 22 autosomal chromosomes
- **Objects**: `vcf.Chr1`, `vcf.Chr2`, ..., `vcf.Chr22`

### Data Structure

Each chromosome object (e.g., `vcf.Chr1`) is a data frame with imputed variants:

```r
# Example structure
str(vcf.Chr1)
# 'data.frame':   ~300000 obs. of 60 variables:
#  $ bp37   : int  10177 10235 10352 ...
#  $ rsid   : chr  "rs367896724" "rs540431307" ...
#  $ chrom  : int  1 1 1 ...
#  $ REF    : chr  "A" "T" "C" ...
#  $ ALT    : chr  "AC" "TA" "T" ...
#  $ QUAL   : chr  "." "." "." ...
#  $ FILTER : chr  "PASS" "PASS" "PASS" ...
#  $ AF     : num  0.425 0.409 0.362 ...      # Allele Frequency
#  $ MAF    : num  0.425 0.409 0.362 ...      # Minor Allele Frequency
#  $ R2     : num  0.981 0.908 0.897 ...      # Imputation Quality
#  $ PHASED : int  1 1 1 ...                   # Phased (1) or Unphased (0)
#  $ TYPED  : int  0 0 0 ...                   # Genotyped (1) or Imputed (0)
#  $ samp1  : chr  "0|1" "0|1" "0|0" ...       # Genotype for sample 1
#  $ samp2  : chr  "1|1" "0|0" "0|1" ...       # Genotype for sample 2
#  ...
#  $ samp50 : chr  "0|1" "1|0" "0|0" ...       # Genotype for sample 50
```

### Conversion to SQLite (For Rust Processor)

Rust cannot read R binary data files (`.RData`), so we convert the reference panel to SQLite:

```bash
# Run conversion script (requires R with DBI, RSQLite, jsonlite)
cd genome-data/R_Scripts
Rscript convert_reference_to_db.R
```

This creates `reference_panel.db` (~4.7 GB) with the following schema:

#### SQLite Schema

```sql
-- Metadata table
CREATE TABLE metadata (
  key TEXT PRIMARY KEY,
  value TEXT
);

-- Reference variants table
CREATE TABLE reference_variants (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  chromosome INTEGER NOT NULL,
  position INTEGER NOT NULL,
  rsid TEXT,
  ref_allele TEXT NOT NULL,
  alt_allele TEXT NOT NULL,
  phased INTEGER,                      -- 1 if phased, 0 if unphased
  allele_freq REAL,                    -- Allele frequency (AF)
  minor_allele_freq REAL,              -- Minor allele frequency (MAF)
  imputation_quality REAL,             -- Imputation quality (R2)
  is_typed INTEGER,                    -- 1 if genotyped, 0 if imputed
  sample_genotypes TEXT NOT NULL       -- JSON: {"samp1": "0|1", "samp2": "1|0", ...}
);

-- Indexes for efficient lookups
CREATE INDEX idx_chr_pos ON reference_variants(chromosome, position);
CREATE INDEX idx_rsid ON reference_variants(rsid);
```

#### Conversion Process

The `convert_reference_to_db.R` script:

1. Loads `VCF.Files3.RData` into R
2. Creates SQLite database with schema above
3. Iterates through each chromosome (1-22)
4. Converts 50-sample genotypes to JSON string
5. Bulk inserts all variants into `reference_variants` table
6. Creates indexes for fast chromosome+position lookups

**Total variants**: ~5.9 million across 22 chromosomes

---

## 3. Setup Instructions

### Option A: Direct Download (Recommended for Rust Processor)

```bash
# Create reference directory
mkdir -p reference

# Download imputed reference panel (167 MB)
cd reference
wget http://www.matthewckeller.com/public/VCF.Files3.RData

# Convert to SQLite (requires R)
Rscript ../scripts/convert_reference_to_db.R
# This creates reference_panel.db (~4.7 GB)

# Clean up RData file (optional, saves 167 MB)
rm VCF.Files3.RData
```

### Option B: Using Provided R Script

If you have R installed with required packages:

```bash
# Install R dependencies
R -e "install.packages(c('DBI', 'RSQLite', 'jsonlite'))"

# Download and convert in one step
cd reference
wget http://www.matthewckeller.com/public/VCF.Files3.RData
Rscript convert_reference_to_db.R
```

The conversion script is included at: `scripts/convert_reference_to_db.R`

### Option C: Pre-converted Database (Future)

We may provide pre-converted `reference_panel.db` for download in future releases to avoid requiring R installation.

---

## 4. Data Provenance & Attribution

### Original Source

Both reference panels originate from **openSNP.org**, a now-defunct open genomics platform where users voluntarily uploaded their genetic data for research purposes.

- **openSNP.org**: Community-driven platform (2011-~2020)
- **License**: Data freely uploaded by users for any research purpose
- **Privacy**: All 50 samples are completely anonymous (no identifiable information)

### Current Mirror

Data is currently hosted by **Dr. Matthew C. Keller** (Institute for Behavioral Genetics, University of Colorado Boulder):

- **Website**: http://www.matthewckeller.com/
- **Direct Links**:
  - Genotyped: http://www.matthewckeller.com/public/genotyped.anon.RData
  - Imputed: http://www.matthewckeller.com/public/VCF.Files3.RData

### Academic Context

These reference panels were created for teaching purposes in Dr. Keller's behavioral genetics course. They allow students to:
- Learn about imputation without uploading personal data
- Calculate polygenic scores using real (but anonymous) genetic data
- Understand genomic data structures and analysis pipelines

### Citation

If you use these reference panels in research, please cite:

> Keller, M.C. (2023). Anonymous Reference Panel for Genetic Imputation and Polygenic Score Analysis. Institute for Behavioral Genetics, University of Colorado Boulder. Retrieved from http://www.matthewckeller.com/public/

---

## 5. Alternative Reference Panels

If Dr. Keller's server is unavailable, you can use alternative imputation services:

### Michigan Imputation Server

- **URL**: https://imputationserver.sph.umich.edu/
- **Reference Panels**: HRC, 1000 Genomes, CAAPA, TOPMed
- **Advantage**: More comprehensive, higher quality imputation
- **Disadvantage**: Requires account registration and data upload

### TOPMed Imputation Server

- **URL**: https://imputation.biodatacatalyst.nhlbi.nih.gov/
- **Reference Panel**: TOPMed r2 (>90,000 samples)
- **Advantage**: Largest reference panel available
- **Disadvantage**: Requires NHLBI account and eRA Commons login

### 1000 Genomes Reference Panel

- **URL**: http://www.internationalgenome.org/
- **Download**: VCF files for all chromosomes
- **Format**: Standard VCF (can be converted to GeneGnome format)
- **Advantage**: Publicly available, no registration required
- **Disadvantage**: Requires significant processing to convert

---

## 6. File Size Considerations

| File | Format | Size | Chromosomes | Variants | Compression |
|------|--------|------|-------------|----------|-------------|
| genotyped.anon.RData | RData | 14 MB | 22 | ~700K | R compression |
| VCF.Files3.RData | RData | 167 MB | 22 | ~5.9M | R compression |
| reference_panel.db | SQLite | 4.7 GB | 22 | ~5.9M | None |
| reference_panel.db (compressed) | SQLite + gzip | ~1.2 GB | 22 | ~5.9M | gzip -9 |

**Storage Recommendation**: Keep `reference_panel.db` on your encrypted volume (`/mnt/genetics-encrypted`) or dedicated storage partition. The 4.7 GB database allows fast random access for the Rust processor.

---

## 7. Verifying Data Integrity

### Check Download Integrity

```bash
# Download and verify file sizes
wget http://www.matthewckeller.com/public/VCF.Files3.RData
ls -lh VCF.Files3.RData
# Expected: ~167 MB

# Count variants (requires R)
R -e "load('VCF.Files3.RData'); sum(sapply(1:22, function(i) nrow(get(paste0('vcf.Chr', i)))))"
# Expected: ~5,900,000 variants
```

### Verify SQLite Database

```bash
# Check database file
ls -lh reference/reference_panel.db
# Expected: ~4.7 GB

# Query variant counts per chromosome
sqlite3 reference/reference_panel.db "
SELECT chromosome, COUNT(*) as variant_count
FROM reference_variants
GROUP BY chromosome
ORDER BY chromosome;
"
# Expected: Chromosome 1 should have most variants (~300K-400K)
```

### Validate with Test Data

```bash
# Use provided example data to test database access
docker-compose run --rm genetics-worker \
  cargo test --release test_reference_panel_lookup
```

---

## 8. Privacy & Ethics

### Anonymous Data

All 50 samples in both reference panels are:
- **Completely anonymous**: No names, emails, or identifiable information
- **Freely contributed**: Users uploaded to openSNP.org for research purposes
- **Public domain**: Released without restriction for any research use

### Responsible Use

When using GeneGnome with **your own genetic data**:

- ✅ **DO** keep your data on encrypted volumes
- ✅ **DO** use secure passwords for downloads
- ✅ **DO** self-host on your own infrastructure
- ✅ **DO** enable automatic deletion after processing
- ❌ **DON'T** upload real genetic data to untrusted servers
- ❌ **DON'T** share download links publicly
- ❌ **DON'T** use reference panel data to attempt re-identification

---

## 9. Troubleshooting

### Download Failures

If downloads fail:

```bash
# Use curl with resume capability
curl -C - -O http://www.matthewckeller.com/public/VCF.Files3.RData

# Or use wget with retries
wget --tries=10 --timeout=30 http://www.matthewckeller.com/public/VCF.Files3.RData
```

### Conversion Failures

If R conversion fails:

```bash
# Check R version (requires 4.0+)
R --version

# Install required packages
R -e "install.packages(c('DBI', 'RSQLite', 'jsonlite'), repos='https://cloud.r-project.org')"

# Try conversion with verbose output
Rscript convert_reference_to_db.R 2>&1 | tee conversion.log
```

### Database Corruption

If SQLite database is corrupted:

```bash
# Verify database integrity
sqlite3 reference_panel.db "PRAGMA integrity_check;"
# Expected: "ok"

# If corrupted, rebuild from RData
rm reference_panel.db
Rscript convert_reference_to_db.R
```

---

## 10. Future Enhancements

Planned improvements for reference data handling:

1. **Pre-converted databases**: Provide `reference_panel.db` for direct download (avoid R dependency)
2. **WASM integration**: Convert `genotyped.anon.RData` to JSON/SQLite for browser VCF generator
3. **Additional panels**: Support for 1000 Genomes, TOPMed, gnomAD reference data
4. **Compression**: Add optional Brotli/Zstd compression for smaller downloads
5. **Checksums**: SHA256 hashes for download verification
6. **Mirror hosting**: Set up redundant mirrors for reliability

---

## Additional Resources

- **Michigan Imputation Server Docs**: https://imputationserver.readthedocs.io/
- **VCF Format Specification**: https://samtools.github.io/hts-specs/VCFv4.2.pdf
- **openSNP Archive**: https://web.archive.org/web/*/opensnp.org
- **Dr. Keller's Course Materials**: http://www.matthewckeller.com/R.Class/

---

**Last Updated**: 2025-11-20
**Author**: Matthew Barham
**License**: Apache-2.0 OR MIT
