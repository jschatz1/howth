//! Smoke benchmark runner.
//!
//! Runs a small set of internal hot-path benchmarks:
//! - `hash_file_blake3`: Blake3 file hashing
//! - `atomic_write`: Atomic file write (temp + rename)
//! - `project_root_walkup`: Project root detection (parent traversal)

use super::{codes, BenchParams, BenchReport, BenchResult, BenchWarning};
use crate::bench::stats::compute_stats;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tempfile::TempDir;

/// Minimum allowed size in bytes (1 MiB).
pub const MIN_SIZE_BYTES: u64 = 1024 * 1024;

/// Maximum allowed size in bytes (256 MiB).
pub const MAX_SIZE_BYTES: u64 = 256 * 1024 * 1024;

/// Run all smoke benchmarks.
///
/// # Arguments
/// - `iters`: Number of measured iterations
/// - `warmup`: Number of warmup iterations (not measured)
/// - `size_bytes`: Payload size for file operations
///
/// Returns a complete benchmark report.
///
/// # Panics
/// Panics if unable to create a temporary directory.
#[must_use]
pub fn run_smoke_benchmarks(iters: u32, warmup: u32, size_bytes: u64) -> BenchReport {
    let mut warnings = Vec::new();

    // Validate and clamp parameters
    if iters < 10 {
        warnings.push(BenchWarning::info(
            codes::LOW_ITERS,
            format!("Low iteration count ({iters}); results may have high variance"),
        ));
    }

    let clamped_size = size_bytes.clamp(MIN_SIZE_BYTES, MAX_SIZE_BYTES);
    if clamped_size != size_bytes {
        warnings.push(BenchWarning::info(
            codes::SIZE_CLAMPED,
            format!("Size clamped from {size_bytes} to {clamped_size} bytes"),
        ));
    }

    let params = BenchParams {
        iters,
        warmup,
        size_bytes: clamped_size,
    };

    // Create temp directory for all benchmarks
    let temp_dir = TempDir::new().expect("failed to create temp directory");

    // Run benchmarks
    let results = vec![
        bench_hash_file_blake3(temp_dir.path(), iters, warmup, clamped_size),
        bench_atomic_write(temp_dir.path(), iters, warmup, clamped_size),
        bench_project_root_walkup(temp_dir.path(), iters, warmup),
    ];

    BenchReport::new(params, results, warnings)
}

/// Generate deterministic payload data.
///
/// Uses a simple repeating pattern to avoid PRNG overhead.
#[allow(clippy::cast_possible_truncation)]
fn generate_payload(size: u64) -> Vec<u8> {
    // Use a repeating pattern for determinism and speed
    const PATTERN: &[u8] = b"fastnode_bench_payload_data_0123456789abcdef";
    // Safe truncation: size is clamped to MAX_SIZE_BYTES (256 MiB)
    let size = size as usize;
    let mut data = Vec::with_capacity(size);

    while data.len() < size {
        let remaining = size - data.len();
        let chunk_size = remaining.min(PATTERN.len());
        data.extend_from_slice(&PATTERN[..chunk_size]);
    }

    data
}

/// Convert Duration to nanoseconds as u64.
///
/// Truncation is intentional: durations over ~585 years would overflow,
/// but our benchmarks measure microseconds to milliseconds.
#[allow(clippy::cast_possible_truncation)]
fn duration_to_nanos(d: std::time::Duration) -> u64 {
    d.as_nanos() as u64
}

/// Benchmark: Blake3 file hashing.
fn bench_hash_file_blake3(
    temp_dir: &Path,
    iters: u32,
    warmup: u32,
    size_bytes: u64,
) -> BenchResult {
    let payload_path = temp_dir.join("payload.bin");
    let payload = generate_payload(size_bytes);

    // Create the payload file once
    fs::write(&payload_path, &payload).expect("failed to write payload file");

    // Warmup
    for _ in 0..warmup {
        let _ = fastnode_util::hash::blake3_file(&payload_path);
    }

    // Measured iterations
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = fastnode_util::hash::blake3_file(&payload_path);
        let elapsed = start.elapsed();
        samples.push(duration_to_nanos(elapsed));
    }

    let stats = compute_stats(&samples);
    BenchResult::new("hash_file_blake3", iters, stats)
}

/// Benchmark: Atomic file write.
fn bench_atomic_write(temp_dir: &Path, iters: u32, warmup: u32, size_bytes: u64) -> BenchResult {
    let target_path = temp_dir.join("atomic_target.bin");
    let payload = generate_payload(size_bytes);

    // Warmup
    for _ in 0..warmup {
        let _ = fastnode_util::fs::atomic_write(&target_path, &payload);
    }

    // Measured iterations
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        fastnode_util::fs::atomic_write(&target_path, &payload).expect("atomic write failed");
        let elapsed = start.elapsed();
        samples.push(duration_to_nanos(elapsed));
    }

    let stats = compute_stats(&samples);
    BenchResult::new("atomic_write", iters, stats)
}

/// Benchmark: Project root walk-up detection.
fn bench_project_root_walkup(temp_dir: &Path, iters: u32, warmup: u32) -> BenchResult {
    // Create nested directories: root/a/b/c/d/e
    let root = temp_dir.join("project_root");
    let nested = root.join("a").join("b").join("c").join("d").join("e");
    fs::create_dir_all(&nested).expect("failed to create nested dirs");

    // Create package.json in root
    fs::write(root.join("package.json"), "{}").expect("failed to write package.json");

    // Warmup
    for _ in 0..warmup {
        let _ = crate::paths::project_root(&nested);
    }

    // Measured iterations
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let start = Instant::now();
        let _ = crate::paths::project_root(&nested);
        let elapsed = start.elapsed();
        samples.push(duration_to_nanos(elapsed));
    }

    let stats = compute_stats(&samples);
    BenchResult::new("project_root_walkup", iters, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_payload_size() {
        let payload = generate_payload(1000);
        assert_eq!(payload.len(), 1000);

        let payload = generate_payload(100_000);
        assert_eq!(payload.len(), 100_000);
    }

    #[test]
    fn test_generate_payload_deterministic() {
        let p1 = generate_payload(1000);
        let p2 = generate_payload(1000);
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_run_smoke_benchmarks_returns_three_results() {
        // Use minimal params for speed
        let report = run_smoke_benchmarks(5, 1, MIN_SIZE_BYTES);

        assert_eq!(report.results.len(), 3);

        let names: Vec<&str> = report.results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"hash_file_blake3"));
        assert!(names.contains(&"atomic_write"));
        assert!(names.contains(&"project_root_walkup"));
    }

    #[test]
    fn test_run_smoke_benchmarks_results_have_samples() {
        let report = run_smoke_benchmarks(10, 2, MIN_SIZE_BYTES);

        for result in &report.results {
            assert_eq!(result.samples, 10);
            assert!(result.min_ns > 0, "{} min_ns should be > 0", result.name);
            assert!(
                result.median_ns >= result.min_ns,
                "{} median should be >= min",
                result.name
            );
            assert!(
                result.p95_ns >= result.median_ns,
                "{} p95 should be >= median",
                result.name
            );
            assert!(
                result.max_ns >= result.p95_ns,
                "{} max should be >= p95",
                result.name
            );
        }
    }

    #[test]
    fn test_low_iters_warning() {
        let report = run_smoke_benchmarks(5, 1, MIN_SIZE_BYTES);

        assert!(
            report.warnings.iter().any(|w| w.code == codes::LOW_ITERS),
            "Should warn about low iteration count"
        );
    }

    #[test]
    fn test_size_clamped_warning() {
        // Size below minimum
        let report = run_smoke_benchmarks(10, 1, 100);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.code == codes::SIZE_CLAMPED),
            "Should warn about size being clamped"
        );
        assert_eq!(report.params.size_bytes, MIN_SIZE_BYTES);

        // Size above maximum
        let report = run_smoke_benchmarks(10, 1, MAX_SIZE_BYTES + 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.code == codes::SIZE_CLAMPED),
            "Should warn about size being clamped"
        );
        assert_eq!(report.params.size_bytes, MAX_SIZE_BYTES);
    }

    #[test]
    fn test_bench_schema_version_in_report() {
        let report = run_smoke_benchmarks(5, 1, MIN_SIZE_BYTES);
        assert_eq!(
            report.bench_schema_version,
            super::super::BENCH_SCHEMA_VERSION
        );
    }
}
