#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::implicit_clone)]
#![allow(clippy::single_match_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::let_unit_value)]
#![allow(clippy::type_complexity)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_not_else)]

//! Long-running daemon for fastnode.
//!
//! The daemon provides:
//! - Persistent process for fast startup of subsequent commands
//! - File watching and incremental rebuilds
//! - Shared caching across CLI invocations
//!
//! ## IPC Protocol
//! Communication uses length-prefixed JSON frames over Unix domain sockets (Unix)
//! or named pipes (Windows). See `fastnode-proto` for message types.

pub mod cache;
pub mod ipc;
pub mod pkg;
mod server;
pub mod state;
pub mod test_worker;
pub mod v8_test_worker;
pub mod watch;

pub use cache::{DaemonPkgJsonCache, DaemonResolverCache};
pub use server::{run_server, DaemonConfig};
pub use state::DaemonState;
pub use watch::{WatchError, WatcherState};

use crate::cache::DaemonBuildCache;
use fastnode_core::build::{
    build_graph_from_project, execute_graph_with_backend, ExecOptions, BUILD_RUN_SCHEMA_VERSION,
};
use fastnode_core::compiler::CompilerBackend;
use fastnode_core::config::Channel;
use fastnode_core::resolver::{
    resolve_v0, PkgJsonCache, ResolveContext, ResolverCache, ResolverCacheKey, ResolverConfig,
};
use fastnode_core::{build_run_plan, RunPlanInput, RunPlanOutput};
use fastnode_proto::{
    codes, BuildCacheStatus, BuildErrorInfo, BuildNodeResult, BuildRunCounts, BuildRunResult,
    BuildRunSummary, FrameResponse, ImportSpec, Request, ResolvedImport, Response, RunPlan,
    TestCaseResult, TestRunResult, TestStatus, PROTO_SCHEMA_VERSION, TEST_RUN_SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::debug;

/// Handle a request and produce a response (sync version).
///
/// Returns a tuple of (response, `should_shutdown` flag).
/// For async operations (pkg commands), use `handle_request_async` instead.
#[must_use]
pub fn handle_request(
    request: &Request,
    client_proto_version: u32,
    state: Option<&Arc<DaemonState>>,
) -> (Response, bool) {
    // Check protocol version
    if client_proto_version != PROTO_SCHEMA_VERSION {
        return (
            Response::error(
                codes::PROTO_VERSION_MISMATCH,
                format!(
                    "Protocol version mismatch: client={client_proto_version}, server={PROTO_SCHEMA_VERSION}"
                ),
            ),
            false,
        );
    }

    match request {
        Request::Ping { nonce } => (Response::pong(*nonce), false),
        Request::Shutdown => (Response::ShutdownAck, true),
        Request::Run { entry, args, cwd } => {
            let cache = state.map(|s| s.cache.clone());
            let pkg_json_cache = state.map(|s| s.pkg_json_cache.clone());
            (
                handle_run(entry, args, cwd.as_deref(), cache, pkg_json_cache),
                false,
            )
        }
        Request::WatchStart { roots } => {
            let watcher = state.map(|s| s.watcher.clone());
            (handle_watch_start(roots, watcher.as_ref()), false)
        }
        Request::WatchStop => {
            let watcher = state.map(|s| s.watcher.clone());
            (handle_watch_stop(watcher.as_ref()), false)
        }
        Request::WatchStatus => {
            let watcher = state.map(|s| s.watcher.clone());
            (handle_watch_status(watcher.as_ref()), false)
        }
        // PkgGraph can be handled sync (no network I/O)
        Request::PkgGraph {
            cwd,
            include_dev_root,
            include_optional,
            max_depth,
            ..
        } => {
            let pkg_json_cache = state.map(|s| s.pkg_json_cache.clone());
            (
                handle_pkg_graph(
                    cwd,
                    *include_dev_root,
                    *include_optional,
                    *max_depth,
                    pkg_json_cache.as_ref(),
                ),
                false,
            )
        }
        // PkgExplain can be handled sync (no network I/O)
        Request::PkgExplain {
            specifier,
            cwd,
            parent,
            channel,
            kind,
        } => {
            let pkg_json_cache = state.map(|s| s.pkg_json_cache.clone());
            (
                handle_pkg_explain(
                    specifier,
                    cwd,
                    parent,
                    channel,
                    kind,
                    pkg_json_cache.as_ref(),
                ),
                false,
            )
        }
        // PkgWhy can be handled sync (no network I/O)
        Request::PkgWhy {
            arg,
            cwd,
            include_dev_root,
            include_optional,
            max_depth,
            max_chains,
            include_trace,
            trace_kind,
            trace_parent,
            ..
        } => {
            let pkg_json_cache = state.map(|s| s.pkg_json_cache.clone());
            let opts = pkg::WhyRequestOptions {
                arg,
                cwd,
                include_dev_root: *include_dev_root,
                include_optional: *include_optional,
                max_depth: *max_depth,
                max_chains: *max_chains,
                include_trace: *include_trace,
                trace_kind: trace_kind.as_deref(),
                trace_parent: trace_parent.as_deref(),
            };
            (handle_pkg_why(opts, pkg_json_cache.as_ref()), false)
        }
        // PkgDoctor can be handled sync (no network I/O)
        Request::PkgDoctor {
            cwd,
            include_dev_root,
            include_optional,
            max_depth,
            min_severity,
            max_items,
            ..
        } => {
            let pkg_json_cache = state.map(|s| s.pkg_json_cache.clone());
            let opts = pkg::DoctorRequestOptions {
                cwd,
                include_dev_root: *include_dev_root,
                include_optional: *include_optional,
                max_depth: *max_depth,
                min_severity,
                max_items: *max_items,
            };
            (handle_pkg_doctor(opts, pkg_json_cache.as_ref()), false)
        }
        // Build request (v2.0, targets v2.1, transpile v3.1)
        Request::Build {
            cwd,
            force,
            dry_run,
            max_parallel,
            profile,
            targets,
        } => {
            let build_cache = state.map(|s| s.build_cache.clone());
            let compiler = state.map(|s| s.compiler.clone());
            (
                handle_build(
                    cwd,
                    *force,
                    *dry_run,
                    *max_parallel,
                    *profile,
                    targets,
                    build_cache,
                    compiler,
                ),
                false,
            )
        }
        // WatchBuild requires streaming handler (v3.0)
        Request::WatchBuild { .. } => (
            Response::error(
                codes::INTERNAL_ERROR,
                "WatchBuild requires streaming handler",
            ),
            false,
        ),
        // RunTests needs async handler (tokio mutex + worker I/O)
        Request::RunTests { .. } => (
            Response::error(
                codes::INTERNAL_ERROR,
                "RunTests requires async handler",
            ),
            false,
        ),
        // Pkg operations that need async - return error if called sync
        Request::PkgAdd { .. }
        | Request::PkgRemove { .. }
        | Request::PkgUpdate { .. }
        | Request::PkgOutdated { .. }
        | Request::PkgPublish { .. }
        | Request::PkgCacheList { .. }
        | Request::PkgCachePrune { .. }
        | Request::PkgInstall { .. } => (
            Response::error(
                codes::INTERNAL_ERROR,
                "Package operations require async handler",
            ),
            false,
        ),
    }
}

/// Handle a request asynchronously.
///
/// Use this for requests that may require network I/O (pkg operations)
/// or test worker access.
/// Returns a tuple of (response, `should_shutdown` flag).
pub async fn handle_request_async(
    request: &Request,
    client_proto_version: u32,
    _state: Option<&Arc<DaemonState>>,
) -> (Response, bool) {
    // Check protocol version
    if client_proto_version != PROTO_SCHEMA_VERSION {
        return (
            Response::error(
                codes::PROTO_VERSION_MISMATCH,
                format!(
                    "Protocol version mismatch: client={client_proto_version}, server={PROTO_SCHEMA_VERSION}"
                ),
            ),
            false,
        );
    }

    match request {
        Request::PkgAdd {
            specs,
            cwd,
            channel,
            save_dev,
        } => (pkg::handle_pkg_add(specs, cwd, channel, *save_dev).await, false),
        Request::PkgRemove {
            packages,
            cwd,
            channel,
        } => (pkg::handle_pkg_remove(packages, cwd, channel).await, false),
        Request::PkgUpdate {
            packages,
            cwd,
            channel,
            latest,
        } => (pkg::handle_pkg_update(packages, cwd, channel, *latest).await, false),
        Request::PkgCacheList { channel } => (pkg::handle_pkg_cache_list(channel), false),
        Request::PkgCachePrune { channel } => (pkg::handle_pkg_cache_prune(channel), false),
        Request::PkgInstall {
            cwd,
            channel,
            frozen,
            include_dev,
            include_optional,
        } => (
            pkg::handle_pkg_install(cwd, channel, *frozen, *include_dev, *include_optional).await,
            false,
        ),
        Request::PkgOutdated { cwd, channel } => {
            (pkg::handle_pkg_outdated(cwd, channel).await, false)
        }
        Request::PkgPublish {
            cwd,
            registry,
            token,
            dry_run,
            tag,
            access,
        } => (
            pkg::handle_pkg_publish(
                cwd,
                registry.as_deref(),
                token.as_deref(),
                *dry_run,
                tag.as_deref(),
                access.as_deref(),
            )
            .await,
            false,
        ),
        Request::RunTests { cwd, files } => (
            handle_run_tests(cwd, files, _state).await,
            false,
        ),
        // Non-async operations - should not reach here, but handle gracefully
        _ => (
            Response::error(
                codes::INTERNAL_ERROR,
                "Use sync handler for non-pkg operations",
            ),
            false,
        ),
    }
}

/// Handle a `WatchStart` request.
fn handle_watch_start(roots: &[String], watcher: Option<&Arc<WatcherState>>) -> Response {
    let Some(watcher) = watcher else {
        return Response::error(codes::WATCH_UNSUPPORTED, "File watcher is not enabled");
    };

    match watcher.start(roots.to_vec()) {
        Ok(()) => Response::WatchStarted {
            roots: roots.to_vec(),
        },
        Err(WatchError::AlreadyRunning) => {
            Response::error(codes::WATCH_ALREADY_RUNNING, "Watcher is already running")
        }
        Err(WatchError::InvalidRoot(root)) => {
            Response::error(codes::WATCH_INVALID_ROOT, format!("Invalid root: {root}"))
        }
        Err(WatchError::WatcherFailed(msg)) => Response::error(codes::INTERNAL_ERROR, msg),
        Err(WatchError::NotRunning) => {
            Response::error(codes::INTERNAL_ERROR, "Unexpected: watcher not running")
        }
    }
}

/// Handle a `WatchStop` request.
fn handle_watch_stop(watcher: Option<&Arc<WatcherState>>) -> Response {
    let Some(watcher) = watcher else {
        return Response::error(codes::WATCH_UNSUPPORTED, "File watcher is not enabled");
    };

    match watcher.stop() {
        Ok(()) => Response::WatchStopped,
        Err(WatchError::NotRunning) => {
            Response::error(codes::WATCH_NOT_RUNNING, "Watcher is not running")
        }
        Err(e) => Response::error(codes::INTERNAL_ERROR, e.to_string()),
    }
}

/// Handle a `PkgGraph` request.
fn handle_pkg_graph(
    cwd: &str,
    include_dev_root: bool,
    include_optional: bool,
    max_depth: u32,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
) -> Response {
    use fastnode_core::resolver::NoPkgJsonCache;

    // Use the daemon's pkg_json_cache if available, otherwise use a no-op cache
    let no_cache = NoPkgJsonCache;
    let cache_ref: &dyn PkgJsonCache =
        pkg_json_cache.map_or(&no_cache as &dyn PkgJsonCache, |c| c.as_ref());

    pkg::handle_pkg_graph(
        cwd,
        include_dev_root,
        include_optional,
        max_depth,
        cache_ref,
    )
}

/// Handle a `PkgExplain` request.
fn handle_pkg_explain(
    specifier: &str,
    cwd: &str,
    parent: &str,
    channel: &str,
    kind: &str,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
) -> Response {
    use fastnode_core::resolver::NoPkgJsonCache;

    // Use the daemon's pkg_json_cache if available, otherwise use a no-op cache
    let no_cache = NoPkgJsonCache;
    let cache_ref: &dyn PkgJsonCache =
        pkg_json_cache.map_or(&no_cache as &dyn PkgJsonCache, |c| c.as_ref());

    pkg::handle_pkg_explain(specifier, cwd, parent, channel, kind, cache_ref)
}

/// Handle a `PkgWhy` request.
fn handle_pkg_why(
    opts: pkg::WhyRequestOptions<'_>,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
) -> Response {
    use fastnode_core::resolver::NoPkgJsonCache;

    // Use the daemon's pkg_json_cache if available, otherwise use a no-op cache
    let no_cache = NoPkgJsonCache;
    let cache_ref: &dyn PkgJsonCache =
        pkg_json_cache.map_or(&no_cache as &dyn PkgJsonCache, |c| c.as_ref());

    pkg::handle_pkg_why(opts, cache_ref)
}

/// Handle a `PkgDoctor` request.
fn handle_pkg_doctor(
    opts: pkg::DoctorRequestOptions<'_>,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
) -> Response {
    use fastnode_core::resolver::NoPkgJsonCache;

    // Use the daemon's pkg_json_cache if available, otherwise use a no-op cache
    let no_cache = NoPkgJsonCache;
    let cache_ref: &dyn PkgJsonCache =
        pkg_json_cache.map_or(&no_cache as &dyn PkgJsonCache, |c| c.as_ref());

    pkg::handle_pkg_doctor(opts, cache_ref)
}

/// Handle a `WatchStatus` request.
fn handle_watch_status(watcher: Option<&Arc<WatcherState>>) -> Response {
    let Some(watcher) = watcher else {
        return Response::WatchStatus {
            roots: Vec::new(),
            running: false,
            last_event_unix_ms: None,
        };
    };

    Response::WatchStatus {
        roots: watcher.roots(),
        running: watcher.is_running(),
        last_event_unix_ms: watcher.last_event_unix_ms(),
    }
}

/// Handle a `Build` request (v2.0, targets v2.1).
fn handle_build(
    cwd: &str,
    force: bool,
    dry_run: bool,
    max_parallel: u32,
    _profile: bool,
    targets: &[String],
    build_cache: Option<Arc<DaemonBuildCache>>,
    compiler: Option<Arc<dyn CompilerBackend>>,
) -> Response {
    // Validate cwd
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.exists() {
        return Response::error(
            codes::BUILD_CWD_INVALID,
            format!("Working directory does not exist: {cwd}"),
        );
    }
    if !cwd_path.is_dir() {
        return Response::error(
            codes::BUILD_CWD_INVALID,
            format!("Working directory is not a directory: {cwd}"),
        );
    }

    // Build the graph from package.json
    let graph = match build_graph_from_project(&cwd_path) {
        Ok(g) => g,
        Err(e) => {
            return Response::error(e.code, e.message);
        }
    };

    // Determine targets to build (v2.1)
    let effective_targets: Vec<String> = if targets.is_empty() {
        // Use defaults from graph
        if graph.defaults.is_empty() {
            return Response::error(
                codes::BUILD_NO_DEFAULT_TARGETS,
                "No targets specified and no default targets in graph",
            );
        }
        graph.defaults.clone()
    } else {
        targets.to_vec()
    };

    // Plan the build (v2.1)
    let plan = match graph.plan_targets(&effective_targets) {
        Ok(p) => p,
        Err(invalid_target) => {
            return Response::error(
                codes::BUILD_TARGET_INVALID,
                format!("Invalid target: {invalid_target}"),
            );
        }
    };

    // Set up execution options
    let options = ExecOptions {
        force,
        dry_run,
        max_parallel: max_parallel as usize,
        profile: false,      // TODO: wire up profiling
        targets: Vec::new(), // Empty = run all nodes
    };

    // Create a wrapper cache that implements BuildCache trait
    // and delegates to the DaemonBuildCache
    let mut wrapper_cache: Option<BuildCacheWrapper> =
        build_cache.as_ref().map(|c| BuildCacheWrapper(c.clone()));

    // Get the compiler backend reference for transpile nodes (v3.1)
    let backend_ref: Option<&dyn CompilerBackend> = compiler.as_ref().map(|c| c.as_ref());

    // Execute only the planned nodes (filtered by targets)
    // TODO: Use plan.nodes for filtered execution
    // For now, execute the full graph but set requested_targets
    let result = match wrapper_cache.as_mut() {
        Some(cache) => execute_graph_with_backend(&graph, Some(cache), &options, backend_ref),
        None => execute_graph_with_backend(&graph, None, &options, backend_ref),
    };

    match result {
        Ok(mut run_result) => {
            // Set the requested targets (v2.1)
            run_result.set_targets(plan.requested_targets);

            // Register file dependencies with the build cache for invalidation
            if let Some(ref cache) = build_cache {
                for node in &graph.nodes {
                    for input in &node.inputs {
                        if let fastnode_core::build::BuildInput::File { path, .. } = input {
                            cache.add_file_dependency(&node.id, std::path::Path::new(path));
                        }
                    }
                }
            }

            // Convert to protocol types
            Response::BuildResult {
                result: convert_build_result(run_result, cwd),
            }
        }
        Err(e) => Response::error(codes::BUILD_HASH_IO_ERROR, e.to_string()),
    }
}

/// Wrapper to implement BuildCache trait for DaemonBuildCache.
struct BuildCacheWrapper(Arc<DaemonBuildCache>);

impl fastnode_core::build::BuildCache for BuildCacheWrapper {
    fn get(&self, node_id: &str, hash: &str) -> Option<bool> {
        self.0.get(node_id, hash)
    }

    fn get_entry(&self, node_id: &str, hash: &str) -> Option<fastnode_core::build::CacheEntry> {
        self.0.get_entry(node_id, hash)
    }

    fn set(&mut self, node_id: &str, hash: &str, ok: bool) {
        self.0.set(node_id, hash, ok);
    }

    fn set_with_fingerprint(
        &mut self,
        node_id: &str,
        hash: &str,
        ok: bool,
        fingerprint: Option<fastnode_core::build::OutputFingerprint>,
    ) {
        self.0.set_with_fingerprint(node_id, hash, ok, fingerprint);
    }

    fn invalidate(&mut self, node_id: &str) {
        // The DaemonBuildCache doesn't have a direct invalidate by node_id
        // It uses path-based invalidation instead
        let _ = node_id;
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// Convert core's `BuildRunResult` to proto's `BuildRunResult`.
fn convert_build_result(result: fastnode_core::build::BuildRunResult, cwd: &str) -> BuildRunResult {
    let results: Vec<BuildNodeResult> = result
        .results
        .into_iter()
        .map(|r| BuildNodeResult {
            id: r.id,
            ok: r.ok,
            cache: match r.cache {
                fastnode_core::build::CacheStatus::Hit => BuildCacheStatus::Hit,
                fastnode_core::build::CacheStatus::Miss => BuildCacheStatus::Miss,
                fastnode_core::build::CacheStatus::Bypass => BuildCacheStatus::Bypass,
                fastnode_core::build::CacheStatus::Skipped => BuildCacheStatus::Skipped,
            },
            hash: r.hash,
            duration_ms: r.duration_ms,
            reason: r.reason.map(|reason| match reason {
                fastnode_core::build::BuildNodeReason::CacheHit => {
                    fastnode_proto::BuildNodeReason::CacheHit
                }
                fastnode_core::build::BuildNodeReason::Forced => {
                    fastnode_proto::BuildNodeReason::Forced
                }
                fastnode_core::build::BuildNodeReason::InputChanged => {
                    fastnode_proto::BuildNodeReason::InputChanged
                }
                fastnode_core::build::BuildNodeReason::DepChanged => {
                    fastnode_proto::BuildNodeReason::DepChanged
                }
                fastnode_core::build::BuildNodeReason::DepFailed => {
                    fastnode_proto::BuildNodeReason::DepFailed
                }
                fastnode_core::build::BuildNodeReason::FirstBuild => {
                    fastnode_proto::BuildNodeReason::FirstBuild
                }
                fastnode_core::build::BuildNodeReason::OutputsChanged => {
                    fastnode_proto::BuildNodeReason::OutputsChanged
                }
            }),
            error: r.error.map(|e| BuildErrorInfo {
                code: e.code.to_string(),
                message: e.message,
                detail: e.detail,
            }),
            stdout_truncated: r.stdout_truncated,
            stderr_truncated: r.stderr_truncated,
            notes: r.notes,
            files_count: r.files_count,
            auto_discovered: r.auto_discovered,
        })
        .collect();

    // Map from core's summary structure to proto's flatter structure
    let summary = &result.summary;
    let succeeded = summary.nodes_run - summary.counts.error;
    let failed = summary.counts.error;

    BuildRunResult {
        schema_version: BUILD_RUN_SCHEMA_VERSION,
        cwd: cwd.to_string(),
        ok: result.ok,
        counts: BuildRunCounts {
            total: summary.nodes_total,
            succeeded,
            failed,
            skipped: summary.nodes_skipped,
            cache_hits: summary.cache_hits,
            executed: summary.nodes_run,
        },
        summary: BuildRunSummary {
            total_duration_ms: summary.duration_ms,
            saved_duration_ms: 0, // TODO: track saved time
        },
        results,
        notes: result.notes,
    }
}

/// Handle a `RunTests` request.
///
/// Transpiles test files via the daemon's warm SWC compiler, then sends
/// the transpiled code to the warm Node.js test worker.
async fn handle_run_tests(
    cwd: &str,
    files: &[String],
    state: Option<&Arc<DaemonState>>,
) -> Response {
    use crate::test_worker::TranspiledTestFile;
    use fastnode_core::compiler::TranspileSpec;

    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.exists() || !cwd_path.is_dir() {
        return Response::error(
            codes::TEST_CWD_INVALID,
            format!("Invalid working directory: {cwd}"),
        );
    }

    if files.is_empty() {
        return Response::error(codes::TEST_NO_FILES, "No test files provided");
    }

    let Some(state) = state else {
        return Response::error(codes::INTERNAL_ERROR, "Daemon state not available");
    };

    // Transpile each file using the daemon's warm SWC compiler
    let compiler = &state.compiler;
    let mut transpiled = Vec::with_capacity(files.len());

    for file_path in files {
        let path = PathBuf::from(file_path);

        // Check if file needs transpilation
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let needs_transpile = matches!(
            ext.to_lowercase().as_str(),
            "ts" | "tsx" | "jsx" | "mts" | "cts"
        );

        if needs_transpile {
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    return Response::error(
                        codes::TEST_TRANSPILE_FAILED,
                        format!("Failed to read {file_path}: {e}"),
                    );
                }
            };

            // Create a dummy output path for TranspileSpec
            let out_path = path.with_extension("mjs");
            let spec = TranspileSpec::new(&path, &out_path);

            match compiler.transpile(&spec, &source) {
                Ok(output) => {
                    transpiled.push(TranspiledTestFile {
                        path: file_path.clone(),
                        code: output.code,
                    });
                }
                Err(e) => {
                    return Response::error(
                        codes::TEST_TRANSPILE_FAILED,
                        format!("Failed to transpile {file_path}: {e}"),
                    );
                }
            }
        } else {
            // JS files — read and send directly
            let source = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    return Response::error(
                        codes::TEST_TRANSPILE_FAILED,
                        format!("Failed to read {file_path}: {e}"),
                    );
                }
            };
            transpiled.push(TranspiledTestFile {
                path: file_path.clone(),
                code: source,
            });
        }
    }

    // Try native V8 test worker first, fall back to Node.js worker
    let v8_result = try_v8_test_worker(state, &transpiled);

    let result = match v8_result {
        Ok(result) => result,
        Err(v8_err) => {
            debug!("V8 test worker failed ({v8_err}), falling back to Node.js worker");
            // Fallback to Node.js worker
            match run_tests_node_worker(state, transpiled).await {
                Ok(r) => r,
                Err(e) => {
                    let code = if e.kind() == std::io::ErrorKind::TimedOut {
                        codes::TEST_WORKER_TIMEOUT
                    } else {
                        codes::TEST_WORKER_FAILED
                    };
                    return Response::error(code, format!("Test worker error: {e}"));
                }
            }
        }
    };

    worker_response_to_response(cwd, result)
}

/// Convert a WorkerResponse into a daemon Response.
fn worker_response_to_response(cwd: &str, result: crate::test_worker::WorkerResponse) -> Response {
    let tests: Vec<TestCaseResult> = result
        .tests
        .into_iter()
        .map(|t| TestCaseResult {
            name: t.name,
            file: t.file,
            status: match t.status.as_str() {
                "pass" => TestStatus::Pass,
                "fail" => TestStatus::Fail,
                _ => TestStatus::Skip,
            },
            duration_ms: t.duration_ms,
            error: t.error,
        })
        .collect();

    Response::TestRunResult {
        result: TestRunResult {
            schema_version: TEST_RUN_SCHEMA_VERSION,
            cwd: cwd.to_string(),
            ok: result.ok,
            total: result.total,
            passed: result.passed,
            failed: result.failed,
            skipped: result.skipped,
            duration_ms: result.duration_ms,
            tests,
            diagnostics: result.diagnostics,
        },
    }
}

/// Try running tests via the native V8 test worker.
fn try_v8_test_worker(
    state: &Arc<DaemonState>,
    files: &[crate::test_worker::TranspiledTestFile],
) -> Result<crate::test_worker::WorkerResponse, std::io::Error> {
    let mut guard = state.v8_test_worker.lock().map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::Other, "V8 worker mutex poisoned")
    })?;

    if guard.is_none() {
        *guard = Some(crate::v8_test_worker::V8TestWorker::spawn()?);
    }

    let worker = guard.as_ref().unwrap();
    let id = format!("v8-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis());

    worker.run_tests(id, files.to_vec())
}

/// Run tests via the Node.js test worker (fallback path).
async fn run_tests_node_worker(
    state: &Arc<DaemonState>,
    files: Vec<crate::test_worker::TranspiledTestFile>,
) -> Result<crate::test_worker::WorkerResponse, std::io::Error> {
    let mut worker_guard = state.test_worker.lock().await;
    if worker_guard.is_none() {
        *worker_guard = Some(crate::test_worker::NodeTestWorker::spawn().await?);
    }

    let worker = worker_guard.as_mut().unwrap();
    match worker.run_tests(files).await {
        Ok(result) => Ok(result),
        Err(e) => {
            *worker_guard = None;
            Err(e)
        }
    }
}

/// Handle a Run request.
fn handle_run(
    entry: &str,
    args: &[String],
    cwd: Option<&str>,
    cache: Option<Arc<DaemonResolverCache>>,
    pkg_json_cache: Option<Arc<DaemonPkgJsonCache>>,
) -> Response {
    // Parse cwd
    let cwd_path = match cwd {
        Some(c) => PathBuf::from(c),
        None => match std::env::current_dir() {
            Ok(p) => p,
            Err(e) => {
                return Response::error(
                    codes::CWD_INVALID,
                    format!("Failed to determine working directory: {e}"),
                );
            }
        },
    };

    // Build the plan
    let input = RunPlanInput {
        cwd: cwd_path,
        entry: PathBuf::from(entry),
        args: args.to_vec(),
        channel: Channel::Stable, // TODO: make configurable
    };

    match build_run_plan(input.clone()) {
        Ok(mut plan) => {
            // Re-resolve imports using daemon cache if available
            if let Some(ref cache) = cache {
                if let Some(ref resolved_entry) = plan.resolved_entry {
                    let entry_path = PathBuf::from(resolved_entry);
                    resolve_imports_with_cache(
                        &mut plan,
                        &entry_path,
                        &input.cwd,
                        cache,
                        pkg_json_cache.as_ref(),
                    );
                }
            }
            Response::RunPlan {
                plan: Box::new(convert_to_proto_plan(plan)),
            }
        }
        Err(e) => Response::error(e.code(), e.to_string()),
    }
}

/// Re-resolve imports using the daemon cache.
fn resolve_imports_with_cache(
    plan: &mut RunPlanOutput,
    entry_path: &Path,
    cwd: &Path,
    cache: &Arc<DaemonResolverCache>,
    pkg_json_cache: Option<&Arc<DaemonPkgJsonCache>>,
) {
    use fastnode_core::runplan::ResolvedImportOutput;

    let entry_dir = entry_path.parent().unwrap_or(cwd);
    let config = ResolverConfig::default();

    // Get pkg_json_cache as trait object if available
    let pkg_json_cache_ref: Option<&dyn PkgJsonCache> =
        pkg_json_cache.map(|c| c.as_ref() as &dyn PkgJsonCache);

    let ctx = ResolveContext {
        cwd: cwd.to_path_buf(),
        parent: entry_dir.to_path_buf(),
        channel: plan.channel.clone(),
        config: &config,
        pkg_json_cache: pkg_json_cache_ref,
    };

    // Re-resolve each import using cache
    plan.resolved_imports = plan
        .imports
        .iter()
        .map(|import| {
            let key = ResolverCacheKey {
                cwd: cwd.to_string_lossy().into_owned(),
                parent: entry_dir.to_string_lossy().into_owned(),
                specifier: import.raw.clone(),
                channel: plan.channel.clone(),
            };

            // Check cache first
            if let Some(cached) = cache.get(&key) {
                return ResolvedImportOutput {
                    raw: import.raw.clone(),
                    resolved: cached.resolved,
                    status: cached.status,
                    reason: cached.reason,
                    from_cache: true,
                    tried: cached.tried,
                };
            }

            // Cache miss - resolve and store
            let result = resolve_v0(&ctx, &import.raw);

            // Store in cache
            cache.put(key, &result);

            // Convert to output
            let tried: Vec<String> = result
                .tried
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();

            match result.resolved {
                Some(path) => ResolvedImportOutput {
                    raw: import.raw.clone(),
                    resolved: Some(path.to_string_lossy().into_owned()),
                    status: "resolved".to_string(),
                    reason: None,
                    from_cache: false,
                    tried,
                },
                None => ResolvedImportOutput {
                    raw: import.raw.clone(),
                    resolved: None,
                    status: "unresolved".to_string(),
                    reason: result.reason.map(|r| r.to_string()),
                    from_cache: false,
                    tried,
                },
            }
        })
        .collect();
}

/// Convert core's `RunPlanOutput` to proto's `RunPlan`.
fn convert_to_proto_plan(output: RunPlanOutput) -> RunPlan {
    let mut plan = RunPlan::new(
        output.resolved_cwd,
        output.requested_entry,
        output.resolved_entry,
        output.entry_kind,
        output.args,
        output.channel,
        output.notes,
    );

    // Convert imports
    let imports: Vec<ImportSpec> = output
        .imports
        .into_iter()
        .map(|i| ImportSpec::new(i.raw, i.kind, i.line))
        .collect();

    // Convert resolved imports
    let resolved_imports: Vec<ResolvedImport> = output
        .resolved_imports
        .into_iter()
        .map(|r| {
            if let Some(resolved) = r.resolved {
                ResolvedImport::resolved(r.raw, resolved, r.from_cache).with_tried(r.tried)
            } else {
                ResolvedImport::unresolved(r.raw, r.reason.unwrap_or_default(), r.from_cache)
                    .with_tried(r.tried)
            }
        })
        .collect();

    plan = plan.with_imports(imports, resolved_imports);
    plan
}

/// Create a response frame.
#[must_use]
pub fn make_response_frame(response: Response) -> FrameResponse {
    FrameResponse::new(fastnode_core::VERSION, response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_handle_ping() {
        let (resp, shutdown) =
            handle_request(&Request::Ping { nonce: 42 }, PROTO_SCHEMA_VERSION, None);

        assert!(!shutdown);
        match resp {
            Response::Pong { nonce, .. } => assert_eq!(nonce, 42),
            _ => panic!("Expected Pong"),
        }
    }

    #[test]
    fn test_handle_shutdown() {
        let (resp, shutdown) = handle_request(&Request::Shutdown, PROTO_SCHEMA_VERSION, None);

        assert!(shutdown);
        assert!(matches!(resp, Response::ShutdownAck));
    }

    #[test]
    fn test_proto_version_mismatch() {
        let (resp, shutdown) = handle_request(&Request::Ping { nonce: 1 }, 999, None);

        assert!(!shutdown);
        match resp {
            Response::Error { code, .. } => {
                assert_eq!(code, codes::PROTO_VERSION_MISMATCH);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_handle_run_success() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        std::fs::write(&entry_path, "// test").unwrap();

        let (resp, shutdown) = handle_request(
            &Request::Run {
                entry: "main.js".to_string(),
                args: vec!["--flag".to_string()],
                cwd: Some(dir.path().to_string_lossy().into_owned()),
            },
            PROTO_SCHEMA_VERSION,
            None,
        );

        assert!(!shutdown);
        match resp {
            Response::RunPlan { plan } => {
                assert_eq!(plan.schema_version, 2);
                assert_eq!(plan.requested_entry, "main.js");
                assert!(plan.resolved_entry.is_some());
                assert_eq!(plan.entry_kind, "file");
                assert_eq!(plan.args, vec!["--flag"]);
            }
            _ => panic!("Expected RunPlan"),
        }
    }

    #[test]
    fn test_handle_run_with_imports() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        let dep_path = dir.path().join("dep.js");

        std::fs::write(&entry_path, r#"import "./dep.js";"#).unwrap();
        std::fs::write(&dep_path, "export const x = 1;").unwrap();

        let state = Arc::new(DaemonState::new());

        let (resp, shutdown) = handle_request(
            &Request::Run {
                entry: "main.js".to_string(),
                args: vec![],
                cwd: Some(dir.path().to_string_lossy().into_owned()),
            },
            PROTO_SCHEMA_VERSION,
            Some(&state),
        );

        assert!(!shutdown);
        match resp {
            Response::RunPlan { plan } => {
                assert_eq!(plan.imports.len(), 1);
                assert_eq!(plan.imports[0].raw, "./dep.js");
                assert_eq!(plan.resolved_imports.len(), 1);
                assert_eq!(plan.resolved_imports[0].status, "resolved");
                assert!(!plan.resolved_imports[0].from_cache); // First run
            }
            _ => panic!("Expected RunPlan"),
        }

        // Second run should be from cache
        let (resp2, _) = handle_request(
            &Request::Run {
                entry: "main.js".to_string(),
                args: vec![],
                cwd: Some(dir.path().to_string_lossy().into_owned()),
            },
            PROTO_SCHEMA_VERSION,
            Some(&state),
        );

        match resp2 {
            Response::RunPlan { plan } => {
                assert_eq!(plan.resolved_imports.len(), 1);
                assert!(plan.resolved_imports[0].from_cache); // Second run - cached
            }
            _ => panic!("Expected RunPlan"),
        }
    }

    #[test]
    fn test_handle_run_missing_entry() {
        let dir = tempdir().unwrap();

        let (resp, shutdown) = handle_request(
            &Request::Run {
                entry: "nonexistent.js".to_string(),
                args: vec![],
                cwd: Some(dir.path().to_string_lossy().into_owned()),
            },
            PROTO_SCHEMA_VERSION,
            None,
        );

        assert!(!shutdown);
        match resp {
            Response::Error { code, .. } => {
                assert_eq!(code, codes::ENTRY_NOT_FOUND);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_handle_run_invalid_cwd() {
        let (resp, shutdown) = handle_request(
            &Request::Run {
                entry: "main.js".to_string(),
                args: vec![],
                cwd: Some("/nonexistent/path/that/does/not/exist".to_string()),
            },
            PROTO_SCHEMA_VERSION,
            None,
        );

        assert!(!shutdown);
        match resp {
            Response::Error { code, .. } => {
                assert_eq!(code, codes::CWD_INVALID);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_watch_status_without_state() {
        let (resp, shutdown) = handle_request(&Request::WatchStatus, PROTO_SCHEMA_VERSION, None);

        assert!(!shutdown);
        match resp {
            Response::WatchStatus {
                roots,
                running,
                last_event_unix_ms,
            } => {
                assert!(roots.is_empty());
                assert!(!running);
                assert!(last_event_unix_ms.is_none());
            }
            _ => panic!("Expected WatchStatus"),
        }
    }

    #[test]
    fn test_watch_start_without_state() {
        let (resp, shutdown) = handle_request(
            &Request::WatchStart {
                roots: vec!["/tmp".to_string()],
            },
            PROTO_SCHEMA_VERSION,
            None,
        );

        assert!(!shutdown);
        match resp {
            Response::Error { code, .. } => {
                assert_eq!(code, codes::WATCH_UNSUPPORTED);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_watch_stop_without_state() {
        let (resp, shutdown) = handle_request(&Request::WatchStop, PROTO_SCHEMA_VERSION, None);

        assert!(!shutdown);
        match resp {
            Response::Error { code, .. } => {
                assert_eq!(code, codes::WATCH_UNSUPPORTED);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_watch_status_with_state() {
        let state = Arc::new(DaemonState::new());

        let (resp, shutdown) =
            handle_request(&Request::WatchStatus, PROTO_SCHEMA_VERSION, Some(&state));

        assert!(!shutdown);
        match resp {
            Response::WatchStatus {
                roots,
                running,
                last_event_unix_ms,
            } => {
                assert!(roots.is_empty());
                assert!(!running);
                assert!(last_event_unix_ms.is_none());
            }
            _ => panic!("Expected WatchStatus"),
        }
    }
}
