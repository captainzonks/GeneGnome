# Genetics Platform Architecture

<!--
==============================================================================
platform_architecture.md - System design for genetic analysis platform
==============================================================================
Description: Architecture for statistical analysis, visualization, and processing
Author: Matt Barham
Created: 2025-11-03
Modified: 2025-01-03
Version: 1.1.1
Repository: https://github.com/captainzonks/GeneGnome
==============================================================================
Document Type: Architecture Design
Audience: Developer
Status: Planning
==============================================================================
-->

## Vision

**Ultimate Goal:** Easy statistical analysis of genetic data with visualization features and flexible processing capabilities.

**Key Capabilities:**
1. **Statistical Analysis** - PGS calculations, GWAS queries, trait correlations, ancestry analysis
2. **Data Visualization** - Manhattan plots, PCA plots, chromosome ideograms, trait distributions
3. **Interactive Processing** - Query SNPs, filter by regions, compare samples, export subsets

**Design Philosophy:**
- **Data-driven:** PostgreSQL as primary data store, not just audit logs
- **API-first:** RESTful endpoints for all operations, enables future web UI
- **Extensible:** Plugin architecture for new analysis types
- **Performance:** Indexed queries, cached results, parallel processing

---

## System Architecture

### High-Level Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│ USER LAYER                                                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Web UI (Future)           │  CLI Tools           │  API Clients   │
│  ├─ Data upload            │  ├─ Bulk import     │  ├─ Python     │
│  ├─ Interactive plots      │  ├─ Analysis jobs   │  ├─ R          │
│  └─ Query builder          │  └─ Admin tasks     │  └─ Custom     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ API GATEWAY (Port 8099)                                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Authentication & Authorization                                    │
│  ├─ Authentik 2FA integration                                      │
│  ├─ JWT token validation                                           │
│  └─ User session management                                        │
│                                                                     │
│  RESTful Endpoints                                                 │
│  ├─ /api/v1/jobs          - Job management                         │
│  ├─ /api/v1/snps          - SNP queries                            │
│  ├─ /api/v1/analysis      - Statistical analysis                   │
│  ├─ /api/v1/visualization - Plot data generation                   │
│  └─ /api/v1/export        - Data export                            │
│                                                                     │
│  WebSocket Support (Future)                                        │
│  └─ Real-time job progress updates                                 │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ APPLICATION LAYER                                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌────────────────────┐  ┌────────────────────┐  ┌──────────────┐ │
│  │ Data Processor     │  │ Analysis Engine    │  │ Query Engine │ │
│  │ (genetics-processor)│  │                    │  │              │ │
│  ├────────────────────┤  ├────────────────────┤  ├──────────────┤ │
│  │ • VCF parsing      │  │ • PGS calculation  │  │ • SNP lookup │ │
│  │ • 23andMe merge    │  │ • GWAS queries     │  │ • Region     │ │
│  │ • Reference panel  │  │ • Trait analysis   │  │   filtering  │ │
│  │ • Data validation  │  │ • Ancestry (PCA)   │  │ • Multi-user │ │
│  │ • Secure deletion  │  │ • Heritability     │  │   isolation  │ │
│  └────────────────────┘  └────────────────────┘  └──────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ DATA LAYER                                                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ PostgreSQL 17.6 (genetics database)                         │   │
│  ├─────────────────────────────────────────────────────────────┤   │
│  │ • genetics_jobs        - Job metadata                       │   │
│  │ • genetics_files       - File tracking                      │   │
│  │ • genetics_audit       - Audit logs                         │   │
│  │ • genetics_snps        - SNP data (NEW)                     │   │
│  │ • genetics_samples     - Sample metadata (NEW)              │   │
│  │ • genetics_genotypes   - User genotypes (NEW)               │   │
│  │ • genetics_pgs_scores  - Polygenic scores (NEW)             │   │
│  │ • genetics_analysis    - Cached analysis results (NEW)      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Redis 8.2.2 (cache + queue)                                 │   │
│  ├─────────────────────────────────────────────────────────────┤   │
│  │ • Job queue (processing tasks)                              │   │
│  │ • Rate limiting                                             │   │
│  │ • Query result cache (TTL: 1 hour)                          │   │
│  │ • Session management                                        │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ Encrypted Volume (/mnt/genetics-encrypted)                  │   │
│  ├─────────────────────────────────────────────────────────────┤   │
│  │ • /uploads/     - Temporary uploads (< 5 min)               │   │
│  │ • /processing/  - Active processing (5-30 min)              │   │
│  │ • /results/     - Download-ready files (0-24 hours)         │   │
│  │ • /reference/   - Reference panel (permanent)               │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Database Schema Design

### Core Tables (Existing)

Already implemented in `database/init.sql` v1.0.1:

- **genetics_jobs** - Job metadata and status
- **genetics_files** - File tracking and lifecycle
- **genetics_audit** - Comprehensive audit logging

### New Tables for Analysis Platform

#### 1. genetics_samples

**Purpose:** Store sample metadata and population information

```sql
CREATE TABLE genetics_samples (
    sample_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,  -- App-level user ID
    job_id UUID NOT NULL REFERENCES genetics_jobs(job_id) ON DELETE CASCADE,

    -- Sample identification
    sample_name TEXT NOT NULL,  -- "USER" for uploaded data
    sample_type TEXT NOT NULL CHECK (sample_type IN ('user', 'reference')),

    -- Data source
    source_23andme_version TEXT,  -- e.g., "v5"
    source_build TEXT,  -- Genome build: "37" (hg19), "38" (hg38)
    imputation_server TEXT DEFAULT 'Michigan Imputation Server 2',

    -- Quality metrics
    snp_count_genotyped INTEGER,  -- Direct 23andMe SNPs
    snp_count_imputed INTEGER,    -- Michigan server SNPs
    snp_count_total INTEGER,
    mean_imputation_quality NUMERIC(4,3),  -- Average DR2

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP WITH TIME ZONE,  -- Soft delete

    UNIQUE (user_id, sample_name)
);

CREATE INDEX idx_samples_user_id ON genetics_samples(user_id);
CREATE INDEX idx_samples_job_id ON genetics_samples(job_id);
CREATE INDEX idx_samples_deleted ON genetics_samples(deleted_at) WHERE deleted_at IS NULL;

-- Row-level security
ALTER TABLE genetics_samples ENABLE ROW LEVEL SECURITY;

CREATE POLICY samples_isolation ON genetics_samples
    FOR ALL
    TO genetics_api
    USING (user_id = current_setting('app.current_user_id', TRUE)::TEXT)
    WITH CHECK (user_id = current_setting('app.current_user_id', TRUE)::TEXT);
```

---

#### 2. genetics_snps

**Purpose:** Reference SNP catalog (shared across all users)

```sql
CREATE TABLE genetics_snps (
    snp_id BIGSERIAL PRIMARY KEY,

    -- SNP identification
    rsid TEXT NOT NULL,  -- "rs12345" or "chr1:10177:A:G" for novel
    chromosome SMALLINT NOT NULL CHECK (chromosome BETWEEN 1 AND 22),
    position BIGINT NOT NULL,

    -- Alleles
    ref_allele TEXT NOT NULL CHECK (LENGTH(ref_allele) <= 100),
    alt_allele TEXT NOT NULL CHECK (LENGTH(alt_allele) <= 100),

    -- Annotations (future)
    gene_symbol TEXT,  -- Nearest gene
    consequence TEXT,  -- "missense", "synonymous", etc.

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,

    UNIQUE (rsid, chromosome, position, ref_allele, alt_allele)
);

CREATE INDEX idx_snps_rsid ON genetics_snps(rsid);
CREATE INDEX idx_snps_position ON genetics_snps(chromosome, position);
CREATE INDEX idx_snps_gene ON genetics_snps(gene_symbol) WHERE gene_symbol IS NOT NULL;

-- No RLS (read-only reference data)
```

**Size Estimate:**
- ~50M SNPs across 22 chromosomes
- ~100 bytes per row
- Total: ~5 GB table size
- With indexes: ~10 GB

**Optimization:** Partition by chromosome for faster queries

```sql
-- Partitioning (for future optimization)
ALTER TABLE genetics_snps PARTITION BY LIST (chromosome);

CREATE TABLE genetics_snps_chr1 PARTITION OF genetics_snps FOR VALUES IN (1);
CREATE TABLE genetics_snps_chr2 PARTITION OF genetics_snps FOR VALUES IN (2);
-- ... (repeat for chr3-22)
```

---

#### 3. genetics_genotypes

**Purpose:** Store user genotype data (dosages)

```sql
CREATE TABLE genetics_genotypes (
    genotype_id BIGSERIAL PRIMARY KEY,

    sample_id UUID NOT NULL REFERENCES genetics_samples(sample_id) ON DELETE CASCADE,
    snp_id BIGINT NOT NULL REFERENCES genetics_snps(snp_id) ON DELETE RESTRICT,

    -- Genotype data
    dosage NUMERIC(4,2) NOT NULL CHECK (dosage >= 0 AND dosage <= 2),
    imputation_quality NUMERIC(4,3),  -- DR2 score (NULL if genotyped)
    is_genotyped BOOLEAN NOT NULL DEFAULT FALSE,  -- TRUE if direct 23andMe

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,

    UNIQUE (sample_id, snp_id)
);

CREATE INDEX idx_genotypes_sample ON genetics_genotypes(sample_id);
CREATE INDEX idx_genotypes_snp ON genetics_genotypes(snp_id);
CREATE INDEX idx_genotypes_quality ON genetics_genotypes(imputation_quality) WHERE imputation_quality IS NOT NULL;

-- Row-level security (inherit from sample)
ALTER TABLE genetics_genotypes ENABLE ROW LEVEL SECURITY;

CREATE POLICY genotypes_isolation ON genetics_genotypes
    FOR SELECT
    TO genetics_api
    USING (
        sample_id IN (
            SELECT sample_id FROM genetics_samples
            WHERE user_id = current_setting('app.current_user_id', TRUE)::TEXT
        )
    );
```

**Storage Estimate:**
- ~40M SNPs per user
- ~20 bytes per row (with indexes)
- Per user: ~800 MB
- 100 users: ~80 GB

**Optimization:**
- Partition by sample_id (when multi-user)
- Compress with TOAST (PostgreSQL automatic)
- Consider TimescaleDB for better compression (hypertable)

**Alternative Design (Columnar Storage):**
For very large datasets, consider moving genotypes to Parquet files with metadata in PostgreSQL.

---

#### 4. genetics_pgs_scores

**Purpose:** Polygenic score weights and user results

```sql
-- Trait definitions
CREATE TABLE genetics_pgs_traits (
    trait_id SERIAL PRIMARY KEY,
    trait_name TEXT NOT NULL UNIQUE,  -- "height", "bmi", "t2d_risk"
    trait_description TEXT,
    trait_category TEXT,  -- "anthropometric", "disease_risk", "behavioral"
    pgs_catalog_id TEXT,  -- Reference to PGS Catalog (e.g., "PGS000001")
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- SNP weights for each trait
CREATE TABLE genetics_pgs_weights (
    weight_id BIGSERIAL PRIMARY KEY,
    trait_id INTEGER NOT NULL REFERENCES genetics_pgs_traits(trait_id),
    snp_id BIGINT NOT NULL REFERENCES genetics_snps(snp_id),

    effect_allele TEXT NOT NULL,  -- A1 (allele with positive weight)
    weight NUMERIC(12,6) NOT NULL,  -- Beta coefficient from GWAS

    UNIQUE (trait_id, snp_id)
);

CREATE INDEX idx_pgs_weights_trait ON genetics_pgs_weights(trait_id);
CREATE INDEX idx_pgs_weights_snp ON genetics_pgs_weights(snp_id);

-- User PGS results (cached)
CREATE TABLE genetics_pgs_results (
    result_id BIGSERIAL PRIMARY KEY,
    sample_id UUID NOT NULL REFERENCES genetics_samples(sample_id) ON DELETE CASCADE,
    trait_id INTEGER NOT NULL REFERENCES genetics_pgs_traits(trait_id),

    -- Scores
    raw_score NUMERIC(12,6) NOT NULL,  -- Σ (dosage × weight)
    normalized_score NUMERIC(8,4),     -- Z-score vs reference panel
    percentile NUMERIC(5,2),           -- Percentile rank (0-100)

    -- Metadata
    snp_count INTEGER NOT NULL,  -- Number of SNPs used
    coverage NUMERIC(5,2) NOT NULL,  -- % of trait SNPs present

    computed_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,

    UNIQUE (sample_id, trait_id)
);

CREATE INDEX idx_pgs_results_sample ON genetics_pgs_results(sample_id);
CREATE INDEX idx_pgs_results_trait ON genetics_pgs_results(trait_id);

-- Row-level security
ALTER TABLE genetics_pgs_results ENABLE ROW LEVEL SECURITY;

CREATE POLICY pgs_results_isolation ON genetics_pgs_results
    FOR SELECT
    TO genetics_api
    USING (
        sample_id IN (
            SELECT sample_id FROM genetics_samples
            WHERE user_id = current_setting('app.current_user_id', TRUE)::TEXT
        )
    );
```

---

#### 5. genetics_analysis_cache

**Purpose:** Cache expensive analysis results

```sql
CREATE TABLE genetics_analysis_cache (
    cache_id BIGSERIAL PRIMARY KEY,

    -- Cache key components
    user_id TEXT NOT NULL,
    analysis_type TEXT NOT NULL,  -- "pca", "ancestry", "gwas_query"
    parameters JSONB NOT NULL,    -- Analysis parameters (for cache key)
    parameters_hash TEXT NOT NULL GENERATED ALWAYS AS (md5(parameters::TEXT)) STORED,

    -- Results
    result_data JSONB NOT NULL,

    -- Metadata
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    accessed_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT (CURRENT_TIMESTAMP + INTERVAL '1 hour'),

    UNIQUE (user_id, analysis_type, parameters_hash)
);

CREATE INDEX idx_analysis_cache_lookup ON genetics_analysis_cache(user_id, analysis_type, parameters_hash);
CREATE INDEX idx_analysis_cache_expiry ON genetics_analysis_cache(expires_at);

-- Automatic cleanup of expired cache
CREATE OR REPLACE FUNCTION cleanup_expired_cache()
RETURNS TRIGGER AS $$
BEGIN
    DELETE FROM genetics_analysis_cache WHERE expires_at < CURRENT_TIMESTAMP;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_cleanup_cache
    AFTER INSERT ON genetics_analysis_cache
    EXECUTE FUNCTION cleanup_expired_cache();

-- Row-level security
ALTER TABLE genetics_analysis_cache ENABLE ROW LEVEL SECURITY;

CREATE POLICY analysis_cache_isolation ON genetics_analysis_cache
    FOR ALL
    TO genetics_api
    USING (user_id = current_setting('app.current_user_id', TRUE)::TEXT)
    WITH CHECK (user_id = current_setting('app.current_user_id', TRUE)::TEXT);
```

---

## API Endpoint Design

### RESTful API Structure

**Base URL:** `https://your-domain.com/api/v1/`

**Authentication:** Bearer token (JWT from Authentik)

```
Authorization: Bearer <jwt_token>
```

---

### 1. Job Management Endpoints

#### POST /api/v1/jobs

**Description:** Create new processing job

**Request:**
```json
{
  "vcf_files": [
    {"chromosome": 1, "file_id": "uuid"},
    {"chromosome": 2, "file_id": "uuid"},
    ...
  ],
  "genotype_file_id": "uuid",  // 23andMe file
  "scores_file_id": "uuid",    // Polygenic scores (optional)
  "options": {
    "min_imputation_quality": 0.3,  // Filter DR2 < 0.3
    "store_genotypes": true,        // Save to database for analysis
    "compute_pgs": true             // Calculate polygenic scores
  }
}
```

**Response:**
```json
{
  "job_id": "uuid",
  "status": "queued",
  "created_at": "2025-11-03T10:30:00Z",
  "estimated_duration": "15 minutes"
}
```

#### GET /api/v1/jobs/{job_id}

**Description:** Get job status

**Response:**
```json
{
  "job_id": "uuid",
  "status": "processing",  // queued, processing, completed, failed
  "progress": {
    "current_chromosome": 5,
    "total_chromosomes": 22,
    "percent_complete": 23
  },
  "started_at": "2025-11-03T10:32:00Z",
  "estimated_completion": "2025-11-03T10:47:00Z"
}
```

#### GET /api/v1/jobs/{job_id}/results

**Description:** Get job results (when completed)

**Response:**
```json
{
  "job_id": "uuid",
  "status": "completed",
  "sample_id": "uuid",
  "metrics": {
    "snp_count_total": 40123456,
    "snp_count_genotyped": 589321,
    "snp_count_imputed": 39534135,
    "mean_imputation_quality": 0.87
  },
  "files": [
    {
      "name": "results.tar.gz",
      "size": 456789012,
      "download_url": "/api/v1/files/{file_id}/download",
      "expires_at": "2025-11-04T10:30:00Z"
    }
  ],
  "pgs_scores": [
    {"trait": "height", "percentile": 67.3},
    {"trait": "bmi", "percentile": 42.1}
  ]
}
```

---

### 2. SNP Query Endpoints

#### GET /api/v1/snps/{rsid}

**Description:** Get SNP information

**Response:**
```json
{
  "rsid": "rs12345",
  "chromosome": 1,
  "position": 752566,
  "ref_allele": "A",
  "alt_allele": "G",
  "gene": "GENE1",
  "user_genotype": {
    "dosage": 1.02,
    "genotype": "AG",
    "is_imputed": false,
    "quality": null
  }
}
```

#### POST /api/v1/snps/query

**Description:** Query SNPs by region or list

**Request:**
```json
{
  "query_type": "region",  // or "list"
  "sample_id": "uuid",
  "region": {
    "chromosome": 1,
    "start": 1000000,
    "end": 2000000
  },
  "filters": {
    "min_quality": 0.8,
    "only_genotyped": false
  }
}
```

**Response:**
```json
{
  "snps": [
    {
      "rsid": "rs12345",
      "position": 1001234,
      "ref": "A",
      "alt": "G",
      "dosage": 1.0
    },
    ...
  ],
  "total_count": 1234,
  "page": 1,
  "page_size": 100
}
```

---

### 3. Analysis Endpoints

#### POST /api/v1/analysis/pgs

**Description:** Calculate polygenic score for trait

**Request:**
```json
{
  "sample_id": "uuid",
  "trait": "height",  // or trait_id
  "options": {
    "use_cache": true,
    "min_snp_coverage": 0.8  // Require 80% of trait SNPs present
  }
}
```

**Response:**
```json
{
  "trait": "height",
  "raw_score": 123.456,
  "normalized_score": 0.67,  // Z-score
  "percentile": 75.2,
  "snp_count": 1234,
  "coverage": 0.92,
  "computed_at": "2025-11-03T10:30:00Z",
  "cached": false
}
```

#### POST /api/v1/analysis/ancestry (Future)

**Description:** Perform PCA-based ancestry analysis

**Request:**
```json
{
  "sample_id": "uuid",
  "reference_populations": ["EUR", "AFR", "EAS", "SAS", "AMR"]
}
```

**Response:**
```json
{
  "ancestry_estimates": {
    "EUR": 0.82,
    "EAS": 0.15,
    "AFR": 0.03
  },
  "pca_coordinates": {
    "PC1": -0.0123,
    "PC2": 0.0456,
    "PC3": -0.0089
  }
}
```

---

### 4. Visualization Data Endpoints

#### GET /api/v1/visualization/chromosome/{chromosome}

**Description:** Get data for chromosome ideogram visualization

**Response:**
```json
{
  "chromosome": 1,
  "length": 249250621,
  "bands": [
    {"name": "p36.33", "start": 0, "end": 2300000, "stain": "gneg"},
    ...
  ],
  "user_snps": {
    "genotyped": [
      {"position": 752566, "rsid": "rs12345"},
      ...
    ],
    "imputed_high_quality": 9234567,
    "imputed_low_quality": 123456
  }
}
```

#### POST /api/v1/visualization/manhattan

**Description:** Generate Manhattan plot data for trait

**Request:**
```json
{
  "sample_id": "uuid",
  "trait": "height",
  "significance_threshold": 5e-8
}
```

**Response:**
```json
{
  "trait": "height",
  "data_points": [
    {
      "chromosome": 1,
      "position": 752566,
      "rsid": "rs12345",
      "p_value": 1.2e-9,
      "beta": 0.023,
      "user_dosage": 1.0
    },
    ...
  ],
  "significant_loci": 45
}
```

---

### 5. Export Endpoints

#### POST /api/v1/export

**Description:** Export data in various formats

**Request:**
```json
{
  "sample_id": "uuid",
  "format": "vcf",  // vcf, csv, parquet, plink
  "filters": {
    "chromosomes": [1, 2, 3],
    "min_quality": 0.8,
    "only_genotyped": false
  }
}
```

**Response:**
```json
{
  "export_id": "uuid",
  "status": "processing",
  "estimated_completion": "2025-11-03T10:35:00Z"
}
```

#### GET /api/v1/export/{export_id}

**Description:** Check export status and download

**Response:**
```json
{
  "export_id": "uuid",
  "status": "completed",
  "file_size": 123456789,
  "download_url": "/api/v1/files/{file_id}/download",
  "expires_at": "2025-11-04T10:30:00Z"
}
```

---

## Implementation Phases

### Phase 1: Data Processing Foundation (Current Focus)

**Goal:** Port mergeData.R to Rust, store results in database

**Components:**
1. ✅ VCF parser (noodles-vcf)
2. ✅ 23andMe parser
3. ✅ Genotype converter (REF/ALT → dosage)
4. ✅ SNP merger (genotyped + imputed)
5. ✅ PostgreSQL schema updates (new tables)
6. ✅ Data ingestion pipeline (VCF → database)

**Deliverable:** User data processed and stored in `genetics_genotypes` table

**Timeline:** 2-3 weeks

---

### Phase 2: Basic API + PGS Analysis

**Goal:** Enable programmatic access and polygenic score calculations

**Components:**
1. API gateway skeleton (Axum/Actix-web framework)
2. Authentication middleware (JWT validation)
3. Job management endpoints
4. SNP query endpoints (basic)
5. PGS calculation engine
6. PGS traits + weights seeding

**Deliverable:** RESTful API for job submission and PGS queries

**Timeline:** 2-3 weeks

---

### Phase 3: Visualization Support

**Goal:** Provide data for plotting in web UI or external tools

**Components:**
1. Manhattan plot data generation
2. Chromosome ideogram data
3. PCA calculation (ancestry)
4. Export endpoints (VCF, CSV, Parquet)
5. Cached query results (Redis)

**Deliverable:** API endpoints returning plot-ready data

**Timeline:** 2-3 weeks

---

### Phase 4: Web UI (Future)

**Goal:** Interactive web interface for non-technical users

**Components:**
1. React/Svelte frontend
2. File upload interface
3. Job monitoring dashboard
4. Interactive plots (D3.js / Plotly)
5. SNP browser
6. Trait explorer

**Deliverable:** Full-featured web application

**Timeline:** 6-8 weeks

---

## Technology Stack

### Backend

| Component | Technology | Version | Notes |
|-----------|-----------|---------|-------|
| Language | Rust | 1.75+ | Core application |
| Web Framework | Axum | 0.8+ | RESTful API (or Actix-web) |
| Async Runtime | Tokio | 1.48.0 | Already in use |
| Database Driver | sqlx | 0.8.6 | Already in use |
| VCF Parsing | noodles-vcf | 0.81.0 | Research complete |
| Serialization | serde_json | 1.0+ | Already in use |
| Authentication | jsonwebtoken | 9.3+ | JWT validation |

### Database

| Component | Technology | Version | Notes |
|-----------|-----------|---------|-------|
| Primary DB | PostgreSQL | 17.6 | Already deployed |
| Cache/Queue | Redis | 8.2.2 | Already deployed |
| Migration Tool | sqlx-cli | 0.8+ | Database migrations |

### Frontend (Future)

| Component | Technology | Version | Notes |
|-----------|-----------|---------|-------|
| Framework | Svelte | 5.0+ | Lightweight, reactive |
| Plotting | D3.js | 7.9+ | Custom visualizations |
| HTTP Client | fetch API | Native | RESTful API calls |

---

## Security Considerations

### Data Access Control

1. **Row-Level Security (RLS)** - All user data tables enforce user isolation
2. **JWT Validation** - Every API request validates Authentik-issued token
3. **Rate Limiting** - Redis-based throttling (100 requests/min per user)
4. **Audit Logging** - All data access logged to `genetics_audit`

### Data Retention

1. **Raw Files** - Auto-deleted after 24 hours (existing policy)
2. **Database Records** - Soft delete with 30-day grace period
3. **Analysis Cache** - Redis TTL of 1 hour
4. **Export Files** - Auto-deleted after 24 hours

### Encryption

1. **At Rest** - LUKS encrypted volume (existing)
2. **In Transit** - TLS 1.3 via Traefik (existing)
3. **Database** - PostgreSQL transparent data encryption (future)

---

## Performance Optimization Strategy

### Database Indexing

1. **Primary Keys** - Clustered indexes on all tables
2. **Foreign Keys** - Indexed for join performance
3. **Query Patterns** - Cover indexes for common queries
   - `(user_id, chromosome, position)` for region queries
   - `(sample_id, trait_id)` for PGS lookups

### Query Caching

1. **Redis Cache** - Expensive queries cached for 1 hour
2. **Materialized Views** - Pre-computed aggregations
3. **PostgreSQL Prepared Statements** - Query plan reuse

### Parallel Processing

1. **Chromosome-Level** - Process 22 chromosomes in parallel
2. **Rayon** - Work-stealing for CPU-bound tasks
3. **Tokio** - Async I/O for network/disk operations

### Data Compression

1. **PostgreSQL TOAST** - Automatic compression for large rows
2. **Parquet** - Columnar format for export (10× compression)
3. **Redis** - Compressed strings for cached results

---

## Monitoring & Observability

### Metrics to Track

1. **Job Processing**
   - Average job duration per chromosome
   - Success/failure rates
   - Queue depth

2. **API Performance**
   - Request latency (p50, p95, p99)
   - Requests per second
   - Error rates by endpoint

3. **Database Performance**
   - Query execution time
   - Cache hit ratio
   - Connection pool usage

4. **Resource Usage**
   - CPU utilization
   - Memory usage
   - Disk I/O

### Logging Strategy

1. **Structured Logging** - JSON format for parsing
2. **Log Levels** - INFO for operations, DEBUG for troubleshooting
3. **Correlation IDs** - Track requests across services
4. **Audit Trail** - All data access logged with timestamps

---

## Open Questions

1. **Reference Panel**
   - Store in PostgreSQL or keep as file?
   - Pre-populate `genetics_snps` table at setup?
   - Update frequency for new SNPs?

2. **Genotype Storage**
   - Store all 40M SNPs per user in database?
   - Or hybrid: common SNPs in DB, rare in files?
   - Compression strategy (columnar vs row-based)?

3. **Analysis Extensibility**
   - Plugin system for new analysis types?
   - User-defined polygenic scores?
   - Integration with external tools (PLINK, GCTA)?

4. **Visualization Hosting**
   - Server-side plot generation (PNG/SVG)?
   - Or client-side rendering (send data only)?
   - Interactive plots (WebGL / Canvas)?

5. **Multi-User Scaling**
   - Current design: Single server deployment
   - Future: Distributed processing?
   - Database sharding by user_id?

---

## Next Steps

### Immediate (Phase 1a) - Update Database Schema

1. Create migration for new tables
2. Seed `genetics_snps` with reference panel
3. Test RLS policies
4. Update audit logging for new tables

### Short-Term (Phase 1b) - Implement Data Pipeline

1. Prototype VCF parser with noodles-vcf
2. Implement SNP ingestion (VCF → database)
3. Implement genotype storage (dosages → database)
4. Test with chr22 (smallest chromosome)

### Medium-Term (Phase 2) - Build API

1. Choose web framework (Axum recommended)
2. Implement authentication middleware
3. Build job management endpoints
4. Implement basic SNP query endpoints

---

## References

- **mergeData.R Analysis:** `docs/mergeData_pipeline_analysis.md`
- **noodles-vcf Research:** `docs/noodles_vcf_research.md`
- **Security Architecture:** `docs/genetic_data_security_architecture.md`
- **Database Schema:** `database/init.sql`

---

**End of Document**

*Architecture designed 2025-11-03. Subject to refinement during implementation.*
