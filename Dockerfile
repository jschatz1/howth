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
FROM rust:1.85-bookworm AS builder

WORKDIR /build

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binaries with native V8 runtime
RUN cargo build --release --workspace --features native-runtime

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

# Install Node.js for playground server
RUN apt-get update && \
    apt-get install -y --no-install-recommends nodejs npm && \
    rm -rf /var/lib/apt/lists/*

# Set up working directory
WORKDIR /app

# Copy examples for playground
COPY examples/playground ./examples/playground

# Set howth as the entrypoint
ENTRYPOINT ["howth"]

# Default command shows version
CMD ["--version"]
