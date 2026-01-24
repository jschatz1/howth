//! Daemon server implementation.

use crate::ipc::{cleanup_socket, IpcListener, IpcStream};
use crate::state::DaemonState;
use crate::{handle_request, handle_request_async, make_response_frame};
use fastnode_proto::{codes, encode_frame, Frame, Request, Response};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
