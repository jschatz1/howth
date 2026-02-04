//! `howth exec` command implementation.
//!
//! Execute binaries from node_modules/.bin or PATH.

use miette::Result;
use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};

/// Exit code for binary not found errors.
const EXIT_NOT_FOUND: i32 = 127;

/// Exit code for execution errors.
const EXIT_ERROR: i32 = 1;

/// Result for JSON output.
#[derive(Serialize)]
struct ExecResult {
    ok: bool,
    binary: String,
    resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ExecError>,
}

/// Error info for JSON output.
#[derive(Serialize)]
struct ExecError {
    code: String,
    message: String,
}

/// Run a binary from node_modules/.bin or PATH.
///
/// Resolution order:
/// 1. `./node_modules/.bin/<binary>` (local project)
/// 2. Walk up directories looking for `node_modules/.bin/<binary>`
/// 3. System PATH
pub fn run(cwd: &Path, binary: &str, args: &[String], json: bool) -> Result<()> {
    // Try to find the binary
    let (resolved_path, search_path) = resolve_binary(cwd, binary);

    match &resolved_path {
        Some(path) => {
            if !json {
                // Don't print anything in non-JSON mode, just execute
            }

            // Execute the binary
            execute_binary(path, args, cwd, &search_path, json)
        }
        None => {
            if json {
                let result = ExecResult {
                    ok: false,
                    binary: binary.to_string(),
                    resolved_path: None,
                    error: Some(ExecError {
                        code: "BINARY_NOT_FOUND".to_string(),
                        message: format!(
                            "Binary '{}' not found in node_modules/.bin or PATH",
                            binary
                        ),
                    }),
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("error: binary '{}' not found", binary);
                eprintln!("hint: install with `howth pkg add {}`", binary);
            }
            std::process::exit(EXIT_NOT_FOUND);
        }
    }
}

/// Resolve a binary by searching node_modules/.bin directories and PATH.
/// Returns (resolved_path, search_path_with_bins).
fn resolve_binary(cwd: &Path, binary: &str) -> (Option<String>, String) {
    let mut bin_dirs = Vec::new();

    // Search for node_modules/.bin in current directory and parents
    let mut current = cwd.to_path_buf();
    loop {
        let bin_dir = current.join("node_modules").join(".bin");
        if bin_dir.is_dir() {
            bin_dirs.push(bin_dir.clone());

            // Check if binary exists in this .bin directory
            let binary_path = bin_dir.join(binary);
            if binary_path.exists() {
                // Build search path with all bin dirs prepended
                let system_path = std::env::var("PATH").unwrap_or_default();
                let bin_path_strs: Vec<String> = bin_dirs
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                let search_path = format!("{}:{}", bin_path_strs.join(":"), system_path);

                return (
                    Some(binary_path.to_string_lossy().into_owned()),
                    search_path,
                );
            }
        }

        // Move to parent directory
        if !current.pop() {
            break;
        }
    }

    // Build search path with all found bin dirs prepended
    let system_path = std::env::var("PATH").unwrap_or_default();
    let search_path = if bin_dirs.is_empty() {
        system_path.clone()
    } else {
        let bin_path_strs: Vec<String> = bin_dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        format!("{}:{}", bin_path_strs.join(":"), system_path)
    };

    // Not found in node_modules/.bin, check if it exists in system PATH
    if let Ok(which_path) = which::which(binary) {
        return (Some(which_path.to_string_lossy().into_owned()), search_path);
    }

    (None, search_path)
}

/// Execute a binary with the given arguments.
fn execute_binary(
    binary_path: &str,
    args: &[String],
    cwd: &Path,
    search_path: &str,
    json: bool,
) -> Result<()> {
    let mut cmd = Command::new(binary_path);

    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .env("PATH", search_path);

    match cmd.status() {
        Ok(status) => {
            // Exit with the same code as the child process
            std::process::exit(status.code().unwrap_or(EXIT_ERROR));
        }
        Err(e) => {
            if json {
                let result = ExecResult {
                    ok: false,
                    binary: binary_path.to_string(),
                    resolved_path: Some(binary_path.to_string()),
                    error: Some(ExecError {
                        code: "EXEC_FAILED".to_string(),
                        message: format!("Failed to execute '{}': {}", binary_path, e),
                    }),
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                eprintln!("error: failed to execute '{}': {}", binary_path, e);
            }
            std::process::exit(EXIT_ERROR);
        }
    }
}
