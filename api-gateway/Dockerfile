# ==============================================================================
# GENETICS API GATEWAY DOCKERFILE
# ==============================================================================
# Description: API Gateway for genetic data processing service
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
#   docker build -t genegnome/genetics-api-gateway:latest \
#     -f api-gateway/Dockerfile .
#
# Build with custom user:
#   docker build -t genegnome/genetics-api-gateway:latest \
#     --build-arg USER_ID=1000 \
#     --build-arg USERNAME=genetics \
#     -f api-gateway/Dockerfile .
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
COPY api-gateway/Cargo.toml ./api-gateway/
COPY api-gateway/src ./api-gateway/src/

# Build genetics-processor library first
WORKDIR /build/app
RUN cargo build --release --lib

# Build API Gateway
WORKDIR /build/api-gateway
RUN cargo build --release

#=============================================================================
# RUNTIME STAGE - Minimal Production Image
#=============================================================================

FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt update && apt install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Build arguments for user configuration
ARG USER_ID=3000
ARG USERNAME=genetics

# Create genetics user (configurable via build args)
RUN useradd --uid ${USER_ID} --shell /usr/sbin/nologin --no-create-home ${USERNAME}

# Copy binary from builder
COPY --from=builder /build/api-gateway/target/release/genetics-api-gateway /usr/local/bin/

# Set permissions
RUN chmod a+rx /usr/local/bin/genetics-api-gateway

# Switch to non-root user
USER ${USERNAME}

# Expose API port
EXPOSE 8080

# Set working directory
WORKDIR /app

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run API Gateway
ENTRYPOINT ["/usr/local/bin/genetics-api-gateway"]
