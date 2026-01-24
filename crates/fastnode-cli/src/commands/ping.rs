use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::VERSION;
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{encode_frame, Frame, FrameResponse, Request, Response};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::io;

/// Ping response for JSON output.
#[derive(Serialize)]
struct PingResult {
    ok: bool,
    nonce: u64,
    server_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_time_unix_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Run the ping command.
///
/// Connects to the daemon and sends a ping request.
#[allow(clippy::cast_possible_truncation)]
pub fn run(channel: Channel, json: bool) -> Result<()> {
    let endpoint = paths::ipc_endpoint(channel);

    // Generate nonce (truncation is fine for nonce purposes)
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    // Run the async client
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result = runtime.block_on(async { ping_daemon(&endpoint, nonce).await });

    match result {
        Ok((response, server_version)) => handle_response(response, nonce, server_version, json),
        Err(e) => {
            if json {
                let result = PingResult {
                    ok: false,
                    nonce,
                    server_version: String::new(),
                    server_time_unix_ms: None,
                    error: Some(format!("Failed to connect: {e}")),
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("error: daemon not running");
                eprintln!("hint: start with `howth daemon`");
            }
            std::process::exit(1);
        }
    }
}

fn handle_response(
    response: Response,
    expected_nonce: u64,
    server_version: String,
    json: bool,
) -> Result<()> {
    match response {
        Response::Pong {
            nonce,
            server_time_unix_ms,
        } => {
            if nonce != expected_nonce {
                if json {
                    let result = PingResult {
                        ok: false,
                        nonce,
                        server_version,
                        server_time_unix_ms,
                        error: Some(format!(
                            "Nonce mismatch: expected {expected_nonce}, got {nonce}"
                        )),
                    };
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
                } else {
                    eprintln!("error: nonce mismatch");
                }
                std::process::exit(1);
            }

            if json {
                let result = PingResult {
                    ok: true,
                    nonce,
                    server_version,
                    server_time_unix_ms,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("pong");
            }
            Ok(())
        }
        Response::Error { code, message } => {
            if json {
                let result = PingResult {
                    ok: false,
                    nonce: expected_nonce,
                    server_version,
                    server_time_unix_ms: None,
                    error: Some(format!("{code}: {message}")),
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("error: {code}: {message}");
            }
            std::process::exit(1);
        }
        _ => {
            if json {
                let result = PingResult {
                    ok: false,
                    nonce: expected_nonce,
                    server_version,
                    server_time_unix_ms: None,
                    error: Some("Unexpected response type".to_string()),
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("error: unexpected response");
            }
            std::process::exit(1);
        }
    }
}

async fn ping_daemon(endpoint: &str, nonce: u64) -> io::Result<(Response, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Connect using cross-platform IpcStream
    let mut stream = IpcStream::connect(endpoint).await?;

    // Create and send request frame
    let frame = Frame::new(VERSION, Request::Ping { nonce });
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
