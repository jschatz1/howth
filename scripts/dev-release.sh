#!/usr/bin/env bash
# Build, install, and restart the howth daemon.
# Usage: ./scripts/dev-release.sh

set -euo pipefail

echo "Building and installing howth..."
cargo install --path crates/fastnode-cli

echo "Stopping daemon..."
pkill -f 'howth daemon' || true

echo "Starting daemon..."
howth daemon &
disown

echo "Done."
