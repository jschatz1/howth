//! IPC transport abstraction for daemon communication.
//!
//! Platform-specific implementations:
//! - Unix: Unix domain sockets via tokio
//! - Windows: Named pipes via tokio

#[cfg(unix)]
mod unix;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{IpcListener, IpcStream};

#[cfg(windows)]
pub use windows::{IpcListener, IpcStream};

use std::io;

/// Trait for async reading/writing on IPC streams.
pub trait IpcStreamExt: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> IpcStreamExt for T {}

/// Maximum frame size for sanity checking (16 MiB).
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Remove the socket file if it exists (Unix only).
///
/// On Windows, named pipes are automatically cleaned up by the OS.
///
/// # Errors
/// Returns an error if the file exists but cannot be removed.
pub fn cleanup_socket(endpoint: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        let path = std::path::Path::new(endpoint);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }

    #[cfg(windows)]
    {
        // Named pipes don't need cleanup - OS handles it
        let _ = endpoint;
    }

    Ok(())
}
