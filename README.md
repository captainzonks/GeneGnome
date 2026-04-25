# GeneGnome

**Secure, high-performance genetic data processing platform**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker)](docker-compose.yml)
[![Rust](https://img.shields.io/badge/Rust-1.94+-orange?logo=rust)](https://www.rust-lang.org/)

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/E1E21U3S1R)

> Process and analyze genetic data with enterprise-grade security, privacy-first design, and blazing-fast performance.

---

## Overview

GeneGnome is a self-hosted platform for processing genetic data from direct-to-consumer services (like 23andMe) and imputation servers (like Michigan Imputation Server). Built with Rust for maximum performance and memory safety, it provides:

- **60x faster processing** than traditional R-based pipelines
- **Multi-format output** (Parquet, VCF, SQLite) for downstream analysis
- **Browser-based VCF generation** via WebAssembly (no upload required)
- **Defense-in-depth security** with LUKS-encrypted storage and automatic data deletion
- **Self-hosted control** over your most sensitive data

### What Can GeneGnome Do?

- Merge 23andMe raw data with Michigan Imputation Server results
- Generate VCF files directly in your browser (no upload required)
- Process up to 6 million variants across 51 samples in ~2 minutes
- Deliver results via secure, password-protected email download links
- Provide self-service data management with recovery codes
- Visualize processed data with interactive charts (allele frequency, imputation quality, Ti/Tv ratio, heterozygosity, dosage distribution, variant types, per-chromosome breakdowns)
- Automatically clean up all data after 24 hours

---

## Key Features

### Security First

- **Encrypted Storage**: LUKS AES-256-XTS encrypted volumes for all genetic data
- **Network Isolation**: Processing containers have zero internet access
- **Automatic Deletion**: All data permanently deleted after 24 hours
- **Secure File Wiping**: DoD 5220.22-M compliant overwrite (not just file deletion)
- **Row-Level Security**: PostgreSQL RLS policies enforce job isolation
- **Audit Logging**: All data access and processing events logged
- **Secure Downloads**: Token-based downloads with password protection and attempt limits
- **Recovery Codes**: 8 single-use codes per job for self-service data deletion

### High Performance

- **Rust-Powered**: Memory-safe, zero-cost abstractions, concurrent processing
- **60x Faster**: ~2 minutes vs ~2 hours for traditional R script processing
- **Streaming Architecture**: Process datasets larger than available RAM
- **Efficient Formats**: Apache Parquet for analytics, SQLite for portability

### User Experience

- **Email Notifications**: Secure download link with password sent on completion
- **Recovery Codes**: Delete your data anytime without email access
- **Job Lookup**: Check status, resend email, or delete data via job ID
- **Data Visualization**: Interactive charts for allele frequency, Ti/Tv ratio, heterozygosity, dosage distribution, and variant types
- **WebSocket Progress**: Real-time processing updates in the browser
- **Chunked Upload**: Large file support (>50MB) bypassing CDN limits

### Self-Hosted

- **Your Infrastructure**: Keep sensitive genetic data on your own servers
- **No Cloud Dependencies**: Fully air-gapped processing possible
- **Docker Compose**: Single-command deployment
- **Reverse Proxy Ready**: Works with Traefik, Nginx, Caddy, etc.
- **Spoke Compatible**: Integrates as an external module in the [Spoke](https://github.com/captainzonks/spoke) platform

---

## Architecture

GeneGnome uses a microservices architecture with defense-in-depth security:

```
┌─────────────────────────────────────────────────────────────────┐
│                        Internet / Users                         │
└────────────────────────────┬────────────────────────────────────┘
                             │
                    ┌────────▼────────┐
                    │ Reverse Proxy   │ ← SSL/TLS Termination
                    │ (Traefik/Nginx) │ ← Rate Limiting
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
   ┌────▼─────┐      ┌──────▼──────┐      ┌─────▼──────┐
   │ Frontend │      │ API Gateway │      │  Download  │
   │ (Nginx)  │      │   (Axum)    │      │  Endpoint  │
   └──────────┘      └──────┬──────┘      └────────────┘
                             │
                    ┌────────▼────────┐
                    │   Job Queue     │
                    │   (Redis)       │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  Worker         │ ← No Internet Access
                    │  (Rust)         │ ← Isolated Network
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
| API Gateway | Rust/Axum (port 8099) | REST API, file uploads, downloads, WebSocket |
| Worker | Rust/Tokio | Background processing, email notifications |
| Database | PostgreSQL 18 | Job metadata, RLS, audit logging |
| Queue | Redis | Job queue |
| Storage | LUKS AES-256-XTS | Encrypted volume for genetic data |

Three independent Rust crates (no workspace):
- `genetics-processor` (`app/`) — core library: parsers, genotype conversion, output generation
- `genetics-api-gateway` (`api-gateway/`) — Axum REST API
- `genetics-worker` (`worker/`) — background job processor

---

## Getting Started

### Prerequisites

- **Docker**: Version 20.10+ with Docker Compose
- **Linux**: Required for LUKS encrypted volumes (tested on Arch, Ubuntu, Debian)
- **Storage**: Minimum 100GB for encrypted volume
- **RAM**: 16GB+ recommended for processing large datasets
- **CPU**: 4+ cores recommended

### Quick Start

1. **Clone the repository**

   ```bash
   git clone https://github.com/captainzonks/GeneGnome.git
   cd GeneGnome
   ```

2. **Create environment file**

   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   nano .env
   ```

3. **Set up encrypted volume**

   ```bash
   # Creates 100GB LUKS-encrypted volume at /mnt/genetics-encrypted
   sudo ./scripts/setup_encrypted_volume.sh
   ```

4. **Generate secrets**

   ```bash
   mkdir -p secrets/genetics secrets/smtp

   # Database password
   openssl rand -base64 32 > secrets/genetics/genetics_psql_password

   # API authentication key
   openssl rand -base64 32 > secrets/genetics/genetics_api_key

   # JWT signing secret
   openssl rand -base64 32 > secrets/genetics/genetics_jwt_secret

   # SMTP password (use your email provider's app password)
   echo 'your-smtp-password' > secrets/smtp/smtp_password

   # Secure the secrets
   chmod 600 secrets/*/*
   ```

5. **Download and prepare reference data**

   ```bash
   mkdir -p reference && cd reference

   # Download imputed reference panel (167 MB, ~5.9M variants)
   wget http://www.matthewckeller.com/public/VCF.Files3.RData

   # Convert to SQLite for Rust processor (requires R)
   cd .. && Rscript scripts/convert_reference_to_db.R

   # This creates reference/reference_panel.db (~4.7 GB)
   # See docs/REFERENCE_DATA.md for details
   ```

6. **Start services**

   ```bash
   docker compose up -d
   docker compose logs -f
   ```

7. **Access the web interface**

   Open your browser to `http://localhost` (or your configured domain)

### Configuration

See [.env.example](.env.example) for all available configuration options. Key settings:

- **Domain & SSL**: Configure your domain and reverse proxy
- **Email**: SMTP settings for download notifications
- **Security**: File size limits, data retention
- **Resources**: Memory limits, CPU allocation for containers

---

## Usage

### Web Interface

1. **Generate VCF** (optional) — Convert 23andMe raw data to VCF format for imputation, entirely in-browser
2. **Upload & process** — Upload your 23andMe file and imputed VCF results for server-side processing
3. **Save recovery codes** — 8 single-use codes shown at submission for self-service data management
4. **Receive email** — Secure download link with password sent when processing completes
5. **Download results** — Password-protected ZIP with Parquet, VCF, and SQLite files
6. **Explore insights** — Interactive visualization dashboard with charts for your processed data
7. **Manage your data** — Look up jobs, resend emails, or delete data via the Job Lookup page

### API Endpoints

```
POST   /api/genetics/jobs                       — Submit processing job (multipart upload)
GET    /api/genetics/jobs/{job_id}               — Job status (authenticated)
DELETE /api/genetics/jobs/{job_id}               — Delete job
GET    /api/genetics/jobs/{job_id}/ws            — WebSocket progress stream
GET    /api/genetics/jobs/{job_id}/status        — Public status lookup
POST   /api/genetics/jobs/{job_id}/resend-email  — Resend download email
POST   /api/genetics/jobs/{job_id}/delete        — Self-service deletion (recovery code)
GET    /api/genetics/jobs/{job_id}/visualization — Visualization data (job ID + password)
GET    /api/genetics/download                    — Token-based file download
GET    /api/genetics/visualization               — Token-based visualization data
POST   /api/genetics/upload/chunks               — Chunked file upload
POST   /api/genetics/upload/finalize             — Finalize chunked upload
GET    /api/genetics/health                      — Health check
GET    /api/genetics/ready                       — Readiness check
```

---

## Security Model

GeneGnome implements defense-in-depth with multiple security layers:

1. **Network Isolation** — Worker containers have zero internet access. Only the API gateway and frontend are publicly reachable.

2. **Encryption at Rest** — All genetic data stored on LUKS AES-256-XTS encrypted volumes.

3. **Secure File Deletion** — Input files are wiped using DoD 5220.22-M compliant overwrite patterns after processing. This is not a simple `rm` — the data is irrecoverable.

4. **Automatic Data Expiration** — All results permanently deleted 24 hours after job completion. Hourly cleanup sweep enforces this.

5. **Secure Downloads** — Token-based authentication with password protection. Maximum 5 attempts per download link. Tokens expire with the job.

6. **Recovery Codes** — 8 Argon2id-hashed single-use codes per job. Users can delete their data at any time without email access.

7. **Row-Level Security** — PostgreSQL RLS policies enforce job isolation at the database level.

8. **Container Hardening** — Non-root users (UID 3000), capability dropping (CAP_DROP ALL), resource limits.

9. **Audit Logging** — All data access and processing events logged to an append-only audit table.

See [PRIVACY.md](PRIVACY.md) for the full privacy policy.

---

## Performance

Tested on AMD Ryzen 5600X (6 cores, 12 threads), 32GB RAM:

| Dataset Size | Variants | Samples | R Script | GeneGnome | Speedup |
|-------------|----------|---------|----------|-----------|---------|
| Small       | 100K     | 1       | 2 min    | 2 sec     | 60x     |
| Medium      | 1M       | 1       | 20 min   | 20 sec    | 60x     |
| Large       | 6M       | 51      | 120 min  | 2 min     | 60x     |

- **Memory**: ~2GB peak vs ~40GB for R (streaming architecture)
- **Concurrent Jobs**: Worker pool processes multiple jobs simultaneously
- **Parquet Compression**: 10x storage reduction over raw formats

---

## Documentation

| Document | Description |
|----------|-------------|
| [README.md](README.md) | This file — overview and quick start |
| [PRIVACY.md](PRIVACY.md) | Privacy policy and data handling |
| [.env.example](.env.example) | Environment configuration reference |
| [docs/REFERENCE_DATA.md](docs/REFERENCE_DATA.md) | Reference panel database details |
| [docs/parquet_usage_guide.md](docs/parquet_usage_guide.md) | Working with Parquet output files |

---

## Contributing

Contributions are welcome!

### Development Setup

Three independent Rust crates (no workspace — each has its own `Cargo.lock`):

```bash
git clone https://github.com/captainzonks/GeneGnome.git
cd GeneGnome

# Build individual crates
cd app && cargo build --release
cd ../api-gateway && cargo build --release
cd ../worker && cargo build --release

# Run tests
cd app && cargo test
cd ../api-gateway && cargo test
cd ../worker && cargo test

# Lint
cargo fmt --all --check    # per crate
cargo clippy --all-targets # per crate
```

### Docker Build

```bash
docker build -f api-gateway/Dockerfile -t genegnome/genetics-api-gateway .
docker build -f worker/Dockerfile -t genegnome/genetics-worker .
docker build -f frontend/Dockerfile -t genegnome/genetics-frontend .
```

---

## License

GeneGnome is dual-licensed under:

- **Apache License 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

You may choose either license when using this software.

---

## Acknowledgments

- **Reference Panel**: 50 anonymous genome samples originally from openSNP.org (now closed), freely uploaded for research. Current mirror: http://www.matthewckeller.com/public/VCF.Files3.RData
- **Michigan Imputation Server**: https://imputationserver.sph.umich.edu/
- **Original Pipeline**: Inspired by the R-based `mergeData()` pipeline by Dr. Matthew C. Keller

---

**Author**: Matthew Barham
**Version**: 1.2.0
**Last Updated**: 2026-04-03

For questions, issues, or feature requests: [GitHub Issues](https://github.com/captainzonks/GeneGnome/issues)

**Disclaimer**: GeneGnome is for research and educational purposes. It is not a medical device and should not be used for clinical decision-making. Always consult qualified healthcare professionals for medical advice.
