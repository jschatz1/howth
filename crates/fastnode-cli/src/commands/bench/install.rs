use fastnode_core::bench::install::{
    run_install_bench, InstallBenchParams, InstallBenchReport,
};
use fastnode_core::bench::Severity;
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::PathBuf;

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = fastnode_core::bench::install::DEFAULT_ITERS;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = fastnode_core::bench::install::DEFAULT_WARMUP;

/// Run the bench install command.
pub fn run(iters: u32, warmup: u32, project: Option<PathBuf>, json: bool) -> Result<()> {
    let params = InstallBenchParams { iters, warmup };
    let project_path = project.as_deref();
    let report = run_install_bench(params, project_path);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &InstallBenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn print_human(report: &InstallBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench install\x1b[0m").into_diagnostic()?;
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
    writeln!(
        out,
        "Project: {} ({} deps)",
        report.project.name, report.project.dep_count
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Results table
    writeln!(
        out,
        "{:<20} {:>12} {:>12} {:>12} {:>12}",
        "Tool", "Median", "p95", "CPU (med)", "Peak RSS"
    )
    .into_diagnostic()?;
    writeln!(out, "{}", "-".repeat(70)).into_diagnostic()?;

    for result in &report.results {
        let median = format_duration(result.median_ns);
        let p95 = format_duration(result.p95_ns);
        let cpu = result
            .median_cpu_us
            .map(format_cpu_time)
            .unwrap_or_else(|| "-".to_string());
        let rss = result
            .peak_rss_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            out,
            "{:<20} {:>12} {:>12} {:>12} {:>12}",
            result.tool, median, p95, cpu, rss
        )
        .into_diagnostic()?;
    }

    // Comparisons
    if !report.comparisons.is_empty() {
        writeln!(out).into_diagnostic()?;
        for cmp in &report.comparisons {
            if cmp.speedup >= 1.0 {
                writeln!(
                    out,
                    "\x1b[1;32mhowth is {:.1}x faster than {}\x1b[0m",
                    cmp.speedup, cmp.tool
                )
                .into_diagnostic()?;
            } else {
                writeln!(
                    out,
                    "\x1b[1;33mhowth is {:.1}x slower than {}\x1b[0m",
                    1.0 / cmp.speedup, cmp.tool
                )
                .into_diagnostic()?;
            }
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

/// Format bytes to a human-readable string.
#[allow(clippy::cast_precision_loss)]
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

/// Format CPU time in microseconds.
#[allow(clippy::cast_precision_loss)]
fn format_cpu_time(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{:.2}ms", us as f64 / 1_000.0)
    } else {
        format!("{us}us")
    }
}

/// Format a duration in nanoseconds.
#[allow(clippy::cast_precision_loss)]
fn format_duration(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2}s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.2}us", ns as f64 / 1_000.0)
    } else {
        format!("{ns}ns")
    }
}
