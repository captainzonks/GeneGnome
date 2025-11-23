# GeneGnome

**Secure, high-performance genetic data processing platform**

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE-MIT)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?logo=docker)](docker-compose.yml)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)

> Process and analyze genetic data with enterprise-grade security, privacy-first design, and blazing-fast performance.

---

## 📋 Table of Contents

- [Overview](#overview)
- [Key Features](#key-features)
- [Architecture](#architecture)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Quick Start](#quick-start)
  - [Configuration](#configuration)
- [Usage](#usage)
- [Security Model](#security-model)
- [Performance](#performance)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)
- [Acknowledgments](#acknowledgments)

---

## Overview

GeneGnome is a self-hosted platform for processing genetic data from direct-to-consumer services (like 23andMe) and imputation servers (like Michigan Imputation Server). Built with Rust for maximum performance and memory safety, it provides:

- **60× faster processing** than traditional R-based pipelines
- **Multi-format output** (Parquet, VCF, SQLite) for downstream analysis
- **Browser-based interface** with WebAssembly-powered VCF generation
- **Defense-in-depth security** with encrypted storage and automatic data deletion
- **Self-hosted control** over your most sensitive data

### What Can GeneGnome Do?

- Merge 23andMe raw data with Michigan Imputation Server results
- Generate VCF files directly in your browser (no upload required)
- Process up to 6 million variants across 51 samples in ~2 minutes
- Automatically clean up processed data after configurable retention period
- Provide secure, password-protected download links via email

---

## Key Features

### 🔒 **Security First**

- **Encrypted Storage**: LUKS AES-256-XTS encrypted volumes for all genetic data
- **Network Isolation**: Processing containers have zero internet access
- **Automatic Deletion**: Configurable data retention (default: 72 hours)
- **Row-Level Security**: PostgreSQL RLS policies enforce multi-tenant isolation
- **Audit Logging**: Comprehensive logs of all data access and processing
- **Secure Downloads**: Token-based downloads with password protection

### ⚡ **High Performance**

- **Rust-Powered**: Memory-safe, zero-cost abstractions, concurrent processing
- **60× Faster**: ~2 minutes vs ~2 hours for traditional R script processing
- **Streaming Architecture**: Process datasets larger than available RAM
- **Efficient Formats**: Apache Parquet for analytics, SQLite for portability

### 🛠️ **Developer Friendly**

- **Docker Compose**: Single-command deployment
- **REST API**: Full-featured API for programmatic access
- **WebAssembly**: Client-side VCF generation (no server upload needed)
- **Multiple Outputs**: Parquet, VCF, SQLite - use what fits your workflow
- **Type Safety**: Rust's type system prevents entire classes of bugs

### 🌐 **Self-Hosted**

- **Your Infrastructure**: Keep sensitive genetic data on your own servers
- **No Cloud Dependencies**: Fully air-gapped processing possible
- **Flexible Deployment**: Docker Compose, Kubernetes, or bare metal
- **Reverse Proxy Ready**: Works with Traefik, Nginx, Caddy, etc.

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
                    └────────┬────────┘ ← Authentication
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
                    │  Worker Pool    │ ← No Internet Access
                    │  (Rust)         │ ← Isolated Network
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
   ┌────▼─────┐      ┌──────▼──────┐      ┌─────▼──────┐
   │ Database │      │  Encrypted  │      │   Email    │
   │(Postgres)│      │   Storage   │      │   Relay    │
   └──────────┘      │   (LUKS)    │      └────────────┘
                     └─────────────┘
```

### Components

- **Frontend**: Nginx serving static HTML/CSS/JS + WebAssembly
- **API Gateway**: Rust/Axum REST API for uploads, status, downloads
- **Worker**: Background job processor (genetics merging/conversion)
- **Database**: PostgreSQL with row-level security for job metadata
- **Queue**: Redis for job queue and rate limiting
- **Storage**: LUKS-encrypted volume for temporary genetic data files

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
   git clone https://github.com/YOUR-USERNAME/GeneGnome.git
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
   mkdir -p secrets/genetics secrets/proton

   # Database password
   openssl rand -base64 32 > secrets/genetics/genetics_psql_password

   # API authentication key
   openssl rand -base64 32 > secrets/genetics/genetics_api_key

   # JWT signing secret
   openssl rand -base64 32 > secrets/genetics/genetics_jwt_secret

   # SMTP password (or use your email provider's app password)
   echo 'your-smtp-password' > secrets/proton/proton_bridge_password

   # Secure the secrets
   chmod 600 secrets/*/*
   ```

5. **Download and prepare reference data**

   ```bash
   # Create reference directory
   mkdir -p reference
   cd reference

   # Download imputed reference panel (167 MB, ~5.9M variants)
   wget http://www.matthewckeller.com/public/VCF.Files3.RData

   # Convert to SQLite for Rust processor (requires R)
   cd ..
   Rscript scripts/convert_reference_to_db.R

   # This creates reference/reference_panel.db (~4.7 GB)
   # See docs/REFERENCE_DATA.md for details
   ```

6. **Start services**

   ```bash
   docker-compose up -d
   ```

7. **Check status**

   ```bash
   docker-compose ps
   docker-compose logs -f
   ```

8. **Access the web interface**

   Open your browser to:
   - Frontend: `http://localhost` (or your configured domain)
   - API: `http://localhost:8090/health`

### Configuration

See [.env.example](.env.example) for all available configuration options. Key settings:

- **Domain & SSL**: Configure your domain and reverse proxy
- **Email**: SMTP settings for download notifications
- **Security**: Session timeout, file size limits, data retention
- **Resources**: Memory limits, CPU allocation for containers

For detailed setup instructions, see [docs/SETUP.md](docs/SETUP.md) (if available).

---

## Usage

### Web Interface

1. **Upload genetic data**: Drag and drop your 23andMe file and Michigan Imputation Server results
2. **Monitor progress**: Real-time progress updates via WebSocket
3. **Receive email**: Secure download link sent when processing completes
4. **Download results**: Password-protected Parquet/VCF/SQLite files

### VCF Generator (Browser-Only)

For privacy-conscious users who don't want to upload data:

1. Navigate to `/vcf-generator.html` in your browser
2. Select your 23andMe file (never leaves your computer)
3. VCF file generated entirely in-browser using WebAssembly
4. Download immediately, no server processing

### API Usage

```bash
# Upload files for processing
curl -X POST http://your-domain.com/api/genetics/upload \
  -F "genome_file=@genome_23andme.txt" \
  -F "vcf_file=@imputed_results.vcf.gz" \
  -F "email=user@example.com"

# Check job status
curl http://your-domain.com/api/genetics/status/{job_id}

# Download results (requires token from email)
curl -O http://your-domain.com/download/{job_id}?token={download_token}&password={password}
```

See [docs/API.md](docs/API.md) for complete API documentation (if available).

---

## Security Model

GeneGnome implements defense-in-depth with multiple security layers:

### 1. **Network Isolation**
   - Worker containers have **zero internet access**
   - Internal networks for database/queue communication only
   - Only API gateway and frontend expose public interfaces

### 2. **Encryption at Rest**
   - All genetic data stored on LUKS AES-256-XTS encrypted volumes
   - Automatic mounting/unmounting with system cryptsetup
   - Keys never stored in application code or containers

### 3. **Automatic Data Deletion**
   - Default 72-hour retention for all uploaded data
   - Background worker runs cleanup every hour
   - Secure deletion using shred (7-pass overwrite)

### 4. **Row-Level Security**
   - PostgreSQL RLS policies enforce job ownership
   - Users can only access their own jobs via session tokens
   - Database prevents cross-user data leaks at query level

### 5. **Secure Downloads**
   - Token-based authentication for download links
   - Additional password protection (user-set during upload)
   - Single-use tokens expire after download

### 6. **Audit Logging**
   - All file access logged with timestamps and user IDs
   - Immutable audit trail for compliance (7-year retention)
   - Tamper-evident logging to detect unauthorized access

### 7. **Container Hardening**
   - Non-root users (UID 3000) in all containers
   - Capability dropping (CAP_DROP ALL)
   - Read-only root filesystems where possible
   - Resource limits prevent DoS attacks

For full security architecture, see [docs/SECURITY.md](docs/SECURITY.md) (if available).

---

## Performance

GeneGnome is designed for high-throughput processing of genetic datasets:

### Benchmarks

Tested on AMD Ryzen 5600X (6 cores, 12 threads), 32GB RAM:

| Dataset Size | Variants | Samples | R Script | GeneGnome | Speedup |
|-------------|----------|---------|----------|-----------|---------|
| Small       | 100K     | 1       | 2 min    | 2 sec     | 60×     |
| Medium      | 1M       | 1       | 20 min   | 20 sec    | 60×     |
| Large       | 6M       | 51      | 120 min  | 2 min     | 60×     |

### Scalability

- **Concurrent Jobs**: Process multiple jobs simultaneously (worker pool)
- **Memory Efficient**: Streaming architecture handles datasets larger than RAM
- **CPU Utilization**: Rayon-powered parallel processing across all cores
- **Format Optimization**: Parquet compression reduces storage by 10×

---

## Documentation

- **[README.md](README.md)** - This file (overview and quick start)
- **[LICENSE](LICENSE)** - Dual Apache-2.0 / MIT licensing
- **[.env.example](.env.example)** - Environment configuration reference
- **[examples/README.md](examples/README.md)** - Example data and testing guide
- **[docs/](docs/)** - Complete documentation directory
  - **[REFERENCE_DATA.md](docs/REFERENCE_DATA.md)** - Reference panel databases explained
  - Security architecture
  - API reference
  - Deployment guides
  - Troubleshooting

---

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines (if available).

### Development Setup

1. Install Rust toolchain: https://rustup.rs/
2. Install Docker and Docker Compose
3. Clone repository and install dependencies:
   ```bash
   git clone https://github.com/YOUR-USERNAME/GeneGnome.git
   cd GeneGnome
   cargo build
   ```

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests (requires Docker)
docker-compose -f docker-compose.test.yml up --abort-on-container-exit
```

### Code Quality

- Format: `cargo fmt`
- Lint: `cargo clippy`
- Audit: `cargo audit`

---

## License

GeneGnome is dual-licensed under:

- **Apache License 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))

You may choose either license when using this software.

### Why Dual License?

This provides maximum flexibility:
- **Apache 2.0**: Explicit patent protection and trademark guidelines
- **MIT**: Maximum simplicity and permissiveness

Choose the license that best fits your use case.

---

## Acknowledgments

### Reference Data

- **Reference Panel**: 50 anonymous genome samples originally from openSNP.org (now closed)
  - Freely uploaded by users for research purposes
  - Current mirror: http://www.matthewckeller.com/public/VCF.Files3.RData
- **Michigan Imputation Server**: https://imputationserver.sph.umich.edu/

### Technologies

- **Rust**: Safe, fast, concurrent programming language
- **WebAssembly**: Browser-based genetics processing
- **PostgreSQL**: Reliable, ACID-compliant database
- **Docker**: Containerization and deployment
- **Axum**: Ergonomic web framework for Rust
- **Apache Parquet**: Efficient columnar storage format

### Inspiration

GeneGnome is inspired by the original R-based `mergeData()` pipeline by Dr. Matthew C. Keller, significantly rewritten and optimized in Rust for production use.

---

## Contact

**Author**: Matthew Barham
**Created**: 2025-10-31
**Last Updated**: 2025-11-20
**Status**: Production-ready (v1.0.0+)

For questions, issues, or feature requests:
- **GitHub Issues**: https://github.com/YOUR-USERNAME/GeneGnome/issues
- **Email**: See [.env.example](.env.example) for contact configuration

---

**⚠️ Disclaimer**: GeneGnome is for research and educational purposes. It is not a medical device and should not be used for clinical decision-making. Always consult qualified healthcare professionals for medical advice.

---

Made with ❤️ and Rust
