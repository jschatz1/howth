//! Windows named pipe IPC implementation.
//!
//! Uses Tokio's named pipe support for async IPC on Windows.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::sync::Mutex;

/// Prefix for Windows named pipes.
const PIPE_PREFIX: &str = r"\\.\pipe\";

/// Maximum number of server instances (concurrent connections).
const MAX_INSTANCES: usize = 64;

/// Connection timeout for clients.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Retry delay when pipe is busy.
const RETRY_DELAY: Duration = Duration::from_millis(50);

/// Maximum retries for busy pipe.
const MAX_RETRIES: u32 = 20;

/// Normalize a pipe endpoint name.
///
/// If the endpoint starts with `\\.\pipe\`, use it as-is.
/// Otherwise, prepend the pipe prefix.
fn normalize_endpoint(endpoint: &str) -> String {
    if endpoint.starts_with(PIPE_PREFIX) {
        endpoint.to_string()
    } else {
        format!("{}{}", PIPE_PREFIX, endpoint)
    }
}

/// Windows named pipe listener.
///
/// Creates server instances that can accept client connections.
pub struct IpcListener {
    endpoint: String,
    /// The current server instance ready to accept.
    current: Arc<Mutex<Option<NamedPipeServer>>>,
}

impl IpcListener {
    /// Bind to the given named pipe endpoint.
    ///
    /// The endpoint can be a full pipe path (`\\.\pipe\name`) or just a name
    /// (which will have the pipe prefix added).
    ///
    /// # Errors
    /// Returns an error if the pipe cannot be created.
    pub fn bind(endpoint: &str) -> io::Result<Self> {
        let endpoint = normalize_endpoint(endpoint);

        // Create the first server instance
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .max_instances(MAX_INSTANCES)
            .create(&endpoint)?;

        Ok(Self {
            endpoint,
            current: Arc::new(Mutex::new(Some(server))),
        })
    }

    /// Accept a new connection.
    ///
    /// Returns an `IpcStream` representing the connected client.
    /// Creates a new server instance for subsequent accepts.
    ///
    /// # Errors
    /// Returns an error if accepting fails.
    pub async fn accept(&self) -> io::Result<IpcStream> {
        // Take the current server instance
        let server = {
            let mut guard = self.current.lock().await;
            guard.take().ok_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "no server instance available")
            })?
        };

        // Wait for a client to connect
        server.connect().await?;

        // Create a new server instance for the next accept
        let next_server = ServerOptions::new()
            .max_instances(MAX_INSTANCES)
            .create(&self.endpoint)?;

        // Store the new instance
        {
            let mut guard = self.current.lock().await;
            *guard = Some(next_server);
        }

        Ok(IpcStream::Server(server))
    }
}

/// Windows named pipe stream.
///
/// Can be either a server-side or client-side pipe.
pub enum IpcStream {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

impl IpcStream {
    /// Connect to the given named pipe endpoint.
    ///
    /// Uses retry logic with backoff to handle the case where the pipe
    /// is temporarily busy.
    ///
    /// # Errors
    /// Returns an error if connection fails after retries or timeout.
    pub async fn connect(endpoint: &str) -> io::Result<Self> {
        let endpoint = normalize_endpoint(endpoint);

        // Use timeout for the overall connect operation
        let connect_fut = async {
            let mut retries = 0;

            loop {
                match ClientOptions::new().open(&endpoint) {
                    Ok(client) => return Ok(IpcStream::Client(client)),
                    Err(e) if e.raw_os_error() == Some(231) && retries < MAX_RETRIES => {
                        // ERROR_PIPE_BUSY (231) - all pipe instances are busy
                        retries += 1;
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        };

        tokio::time::timeout(CONNECT_TIMEOUT, connect_fut)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out connecting to daemon; is it running?",
                )
            })?
    }
}

impl tokio::io::AsyncRead for IpcStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            IpcStream::Server(s) => Pin::new(s).poll_read(cx, buf),
            IpcStream::Client(c) => Pin::new(c).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for IpcStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            IpcStream::Server(s) => Pin::new(s).poll_write(cx, buf),
            IpcStream::Client(c) => Pin::new(c).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            IpcStream::Server(s) => Pin::new(s).poll_flush(cx),
            IpcStream::Client(c) => Pin::new(c).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            IpcStream::Server(s) => Pin::new(s).poll_shutdown(cx),
            IpcStream::Client(c) => Pin::new(c).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_endpoint_with_prefix() {
        let endpoint = r"\\.\pipe\my-pipe";
        assert_eq!(normalize_endpoint(endpoint), endpoint);
    }

    #[test]
    fn test_normalize_endpoint_without_prefix() {
        let endpoint = "fastnode-test";
        assert_eq!(normalize_endpoint(endpoint), r"\\.\pipe\fastnode-test");
    }

    #[test]
    fn test_normalize_endpoint_short_name() {
        let endpoint = "test";
        assert_eq!(normalize_endpoint(endpoint), r"\\.\pipe\test");
    }
}
