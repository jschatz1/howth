//! Test benchmark harness for fastnode.
//!
//! Compares test execution speed across howth, node, and bun by shelling out
//! to each tool as a subprocess. All tools are measured the same way
//! (wall clock + `RUSAGE_CHILDREN`) for a fair comparison.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::format_push_string)]

use crate::bench::build::MachineInfo;
use crate::bench::rusage;
use crate::bench::stats::{compute_median, compute_stats};
use crate::bench::BenchWarning;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Schema version for test benchmark reports.
pub const TEST_BENCH_SCHEMA_VERSION: u32 = 1;

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = 5;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = 1;

/// Number of test files to generate.
const NUM_TEST_FILES: u32 = 500;

/// Number of test cases per file.
const TESTS_PER_FILE: u32 = 20;

/// Parameters for the test benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestBenchParams {
    /// Number of measured iterations.
    pub iters: u32,
    /// Number of warmup iterations (not measured).
    pub warmup: u32,
}

/// Info about the test project used for the benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestProjectInfo {
    /// Project name.
    pub name: String,
    /// Number of test files.
    pub test_files: u32,
    /// Total number of test cases.
    pub test_cases: u32,
}

/// Result for a single tool's test benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestToolResult {
    /// Tool name: "howth", "node", or "bun".
    pub tool: String,
    /// Exact command that was run.
    pub command: String,
    /// Median time in nanoseconds.
    pub median_ns: u64,
    /// 95th percentile time in nanoseconds.
    pub p95_ns: u64,
    /// Minimum time in nanoseconds.
    pub min_ns: u64,
    /// Maximum time in nanoseconds.
    pub max_ns: u64,
    /// Number of measured samples.
    pub samples: u32,
    /// Median total CPU time (user + system) in microseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_cpu_us: Option<u64>,
    /// Peak resident set size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_rss_bytes: Option<u64>,
}

/// Comparison of howth vs another tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestComparison {
    /// The tool being compared against ("node" or "bun").
    pub tool: String,
    /// Speedup factor (e.g. 2.3 means "howth is 2.3x faster").
    pub speedup: f64,
}

/// Complete test benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestBenchReport {
    /// Schema version.
    pub schema_version: u32,
    /// Machine information.
    pub machine: MachineInfo,
    /// Benchmark parameters.
    pub params: TestBenchParams,
    /// Project info.
    pub project: TestProjectInfo,
    /// Per-tool results.
    pub results: Vec<TestToolResult>,
    /// Comparisons (howth vs each other tool).
    pub comparisons: Vec<TestComparison>,
    /// Warnings encountered.
    pub warnings: Vec<BenchWarning>,
}

/// Run the test benchmark.
///
/// Creates a temporary test project with generated test files and benchmarks
/// howth test, node --test, and bun test. Starts a daemon for howth benchmarks
/// so the warm Node worker pool is used.
#[must_use]
pub fn run_test_bench(params: TestBenchParams) -> TestBenchReport {
    let mut warnings = Vec::new();

    if params.iters < 3 {
        warnings.push(BenchWarning::warn(
            "LOW_ITERS",
            format!(
                "Low iteration count ({}); results may be noisy",
                params.iters
            ),
        ));
    }

    // Create temp project with test files
    let temp_dir = create_test_project();
    let project_dir = temp_dir.path().to_path_buf();

    let total_tests = NUM_TEST_FILES * TESTS_PER_FILE;
    let project_info = TestProjectInfo {
        name: "bench-test-project".to_string(),
        test_files: NUM_TEST_FILES,
        test_cases: total_tests,
    };

    // Copy project into per-tool subdirs
    let work_dir = tempfile::tempdir().expect("Failed to create work directory");
    let howth_dir = work_dir.path().join("howth");
    let node_dir = work_dir.path().join("node");
    let bun_dir = work_dir.path().join("bun");

    copy_dir_contents(&project_dir, &howth_dir);
    copy_dir_contents(&project_dir, &node_dir);
    copy_dir_contents(&project_dir, &bun_dir);

    // For node: pre-generate .mjs files since node can't run .ts directly
    generate_js_test_files(&node_dir);

    // For bun: generate bun:test files since bun test doesn't support node:test
    generate_bun_test_files(&bun_dir);

    // Start a daemon for howth benchmarks (warm worker pool)
    let daemon_ctx = start_bench_daemon(&mut warnings);

    // Run benchmarks for each tool
    let mut results = Vec::new();

    if let Some(r) = bench_howth(&howth_dir, &params, &mut warnings, daemon_ctx.as_ref()) {
        results.push(r);
    }
    if let Some(r) = bench_node(&node_dir, &params, &mut warnings) {
        results.push(r);
    }
    if let Some(r) = bench_bun(&bun_dir, &params, &mut warnings) {
        results.push(r);
    }

    // Stop daemon
    if let Some(mut ctx) = daemon_ctx {
        let _ = ctx.child.kill();
        let _ = ctx.child.wait();
        let _ = fs::remove_file(&ctx.endpoint);
    }

    // Compute comparisons (howth vs others)
    let comparisons = compute_comparisons(&results);

    TestBenchReport {
        schema_version: TEST_BENCH_SCHEMA_VERSION,
        machine: MachineInfo::detect(),
        params,
        project: project_info,
        results,
        comparisons,
        warnings,
    }
}

/// Context for a benchmark daemon instance.
struct BenchDaemonCtx {
    child: std::process::Child,
    endpoint: String,
}

/// Start a daemon for benchmarking. Returns None if howth is not available.
fn start_bench_daemon(warnings: &mut Vec<BenchWarning>) -> Option<BenchDaemonCtx> {
    if !tool_available("howth") {
        return None;
    }

    // Create a unique socket path for the bench daemon
    #[cfg(unix)]
    let endpoint = format!("/tmp/howth-bench-test-{}.sock", std::process::id());
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\howth-bench-test-{}", std::process::id());

    // Clean up any stale socket (Unix only - Windows named pipes are cleaned up automatically)
    #[cfg(unix)]
    let _ = fs::remove_file(&endpoint);

    eprintln!("  Starting daemon for howth benchmarks...");

    let child = Command::new("howth")
        .arg("daemon")
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    let child = match child {
        Ok(c) => c,
        Err(e) => {
            warnings.push(BenchWarning::warn(
                "DAEMON_START_FAILED",
                format!("Failed to start daemon: {e}"),
            ));
            return None;
        }
    };

    // Wait for daemon to be ready (poll with ping)
    let ready = wait_for_daemon_ready(&endpoint);
    if ready {
        eprintln!("  Daemon ready.");
    } else {
        warnings.push(BenchWarning::warn(
            "DAEMON_NOT_READY",
            "Daemon did not become ready; howth bench will use fallback path",
        ));
        // Don't return None — we still have the child to clean up
    }

    Some(BenchDaemonCtx { child, endpoint })
}

/// Wait for the daemon to accept connections (up to 5 seconds).
fn wait_for_daemon_ready(endpoint: &str) -> bool {
    for _ in 0..50 {
        let result = Command::new("howth")
            .arg("ping")
            .env("HOWTH_IPC_ENDPOINT", endpoint)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if let Ok(status) = result {
            if status.success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

/// Benchmark howth test.
fn bench_howth(
    project_dir: &Path,
    params: &TestBenchParams,
    warnings: &mut Vec<BenchWarning>,
    daemon_ctx: Option<&BenchDaemonCtx>,
) -> Option<TestToolResult> {
    if !tool_available("howth") {
        warnings.push(BenchWarning::info(
            "TOOL_MISSING",
            "howth not found in PATH, skipping",
        ));
        return None;
    }

    eprintln!("  Benchmarking howth test...");

    // If we have a daemon, pass its endpoint so howth uses the warm worker pool
    let env_vars: Vec<(&str, &str)> = match daemon_ctx {
        Some(ctx) => vec![("HOWTH_IPC_ENDPOINT", ctx.endpoint.as_str())],
        None => vec![],
    };

    run_bench_iterations_with_env(
        "howth",
        "howth test",
        &["test"],
        project_dir,
        params,
        warnings,
        false,
        &env_vars,
    )
}

/// Benchmark node --test.
fn bench_node(
    project_dir: &Path,
    params: &TestBenchParams,
    warnings: &mut Vec<BenchWarning>,
) -> Option<TestToolResult> {
    if !tool_available("node") {
        warnings.push(BenchWarning::info(
            "TOOL_MISSING",
            "node not found in PATH, skipping",
        ));
        return None;
    }

    eprintln!("  Benchmarking node --test...");

    // Collect .test.mjs files
    let test_files: Vec<String> = fs::read_dir(project_dir)
        .ok()?
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".test.mjs"))
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    if test_files.is_empty() {
        warnings.push(BenchWarning::warn(
            "NO_TEST_FILES",
            "No .test.mjs files found for node benchmark",
        ));
        return None;
    }

    let mut args: Vec<&str> = vec!["--test"];
    let file_refs: Vec<&str> = test_files.iter().map(std::string::String::as_str).collect();
    args.extend(file_refs);

    run_bench_iterations(
        "node",
        "node --test",
        &args,
        project_dir,
        params,
        warnings,
        true,
    )
}

/// Benchmark bun test.
fn bench_bun(
    project_dir: &Path,
    params: &TestBenchParams,
    warnings: &mut Vec<BenchWarning>,
) -> Option<TestToolResult> {
    if !tool_available("bun") {
        warnings.push(BenchWarning::info(
            "TOOL_MISSING",
            "bun not found in PATH, skipping",
        ));
        return None;
    }

    eprintln!("  Benchmarking bun test...");
    // bun test runs .ts files natively and supports node:test
    run_bench_iterations(
        "bun",
        "bun test",
        &["test"],
        project_dir,
        params,
        warnings,
        true,
    )
}

/// Run benchmark iterations for a tool.
fn run_bench_iterations(
    tool_name: &str,
    display_cmd: &str,
    args: &[&str],
    project_dir: &Path,
    params: &TestBenchParams,
    warnings: &mut Vec<BenchWarning>,
    capture_rusage: bool,
) -> Option<TestToolResult> {
    run_bench_iterations_with_env(
        tool_name,
        display_cmd,
        args,
        project_dir,
        params,
        warnings,
        capture_rusage,
        &[],
    )
}

/// Run benchmark iterations for a tool with optional environment variables.
fn run_bench_iterations_with_env(
    tool_name: &str,
    display_cmd: &str,
    args: &[&str],
    project_dir: &Path,
    params: &TestBenchParams,
    warnings: &mut Vec<BenchWarning>,
    capture_rusage: bool,
    env_vars: &[(&str, &str)],
) -> Option<TestToolResult> {
    let cmd = tool_name;

    // Warmup runs
    for i in 0..params.warmup {
        eprintln!("    warmup {}/{}", i + 1, params.warmup);
        let mut c = Command::new(cmd);
        c.args(args).current_dir(project_dir);
        for (k, v) in env_vars {
            c.env(k, v);
        }
        let _ = c.output();
    }

    // Measured runs
    let mut samples = Vec::with_capacity(params.iters as usize);
    let mut cpu_samples = Vec::with_capacity(params.iters as usize);
    let mut peak_rss: u64 = 0;

    for i in 0..params.iters {
        eprintln!("    run {}/{}", i + 1, params.iters);

        let ru_before = if capture_rusage {
            rusage::snapshot_children()
        } else {
            None
        };
        let start = Instant::now();
        let mut c = Command::new(cmd);
        c.args(args).current_dir(project_dir);
        for (k, v) in env_vars {
            c.env(k, v);
        }
        let output = c.output();
        let elapsed = start.elapsed();

        if let Ok(ref out) = output {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                warnings.push(BenchWarning::warn(
                    "TEST_FAILED",
                    format!(
                        "{tool_name} test failed (exit {}): {}{}",
                        out.status,
                        stderr.chars().take(200).collect::<String>(),
                        if stderr.is_empty() {
                            format!(" stdout: {}", stdout.chars().take(200).collect::<String>())
                        } else {
                            String::new()
                        }
                    ),
                ));
                return None;
            }
        }

        samples.push(elapsed.as_nanos() as u64);

        if let (Some(before), Some(after)) = (ru_before, rusage::snapshot_children()) {
            let d = rusage::delta(&before, &after);
            cpu_samples.push(d.total_cpu_us());
            peak_rss = peak_rss.max(d.max_rss);
        }
    }

    let stats = compute_stats(&samples);
    Some(TestToolResult {
        tool: tool_name.to_string(),
        command: display_cmd.to_string(),
        median_ns: stats.median_ns,
        p95_ns: stats.p95_ns,
        min_ns: stats.min_ns,
        max_ns: stats.max_ns,
        samples: params.iters,
        median_cpu_us: if cpu_samples.is_empty() {
            None
        } else {
            Some(compute_median(&cpu_samples))
        },
        peak_rss_bytes: if peak_rss > 0 { Some(peak_rss) } else { None },
    })
}

/// Create a temporary test project with generated test files.
fn create_test_project() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let path = temp_dir.path();

    eprintln!(
        "Creating test project with {NUM_TEST_FILES} test files ({} test cases)...",
        NUM_TEST_FILES * TESTS_PER_FILE
    );

    // Write package.json
    let package_json = r#"{
  "name": "bench-test-project",
  "version": "1.0.0",
  "private": true,
  "type": "module"
}
"#;
    fs::write(path.join("package.json"), package_json).expect("Failed to write package.json");

    // Generate .test.ts files
    for i in 0..NUM_TEST_FILES {
        let content = generate_test_file_ts(i);
        let filename = format!("{}.test.ts", test_module_name(i));
        fs::write(path.join(&filename), content).expect("Failed to write test file");
    }

    temp_dir
}

/// Generate .test.mjs files in the node dir (JavaScript equivalents of the .ts files).
fn generate_js_test_files(project_dir: &Path) {
    for i in 0..NUM_TEST_FILES {
        let content = generate_test_file_js(i);
        let filename = format!("{}.test.mjs", test_module_name(i));
        fs::write(project_dir.join(&filename), content).expect("Failed to write .test.mjs file");
    }
}

/// Generate bun:test files in the bun dir (replaces node:test .ts files).
fn generate_bun_test_files(project_dir: &Path) {
    // Remove the node:test .ts files
    for i in 0..NUM_TEST_FILES {
        let filename = format!("{}.test.ts", test_module_name(i));
        let _ = fs::remove_file(project_dir.join(&filename));
    }
    // Write bun:test .ts files
    for i in 0..NUM_TEST_FILES {
        let content = generate_test_file_bun(i);
        let filename = format!("{}.test.ts", test_module_name(i));
        fs::write(project_dir.join(&filename), content).expect("Failed to write bun test file");
    }
}

/// Module name for test file index.
fn test_module_name(index: u32) -> String {
    let names = [
        "math-utils",
        "string-helpers",
        "array-ops",
        "date-format",
        "validator",
        "parser",
        "encoder",
        "converter",
        "sorter",
        "filter",
        "mapper",
        "reducer",
        "cache",
        "queue",
        "stack",
    ];
    if (index as usize) < names.len() {
        names[index as usize].to_string()
    } else {
        format!("module-{index}")
    }
}

/// Generate a TypeScript test file using node:test.
fn generate_test_file_ts(index: u32) -> String {
    let module_name = test_module_name(index);
    let mut content = String::from(
        "import { test, describe } from 'node:test';\nimport assert from 'node:assert/strict';\n\n",
    );

    content.push_str(&format!("describe('{module_name}', () => {{\n"));

    for t in 0..TESTS_PER_FILE {
        let (test_name, test_body) = generate_test_case(index, t);
        content.push_str(&format!("  test('{test_name}', () => {{\n"));
        content.push_str(&test_body);
        content.push_str("  });\n\n");
    }

    content.push_str("});\n");
    content
}

/// Generate a JavaScript (.mjs) test file using node:test.
fn generate_test_file_js(index: u32) -> String {
    // Same as TS but without type annotations (our generated tests don't use any)
    generate_test_file_ts(index)
}

/// Generate a TypeScript test file using bun:test (describe/test/expect).
fn generate_test_file_bun(index: u32) -> String {
    let module_name = test_module_name(index);
    let mut content = String::from("import { test, describe, expect } from 'bun:test';\n\n");

    content.push_str(&format!("describe(\"{module_name}\", () => {{\n"));

    for t in 0..TESTS_PER_FILE {
        let (test_name, test_body) = generate_test_case_bun(index, t);
        content.push_str(&format!("  test(\"{test_name}\", () => {{\n"));
        content.push_str(&test_body);
        content.push_str("  });\n\n");
    }

    content.push_str("});\n");
    content
}

/// Generate a single test case body for bun:test (uses expect instead of assert).
fn generate_test_case_bun(file_index: u32, test_index: u32) -> (String, String) {
    let seed = file_index * TESTS_PER_FILE + test_index;

    match seed % 6 {
        0 => {
            let a = seed + 1;
            let b = seed + 2;
            (
                format!("adds {a} + {b} correctly"),
                format!(
                    "    const result = {} + {};\n    expect(result).toBe({});\n",
                    a,
                    b,
                    a + b
                ),
            )
        }
        1 => {
            let vals: Vec<u32> = (0..5).map(|i| (seed + i) * 3).collect();
            let sorted = {
                let mut v = vals.clone();
                v.sort_unstable();
                v
            };
            (
                format!("sorts array starting at {}", vals[0]),
                format!(
                    "    const arr = {vals:?};\n    arr.sort((a, b) => a - b);\n    expect(arr).toEqual({sorted:?});\n"
                ),
            )
        }
        2 => {
            let word = match seed % 4 {
                0 => "hello",
                1 => "world",
                2 => "benchmark",
                _ => "testing",
            };
            (
                format!("converts {word} to uppercase"),
                format!(
                    "    const str = \"{}\";\n    expect(str.toUpperCase()).toBe(\"{}\");\n",
                    word,
                    word.to_uppercase()
                ),
            )
        }
        3 => {
            let len = (seed % 10) + 3;
            (
                format!("creates array of length {len}"),
                format!(
                    "    const arr = Array.from({{ length: {} }}, (_, i) => i);\n    expect(arr.length).toBe({});\n    expect(arr[0]).toBe(0);\n    expect(arr[arr.length - 1]).toBe({});\n",
                    len, len, len - 1
                ),
            )
        }
        4 => {
            let key = format!("key_{seed}");
            let val = seed * 7;
            (
                format!("handles object property {key}"),
                format!(
                    "    const obj = {{ \"{key}\": {val} }};\n    expect(obj[\"{key}\"]).toBe({val});\n    expect(Object.hasOwn(obj, \"{key}\")).toBe(true);\n"
                ),
            )
        }
        _ => {
            let input = format!("test-string-{seed}");
            (
                format!("checks string includes {seed}"),
                format!(
                    "    const str = \"{}\";\n    expect(str.includes(\"{}\")).toBe(true);\n    expect(str.length).toBe({});\n",
                    input, seed, input.len()
                ),
            )
        }
    }
}

/// Generate a single test case body.
fn generate_test_case(file_index: u32, test_index: u32) -> (String, String) {
    let seed = file_index * TESTS_PER_FILE + test_index;

    match seed % 6 {
        0 => {
            let a = seed + 1;
            let b = seed + 2;
            (
                format!("adds {a} + {b} correctly"),
                format!(
                    "    const result = {} + {};\n    assert.strictEqual(result, {});\n",
                    a,
                    b,
                    a + b
                ),
            )
        }
        1 => {
            let vals: Vec<u32> = (0..5).map(|i| (seed + i) * 3).collect();
            let sorted = {
                let mut v = vals.clone();
                v.sort_unstable();
                v
            };
            (
                format!("sorts array starting at {}", vals[0]),
                format!(
                    "    const arr = {vals:?};\n    arr.sort((a, b) => a - b);\n    assert.deepStrictEqual(arr, {sorted:?});\n"
                ),
            )
        }
        2 => {
            let word = match seed % 4 {
                0 => "hello",
                1 => "world",
                2 => "benchmark",
                _ => "testing",
            };
            (
                format!("converts {word} to uppercase"),
                format!(
                    "    const str = \"{}\";\n    assert.strictEqual(str.toUpperCase(), \"{}\");\n",
                    word,
                    word.to_uppercase()
                ),
            )
        }
        3 => {
            let len = (seed % 10) + 3;
            (
                format!("creates array of length {len}"),
                format!(
                    "    const arr = Array.from({{ length: {} }}, (_, i) => i);\n    assert.strictEqual(arr.length, {});\n    assert.strictEqual(arr[0], 0);\n    assert.strictEqual(arr[arr.length - 1], {});\n",
                    len, len, len - 1
                ),
            )
        }
        4 => {
            let key = format!("key_{seed}");
            let val = seed * 7;
            (
                format!("handles object property {key}"),
                format!(
                    "    const obj = {{ \"{key}\": {val} }};\n    assert.strictEqual(obj[\"{key}\"], {val});\n    assert.ok(Object.hasOwn(obj, \"{key}\"));\n"
                ),
            )
        }
        _ => {
            let input = format!("test-string-{seed}");
            (
                format!("checks string includes {seed}"),
                format!(
                    "    const str = \"{}\";\n    assert.ok(str.includes(\"{}\"));\n    assert.strictEqual(str.length, {});\n",
                    input, seed, input.len()
                ),
            )
        }
    }
}

/// Compute comparisons between howth and other tools.
fn compute_comparisons(results: &[TestToolResult]) -> Vec<TestComparison> {
    let howth_median = results
        .iter()
        .find(|r| r.tool == "howth")
        .map(|r| r.median_ns);

    let Some(howth_ns) = howth_median else {
        return Vec::new();
    };

    if howth_ns == 0 {
        return Vec::new();
    }

    results
        .iter()
        .filter(|r| r.tool != "howth")
        .map(|r| TestComparison {
            tool: r.tool.clone(),
            speedup: r.median_ns as f64 / howth_ns as f64,
        })
        .collect()
}

/// Check if a tool is available in PATH.
fn tool_available(tool: &str) -> bool {
    Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Recursively copy directory contents (skips `node_modules`).
fn copy_dir_contents(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("Failed to create destination directory");
    for entry in fs::read_dir(src).expect("Failed to read source directory") {
        let entry = entry.expect("Failed to read directory entry");
        let file_type = entry.file_type().expect("Failed to get file type");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            if entry.file_name() == "node_modules" {
                continue;
            }
            copy_dir_contents(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).expect("Failed to copy file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version() {
        assert_eq!(TEST_BENCH_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_module_names() {
        assert_eq!(test_module_name(0), "math-utils");
        assert_eq!(test_module_name(14), "stack");
        assert_eq!(test_module_name(15), "module-15");
    }

    #[test]
    fn test_generate_ts_file() {
        let content = generate_test_file_ts(0);
        assert!(content.contains("import { test, describe } from 'node:test'"));
        assert!(content.contains("import assert from 'node:assert/strict'"));
        assert!(content.contains("describe('math-utils'"));
    }

    #[test]
    fn test_generate_js_file() {
        let content = generate_test_file_js(0);
        // JS and TS are identical for our generated tests
        assert!(content.contains("import { test, describe } from 'node:test'"));
    }

    #[test]
    fn test_generate_test_cases_variety() {
        // Ensure different test case patterns are generated
        let mut bodies = Vec::new();
        for i in 0..6 {
            let (name, body) = generate_test_case(0, i);
            assert!(!name.is_empty());
            assert!(!body.is_empty());
            bodies.push(body);
        }
        // Not all bodies should be identical
        let unique: std::collections::HashSet<&String> = bodies.iter().collect();
        assert!(unique.len() > 1);
    }

    #[test]
    fn test_compute_comparisons_basic() {
        let results = vec![
            TestToolResult {
                tool: "howth".to_string(),
                command: "howth test".to_string(),
                median_ns: 500_000_000,
                p95_ns: 600_000_000,
                min_ns: 400_000_000,
                max_ns: 700_000_000,
                samples: 5,
                median_cpu_us: None,
                peak_rss_bytes: None,
            },
            TestToolResult {
                tool: "node".to_string(),
                command: "node --test".to_string(),
                median_ns: 1_000_000_000,
                p95_ns: 1_200_000_000,
                min_ns: 800_000_000,
                max_ns: 1_500_000_000,
                samples: 5,
                median_cpu_us: None,
                peak_rss_bytes: None,
            },
        ];

        let comparisons = compute_comparisons(&results);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].tool, "node");
        assert!((comparisons[0].speedup - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_comparisons_no_howth() {
        let results = vec![TestToolResult {
            tool: "node".to_string(),
            command: "node --test".to_string(),
            median_ns: 1_000_000_000,
            p95_ns: 1_200_000_000,
            min_ns: 800_000_000,
            max_ns: 1_500_000_000,
            samples: 5,
            median_cpu_us: None,
            peak_rss_bytes: None,
        }];

        let comparisons = compute_comparisons(&results);
        assert!(comparisons.is_empty());
    }

    #[test]
    fn test_create_test_project() {
        let temp = create_test_project();
        let path = temp.path();

        assert!(path.join("package.json").exists());
        assert!(path.join("math-utils.test.ts").exists());
        assert!(path.join("string-helpers.test.ts").exists());

        // Count test files
        let test_files: Vec<_> = fs::read_dir(path)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.ends_with(".test.ts"))
            })
            .collect();
        assert_eq!(test_files.len(), NUM_TEST_FILES as usize);
    }
}
