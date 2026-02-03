use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::VERSION;
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{encode_frame, Frame, Request, Response, FrameResponse};
use miette::{IntoDiagnostic, Result};
use std::io;

/// Stop the running daemon by sending a Shutdown request.
pub fn run(channel: Channel, _json: bool) -> Result<()> {
    let endpoint = paths::ipc_endpoint(channel);

    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result = runtime.block_on(async { stop_daemon(&endpoint).await });

    match result {
        Ok(Response::ShutdownAck) => {
            eprintln!("daemon stopped");
            Ok(())
        }
        Ok(_) => {
            eprintln!("error: unexpected response from daemon");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: daemon not running ({e})");
            std::process::exit(1);
        }
    }
}

async fn stop_daemon(endpoint: &str) -> io::Result<Response> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = IpcStream::connect(endpoint).await?;

    let frame = Frame::new(VERSION, Request::Shutdown);
    let encoded = encode_frame(&frame)?;

    stream.write_all(&encoded).await?;
    stream.flush().await?;

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

    Ok(response.response)
}
