# Smoke test script for fastnode (Windows)
# Usage: .\scripts\smoke.ps1

$ErrorActionPreference = "Stop"

Write-Host "=== Building fastnode ===" -ForegroundColor Cyan
cargo build --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "=== Running smoke tests ===" -ForegroundColor Cyan

Write-Host "Testing: fastnode --version"
cargo run -p fastnode-cli --quiet -- --version
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode version"
cargo run -p fastnode-cli --quiet -- version
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode --help"
cargo run -p fastnode-cli --quiet -- --help | Out-Null
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode run (placeholder)"
cargo run -p fastnode-cli --quiet -- run test.js
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode install (placeholder)"
cargo run -p fastnode-cli --quiet -- install
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode build (placeholder)"
cargo run -p fastnode-cli --quiet -- build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Testing: fastnode test (placeholder)"
cargo run -p fastnode-cli --quiet -- test
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "=== All smoke tests passed ===" -ForegroundColor Green
