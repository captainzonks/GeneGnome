# R Script Output Analysis - GenomicData4152.RData

## Overview

Analysis of the R script output (`GenomicData4152.RData`) to ensure our Rust implementation matches the expected multi-sample format.

**Generated**: 2025-11-12
**R Script Output**: `GenomicData4152.RData` (182 MB)
**Purpose**: Define target output format for 51-sample genomic data processing

---

## File Structure

### Objects in RData File

1. **`pgs.scaled`** - Scaled polygenic scores
   - Dimensions: 4,489 traits × 53 columns
   - Columns: `pgsnum`, `trait`, `1`, `2`, ..., `51` (51 samples)
   - Values: Z-score normalized PGS values

2. **`pgs.unscaled`** - Unscaled polygenic scores
   - Dimensions: 4,489 traits × 53 columns
   - Columns: `pgsnum`, `trait`, `1`, `2`, ..., `51` (51 samples)
   - Values: Raw PGS values

3. **`vcf1` through `vcf22`** - Chromosome-specific variant data
   - 22 data frames (one per autosomal chromosome)
   - Each contains merged 51-sample variant data

---

## VCF Data Frame Structure

### Columns (61 total)

**Variant Information (10 columns):**
- `bp37` - Position (Build 37)
- `rsid` - RS ID (or `<NA>` for imputed-only variants)
- `chrom` - Chromosome number (1-22)
- `REF` - Reference allele
- `ALT` - Alternate allele
- `PHASED` - Boolean (`TRUE`/`FALSE`) - whether variant is phased
- `AF` - Allele frequency
- `MAF` - Minor allele frequency
- `R2` - Imputation quality (R²) - `NA` for genotyped variants
- `TYPED` - Boolean (`TRUE`/`FALSE`) - whether variant was genotyped

**Sample Data (51 columns):**
- `samp1`, `samp2`, ..., `samp51` - Genotype data for 51 samples
  - **Format**: Phased genotypes as strings
  - **Values**: `"0|0"`, `"0|1"`, `"1|0"`, `"1|1"`
  - **Type**: Character/string
  - **Sample 51**: User's sample (merged with 50 reference samples)
  - **Samples 1-50**: Reference panel samples from openSNP

### Sample Data Example

```r
     bp37        rsid REF ALT samp1 samp2 samp3 samp50 samp51
1   69869 rs548049170   T   A   0|0   0|0   0|0    0|0    0|0
2  727841 rs116587930   G   A   0|0   0|0   0|0    0|0    0|0
3  752721   rs3131972   A   G   1|1   1|1   1|0    0|1    0|1  <- Phased
4  754105  rs12184325   C   T   0|0   0|0   0|0    0|0    0|0
5  756268  rs12567639   G   A   1|1   1|1   1|1    1|1    1|1
```

### Variant Counts by Chromosome

```
Chr   Variants
 1    447,224
 2    535,314
 3    463,714
 4    459,324
 5    404,819
 6    405,891
 7    354,105
 8    351,224
 9    249,866
10    309,865
11    301,566
12    286,111
13    220,127
14    187,990
15    148,741
16    156,972
17    120,265
18    164,209
19     89,055
20    118,783
21     67,640
22     57,322

Total: ~5,900,127 variants
```

---

## Key Findings

### ✅ Matches Our Rust Design

1. **51 Samples Total**
   - 50 reference panel samples (samp1-samp50)
   - 1 user sample (samp51)
   - Matches our `MultiSampleVariant` with 51 samples

2. **Genotype Format**
   - Phased format: `"0|0"`, `"0|1"`, `"1|0"`, `"1|1"`
   - Matches our genotype string format in `SampleData`

3. **Sample Naming Convention**
   - R uses: `samp1`, `samp2`, ..., `samp51`
   - Rust uses: `"samp1"`, `"samp2"`, ..., `"samp51"`
   - ✅ **Compatible**

4. **Data Structure**
   - Chromosome-separated data frames
   - Each variant has 51 sample columns
   - Matches our `HashMap<u8, Vec<MultiSampleVariant>>` structure

---

## Comparison: R Output vs. Rust Output

| Aspect | R Script Output | Rust Output | Status |
|--------|----------------|-------------|--------|
| Sample count | 51 (samp1-samp51) | 51 (samp1-samp51) | ✅ Match |
| Genotype format | `"0\|0"`, `"0\|1"`, etc. | `"0\|0"`, `"0\|1"`, etc. | ✅ Match |
| Sample naming | `samp1` through `samp51` | `samp1` through `samp51` | ✅ Match |
| Data organization | vcf1-vcf22 data frames | HashMap by chromosome | ✅ Compatible |
| Variant fields | bp37, rsid, REF, ALT, AF, MAF, R2, TYPED | position, rsid, ref_allele, alt_allele, allele_freq, minor_allele_freq, imputation_quality, is_typed | ✅ Match |
| Output formats | RData only | JSON, SQLite, Parquet, VCF | ✅ Extended |

---

## Rust Implementation Verification

### Output Module (`output.rs`)

**✅ Multi-Sample Structures Created:**
- `MultiSampleVariantOutput` - 51 samples per variant
- `SampleDataOutput` - Individual sample data
- `MultiSampleGeneticOutput` - Top-level structure

**✅ Format-Specific Generators Implemented:**
1. **JSON** - Nested structure with samples array
2. **SQLite** - 51 rows per variant (with sample_id column)
3. **Parquet** - Columnar format with sample_id
4. **VCF** - Standard multi-sample VCF with 51 sample columns

**✅ Sample ID Format:**
- Sample IDs: `"samp1"` through `"samp51"`
- Matches R script naming convention

**✅ Genotype Format:**
- Phased genotypes: `"0|0"`, `"0|1"`, `"1|0"`, `"1|1"`
- Matches R script format

---

## Testing Recommendations

### 1. Compare Variant Counts
```bash
# R script variant counts (by chromosome)
Rscript -e "load('GenomicData4152.RData'); sapply(1:22, function(i) nrow(get(paste0('vcf', i))))"

# Rust output variant counts
sqlite3 output.db "SELECT chromosome, COUNT(*) FROM variants GROUP BY chromosome ORDER BY chromosome"
```

### 2. Validate Sample Structure
```bash
# Check VCF header has 51 samples
head -50 output.vcf | grep "^#CHROM"

# Verify sample IDs match samp1-samp51
# Expected: #CHROM POS ID REF ALT QUAL FILTER INFO FORMAT samp1 samp2 ... samp51
```

### 3. Spot-Check Genotypes
```bash
# Compare specific variants between R and Rust output
# Example: chr1:752721 (rs3131972) should have same genotypes across all 51 samples
```

---

## Next Steps

1. ✅ **Output module refactor complete** - All 4 formats support 51 samples
2. ✅ **Compilation successful** - Worker and API gateway built without errors
3. ⏳ **Integration testing** - Wire up worker to use multi-sample output
4. ⏳ **Validation** - Compare Rust output against R script GenomicData4152.RData
5. ⏳ **Performance testing** - Measure processing time for 5.9M variants × 51 samples

---

## Notes

- R script outputs RData format only (R-specific)
- Our Rust implementation provides 4 portable formats (JSON, SQLite, Parquet, VCF)
- VCF format is standard for bioinformatics tools
- SQLite and Parquet enable efficient querying in Python/R
- JSON provides web-friendly format for browser visualization

**Conclusion**: Our Rust multi-sample implementation matches the R script output structure and extends it with multiple export formats for broader compatibility.
