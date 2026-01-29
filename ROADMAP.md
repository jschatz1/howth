# howth Roadmap

A multi-year project to build a Bun-class (and eventually faster-than-Bun) JavaScript/TypeScript toolchain and runtime.

## Overview

The roadmap is organized into 10 phases, from foundation to release. Each phase builds on the previous, with the goal of creating a complete, production-ready Node-compatible toolchain.

---

## Phase 0 — Foundation

Core infrastructure and developer experience basics.

| Project | Status | Description |
|---------|--------|-------------|
| CLI Skeleton | Done | Basic CLI structure with clap, logging, global flags |
| Doctor Command | Done | System health checks (`howth doctor`) |
| Bench Smoke | Done | Internal micro-benchmarks for hot paths |

---

## Phase 1 — Daemon

Long-running daemon for caching, watching, and IPC.

| Project | Status | Description |
|---------|--------|-------------|
| Daemon Lifecycle | Done | Start/stop, graceful shutdown, PID management |
| Daemon State Model | Done | Shared state, resolver cache, pkg cache |
| IPC Maturity | Done | Unix sockets, Windows named pipes, frame protocol |

**Completed milestones:**
- [x] Daemon-routed `howth run` (RunPlan)
- [x] Resolver v0 + Daemon Cache + RunPlan Schema v2

---

## Phase 2 — Filesystem

File watching and metadata caching for incremental operations.

| Project | Status | Description |
|---------|--------|-------------|
| File Watcher | Done | notify-based watcher with debouncing |
| Metadata Cache | Done | package.json cache with invalidation |

**Completed milestones:**
- [x] File Watcher v0 for daemon
- [x] Cache invalidation on file changes

---

## Phase 3 — Package Manager

npm-compatible package resolution, installation, and caching.

| Project | Status | Description |
|---------|--------|-------------|
| Package Resolution | Done | Registry client, version resolution, tarball extraction |
| Global Cache | Done | Immutable cache at `~/.cache/howth/` |
| Install Command | In Progress | `howth install` / `howth pkg add` |
| Lockfile | Planned | Deterministic lockfile generation |

**Completed milestones:**
- [x] Package Resolution v1 (Registry + Cache + node_modules)
- [x] Resolver v1.1 - package.json exports + imports
- [x] Resolver v1.2 - exports subpaths + patterns
- [x] Package Resolution v1.3 - `howth pkg add --deps`
- [x] Package Graph v1.4 - dependency graph building
- [x] Package Why v1.5 - `howth pkg explain --why`
- [x] Package Explain v1.6 - resolver tracing
- [x] Package Doctor v1.7 - health diagnostics
- [x] Package Doctor v1.7.1 - determinism locks, UX polish

**Next:**
- [ ] Lockfile v1 - deterministic lock generation
- [ ] `howth install` from lockfile
- [ ] Workspace support

---

## Phase 4 — Build Toolchain

TypeScript transpilation, bundling, and source maps.

| Project | Status | Description |
|---------|--------|-------------|
| TypeScript Transpiler | Done | SWC-based TS → JS (1.2ms cold, 0.1ms warm) |
| Bundler | Done | ESM bundling, tree shaking, code splitting, CSS, plugins |
| Source Maps | Done | Source map generation |
| Watch Mode | Done | File watching with transpile-only default |

---

## Phase 5 — Test Runner

Built-in test runner with discovery, execution, and coverage.

| Project | Status | Description |
|---------|--------|-------------|
| Test Discovery | Done | Find test files by `*.test.*` / `*.spec.*` pattern |
| Test Runner | Done | Warm Node worker pool, 8.7x faster than node --test |
| Daemon Integration | Done | SWC transpile + IPC to warm worker |
| Coverage | Planned | Code coverage collection and reporting |

---

## Phase 6 — JS Runtime

JavaScript engine integration and Node API compatibility.

| Project | Status | Description |
|---------|--------|-------------|
| V8 Integration | Done | Native V8 via deno_core |
| ESM Loader | Done | ES modules + CommonJS + TypeScript |
| Node APIs | Done | 85% module coverage (34/40 modules) |
| Web APIs | Done | fetch, URL, crypto.subtle, streams, etc. |
| Native Addons | Planned | N-API / node-gyp support |

---

## Phase 7 — Performance

Optimization and benchmarking for production readiness.

| Project | Status | Description |
|---------|--------|-------------|
| Startup Time | Done | 1.2ms cold transpile, 19ms test run |
| Memory Footprint | Done | 8.2MB peak RSS (transpile), vs 143MB for tsc |
| Benchmark Suite | Done | `howth bench` — transpile, test, install, smoke, devloop |

---

## Phase 8 — Compatibility

Ecosystem compatibility and framework support.

| Project | Status | Description |
|---------|--------|-------------|
| Node Compatibility | Done | 45 compat tests, 85% pass rate |
| npm Ecosystem | In Progress | Package install, linking, graph, doctor |
| Framework Support | In Progress | Examples for React, Next.js, Remix, SvelteKit, Express |

---

## Phase 9 — Release

Distribution, documentation, and launch preparation.

| Project | Status | Description |
|---------|--------|-------------|
| CI/CD Pipeline | Done | GitHub Actions for test/build/release |
| Distribution | Done | Binary releases for macOS/Linux/Windows |
| Documentation | Done | README, site (howth.run), API docs, guides |
| Launch Checklist | In Progress | Pre-launch verification |

---

## Version History

| Version | Milestone |
|---------|-----------|
| v1.8.0 | Rename to `howth` |
| v1.7.1 | Package Doctor determinism locks |
| v1.7.0 | Package Doctor |
| v1.6.0 | Package Why / Explain |
| v1.5.0 | Package Explain |
| v1.4.0 | Package Graph |
| v1.3.0 | Package Add --deps |
| v1.2.0 | Resolver exports subpaths |
| v1.1.0 | Resolver exports/imports |
| v1.0.0 | Package Resolution |
| v0.1.0 | Initial skeleton |

---

## Contributing

See individual project boards in 1medium for detailed task tracking. Each phase has its own space with granular tasks and priorities.

## License

MIT
