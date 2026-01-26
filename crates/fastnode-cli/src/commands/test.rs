//! `howth test` command implementation.
//!
//! Phase 1: Node wrapper - discovers test files, transpiles TS, runs via Node's --test.

use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};
use fastnode_core::Config;
use miette::Result;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

/// Exit code for validation errors.
const EXIT_VALIDATION_ERROR: i32 = 2;

/// Exit code for internal errors.
const EXIT_INTERNAL_ERROR: i32 = 1;

/// Directories to exclude from test discovery.
const EXCLUDE_DIRS: &[&str] = &["node_modules", ".git", "dist", "build", "target", "coverage"];

/// Run the test command.
pub fn run(config: &Config) -> Result<()> {
    let cwd = &config.cwd;

    // Discover test files
    let test_files = discover_test_files(cwd);

    if test_files.is_empty() {
        println!("No test files found.");
        println!("hint: create files matching *.test.ts, *.spec.ts, etc.");
        return Ok(());
    }

    println!("Found {} test file(s)", test_files.len());

    // Separate files by type
    let (ts_files, js_files): (Vec<_>, Vec<_>) = test_files
        .into_iter()
        .partition(|f| needs_transpilation(f));

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
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // Check for .test. or .spec. pattern before extension
    (file_name.ends_with(".test") || file_name.ends_with(".spec"))
        && is_supported_extension(path)
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

/// Check if a path is in an excluded directory.
fn is_in_excluded_dir(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(name) = c {
            EXCLUDE_DIRS
                .iter()
                .any(|excluded| name == std::ffi::OsStr::new(*excluded))
        } else {
            false
        }
    })
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
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette::miette!("Failed to read file: {}", e))?;

    let backend = SwcBackend::new();

    // Create output path in temp directory
    let file_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test");
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
