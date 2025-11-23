# Parquet Usage Guide

## What is Parquet?

Apache Parquet is a columnar storage format optimized for analytical queries on large datasets. For genomic data with millions of variants across multiple samples, Parquet is ideal because it:

- **Compresses efficiently**: 50-100× smaller than SQLite for the same data
- **Queries fast**: Read only the columns you need (e.g., just dosage values)
- **Universally supported**: Works with Python, R, SQL, Spark, and more

## Reading Parquet Files

### Python (pandas)

```python
import pandas as pd

# Read entire file
df = pd.read_parquet('GenomicData_jobid_51samples.parquet')

# Read specific columns only (faster!)
df = pd.read_parquet('GenomicData_jobid_51samples.parquet',
                      columns=['rsid', 'chromosome', 'position', 'dosage'])

# Filter by chromosome
df_chr1 = df[df['chromosome'] == 1]

# Get all variants for a specific sample
sample_data = df[df['sample_id'] == 'HG00096']

# Calculate statistics
mean_dosage = df.groupby('rsid')['dosage'].mean()
```

### Python (PyArrow - faster for large files)

```python
import pyarrow.parquet as pq

# Read file
table = pq.read_table('GenomicData_jobid_51samples.parquet')

# Convert to pandas if needed
df = table.to_pandas()

# Filter before converting (more efficient)
table_filtered = pq.read_table('GenomicData_jobid_51samples.parquet',
                                filters=[('chromosome', '=', 7)])
df_chr7 = table_filtered.to_pandas()
```

### R (arrow package)

```r
library(arrow)

# Read entire file
df <- read_parquet('GenomicData_jobid_51samples.parquet')

# Read with dplyr integration
library(dplyr)
df <- read_parquet('GenomicData_jobid_51samples.parquet') %>%
  filter(chromosome == 1) %>%
  select(rsid, position, dosage, sample_id)

# Query without loading entire file into memory
dataset <- open_dataset('GenomicData_jobid_51samples.parquet')
result <- dataset %>%
  filter(chromosome == 7, imputation_quality > 0.9) %>%
  select(rsid, position, dosage) %>%
  collect()
```

### R (data.table - fast)

```r
library(arrow)
library(data.table)

# Read as data.table
dt <- as.data.table(read_parquet('GenomicData_jobid_51samples.parquet'))

# Fast queries
dt[chromosome == 1 & imputation_quality > 0.8, .(rsid, dosage, sample_id)]
```

### DuckDB (SQL queries on Parquet)

```python
import duckdb

# Query Parquet with SQL (no loading into memory!)
result = duckdb.query("""
    SELECT rsid, position, AVG(dosage) as avg_dosage
    FROM 'GenomicData_jobid_51samples.parquet'
    WHERE chromosome = 7
    GROUP BY rsid, position
    ORDER BY position
""").to_df()
```

```sql
-- From DuckDB CLI
duckdb

D SELECT COUNT(*) FROM 'GenomicData_jobid_51samples.parquet';
D SELECT * FROM 'GenomicData_jobid_51samples.parquet' WHERE chromosome = 1 LIMIT 10;
```

### Polars (Rust-based, extremely fast)

```python
import polars as pl

# Read file
df = pl.read_parquet('GenomicData_jobid_51samples.parquet')

# Lazy evaluation (queries optimized before execution)
result = (
    pl.scan_parquet('GenomicData_jobid_51samples.parquet')
    .filter(pl.col('chromosome') == 7)
    .filter(pl.col('imputation_quality') > 0.9)
    .select(['rsid', 'position', 'dosage', 'sample_id'])
    .collect()
)
```

## Common Analysis Examples

### Example 1: Get all variants for your genome sample

```python
import pandas as pd

df = pd.read_parquet('GenomicData_jobid_51samples.parquet')

# Your 23andMe data is in the sample with your genome ID
your_data = df[df['sample_id'] == 'genome_Matthew_Barham']

# See high-impact variants
high_dosage = your_data[your_data['dosage'] > 1.5]
print(high_dosage[['rsid', 'chromosome', 'position', 'dosage', 'genotype']])
```

### Example 2: Compare your genotype to reference panel

```python
import pandas as pd

df = pd.read_parquet('GenomicData_jobid_51samples.parquet')

# Get a specific variant (e.g., rs1234567)
variant = df[df['rsid'] == 'rs1234567']

# Your genotype
your_genotype = variant[variant['sample_id'] == 'genome_Matthew_Barham']['genotype'].values[0]

# Reference panel genotypes for comparison
ref_genotypes = variant[variant['source'] == 'reference']['genotype'].value_counts()

print(f"Your genotype: {your_genotype}")
print(f"Reference panel distribution:\n{ref_genotypes}")
```

### Example 3: Calculate allele frequency across samples

```python
import pandas as pd

df = pd.read_parquet('GenomicData_jobid_51samples.parquet')

# Calculate mean dosage (allele frequency proxy) per variant
allele_freq = df.groupby(['rsid', 'chromosome', 'position']).agg({
    'dosage': 'mean',
    'allele_freq': 'first',  # Reference allele freq
    'ref_allele': 'first',
    'alt_allele': 'first'
}).reset_index()

# Find rare variants (MAF < 0.01)
rare_variants = allele_freq[allele_freq['allele_freq'] < 0.01]
```

### Example 4: Extract chromosome subset for GWAS

```python
import pandas as pd

df = pd.read_parquet('GenomicData_jobid_51samples.parquet',
                      columns=['rsid', 'chromosome', 'position', 'dosage', 'sample_id'])

# Pivot to wide format (samples as columns)
wide = df.pivot(index=['rsid', 'chromosome', 'position'],
                columns='sample_id',
                values='dosage')

# Save for GWAS software
wide.to_csv('chr7_dosages_wide.csv')
```

## File Schema

Your Parquet file contains these columns:

| Column | Type | Description |
|--------|------|-------------|
| `rsid` | string | Variant ID (e.g., "rs1234567") |
| `chromosome` | int64 | Chromosome number (1-22) |
| `position` | int64 | Base pair position |
| `ref_allele` | string | Reference allele (A/C/G/T) |
| `alt_allele` | string | Alternate allele (A/C/G/T) |
| `allele_freq` | float64 | Allele frequency in reference panel |
| `minor_allele_freq` | float64 | Minor allele frequency |
| `is_typed` | bool | Whether variant was directly genotyped (vs imputed) |
| `sample_id` | string | Sample identifier |
| `genotype` | string | Genotype call (e.g., "0|1", "1|1") |
| `dosage` | float64 | Allelic dosage (0.0 - 2.0) |
| `source` | string | "user" or "reference" |
| `imputation_quality` | float64 | R² imputation quality score (0.0 - 1.0) |

## Performance Tips

1. **Read only needed columns**: Parquet is columnar, so you pay only for what you read
2. **Use PyArrow/Polars for large files**: Faster than pandas for multi-GB files
3. **Filter before loading**: Use Arrow/DuckDB to filter before converting to pandas
4. **Partition by chromosome**: If working with one chromosome, extract it once

## Installing Required Packages

### Python
```bash
pip install pandas pyarrow polars duckdb
```

### R
```r
install.packages(c("arrow", "dplyr", "data.table"))
```

## Example: Quick Start Script

```python
#!/usr/bin/env python3
"""Quick exploration of genetics Parquet file"""

import pandas as pd

# Read file
print("Reading Parquet file...")
df = pd.read_parquet('GenomicData_jobid_51samples.parquet')

# Basic stats
print(f"\nDataset shape: {df.shape}")
print(f"Chromosomes: {sorted(df['chromosome'].unique())}")
print(f"Total variants: {df['rsid'].nunique()}")
print(f"Total samples: {df['sample_id'].nunique()}")

# Sample preview
print("\nFirst 10 rows:")
print(df.head(10))

# Quality summary
print("\nImputation quality summary:")
print(df['imputation_quality'].describe())

# Your data
your_sample = 'genome_Matthew_Barham'  # Adjust to your sample ID
if your_sample in df['sample_id'].values:
    your_data = df[df['sample_id'] == your_sample]
    print(f"\nYour genome: {len(your_data)} variants")
    print(f"Typed variants: {your_data['is_typed'].sum()}")
    print(f"Imputed variants: {(~your_data['is_typed']).sum()}")
```

Save this as `explore_genetics.py` and run:
```bash
python explore_genetics.py
```
