# GeneGnome Example Data Files

This directory contains anonymized example data files for testing and demonstration purposes.

## ⚠️ Important Notice

**THESE ARE NOT REAL GENETIC DATA FILES**

All data in this directory is completely synthetic and randomly generated. These files:
- Do NOT represent any real person's genetic information
- Are purely for testing, documentation, and educational purposes
- Should NOT be used for any actual genetic analysis or medical decisions

## Example Files

### `example_23andme.txt`

A small example file in 23andMe raw data format.

- **Format**: Tab-separated values (TSV)
- **Columns**: rsid, chromosome, position, genotype
- **Variants**: 50 synthetic SNPs across chromosomes 1-10
- **Build**: GRCh37 (hg19)

**Usage:**
```bash
# Test VCF generator (browser-only, no upload)
# 1. Open http://your-domain.com/vcf-generator.html
# 2. Select this file
# 3. VCF generated in-browser

# Test full processing pipeline (upload + imputation merge)
curl -X POST http://your-domain.com/api/genetics/upload \
  -F "genome_file=@example_23andme.txt" \
  -F "vcf_file=@example_imputed.vcf" \
  -F "email=test@example.com"
```

### `example_imputed.vcf`

A small example VCF file mimicking Michigan Imputation Server output.

- **Format**: Variant Call Format (VCF) v4.2
- **Variants**: 25 synthetic imputed variants
- **Samples**: 1 (SAMPLE001)
- **Build**: GRCh37 (hg19)
- **Fields**: GT (Genotype), DS (Dosage), GP (Genotype Probabilities)
- **Info**: AF (Allele Frequency), MAF (Minor Allele Frequency), R2 (Imputation Quality)

**Imputation markers:**
- `TYPED` flag: Variants that were genotyped (from 23andMe)
- `IMPUTED` flag: Variants inferred by imputation algorithm
- `R2`: Imputation quality score (0-1, higher is better)

**Usage:**
```bash
# Merge with 23andMe file
curl -X POST http://your-domain.com/api/genetics/upload \
  -F "genome_file=@example_23andme.txt" \
  -F "vcf_file=@example_imputed.vcf" \
  -F "email=test@example.com" \
  -F "password=test123"
```

## Testing Workflow

### 1. Browser-Only VCF Generation (No Upload)

```bash
# Open VCF generator
open http://your-domain.com/vcf-generator.html

# Select example_23andme.txt
# File is processed entirely in your browser using WebAssembly
# Download generated VCF immediately
```

### 2. Full Processing Pipeline (Upload + Merge)

```bash
# Upload files for server-side processing
curl -X POST http://your-domain.com/api/genetics/upload \
  -F "genome_file=@example_23andme.txt" \
  -F "vcf_file=@example_imputed.vcf" \
  -F "email=your-email@example.com" \
  -F "password=secure-password"

# Response contains job_id
# {
#   "job_id": "123e4567-e89b-12d3-a456-426614174000",
#   "status": "pending",
#   "message": "Job queued for processing"
# }

# Check status
curl http://your-domain.com/api/genetics/status/123e4567-e89b-12d3-a456-426614174000

# Wait for email with download link, or poll status endpoint
# When complete, download results using token from email
curl -O "http://your-domain.com/download/123e4567-e89b-12d3-a456-426614174000?token=<token>&password=secure-password"
```

### 3. Local Testing (Docker Compose)

```bash
# Start services
docker-compose up -d

# Wait for services to be healthy
docker-compose ps

# Upload test data
curl -X POST http://localhost:8090/api/genetics/upload \
  -F "genome_file=@examples/example_23andme.txt" \
  -F "vcf_file=@examples/example_imputed.vcf" \
  -F "email=test@example.com"

# Monitor worker logs
docker-compose logs -f genetics-worker

# Check results (look for job_id in API response)
curl http://localhost:8090/api/genetics/status/<job_id>
```

## Expected Output

When processing completes successfully, you'll receive:

### Parquet Output
- **File**: `merged_results.parquet`
- **Format**: Apache Parquet (columnar storage)
- **Compression**: Snappy
- **Usage**: Load in pandas, polars, DuckDB, etc.

```python
import pandas as pd
df = pd.read_parquet('merged_results.parquet')
print(df.head())
```

### VCF Output
- **File**: `merged_results.vcf.gz`
- **Format**: VCF v4.2 (bgzip compressed)
- **Usage**: bcftools, PLINK, genetics analysis tools

```bash
# View VCF
zcat merged_results.vcf.gz | head

# Index for fast access
tabix -p vcf merged_results.vcf.gz

# Query specific region
tabix merged_results.vcf.gz 1:100000-200000
```

### SQLite Output
- **File**: `merged_results.db`
- **Format**: SQLite3 database
- **Tables**: variants (SNP data)
- **Usage**: SQLite, DBeaver, any SQL tool

```bash
# Query SQLite
sqlite3 merged_results.db "SELECT * FROM variants LIMIT 10;"
```

## Creating Your Own Test Data

### Generate Custom 23andMe File

```python
#!/usr/bin/env python3
import random

chroms = list(range(1, 23)) + ['X', 'Y', 'MT']
genotypes = ['AA', 'AG', 'GG', 'AC', 'CC', 'AT', 'TT', 'GC', 'CG', 'CT']

print("# rsid\tchromosome\tposition\tgenotype")
for i in range(100):
    rsid = f"rs{random.randint(10000, 99999999)}"
    chrom = random.choice(chroms[:22])  # Autosomes only
    pos = random.randint(100000, 100000000)
    geno = random.choice(genotypes)
    print(f"{rsid}\t{chrom}\t{pos}\t{geno}")
```

### Generate Custom VCF File

Use `bcftools` or Python's `pysam` library to create custom VCF files with specific variants.

## Troubleshooting

### File Format Errors

If you get format errors:

1. **Check file encoding**: Must be UTF-8 or ASCII
2. **Check line endings**: Unix (LF) preferred over Windows (CRLF)
3. **Check delimiters**: 23andMe uses TAB, VCF uses TAB
4. **Check headers**: VCF requires `##fileformat=VCFv4.2` header

### Upload Failures

If uploads fail:

1. **Check file size**: Default limit is 500MB (configurable in `.env`)
2. **Check permissions**: Ensure encrypted volume is mounted
3. **Check logs**: `docker-compose logs genetics-api-gateway`
4. **Check health**: `curl http://localhost:8090/health`

### Processing Failures

If processing fails:

1. **Check worker logs**: `docker-compose logs genetics-worker`
2. **Check database**: PostgreSQL must be running
3. **Check Redis**: Job queue must be available
4. **Check reference data**: reference_panel.db must exist in reference directory

## Reference Data

For actual (non-example) genetic processing, you'll need reference panel databases. GeneGnome uses two types of reference data containing 50 anonymous genome samples:

### 1. Genotyped Reference Panel (For VCF Generation)

**File**: `genotyped.anon.RData` (14 MB)
**Purpose**: Used by browser VCF generator to create multi-sample VCF files for Michigan Imputation Server

```bash
# Download genotyped reference panel
mkdir -p reference
cd reference
wget http://www.matthewckeller.com/public/genotyped.anon.RData
cd ..
```

**Note**: This is currently NOT integrated into the WASM VCF generator. Future enhancement planned.

### 2. Imputed Reference Panel (For Merge Processing)

**File**: `VCF.Files3.RData` (167 MB) → converts to `reference_panel.db` (4.7 GB)
**Purpose**: Used by Rust processor to merge imputation results with your 23andMe data

```bash
# Download imputed reference panel
mkdir -p reference
cd reference
wget http://www.matthewckeller.com/public/VCF.Files3.RData

# Convert to SQLite (requires R with DBI, RSQLite, jsonlite packages)
cd ..
Rscript scripts/convert_reference_to_db.R

# Verify database was created
ls -lh reference/reference_panel.db
# Expected: ~4.7 GB

# Query variant counts
sqlite3 reference/reference_panel.db \
  "SELECT COUNT(*) FROM reference_variants;"
# Expected: ~5,900,000 variants
```

### Alternative: Michigan Imputation Server

If you prefer not to self-host reference data, use Michigan Imputation Server:

- **URL**: https://imputationserver.sph.umich.edu/
- **Reference Panels**: HRC, 1000 Genomes, CAAPA, TOPMed
- **Requirements**: Account registration and data upload
- **Advantages**: More comprehensive imputation, higher quality
- **Disadvantages**: Requires uploading your genetic data

### Data Provenance

Both reference panels contain 50 anonymous samples originally from **openSNP.org** (now closed):
- Freely uploaded by users for research purposes
- Completely anonymous (no identifiable information)
- Currently hosted by Dr. Matthew C. Keller (University of Colorado Boulder)

**For complete documentation**, see: [../docs/REFERENCE_DATA.md](../docs/REFERENCE_DATA.md)

## Privacy & Security

When using GeneGnome with **real genetic data**:

- ✅ **DO** use the browser-only VCF generator (no upload)
- ✅ **DO** self-host on your own infrastructure
- ✅ **DO** enable LUKS encryption for storage
- ✅ **DO** use strong passwords for download links
- ✅ **DO** review security documentation before deployment
- ❌ **DON'T** use example data for real analysis
- ❌ **DON'T** upload real data to untrusted servers
- ❌ **DON'T** share download links publicly
- ❌ **DON'T** disable encryption or automatic deletion

## Additional Resources

- **GeneGnome Documentation**: [../README.md](../README.md)
- **API Reference**: [../docs/API.md](../docs/API.md) (if available)
- **Security Architecture**: [../docs/SECURITY.md](../docs/SECURITY.md) (if available)
- **Environment Configuration**: [../.env.example](../.env.example)

---

**License**: Apache-2.0 OR MIT (see [../LICENSE](../LICENSE))
**Author**: Matthew Barham
**Last Updated**: 2025-11-20
