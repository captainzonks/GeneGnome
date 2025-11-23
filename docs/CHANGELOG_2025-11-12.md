# Genetics Processing Pipeline - Changes 2025-11-12

## Summary
Implemented R2 quality filtering, fixed indel handling bug, improved websocket reconnection, and began work on 51-sample multi-sample analysis support to match R script workflow.

## Key Changes

### 1. R2 Quality Filtering Implementation
**Files Modified:**
- `app/src/processor.rs`
- `worker/src/job_processor.rs`
- `api-gateway/src/models.rs`
- `api-gateway/src/queue.rs`

**Changes:**
- Added `QualityThreshold` enum: `R08` (≥0.8), `R09` (≥0.9), `NoFilter`
- Processor now filters imputed variants based on R2 quality scores
- Quality filtering applied during merge process, not post-processing
- Metadata tracks filtered variant counts

**Results:**
- With R2≥0.9: 4,339,890 variants kept (from 11,710,235 unfiltered)
- 63% of low-quality variants removed
- Single-sample processing validated against R data structure

### 2. Indel Handling Fix
**Files Modified:**
- `app/src/genotype_converter.rs`

**Problem:**
- 23andMe genotypes are 2 characters (e.g., "AA")
- Converter was incorrectly matching indels (REF="A", ALT="AG")
- Both SNPs and indels at same position marked as "Genotyped"

**Fix:**
- Added validation: REF and ALT must be single characters
- Indels (length > 1) now return AllelesMismatch error
- Processor correctly uses imputed dosage for indels
- Updated tests to verify indel rejection

**Verification:**
- Position 93752551: A→G SNP correctly "Genotyped", A→AG insertion correctly filtered/imputed
- Genotyped count: 481,328 (consistent across filtering)

### 3. Websocket Reconnection Improvements
**Files Modified:**
- `api-gateway/src/handlers.rs`

**Problem:**
- Client lost progress updates after websocket disconnect
- No initial state sent when reconnecting
- Users saw "0% waiting for updates" after reconnection

**Fix:**
- Added database query on websocket connect
- Sends current job status immediately upon connection
- Reconnecting clients now receive current progress state

**Implementation:**
```rust
// Query current status and send immediately
match sqlx::query_as::<_, (String,)>("SELECT status FROM genetics_jobs WHERE id = $1")
    .bind(job_id)
    .fetch_optional(state.db_pool())
    .await
{
    Ok(Some((status,))) => {
        let initial_msg = serde_json::json!({
            "type": "status",
            "status": status,
            "message": format!("Current status: {}", status)
        });
        // Send to client immediately
    }
    // ...
}
```

### 4. Data Validation & Analysis
**Created Files:**
- `docs/analysis/data_comparison_summary.md` (in Downloads, to be relocated)

**Findings:**
- **Rust single-sample (filtered)**: 4.3M variants, 0.19% duplication
- **R 51-sample dataset**: 5.9M variants, 90.6% duplication (expected for multi-sample)
- R data uses 50-sample reference panel from VCF.Files3.RData
- Difference due to single vs multi-sample processing approaches

**Conclusion:**
- Current Rust processor works correctly for single-sample analysis
- Need to implement 51-sample support to match R workflow for academia/research

## Architecture Changes Planned

### 5. Multi-Sample Support (In Progress)
**Goal:** Match R script workflow with 50-sample reference panel + user data = 51 samples

**Phase 1: Reference Panel Conversion**
- Converting VCF.Files3.RData to SQLite database
- 5,900,127 variants across 50 anonymized samples
- Database includes: position, rsid, ref, alt, R2, typed flag, 50 sample genotypes

**Phase 2: Rust Data Model Changes**
```rust
// Current: Single sample
struct MergedVariant {
    rsid: String,
    dosage: f64,
    source: DataSource,
}

// New: Multi-sample
struct MultiSampleVariant {
    rsid: String,
    samples: Vec<SampleData>,  // 51 samples
}

struct SampleData {
    sample_id: String,  // "samp1"..."samp50", "samp51"
    genotype: String,
    dosage: f64,
    source: DataSource,
}
```

**Phase 3: Processor Logic**
- Load 50-sample reference per chromosome
- Merge user data as sample 51
- Match by: chromosome + position + REF + ALT (not just position)
- Handle variants in reference but not user data (vice versa)

**Phase 4: Output Format Updates**
- SQLite: Add `sample_id` column
- JSON: Array of samples per variant
- Parquet: Multi-sample columns
- VCF: FORMAT + 51 sample columns

## Testing Results

### Quality Filtering
- ✅ Filters applied correctly per chromosome
- ✅ Metadata reflects filtered counts
- ✅ R2≥0.9 reduces dataset by 63%
- ✅ Output file sizes reduced proportionally

### Indel Fix
- ✅ Position 93752551 test case verified
- ✅ SNPs correctly marked as Genotyped
- ✅ Indels correctly use imputed dosage
- ✅ All unit tests passing

### Websocket Reconnection
- ✅ Initial status sent on connect
- ✅ Client receives current progress
- ✅ No more "lost progress" after disconnect

## Docker Images Built
- `rome/genetics-worker:1.0.0` (111MB, built 2025-11-12)
- `rome/genetics-api-gateway:1.0.0` (97.1MB, built 2025-11-12)

## Database Schema (Current)
```sql
-- variants table (single-sample)
CREATE TABLE variants (
    rsid TEXT PRIMARY KEY,
    chromosome INTEGER NOT NULL,
    position INTEGER NOT NULL,
    ref_allele TEXT NOT NULL,
    alt_allele TEXT NOT NULL,
    dosage REAL NOT NULL,
    source TEXT NOT NULL,
    imputation_quality REAL
);

-- metadata table
CREATE TABLE metadata (
    job_id TEXT,
    user_id TEXT,
    processing_date TEXT,
    genome_file TEXT,
    imputation_server TEXT,
    reference_panel TEXT,
    total_snps INTEGER,
    genotyped_snps INTEGER,
    imputed_snps INTEGER,
    low_quality_snps INTEGER
);
```

## Database Schema (Planned for Multi-Sample)
```sql
-- sample_variants table
CREATE TABLE sample_variants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chromosome INTEGER NOT NULL,
    position INTEGER NOT NULL,
    rsid TEXT,
    ref_allele TEXT NOT NULL,
    alt_allele TEXT NOT NULL,
    sample_id TEXT NOT NULL,
    genotype TEXT NOT NULL,
    dosage REAL NOT NULL,
    source TEXT NOT NULL,
    imputation_quality REAL,
    UNIQUE(chromosome, position, ref_allele, alt_allele, sample_id)
);

-- reference_panel table
CREATE TABLE reference_variants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chromosome INTEGER NOT NULL,
    position INTEGER NOT NULL,
    rsid TEXT,
    ref_allele TEXT NOT NULL,
    alt_allele TEXT NOT NULL,
    phased INTEGER,
    allele_freq REAL,
    minor_allele_freq REAL,
    imputation_quality REAL,
    is_typed INTEGER,
    sample_genotypes TEXT NOT NULL  -- JSON array
);
```

## Future Work

### Immediate (Next Session)
1. Complete reference panel database conversion
2. Implement multi-sample data model in Rust
3. Update processor to merge 50+1 samples
4. Test 51-sample output matches R script
5. Update output formats for multi-sample

### Short Term
1. Add output metadata with quality statistics
2. Implement frontend quality threshold selection
3. Fix UX: Clear job ID after delete, return to process.html
4. Alert users about job ID and retrieval process

### Medium Term
1. Implement secure "find job" feature
2. Add authentication/authorization for job access
3. Prevent unauthorized genomic data access
4. Consider encryption for sensitive data

## Technical Debt
- Worker has unused code warnings (cleanup needed)
- API gateway has dead code warnings (cleanup needed)
- Need to sanitize instructor name references in documentation

## Documentation Updates Needed
- Update README with quality filtering options
- Document multi-sample architecture
- Create user guide for quality threshold selection
- Document security model for job access

## Files Modified This Session
```
stacks/genetics/
├── api-gateway/src/
│   ├── handlers.rs          (websocket reconnection fix)
│   ├── models.rs            (QualityThreshold enum)
│   └── queue.rs             (quality_threshold field)
├── app/src/
│   └── genotype_converter.rs (indel validation fix)
├── worker/src/
│   ├── job_processor.rs     (quality filtering logic)
│   ├── main.rs              (quality threshold support)
│   └── queue.rs             (quality_threshold field)
└── docs/
    └── CHANGELOG_2025-11-12.md (this file)
```

## Performance Metrics
- Unfiltered processing: 11.7M variants
- R2≥0.9 filtered: 4.3M variants (63% reduction)
- File size reduction: ~62% (SQLite: 1.3GB → 486MB)
- Processing time: ~5-10 minutes for full pipeline
- Docker image sizes: Worker 111MB, API 97MB

## Notes
- Private genomics files stored locally (not in repo)
- Reference panel database will be stored separately
- Multi-sample implementation is significant refactor
- Current single-sample code will be preserved for reference
