# ==============================================================================
# GENETICS WORKER DOCKERFILE
# ==============================================================================
# Description: Background worker for genetic data processing
# Author: Matthew Barham
# Created: 2025-11-07
# Modified: 2025-11-07
# Version: 1.0.0
# Host: your-server
# ==============================================================================
# Base Image: rust:1.91-slim (build), alpine:latest (runtime)
# Target Architecture: amd64
# Security: non-root user (default genetics:3000, configurable), minimal dependencies
# ==============================================================================
# Build Command (from GeneGnome root):
#   docker build -t genegnome/genetics-worker:latest \
#     -f worker/Dockerfile .
#
# Build with custom user:
#   docker build -t genegnome/genetics-worker:latest \
#     --build-arg USER_ID=1000 \
#     --build-arg USERNAME=genetics \
#     -f worker/Dockerfile .
# ==============================================================================

#=============================================================================
# BUILD STAGE - Compile Rust Application
#=============================================================================

FROM rust:1.91-slim AS builder

# Install build dependencies
RUN apt update -qq && apt install -y -qq --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /build

# Copy workspace files
COPY app/Cargo.toml ./app/
COPY app/src ./app/src/
COPY worker/Cargo.toml ./worker/
COPY worker/src ./worker/src/

# Build genetics-processor library first
WORKDIR /build/app
RUN cargo build --release --lib

# Build Worker
WORKDIR /build/worker
RUN cargo build --release

#=============================================================================
# RUNTIME STAGE - Minimal Production Image
#=============================================================================

FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt update && apt install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Build arguments for user configuration
ARG USER_ID=3000
ARG USERNAME=genetics

# Create genetics user (configurable via build args)
RUN useradd --uid ${USER_ID} --shell /usr/sbin/nologin --no-create-home ${USERNAME}

# Copy binary from builder
COPY --from=builder /build/worker/target/release/genetics-worker /usr/local/bin/

# Set permissions
RUN chmod a+rx /usr/local/bin/genetics-worker

# Switch to non-root user
USER ${USERNAME}

# Set working directory
WORKDIR /app

# Health check (worker doesn't expose HTTP, check process)
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD pgrep -x genetics-worker || exit 1

# Run Worker
ENTRYPOINT ["/usr/local/bin/genetics-worker"]
