use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_daemon::{run_server, DaemonConfig};
use miette::{IntoDiagnostic, Result};
use std::io::Write;

/// Run the daemon command.
///
/// Starts the daemon in the foreground.
pub fn run(channel: Channel, _json: bool) -> Result<()> {
    // Ensure IPC directory exists
    paths::ensure_ipc_dir(channel).into_diagnostic()?;

    let endpoint = paths::ipc_endpoint(channel);
    let config = DaemonConfig { endpoint };

    // Print startup message to stderr
    eprintln!("daemon listening at {}", config.endpoint);
    std::io::stderr().flush().into_diagnostic()?;

    // Run the async server
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    runtime.block_on(async { run_server(config).await.into_diagnostic() })
}
