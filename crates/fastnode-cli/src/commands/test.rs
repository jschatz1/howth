//! `howth test` command implementation.
//!
//! If package.json has a "test" script, runs that.
//! Otherwise, discovers test files and runs via daemon's warm Node worker pool
//! (falling back to direct `node --test` if the daemon is not running).

use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};
use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::Config;
#[cfg(unix)]
use fastnode_core::VERSION;
#[cfg(unix)]
use fastnode_daemon::ipc::MAX_FRAME_SIZE;
use fastnode_proto::Response;
#[cfg(unix)]
use fastnode_proto::{encode_frame, Frame, FrameResponse, Request};
use miette::Result;
use serde_json::Value;
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
pub fn run(
    config: &Config,
    setup: Option<&str>,
    timeout: Option<u64>,
    force_exit: bool,
    paths: &[String],
) -> Result<()> {
    let cwd = &config.cwd;

    // Check for package.json test script first (only if no howth-specific flags given)
    let has_howth_flags = setup.is_some() || timeout.is_some() || force_exit;
    if paths.is_empty() && !has_howth_flags {
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
    for f in &test_files {
        println!("  {}", f.display());
    }

    // Resolve setup file path
    let setup_path = setup.map(|s| {
        let p = Path::new(s);
        if p.is_absolute() {
            PathBuf::from(s)
        } else {
            cwd.join(s)
        }
    });

    // Try running via daemon first
    if let Some(exit_code) =
        try_run_via_daemon(cwd, &test_files, setup_path.as_deref(), timeout, force_exit)
    {
        std::process::exit(exit_code);
    }

    // Fallback: run directly via node --test
    run_direct(cwd, test_files, setup_path.as_deref(), force_exit)
}

/// Try to run tests via the daemon's warm Node worker pool.
/// Returns Some(exit_code) on success, None if daemon is unavailable.
///
/// Uses a blocking Unix socket to avoid tokio runtime startup overhead.
fn try_run_via_daemon(
    cwd: &Path,
    test_files: &[PathBuf],
    setup: Option<&Path>,
    timeout: Option<u64>,
    force_exit: bool,
) -> Option<i32> {
    let endpoint = paths::ipc_endpoint(Channel::Stable);

    let file_paths: Vec<String> = test_files
        .iter()
        .map(|f| f.to_string_lossy().into_owned())
        .collect();

    let setup_str = setup.map(|p| p.to_string_lossy().into_owned());

    let result = send_run_tests_blocking(
        &endpoint,
        cwd,
        &file_paths,
        setup_str.as_deref(),
        timeout,
        force_exit,
    );

    match result {
        Ok(response) => Some(handle_test_response(response)),
        Err(_) => {
            // Daemon not running — fall back to direct execution
            None
        }
    }
}

/// Send RunTests request to daemon using a blocking socket.
/// Avoids tokio runtime initialization overhead (~2-5ms).
#[cfg(unix)]
fn send_run_tests_blocking(
    endpoint: &str,
    cwd: &Path,
    files: &[String],
    setup: Option<&str>,
    timeout: Option<u64>,
    force_exit: bool,
) -> std::io::Result<Response> {
    let mut stream = std::os::unix::net::UnixStream::connect(endpoint)?;
    send_run_tests_blocking_impl(&mut stream, cwd, files, setup, timeout, force_exit)
}

/// Send RunTests request to daemon using named pipes on Windows.
#[cfg(windows)]
fn send_run_tests_blocking(
    endpoint: &str,
    _cwd: &Path,
    _files: &[String],
    _setup: Option<&str>,
    _timeout: Option<u64>,
    _force_exit: bool,
) -> std::io::Result<Response> {
    // On Windows, we can't use blocking named pipes easily without tokio.
    // Return an error indicating daemon mode isn't supported for blocking tests on Windows.
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("Blocking daemon connection not supported on Windows. Use async mode or run tests directly. Endpoint: {endpoint}"),
    ))
}

/// Common implementation for sending test request over a stream.
#[cfg(unix)]
fn send_run_tests_blocking_impl(
    stream: &mut (impl std::io::Read + std::io::Write),
    cwd: &Path,
    files: &[String],
    setup: Option<&str>,
    timeout: Option<u64>,
    force_exit: bool,
) -> std::io::Result<Response> {
    let frame = Frame::new(
        VERSION,
        Request::RunTests {
            cwd: cwd.to_string_lossy().into_owned(),
            files: files.to_vec(),
            setup: setup.map(String::from),
            timeout_ms: timeout,
            force_exit,
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
                    for line in err.lines() {
                        eprintln!("    {line}");
                    }
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

            i32::from(!result.ok)
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
fn run_direct(
    cwd: &Path,
    test_files: Vec<PathBuf>,
    setup: Option<&Path>,
    force_exit: bool,
) -> Result<()> {
    // Separate files by type
    let (ts_files, js_files): (Vec<_>, Vec<_>) =
        test_files.into_iter().partition(|f| needs_transpilation(f));

    // Write the howth:mocha shim for .timeout() chaining support
    let shim_dir = std::env::temp_dir().join("howth-test-worker");
    let _ = std::fs::create_dir_all(&shim_dir);
    let shim_path = shim_dir.join("howth-mocha-shim.mjs");
    let _ = std::fs::write(
        &shim_path,
        r#"
import { describe as _describe, it as _it, before, after, beforeEach, afterEach } from 'node:test';
function chainable(result) {
  const c = { timeout() { return c; }, slow() { return c; }, retries() { return c; } };
  if (result && typeof result.then === 'function') { c.then = result.then.bind(result); c.catch = result.catch.bind(result); }
  return c;
}
const mochaCtx = { timeout() { return mochaCtx; }, slow() { return mochaCtx; }, retries() { return mochaCtx; }, skip() {} };
function bindCtx(fn) { if (!fn) return fn; return function(...a) { return fn.call(mochaCtx, ...a); }; }
function describe(name, fn) { return chainable(_describe(name, bindCtx(fn))); }
describe.only = function(name, fn) { return chainable(_describe(name, { only: true }, bindCtx(fn))); };
describe.skip = function(name, fn) { return chainable(_describe(name, { skip: true }, bindCtx(fn))); };
const context = describe;
function it(name, fn) { return chainable(_it(name, bindCtx(fn))); }
it.only = function(name, fn) { return chainable(_it(name, { only: true }, bindCtx(fn))); };
it.skip = function(name, fn) { return chainable(_it(name, { skip: true }, bindCtx(fn))); };
const specify = it;
export { describe, context, it, specify, before, after, beforeEach, afterEach };
export default describe;
"#,
    );
    let shim_str = shim_path.to_string_lossy().to_string();

    // Rewrite howth:mocha to shim in plain JS files that use it.
    // Write temp files next to originals (for node_modules resolution) with
    // .test/.spec stripped from the name (so node:test doesn't discover them).
    let mut files_to_run: Vec<PathBuf> = Vec::new();
    let mut temp_files: Vec<PathBuf> = Vec::new();
    for js_file in &js_files {
        if let Ok(source) = std::fs::read_to_string(js_file) {
            let needs_rewrite = source.contains("howth:mocha")
                || source.contains("from 'mocha'")
                || source.contains("from \"mocha\"")
                || source.contains("require('mocha')")
                || source.contains("require(\"mocha\")");
            if needs_rewrite {
                let rewritten = source
                    .replace("howth:mocha", &shim_str)
                    .replace("from 'mocha'", &format!("from '{shim_str}'"))
                    .replace("from \"mocha\"", &format!("from \"{shim_str}\""))
                    .replace("require('mocha')", &format!("require('{shim_str}')"))
                    .replace("require(\"mocha\")", &format!("require(\"{shim_str}\")"));
                let dir = js_file.parent().unwrap_or(cwd);
                let stem = js_file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("test");
                let name = stem
                    .strip_suffix(".test")
                    .or_else(|| stem.strip_suffix(".spec"))
                    .unwrap_or(stem);
                let ext = js_file
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("mjs");
                let temp_path = dir.join(format!(
                    ".howth-test-{}-{}.{}",
                    name,
                    std::process::id(),
                    ext,
                ));
                let _ = std::fs::write(&temp_path, rewritten);
                files_to_run.push(temp_path.clone());
                temp_files.push(temp_path);
                continue;
            }
        }
        files_to_run.push(js_file.clone());
    }

    // Transpile TypeScript files

    for ts_file in &ts_files {
        match transpile_test_file(ts_file, Some(&shim_str)) {
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

    // Prepend setup file if provided
    if let Some(setup_path) = setup {
        if needs_transpilation(setup_path) {
            match transpile_test_file(setup_path, Some(&shim_str)) {
                Ok(temp_path) => {
                    files_to_run.insert(0, temp_path.clone());
                    temp_files.push(temp_path);
                }
                Err(e) => {
                    eprintln!(
                        "error: failed to transpile setup file {}: {e}",
                        setup_path.display()
                    );
                    cleanup_temp_files(&temp_files);
                    std::process::exit(EXIT_INTERNAL_ERROR);
                }
            }
        } else {
            files_to_run.insert(0, setup_path.to_path_buf());
        }
    }

    // Run tests via Node
    let exit_code = if force_exit {
        run_node_tests_force_exit(cwd, &files_to_run)
    } else {
        run_node_tests(cwd, &files_to_run)
    };

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
        .filter_map(std::result::Result::ok)
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
/// Writes the output next to the original file (for node_modules resolution)
/// with .test/.spec stripped from the name (so node:test doesn't discover it).
fn transpile_test_file(path: &Path, mocha_shim: Option<&str>) -> Result<PathBuf> {
    let source =
        std::fs::read_to_string(path).map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    let backend = SwcBackend::new();

    // Write next to the original so Node's module resolution finds node_modules.
    // Strip .test/.spec from the name to avoid node:test discovery.
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
    let name = stem
        .strip_suffix(".test")
        .or_else(|| stem.strip_suffix(".spec"))
        .unwrap_or(stem);
    let output_path = dir.join(format!(".howth-test-{}-{}.mjs", name, std::process::id()));

    let spec = TranspileSpec::new(path, &output_path);

    let output = backend
        .transpile(&spec, &source)
        .map_err(|e| miette::miette!("Transpilation failed: {}", e))?;

    // Rewrite mocha imports to shim (or node:test) since the fallback path uses Node.js
    let replacement = mocha_shim.unwrap_or("node:test");
    let code = output
        .code
        .replace("howth:mocha", replacement)
        .replace("from 'mocha'", &format!("from '{replacement}'"))
        .replace("from \"mocha\"", &format!("from \"{replacement}\""))
        .replace("require('mocha')", &format!("require('{replacement}')"))
        .replace("require(\"mocha\")", &format!("require(\"{replacement}\")"));
    std::fs::write(&output_path, &code)
        .map_err(|e| miette::miette!("Failed to write transpiled file: {}", e))?;

    Ok(output_path)
}

/// Run tests via a wrapper that forces process.exit() after tests complete.
/// Uses node:test's programmatic API with isolation:'none' and idle detection,
/// so open handles (Express servers, DB connections) don't prevent exit.
fn run_node_tests_force_exit(cwd: &Path, files: &[PathBuf]) -> i32 {
    let wrapper_dir = std::env::temp_dir().join("howth-test-worker");
    let _ = std::fs::create_dir_all(&wrapper_dir);
    let wrapper_path = wrapper_dir.join("force-exit-runner.mjs");
    let _ = std::fs::write(
        &wrapper_path,
        r#"
import { describe as _describe, it as _it, before, after, beforeEach, afterEach } from 'node:test';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Track individual test completion to know when all tests are done.
// We wrap it() to count registrations and completions, then force-exit
// once all tests finish — even if open handles (Express, DB) remain.
let registered = 0;
let completed = 0;
let failed = false;
let totalExpected = 0; // snapshot after all imports

function checkDone() {
  // Only check after we know the final count
  if (totalExpected > 0 && completed >= totalExpected) {
    // Let node:test's reporter flush output before exiting
    setTimeout(() => process.exit(failed ? 1 : 0), 100);
  }
}

// Wrap it() to track completion
function it(name, opts, fn) {
  if (typeof opts === 'function') { fn = opts; opts = undefined; }
  registered++;
  const wrappedFn = async (...args) => {
    try {
      return await fn(...args);
    } catch (e) {
      failed = true;
      throw e;
    } finally {
      completed++;
      checkDone();
    }
  };
  return opts ? _it(name, opts, wrappedFn) : _it(name, wrappedFn);
}
it.only = function(name, fn) { return it(name, { only: true }, fn); };
it.skip = function(name, fn) { registered++; completed++; return _it.skip(name, fn); };

// Mocha compatibility: bindCtx provides a mock `this` with chainable stubs
const mochaCtx = { timeout() { return mochaCtx; }, slow() { return mochaCtx; }, retries() { return mochaCtx; }, skip() {} };
function bindCtx(fn) { if (!fn) return fn; return function(...a) { return fn.call(mochaCtx, ...a); }; }
function describe(name, fn) { return _describe(name, bindCtx(fn)); }
describe.only = function(name, fn) { return _describe(name, { only: true }, bindCtx(fn)); };
describe.skip = function(name, fn) { return _describe(name, { skip: true }, bindCtx(fn)); };

globalThis.describe = describe;
globalThis.context = describe;
globalThis.it = it;
globalThis.specify = it;
globalThis.before = before;
globalThis.after = after;
globalThis.beforeEach = beforeEach;
globalThis.afterEach = afterEach;

// Import each test file — global describe/it calls register with node:test
const files = process.argv.slice(2).map(f => resolve(f));

for (const file of files) {
  try {
    await import(pathToFileURL(file).href);
  } catch (err) {
    failed = true;
    console.error(`\n✖ ${file}`);
    console.error(err);
  }
}

// Now that all files are imported, snapshot the final registration count.
// Tests may have already started completing during imports, but we only
// start checking completion against the final total from this point.
totalExpected = registered;
if (totalExpected === 0) {
  setTimeout(() => process.exit(failed ? 1 : 0), 500);
} else {
  checkDone();
}
"#,
    );

    let mut cmd = Command::new("node");
    cmd.arg(&wrapper_path)
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

    let script = package.get("scripts")?.get("test")?.as_str()?;

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
