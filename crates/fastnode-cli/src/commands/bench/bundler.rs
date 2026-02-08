use fastnode_core::bench::{
    run_bundler_bench, BundlerBenchParams, BundlerBenchReport, Severity,
};
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};

/// Default number of modules to generate.
pub const DEFAULT_MODULES: u32 = 1000;

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = 5;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = 1;

/// Run the bench bundler command.
pub fn run(modules: u32, iters: u32, warmup: u32, json: bool) -> Result<()> {
    let params = BundlerBenchParams {
        module_count: modules,
        iters,
        warmup,
    };

    eprintln!(
        "\x1b[1mGenerating {} modules and benchmarking bundlers...\x1b[0m\n",
        modules
    );

    let report = run_bundler_bench(params);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &BundlerBenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

fn print_human(report: &BundlerBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(
        out,
        "\x1b[1mhowth bench bundler\x1b[0m ({} modules)",
        report.params.module_count
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Machine info
    writeln!(
        out,
        "\x1b[90mMachine: {} ({} cores, {})\x1b[0m",
        report.machine.cpu, report.machine.cores, report.machine.os
    )
    .into_diagnostic()?;
    writeln!(
        out,
        "\x1b[90mRuns: {} (warmup: {})\x1b[0m",
        report.params.iters, report.params.warmup
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Results table header
    writeln!(
        out,
        "\x1b[1m{:<12} {:>12} {:>12} {:>12} {:>12}\x1b[0m",
        "Tool", "Median", "Min", "Max", "Bundle Size"
    )
    .into_diagnostic()?;
    writeln!(out, "{}", "-".repeat(64)).into_diagnostic()?;

    // Find fastest tool
    let min_median = report
        .results
        .iter()
        .filter_map(|r| r.median_ms)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Results
    for result in &report.results {
        if !result.available {
            writeln!(
                out,
                "\x1b[90m{:<12} (not available)\x1b[0m",
                result.name
            )
            .into_diagnostic()?;
            continue;
        }

        if let Some(error) = &result.error {
            writeln!(out, "\x1b[31m{:<12} ERROR: {}\x1b[0m", result.name, error)
                .into_diagnostic()?;
            continue;
        }

        let median = result
            .median_ms
            .map(|ms| format!("{:.2}ms", ms))
            .unwrap_or_else(|| "-".to_string());
        let min = result
            .min_ms
            .map(|ms| format!("{:.2}ms", ms))
            .unwrap_or_else(|| "-".to_string());
        let max = result
            .max_ms
            .map(|ms| format!("{:.2}ms", ms))
            .unwrap_or_else(|| "-".to_string());
        let size = result
            .bundle_size_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());

        // Highlight fastest
        let is_fastest = result.median_ms == min_median;
        if is_fastest {
            writeln!(
                out,
                "\x1b[1;32m{:<12} {:>12} {:>12} {:>12} {:>12}\x1b[0m",
                result.name, median, min, max, size
            )
            .into_diagnostic()?;
        } else {
            writeln!(
                out,
                "{:<12} {:>12} {:>12} {:>12} {:>12}",
                result.name, median, min, max, size
            )
            .into_diagnostic()?;
        }
    }

    // Comparisons
    if !report.comparisons.is_empty() {
        writeln!(out).into_diagnostic()?;
        writeln!(out, "\x1b[1mComparisons\x1b[0m").into_diagnostic()?;
        for comparison in &report.comparisons {
            if comparison.speedup > 1.0 {
                writeln!(out, "  \x1b[32m{}\x1b[0m", comparison.description).into_diagnostic()?;
            } else if comparison.speedup < 1.0 {
                writeln!(out, "  \x1b[33m{}\x1b[0m", comparison.description).into_diagnostic()?;
            } else {
                writeln!(out, "  {}", comparison.description).into_diagnostic()?;
            }
        }
    }

    // Warnings
    if !report.warnings.is_empty() {
        writeln!(out).into_diagnostic()?;
        for warning in &report.warnings {
            let prefix = match warning.severity {
                Severity::Info => "\x1b[34minfo\x1b[0m",
                Severity::Warn => "\x1b[33mwarn\x1b[0m",
            };
            writeln!(
                out,
                "\x1b[33mWarning\x1b[0m: [{prefix}] {}: {}",
                warning.code, warning.message
            )
            .into_diagnostic()?;
        }
    }

    out.flush().into_diagnostic()?;
    Ok(())
}

/// Format bytes to a human-readable string.
#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(1_048_576), "1.0MB");
    }
}
