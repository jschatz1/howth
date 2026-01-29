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

/// ANSI color codes for each case in the benchmark.
const CASE_COLORS: &[&str] = &[
    "\x1b[1;32m", // green bold  — first case (cold)
    "\x1b[1;36m", // cyan bold   — second case (warm_noop)
    "\x1b[1;35m", // magenta bold — third case (warm_1_change)
    "\x1b[1;33m", // yellow bold — fourth case (watch_ttg)
];

fn case_color(index: usize) -> &'static str {
    CASE_COLORS.get(index).unwrap_or(&"\x1b[1;37m")
}

fn print_human(report: &BuildBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench {}\x1b[0m", report.target).into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Machine info (dim)
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

    // Find the fastest median
    let min_median = report.results.iter().map(|r| r.median_ns).min().unwrap_or(0);

    // Results — one block per case (hyperfine style)
    for (i, result) in report.results.iter().enumerate() {
        let color = case_color(i);
        let median = format_duration(result.median_ns);
        let p95 = format_duration(result.p95_ns);

        let files = result
            .work_done
            .as_ref()
            .and_then(|w| w.files_transpiled)
            .or(result.files_count)
            .map(|c| c.to_string())
            .unwrap_or_else(|| "-".to_string());

        let nodes = result
            .work_done
            .as_ref()
            .map(|w| format!("{}/{}", w.nodes_executed, w.nodes_cached))
            .unwrap_or_else(|| "-".to_string());

        let cpu = result
            .resource_stats
            .as_ref()
            .map(|r| format_cpu_time(r.median_cpu_us))
            .unwrap_or_else(|| "-".to_string());
        let rss = result
            .resource_stats
            .as_ref()
            .map(|r| format_bytes(r.peak_rss_bytes))
            .unwrap_or_else(|| "-".to_string());

        // Case name
        writeln!(
            out,
            "\x1b[1mBenchmark #{}: {color}{}\x1b[0m",
            i + 1,
            result.case
        )
        .into_diagnostic()?;

        // Median line — green if fastest
        if result.median_ns == min_median {
            write!(out, "  Time (median):     \x1b[1;32m{median:>10}\x1b[0m").into_diagnostic()?;
        } else {
            write!(out, "  Time (median):     {median:>10}").into_diagnostic()?;
        }
        writeln!(out, "     p95: {p95:>10}").into_diagnostic()?;

        // Work done
        writeln!(
            out,
            "  \x1b[90mWork:                files: {files:>5}       nodes: {nodes}\x1b[0m",
        )
        .into_diagnostic()?;

        // Resources (dim)
        writeln!(
            out,
            "  \x1b[90mResources:           CPU: {cpu:>10}     RSS: {rss:>10}\x1b[0m",
        )
        .into_diagnostic()?;

        // Samples
        writeln!(out, "  \x1b[90m{} runs\x1b[0m", result.samples).into_diagnostic()?;
        writeln!(out).into_diagnostic()?;
    }

    // Baselines
    if !report.baselines.is_empty() {
        writeln!(out, "\x1b[1mBaselines\x1b[0m").into_diagnostic()?;
        for baseline in &report.baselines {
            let median = format_duration(baseline.median_ns);
            let mut extras = Vec::new();
            if let Some(cpu_us) = baseline.median_cpu_us {
                extras.push(format!("CPU: \x1b[33m{}\x1b[0m", format_cpu_time(cpu_us)));
            }
            if let Some(rss) = baseline.peak_rss_bytes {
                extras.push(format!("RSS: \x1b[33m{}\x1b[0m", format_bytes(rss)));
            }
            if extras.is_empty() {
                writeln!(out, "  \x1b[1;31m{}\x1b[0m: \x1b[31m{median}\x1b[0m", baseline.name)
                    .into_diagnostic()?;
            } else {
                writeln!(
                    out,
                    "  \x1b[1;31m{}\x1b[0m: \x1b[31m{median}\x1b[0m ({})",
                    baseline.name,
                    extras.join(", ")
                )
                .into_diagnostic()?;
            }
            writeln!(out, "    \x1b[90m$ {}\x1b[0m", baseline.command).into_diagnostic()?;
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

/// Format bytes to a human-readable string (KB/MB/GB).
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

/// Format CPU time in microseconds to a human-readable string.
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

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(1_048_576), "1.0MB");
        assert_eq!(format_bytes(47_448_064), "45.2MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0GB");
    }

    #[test]
    fn test_format_cpu_time() {
        assert_eq!(format_cpu_time(500), "500us");
        assert_eq!(format_cpu_time(1_000), "1.00ms");
        assert_eq!(format_cpu_time(120_500), "120.50ms");
        assert_eq!(format_cpu_time(1_000_000), "1.00s");
        assert_eq!(format_cpu_time(3_400_000), "3.40s");
    }
}
