# howth

<p align="center">
  <img src="assets/earwig.png" alt="howth mascot - an earwig on Howth Castle" width="200">
</p>

<p align="center"><em>A commodius vicus of recirculation for your JavaScript.</em></p>

A complete JavaScript/TypeScript toolchain — runtime, build system, bundler, test runner, package manager, and dev server — written in Rust.

> `howth` is the new name of the project formerly known as `fastnode`.

## What is howth?

howth replaces the patchwork of tools in a typical JS/TS project with a single binary that covers the entire development lifecycle:

- **`howth run`** — Execute TypeScript/JavaScript directly in a native V8 runtime. No `ts-node`, no `tsx`, no subprocess overhead.
- **`howth build`** — Transpile TypeScript via SWC. Content-addressed caching means only changed files rebuild.
- **`howth test`** — Run tests 29x faster than `node --test` and 2.7x faster than `bun test` at scale.
- **`howth bundle`** — Tree shaking, code splitting, CSS bundling, minification. Rollup-compatible plugin system.
- **`howth install`** — Package management with lockfile support, integrity checking, and offline caching.
- **`howth dev`** — Dev server with hot module replacement.

All of these share a single long-running daemon process that keeps compilers, caches, and a warm V8 runtime in memory. The first invocation pays a one-time startup cost; every subsequent command is near-instant because the daemon is already running.

## Why it's fast

The speed comes from architecture, not just using Rust:

1. **Persistent daemon** — A long-running background process holds a warm SWC compiler, build caches, module resolution caches, and a warm V8 runtime in memory. CLI commands connect via Unix domain sockets with length-prefixed JSON frames. The daemon stays alive across invocations, so there's no startup cost after the first run.

2. **Native V8 runtime** — Tests and scripts run directly in an embedded V8 engine (via deno_core), not in a Node.js subprocess. The runtime implements 85% of the Node.js API surface — `http` (backed by hyper), `fs`, `crypto`, `streams`, `child_process`, and more — so most real-world code works without Node.js installed.

3. **In-memory module loading** — Transpiled test files are loaded from a virtual module map in V8's address space. No temp files are written to or read from disk. The module loader checks this in-memory map before touching the filesystem.

4. **In-memory result passing** — Test results are extracted directly from V8's `globalThis` via `eval_to_string()`. No JSON files written to disk, no serialization round-trip through the filesystem.

5. **Parallel SWC transpilation** — All test files are transpiled concurrently using rayon across all available cores. On an 11-core M3 Pro, 500 TypeScript files transpile in the time it takes Node.js to start up.

6. **IPC, not subprocesses** — The CLI talks to the daemon over a Unix domain socket (or named pipe on Windows). A test run is a single IPC round-trip: send file paths, receive results. No process spawning, no pipe setup, no environment inheritance.

## Why "howth"?

The name comes from the opening of James Joyce's *Finnegans Wake*:

> *"riverrun, past Eve and Adam's, from swerve of shore to bend of bay, brings us by a commodius vicus of recirculation back to **Howth Castle and Environs**."*

The circular structure of the Wake — where the final sentence flows back into the first — mirrors the JavaScript event loop: code flows from parse to execute to await and back again, an endless commodius vicus of recirculation.

**HCE** (Howth Castle and Environs) also stands for **Here Comes Everybody** in Joyce's dream-logic, which fits nicely for a toolchain meant for everyone.

The connection runs deeper: deterministic builds are about always arriving back at the same place. Same inputs, same outputs. The riverrun ends where it begins.

## Status

**Active Development** — All core functionality is implemented and working:

- TypeScript transpilation via SWC (1.2ms cold, 0.1ms warm)
- Test runner (29x faster than node, 2.7x faster than bun at 10k tests)
- Package installation and dependency management
- Bundler with tree shaking and code splitting
- Dev server with HMR
- Native V8 runtime (via deno_core) with 85% Node.js API coverage
- Long-running daemon with IPC for persistent caching

## Benchmarks

All benchmarks on Apple M3 Pro (11 cores). Run with `howth bench`.

### Transpile

| Tool | Cold | Warm | Peak RSS |
|------|------|------|----------|
| **howth** | **1.2ms** | **0.1ms** | **8.2MB** |
| tsc --noEmit | 975ms | - | 143MB |

### Test Runner (500 files, 10,000 tests)

| Tool | Median | p95 | Peak RSS |
|------|--------|-----|----------|
| **howth** | **139ms** | **146ms** | - |
| bun test | 368ms | 394ms | 138MB |
| node --test | 4.08s | 6.26s | 138MB |

howth is **29x faster** than node and **2.7x faster** than bun.

### Test Runner (100 files, 1,600 tests)

| Tool | Median | p95 | Peak RSS |
|------|--------|-----|----------|
| **howth** | **33ms** | **37ms** | - |
| bun test | 260ms | 1.51s | 77MB |
| node --test | 816ms | 916ms | 77MB |

howth is **25x faster** than node and **7.8x faster** than bun.

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
# Run all Rust tests
cargo test --workspace

# Run benchmarks
howth bench transpile     # Transpile speed
howth bench test          # Test runner speed (vs node, bun)
howth bench install       # Install speed (vs npm, bun)
howth bench smoke         # Internal micro-benchmarks

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
| `node:crypto` | ✅ | randomBytes, randomUUID, createHash, createCipheriv, sign/verify, RSA |
| `node:http` | ✅ | Client (request/get), Server, Agent, IncomingMessage |
| `node:https` | ✅ | Client (request/get), wraps http with TLS |
| `node:util` | ✅ | format, inspect, promisify, types, deprecate |
| `node:stream` | ✅ | Readable, Writable, Duplex, Transform, pipeline |
| `node:os` | ✅ | platform, arch, cpus, homedir, tmpdir, EOL, constants |
| `node:querystring` | ✅ | parse, stringify, escape, unescape |
| `node:timers` | ✅ | setTimeout, setInterval, setImmediate, promises |
| `node:string_decoder` | ✅ | StringDecoder class for buffer decoding |
| `node:url` | ✅ | parse, format, resolve, pathToFileURL, fileURLToPath |
| `node:punycode` | ✅ | toASCII, toUnicode, ucs2 encode/decode |
| `node:console` | ✅ | Global console exported as module |
| `node:constants` | ✅ | Deprecated constants module |
| `node:perf_hooks` | ✅ | performance.mark/measure, PerformanceObserver |
| `node:tty` | ✅ | isatty, ReadStream, WriteStream |
| `node:v8` | ✅ | Heap statistics, serialize/deserialize |
| `node:domain` | ✅ | Deprecated domain module for error handling |
| `node:async_hooks` | ✅ | AsyncLocalStorage, AsyncResource |
| `node:net` | ✅ | Socket, Server, isIP/isIPv4/isIPv6 |
| `node:zlib` | ✅ | gzip/gunzip/deflate/inflate sync + async + streaming |
| `node:vm` | ✅ | Script, createContext, runInContext/NewContext/ThisContext |
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

**Current Results:**

| Category | Passed | Skipped | Total |
|----------|--------|---------|-------|
| Buffer | 1 | 0 | 1 |
| URL | 2 | 0 | 2 |
| Process | 1 | 0 | 1 |
| Events | 1 | 0 | 1 |
| Util | 1 | 0 | 1 |
| Stream | 1 | 0 | 1 |
| Crypto | 1 | 0 | 1 |
| OS | 1 | 0 | 1 |
| Querystring | 1 | 0 | 1 |
| Timers | 1 | 0 | 1 |
| String decoder | 1 | 0 | 1 |
| Punycode | 1 | 0 | 1 |
| Perf hooks | 1 | 0 | 1 |
| TTY | 1 | 0 | 1 |
| V8 | 1 | 0 | 1 |
| Async hooks | 1 | 0 | 1 |
| HTTP | 2 | 0 | 2 |
| HTTPS | 1 | 0 | 1 |
| Net | 1 | 0 | 1 |
| Path module | 8 | 2 | 10 |
| FS module | 9 | 5 | 14 |
| **Total** | **39** | **7** | **46** |

**Pass Rate: 85%** (39/46)

**Skipped Tests (known limitations):**

| Test | Reason |
|------|--------|
| `test-path-normalize.js` | CVE-2024-36139 Windows path traversal fixes |
| `test-path-join.js` | CVE-2024-36139 Windows path traversal fixes |
| `test-fs-stat.js` | `fstat()` on stdin/stdout not supported |
| `test-fs-mkdir.js` | Requires `worker_threads` module |
| `test-fs-realpath.js` | Requires `worker_threads` module |
| `test-fs-access.js` | Requires `internal/test/binding` |
| `test-fs-copyfile.js` | Requires `internal/test/binding` |

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
  fastnode-cli/      # CLI binary and all commands (~8k LOC)
  fastnode-core/     # Build system, bundler, compiler, resolver, pkg manager (~31k LOC)
  fastnode-daemon/   # Long-running daemon: IPC, file watching, warm worker pool (~6k LOC)
  fastnode-runtime/  # Native V8 runtime via deno_core, Node API shims (~5k LOC)
  fastnode-proto/    # IPC/RPC protocol types and frame encoding (~3k LOC)
  fastnode-util/     # Pure utilities: fs helpers, hashing
  fastnode-compat/   # Node API compatibility layer
  fastnode-bench/    # Benchmark infrastructure
```

~59k lines of Rust across 8 crates.

## Feature Flags

- `native-runtime` - Native V8 runtime via deno_core (recommended for `howth run`)

## Cache/Data Directories

Versioned and namespaced by channel to prevent breakage on format changes:

- Linux: `~/.cache/howth/v1/stable/`
- macOS: `~/Library/Caches/howth/v1/stable/`
- Windows: `%LOCALAPPDATA%\howth\cache\v1\stable\`

## License

MIT
