//! `fastnode watch` command implementation.

use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::VERSION;
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{encode_frame, Frame, FrameResponse, Request, Response};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::io;
use std::path::PathBuf;

/// Watch command action.
#[derive(Debug, Clone)]
pub enum WatchAction {
    Start { roots: Vec<PathBuf> },
    Stop,
    Status,
}

/// Watch status response for JSON output.
#[derive(Serialize)]
struct WatchStatusResult {
    ok: bool,
    running: bool,
    roots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_event_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Watch start/stop response for JSON output.
#[derive(Serialize)]
struct WatchActionResult {
    ok: bool,
    action: String,
    roots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Run the watch command.
pub fn run(action: WatchAction, channel: Channel, json: bool) -> Result<()> {
    let endpoint = paths::ipc_endpoint(channel);

    // Run the async client
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result = runtime.block_on(async { send_watch_request(&endpoint, &action).await });

    match result {
        Ok((response, _server_version)) => handle_response(response, &action, json),
        Err(e) => {
            if json {
                match &action {
                    WatchAction::Status => {
                        let result = WatchStatusResult {
                            ok: false,
                            running: false,
                            roots: Vec::new(),
                            last_event_unix_ms: None,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    WatchAction::Start { .. } | WatchAction::Stop => {
                        let result = WatchActionResult {
                            ok: false,
                            action: action_name(&action).to_string(),
                            roots: Vec::new(),
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: daemon not running");
                eprintln!("hint: start with `howth daemon`");
            }
            std::process::exit(1);
        }
    }
}

fn action_name(action: &WatchAction) -> &'static str {
    match action {
        WatchAction::Start { .. } => "start",
        WatchAction::Stop => "stop",
        WatchAction::Status => "status",
    }
}

fn handle_response(response: Response, action: &WatchAction, json: bool) -> Result<()> {
    match response {
        Response::WatchStarted { roots } => {
            if json {
                let result = WatchActionResult {
                    ok: true,
                    action: "start".to_string(),
                    roots,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Watcher started");
                for root in &roots {
                    println!("  Watching: {root}");
                }
            }
            Ok(())
        }
        Response::WatchStopped => {
            if json {
                let result = WatchActionResult {
                    ok: true,
                    action: "stop".to_string(),
                    roots: Vec::new(),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Watcher stopped");
            }
            Ok(())
        }
        Response::WatchStatus {
            roots,
            running,
            last_event_unix_ms,
        } => {
            if json {
                let result = WatchStatusResult {
                    ok: true,
                    running,
                    roots,
                    last_event_unix_ms,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Status: {}", if running { "running" } else { "stopped" });
                if !roots.is_empty() {
                    println!("Roots:");
                    for root in &roots {
                        println!("  {root}");
                    }
                }
                if let Some(ts) = last_event_unix_ms {
                    println!("Last event: {ts} ms since epoch");
                }
            }
            Ok(())
        }
        Response::Error { code, message } => {
            if json {
                match action {
                    WatchAction::Status => {
                        let result = WatchStatusResult {
                            ok: false,
                            running: false,
                            roots: Vec::new(),
                            last_event_unix_ms: None,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    WatchAction::Start { .. } | WatchAction::Stop => {
                        let result = WatchActionResult {
                            ok: false,
                            action: action_name(action).to_string(),
                            roots: Vec::new(),
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: {code}: {message}");
            }
            std::process::exit(1);
        }
        _ => {
            if json {
                match action {
                    WatchAction::Status => {
                        let result = WatchStatusResult {
                            ok: false,
                            running: false,
                            roots: Vec::new(),
                            last_event_unix_ms: None,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    WatchAction::Start { .. } | WatchAction::Stop => {
                        let result = WatchActionResult {
                            ok: false,
                            action: action_name(action).to_string(),
                            roots: Vec::new(),
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: unexpected response");
            }
            std::process::exit(1);
        }
    }
}

async fn send_watch_request(
    endpoint: &str,
    action: &WatchAction,
) -> io::Result<(Response, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Connect using cross-platform IpcStream
    let mut stream = IpcStream::connect(endpoint).await?;

    // Create request based on action
    let request = match action {
        WatchAction::Start { roots } => Request::WatchStart {
            roots: roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        },
        WatchAction::Stop => Request::WatchStop,
        WatchAction::Status => Request::WatchStatus,
    };

    // Create and send request frame
    let frame = Frame::new(VERSION, request);
    let encoded = encode_frame(&frame)?;

    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Read response
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("response frame too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let response: FrameResponse =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((response.response, response.hello.server_version))
}
