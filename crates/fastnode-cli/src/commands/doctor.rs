use fastnode_core::config::Channel;
use fastnode_core::doctor::{DoctorReport, Severity};
use miette::{IntoDiagnostic, Result};
use std::io::{self, Write};
use std::path::Path;

/// Run the doctor command.
///
/// When `json` is true, outputs a single JSON object to stdout.
/// Otherwise, outputs human-readable formatted text to stdout.
pub fn run(cwd: &Path, channel: Channel, json: bool) -> Result<()> {
    let report = DoctorReport::collect(cwd, channel);

    if json {
        print_json(&report)?;
    } else {
        print_human(&report)?;
    }

    Ok(())
}

fn print_json(report: &DoctorReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report).into_diagnostic()?;
    println!("{json}");
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn print_human(report: &DoctorReport) -> Result<()> {
    let mut out = io::stdout().lock();

    // Runtime
    w(&mut out, "\x1b[1m## Runtime\x1b[0m\n")?;
    w(
        &mut out,
        &format!("  Version:        {}\n", report.runtime.version),
    )?;
    w(
        &mut out,
        &format!("  Schema:         v{}\n", report.runtime.schema_version),
    )?;
    w(
        &mut out,
        &format!("  Channel:        {}\n", report.runtime.channel),
    )?;
    w(&mut out, "\n")?;

    // OS
    w(&mut out, "\x1b[1m## OS\x1b[0m\n")?;
    w(&mut out, &format!("  Name:           {}\n", report.os.name))?;
    w(
        &mut out,
        &format!(
            "  Version:        {}\n",
            report.os.version.as_deref().unwrap_or("unknown")
        ),
    )?;
    w(&mut out, &format!("  Arch:           {}\n", report.os.arch))?;
    w(&mut out, "\n")?;

    // Hardware
    w(&mut out, "\x1b[1m## Hardware\x1b[0m\n")?;
    w(
        &mut out,
        &format!("  CPU Cores:      {}\n", report.hardware.cpu_cores),
    )?;
    if let Some(physical) = report.hardware.cpu_cores_physical {
        w(&mut out, &format!("  Physical Cores: {physical}\n"))?;
    }
    w(&mut out, "\n")?;

    // Paths
    w(&mut out, "\x1b[1m## Paths\x1b[0m\n")?;
    w(
        &mut out,
        &format!("  CWD:            {}\n", report.paths.cwd.display()),
    )?;
    w(
        &mut out,
        &format!(
            "  Cache:          {} {}\n",
            report.paths.cache_dir.display(),
            if report.paths.cache_writable {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[31m✗\x1b[0m"
            }
        ),
    )?;
    w(
        &mut out,
        &format!(
            "  Data:           {} {}\n",
            report.paths.data_dir.display(),
            if report.paths.data_writable {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[31m✗\x1b[0m"
            }
        ),
    )?;
    w(&mut out, "\n")?;

    // Project
    w(&mut out, "\x1b[1m## Project\x1b[0m\n")?;
    match &report.project.root {
        Some(root) => {
            w(&mut out, &format!("  Root:           {}\n", root.display()))?;
            w(
                &mut out,
                &format!(
                    "  package.json:   {}\n",
                    if report.project.has_package_json {
                        "yes"
                    } else {
                        "no"
                    }
                ),
            )?;
            w(
                &mut out,
                &format!(
                    "  .git:           {}\n",
                    if report.project.has_git { "yes" } else { "no" }
                ),
            )?;
        }
        None => {
            w(&mut out, "  Root:           (not found)\n")?;
        }
    }
    w(&mut out, "\n")?;

    // Capabilities
    w(&mut out, "\x1b[1m## Capabilities\x1b[0m\n")?;
    w(
        &mut out,
        &format!(
            "  Case-sensitive: {}\n",
            yes_no(report.capabilities.fs_case_sensitive)
        ),
    )?;
    w(
        &mut out,
        &format!(
            "  Symlinks:       {}\n",
            yes_no(report.capabilities.symlink_supported)
        ),
    )?;
    w(
        &mut out,
        &format!(
            "  Hardlinks:      {}\n",
            yes_no(report.capabilities.hardlink_supported)
        ),
    )?;

    #[cfg(target_os = "linux")]
    if let Some(supported) = report.capabilities.io_uring_supported {
        w(
            &mut out,
            &format!("  io_uring:       {}\n", yes_no(supported)),
        )?;
    }

    #[cfg(unix)]
    if let Some(ref rlimit) = report.capabilities.rlimit_nofile {
        w(
            &mut out,
            &format!(
                "  Open files:     {} / {} (soft/hard)\n",
                rlimit.soft, rlimit.hard
            ),
        )?;
    }

    w(&mut out, "\n")?;

    // Warnings
    if report.warnings.is_empty() {
        w(&mut out, "\x1b[1m## Warnings\x1b[0m\n")?;
        w(&mut out, "  \x1b[32mNo warnings\x1b[0m\n")?;
    } else {
        w(
            &mut out,
            &format!(
                "\x1b[1m## Warnings\x1b[0m ({} total)\n",
                report.warnings.len()
            ),
        )?;
        for warning in &report.warnings {
            let prefix = match warning.severity {
                Severity::Info => "\x1b[34minfo\x1b[0m",
                Severity::Warn => "\x1b[33mwarn\x1b[0m",
            };
            w(
                &mut out,
                &format!("  [{prefix}] {}: {}\n", warning.code, warning.message),
            )?;
        }
    }

    out.flush().into_diagnostic()?;
    Ok(())
}

fn w(out: &mut impl Write, s: &str) -> Result<()> {
    out.write_all(s.as_bytes()).into_diagnostic()
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
