//! Install benchmark harness for fastnode.
//!
//! Compares cold install speed across howth, npm, and bun by shelling out
//! to each package manager as a subprocess. All tools are measured the same
//! way (wall clock + `RUSAGE_CHILDREN`) for a fair comparison.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use crate::bench::build::MachineInfo;
use crate::bench::rusage;
use crate::bench::stats::{compute_median, compute_stats};
use crate::bench::BenchWarning;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Schema version for install benchmark reports.
pub const INSTALL_BENCH_SCHEMA_VERSION: u32 = 1;

/// Default number of measured iterations (lower than build bench since installs hit the network).
pub const DEFAULT_ITERS: u32 = 3;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = 1;

/// Parameters for the install benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallBenchParams {
    /// Number of measured iterations.
    pub iters: u32,
    /// Number of warmup iterations (not measured).
    pub warmup: u32,
}

/// Info about the project used for the benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProjectInfo {
    /// Project name.
    pub name: String,
    /// Total number of dependencies (deps + dev deps).
    pub dep_count: u32,
}

/// Result for a single tool's install benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallToolResult {
    /// Tool name: "howth", "npm", or "bun".
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
pub struct InstallComparison {
    /// The tool being compared against ("npm" or "bun").
    pub tool: String,
    /// Speedup factor (e.g. 2.3 means "howth is 2.3x faster").
    pub speedup: f64,
}

/// Complete install benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallBenchReport {
    /// Schema version.
    pub schema_version: u32,
    /// Machine information.
    pub machine: MachineInfo,
    /// Benchmark parameters.
    pub params: InstallBenchParams,
    /// Project info.
    pub project: InstallProjectInfo,
    /// Per-tool results.
    pub results: Vec<InstallToolResult>,
    /// Comparisons (howth vs each other tool).
    pub comparisons: Vec<InstallComparison>,
    /// Warnings encountered.
    pub warnings: Vec<BenchWarning>,
}

/// The test project's package.json content.
const TEST_PACKAGE_JSON: &str = r#"{
  "name": "bench-install-project",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "express": "^4.18.0",
    "lodash": "^4.17.0",
    "axios": "^1.6.0",
    "chalk": "^4.1.0",
    "commander": "^11.0.0",
    "dotenv": "^16.3.0",
    "uuid": "^9.0.0",
    "zod": "^3.22.0",
    "date-fns": "^3.0.0",
    "debug": "^4.3.0",
    "semver": "^7.5.0",
    "minimatch": "^9.0.0",
    "glob": "^10.3.0",
    "fs-extra": "^11.2.0",
    "mime-types": "^2.1.0",
    "ms": "^2.1.0",
    "on-finished": "^2.4.0",
    "qs": "^6.11.0"
  },
  "devDependencies": {
    "typescript": "^5.3.0",
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "@types/express": "^4.17.0",
    "@types/lodash": "^4.17.0",
    "@types/node": "^20.10.0",
    "@types/uuid": "^9.0.0",
    "@types/debug": "^4.1.0",
    "@types/fs-extra": "^11.0.0",
    "@types/mime-types": "^2.1.0"
  }
}
"#;

const TEST_TSCONFIG_JSON: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "esModuleInterop": true,
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "jsx": "react-jsx"
  }
}
"#;

/// Total dependency count in the test project.
const TEST_DEP_COUNT: u32 = 30;

/// Run the install benchmark.
///
/// If `project_path` is `None`, creates a temporary test project with ~30 deps.
/// Benchmarks howth, npm, and bun (skipping any that aren't installed).
#[must_use]
pub fn run_install_bench(
    params: InstallBenchParams,
    project_path: Option<&Path>,
) -> InstallBenchReport {
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

    // Use provided project or create temp project
    let (temp_dir, project_dir, project_info) = if let Some(path) = project_path {
        let dep_count = count_deps_in_package_json(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
        (
            None,
            path.to_path_buf(),
            InstallProjectInfo { name, dep_count },
        )
    } else {
        let temp = create_install_test_project(&mut warnings);
        let path = temp.path().to_path_buf();
        let info = InstallProjectInfo {
            name: "bench-install-project".to_string(),
            dep_count: TEST_DEP_COUNT,
        };
        (Some(temp), path, info)
    };

    // Copy project dir into per-tool subdirs so lockfiles don't conflict
    let work_dir = tempfile::tempdir().expect("Failed to create work directory");
    let howth_dir = work_dir.path().join("howth");
    let npm_dir = work_dir.path().join("npm");
    let bun_dir = work_dir.path().join("bun");

    copy_dir_contents(&project_dir, &howth_dir);
    copy_dir_contents(&project_dir, &npm_dir);
    copy_dir_contents(&project_dir, &bun_dir);

    // Run benchmarks for each tool
    let mut results = Vec::new();

    if let Some(r) = bench_tool("howth", "howth install", &howth_dir, &params, &mut warnings) {
        results.push(r);
    }
    if let Some(r) = bench_tool("npm", "npm install", &npm_dir, &params, &mut warnings) {
        results.push(r);
    }
    if let Some(r) = bench_tool("bun", "bun install", &bun_dir, &params, &mut warnings) {
        results.push(r);
    }

    // Compute comparisons (howth vs others)
    let comparisons = compute_comparisons(&results);

    drop(temp_dir);

    InstallBenchReport {
        schema_version: INSTALL_BENCH_SCHEMA_VERSION,
        machine: MachineInfo::detect(),
        params,
        project: project_info,
        results,
        comparisons,
        warnings,
    }
}

/// Benchmark a single tool's install command.
fn bench_tool(
    tool_name: &str,
    install_cmd: &str,
    project_dir: &Path,
    params: &InstallBenchParams,
    warnings: &mut Vec<BenchWarning>,
) -> Option<InstallToolResult> {
    let (cmd, args) = parse_command(install_cmd);

    // Check if tool is available
    if !tool_available(&cmd) {
        warnings.push(BenchWarning::info(
            "TOOL_MISSING",
            format!("{tool_name} not found in PATH, skipping"),
        ));
        return None;
    }

    // Generate lockfile for npm if needed (npm needs package-lock.json for deterministic installs)
    if tool_name == "npm" && !project_dir.join("package-lock.json").exists() {
        eprintln!("  Generating package-lock.json for npm...");
        let _ = Command::new(&cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
        // Clean node_modules after generating lockfile
        let _ = fs::remove_dir_all(project_dir.join("node_modules"));
    }

    eprintln!("  Benchmarking {tool_name}...");

    // Warmup runs
    for i in 0..params.warmup {
        eprintln!("    warmup {}/{}", i + 1, params.warmup);
        clean_before_install(tool_name, project_dir);
        let _ = Command::new(&cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
    }

    // Measured runs
    // Skip rusage for howth — work happens in the daemon process (not a child),
    // so RUSAGE_CHILDREN can't measure it accurately.
    let capture_rusage = tool_name != "howth";
    let mut samples = Vec::with_capacity(params.iters as usize);
    let mut cpu_samples = Vec::with_capacity(params.iters as usize);
    let mut peak_rss: u64 = 0;

    for i in 0..params.iters {
        eprintln!("    run {}/{}", i + 1, params.iters);
        clean_before_install(tool_name, project_dir);

        let ru_before = if capture_rusage {
            rusage::snapshot_children()
        } else {
            None
        };
        let start = Instant::now();
        let output = Command::new(&cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
        let elapsed = start.elapsed();

        if let Ok(ref out) = output {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                warnings.push(BenchWarning::warn(
                    "INSTALL_FAILED",
                    format!(
                        "{tool_name} install failed (exit {}): {}",
                        out.status,
                        stderr.chars().take(200).collect::<String>()
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
    Some(InstallToolResult {
        tool: tool_name.to_string(),
        command: install_cmd.to_string(),
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

/// Clean node_modules and tool caches before an install run.
fn clean_before_install(tool_name: &str, project_dir: &Path) {
    // Always remove node_modules
    let _ = fs::remove_dir_all(project_dir.join("node_modules"));

    // Clear tool-specific caches for a true cold install
    match tool_name {
        "npm" => {
            let _ = Command::new("npm")
                .args(["cache", "clean", "--force"])
                .current_dir(project_dir)
                .output();
        }
        "bun" => {
            // Bun's cache location
            if let Some(home) = dirs_next::home_dir() {
                let _ = fs::remove_dir_all(home.join(".bun/install/cache"));
            }
        }
        "howth" => {
            // Clear howth's package cache
            if let Some(cache_dir) = dirs_next::cache_dir() {
                let _ = fs::remove_dir_all(cache_dir.join("howth/packages"));
            }
        }
        _ => {}
    }
}

/// Create a temporary test project with ~30 dependencies.
///
/// Generates lockfiles for both howth and npm so each tool gets a deterministic install.
fn create_install_test_project(warnings: &mut Vec<BenchWarning>) -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let path = temp_dir.path();

    eprintln!("Creating test project with {TEST_DEP_COUNT} dependencies...");

    // Write package.json and tsconfig.json
    fs::write(path.join("package.json"), TEST_PACKAGE_JSON).expect("Failed to write package.json");
    fs::write(path.join("tsconfig.json"), TEST_TSCONFIG_JSON)
        .expect("Failed to write tsconfig.json");

    // Generate lockfiles by running initial installs
    // npm first (to get package-lock.json)
    if tool_available("npm") {
        eprintln!("  Generating package-lock.json...");
        let output = Command::new("npm")
            .args(["install"])
            .current_dir(path)
            .output();
        if let Ok(ref out) = output {
            if !out.status.success() {
                warnings.push(BenchWarning::warn(
                    "LOCKFILE_GEN_FAILED",
                    "Failed to generate package-lock.json",
                ));
            }
        }
        let _ = fs::remove_dir_all(path.join("node_modules"));
    }

    // howth (to get howth.lock)
    if tool_available("howth") {
        eprintln!("  Generating howth.lock...");
        let output = Command::new("howth")
            .args(["install"])
            .current_dir(path)
            .output();
        if let Ok(ref out) = output {
            if !out.status.success() {
                warnings.push(BenchWarning::warn(
                    "LOCKFILE_GEN_FAILED",
                    "Failed to generate howth.lock",
                ));
            }
        }
        let _ = fs::remove_dir_all(path.join("node_modules"));
    }

    temp_dir
}

/// Compute comparisons between howth and other tools.
fn compute_comparisons(results: &[InstallToolResult]) -> Vec<InstallComparison> {
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
        .map(|r| InstallComparison {
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

/// Parse a command string into (program, args).
fn parse_command(cmd: &str) -> (String, Vec<String>) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let program = parts[0].to_string();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
    (program, args)
}

/// Count dependencies in a project's package.json.
fn count_deps_in_package_json(project_dir: &Path) -> u32 {
    let pkg_path = project_dir.join("package.json");
    let Ok(content) = fs::read_to_string(&pkg_path) else {
        return 0;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return 0;
    };

    let deps = value
        .get("dependencies")
        .and_then(|v| v.as_object())
        .map(|o| o.len() as u32)
        .unwrap_or(0);
    let dev_deps = value
        .get("devDependencies")
        .and_then(|v| v.as_object())
        .map(|o| o.len() as u32)
        .unwrap_or(0);

    deps + dev_deps
}

/// Recursively copy directory contents.
fn copy_dir_contents(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("Failed to create destination directory");
    for entry in fs::read_dir(src).expect("Failed to read source directory") {
        let entry = entry.expect("Failed to read directory entry");
        let file_type = entry.file_type().expect("Failed to get file type");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            // Skip node_modules when copying
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
    fn test_install_bench_schema_version() {
        assert_eq!(INSTALL_BENCH_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_parse_command() {
        let (cmd, args) = parse_command("npm install");
        assert_eq!(cmd, "npm");
        assert_eq!(args, vec!["install"]);

        let (cmd, args) = parse_command("howth install");
        assert_eq!(cmd, "howth");
        assert_eq!(args, vec!["install"]);
    }

    #[test]
    fn test_compute_comparisons_basic() {
        let results = vec![
            InstallToolResult {
                tool: "howth".to_string(),
                command: "howth install".to_string(),
                median_ns: 1_000_000_000, // 1s
                p95_ns: 1_200_000_000,
                min_ns: 900_000_000,
                max_ns: 1_300_000_000,
                samples: 3,
                median_cpu_us: None,
                peak_rss_bytes: None,
            },
            InstallToolResult {
                tool: "npm".to_string(),
                command: "npm install".to_string(),
                median_ns: 5_000_000_000, // 5s
                p95_ns: 6_000_000_000,
                min_ns: 4_000_000_000,
                max_ns: 7_000_000_000,
                samples: 3,
                median_cpu_us: None,
                peak_rss_bytes: None,
            },
        ];

        let comparisons = compute_comparisons(&results);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].tool, "npm");
        assert!((comparisons[0].speedup - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_comparisons_no_howth() {
        let results = vec![InstallToolResult {
            tool: "npm".to_string(),
            command: "npm install".to_string(),
            median_ns: 5_000_000_000,
            p95_ns: 6_000_000_000,
            min_ns: 4_000_000_000,
            max_ns: 7_000_000_000,
            samples: 3,
            median_cpu_us: None,
            peak_rss_bytes: None,
        }];

        let comparisons = compute_comparisons(&results);
        assert!(comparisons.is_empty());
    }

    #[test]
    fn test_count_deps_in_package_json() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("package.json"), TEST_PACKAGE_JSON).unwrap();
        let count = count_deps_in_package_json(temp.path());
        assert_eq!(count, TEST_DEP_COUNT);
    }

    #[test]
    fn test_copy_dir_contents() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        fs::write(src.path().join("file.txt"), "hello").unwrap();
        fs::create_dir(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/nested.txt"), "world").unwrap();

        // Create node_modules that should be skipped
        fs::create_dir(src.path().join("node_modules")).unwrap();
        fs::write(src.path().join("node_modules/pkg.js"), "skip me").unwrap();

        let dst_path = dst.path().join("output");
        copy_dir_contents(src.path(), &dst_path);

        assert!(dst_path.join("file.txt").exists());
        assert!(dst_path.join("sub/nested.txt").exists());
        assert!(!dst_path.join("node_modules").exists());
    }
}
