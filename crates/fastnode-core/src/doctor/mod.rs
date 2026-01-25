//! System diagnostics for fastnode.
//!
//! The doctor module provides environment detection and health checks.
//! Used by `fastnode doctor` to report system capabilities and warnings.
//!
//! ## Design Principles
//! - No subprocess calls (fast, sandbox-friendly)
//! - No network calls
//! - No directory walks beyond project root detection
//! - All detection done via file reads or syscalls

#![allow(clippy::doc_markdown)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod capabilities;
mod collectors;

pub use capabilities::*;
pub use collectors::*;

/// Report schema version. Bump when changing JSON structure.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Warning severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
}

/// A diagnostic warning with a stable code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    /// Stable warning code (e.g., `CACHE_NOT_WRITABLE`).
    pub code: String,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
}

impl Warning {
    #[must_use]
    pub fn info(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Info,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn warn(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Warn,
            message: message.into(),
        }
    }
}

/// Stable warning codes. These are part of the public API and must not change.
/// New codes may be added in future versions.
pub mod codes {
    pub const CACHE_NOT_WRITABLE: &str = "CACHE_NOT_WRITABLE";
    pub const DATA_NOT_WRITABLE: &str = "DATA_NOT_WRITABLE";
    pub const LOW_NOFILE_LIMIT: &str = "LOW_NOFILE_LIMIT";
    pub const PROJECT_ROOT_NOT_FOUND: &str = "PROJECT_ROOT_NOT_FOUND";
    pub const FS_CASE_INSENSITIVE: &str = "FS_CASE_INSENSITIVE";
    pub const SYMLINK_UNAVAILABLE: &str = "SYMLINK_UNAVAILABLE";
    pub const HARDLINK_UNAVAILABLE: &str = "HARDLINK_UNAVAILABLE";
    pub const UNKNOWN_OS_VERSION: &str = "UNKNOWN_OS_VERSION";
}

/// Runtime information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub version: String,
    pub schema_version: u32,
    pub channel: String,
}

/// Operating system information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsInfo {
    pub name: String,
    pub version: Option<String>,
    pub arch: String,
}

/// Hardware information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_cores: usize,
    pub cpu_cores_physical: Option<usize>,
}

/// Path information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsInfo {
    pub cwd: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_writable: bool,
    pub data_writable: bool,
}

/// Project information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub root: Option<PathBuf>,
    pub has_package_json: bool,
    pub has_git: bool,
}

/// Filesystem capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub fs_case_sensitive: bool,
    pub symlink_supported: bool,
    pub hardlink_supported: bool,
    /// io_uring support (Linux 5.1+). None if detection failed.
    #[cfg(target_os = "linux")]
    pub io_uring_supported: Option<bool>,
    /// File descriptor limits (Unix only).
    #[cfg(unix)]
    pub rlimit_nofile: Option<RlimitInfo>,
}

/// Resource limit information (Unix only).
#[cfg(unix)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlimitInfo {
    pub soft: u64,
    pub hard: u64,
}

/// Complete doctor report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Schema version for this report format.
    pub report_schema_version: u32,
    pub runtime: RuntimeInfo,
    pub os: OsInfo,
    pub hardware: HardwareInfo,
    pub paths: PathsInfo,
    pub project: ProjectInfo,
    pub capabilities: Capabilities,
    pub warnings: Vec<Warning>,
}

impl DoctorReport {
    /// Collect all diagnostic information.
    ///
    /// This function:
    /// - Does NOT spawn subprocesses
    /// - Does NOT make network calls
    /// - Only reads files and makes syscalls
    #[must_use]
    pub fn collect(cwd: &std::path::Path, channel: crate::config::Channel) -> Self {
        let mut warnings = Vec::new();

        let runtime = collectors::collect_runtime(channel);
        let os = collectors::collect_os(&mut warnings);
        let hardware = collectors::collect_hardware();
        let paths = collectors::collect_paths(cwd, channel, &mut warnings);
        let project = collectors::collect_project(cwd, &mut warnings);
        let capabilities = collectors::collect_capabilities(&mut warnings);

        Self {
            report_schema_version: REPORT_SCHEMA_VERSION,
            runtime,
            os,
            hardware,
            paths,
            project,
            capabilities,
            warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_schema_version_is_stable() {
        // This test documents the current schema version
        // Update this when you intentionally bump the schema
        assert_eq!(REPORT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_warning_codes_are_uppercase() {
        // All warning codes should be SCREAMING_SNAKE_CASE
        let codes = [
            codes::CACHE_NOT_WRITABLE,
            codes::DATA_NOT_WRITABLE,
            codes::LOW_NOFILE_LIMIT,
            codes::PROJECT_ROOT_NOT_FOUND,
            codes::FS_CASE_INSENSITIVE,
            codes::SYMLINK_UNAVAILABLE,
            codes::HARDLINK_UNAVAILABLE,
            codes::UNKNOWN_OS_VERSION,
        ];

        for code in codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Warning code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }
}
