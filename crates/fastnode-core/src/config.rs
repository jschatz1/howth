use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Runtime configuration for fastnode CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Current working directory.
    pub cwd: PathBuf,

    /// Whether to emit JSON logs.
    pub json_logs: bool,

    /// Verbosity level (0 = INFO, 1 = DEBUG, 2+ = TRACE).
    pub verbosity: u8,

    /// Channel (dev, nightly, stable) - affects cache paths.
    pub channel: Channel,
}

/// Release channel for cache/data directory namespacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Nightly,
    Dev,
}

impl Channel {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Dev => "dev",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            json_logs: false,
            verbosity: 0,
            channel: Channel::default(),
        }
    }
}

impl Config {
    /// Create a new config with the given working directory.
    #[must_use]
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Default::default()
        }
    }

    /// Set verbosity level.
    #[must_use]
    pub fn with_verbosity(mut self, verbosity: u8) -> Self {
        self.verbosity = verbosity;
        self
    }

    /// Set JSON log output.
    #[must_use]
    pub fn with_json_logs(mut self, json: bool) -> Self {
        self.json_logs = json;
        self
    }

    /// Set channel.
    #[must_use]
    pub fn with_channel(mut self, channel: Channel) -> Self {
        self.channel = channel;
        self
    }
}
