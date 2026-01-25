# howth

A deterministic Node toolchain inspector and runtime foundation.

> `howth` is the new name of the project formerly known as `fastnode`.

## Status

**Skeleton only** - This is the initial project structure. No runtime or package manager functionality is implemented yet.

## Vision

A multi-year project to build a Bun-class (and eventually faster-than-Bun) JavaScript/TypeScript toolchain and runtime:

- **Phase 1**: Toolchain around Node (package manager, test runner, bundler)
- **Phase 2**: Runtime experiments + Node compatibility harness
- **Phase 3**: Eliminate fallbacks + performance optimization

## Building

```bash
# Build all crates
cargo build --workspace

# Build release
cargo build --release --workspace

# Run the CLI
cargo run -p fastnode-cli --bin howth -- version
cargo run -p fastnode-cli --bin howth -- --help
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run benchmarks
cargo bench -p fastnode-bench

# Smoke tests
./scripts/smoke.sh        # Unix
.\scripts\smoke.ps1       # Windows
```

## CLI Usage

```bash
# Show version
howth version
howth --version

# Check system health and capabilities
howth doctor
howth --json doctor  # Machine-readable output

# Run a file (not implemented yet)
howth run script.js

# Install dependencies (not implemented yet)
howth install

# Build project (not implemented yet)
howth build

# Run tests (not implemented yet)
howth test

# Global flags
howth -v run script.js       # DEBUG logging
howth -vv run script.js      # TRACE logging
howth --json run script.js   # Stable JSON log output
howth --cwd /path run script.js  # Override working directory
```

## Doctor Command

`howth doctor` checks system health and capabilities:

- **Runtime**: Version, schema version, channel
- **OS**: Name, version, architecture
- **Hardware**: CPU cores
- **Paths**: Cache/data directories and write permissions
- **Project**: Root detection (package.json, .git)
- **Capabilities**: Case sensitivity, symlinks, hardlinks, file limits

### Warning Codes (Stable)

| Code | Severity | Description |
|------|----------|-------------|
| `CACHE_NOT_WRITABLE` | warn | Cache directory not writable |
| `DATA_NOT_WRITABLE` | warn | Data directory not writable |
| `LOW_NOFILE_LIMIT` | warn | File descriptor limit too low |
| `PROJECT_ROOT_NOT_FOUND` | info | No package.json or .git found |
| `FS_CASE_INSENSITIVE` | info | Filesystem is case-insensitive |
| `SYMLINK_UNAVAILABLE` | warn | Symlinks not supported |
| `HARDLINK_UNAVAILABLE` | warn | Hardlinks not supported |
| `UNKNOWN_OS_VERSION` | info | Could not determine OS version |

### JSON Output

With `--json`, outputs a single JSON object to stdout (no logs mixed in):

```json
{
  "runtime": {"version": "0.1.0", "schema_version": 1, "channel": "stable"},
  "os": {"name": "macos", "version": "15.3.1", "arch": "aarch64"},
  "hardware": {"cpu_cores": 11},
  "paths": {...},
  "project": {...},
  "capabilities": {...},
  "warnings": [{"code": "...", "severity": "info|warn", "message": "..."}]
}
```

## Package Doctor

`howth pkg doctor` is a **read-only** dependency health report for an existing `node_modules/` tree.
It performs no installs, no network access, and no subprocess execution. Output is deterministic.

### Usage

Summary (human):
```bash
howth pkg doctor --cwd .
```

List (human):
```bash
howth pkg doctor --format list
```

Filter to warnings+errors only:
```bash
howth pkg doctor --severity warn
```

JSON (machine-readable):
```bash
howth pkg doctor --json
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--cwd <path>` | Working directory | current directory |
| `--dev` | Include root `devDependencies` | false |
| `--no-optional` | Exclude `optionalDependencies` | false |
| `--max-depth <N>` | Traversal depth | 25 |
| `--max-items <N>` | Maximum findings returned | 200 |
| `--severity <level>` | `info\|warn\|error` | info |
| `--format <fmt>` | `summary\|list` | summary |
| `--json` | Emit JSON only (no extra stdout) | false |

### Findings

Doctor reports issues such as:

| Code | Severity | Description |
|------|----------|-------------|
| `PKG_DOCTOR_NODE_MODULES_NOT_FOUND` | error | `node_modules/` directory not found |
| `PKG_DOCTOR_GRAPH_ERROR` | warn | Graph scan error |
| `PKG_DOCTOR_ORPHAN_PACKAGE` | warn | Installed but not reachable from root |
| `PKG_DOCTOR_MISSING_EDGE_TARGET` | warn | Declared but not installed |
| `PKG_DOCTOR_INVALID_PACKAGE_JSON` | warn | Invalid or missing package.json |
| `PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION` | info/warn | Multiple versions installed |
| `PKG_DOCTOR_MAX_ITEMS_REACHED` | info | Output truncated |

### JSON Output Contract

With `--json`, stdout contains exactly one JSON object:

```json
{
  "ok": true,
  "doctor": {
    "schema_version": 1,
    "cwd": "...",
    "summary": {
      "severity": "warn",
      "counts": {"info": 0, "warn": 2, "error": 0},
      "packages_indexed": 42,
      "reachable_packages": 40,
      "orphans": 1,
      "missing_edges": 1,
      "invalid_packages": 0
    },
    "findings": [
      {"code": "PKG_DOCTOR_ORPHAN_PACKAGE", "severity": "warn", "message": "...", "package": "..."}
    ],
    "notes": []
  }
}
```

**Contract notes:**
- `doctor.notes` is always present (even if empty)
- Finding objects always include `{code, severity, message}` and may include `{package, path, detail, related}`
- Output ordering is deterministic; truncation appends a final `PKG_DOCTOR_MAX_ITEMS_REACHED` finding

## Trust Guarantees

howth is designed to be predictable and non-surprising:

### JSON Output Contract

- **`--json` produces exactly one JSON object** to stdout. No logs, no progress output, no extra lines.
- This is a **stable contract** - parsers can rely on `stdout | jq` always working.
- `--watch --json` is currently disallowed because watch mode streams multiple results.
  - Future: `--json-stream` will emit newline-delimited JSON objects.

### No Surprise Network

- howth **never makes network calls** except during explicit install commands.
- `howth build`, `howth doctor`, `howth pkg doctor` are fully offline.
- `npx --no-install` is used where possible to fail fast if dependencies are missing (instead of fetching).

### Deterministic Output

- All commands produce **deterministic output** for the same inputs.
- Findings, warnings, and results are sorted by stable keys.
- Timestamps and random values are never included in hashes or output.

## Project Structure

```
crates/
  fastnode-cli/      # CLI binary (owns logging)
  fastnode-core/     # Core types: errors, config, paths, versioning
  fastnode-util/     # Pure utilities: fs helpers, hashing
  fastnode-proto/    # IPC/RPC protocol types
  fastnode-daemon/   # Long-running daemon (placeholder)
  fastnode-compat/   # Node API compatibility layer (placeholder)
  fastnode-bench/    # Benchmarks (criterion)

scripts/
  smoke.sh           # Unix smoke tests
  smoke.ps1          # Windows smoke tests
  dev.sh             # Development runner with defaults
  bench-install.sh   # Placeholder for hyperfine benchmarks
```

## Feature Flags

Defined at workspace level for future use:

- `engine-v8` - V8 JavaScript engine
- `engine-sm` - SpiderMonkey engine
- `engine-jsc` - JavaScriptCore engine
- `daemon` - Daemon mode
- `pm` - Package manager
- `bundler` - Bundler
- `test-runner` - Test runner

## Cache/Data Directories

Versioned and namespaced by channel to prevent breakage on format changes:

- Linux: `~/.cache/howth/v1/stable/`
- macOS: `~/Library/Caches/howth/v1/stable/`
- Windows: `%LOCALAPPDATA%\howth\cache\v1\stable\`

## License

MIT
