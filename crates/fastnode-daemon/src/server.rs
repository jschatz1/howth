//! Daemon server implementation.

use crate::ipc::{cleanup_socket, IpcListener, IpcStream};
use crate::state::DaemonState;
use crate::{handle_build, handle_request, handle_request_async, make_response_frame};
use fastnode_proto::{codes, encode_frame, Frame, Request, Response};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Maximum frame size for sanity checking (16 MiB).
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Daemon configuration.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// IPC endpoint (socket path on Unix, pipe name on Windows).
    pub endpoint: String,
}

/// Run the daemon server.
///
/// Listens for IPC connections and handles requests.
///
/// # Errors
/// Returns an error if the server cannot start or encounters a fatal error.
pub async fn run_server(config: DaemonConfig) -> io::Result<()> {
    // Clean up any stale socket
    cleanup_socket(&config.endpoint)?;

    // Bind to the endpoint
    let listener = IpcListener::bind(&config.endpoint)?;
    info!(endpoint = %config.endpoint, "daemon listening");

    // Shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));

    // Create daemon state (cache + watcher)
    let state = Arc::new(DaemonState::new());

    // Wire caches to watcher for invalidation
    state.watcher.set_cache(state.cache.clone());
    state.watcher.set_pkg_json_cache(state.pkg_json_cache.clone());
    state.watcher.set_build_cache(state.build_cache.clone());

    // Accept loop
    loop {
        if shutdown.load(Ordering::Relaxed) {
            info!("shutdown requested, exiting");
            break;
        }

        // Accept with timeout to check shutdown flag periodically
        let accept_result =
            tokio::time::timeout(std::time::Duration::from_secs(1), listener.accept()).await;

        match accept_result {
            Ok(Ok(stream)) => {
                debug!("accepted connection");
                let shutdown_flag = shutdown.clone();
                let daemon_state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, shutdown_flag, daemon_state).await {
                        warn!(error = %e, "connection handler error");
                    }
                });
            }
            Ok(Err(e)) => {
                error!(error = %e, "accept failed");
            }
            Err(_) => {
                // Timeout, check shutdown flag and continue
            }
        }
    }

    // Stop watcher if running
    let _ = state.watcher.stop();

    // Clean up socket on exit
    let _ = cleanup_socket(&config.endpoint);

    Ok(())
}

/// Check if a request requires async handling (pkg operations).
fn is_pkg_request(request: &Request) -> bool {
    matches!(
        request,
        Request::PkgAdd { .. } | Request::PkgCacheList { .. } | Request::PkgCachePrune { .. }
    )
}

/// Check if a request is a watch build (requires streaming).
fn is_watch_build(request: &Request) -> bool {
    matches!(request, Request::WatchBuild { .. })
}

/// Handle watch build with streaming responses (v3.0).
async fn handle_watch_build_streaming(
    mut stream: IpcStream,
    frame: Frame,
    state: Arc<DaemonState>,
) -> io::Result<()> {
    // Extract watch build parameters
    let (cwd, targets, debounce_ms, max_parallel) = match &frame.request {
        Request::WatchBuild {
            cwd,
            targets,
            debounce_ms,
            max_parallel,
        } => (cwd.clone(), targets.clone(), *debounce_ms, *max_parallel),
        _ => {
            // Should not happen - we checked is_watch_build
            let response = make_response_frame(Response::error(
                codes::INTERNAL_ERROR,
                "Expected WatchBuild request",
            ));
            let encoded = encode_frame(&response)?;
            stream.write_all(&encoded).await?;
            return Ok(());
        }
    };

    // Validate cwd
    let cwd_path = PathBuf::from(&cwd);
    if !cwd_path.exists() || !cwd_path.is_dir() {
        let response = make_response_frame(Response::error(
            codes::BUILD_CWD_INVALID,
            format!("Invalid working directory: {cwd}"),
        ));
        let encoded = encode_frame(&response)?;
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    info!(cwd = %cwd, targets = ?targets, debounce_ms, "starting watch build");

    // Send WatchBuildStarted confirmation
    let started_response = make_response_frame(Response::WatchBuildStarted {
        cwd: cwd.clone(),
        targets: targets.clone(),
        debounce_ms,
    });
    let encoded = encode_frame(&started_response)?;
    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Create a channel for file change notifications
    let (tx, mut rx) = mpsc::channel::<()>(16);

    // Subscribe watcher to the cwd
    if let Err(e) = state.watcher.watch_for_build(&cwd_path, tx) {
        warn!(error = %e, "failed to start watcher");
        let response = make_response_frame(Response::WatchBuildStopped {
            reason: format!("Failed to start watcher: {e}"),
        });
        let encoded = encode_frame(&response)?;
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    // Helper to run a build and send result
    let run_build = || {
        let build_cache = Some(state.build_cache.clone());
        handle_build(&cwd, false, false, max_parallel, false, &targets, build_cache)
    };

    // Run initial build
    let initial_result = run_build();
    let response = make_response_frame(initial_result);
    let encoded = encode_frame(&response)?;
    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Watch loop with debouncing
    let debounce_duration = std::time::Duration::from_millis(u64::from(debounce_ms));
    let mut read_buf = [0u8; 1];

    loop {
        // Wait for file change notification or connection close
        tokio::select! {
            _ = rx.recv() => {
                // File changed - debounce
                debug!("file change detected, debouncing...");

                // Drain any additional events during debounce period
                let deadline = tokio::time::Instant::now() + debounce_duration;
                loop {
                    tokio::select! {
                        _ = rx.recv() => {
                            // More events, keep debouncing
                        }
                        _ = tokio::time::sleep_until(deadline) => {
                            break;
                        }
                    }
                }

                debug!("debounce complete, rebuilding...");

                // Invalidate build cache for this cwd
                state.build_cache.clear();

                // Run build
                let result = run_build();
                let response = make_response_frame(result);
                match encode_frame(&response) {
                    Ok(encoded) => {
                        if let Err(e) = stream.write_all(&encoded).await {
                            info!(error = %e, "client disconnected");
                            break;
                        }
                        if let Err(e) = stream.flush().await {
                            info!(error = %e, "client disconnected");
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "failed to encode response");
                        break;
                    }
                }
            }
            // Check if stream is still open by trying to read
            result = stream.read(&mut read_buf) => {
                match result {
                    Ok(0) | Err(_) => {
                        // EOF or error - client disconnected
                        info!("client disconnected, stopping watch");
                        break;
                    }
                    Ok(_) => {
                        // Unexpected data - ignore
                    }
                }
            }
        }
    }

    // Stop watching
    let _ = state.watcher.unwatch(&cwd_path);

    Ok(())
}

/// Handle a single connection.
async fn handle_connection(
    mut stream: IpcStream,
    shutdown: Arc<AtomicBool>,
    state: Arc<DaemonState>,
) -> io::Result<()> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    // Sanity check
    if len > MAX_FRAME_SIZE {
        let response = make_response_frame(Response::error(
            codes::INVALID_REQUEST,
            format!("frame too large: {len} bytes"),
        ));
        let encoded = encode_frame(&response)?;
        stream.write_all(&encoded).await?;
        return Ok(());
    }

    // Read frame
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    // Decode
    let frame: Frame = match serde_json::from_slice(&buf) {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, "invalid frame");
            let response = make_response_frame(Response::error(
                codes::INVALID_REQUEST,
                format!("invalid frame: {e}"),
            ));
            let encoded = encode_frame(&response)?;
            stream.write_all(&encoded).await?;
            return Ok(());
        }
    };

    debug!(
        client_version = %frame.hello.client_version,
        proto_version = frame.hello.proto_schema_version,
        request = ?frame.request,
        "handling request"
    );

    // v3.0: Watch build requires streaming handler
    if is_watch_build(&frame.request) {
        return handle_watch_build_streaming(stream, frame, state).await;
    }

    // Handle request - use async handler for pkg operations
    let (response, should_shutdown) = if is_pkg_request(&frame.request) {
        handle_request_async(
            &frame.request,
            frame.hello.proto_schema_version,
            Some(&state),
        )
        .await
    } else {
        handle_request(
            &frame.request,
            frame.hello.proto_schema_version,
            Some(&state),
        )
    };

    // Send response
    let response_frame = make_response_frame(response);
    let encoded = encode_frame(&response_frame)?;
    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Set shutdown flag if requested
    if should_shutdown {
        shutdown.store(true, Ordering::Relaxed);
    }

    Ok(())
}
