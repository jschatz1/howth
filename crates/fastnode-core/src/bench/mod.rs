//! Micro-benchmark harness for fastnode.
//!
//! Provides a fast, deterministic benchmark for internal hot-path operations.
//! This is NOT a full benchmarking suite - use `fastnode-bench` (Criterion) for that.
//!
//! ## Design Principles
//! - No subprocess calls
//! - No network calls
//! - Uses only temp directories
//! - Low noise: warmup runs + multiple iterations + simple statistics

use serde::{Deserialize, Serialize};

pub mod build;
pub mod bundler;
pub mod http;
pub mod install;
pub mod rusage;
pub mod smoke;
pub mod stats;
pub mod test;

pub use build::{
    run_build_bench, BaselineResult, BenchTarget, BuildBenchParams, BuildBenchReport,
    BuildBenchResult, MachineInfo, ResourceStats, WorkDoneStats, BUILD_BENCH_SCHEMA_VERSION,
};
pub use bundler::{
    run_bundler_bench, BundlerBenchParams, BundlerBenchReport, BundlerComparison,
    BundlerToolResult, BUNDLER_BENCH_SCHEMA_VERSION,
};
pub use http::{
    run_http_bench, HttpBenchParams, HttpBenchReport, HttpComparison, HttpToolResult,
    DEFAULT_CONNECTIONS, DEFAULT_DURATION_SECS, DEFAULT_WARMUP_SECS, HTTP_BENCH_SCHEMA_VERSION,
};
pub use install::{
    run_install_bench, InstallBenchParams, InstallBenchReport, InstallComparison,
    InstallProjectInfo, InstallToolResult, INSTALL_BENCH_SCHEMA_VERSION,
};
pub use smoke::run_smoke_benchmarks;
pub use stats::compute_stats;
pub use test::{
    run_test_bench, TestBenchParams, TestBenchReport, TestComparison, TestProjectInfo,
    TestToolResult, TEST_BENCH_SCHEMA_VERSION,
};

/// Bench report schema version. Bump when changing JSON structure.
pub const BENCH_SCHEMA_VERSION: u32 = 1;

/// Severity levels for bench warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
}

/// A benchmark warning with a stable code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchWarning {
    /// Stable warning code (e.g., `LOW_ITERS`).
    pub code: String,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
}

impl BenchWarning {
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

/// Warning codes for bench.
pub mod codes {
    pub const LOW_ITERS: &str = "LOW_ITERS";
    pub const SIZE_CLAMPED: &str = "SIZE_CLAMPED";
}

/// Runtime information for the benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchRuntimeInfo {
    pub fastnode_version: String,
    pub os: String,
    pub arch: String,
}

/// Benchmark parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchParams {
    pub iters: u32,
    pub warmup: u32,
    pub size_bytes: u64,
}

/// Statistics for a single benchmark.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BenchStats {
    pub min_ns: u64,
    pub median_ns: u64,
    pub p95_ns: u64,
    pub max_ns: u64,
}

/// Result of a single benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    /// Stable identifier (e.g., `hash_file_blake3`).
    pub name: String,
    /// Unit of measurement.
    pub unit: String,
    /// Number of samples taken.
    pub samples: u32,
    /// Minimum time in nanoseconds.
    pub min_ns: u64,
    /// Median time in nanoseconds.
    pub median_ns: u64,
    /// 95th percentile time in nanoseconds.
    pub p95_ns: u64,
    /// Maximum time in nanoseconds.
    pub max_ns: u64,
}

impl BenchResult {
    /// Create a new bench result from stats.
    #[must_use]
    pub fn new(name: impl Into<String>, samples: u32, stats: BenchStats) -> Self {
        Self {
            name: name.into(),
            unit: "ns/op".to_string(),
            samples,
            min_ns: stats.min_ns,
            median_ns: stats.median_ns,
            p95_ns: stats.p95_ns,
            max_ns: stats.max_ns,
        }
    }
}

/// Complete benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    /// Schema version for this report format.
    pub bench_schema_version: u32,
    /// Runtime information.
    pub runtime: BenchRuntimeInfo,
    /// Benchmark parameters.
    pub params: BenchParams,
    /// Benchmark results.
    pub results: Vec<BenchResult>,
    /// Warnings encountered during benchmarking.
    pub warnings: Vec<BenchWarning>,
}

impl BenchReport {
    /// Create a new benchmark report.
    #[must_use]
    pub fn new(
        params: BenchParams,
        results: Vec<BenchResult>,
        warnings: Vec<BenchWarning>,
    ) -> Self {
        Self {
            bench_schema_version: BENCH_SCHEMA_VERSION,
            runtime: BenchRuntimeInfo {
                fastnode_version: crate::version::VERSION.to_string(),
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
            },
            params,
            results,
            warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bench_schema_version_is_stable() {
        assert_eq!(BENCH_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_warning_codes_are_uppercase() {
        let codes = [codes::LOW_ITERS, codes::SIZE_CLAMPED];

        for code in codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Warning code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }
}
