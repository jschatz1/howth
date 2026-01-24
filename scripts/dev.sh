#!/usr/bin/env bash
# Development helper script for fastnode
# Usage: ./scripts/dev.sh [args...]
# Example: ./scripts/dev.sh run index.js

set -euo pipefail

# Set development defaults
export RUST_LOG="${RUST_LOG:-debug}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

exec cargo run -p fastnode-cli -- "$@"
