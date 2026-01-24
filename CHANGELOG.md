# Changelog

All notable changes to this project will be documented in this file.

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
