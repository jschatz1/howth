#!/usr/bin/env bash
# Smoke test script for fastnode
# Usage: ./scripts/smoke.sh

set -euo pipefail

echo "=== Building fastnode ==="
cargo build --workspace

echo ""
echo "=== Running smoke tests ==="

echo "Testing: fastnode --version"
cargo run -p fastnode-cli --quiet -- --version

echo "Testing: fastnode version"
cargo run -p fastnode-cli --quiet -- version

echo "Testing: fastnode --help"
cargo run -p fastnode-cli --quiet -- --help > /dev/null

echo "Testing: fastnode run (placeholder)"
cargo run -p fastnode-cli --quiet -- run test.js

echo "Testing: fastnode install (placeholder)"
cargo run -p fastnode-cli --quiet -- install

echo "Testing: fastnode build (placeholder)"
cargo run -p fastnode-cli --quiet -- build

echo "Testing: fastnode test (placeholder)"
cargo run -p fastnode-cli --quiet -- test

echo "Testing: fastnode --json run (JSON logs)"
cargo run -p fastnode-cli --quiet -- --json run test.js 2>&1 | head -1

echo ""
echo "=== All smoke tests passed ==="
