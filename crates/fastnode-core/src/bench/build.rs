//! Build benchmark harness for fastnode.
//!
//! Provides benchmarks for transpile and devloop performance, designed for
//! screenshot-able performance tables.
//!
//! ## Benchmark Cases
//!
//! - **cold**: Full build with no cache (clears `.howth/cache`)
//! - **warm_noop**: Cached build (should be ~instant)
//! - **warm_1_change**: Touch one file, rebuild
//! - **watch_ttg**: Watch mode time-to-green (file change → build complete)

use crate::bench::{compute_stats, BenchStats, BenchWarning};
use crate::build::{
    build_graph_from_project, execute_graph_with_file_cache, ExecOptions, InMemoryFileHashCache,
    MemoryCache,
};
use crate::compiler::SwcBackend;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Schema version for build benchmark reports.
pub const BUILD_BENCH_SCHEMA_VERSION: u32 = 1;

/// Benchmark target type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchTarget {
    /// Transpile benchmark (transpile only).
    Transpile,
    /// Devloop benchmark (full dev loop with watch).
    Devloop,
}

impl BenchTarget {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transpile => "transpile",
            Self::Devloop => "devloop",
        }
    }
}

impl std::fmt::Display for BenchTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Parameters for build benchmarks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildBenchParams {
    /// Benchmark target.
    pub target: BenchTarget,
    /// Number of measured iterations.
    pub iters: u32,
    /// Number of warmup iterations (not measured).
    pub warmup: u32,
}

impl Default for BuildBenchParams {
    fn default() -> Self {
        Self {
            target: BenchTarget::Transpile,
            iters: 10,
            warmup: 2,
        }
    }
}

/// Machine information for the benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineInfo {
    /// CPU description (e.g., "Apple M2 Pro").
    pub cpu: String,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// Number of CPU cores.
    pub cores: u32,
}

impl MachineInfo {
    /// Detect machine info from the current system.
    #[must_use]
    pub fn detect() -> Self {
        let cpu = detect_cpu_name();
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let cores = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1) as u32;

        Self {
            cpu,
            os,
            arch,
            cores,
        }
    }
}

/// Work-done statistics for a benchmark case.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkDoneStats {
    /// Number of nodes executed (not cached).
    pub nodes_executed: u32,
    /// Number of nodes served from cache.
    pub nodes_cached: u32,
    /// Number of files transpiled.
    pub files_transpiled: Option<u32>,
}

/// Result for a single benchmark case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildBenchResult {
    /// Case name (e.g., "cold", "warm_noop").
    pub case: String,
    /// Median time in nanoseconds.
    pub median_ns: u64,
    /// 95th percentile time in nanoseconds.
    pub p95_ns: u64,
    /// Minimum time in nanoseconds.
    pub min_ns: u64,
    /// Maximum time in nanoseconds.
    pub max_ns: u64,
    /// Number of samples.
    pub samples: u32,
    /// Number of files processed (for context).
    pub files_count: Option<u32>,
    /// Work-done statistics (from last run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done: Option<WorkDoneStats>,
}

impl BuildBenchResult {
    /// Create from stats.
    #[must_use]
    pub fn new(case: impl Into<String>, samples: u32, stats: BenchStats) -> Self {
        Self {
            case: case.into(),
            median_ns: stats.median_ns,
            p95_ns: stats.p95_ns,
            min_ns: stats.min_ns,
            max_ns: stats.max_ns,
            samples,
            files_count: None,
            work_done: None,
        }
    }

    /// Set files count.
    #[must_use]
    pub fn with_files_count(mut self, count: u32) -> Self {
        self.files_count = Some(count);
        self
    }

    /// Set work-done stats.
    #[must_use]
    pub fn with_work_done(mut self, work_done: WorkDoneStats) -> Self {
        self.work_done = Some(work_done);
        self
    }
}

/// Result for a baseline comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResult {
    /// Baseline name (e.g., "tsc --noEmit").
    pub name: String,
    /// Exact command that was run.
    pub command: String,
    /// Median time in nanoseconds.
    pub median_ns: u64,
}

/// Complete build benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildBenchReport {
    /// Schema version.
    pub schema_version: u32,
    /// Benchmark target.
    pub target: String,
    /// Machine information.
    pub machine: MachineInfo,
    /// Benchmark parameters.
    pub params: BuildBenchParams,
    /// Benchmark results by case.
    pub results: Vec<BuildBenchResult>,
    /// Baseline comparisons.
    pub baselines: Vec<BaselineResult>,
    /// Warnings encountered.
    pub warnings: Vec<BenchWarning>,
}

impl BuildBenchReport {
    /// Create a new report.
    #[must_use]
    pub fn new(params: BuildBenchParams) -> Self {
        Self {
            schema_version: BUILD_BENCH_SCHEMA_VERSION,
            target: params.target.to_string(),
            machine: MachineInfo::detect(),
            params,
            results: Vec::new(),
            baselines: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a result.
    pub fn add_result(&mut self, result: BuildBenchResult) {
        self.results.push(result);
    }

    /// Add a baseline.
    pub fn add_baseline(&mut self, baseline: BaselineResult) {
        self.baselines.push(baseline);
    }

    /// Add a warning.
    pub fn add_warning(&mut self, warning: BenchWarning) {
        self.warnings.push(warning);
    }
}

/// Run build benchmarks on a project directory.
///
/// If `project_path` is None, creates a temporary test project.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn run_build_bench(params: BuildBenchParams, project_path: Option<&Path>) -> BuildBenchReport {
    let mut report = BuildBenchReport::new(params.clone());

    // Validate params
    if params.iters < 3 {
        report.add_warning(BenchWarning::warn(
            "LOW_ITERS",
            format!("Low iteration count ({}); results may be noisy", params.iters),
        ));
    }

    // Use provided project or create temp project
    let (temp_dir, project_dir) = if let Some(path) = project_path {
        (None, path.to_path_buf())
    } else {
        let temp = create_temp_project();
        let path = temp.path().to_path_buf();
        (Some(temp), path)
    };

    // Check if project has transpilable content
    if !project_dir.join("src").exists() {
        report.add_warning(BenchWarning::warn(
            "NO_SRC_DIR",
            "Project has no src/ directory",
        ));
    }

    // Build the graph to count files
    let files_count = match build_graph_from_project(&project_dir) {
        Ok(graph) => graph
            .get_node("transpile")
            .and_then(|n| n.transpile.as_ref())
            .map(|_| count_transpilable_files(&project_dir.join("src"))),
        Err(_) => None,
    };

    // Determine targets based on benchmark type
    // Transpile benchmark: only run transpile node (no typecheck)
    // Devloop benchmark: run all nodes (transpile + typecheck)
    let targets: Vec<String> = match params.target {
        BenchTarget::Transpile => vec!["transpile".to_string()],
        BenchTarget::Devloop => vec![], // Empty = all nodes
    };

    // Run benchmarks based on target
    match params.target {
        BenchTarget::Transpile => {
            // Cold benchmark
            let cold_result = run_cold_bench(&project_dir, params.iters, params.warmup, &targets);
            let mut cold_result = cold_result;
            if let Some(count) = files_count {
                cold_result = cold_result.with_files_count(count);
            }
            report.add_result(cold_result);

            // Warm noop benchmark
            let warm_noop_result = run_warm_noop_bench(&project_dir, params.iters, params.warmup, &targets);
            report.add_result(warm_noop_result);

            // Warm 1-change benchmark
            let warm_change_result =
                run_warm_1_change_bench(&project_dir, params.iters, params.warmup, &targets);
            let warm_change_result = warm_change_result.with_files_count(1);
            report.add_result(warm_change_result);

            // Run baselines
            if let Some(baseline) = run_tsc_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
            if let Some(baseline) = run_esbuild_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
            if let Some(baseline) = run_swc_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
        }
        BenchTarget::Devloop => {
            // Cold benchmark
            let cold_result = run_cold_bench(&project_dir, params.iters, params.warmup, &targets);
            let mut cold_result = cold_result;
            if let Some(count) = files_count {
                cold_result = cold_result.with_files_count(count);
            }
            report.add_result(cold_result);

            // Warm noop benchmark
            let warm_noop_result = run_warm_noop_bench(&project_dir, params.iters, params.warmup, &targets);
            report.add_result(warm_noop_result);

            // Warm 1-change benchmark
            let warm_change_result =
                run_warm_1_change_bench(&project_dir, params.iters, params.warmup, &targets);
            let warm_change_result = warm_change_result.with_files_count(1);
            report.add_result(warm_change_result);

            // Watch TTG benchmark (the killer metric)
            let watch_ttg_result =
                run_watch_ttg_bench(&project_dir, params.iters, params.warmup, &targets);
            let watch_ttg_result = watch_ttg_result.with_files_count(1);
            report.add_result(watch_ttg_result);

            // Run baselines
            if let Some(baseline) = run_tsc_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
            if let Some(baseline) = run_esbuild_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
            if let Some(baseline) = run_swc_baseline(&project_dir) {
                report.add_baseline(baseline);
            }
        }
    }

    // Clean up temp dir reference (it will drop automatically)
    drop(temp_dir);

    report
}

/// Run cold benchmark (no cache).
#[allow(clippy::cast_possible_truncation)]
fn run_cold_bench(project_dir: &Path, iters: u32, warmup: u32, targets: &[String]) -> BuildBenchResult {
    let mut samples = Vec::with_capacity(iters as usize);
    let backend = SwcBackend::new();
    let mut last_work_done = WorkDoneStats::default();

    // Warmup runs - each with fresh caches (simulating cold builds)
    for _ in 0..warmup {
        let mut cache = MemoryCache::new();
        let file_cache = InMemoryFileHashCache::new();
        let options = ExecOptions::new().with_targets(targets.to_vec());

        if let Ok(graph) = build_graph_from_project(project_dir) {
            let _ = execute_graph_with_file_cache(
                &graph,
                Some(&mut cache),
                &options,
                Some(&backend),
                Some(&file_cache),
            );
        }
    }

    // Measured runs - each with fresh caches (cold build)
    for _ in 0..iters {
        // Fresh caches each iteration for true cold measurement
        let mut cache = MemoryCache::new();
        let file_cache = InMemoryFileHashCache::new();
        let options = ExecOptions::new().with_targets(targets.to_vec());

        let start = Instant::now();
        if let Ok(graph) = build_graph_from_project(project_dir) {
            if let Ok(result) = execute_graph_with_file_cache(
                &graph,
                Some(&mut cache),
                &options,
                Some(&backend),
                Some(&file_cache),
            ) {
                // Track work done from last run
                last_work_done = WorkDoneStats {
                    nodes_executed: result.summary.nodes_run as u32,
                    nodes_cached: result.summary.cache_hits as u32,
                    files_transpiled: Some(count_transpilable_files(&project_dir.join("src"))),
                };
            }
        }
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    let stats = compute_stats(&samples);
    BuildBenchResult::new("cold", iters, stats).with_work_done(last_work_done)
}

/// Run warm noop benchmark (cached, no changes).
#[allow(clippy::cast_possible_truncation)]
fn run_warm_noop_bench(project_dir: &Path, iters: u32, warmup: u32, targets: &[String]) -> BuildBenchResult {
    let mut samples = Vec::with_capacity(iters as usize);
    let backend = SwcBackend::new();
    let mut last_work_done = WorkDoneStats::default();

    // Build graph once - graph construction is not part of warm benchmark
    let graph = match build_graph_from_project(project_dir) {
        Ok(g) => g,
        Err(_) => {
            return BuildBenchResult::new("warm_noop", iters, compute_stats(&[0]));
        }
    };

    // Create caches that persist across all iterations
    let mut cache = MemoryCache::new();
    let file_cache = InMemoryFileHashCache::new();
    let options = ExecOptions::new().with_targets(targets.to_vec());

    // Pre-warm the caches
    let _ = execute_graph_with_file_cache(
        &graph,
        Some(&mut cache),
        &options,
        Some(&backend),
        Some(&file_cache),
    );

    // Warmup runs (with same caches and graph)
    for _ in 0..warmup {
        let _ = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        );
    }

    // Measured runs - only time execution, not graph construction
    // File hash cache should make this very fast
    for _ in 0..iters {
        let start = Instant::now();
        if let Ok(result) = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        ) {
            last_work_done = WorkDoneStats {
                nodes_executed: result.summary.nodes_run as u32,
                nodes_cached: result.summary.cache_hits as u32,
                files_transpiled: None, // No files transpiled (all cached)
            };
        }
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    let stats = compute_stats(&samples);
    BuildBenchResult::new("warm_noop", iters, stats).with_work_done(last_work_done)
}

/// Run warm 1-change benchmark (cached, touch one file).
#[allow(clippy::cast_possible_truncation)]
fn run_warm_1_change_bench(project_dir: &Path, iters: u32, warmup: u32, targets: &[String]) -> BuildBenchResult {
    let mut samples = Vec::with_capacity(iters as usize);
    let backend = SwcBackend::new();
    let mut last_work_done = WorkDoneStats::default();

    // Find a file to touch
    let touch_file = find_file_to_touch(&project_dir.join("src"));

    // Build graph once - graph structure doesn't change when file contents change
    let graph = match build_graph_from_project(project_dir) {
        Ok(g) => g,
        Err(_) => {
            return BuildBenchResult::new("warm_1_change", iters, compute_stats(&[0]));
        }
    };

    // Create caches that persist across iterations
    // File hash cache will automatically invalidate for touched files (mtime changes)
    let mut cache = MemoryCache::new();
    let file_cache = InMemoryFileHashCache::new();
    let options = ExecOptions::new().with_targets(targets.to_vec());

    // Pre-warm the caches
    let _ = execute_graph_with_file_cache(
        &graph,
        Some(&mut cache),
        &options,
        Some(&backend),
        Some(&file_cache),
    );

    // Warmup runs
    for _ in 0..warmup {
        if let Some(ref file) = touch_file {
            touch_file_content(file);
        }
        // Keep same caches - file content change invalidates that file's hash cache entry
        let _ = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        );
    }

    // Measured runs - only time execution, not graph construction
    // File hash cache should make unchanged files very fast
    for _ in 0..iters {
        if let Some(ref file) = touch_file {
            touch_file_content(file);
        }
        // Keep same caches - measures incremental rebuild after 1 file change

        let start = Instant::now();
        if let Ok(result) = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        ) {
            last_work_done = WorkDoneStats {
                nodes_executed: result.summary.nodes_run as u32,
                nodes_cached: result.summary.cache_hits as u32,
                files_transpiled: Some(1), // One file changed
            };
        }
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    let stats = compute_stats(&samples);
    BuildBenchResult::new("warm_1_change", iters, stats).with_work_done(last_work_done)
}

/// Run watch time-to-green benchmark.
///
/// Simulates watch mode: measures time from file change to build completion.
/// This is the "killer metric" - what users actually experience in watch mode.
#[allow(clippy::cast_possible_truncation)]
fn run_watch_ttg_bench(project_dir: &Path, iters: u32, warmup: u32, targets: &[String]) -> BuildBenchResult {
    let mut samples = Vec::with_capacity(iters as usize);
    let backend = SwcBackend::new();
    let mut last_work_done = WorkDoneStats::default();

    // Find a file to touch
    let touch_file = find_file_to_touch(&project_dir.join("src"));

    // Build graph once - in watch mode, graph is persistent
    let graph = match build_graph_from_project(project_dir) {
        Ok(g) => g,
        Err(_) => {
            return BuildBenchResult::new("watch_ttg", iters, compute_stats(&[0]));
        }
    };

    // Create caches that persist across iterations (like real watch mode)
    let mut cache = MemoryCache::new();
    let file_cache = InMemoryFileHashCache::new();
    let options = ExecOptions::new().with_targets(targets.to_vec());

    // Pre-warm: do initial build to populate caches
    let _ = execute_graph_with_file_cache(
        &graph,
        Some(&mut cache),
        &options,
        Some(&backend),
        Some(&file_cache),
    );

    // Warmup runs
    for _ in 0..warmup {
        if let Some(ref file) = touch_file {
            touch_file_content(file);
        }
        // Simulate watch mode: keep same caches but rebuild changed file
        let _ = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        );
    }

    // Measured runs - simulate watch mode TTG:
    // 1. Touch file (simulates file change notification)
    // 2. Start timer immediately
    // 3. Rebuild with existing caches (graph already built, like real watch mode)
    // 4. Stop timer when build completes
    for _ in 0..iters {
        if let Some(ref file) = touch_file {
            touch_file_content(file);
        }

        // Start timing immediately after file change
        let start = Instant::now();

        if let Ok(result) = execute_graph_with_file_cache(
            &graph,
            Some(&mut cache),
            &options,
            Some(&backend),
            Some(&file_cache),
        ) {
            last_work_done = WorkDoneStats {
                nodes_executed: result.summary.nodes_run as u32,
                nodes_cached: result.summary.cache_hits as u32,
                files_transpiled: Some(1),
            };
        }

        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    let stats = compute_stats(&samples);
    BuildBenchResult::new("watch_ttg", iters, stats).with_work_done(last_work_done)
}

/// Resolve the tsc command to use.
///
/// Prefers local node_modules/.bin/tsc if present, otherwise uses npx --no-install.
/// This reduces variance from npx resolution and avoids surprise network calls.
fn resolve_tsc_command(project_dir: &Path) -> (String, Vec<String>) {
    let local_tsc = project_dir.join("node_modules/.bin/tsc");
    if local_tsc.exists() {
        (
            local_tsc.to_string_lossy().to_string(),
            vec!["--noEmit".to_string()],
        )
    } else {
        // Use npx --no-install to fail fast if tsc not installed
        (
            "npx".to_string(),
            vec![
                "--no-install".to_string(),
                "tsc".to_string(),
                "--noEmit".to_string(),
            ],
        )
    }
}

/// Run tsc --noEmit baseline.
#[allow(clippy::cast_possible_truncation)]
fn run_tsc_baseline(project_dir: &Path) -> Option<BaselineResult> {
    // Check if tsconfig.json exists
    if !project_dir.join("tsconfig.json").exists() {
        return None;
    }

    let (cmd, args) = resolve_tsc_command(project_dir);
    let exact_command = format!("{} {}", cmd, args.join(" "));

    // Run tsc --noEmit and time it
    let mut samples = Vec::with_capacity(3);

    for _ in 0..3 {
        let start = Instant::now();
        let _ = Command::new(&cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    let stats = compute_stats(&samples);
    Some(BaselineResult {
        name: "tsc --noEmit".to_string(),
        command: exact_command,
        median_ns: stats.median_ns,
    })
}

/// Run esbuild transpile baseline.
#[allow(clippy::cast_possible_truncation)]
fn run_esbuild_baseline(project_dir: &Path) -> Option<BaselineResult> {
    let esbuild_path = project_dir.join("node_modules/.bin/esbuild");
    if !esbuild_path.exists() {
        return None;
    }

    let src_dir = project_dir.join("src");
    if !src_dir.exists() {
        return None;
    }

    // Create temp output dir
    let out_dir = project_dir.join(".howth/bench-esbuild");
    let _ = fs::create_dir_all(&out_dir);

    let cmd = esbuild_path.to_string_lossy().to_string();
    let exact_command = format!("{} src/**/*.ts src/**/*.tsx --outdir=.howth/bench-esbuild --format=esm", cmd);

    // Collect all .ts and .tsx files
    let mut input_files = Vec::new();
    for entry in walkdir::WalkDir::new(&src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "ts" || ext == "tsx" {
                input_files.push(path.to_path_buf());
            }
        }
    }

    if input_files.is_empty() {
        return None;
    }

    // Run esbuild and time it
    let mut samples = Vec::with_capacity(3);

    for _ in 0..3 {
        let start = Instant::now();
        let mut args: Vec<String> = input_files.iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        args.push(format!("--outdir={}", out_dir.to_string_lossy()));
        args.push("--format=esm".to_string());

        let _ = Command::new(&cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    // Cleanup
    let _ = fs::remove_dir_all(&out_dir);

    let stats = compute_stats(&samples);
    Some(BaselineResult {
        name: "esbuild".to_string(),
        command: exact_command,
        median_ns: stats.median_ns,
    })
}

/// Run swc transpile baseline.
#[allow(clippy::cast_possible_truncation)]
fn run_swc_baseline(project_dir: &Path) -> Option<BaselineResult> {
    let swc_path = project_dir.join("node_modules/.bin/swc");
    if !swc_path.exists() {
        return None;
    }

    let src_dir = project_dir.join("src");
    if !src_dir.exists() {
        return None;
    }

    // Create temp output dir
    let out_dir = project_dir.join(".howth/bench-swc");
    let _ = fs::create_dir_all(&out_dir);

    let cmd = swc_path.to_string_lossy().to_string();
    let exact_command = format!("{} src -d .howth/bench-swc", cmd);

    // Run swc and time it
    let mut samples = Vec::with_capacity(3);

    for _ in 0..3 {
        let start = Instant::now();
        let _ = Command::new(&cmd)
            .args(["src", "-d", &out_dir.to_string_lossy()])
            .current_dir(project_dir)
            .output();
        let elapsed = start.elapsed();
        samples.push(elapsed.as_nanos() as u64);
    }

    // Cleanup
    let _ = fs::remove_dir_all(&out_dir);

    let stats = compute_stats(&samples);
    Some(BaselineResult {
        name: "swc".to_string(),
        command: exact_command,
        median_ns: stats.median_ns,
    })
}

/// Create a temporary project for benchmarking.
fn create_temp_project() -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let path = temp_dir.path();

    // Create package.json
    fs::write(
        path.join("package.json"),
        r#"{"name": "bench-project", "scripts": {}}"#,
    )
    .expect("Failed to write package.json");

    // Create tsconfig.json
    fs::write(
        path.join("tsconfig.json"),
        r#"{"compilerOptions": {"strict": true, "esModuleInterop": true, "target": "ES2020", "module": "ESNext", "moduleResolution": "node", "jsx": "react-jsx"}}"#,
    )
    .expect("Failed to write tsconfig.json");

    // Create src/ directory with sample files
    let src_dir = path.join("src");
    fs::create_dir(&src_dir).expect("Failed to create src directory");

    // Create sample TypeScript files
    for i in 0..10 {
        fs::write(
            src_dir.join(format!("module{i}.ts")),
            format!(
                r#"// Module {i}
export const value{i}: number = {i};
export function fn{i}(x: number): number {{
    return x * {i};
}}
"#
            ),
        )
        .expect("Failed to write module file");
    }

    // Create index.ts that imports all modules
    let imports: String = (0..10)
        .map(|i| format!("export * from './module{i}';"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(src_dir.join("index.ts"), imports).expect("Failed to write index.ts");

    temp_dir
}

/// Count transpilable files in a directory.
fn count_transpilable_files(dir: &Path) -> u32 {
    if !dir.exists() {
        return 0;
    }

    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext.to_lowercase().as_str(), "ts" | "tsx" | "js" | "jsx"))
                .unwrap_or(false)
        })
        .count() as u32
}

/// Find a file to touch for warm_1_change benchmark.
fn find_file_to_touch(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }

    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .find(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext.to_lowercase().as_str(), "ts" | "tsx"))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
}

/// Touch a file by appending a comment.
fn touch_file_content(path: &Path) {
    if let Ok(content) = fs::read_to_string(path) {
        let new_content = format!("{content}\n// touched at {:?}", Instant::now());
        let _ = fs::write(path, new_content);
    }
}

/// Detect the CPU name from the system.
fn detect_cpu_name() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            for line in content.lines() {
                if line.starts_with("model name") {
                    if let Some(name) = line.split(':').nth(1) {
                        return name.trim().to_string();
                    }
                }
            }
        }
    }

    // Fallback
    format!("{} {}", std::env::consts::ARCH, std::env::consts::OS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bench_target_as_str() {
        assert_eq!(BenchTarget::Transpile.as_str(), "transpile");
        assert_eq!(BenchTarget::Devloop.as_str(), "devloop");
    }

    #[test]
    fn test_machine_info_detect() {
        let info = MachineInfo::detect();
        assert!(!info.cpu.is_empty());
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(info.cores > 0);
    }

    #[test]
    fn test_build_bench_params_default() {
        let params = BuildBenchParams::default();
        assert_eq!(params.target, BenchTarget::Transpile);
        assert_eq!(params.iters, 10);
        assert_eq!(params.warmup, 2);
    }

    #[test]
    fn test_build_bench_report_new() {
        let params = BuildBenchParams::default();
        let report = BuildBenchReport::new(params);
        assert_eq!(report.schema_version, BUILD_BENCH_SCHEMA_VERSION);
        assert_eq!(report.target, "transpile");
        assert!(report.results.is_empty());
        assert!(report.baselines.is_empty());
    }

    #[test]
    fn test_create_temp_project() {
        let temp = create_temp_project();
        let path = temp.path();

        assert!(path.join("package.json").exists());
        assert!(path.join("tsconfig.json").exists());
        assert!(path.join("src").exists());
        assert!(path.join("src/index.ts").exists());
        assert!(path.join("src/module0.ts").exists());
    }

    #[test]
    fn test_count_transpilable_files() {
        let temp = create_temp_project();
        let count = count_transpilable_files(&temp.path().join("src"));
        assert_eq!(count, 11); // 10 modules + index.ts
    }

    #[test]
    fn test_run_build_bench_with_temp_project() {
        let params = BuildBenchParams {
            target: BenchTarget::Transpile,
            iters: 3,
            warmup: 1,
        };

        let report = run_build_bench(params, None);

        // Should have 3 results: cold, warm_noop, warm_1_change
        assert_eq!(report.results.len(), 3);
        assert_eq!(report.results[0].case, "cold");
        assert_eq!(report.results[1].case, "warm_noop");
        assert_eq!(report.results[2].case, "warm_1_change");

        // All should have valid timing
        for result in &report.results {
            assert!(result.median_ns > 0);
            assert_eq!(result.samples, 3);
        }
    }

    #[test]
    fn test_run_build_bench_low_iters_warning() {
        let params = BuildBenchParams {
            target: BenchTarget::Transpile,
            iters: 2,
            warmup: 1,
        };

        let report = run_build_bench(params, None);

        // Should have LOW_ITERS warning
        assert!(report.warnings.iter().any(|w| w.code == "LOW_ITERS"));
    }
}
