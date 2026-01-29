use fastnode_core::bench::{run_smoke_benchmarks, BenchReport, Severity};
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};

/// Default number of measured iterations.
pub const DEFAULT_ITERS: u32 = 50;

/// Default number of warmup iterations.
pub const DEFAULT_WARMUP: u32 = 10;

/// Default payload size in MiB.
pub const DEFAULT_SIZE_MIB: u32 = 8;

/// Run the bench smoke command.
///
/// When `json` is true, outputs a single JSON object to stdout.
/// Otherwise, outputs human-readable formatted text to stdout.
pub fn run(iters: u32, warmup: u32, size_mib: u32, json: bool) -> Result<()> {
    // Convert MiB to bytes
    let size_bytes = u64::from(size_mib) * 1024 * 1024;

    let report = run_smoke_benchmarks(iters, warmup, size_bytes);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &BenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

/// ANSI color codes for each smoke benchmark.
const SMOKE_COLORS: &[&str] = &[
    "\x1b[1;32m", // green bold
    "\x1b[1;36m", // cyan bold
    "\x1b[1;35m", // magenta bold
    "\x1b[1;33m", // yellow bold
    "\x1b[1;34m", // blue bold
    "\x1b[1;37m", // white bold
];

fn smoke_color(index: usize) -> &'static str {
    SMOKE_COLORS.get(index % SMOKE_COLORS.len()).unwrap_or(&"\x1b[1;37m")
}

fn print_human(report: &BenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench smoke\x1b[0m").into_diagnostic()?;
    writeln!(
        out,
        "\x1b[90mParams: iters={} warmup={} size={} MiB\x1b[0m",
        report.params.iters,
        report.params.warmup,
        report.params.size_bytes / (1024 * 1024)
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Find the fastest median
    let min_median = report.results.iter().map(|r| r.median_ns).min().unwrap_or(0);

    // Results — one block per benchmark
    for (i, result) in report.results.iter().enumerate() {
        let color = smoke_color(i);
        let median = format_duration(result.median_ns);
        let p95 = format_duration(result.p95_ns);
        let min = format_duration(result.min_ns);
        let max = format_duration(result.max_ns);

        // Name
        writeln!(
            out,
            "\x1b[1mBenchmark #{}: {color}{}\x1b[0m",
            i + 1,
            result.name
        )
        .into_diagnostic()?;

        // Time line
        if result.median_ns == min_median {
            write!(out, "  Time (median):     \x1b[1;32m{median:>10}\x1b[0m").into_diagnostic()?;
        } else {
            write!(out, "  Time (median):     {median:>10}").into_diagnostic()?;
        }
        writeln!(out, "     p95: {p95:>10}").into_diagnostic()?;

        // Range line — min green, max red
        writeln!(
            out,
            "  Range (min \u{2026} max):  \x1b[32m{min:>10}\x1b[0m \u{2026} \x1b[31m{max:>10}\x1b[0m    \x1b[90m{} runs\x1b[0m",
            result.samples
        )
        .into_diagnostic()?;
        writeln!(out).into_diagnostic()?;
    }

    // Warnings
    if !report.warnings.is_empty() {
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

/// Format a duration in nanoseconds to a human-readable string.
#[allow(clippy::cast_precision_loss)]
fn format_duration(ns: u64) -> String {
    // Precision loss is acceptable for display purposes
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
        assert_eq!(format_duration(999_999), "1000.00us");
    }

    #[test]
    fn test_format_duration_milliseconds() {
        assert_eq!(format_duration(1_000_000), "1.00ms");
        assert_eq!(format_duration(1_500_000), "1.50ms");
        assert_eq!(format_duration(999_999_999), "1000.00ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(1_000_000_000), "1.00s");
        assert_eq!(format_duration(1_500_000_000), "1.50s");
    }
}
