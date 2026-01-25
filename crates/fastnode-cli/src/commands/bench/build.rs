use fastnode_core::bench::{
    run_build_bench, BenchTarget, BuildBenchParams, BuildBenchReport, Severity,
};
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::PathBuf;

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = 10;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = 2;

/// Run the bench transpile command.
pub fn run_transpile(iters: u32, warmup: u32, project: Option<PathBuf>, json: bool) -> Result<()> {
    let params = BuildBenchParams {
        target: BenchTarget::Transpile,
        iters,
        warmup,
    };

    let project_path = project.as_deref();
    let report = run_build_bench(params, project_path);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

/// Run the bench devloop command.
pub fn run_devloop(iters: u32, warmup: u32, project: Option<PathBuf>, json: bool) -> Result<()> {
    let params = BuildBenchParams {
        target: BenchTarget::Devloop,
        iters,
        warmup,
    };

    let project_path = project.as_deref();
    let report = run_build_bench(params, project_path);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &BuildBenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

fn print_human(report: &BuildBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench {}\x1b[0m", report.target).into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Machine info
    writeln!(
        out,
        "Machine: {} ({} cores, {})",
        report.machine.cpu, report.machine.cores, report.machine.os
    )
    .into_diagnostic()?;
    writeln!(
        out,
        "Runs: {} (warmup: {})",
        report.params.iters, report.params.warmup
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Results table header
    writeln!(
        out,
        "{:<20} {:>12} {:>12} {:>8} {:>18}",
        "Case", "Median", "p95", "Files", "Nodes (exec/hit)"
    )
    .into_diagnostic()?;
    writeln!(out, "{}", "-".repeat(74)).into_diagnostic()?;

    // Results
    for result in &report.results {
        let median = format_duration(result.median_ns);
        let p95 = format_duration(result.p95_ns);

        // Files column: show files_transpiled if available, else files_count, else "-"
        let files = result
            .work_done
            .as_ref()
            .and_then(|w| w.files_transpiled)
            .or(result.files_count)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());

        // Nodes column: "executed/cached" or "-"
        let nodes = result
            .work_done
            .as_ref()
            .map(|w| format!("{}/{}", w.nodes_executed, w.nodes_cached))
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            out,
            "{:<20} {:>12} {:>12} {:>8} {:>18}",
            result.case, median, p95, files, nodes
        )
        .into_diagnostic()?;
    }

    // Baselines
    if !report.baselines.is_empty() {
        writeln!(out).into_diagnostic()?;
        writeln!(out, "\x1b[1mBaselines\x1b[0m").into_diagnostic()?;
        for baseline in &report.baselines {
            let median = format_duration(baseline.median_ns);
            writeln!(out, "  {}: {}", baseline.name, median).into_diagnostic()?;
            writeln!(out, "    \x1b[90m$ {}\x1b[0m", baseline.command).into_diagnostic()?;
        }
    }

    // Warnings
    if !report.warnings.is_empty() {
        writeln!(out).into_diagnostic()?;
        writeln!(
            out,
            "\x1b[1mWarnings\x1b[0m ({} total)",
            report.warnings.len()
        )
        .into_diagnostic()?;
        for warning in &report.warnings {
            let prefix = match warning.severity {
                Severity::Info => "\x1b[34minfo\x1b[0m",
                Severity::Warn => "\x1b[33mwarn\x1b[0m",
            };
            writeln!(out, "  [{prefix}] {}: {}", warning.code, warning.message)
                .into_diagnostic()?;
        }
    }

    out.flush().into_diagnostic()?;
    Ok(())
}

/// Format a duration in nanoseconds to a human-readable string.
#[allow(clippy::cast_precision_loss)]
fn format_duration(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        // Seconds
        format!("{:.2}s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        // Milliseconds
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        // Microseconds
        format!("{:.2}us", ns as f64 / 1_000.0)
    } else {
        // Nanoseconds
        format!("{ns}ns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_nanoseconds() {
        assert_eq!(format_duration(500), "500ns");
        assert_eq!(format_duration(999), "999ns");
    }

    #[test]
    fn test_format_duration_microseconds() {
        assert_eq!(format_duration(1_000), "1.00us");
        assert_eq!(format_duration(1_500), "1.50us");
    }

    #[test]
    fn test_format_duration_milliseconds() {
        assert_eq!(format_duration(1_000_000), "1.00ms");
        assert_eq!(format_duration(142_310_000), "142.31ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(1_000_000_000), "1.00s");
        assert_eq!(format_duration(1_820_000_000), "1.82s");
    }
}
