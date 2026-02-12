# Howth CI Image
#
# Pre-built image for CI pipelines (GitLab CI, GitHub Actions, etc.)
# Includes Node.js 20, howth, and pnpm — ready to run tests immediately.
#
# Usage in GitLab CI:
#   image: ghcr.io/jschatz1/howth/ci:latest
#   script:
#     - pnpm install
#     - howth test --exit
#
# Usage in GitHub Actions:
#   container:
#     image: ghcr.io/jschatz1/howth/ci:latest
#
# Tags: :latest, :0.5.17, :0.5

FROM node:20-bookworm-slim

# Install common CI dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      git \
      ca-certificates \
      curl \
      python3 \
      make \
      g++ \
    && rm -rf /var/lib/apt/lists/*

# Install pnpm
RUN npm install -g pnpm@9

# Install howth from npm (uses the postinstall script to download the right binary)
ARG HOWTH_VERSION=latest
RUN npm install -g howth@${HOWTH_VERSION}

# Verify installations
RUN node --version && npm --version && pnpm --version && howth --version

WORKDIR /app
