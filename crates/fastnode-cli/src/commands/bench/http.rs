use fastnode_core::bench::http::{run_http_bench, HttpBenchParams, HttpBenchReport};
use fastnode_core::bench::Severity;
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};

/// Default benchmark duration in seconds.
pub const DEFAULT_DURATION_SECS: u32 = fastnode_core::bench::DEFAULT_DURATION_SECS;

/// Default number of concurrent connections.
pub const DEFAULT_CONNECTIONS: u32 = fastnode_core::bench::DEFAULT_CONNECTIONS;

/// Default warmup duration in seconds.
pub const DEFAULT_WARMUP_SECS: u32 = fastnode_core::bench::DEFAULT_WARMUP_SECS;

/// Run the bench http command.
pub fn run(duration_secs: u32, connections: u32, warmup_secs: u32, json: bool) -> Result<()> {
    let params = HttpBenchParams {
        duration_secs,
        connections,
        warmup_secs,
    };

    eprintln!("Running HTTP benchmark...");
    eprintln!(
        "  Duration: {}s, Connections: {}, Warmup: {}s",
        duration_secs, connections, warmup_secs
    );
    eprintln!();

    let report = run_http_bench(params);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &HttpBenchReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

/// ANSI color codes for each tool in the benchmark.
const TOOL_COLORS: &[&str] = &[
    "\x1b[1;32m", // green bold  — fastest
    "\x1b[1;36m", // cyan bold   — second
    "\x1b[1;35m", // magenta bold — third
    "\x1b[1;33m", // yellow bold — fourth
];

fn tool_color(index: usize) -> &'static str {
    TOOL_COLORS.get(index).unwrap_or(&"\x1b[1;37m")
}

#[allow(clippy::cast_precision_loss)]
fn print_human(report: &HttpBenchReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Header
    writeln!(out, "\x1b[1mhowth bench http\x1b[0m").into_diagnostic()?;
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
        "\x1b[90mParams: {}s duration, {} connections, {}s warmup\x1b[0m",
        report.params.duration_secs, report.params.connections, report.params.warmup_secs
    )
    .into_diagnostic()?;
    writeln!(out).into_diagnostic()?;

    // Find the highest RPS
    let max_rps = report.results.iter().map(|r| r.rps).fold(0.0f64, f64::max);

    // Results table header
    writeln!(
        out,
        "{:<10} {:>12} {:>14} {:>12} {:>12} {:>8}",
        "Tool", "RPS", "Total Reqs", "Avg Lat", "p99 Lat", "Errors"
    )
    .into_diagnostic()?;
    writeln!(
        out,
        "{:-<10} {:-<12} {:-<14} {:-<12} {:-<12} {:-<8}",
        "", "", "", "", "", ""
    )
    .into_diagnostic()?;

    // Results — one row per tool (sorted by RPS, highest first)
    for (i, result) in report.results.iter().enumerate() {
        let color = tool_color(i);
        let rps = format_rps(result.rps);
        let total = format_number(result.total_requests);
        let avg_lat = format_latency(result.avg_latency_us);
        let p99_lat = format_latency(result.p99_latency_us);

        // Highlight the fastest
        if (result.rps - max_rps).abs() < 0.01 {
            writeln!(
                out,
                "{color}{:<10}\x1b[0m \x1b[1;32m{:>12}\x1b[0m {:>14} {:>12} {:>12} {:>8}",
                result.tool, rps, total, avg_lat, p99_lat, result.errors
            )
            .into_diagnostic()?;
        } else {
            writeln!(
                out,
                "{color}{:<10}\x1b[0m {:>12} {:>14} {:>12} {:>12} {:>8}",
                result.tool, rps, total, avg_lat, p99_lat, result.errors
            )
            .into_diagnostic()?;
        }
    }

    writeln!(out).into_diagnostic()?;

    // Summary comparisons
    if !report.comparisons.is_empty() {
        let howth_result = report.results.iter().find(|r| r.tool == "howth");
        if let Some(howth) = howth_result {
            writeln!(out, "\x1b[1mSummary\x1b[0m").into_diagnostic()?;
            writeln!(
                out,
                "  \x1b[1;32mhowth\x1b[0m: {} requests/sec",
                format_rps(howth.rps)
            )
            .into_diagnostic()?;

            for cmp in &report.comparisons {
                if cmp.speedup >= 1.0 {
                    writeln!(
                        out,
                        "    \x1b[1;32m{:.2}x\x1b[0m faster than {}",
                        cmp.speedup, cmp.tool
                    )
                    .into_diagnostic()?;
                } else {
                    writeln!(
                        out,
                        "    \x1b[1;31m{:.2}x\x1b[0m slower than {}",
                        1.0 / cmp.speedup,
                        cmp.tool
                    )
                    .into_diagnostic()?;
                }
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
            writeln!(out, "[{prefix}] {}: {}", warning.code, warning.message).into_diagnostic()?;
        }
    }

    out.flush().into_diagnostic()?;
    Ok(())
}

/// Format RPS with K/M suffix.
fn format_rps(rps: f64) -> String {
    if rps >= 1_000_000.0 {
        format!("{:.2}M", rps / 1_000_000.0)
    } else if rps >= 1_000.0 {
        format!("{:.2}K", rps / 1_000.0)
    } else {
        format!("{:.0}", rps)
    }
}

/// Format a large number with commas.
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format latency in microseconds.
fn format_latency(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{:.2}ms", us as f64 / 1_000.0)
    } else {
        format!("{}us", us)
    }
}
