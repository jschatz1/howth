#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="${FIXTURES_DIR:-/tmp/howth-ts-fixtures}"
CUSTOM_DIR="$ROOT_DIR/tests/ts-compat/custom"
LIMIT="${1:-1000}"

echo "=== Howth TypeScript Compatibility Test ==="
echo "Fixtures: $FIXTURES_DIR"
echo "Custom:   $CUSTOM_DIR"
echo "Limit:    $LIMIT files"
echo ""

mkdir -p "$FIXTURES_DIR"

# --- Clone repositories ---

# TypeScript compiler test cases (thousands of focused .ts files)
if [ ! -d "$FIXTURES_DIR/typescript/.git" ]; then
    echo "[clone] microsoft/TypeScript (sparse: tests/cases/compiler) ..."
    git clone --depth 1 --filter=blob:none --sparse \
        https://github.com/microsoft/TypeScript.git "$FIXTURES_DIR/typescript" 2>/dev/null
    (cd "$FIXTURES_DIR/typescript" && git sparse-checkout set tests/cases/compiler 2>/dev/null)
    echo "[clone] TypeScript: done"
else
    echo "[clone] TypeScript: cached"
fi

# Vue 3 core (real-world framework code)
if [ ! -d "$FIXTURES_DIR/vue/.git" ]; then
    echo "[clone] vuejs/core (sparse: packages) ..."
    git clone --depth 1 --filter=blob:none --sparse \
        https://github.com/vuejs/core.git "$FIXTURES_DIR/vue" 2>/dev/null
    (cd "$FIXTURES_DIR/vue" && git sparse-checkout set packages 2>/dev/null)
    echo "[clone] Vue: done"
else
    echo "[clone] Vue: cached"
fi

# Deno standard library (idiomatic TypeScript)
if [ ! -d "$FIXTURES_DIR/deno-std/.git" ]; then
    echo "[clone] denoland/deno_std ..."
    git clone --depth 1 https://github.com/denoland/deno_std.git "$FIXTURES_DIR/deno-std" 2>/dev/null
    echo "[clone] Deno std: done"
else
    echo "[clone] Deno std: cached"
fi

echo ""

# --- Build ---
echo "Building parse_ts_dir (release)..."
cargo build --example parse_ts_dir -p howth-parser --features full --release 2>&1 | tail -3
echo ""

# --- Run ---
echo "Parsing up to $LIMIT TypeScript files..."
echo ""

cargo run --example parse_ts_dir -p howth-parser --features full --release -- \
    --limit "$LIMIT" \
    --codegen \
    "$CUSTOM_DIR" \
    "$FIXTURES_DIR/typescript/tests/cases/compiler" \
    "$FIXTURES_DIR/vue/packages" \
    "$FIXTURES_DIR/deno-std"
