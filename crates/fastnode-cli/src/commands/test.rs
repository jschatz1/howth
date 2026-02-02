//! `howth test` command implementation.
//!
//! If package.json has a "test" script, runs that.
//! Otherwise, discovers test files and runs via daemon's warm Node worker pool
//! (falling back to direct `node --test` if the daemon is not running).

use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};
use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::Config;
use fastnode_core::VERSION;
use fastnode_daemon::ipc::MAX_FRAME_SIZE;
use fastnode_proto::{encode_frame, Frame, FrameResponse, Request, Response};
use miette::Result;
use serde_json::Value;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

/// Exit code for validation errors.
#[allow(dead_code)]
const EXIT_VALIDATION_ERROR: i32 = 2;

/// Exit code for internal errors.
const EXIT_INTERNAL_ERROR: i32 = 1;

/// Directories to exclude from test discovery.
const EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    "coverage",
];

/// Run the test command.
///
/// First checks for a "test" script in package.json and runs it.
/// If no script exists, discovers test files and tries to run via
/// the daemon's warm Node worker pool for speed. Falls back to
/// direct `node --test` if daemon is not running.
pub fn run(config: &Config, paths: &[String]) -> Result<()> {
    let cwd = &config.cwd;

    // Check for package.json test script first (only if no explicit paths given)
    if paths.is_empty() {
        if let Some(script) = get_test_script(cwd) {
            return run_test_script(cwd, &script);
        }
    }

    // Discover test files from explicit paths or cwd
    let test_files = if paths.is_empty() {
        discover_test_files(cwd)
    } else {
        let mut files = Vec::new();
        for p in paths {
            let path = if Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                cwd.join(p)
            };
            if path.is_file() && is_test_file(&path) {
                files.push(path);
            } else if path.is_dir() {
                files.extend(discover_test_files(&path));
            } else if path.is_file() {
                // Allow non-test-pattern files if explicitly specified
                files.push(path);
            }
        }
        files.sort();
        files.dedup();
        files
    };

    if test_files.is_empty() {
        println!("No test files found.");
        println!("hint: create files matching *.test.ts, *.spec.ts, etc.");
        return Ok(());
    }

    println!("Found {} test file(s)", test_files.len());

    // Try running via daemon first
    if let Some(exit_code) = try_run_via_daemon(cwd, &test_files) {
        std::process::exit(exit_code);
    }

    // Fallback: run directly via node --test
    run_direct(cwd, test_files)
}

/// Try to run tests via the daemon's warm Node worker pool.
/// Returns Some(exit_code) on success, None if daemon is unavailable.
///
/// Uses a blocking Unix socket to avoid tokio runtime startup overhead.
fn try_run_via_daemon(cwd: &Path, test_files: &[PathBuf]) -> Option<i32> {
    let endpoint = paths::ipc_endpoint(Channel::Stable);

    let file_paths: Vec<String> = test_files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();

    let result = send_run_tests_blocking(&endpoint, cwd, &file_paths);

    match result {
        Ok(response) => Some(handle_test_response(response)),
        Err(_) => {
            // Daemon not running — fall back to direct execution
            None
        }
    }
}

/// Send RunTests request to daemon using a blocking Unix socket.
/// Avoids tokio runtime initialization overhead (~2-5ms).
fn send_run_tests_blocking(
    endpoint: &str,
    cwd: &Path,
    files: &[String],
) -> std::io::Result<Response> {
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint)?;

    let frame = Frame::new(
        VERSION,
        Request::RunTests {
            cwd: cwd.to_string_lossy().into_owned(),
            files: files.to_vec(),
        },
    );
    let encoded = encode_frame(&frame)?;

    stream.write_all(&encoded)?;
    stream.flush()?;

    // Read response length prefix (4 bytes, little-endian)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("response frame too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;

    let response: FrameResponse = serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(response.response)
}

/// Handle test response from daemon and print results.
/// Returns the exit code.
fn handle_test_response(response: Response) -> i32 {
    match response {
        Response::TestRunResult { result } => {
            // Print results
            for test in &result.tests {
                let status_str = match test.status {
                    fastnode_proto::TestStatus::Pass => "\x1b[32m✓\x1b[0m",
                    fastnode_proto::TestStatus::Fail => "\x1b[31m✗\x1b[0m",
                    fastnode_proto::TestStatus::Skip => "\x1b[33m-\x1b[0m",
                };
                print!("{status_str} {}", test.name);
                if test.duration_ms > 0.0 {
                    print!(" ({:.0}ms)", test.duration_ms);
                }
                println!();
                if let Some(ref err) = test.error {
                    eprintln!("  {err}");
                }
            }

            // Summary line
            println!();
            let duration_str = if result.duration_ms >= 1000.0 {
                format!("{:.2}s", result.duration_ms / 1000.0)
            } else {
                format!("{:.0}ms", result.duration_ms)
            };

            if result.ok {
                println!(
                    "\x1b[32m{} tests passed\x1b[0m ({duration_str})",
                    result.passed
                );
            } else {
                println!(
                    "\x1b[31m{} failed\x1b[0m, {} passed ({duration_str})",
                    result.failed, result.passed
                );
            }

            if result.skipped > 0 {
                println!("{} skipped", result.skipped);
            }

            if !result.diagnostics.is_empty() {
                eprintln!("{}", result.diagnostics.trim_end());
            }

            if result.ok { 0 } else { 1 }
        }
        Response::Error { code, message } => {
            eprintln!("error: {code}: {message}");
            EXIT_INTERNAL_ERROR
        }
        _ => {
            eprintln!("error: unexpected response from daemon");
            EXIT_INTERNAL_ERROR
        }
    }
}

/// Fallback: run tests directly via transpile + node --test.
fn run_direct(cwd: &Path, test_files: Vec<PathBuf>) -> Result<()> {
    // Separate files by type
    let (ts_files, js_files): (Vec<_>, Vec<_>) =
        test_files.into_iter().partition(|f| needs_transpilation(f));

    // Transpile TypeScript files
    let mut files_to_run: Vec<PathBuf> = js_files;
    let mut temp_files: Vec<PathBuf> = Vec::new();

    for ts_file in &ts_files {
        match transpile_test_file(ts_file) {
            Ok(temp_path) => {
                files_to_run.push(temp_path.clone());
                temp_files.push(temp_path);
            }
            Err(e) => {
                eprintln!("error: failed to transpile {}: {e}", ts_file.display());
                cleanup_temp_files(&temp_files);
                std::process::exit(EXIT_INTERNAL_ERROR);
            }
        }
    }

    // Run tests via Node
    let exit_code = run_node_tests(cwd, &files_to_run);

    // Clean up temp files
    cleanup_temp_files(&temp_files);

    std::process::exit(exit_code);
}

/// Discover test files in the given directory.
fn discover_test_files(cwd: &Path) -> Vec<PathBuf> {
    let mut test_files = Vec::new();

    for entry in WalkDir::new(cwd)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && is_test_file(path) {
            test_files.push(path.to_path_buf());
        }
    }

    // Sort for deterministic order
    test_files.sort();

    test_files
}

/// Check if an entry is in an excluded directory.
fn is_excluded_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && EXCLUDE_DIRS
            .iter()
            .any(|excluded| entry.file_name() == std::ffi::OsStr::new(*excluded))
}

/// Check if a file matches test file patterns (*.test.* or *.spec.*).
fn is_test_file(path: &Path) -> bool {
    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    // Check for .test. or .spec. pattern before extension
    (file_name.ends_with(".test") || file_name.ends_with(".spec")) && is_supported_extension(path)
}

/// Check if file has a supported extension.
fn is_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" | "cts" | "cjs"
            )
        })
        .unwrap_or(false)
}

/// Check if a file needs transpilation (TypeScript/TSX/JSX).
fn needs_transpilation(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "ts" | "tsx" | "jsx" | "mts" | "cts"
            )
        })
        .unwrap_or(false)
}

/// Transpile a TypeScript test file to JavaScript.
fn transpile_test_file(path: &Path) -> Result<PathBuf> {
    let source =
        std::fs::read_to_string(path).map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    let backend = SwcBackend::new();

    // Create output path in temp directory
    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
    let temp_dir = std::env::temp_dir();
    let output_path = temp_dir.join(format!(
        "howth-test-{}-{}.mjs",
        file_name,
        std::process::id()
    ));

    let spec = TranspileSpec::new(path, &output_path);

    let output = backend
        .transpile(&spec, &source)
        .map_err(|e| miette::miette!("Transpilation failed: {}", e))?;

    std::fs::write(&output_path, &output.code)
        .map_err(|e| miette::miette!("Failed to write transpiled file: {}", e))?;

    Ok(output_path)
}

/// Run tests via Node's built-in test runner.
fn run_node_tests(cwd: &Path, files: &[PathBuf]) -> i32 {
    // Node 18+ has built-in test runner with --test flag
    let mut cmd = Command::new("node");
    cmd.arg("--test")
        .args(files)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    match cmd.status() {
        Ok(status) => status.code().unwrap_or(EXIT_INTERNAL_ERROR),
        Err(e) => {
            eprintln!("error: failed to execute node: {e}");
            eprintln!("hint: Is Node.js 18+ installed?");
            EXIT_VALIDATION_ERROR
        }
    }
}

/// Clean up temporary transpiled files.
fn cleanup_temp_files(files: &[PathBuf]) {
    for file in files {
        let _ = std::fs::remove_file(file);
    }
}

/// Check for a "test" script in package.json.
fn get_test_script(cwd: &Path) -> Option<String> {
    let package_json_path = cwd.join("package.json");
    let content = std::fs::read_to_string(&package_json_path).ok()?;
    let package: Value = serde_json::from_str(&content).ok()?;

    let script = package
        .get("scripts")?
        .get("test")?
        .as_str()?;

    // Avoid infinite recursion if the test script invokes howth test
    let trimmed = script.trim();
    if trimmed == "howth test"
        || trimmed == "fastnode test"
        || trimmed.starts_with("howth test ")
        || trimmed.starts_with("fastnode test ")
    {
        return None;
    }

    Some(script.to_string())
}

/// Run the test script from package.json.
fn run_test_script(cwd: &Path, script: &str) -> Result<()> {
    use std::io::Write;

    println!("$ {}", script);
    let _ = std::io::stdout().flush();

    #[cfg(unix)]
    let mut cmd = {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    };

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(script);
        c
    };

    cmd.current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Add node_modules/.bin to PATH
    let node_modules_bin = cwd.join("node_modules").join(".bin");
    if node_modules_bin.exists() {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", node_modules_bin.display(), path);
        cmd.env("PATH", new_path);
    }

    let status = cmd
        .status()
        .map_err(|e| miette::miette!("Failed to execute test script: {}", e))?;

    std::process::exit(status.code().unwrap_or(EXIT_INTERNAL_ERROR));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_transpilation() {
        assert!(needs_transpilation(Path::new("foo.ts")));
        assert!(needs_transpilation(Path::new("foo.tsx")));
        assert!(needs_transpilation(Path::new("foo.jsx")));
        assert!(needs_transpilation(Path::new("foo.mts")));
        assert!(!needs_transpilation(Path::new("foo.js")));
        assert!(!needs_transpilation(Path::new("foo.mjs")));
    }

    #[test]
    fn test_is_test_file() {
        assert!(is_test_file(Path::new("foo.test.ts")));
        assert!(is_test_file(Path::new("foo.test.tsx")));
        assert!(is_test_file(Path::new("foo.spec.ts")));
        assert!(is_test_file(Path::new("foo.spec.js")));
        assert!(is_test_file(Path::new("bar.test.mjs")));
        assert!(!is_test_file(Path::new("foo.ts")));
        assert!(!is_test_file(Path::new("test.ts"))); // no .test. before extension
        assert!(!is_test_file(Path::new("test.config.ts"))); // not .test or .spec
    }

    #[test]
    fn test_is_supported_extension() {
        assert!(is_supported_extension(Path::new("foo.ts")));
        assert!(is_supported_extension(Path::new("foo.tsx")));
        assert!(is_supported_extension(Path::new("foo.js")));
        assert!(is_supported_extension(Path::new("foo.jsx")));
        assert!(is_supported_extension(Path::new("foo.mts")));
        assert!(is_supported_extension(Path::new("foo.mjs")));
        assert!(!is_supported_extension(Path::new("foo.py")));
        assert!(!is_supported_extension(Path::new("foo.rs")));
    }
}
