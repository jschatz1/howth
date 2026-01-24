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
| TypeScript Transpiler | Planned | Fast TS → JS transpilation (swc or oxc) |
| Bundler | Planned | ESM bundling for production |
| Source Maps | Planned | Source map generation and consumption |

---

## Phase 5 — Test Runner

Built-in test runner with discovery, execution, and coverage.

| Project | Status | Description |
|---------|--------|-------------|
| Test Discovery | Planned | Find test files by pattern |
| Test Runner | Planned | Execute tests with reporting |
| Coverage | Planned | Code coverage collection and reporting |

---

## Phase 6 — JS Runtime

JavaScript engine integration and Node API compatibility.

| Project | Status | Description |
|---------|--------|-------------|
| V8 Integration | Planned | Embed V8 as primary engine |
| ESM Loader | Planned | Native ESM module loading |
| Node APIs | Planned | fs, path, crypto, etc. compatibility |
| Native Addons | Planned | N-API / node-gyp support |

---

## Phase 7 — Performance

Optimization and benchmarking for production readiness.

| Project | Status | Description |
|---------|--------|-------------|
| Startup Time | Planned | Sub-10ms cold start target |
| Memory Footprint | Planned | Minimize RSS and heap usage |
| Benchmark Suite | Planned | Comparative benchmarks vs Node/Bun/Deno |

---

## Phase 8 — Compatibility

Ecosystem compatibility and framework support.

| Project | Status | Description |
|---------|--------|-------------|
| Node Compatibility | Planned | Node.js API parity testing |
| npm Ecosystem | Planned | Top-1000 package compatibility |
| Framework Support | Planned | Next.js, Remix, Astro, etc. |

---

## Phase 9 — Release

Distribution, documentation, and launch preparation.

| Project | Status | Description |
|---------|--------|-------------|
| CI/CD Pipeline | In Progress | GitHub Actions for test/build/release |
| Distribution | Planned | Binary releases, npm package, installers |
| Documentation | In Progress | README, API docs, guides |
| Launch Checklist | Planned | Pre-launch verification |

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
