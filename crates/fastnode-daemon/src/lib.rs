#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

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
pub mod watch;

pub use cache::{DaemonPkgJsonCache, DaemonResolverCache};
pub use server::{run_server, DaemonConfig};
pub use state::DaemonState;
pub use watch::{WatchError, WatcherState};

use fastnode_core::config::Channel;
use fastnode_core::resolver::{
    resolve_v0, PkgJsonCache, ResolveContext, ResolverCache, ResolverCacheKey, ResolverConfig,
};
use fastnode_core::{build_run_plan, RunPlanInput, RunPlanOutput};
use fastnode_proto::{
    codes, FrameResponse, ImportSpec, Request, ResolvedImport, Response, RunPlan,
    PROTO_SCHEMA_VERSION,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
        // Pkg operations that need async - return error if called sync
        Request::PkgAdd { .. } | Request::PkgCacheList { .. } | Request::PkgCachePrune { .. } => (
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
/// Use this for requests that may require network I/O (pkg operations).
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
        } => (pkg::handle_pkg_add(specs, cwd, channel).await, false),
        Request::PkgCacheList { channel } => (pkg::handle_pkg_cache_list(channel), false),
        Request::PkgCachePrune { channel } => (pkg::handle_pkg_cache_prune(channel), false),
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
