# Howth Dockerfile
#
# Multi-stage build that compiles Howth from source.
# Produces a minimal runtime image with just the binaries.
#
# Build:
#   docker build -t howth .
#
# Run:
#   docker run -v $(pwd):/app howth run script.js
#
# For multi-platform builds:
#   docker buildx build --platform linux/amd64,linux/arm64 -t howth .

# =============================================================================
# Build stage - compile Howth from source
# =============================================================================
FROM rust:1.75-bookworm AS builder

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binaries
RUN cargo build --release --workspace

# =============================================================================
# Runtime stage - minimal image with just the binaries
# =============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binaries from builder
COPY --from=builder /build/target/release/howth /usr/local/bin/
COPY --from=builder /build/target/release/fastnode /usr/local/bin/

# Set up working directory
WORKDIR /app

# Default command shows version
CMD ["howth", "--version"]
