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
| Dev Server (Vite Compat) | In Progress | Vite-compatible unbundled dev server |

### Dev Server — Vite Compatibility

Unbundled ES module serving with HMR, React Fast Refresh, and a Vite-compatible plugin system.

**Done:**
- [x] Unbundled module serving (per-request resolve → load → SWC transpile → transform → rewrite)
- [x] Import rewriting (bare specifiers → `/@modules/`, relative → absolute, CSS → `/@style/`)
- [x] Dependency pre-bundling (scan entry, bundle each dep into `.howth/deps/`)
- [x] HMR client API (`import.meta.hot` with accept, dispose, invalidate, data, on/send)
- [x] HMR module graph with boundary detection
- [x] React Fast Refresh plugin (component detection, preamble/footer injection, `/@react-refresh` virtual module)
- [x] Plugin enforce ordering (Pre/Normal/Post)
- [x] Vite plugin hooks (`config`, `configResolved`, `configureServer`, `transformIndexHtml`, `handleHotUpdate`)
- [x] CSS-as-JS modules with HMR (inject/remove `<style>` tags)
- [x] WebSocket HMR on `/__hmr` with error overlay
- [x] File watching with debounce and cache invalidation
- [x] Module transform caching

**Critical (P1):**
- [ ] Wire up HMR module graph edges (`update_module_imports()` never called — breaks boundary detection)
- [ ] Support user-provided `index.html` (currently always generates synthetic HTML)
- [ ] Config file support (`howth.config.ts`)
- [ ] JavaScript/TypeScript plugin loading (Rust-only plugins blocks ecosystem compat)
- [ ] SPA history API fallback (404 on refresh for client-side routes)

**High (P2):**
- [ ] CSS Modules (`.module.css`)
- [ ] PostCSS integration (Tailwind, autoprefixer)
- [ ] `.env` file loading and `import.meta.env`
- [ ] Dev server proxy configuration (`/api/*` → backend)
- [ ] CSS preprocessors (Sass/Less/Stylus)
- [ ] `package.json` `browser` field resolution
- [ ] tsconfig `paths` resolution

**Medium (P3):**
- [ ] Rich error overlay with code frame and click-to-open
- [ ] CORS headers
- [ ] Wire up plugin middleware to Axum router (registered but never invoked)
- [ ] `import.meta.glob()` support
- [ ] Asset query suffixes (`?raw`, `?url`, `?inline`)
- [ ] `public/` directory support
- [ ] CJS-to-ESM conversion in pre-bundling
- [ ] Dependency cache invalidation and force re-optimization
- [ ] HTTPS/TLS support
- [ ] SSR support (`transformRequest`, `ssrLoadModule`)
- [ ] Library build mode (`build.lib`)
- [ ] Web Worker support (`?worker`, `new Worker`)
- [ ] `base` path wiring (field exists but unused)
- [ ] `import.meta.hot.acceptExports()` partial accept
- [ ] Multi-page app build support
- [ ] Rollup hooks: `generateBundle`, `writeBundle`, `moduleParsed`, `closeBundle`
- [ ] CSS `url()` rewriting and `@import` resolution
- [ ] `package.json` exports wildcard/pattern matching

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
