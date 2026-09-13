# GeneGnome

<!--
==============================================================================
README.md - GeneGnome project overview
==============================================================================
Description: Overview, architecture, deployment, security model, and measured
             performance for the GeneGnome genetic data processing platform
Author: Matt Barham
Created: 2025-11-22
Modified: 2026-09-13
Version: 1.3.0
==============================================================================
Document Type: Reference
Audience: Developer, Operator
Status: Active
==============================================================================
-->

**Self-hosted genetic data processing platform**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker)](docker-compose.yml)
[![Rust](https://img.shields.io/badge/Rust-1.98+-orange?logo=rust)](https://www.rust-lang.org/)

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/E1E21U3S1R)

> Merge 23andMe raw data with imputation server results on your own hardware,
> with encrypted storage and automatic data expiry.

---

## Overview

GeneGnome processes genetic data from direct-to-consumer services (23andMe)
and imputation servers (Michigan Imputation Server). It is written in Rust,
deployed with Docker Compose, and designed to run on infrastructure you
control.

- Merges a 23andMe export with Michigan Imputation Server output against a
  50-sample reference panel
- Generates the multi-sample VCF that Michigan requires, in the browser via
  WebAssembly — that step involves no upload
- Writes Parquet, VCF, and SQLite for downstream analysis
- Stores all genetic data on a LUKS AES-256-XTS volume and deletes it after
  24 hours
- Delivers results through a rate-limited, token-gated download endpoint

A full 6M-variant, 51-sample merge takes roughly two minutes on a 6-core
desktop. See [Performance](#performance) for what was measured and how.

**Scope:** autosomes 1–22, GRCh37/hg19, fixed 51-sample merge model. See
[Limitations](#limitations) before deploying.

---

## Features

### Security

- **Encrypted storage** — LUKS AES-256-XTS volume for all genetic data
- **Network isolation** — the worker runs on a Docker network declared
  `internal: true` and has no route off-host
- **Automatic deletion** — results removed 24 hours after completion by an
  hourly cleanup loop
- **Secure file wiping** — input files overwritten with the DoD 5220.22-M
  7-pass pattern, not unlinked
- **Row-level security** — `api-gateway` and `worker` connect as
  `genetics_app`, a role that is neither superuser nor schema owner, so the
  isolation policies actually evaluate. This was not true before
  [ADR 0001](docs/adr/0001-rls-role-separation.md); that document explains
  why writing correct policy SQL was not sufficient, and
  `api-gateway/tests/rls_enforcement.rs` is the integration test that proves
  the current behaviour against a live connection.
- **Download gating** — single-use token plus password, maximum five attempts,
  token expires with the job
- **Recovery codes** — eight Argon2id-hashed single-use codes per job, for
  deletion without email access
- **Container hardening** — non-root (UID 3000), `cap_drop: ALL`, per-service
  memory and CPU limits
- **Audit logging** — data access and processing events written to an
  append-only table

Full policy in [PRIVACY.md](PRIVACY.md).

### Processing

- **Streaming output on the worker path** — Parquet written in 10,000-row
  batches per chromosome (`app/src/output.rs`, invoked from
  `worker/src/job_processor.rs` via `initialize_streaming_output`/
  `finalize_streaming_output`), so peak memory does not scale with dataset
  size. The in-process library path (`app/src/processor.rs`) accumulates
  merged chromosomes in memory instead; use the worker for full-size
  datasets.
- **Strand-flip handling** — reverse-complement fallback on allele mismatch,
  with unit tests (`app/src/genotype_converter.rs`)
- **Quality filtering** — configurable DR2 threshold during VCF parsing
- **Concurrent jobs** — each dequeued job is spawned as its own Tokio task.
  There is no concurrency cap; bound it with container resource limits.

### Interface

- WebSocket progress stream during processing
- Chunked upload for files over 50MB, bypassing CDN body limits
- Email notification with download link and password on completion
- Job lookup: status, email resend, self-service deletion
- Visualization dashboard: allele frequency, imputation quality, Ti/Tv ratio,
  heterozygosity, dosage distribution, variant types, per-chromosome counts

### Deployment

- Docker Compose, single command after setup
- Works behind Traefik, Nginx, or Caddy
- Integrates as an external module in the
  [Spoke](https://github.com/captainzonks/spoke) platform, and runs standalone
  without it

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Internet / Users                         │
└────────────────────────────┬────────────────────────────────────┘
                             │
                    ┌────────▼────────┐
                    │ Reverse Proxy   │ ← TLS termination
                    │ (Traefik/Nginx) │ ← rate limiting
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
   ┌────▼─────┐      ┌──────▼──────┐      ┌─────▼──────┐
   │ Frontend │      │ API Gateway │      │  Download  │
   │ (Nginx)  │      │   (Axum)    │      │  Endpoint  │
   │  + WASM  │      │             │      │            │
   └──────────┘      └──────┬──────┘      └────────────┘
                             │
══════════════════════════════════════════════════════════════════
   internal: true — no route to any external network below here
══════════════════════════════════════════════════════════════════
                             │
                    ┌────────▼────────┐
                    │   Job Queue     │
                    │   (Redis)       │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │     Worker      │
                    │  (Rust/Tokio)   │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
   ┌────▼─────┐      ┌──────▼──────┐      ┌─────▼──────┐
   │ Database │      │  Encrypted  │      │   Email    │
   │(Postgres)│      │   Storage   │      │   (SMTP)   │
   └──────────┘      │   (LUKS)    │      └────────────┘
                     └─────────────┘
```

### Components

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Frontend | Nginx + WebAssembly | Static UI, client-side VCF generation |
| API Gateway | Rust / Axum (port 8099) | REST API, uploads, downloads, WebSocket |
| Worker | Rust / Tokio | Background processing, email notification |
| Database | PostgreSQL 18 | Job metadata, RLS, audit log |
| Queue | Redis | Job queue |
| Storage | LUKS AES-256-XTS | Encrypted volume for genetic data |

Three independent crates, no Cargo workspace — each carries its own
`Cargo.lock`:

| Crate | Path | Role |
|---|---|---|
| `genetics-processor` | `app/` | Parsers, genotype conversion, output generation |
| `genetics-api-gateway` | `api-gateway/` | Axum REST API |
| `genetics-worker` | `worker/` | Background job processor |

Rust 1.98+, edition 2024.

### Client-side VCF generation

Michigan Imputation Server rejects single-sample VCFs, so submission requires
merging the user's genotypes with a reference panel first. GeneGnome does that
in the browser: `frontend/www/stisty_wasm_bg.wasm` loads
`frontend/www/reference_db.bin.br` (8.4MB, Brotli-compressed) and emits a
reference-aware VCF with correct REF/ALT alleles from GRCh37. The genotype
file never leaves the machine for this step. If the reference database fails
to load, `vcf-app.js` falls back to generation without reference alleles.

The later merge step — combining imputation results with the reference panel —
is server-side and does involve upload.

---

## Getting Started

### Prerequisites

- Docker 20.10+ with Compose v2
- Linux — required for the LUKS encrypted volume (tested on Arch, Ubuntu,
  Debian)
- R — required once, to convert the reference panel to SQLite
- 100GB+ storage for the encrypted volume; the reference database alone is
  ~4.7GB
- 16GB+ RAM and 4+ cores recommended

### Quick Start

1. **Clone**

   ```bash
   git clone https://github.com/captainzonks/GeneGnome.git
   cd GeneGnome
   ```

2. **Configure**

   ```bash
   cp .env.example .env
   $EDITOR .env
   ```

3. **Create the encrypted volume**

   ```bash
   # 100GB LUKS volume at /mnt/genetics-encrypted
   sudo ./scripts/setup_encrypted_volume.sh
   ```

4. **Generate secrets**

   ```bash
   mkdir -p secrets/genetics secrets/smtp

   # Schema owner / bootstrap role — NOT what the services connect as
   openssl rand -base64 32 > secrets/genetics/genetics_psql_password

   # Runtime role — api-gateway and worker connect as this (see ADR 0001)
   openssl rand -base64 32 > secrets/genetics/genetics_app_db_password

   # API authentication key
   openssl rand -base64 32 > secrets/genetics/genetics_api_key

   # JWT signing secret
   openssl rand -base64 32 > secrets/genetics/genetics_jwt_secret

   # SMTP password (an app password from your provider)
   printf '%s' 'your-smtp-password' > secrets/smtp/smtp_password

   chmod 600 secrets/*/*
   ```

5. **Prepare reference data**

   ```bash
   mkdir -p reference && cd reference

   # Imputed reference panel, 167MB, ~5.9M variants
   wget http://www.matthewckeller.com/public/VCF.Files3.RData

   cd .. && Rscript scripts/convert_reference_to_db.R
   # Produces reference/reference_panel.db (~4.7GB)
   ```

   Details and provenance in [docs/REFERENCE_DATA.md](docs/REFERENCE_DATA.md).

6. **Start**

   ```bash
   docker compose up -d
   docker compose logs -f
   ```

7. Open `http://localhost`, or your configured domain.

### Upgrading an existing deployment

Deployments created before the role separation in
`database/migrations/004_separate_runtime_role_from_owner.sql` must be
upgraded in a specific order — provision the `genetics_app` password secret,
run `database/00-create-app-role.sh`, apply the migration, then redeploy the
services pointed at the new role. Out of order, the application either keeps
bypassing RLS or fails to connect. The runbook is in the migration header and
in [ADR 0001](docs/adr/0001-rls-role-separation.md).

### Configuration

See [.env.example](.env.example). Key groups: domain and TLS, SMTP, upload
size limits and retention, per-container memory and CPU limits.

---

## Usage

1. **Generate VCF** (optional) — convert a 23andMe export to a multi-sample
   VCF for imputation, in-browser
2. **Upload and process** — submit the 23andMe file and the imputed VCF
   results
3. **Save recovery codes** — eight single-use codes, shown once at submission
4. **Receive email** — download link and password on completion
5. **Download** — a ZIP containing Parquet, VCF, and SQLite outputs
6. **Explore** — visualization dashboard for the processed job
7. **Manage** — look up jobs, resend email, or delete data

**On the download ZIP:** the archive is written with
`CompressionMethod::Stored` and is not itself encrypted. The password gates
the download endpoint (single-use token, five-attempt limit); protection at
rest is the LUKS volume. Treat the downloaded file as plaintext once it
reaches your machine.

### API

```
POST   /api/genetics/jobs                        — submit job (multipart)
GET    /api/genetics/jobs/{job_id}               — job status (authenticated)
DELETE /api/genetics/jobs/{job_id}               — delete job
GET    /api/genetics/jobs/{job_id}/ws            — WebSocket progress stream
GET    /api/genetics/jobs/{job_id}/status        — public status lookup
POST   /api/genetics/jobs/{job_id}/resend-email  — resend download email
POST   /api/genetics/jobs/{job_id}/delete        — deletion via recovery code
GET    /api/genetics/jobs/{job_id}/visualization — visualization data
GET    /api/genetics/download                    — token-based download
GET    /api/genetics/visualization               — token-based visualization
POST   /api/genetics/upload/chunks               — chunked upload
POST   /api/genetics/upload/finalize             — finalize chunked upload
GET    /api/genetics/health                      — health check
GET    /api/genetics/ready                       — readiness check
```

---

## Performance

### End-to-end merge, measured once

One full comparison against the R pipeline was run on 2025-11-17, recorded in
[docs/CHANGELOG_2025-11-17.md](docs/CHANGELOG_2025-11-17.md). Dataset: the
full 5.9M-variant reference panel merged across 51 samples, on a Ryzen 5600X
(6 cores / 12 threads), 32GB RAM.

| Metric | R pipeline | GeneGnome |
|---|---|---|
| Wall clock | ~120 min | ~2 min |
| Peak memory | ~40 GB | ~2 GB |
| Parallelism | single core | multi-core |

This is **one measurement of one dataset**, not a benchmark suite. There is no
committed benchmark harness in this repository — no `benches/`, no criterion —
and the R script and comparison tooling live under the gitignored
`genome-data/` directory, so the run above cannot be reproduced from a clone.
Read the numbers as an order-of-magnitude result for this workload rather than
a general speedup factor, and do not extrapolate them to smaller inputs.

Output sizes from the same run:

| Format | Size | Notes |
|---|---|---|
| Parquet (Snappy) | 436 MB | columnar, best for analysis |
| VCF (bgzip) | 243 MB | 19.75:1 against plain text |
| VCF (plain) | 4.8 GB | |
| SQLite | ~1.3 GB | queryable |
| RData (R output) | 182 MB | for comparison |

### VCF parsing, reproducible

`app/examples/vcf_test.rs` times the parser and can be run against any
`.dose.vcf.gz`:

```bash
cd app && cargo run --release --example vcf_test -- /path/to/chr22.dose.vcf.gz
```

Recorded results for chr22 (152K SNPs) are in
[app/docs/vcf_parser_benchmark.md](app/docs/vcf_parser_benchmark.md): 7.12s
via `noodles-vcf` (21,373 records/sec), against 4.49s (31,312 records/sec) for
a hand-rolled text parser that was rejected for weaker error handling and
partial spec compliance. The slower, correct parser is the one in production;
that document explains the trade.

### Correctness against the R pipeline

[docs/rust_vs_r_comparison.md](docs/rust_vs_r_comparison.md) records a
1,560,234-variant (26.4%) discrepancy found between the two implementations
and traced to a superseded single-sample merge strategy. It is kept as a
point-in-time record of that investigation, not as current output.

---

## Limitations

- **Autosomes only.** Chromosomes 1–22. X, Y, and mitochondrial variants are
  dropped at parse time.
- **GRCh37/hg19 only.** No liftover. Inputs on another build will merge
  incorrectly rather than fail loudly.
- **Fixed merge model.** 50 reference samples plus one user sample. The panel
  is not swappable without code changes.
- **Reference preparation needs R.** `scripts/convert_reference_to_db.R` is
  the only supported path to `reference_panel.db`.
- **Linux and root required** for the LUKS volume. There is no unencrypted
  deployment mode.
- **24-hour retention is not configurable per user.** Everything is deleted by
  the hourly sweep.
- **The library path is not streaming.** `app/src/processor.rs` holds merged
  chromosomes in memory; only the worker streams.
- **Single-node deployment.** No CI/CD to production and no horizontal
  scaling. See the ADR discussion in
  [Spoke](https://github.com/captainzonks/spoke) for the reasoning.

---

## Documentation

| Document | Description |
|----------|-------------|
| [PRIVACY.md](PRIVACY.md) | Privacy policy and data handling |
| [.env.example](.env.example) | Configuration reference |
| [docs/adr/0001-rls-role-separation.md](docs/adr/0001-rls-role-separation.md) | Why RLS needed role separation, and what changed |
| [docs/REFERENCE_DATA.md](docs/REFERENCE_DATA.md) | Reference panel provenance and preparation |
| [docs/parquet_usage_guide.md](docs/parquet_usage_guide.md) | Working with Parquet output |
| [docs/PGS_CALCULATION_REFERENCE.md](docs/PGS_CALCULATION_REFERENCE.md) | Polygenic score calculation |
| [docs/platform_architecture.md](docs/platform_architecture.md) | Platform architecture detail |
| [docs/email_credential_security_design.md](docs/email_credential_security_design.md) | Email credential handling |
| [app/docs/vcf_parser_benchmark.md](app/docs/vcf_parser_benchmark.md) | Parser selection and timings |

Point-in-time records, kept for history and not maintained:
[docs/CHANGELOG_2025-11-12.md](docs/CHANGELOG_2025-11-12.md),
[docs/CHANGELOG_2025-11-17.md](docs/CHANGELOG_2025-11-17.md),
[docs/rust_vs_r_comparison.md](docs/rust_vs_r_comparison.md),
[docs/architecture/data_comparison_analysis.md](docs/architecture/data_comparison_analysis.md),
[docs/mergeData_pipeline_analysis.md](docs/mergeData_pipeline_analysis.md),
[docs/r_processing_pipeline_specification.md](docs/r_processing_pipeline_specification.md),
[docs/r_script_output_analysis.md](docs/r_script_output_analysis.md),
[docs/rust_implementation_strategy.md](docs/rust_implementation_strategy.md),
[docs/noodles_vcf_research.md](docs/noodles_vcf_research.md).

---

## Development

```bash
git clone https://github.com/captainzonks/GeneGnome.git
cd GeneGnome

# Build (per crate — there is no workspace)
cd app         && cargo build --release
cd ../api-gateway && cargo build --release
cd ../worker   && cargo build --release

# Test — 67 tests: app 40, api-gateway 19, worker 8
cd app         && cargo test
cd ../api-gateway && cargo test
cd ../worker   && cargo test

# Lint (per crate)
cargo fmt --check
cargo clippy --all-targets
```

`api-gateway/tests/rls_enforcement.rs` is an integration test and needs a live
PostgreSQL with the `genetics_app` role provisioned; see the ADR for setup.

### Docker images

```bash
docker build -f api-gateway/Dockerfile -t genegnome/genetics-api-gateway .
docker build -f worker/Dockerfile      -t genegnome/genetics-worker .
docker build -f frontend/Dockerfile    -t genegnome/genetics-frontend .
```

---

## License

Dual-licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option.

---

## Acknowledgments

- **Reference panel** — 50 anonymous genomes originally from openSNP.org (now
  closed), uploaded freely for research. Current mirror:
  http://www.matthewckeller.com/public/VCF.Files3.RData
- **Michigan Imputation Server** — https://imputationserver.sph.umich.edu/
- **Original pipeline** — the R `mergeData()` pipeline by Dr. Matthew C.
  Keller

---

**Disclaimer:** GeneGnome is for research and educational use. It is not a
medical device and must not be used for clinical decision-making.

Issues and feature requests:
[GitHub Issues](https://github.com/captainzonks/GeneGnome/issues)
