//! Bundler benchmark harness for fastnode.
//!
//! Benchmarks `howth bundle` against `bun build` and `esbuild` on large projects.
//!
//! ## Benchmark Scales
//!
//! - **1000 modules**: Small-medium project
//! - **5000 modules**: Large project
//! - **10000 modules**: Enterprise-scale project

#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::uninlined_format_args)]

use crate::bench::{compute_stats, BenchWarning, MachineInfo};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Schema version for bundler benchmark reports.
pub const BUNDLER_BENCH_SCHEMA_VERSION: u32 = 1;

/// Bundler benchmark parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerBenchParams {
    /// Number of modules to generate.
    pub module_count: u32,
    /// Number of measured iterations.
    pub iters: u32,
    /// Number of warmup iterations.
    pub warmup: u32,
}

impl Default for BundlerBenchParams {
    fn default() -> Self {
        Self {
            module_count: 1000,
            iters: 5,
            warmup: 1,
        }
    }
}

/// Result for a single bundler tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerToolResult {
    /// Tool name (e.g., "howth", "bun", "esbuild").
    pub name: String,
    /// Exact command that was run.
    pub command: String,
    /// Whether the tool was available and ran successfully.
    pub available: bool,
    /// Median time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_ms: Option<f64>,
    /// Minimum time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ms: Option<f64>,
    /// Maximum time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_ms: Option<f64>,
    /// Output bundle size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_size_bytes: Option<u64>,
    /// Error message if tool failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Comparison between tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerComparison {
    /// Baseline tool name.
    pub baseline: String,
    /// Compared tool name.
    pub compared: String,
    /// Speedup factor (> 1 means compared is faster).
    pub speedup: f64,
    /// Description (e.g., "howth is 2.5x faster than bun").
    pub description: String,
}

/// Complete bundler benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerBenchReport {
    /// Schema version.
    pub schema_version: u32,
    /// Machine information.
    pub machine: MachineInfo,
    /// Benchmark parameters.
    pub params: BundlerBenchParams,
    /// Results for each tool.
    pub results: Vec<BundlerToolResult>,
    /// Comparisons between tools.
    pub comparisons: Vec<BundlerComparison>,
    /// Warnings encountered.
    pub warnings: Vec<BenchWarning>,
}

impl BundlerBenchReport {
    /// Create a new report.
    #[must_use]
    pub fn new(params: BundlerBenchParams) -> Self {
        Self {
            schema_version: BUNDLER_BENCH_SCHEMA_VERSION,
            machine: MachineInfo::detect(),
            params,
            results: Vec::new(),
            comparisons: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Run bundler benchmarks.
#[must_use]
pub fn run_bundler_bench(params: BundlerBenchParams) -> BundlerBenchReport {
    let mut report = BundlerBenchReport::new(params.clone());

    // Validate params
    if params.iters < 3 {
        report.warnings.push(BenchWarning::warn(
            "LOW_ITERS",
            format!(
                "Low iteration count ({}); results may be noisy",
                params.iters
            ),
        ));
    }

    // Create temp project with generated modules
    let temp_dir = create_bench_project(params.module_count);
    let project_dir = temp_dir.path();

    // Run each bundler
    let tools = ["howth", "bun", "esbuild", "rolldown"];
    for tool in tools {
        let result = run_bundler_tool(tool, project_dir, &params);
        report.results.push(result);
    }

    // Generate comparisons
    report.comparisons = generate_comparisons(&report.results);

    report
}

/// Create a benchmark project with generated modules.
fn create_bench_project(module_count: u32) -> tempfile::TempDir {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let path = temp_dir.path();

    // Create package.json
    fs::write(
        path.join("package.json"),
        r#"{"name": "bundler-bench", "type": "module"}"#,
    )
    .expect("Failed to write package.json");

    // Create src/ directory
    let src_dir = path.join("src");
    fs::create_dir(&src_dir).expect("Failed to create src directory");

    // Generate modules with import chains
    // Each module imports the previous one (creating a chain)
    // This is similar to real-world import graphs
    for i in 0..module_count {
        let content = if i == 0 {
            // Root module - no imports
            format!(
                r#"// Module {i}
export const value{i} = {i};
export function compute{i}(x) {{
    return x * {i} + {i};
}}
"#
            )
        } else {
            // Import from previous module and re-export
            format!(
                r#"// Module {i}
import {{ value{prev}, compute{prev} }} from './module{prev}.js';

export const value{i} = value{prev} + {i};
export function compute{i}(x) {{
    return compute{prev}(x) + {i};
}}
"#,
                prev = i - 1
            )
        };
        fs::write(src_dir.join(format!("module{i}.js")), content)
            .expect("Failed to write module file");
    }

    // Create index.js that imports the last module (pulls in the whole chain)
    let last = module_count - 1;
    let index_content = format!(
        r#"// Entry point - imports the end of the chain, triggering full bundle
import {{ value{last}, compute{last} }} from './module{last}.js';

console.log('Value:', value{last});
console.log('Computed:', compute{last}(42));

export {{ value{last}, compute{last} }};
"#
    );
    fs::write(src_dir.join("index.js"), index_content).expect("Failed to write index.js");

    temp_dir
}

/// Check if a tool is available.
fn tool_available(tool: &str) -> bool {
    let binary = match tool {
        "howth" => "howth",
        "bun" => "bun",
        "esbuild" => "esbuild",
        "rolldown" => "rolldown",
        _ => return false,
    };

    Command::new("which")
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a single bundler and measure performance.
fn run_bundler_tool(
    tool: &str,
    project_dir: &Path,
    params: &BundlerBenchParams,
) -> BundlerToolResult {
    let entry = project_dir.join("src/index.js");
    let outfile = project_dir.join(format!("dist-{tool}.js"));

    // Build command based on tool
    let (cmd, args, command_str) = match tool {
        "howth" => (
            "howth",
            vec![
                "bundle".to_string(),
                entry.to_string_lossy().to_string(),
                "--outfile".to_string(),
                outfile.to_string_lossy().to_string(),
            ],
            format!(
                "howth bundle {} --outfile {}",
                entry.display(),
                outfile.display()
            ),
        ),
        "bun" => (
            "bun",
            vec![
                "build".to_string(),
                entry.to_string_lossy().to_string(),
                "--outfile".to_string(),
                outfile.to_string_lossy().to_string(),
            ],
            format!(
                "bun build {} --outfile {}",
                entry.display(),
                outfile.display()
            ),
        ),
        "esbuild" => (
            "esbuild",
            vec![
                entry.to_string_lossy().to_string(),
                "--bundle".to_string(),
                format!("--outfile={}", outfile.to_string_lossy()),
            ],
            format!(
                "esbuild {} --bundle --outfile={}",
                entry.display(),
                outfile.display()
            ),
        ),
        "rolldown" => {
            // rolldown outputs to a directory, we'll read the index.js from it
            let outdir = project_dir.join(format!("dist-{tool}"));
            (
                "rolldown",
                vec![
                    entry.to_string_lossy().to_string(),
                    "--dir".to_string(),
                    outdir.to_string_lossy().to_string(),
                ],
                format!("rolldown {} --dir {}", entry.display(), outdir.display()),
            )
        }
        _ => {
            return BundlerToolResult {
                name: tool.to_string(),
                command: String::new(),
                available: false,
                median_ms: None,
                min_ms: None,
                max_ms: None,
                bundle_size_bytes: None,
                error: Some(format!("Unknown tool: {tool}")),
            };
        }
    };

    // Check availability
    if !tool_available(tool) {
        return BundlerToolResult {
            name: tool.to_string(),
            command: command_str,
            available: false,
            median_ms: None,
            min_ms: None,
            max_ms: None,
            bundle_size_bytes: None,
            error: Some(format!("{tool} not found in PATH")),
        };
    }

    // Determine actual output path (rolldown uses directory)
    let actual_outfile = if tool == "rolldown" {
        project_dir.join(format!("dist-{tool}/index.js"))
    } else {
        outfile.clone()
    };
    let outdir = project_dir.join(format!("dist-{tool}"));

    // Warmup runs
    for _ in 0..params.warmup {
        let _ = fs::remove_file(&actual_outfile);
        let _ = fs::remove_dir_all(&outdir);
        let _ = Command::new(cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
    }

    // Measured runs
    let mut samples_ns: Vec<u64> = Vec::with_capacity(params.iters as usize);
    let mut last_error: Option<String> = None;
    let mut bundle_size: Option<u64> = None;

    for _ in 0..params.iters {
        let _ = fs::remove_file(&actual_outfile);
        let _ = fs::remove_dir_all(&outdir);

        let start = Instant::now();
        let output = Command::new(cmd)
            .args(&args)
            .current_dir(project_dir)
            .output();
        let elapsed = start.elapsed();

        match output {
            Ok(o) if o.status.success() => {
                samples_ns.push(elapsed.as_nanos() as u64);
                // Get bundle size from last successful run
                if let Ok(meta) = fs::metadata(&actual_outfile) {
                    bundle_size = Some(meta.len());
                }
            }
            Ok(o) => {
                last_error = Some(format!(
                    "Exit code: {:?}, stderr: {}",
                    o.status.code(),
                    String::from_utf8_lossy(&o.stderr).trim()
                ));
            }
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }
    }

    if samples_ns.is_empty() {
        return BundlerToolResult {
            name: tool.to_string(),
            command: command_str,
            available: true,
            median_ms: None,
            min_ms: None,
            max_ms: None,
            bundle_size_bytes: bundle_size,
            error: last_error,
        };
    }

    let stats = compute_stats(&samples_ns);

    #[allow(clippy::cast_precision_loss)]
    BundlerToolResult {
        name: tool.to_string(),
        command: command_str,
        available: true,
        median_ms: Some(stats.median_ns as f64 / 1_000_000.0),
        min_ms: Some(stats.min_ns as f64 / 1_000_000.0),
        max_ms: Some(stats.max_ns as f64 / 1_000_000.0),
        bundle_size_bytes: bundle_size,
        error: None,
    }
}

/// Generate comparisons between tools.
fn generate_comparisons(results: &[BundlerToolResult]) -> Vec<BundlerComparison> {
    let mut comparisons = Vec::new();

    // Find howth result as baseline for comparison
    let howth_result = results.iter().find(|r| r.name == "howth");

    if let Some(howth) = howth_result {
        if let Some(howth_ms) = howth.median_ms {
            for result in results {
                if result.name == "howth" {
                    continue;
                }
                if let Some(other_ms) = result.median_ms {
                    let speedup = other_ms / howth_ms;
                    let description = if speedup > 1.0 {
                        format!("howth is {:.2}x faster than {}", speedup, result.name)
                    } else if speedup < 1.0 {
                        format!("{} is {:.2}x faster than howth", result.name, 1.0 / speedup)
                    } else {
                        format!("howth and {} have similar performance", result.name)
                    };

                    comparisons.push(BundlerComparison {
                        baseline: "howth".to_string(),
                        compared: result.name.clone(),
                        speedup,
                        description,
                    });
                }
            }
        }
    }

    comparisons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundler_bench_params_default() {
        let params = BundlerBenchParams::default();
        assert_eq!(params.module_count, 1000);
        assert_eq!(params.iters, 5);
        assert_eq!(params.warmup, 1);
    }

    #[test]
    fn test_create_bench_project() {
        let temp = create_bench_project(10);
        let path = temp.path();

        assert!(path.join("package.json").exists());
        assert!(path.join("src").exists());
        assert!(path.join("src/index.js").exists());
        assert!(path.join("src/module0.js").exists());
        assert!(path.join("src/module9.js").exists());
    }

    #[test]
    fn test_bundler_bench_report_new() {
        let params = BundlerBenchParams::default();
        let report = BundlerBenchReport::new(params);
        assert_eq!(report.schema_version, BUNDLER_BENCH_SCHEMA_VERSION);
        assert!(report.results.is_empty());
        assert!(report.comparisons.is_empty());
    }

    #[test]
    fn test_generate_comparisons() {
        let results = vec![
            BundlerToolResult {
                name: "howth".to_string(),
                command: "howth bundle".to_string(),
                available: true,
                median_ms: Some(100.0),
                min_ms: Some(90.0),
                max_ms: Some(110.0),
                bundle_size_bytes: Some(50000),
                error: None,
            },
            BundlerToolResult {
                name: "bun".to_string(),
                command: "bun build".to_string(),
                available: true,
                median_ms: Some(200.0),
                min_ms: Some(180.0),
                max_ms: Some(220.0),
                bundle_size_bytes: Some(50000),
                error: None,
            },
        ];

        let comparisons = generate_comparisons(&results);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].baseline, "howth");
        assert_eq!(comparisons[0].compared, "bun");
        assert!((comparisons[0].speedup - 2.0).abs() < 0.01);
    }
}
