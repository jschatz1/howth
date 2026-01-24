//! Logging initialization for the CLI.
//!
//! Logging is owned by the CLI crate to keep library crates lightweight.
//! Uses tracing with structured JSON output for machine-readable logs.

use tracing::Level;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the tracing subscriber based on configuration.
///
/// # Arguments
/// * `verbosity` - 0 = INFO, 1 = DEBUG, 2+ = TRACE
/// * `json` - If true, output stable JSON lines to stderr
///
/// JSON output format (stable contract):
/// ```json
/// {"timestamp":"...","level":"INFO","target":"fastnode","span":{"cmd":"run","cwd":"/path"},"message":"..."}
/// ```
///
/// # Panics
/// Panics if the subscriber cannot be initialized (e.g., called twice).
pub fn init(verbosity: u8, json: bool) {
    let level = match verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    // Support RUST_LOG env var, with verbosity flag as override
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"))
        .add_directive(format!("fastnode={level}").parse().unwrap())
        .add_directive(level.into());

    let subscriber = tracing_subscriber::registry().with(filter);

    if json {
        // Stable JSON format for machine parsing
        subscriber
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(false)
                    .with_writer(std::io::stderr),
            )
            .init();
    } else {
        subscriber
            .with(fmt::layer().with_target(false).with_writer(std::io::stderr))
            .init();
    }
}
