# Changelog

All notable changes to this project will be documented in this file.

## v0.2.0 — Fastest JavaScript Bundler

### Bundler Performance
- **JSX fast path**: `.jsx` files skip SWC entirely, using howth-parser for single-pass parse + JSX transform + import extraction
- **Parallel resolver**: import resolution runs inside rayon parallel closures with `RwLock<HashMap>` cache
- **Directory listing cache**: eliminates per-extension `stat()` calls with a single directory read cached as `HashSet` lookups
- **Replaced SWC minifier**: switched to `swc_ecma_minifier` with proper compress/mangle passes for correct minification
- **Scope hoisting**: top-level declarations hoisted across modules for smaller output

### Benchmark Results (apps/10000 — 19,014 modules)
- **GCP c3-highcpu-8 (Linux x64)**: 275ms — 1.12x faster than Bun (307ms), 2.1x faster than esbuild
- **macOS (Apple Silicon)**: 276ms — 1.27x faster than Bun (350ms), 2.6x faster than esbuild

### Other
- Dev server and runtime improvements
- Worker, atomics, and vite-compat tests
- New examples (cookies, data-pipeline, markdown-api, parallel-compute, real-time-game)

---

## v1.8.0 — Rename to howth

### Changed
- Renamed CLI from `fastnode` to `howth`
- `fastnode` remains as a temporary alias for backward compatibility (will be removed in v2.0)
- Updated all user-facing paths: `~/.cache/howth/`, `~/.local/share/howth/`, `howth.sock`
- Updated environment variable from `FASTNODE_IPC_ENDPOINT` to `HOWTH_IPC_ENDPOINT`

### Migration
- Replace `fastnode` with `howth` in scripts and commands
- The `fastnode` command will print a deprecation notice and delegate to `howth`

---

## v1.7.1 — Package Doctor (deterministic health report)

Added `fastnode pkg doctor`, a read-only diagnostic command for installed `node_modules/`.

### Highlights
- Deterministic findings ordering and deterministic truncation (`--max-items`)
- Severity filtering (`--severity`) affects both findings and summary
- Locked JSON output shape with schema versioning (JSON-only with `--json`)
- Hardened JSON: `notes` is always present (even when empty)
- No network, no installs, no subprocesses; daemon-routed and cache-friendly

### CLI
```
fastnode pkg doctor [--cwd <path>] [--dev] [--no-optional] [--max-depth <N>] [--max-items <N>] [--severity <info|warn|error>] [--format <summary|list>] [--json]
```

### Finding codes
- `PKG_DOCTOR_NODE_MODULES_NOT_FOUND`
- `PKG_DOCTOR_GRAPH_ERROR`
- `PKG_DOCTOR_ORPHAN_PACKAGE`
- `PKG_DOCTOR_MISSING_EDGE_TARGET`
- `PKG_DOCTOR_INVALID_PACKAGE_JSON`
- `PKG_DOCTOR_DUPLICATE_PACKAGE_VERSION`
- `PKG_DOCTOR_MAX_ITEMS_REACHED`

---

## v1.7.0 — Package Doctor (initial)

Initial implementation of `fastnode pkg doctor` with core detection algorithms.

---

## v1.6.0 — Package Why

Added `fastnode pkg why` to explain dependency chains.

---

## v1.5.0 — Package Explain

Added `fastnode pkg explain` for resolver tracing.

---

## v1.4.0 — Package Graph

Added `fastnode pkg graph` to build and display dependency graphs.

---

## v1.3.0 — Package Add --deps

Added `fastnode pkg add --deps` to install from package.json.

---

## v1.2.0 — Resolver exports subpaths

Added support for package.json exports subpaths and patterns.

---

## v1.1.0 — Resolver exports/imports

Added support for package.json exports and imports fields.

---

## v1.0.0 — Package Resolution

Initial package resolution: registry, cache, node_modules linking.

---

## v0.1.0 — Initial skeleton

Project structure, daemon IPC, basic CLI.
