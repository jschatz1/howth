use fastnode_core::bench::test::{run_test_bench, TestBenchParams, TestBenchReport};
use fastnode_core::bench::Severity;
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = fastnode_core::bench::test::DEFAULT_ITERS;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = fastnode_core::bench::test::DEFAULT_WARMUP;

/// Run the bench test command.
pub fn run(iters: u32, warmup: u32, json: bool) -> Result<()> {
    let params = TestBenchParams { iters, warmup };
    let report = run_test_bench(params);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &TestBenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

/// ANSI color codes for each tool in the benchmark.
const TOOL_COLORS: &[&str] = &[
    "\x1b[1;32m", // green bold  — howth (first)
    "\x1b[1;36m", // cyan bold   — second tool
    "\x1b[1;35m", // magenta bold — third tool
    "\x1b[1;33m", // yellow bold — fourth tool
];

fn tool_color(index: usize) -> &'static str {
    TOOL_COLORS.get(index).unwrap_or(&"\x1b[1;37m")
}

#[allow(clippy::cast_precision_loss)]
fn print_human(report: &TestBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench test\x1b[0m").into_diagnostic()?;
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
    writeln!(
        out,
        "\x1b[90mProject: {} ({} files, {} tests)\x1b[0m",
        report.project.name, report.project.test_files, report.project.test_cases
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Find the fastest median
    let min_median = report.results.iter().map(|r| r.median_ns).min().unwrap_or(0);

    // Results — one block per tool (hyperfine style)
    for (i, result) in report.results.iter().enumerate() {
        let color = tool_color(i);
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

        // Tool name + command
        writeln!(
            out,
            "\x1b[1mBenchmark #{}: {color}{}\x1b[0m",
            i + 1,
            result.tool
        )
        .into_diagnostic()?;

        // Median line — green if fastest
        if result.median_ns == min_median {
            write!(out, "  Time (median):     \x1b[1;32m{median:>10}\x1b[0m").into_diagnostic()?;
        } else {
            write!(out, "  Time (median):     {median:>10}").into_diagnostic()?;
        }
        writeln!(out, "     p95: {p95:>10}").into_diagnostic()?;

        // Resource line (dim)
        writeln!(
            out,
            "  \x1b[90mResources:           CPU: {cpu:>10}     RSS: {rss:>10}\x1b[0m",
        )
        .into_diagnostic()?;

        // Samples count
        writeln!(
            out,
            "  \x1b[90m{} runs\x1b[0m",
            result.samples
        )
        .into_diagnostic()?;
        writeln!(out).into_diagnostic()?;
    }

    // Summary
    if !report.comparisons.is_empty() {
        writeln!(out, "\x1b[1mSummary\x1b[0m").into_diagnostic()?;
        writeln!(
            out,
            "  {}\x1b[1;32mhowth\x1b[0m ran",
            tool_color(0)
        )
        .into_diagnostic()?;
        for (i, cmp) in report.comparisons.iter().enumerate() {
            let cmp_color = tool_color(i + 1);
            if cmp.speedup >= 1.0 {
                writeln!(
                    out,
                    "    \x1b[1;32m{:.2}\x1b[0m times faster than {cmp_color}{}\x1b[0m",
                    cmp.speedup, cmp.tool
                )
                .into_diagnostic()?;
            } else {
                writeln!(
                    out,
                    "    \x1b[1;31m{:.2}\x1b[0m times slower than {cmp_color}{}\x1b[0m",
                    1.0 / cmp.speedup, cmp.tool
                )
                .into_diagnostic()?;
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
