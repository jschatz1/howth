# howth

<p align="center">
  <img src="assets/earwig.png" alt="howth mascot - an earwig on Howth Castle" width="200">
</p>

<p align="center"><em>A commodius vicus of recirculation for your JavaScript.</em></p>

A deterministic Node toolchain inspector and runtime foundation.

> `howth` is the new name of the project formerly known as `fastnode`.

## Why "howth"?

The name comes from the opening of James Joyce's *Finnegans Wake*:

> *"riverrun, past Eve and Adam's, from swerve of shore to bend of bay, brings us by a commodius vicus of recirculation back to **Howth Castle and Environs**."*

The circular structure of the Wake — where the final sentence flows back into the first — mirrors the JavaScript event loop: code flows from parse to execute to await and back again, an endless commodius vicus of recirculation.

**HCE** (Howth Castle and Environs) also stands for **Here Comes Everybody** in Joyce's dream-logic, which fits nicely for a toolchain meant for everyone.

The connection runs deeper: deterministic builds are about always arriving back at the same place. Same inputs, same outputs. The riverrun ends where it begins.

## Status

**Active Development** - Core toolchain functionality is implemented:

- Native V8 runtime (via deno_core) with Web API support
- ES module loading with TypeScript transpilation
- Package installation and dependency management
- Bundler with tree shaking
- Test runner
- Dev server with HMR

## Vision

A multi-year project to build a Bun-class (and eventually faster-than-Bun) JavaScript/TypeScript toolchain and runtime:

- **Phase 1**: Toolchain around Node (package manager, test runner, bundler) - **Complete**
- **Phase 2**: Runtime experiments + Node compatibility harness - **In Progress**
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

## Native Runtime

When built with `--features native-runtime`, howth uses a native V8 runtime (via deno_core) instead of spawning Node.js. This provides faster startup and tighter integration.

```bash
# Build with native runtime
cargo build --features native-runtime -p fastnode-cli

# Run uses native V8 by default
howth run script.ts

# Fall back to Node.js if needed
howth run --node script.ts
```

### Web API Coverage

| API | Status | Notes |
|-----|--------|-------|
| `console.log/error/warn/info/debug` | ✅ | Full support |
| `setTimeout`, `setInterval` | ✅ | Full support |
| `clearTimeout`, `clearInterval` | ✅ | Full support |
| `queueMicrotask` | ✅ | Full support |
| `fetch` | ✅ | Full HTTP client via reqwest |
| `Request`, `Response`, `Headers` | ✅ | Full support |
| `URL`, `URLSearchParams` | ✅ | Full support |
| `TextEncoder`, `TextDecoder` | ✅ | UTF-8 only |
| `atob`, `btoa` | ✅ | Base64 encode/decode |
| `crypto.getRandomValues()` | ✅ | Full support |
| `crypto.randomUUID()` | ✅ | Full support |
| `crypto.subtle.digest()` | ✅ | SHA-1, SHA-256, SHA-384, SHA-512, MD5 |
| `AbortController`, `AbortSignal` | ✅ | Full support |
| `Event`, `EventTarget` | ✅ | Basic implementation |
| `DOMException` | ✅ | Full support |
| `Blob` | ✅ | Full support |
| `File` | ✅ | Full support |
| `FormData` | ✅ | Full support |
| `ReadableStream` | ✅ | Basic implementation |
| `WritableStream` | ✅ | Basic implementation |
| `TransformStream` | ✅ | Basic implementation |
| `performance.now()` | ✅ | Full support |
| `structuredClone` | ✅ | Via JSON (limited) |

### Node.js API Coverage

| API | Status | Notes |
|-----|--------|-------|
| `process.env` | ✅ | Get and set |
| `process.cwd()` | ✅ | Full support |
| `process.exit()` | ✅ | Full support |
| `process.argv` | ✅ | Full support |
| `process.platform` | ✅ | Full support |
| `process.version` | ✅ | Reports v20.0.0 |
| `process.hrtime.bigint()` | ✅ | Full support |
| `process.nextTick()` | ✅ | Via queueMicrotask |
| `Buffer` | ✅ | Full support (alloc, from, concat, fill, encoding, read/write) |
| `URL` / `URLSearchParams` | ✅ | Full support |
| `node:fs` | ✅ | Sync, async, and promises API |
| `node:path` | ✅ | Full support (join, resolve, dirname, basename, etc.) |
| `node:events` | ✅ | EventEmitter with full API |
| `node:assert` | ✅ | Full assertion support |
| `node:child_process` | ✅ | execSync, spawnSync, exec, spawn |
| `node:module` | ✅ | createRequire, builtinModules |
| `node:crypto` | ✅ | randomBytes, randomUUID, createHash (MD5), timingSafeEqual |
| `node:http` | ❌ | Not yet implemented |
| `node:https` | ❌ | Not yet implemented |
| `node:util` | ✅ | format, inspect, promisify, types, deprecate |
| `node:stream` | ✅ | Readable, Writable, Duplex, Transform, pipeline |
| `node:os` | ✅ | platform, arch, cpus, homedir, tmpdir, EOL, constants |
| `node:querystring` | ✅ | parse, stringify, escape, unescape |
| `node:timers` | ✅ | setTimeout, setInterval, setImmediate, promises |
| `node:worker_threads` | ❌ | Not yet implemented |
| `require()` | ✅ | Full CommonJS support |

### Node.js Compatibility Testing

howth includes a Node.js compatibility test suite that validates behavior against official Node.js tests.

**Running the tests:**

```bash
# Build with native runtime
cargo build --features native-runtime

# Run compatibility tests (requires Node.js for test runner)
HOWTH_BIN=$(pwd)/target/debug/howth node tests/node_compat/run-tests.js
```

**Current Results (as of January 2025):**

| Category | Passed | Skipped | Total |
|----------|--------|---------|-------|
| Buffer | 1 | 0 | 1 |
| URL | 1 | 0 | 1 |
| Process | 1 | 0 | 1 |
| Events | 1 | 0 | 1 |
| Util | 1 | 0 | 1 |
| Stream | 1 | 0 | 1 |
| Crypto | 1 | 0 | 1 |
| OS | 1 | 0 | 1 |
| Querystring | 1 | 0 | 1 |
| Timers | 1 | 0 | 1 |
| Path module | 8 | 2 | 10 |
| FS module | 9 | 4 | 13 |
| **Total** | **27** | **6** | **33** |

**Pass Rate: 82%**

**Passing Tests:**
- `test-buffer-basic.js` - Buffer operations (alloc, from, concat, fill, encoding)
- `test-url-basic.js` - URL and URLSearchParams
- `test-process-basic.js` - process object (env, cwd, argv, events)
- `test-events-basic.js` - EventEmitter
- `test-util-basic.js` - util module (format, inspect, promisify, types)
- `test-stream-basic.js` - stream module (Readable, Writable, Transform, pipeline)
- `test-crypto-basic.js` - crypto module (randomBytes, randomUUID, MD5, timingSafeEqual)
- `test-path.js` - Main path module tests
- `test-path-parse-format.js` - `path.parse()` and `path.format()`
- `test-path-dirname.js` - `path.dirname()`
- `test-path-basename.js` - `path.basename()`
- `test-path-extname.js` - `path.extname()`
- `test-path-relative.js` - `path.relative()`
- `test-path-resolve.js` - `path.resolve()` (uses child_process)
- `test-path-isabsolute.js` - `path.isAbsolute()`
- `test-fs-exists.js` - `fs.exists()` and `fs.existsSync()`
- `test-fs-readdir.js` - `fs.readdir()` and `fs.readdirSync()`
- `test-fs-readfile.js` - `fs.readFile()` and `fs.readFileSync()`
- `test-fs-writefile.js` - `fs.writeFile()` and `fs.writeFileSync()`
- `test-fs-appendfile.js` - `fs.appendFile()` and `fs.appendFileSync()`
- `test-fs-rename.js` - `fs.rename()` and `fs.renameSync()`
- `test-fs-unlink.js` - `fs.unlink()` and `fs.unlinkSync()`
- `test-fs-mkdir-basic.js` - `fs.mkdir()` and `fs.mkdirSync()`
- `test-fs-copyfile-basic.js` - `fs.copyFile()` and `fs.copyFileSync()`
- `test-os-basic.js` - OS module (platform, arch, cpus, homedir, tmpdir)
- `test-querystring-basic.js` - querystring module (parse, stringify)
- `test-timers-basic.js` - timers module (setTimeout, clearTimeout, promises)

**Skipped Tests (known limitations):**
| Test | Reason |
|------|--------|
| `test-path-normalize.js` | CVE-2024-36139 Windows path traversal fixes |
| `test-path-join.js` | CVE-2024-36139 Windows path traversal fixes |
| `test-fs-stat.js` | `fstat()` on stdin/stdout not supported |
| `test-fs-mkdir.js` | Requires `worker_threads` module |
| `test-fs-realpath.js` | Requires `worker_threads` module |
| `test-fs-access.js` | Requires `internal/test/binding` |

### ES Module Support

- ✅ ES module imports (`import`/`export`)
- ✅ TypeScript transpilation on-the-fly
- ✅ Extension-less imports (auto-resolves `.ts`, `.js`, etc.)
- ✅ Index file resolution (`./dir` → `./dir/index.ts`)
- ✅ Bare specifiers (`import lodash from 'lodash'`) - resolves from node_modules
- ✅ Scoped packages (`import x from '@scope/pkg'`)
- ✅ Subpath imports (`import fp from 'lodash/fp'`)
- ✅ Package.json `exports` field (conditional exports)
- ✅ CommonJS (`require()`, `module.exports`, `exports`)
- ✅ JSON imports via require
- ✅ `__dirname` and `__filename`
- ✅ Module caching

## CLI Usage

```bash
# Show version
howth version
howth --version

# Check system health and capabilities
howth doctor
howth --json doctor  # Machine-readable output

# Run a JavaScript/TypeScript file (native V8 runtime by default)
howth run script.ts
howth run script.js
howth run --node script.ts   # Fall back to Node.js subprocess

# Install dependencies
howth install
howth install --frozen-lockfile  # CI mode

# Bundle modules
howth bundle src/index.ts -o dist/bundle.js
howth bundle src/index.ts --minify --sourcemap

# Build project
howth build
howth build --watch          # Watch mode

# Run tests
howth test

# Start dev server
howth dev src/index.ts --port 3000

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

Defined at workspace level:

- `native-runtime` - Native V8 runtime via deno_core (recommended)
- `engine-v8` - V8 JavaScript engine (placeholder)
- `engine-sm` - SpiderMonkey engine (placeholder)
- `engine-jsc` - JavaScriptCore engine (placeholder)
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
