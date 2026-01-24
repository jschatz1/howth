#!/usr/bin/env bash
# Benchmark package installation (placeholder for hyperfine)
# Usage: ./scripts/bench-install.sh

set -euo pipefail

echo "=== Benchmark: fastnode install ==="
echo "This script will use hyperfine for benchmarking once implemented."
echo ""

# Check if hyperfine is installed
if command -v hyperfine &> /dev/null; then
    echo "hyperfine found, ready for benchmarking."
    echo ""
    echo "Example usage (when install is implemented):"
    echo "  hyperfine --warmup 3 'fastnode install' 'npm install' 'bun install'"
else
    echo "hyperfine not found. Install with:"
    echo "  brew install hyperfine  # macOS"
    echo "  cargo install hyperfine # any platform"
fi
