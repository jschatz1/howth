//! Unix domain socket IPC implementation.

use std::io;
use std::path::Path;
use tokio::net::{UnixListener, UnixStream};

/// Unix domain socket listener.
pub struct IpcListener {
    inner: UnixListener,
}

impl IpcListener {
    /// Bind to the given socket path.
    ///
    /// # Errors
    /// Returns an error if binding fails.
    pub fn bind(path: &str) -> io::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let inner = UnixListener::bind(path)?;
        Ok(Self { inner })
    }

    /// Accept a new connection.
    ///
    /// # Errors
    /// Returns an error if accepting fails.
    pub async fn accept(&self) -> io::Result<IpcStream> {
        let (stream, _addr) = self.inner.accept().await?;
        Ok(IpcStream { inner: stream })
    }
}

/// Unix domain socket stream.
pub struct IpcStream {
    inner: UnixStream,
}

impl IpcStream {
    /// Connect to the given socket path.
    ///
    /// # Errors
    /// Returns an error if connection fails.
    pub async fn connect(path: &str) -> io::Result<Self> {
        let inner = UnixStream::connect(path).await?;
        Ok(Self { inner })
    }
}

impl tokio::io::AsyncRead for IpcStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for IpcStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
