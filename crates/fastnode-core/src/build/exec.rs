//! Build execution engine.
//!
//! Executes build nodes, respecting dependencies and caching.
//!
//! ## Cache with Output Fingerprinting (v2.2)
//!
//! The build cache stores both input hashes and output fingerprints:
//! - When a node has declared outputs, a cache hit requires BOTH:
//!   1. Input hash matches cached input hash
//!   2. Output fingerprint matches cached output fingerprint
//! - When no outputs are declared, only input hash is checked (legacy behavior)
//!
//! ## Build Reasons (v2.3)
//!
//! The `BuildNodeReason` enum explains why a node was rebuilt or skipped,
//! enabling deterministic "why" explanations for debugging builds.

#![allow(clippy::if_not_else)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::manual_div_ceil)]

use super::codes;
use super::fingerprint::{compute_fingerprint, OutputFingerprint};
use super::graph::{
    BuildErrorInfo, BuildGraph, BuildNode, BuildNodeKind, BuildNodeReason, BuildNodeResult,
    BuildRunResult, CacheStatus, MAX_OUTPUT_SIZE,
};
use crate::compiler::{CompilerBackend, TranspileSpec};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// Options for build execution.
#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    /// Force rebuild (bypass cache).
    pub force: bool,
    /// Dry run (don't execute, just plan).
    pub dry_run: bool,
    /// Maximum parallel jobs.
    pub max_parallel: usize,
    /// Include profiling information.
    pub profile: bool,
    /// Target nodes to execute (empty = all nodes).
    /// Only nodes in this set (and their dependencies) will be executed.
    pub targets: Vec<String>,
}

impl ExecOptions {
    /// Create new options with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            force: false,
            dry_run: false,
            max_parallel: num_cpus(),
            profile: false,
            targets: Vec::new(),
        }
    }

    /// Set force mode.
    #[must_use]
    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    /// Set dry run mode.
    #[must_use]
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Set target nodes to execute (empty = all nodes).
    #[must_use]
    pub fn with_targets(mut self, targets: Vec<String>) -> Self {
        self.targets = targets;
        self
    }
}

/// Get number of CPUs (clamped to 1..=64).
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .clamp(1, 64)
}

/// Cache entry with fingerprint support (v2.2).
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Input hash.
    pub hash: String,
    /// Whether the build succeeded.
    pub ok: bool,
    /// Output fingerprint (None if no outputs declared).
    pub fingerprint: Option<OutputFingerprint>,
}

impl CacheEntry {
    /// Create a new cache entry without fingerprint (legacy).
    #[must_use]
    pub fn new(hash: &str, ok: bool) -> Self {
        Self {
            hash: hash.to_string(),
            ok,
            fingerprint: None,
        }
    }

    /// Create a new cache entry with fingerprint.
    #[must_use]
    pub fn with_fingerprint(hash: &str, ok: bool, fingerprint: Option<OutputFingerprint>) -> Self {
        Self {
            hash: hash.to_string(),
            ok,
            fingerprint,
        }
    }
}

/// Cache interface for build results.
pub trait BuildCache {
    /// Check if a node hash is cached and was successful.
    ///
    /// Returns `Some(true)` if cached and successful, `Some(false)` if cached and failed,
    /// `None` if not cached or hash doesn't match.
    fn get(&self, node_id: &str, hash: &str) -> Option<bool>;

    /// Get the full cache entry for a node.
    ///
    /// Returns the entry if the hash matches, None otherwise.
    fn get_entry(&self, node_id: &str, hash: &str) -> Option<CacheEntry>;

    /// Store a result for a node.
    fn set(&mut self, node_id: &str, hash: &str, ok: bool);

    /// Store a result with fingerprint for a node (v2.2).
    fn set_with_fingerprint(
        &mut self,
        node_id: &str,
        hash: &str,
        ok: bool,
        fingerprint: Option<OutputFingerprint>,
    );

    /// Invalidate cache for a node.
    fn invalidate(&mut self, node_id: &str);

    /// Clear all cache entries.
    fn clear(&mut self);
}

/// In-memory build cache with fingerprint support (v2.2).
#[derive(Debug, Default)]
pub struct MemoryCache {
    /// Map of `node_id` -> `CacheEntry`
    entries: HashMap<String, CacheEntry>,
}

impl MemoryCache {
    /// Create a new memory cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get an entry directly (for testing).
    #[cfg(test)]
    #[must_use]
    pub fn get_raw(&self, node_id: &str) -> Option<&CacheEntry> {
        self.entries.get(node_id)
    }
}

impl BuildCache for MemoryCache {
    fn get(&self, node_id: &str, hash: &str) -> Option<bool> {
        self.entries
            .get(node_id)
            .filter(|entry| entry.hash == hash)
            .map(|entry| entry.ok)
    }

    fn get_entry(&self, node_id: &str, hash: &str) -> Option<CacheEntry> {
        self.entries
            .get(node_id)
            .filter(|entry| entry.hash == hash)
            .cloned()
    }

    fn set(&mut self, node_id: &str, hash: &str, ok: bool) {
        self.entries
            .insert(node_id.to_string(), CacheEntry::new(hash, ok));
    }

    fn set_with_fingerprint(
        &mut self,
        node_id: &str,
        hash: &str,
        ok: bool,
        fingerprint: Option<OutputFingerprint>,
    ) {
        self.entries.insert(
            node_id.to_string(),
            CacheEntry::with_fingerprint(hash, ok, fingerprint),
        );
    }

    fn invalidate(&mut self, node_id: &str) {
        self.entries.remove(node_id);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Output from running a script.
#[derive(Debug, Default)]
pub struct ScriptOutput {
    /// Exit code.
    pub exit_code: i32,
    /// Captured stdout (may be truncated).
    pub stdout: String,
    /// Captured stderr (may be truncated).
    pub stderr: String,
    /// Whether stdout was truncated.
    pub stdout_truncated: bool,
    /// Whether stderr was truncated.
    pub stderr_truncated: bool,
}

/// Run a script command.
///
/// # Errors
/// Returns an error if the shell command fails to spawn or wait.
pub fn run_script(command: &str, cwd: &Path) -> io::Result<ScriptOutput> {
    let (shell, shell_arg) = if cfg!(windows) {
        ("cmd.exe", "/C")
    } else {
        ("sh", "-c")
    };

    let mut child = Command::new(shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut output = ScriptOutput::default();

    // Read stdout
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if output.stdout.len() + line.len() + 1 > MAX_OUTPUT_SIZE {
                output.stdout_truncated = true;
                break;
            }
            if !output.stdout.is_empty() {
                output.stdout.push('\n');
            }
            output.stdout.push_str(&line);
        }
    }

    // Read stderr
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if output.stderr.len() + line.len() + 1 > MAX_OUTPUT_SIZE {
                output.stderr_truncated = true;
                break;
            }
            if !output.stderr.is_empty() {
                output.stderr.push('\n');
            }
            output.stderr.push_str(&line);
        }
    }

    let status = child.wait()?;
    output.exit_code = status.code().unwrap_or(-1);

    Ok(output)
}

/// Execute a single build node.
///
/// ## Cache with Fingerprint Verification (v2.2)
///
/// When a node has declared outputs:
/// 1. Input hash must match cached input hash
/// 2. Output fingerprint must match cached output fingerprint
///
/// When no outputs are declared, only input hash is checked.
///
/// ## Lazy Fingerprinting (v3.5)
///
/// On first build (cache cold), fingerprint computation is skipped.
/// On subsequent cache lookups, if fingerprint is None, we compute it lazily
/// and update the cache, returning a cache hit without re-executing.
///
/// ## Build Reasons (v2.3)
///
/// The function tracks why a node was rebuilt:
/// - `Forced`: --force flag was used
/// - `FirstBuild`: No cache entry existed
/// - `OutputsChanged`: Fingerprint mismatch (outputs modified externally)
#[must_use]
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
pub fn execute_node(
    node: &BuildNode,
    cwd: &Path,
    hash: &str,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> BuildNodeResult {
    let has_outputs = !node.outputs.is_empty();

    // Track reason for rebuild (v2.3)
    let mut rebuild_reason = BuildNodeReason::FirstBuild; // Default: cache cold

    // Check cache unless force
    if options.force {
        rebuild_reason = BuildNodeReason::Forced;
    } else if let Some(cache) = cache.as_ref() {
        if let Some(entry) = cache.get_entry(&node.id, hash) {
            if entry.ok {
                // v2.2: If outputs are declared, verify fingerprint matches
                if has_outputs {
                    // Compute current output fingerprint
                    let current_fingerprint = compute_fingerprint(&node.outputs, cwd).ok();

                    // Check if fingerprint matches cached
                    let fingerprint_matches = match (&current_fingerprint, &entry.fingerprint) {
                        (Some(Some(current)), Some(cached)) => current.hash == cached.hash,
                        (Some(None), None) => true, // Both have no outputs
                        _ => false,
                    };

                    if fingerprint_matches {
                        return BuildNodeResult::cache_hit(&node.id, hash);
                    }
                    // Fingerprint mismatch - need to rebuild
                    rebuild_reason = BuildNodeReason::OutputsChanged;
                } else {
                    // No outputs declared - legacy behavior
                    return BuildNodeResult::cache_hit(&node.id, hash);
                }
            }
            // Entry exists but failed - rebuild
        }
        // No cache entry - FirstBuild (already set)
    }

    // Dry run - don't execute
    if options.dry_run {
        let mut result = BuildNodeResult::cache_miss_with_reason(&node.id, hash, 0, rebuild_reason);
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };
        result.notes.push("dry run - not executed".to_string());
        return result;
    }

    // Execute the script
    let Some(script) = &node.script else {
        return BuildNodeResult::failed(
            &node.id,
            hash,
            0,
            BuildErrorInfo::new(
                codes::BUILD_SCRIPT_NOT_FOUND,
                "No script specified for node",
            ),
        );
    };

    let start = Instant::now();
    let output = match run_script(&script.command, cwd) {
        Ok(out) => out,
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            return BuildNodeResult::failed(
                &node.id,
                hash,
                duration_ms,
                BuildErrorInfo::new(codes::BUILD_SCRIPT_FAILED, format!("Failed to spawn: {e}")),
            );
        }
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    if output.exit_code != 0 {
        let error = BuildErrorInfo::new(
            codes::BUILD_SCRIPT_FAILED,
            format!("Exit code {}", output.exit_code),
        )
        .with_detail(if output.stderr.is_empty() {
            String::new()
        } else {
            // Get last 20 lines of stderr
            output
                .stderr
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        });

        let mut result = BuildNodeResult::failed(&node.id, hash, duration_ms, error);
        result.stdout_truncated = output.stdout_truncated;
        result.stderr_truncated = output.stderr_truncated;
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };

        // Update cache with failure (no fingerprint for failures)
        if let Some(cache) = cache {
            cache.set(&node.id, hash, false);
        }

        return result;
    }

    // Success - compute output fingerprint if outputs are declared
    let fingerprint = if has_outputs {
        compute_fingerprint(&node.outputs, cwd).ok().flatten()
    } else {
        None
    };

    let mut result =
        BuildNodeResult::cache_miss_with_reason(&node.id, hash, duration_ms, rebuild_reason);
    result.stdout_truncated = output.stdout_truncated;
    result.stderr_truncated = output.stderr_truncated;
    result.cache = if options.force {
        CacheStatus::Bypass
    } else {
        CacheStatus::Miss
    };

    // Update cache with fingerprint
    if let Some(cache) = cache {
        cache.set_with_fingerprint(&node.id, hash, true, fingerprint);
    }

    result
}

/// Execute a transpile node using the compiler backend.
///
/// This function:
/// 1. Reads the input file
/// 2. Transpiles using the provided compiler backend
/// 3. Writes the output file
/// 4. Optionally writes a source map file
///
/// v3.5: Uses lazy fingerprinting - skips fingerprint on first build.
///
/// Returns a `BuildNodeResult` with success/failure status.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn execute_transpile(
    node: &BuildNode,
    cwd: &Path,
    hash: &str,
    spec: &TranspileSpec,
    backend: &dyn CompilerBackend,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> BuildNodeResult {
    let has_outputs = !node.outputs.is_empty();

    // Track reason for rebuild (v2.3)
    let mut rebuild_reason = BuildNodeReason::FirstBuild;

    // Check cache unless force
    if options.force {
        rebuild_reason = BuildNodeReason::Forced;
    } else if let Some(cache) = cache.as_ref() {
        if let Some(entry) = cache.get_entry(&node.id, hash) {
            if entry.ok {
                if has_outputs {
                    // v3.5: Lazy fingerprinting - if cached fingerprint is None,
                    // trust the cache entry (first build used lazy fingerprinting)
                    if entry.fingerprint.is_none() {
                        return BuildNodeResult::cache_hit(&node.id, hash);
                    }

                    let current_fingerprint = compute_fingerprint(&node.outputs, cwd).ok();
                    let fingerprint_matches = match (&current_fingerprint, &entry.fingerprint) {
                        (Some(Some(current)), Some(cached)) => current.hash == cached.hash,
                        (Some(None), None) => true,
                        _ => false,
                    };

                    if fingerprint_matches {
                        return BuildNodeResult::cache_hit(&node.id, hash);
                    }
                    rebuild_reason = BuildNodeReason::OutputsChanged;
                } else {
                    return BuildNodeResult::cache_hit(&node.id, hash);
                }
            }
        }
    }

    // Dry run - don't execute
    if options.dry_run {
        let mut result = BuildNodeResult::cache_miss_with_reason(&node.id, hash, 0, rebuild_reason);
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };
        result.notes.push("dry run - not executed".to_string());
        return result;
    }

    let start = Instant::now();

    // Read input file
    let input_path = cwd.join(&spec.input_path);
    let source = match fs::read_to_string(&input_path) {
        Ok(s) => s,
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            return BuildNodeResult::failed(
                &node.id,
                hash,
                duration_ms,
                BuildErrorInfo::new(
                    codes::BUILD_TRANSPILE_READ_ERROR,
                    format!("Failed to read input file: {e}"),
                ),
            );
        }
    };

    // Transpile
    let output = match backend.transpile(spec, &source) {
        Ok(out) => out,
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            return BuildNodeResult::failed(
                &node.id,
                hash,
                duration_ms,
                BuildErrorInfo::new(codes::BUILD_TRANSPILE_FAILED, e.to_string()),
            );
        }
    };

    // Write output file
    let output_path = cwd.join(&spec.output_path);
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                let duration_ms = start.elapsed().as_millis() as u64;
                return BuildNodeResult::failed(
                    &node.id,
                    hash,
                    duration_ms,
                    BuildErrorInfo::new(
                        codes::BUILD_TRANSPILE_WRITE_ERROR,
                        format!("Failed to create output directory: {e}"),
                    ),
                );
            }
        }
    }

    // Write the transpiled code
    let code_with_sourcemap = if let Some(ref map) = output.source_map {
        match spec.sourcemaps {
            crate::compiler::SourceMapKind::Inline => {
                // Append inline source map as data URL
                let encoded = base64_encode(map.as_bytes());
                format!(
                    "{}\n//# sourceMappingURL=data:application/json;base64,{}",
                    output.code, encoded
                )
            }
            crate::compiler::SourceMapKind::External => {
                // Write external source map file
                let map_path = output_path.with_extension("js.map");
                if let Err(e) = fs::write(&map_path, map) {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return BuildNodeResult::failed(
                        &node.id,
                        hash,
                        duration_ms,
                        BuildErrorInfo::new(
                            codes::BUILD_TRANSPILE_WRITE_ERROR,
                            format!("Failed to write source map: {e}"),
                        ),
                    );
                }
                // Add reference to external source map
                let map_filename = map_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output.js.map");
                format!("{}\n//# sourceMappingURL={}", output.code, map_filename)
            }
            crate::compiler::SourceMapKind::None => output.code.clone(),
        }
    } else {
        output.code.clone()
    };

    if let Err(e) = fs::write(&output_path, &code_with_sourcemap) {
        let duration_ms = start.elapsed().as_millis() as u64;
        return BuildNodeResult::failed(
            &node.id,
            hash,
            duration_ms,
            BuildErrorInfo::new(
                codes::BUILD_TRANSPILE_WRITE_ERROR,
                format!("Failed to write output file: {e}"),
            ),
        );
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    // Compute output fingerprint
    // v3.5: Skip fingerprint on first build (lazy fingerprinting)
    let fingerprint = if has_outputs && rebuild_reason != BuildNodeReason::FirstBuild {
        compute_fingerprint(&node.outputs, cwd).ok().flatten()
    } else {
        None
    };

    let mut result =
        BuildNodeResult::cache_miss_with_reason(&node.id, hash, duration_ms, rebuild_reason);
    result.cache = if options.force {
        CacheStatus::Bypass
    } else {
        CacheStatus::Miss
    };

    // Update cache with fingerprint
    if let Some(cache) = cache {
        cache.set_with_fingerprint(&node.id, hash, true, fingerprint);
    }

    result
}

/// Resolve the tsc command to use for typecheck.
///
/// Prefers local `node_modules/.bin/tsc` if present, otherwise uses `npx --no-install tsc`.
/// This reduces variance from npx resolution and avoids surprise network calls.
fn resolve_tsc_command(cwd: &Path) -> String {
    let local_tsc = cwd.join("node_modules/.bin/tsc");
    if local_tsc.exists() {
        format!("{} --noEmit", local_tsc.to_string_lossy())
    } else {
        // Use npx --no-install to fail fast if tsc not installed
        "npx --no-install tsc --noEmit".to_string()
    }
}

/// Execute a typecheck node using `tsc --noEmit`.
///
/// Typecheck nodes are validation-only (no outputs). They run TypeScript's
/// type checker to verify type correctness without producing output files.
///
/// The command is resolved at execution time:
/// - Prefers local `node_modules/.bin/tsc` if present
/// - Falls back to `npx --no-install tsc` (fails fast if not installed)
///
/// Returns a `BuildNodeResult` with success/failure status.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn execute_typecheck(
    node: &BuildNode,
    cwd: &Path,
    hash: &str,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> BuildNodeResult {
    // Track reason for rebuild
    let mut rebuild_reason = BuildNodeReason::FirstBuild;

    // Check cache unless force
    if options.force {
        rebuild_reason = BuildNodeReason::Forced;
    } else if let Some(cache) = cache.as_ref() {
        // Typecheck has no outputs, so we only check input hash
        if let Some(entry) = cache.get_entry(&node.id, hash) {
            if entry.ok {
                return BuildNodeResult::cache_hit(&node.id, hash);
            }
        }
    }

    // Dry run - don't execute
    if options.dry_run {
        let mut result = BuildNodeResult::cache_miss_with_reason(&node.id, hash, 0, rebuild_reason);
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };
        result.notes.push("dry run - not executed".to_string());
        return result;
    }

    // Resolve the tsc command (prefer local, fallback to npx --no-install)
    let command_str = resolve_tsc_command(cwd);

    let start = Instant::now();
    let output = match run_script(&command_str, cwd) {
        Ok(out) => out,
        Err(e) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            return BuildNodeResult::failed(
                &node.id,
                hash,
                duration_ms,
                BuildErrorInfo::new(
                    codes::BUILD_TYPECHECK_FAILED,
                    format!("Failed to spawn: {e}"),
                ),
            );
        }
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    if output.exit_code != 0 {
        let error = BuildErrorInfo::new(
            codes::BUILD_TYPECHECK_FAILED,
            format!("Type errors found (exit code {})", output.exit_code),
        )
        .with_detail(if output.stdout.is_empty() && output.stderr.is_empty() {
            String::new()
        } else {
            // tsc outputs type errors to stdout, not stderr
            let combined = if !output.stdout.is_empty() {
                output.stdout.clone()
            } else {
                output.stderr.clone()
            };
            // Get last 30 lines
            combined
                .lines()
                .rev()
                .take(30)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        });

        let mut result = BuildNodeResult::failed(&node.id, hash, duration_ms, error);
        result.stdout_truncated = output.stdout_truncated;
        result.stderr_truncated = output.stderr_truncated;
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };

        // Update cache with failure
        if let Some(cache) = cache {
            cache.set(&node.id, hash, false);
        }

        return result;
    }

    // Success - no fingerprint needed since typecheck has no outputs
    let mut result =
        BuildNodeResult::cache_miss_with_reason(&node.id, hash, duration_ms, rebuild_reason);
    result.stdout_truncated = output.stdout_truncated;
    result.stderr_truncated = output.stderr_truncated;
    result.cache = if options.force {
        CacheStatus::Bypass
    } else {
        CacheStatus::Miss
    };

    // Update cache (no fingerprint)
    if let Some(cache) = cache {
        cache.set(&node.id, hash, true);
    }

    result
}

/// File extensions that can be transpiled.
const TRANSPILABLE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];

/// Execute a batch transpile node (transpile all files in a directory).
///
/// This function:
/// 1. Scans the input directory for matching files (*.ts, *.tsx, *.js, *.jsx)
/// 2. Transpiles each file using the provided compiler backend
/// 3. Writes output files to the output directory, preserving structure
///
/// v3.5: Uses lazy fingerprinting - skips fingerprint on first build.
///
/// Returns a `BuildNodeResult` with aggregate success/failure status.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn execute_transpile_batch(
    node: &BuildNode,
    cwd: &Path,
    hash: &str,
    spec: &TranspileSpec,
    backend: &dyn CompilerBackend,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> BuildNodeResult {
    let has_outputs = !node.outputs.is_empty();

    // Track reason for rebuild (v2.3)
    let mut rebuild_reason = BuildNodeReason::FirstBuild;

    // Check cache unless force
    if options.force {
        rebuild_reason = BuildNodeReason::Forced;
    } else if let Some(cache) = cache.as_ref() {
        if let Some(entry) = cache.get_entry(&node.id, hash) {
            if entry.ok {
                if has_outputs {
                    // v3.5: Lazy fingerprinting - if cached fingerprint is None,
                    // trust the cache entry (first build used lazy fingerprinting)
                    if entry.fingerprint.is_none() {
                        return BuildNodeResult::cache_hit(&node.id, hash);
                    }

                    let current_fingerprint = compute_fingerprint(&node.outputs, cwd).ok();
                    let fingerprint_matches = match (&current_fingerprint, &entry.fingerprint) {
                        (Some(Some(current)), Some(cached)) => current.hash == cached.hash,
                        (Some(None), None) => true,
                        _ => false,
                    };

                    if fingerprint_matches {
                        return BuildNodeResult::cache_hit(&node.id, hash);
                    }
                    rebuild_reason = BuildNodeReason::OutputsChanged;
                } else {
                    return BuildNodeResult::cache_hit(&node.id, hash);
                }
            }
        }
    }

    // Dry run - don't execute
    if options.dry_run {
        let mut result = BuildNodeResult::cache_miss_with_reason(&node.id, hash, 0, rebuild_reason);
        result.cache = if options.force {
            CacheStatus::Bypass
        } else {
            CacheStatus::Miss
        };
        result.notes.push("dry run - not executed".to_string());
        return result;
    }

    let start = Instant::now();

    // Resolve input and output directories
    let input_dir = cwd.join(&spec.input_path);
    let output_dir = cwd.join(&spec.output_path);

    if !input_dir.exists() || !input_dir.is_dir() {
        let duration_ms = start.elapsed().as_millis() as u64;
        return BuildNodeResult::failed(
            &node.id,
            hash,
            duration_ms,
            BuildErrorInfo::new(
                codes::BUILD_TRANSPILE_READ_ERROR,
                format!("Input directory does not exist: {}", input_dir.display()),
            ),
        );
    }

    // Create output directory
    if let Err(e) = fs::create_dir_all(&output_dir) {
        let duration_ms = start.elapsed().as_millis() as u64;
        return BuildNodeResult::failed(
            &node.id,
            hash,
            duration_ms,
            BuildErrorInfo::new(
                codes::BUILD_TRANSPILE_WRITE_ERROR,
                format!("Failed to create output directory: {e}"),
            ),
        );
    }

    // Collect all transpilable files (deterministic ordering)
    let mut files: Vec<_> = walkdir::WalkDir::new(&input_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| TRANSPILABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort for deterministic ordering
    files.sort();

    if files.is_empty() {
        let duration_ms = start.elapsed().as_millis() as u64;
        let mut result =
            BuildNodeResult::cache_miss_with_reason(&node.id, hash, duration_ms, rebuild_reason);
        result.files_count = Some(0);
        result.notes.push("no files to transpile".to_string());
        if let Some(cache) = cache {
            cache.set_with_fingerprint(&node.id, hash, true, None);
        }
        return result;
    }

    let mut transpiled_count = 0;
    let mut errors = Vec::new();

    for file_path in &files {
        // Compute relative path from input_dir
        let rel_path = file_path.strip_prefix(&input_dir).unwrap_or(file_path);

        // Compute output path: change extension to .js
        let output_rel = rel_path.with_extension("js");
        let output_path = output_dir.join(&output_rel);

        // Create parent directories for output
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                if let Err(e) = fs::create_dir_all(parent) {
                    errors.push(format!(
                        "{}: failed to create directory: {e}",
                        rel_path.display()
                    ));
                    continue;
                }
            }
        }

        // Read input file
        let source = match fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                errors.push(format!("{}: failed to read: {e}", rel_path.display()));
                continue;
            }
        };

        // Create a file-specific spec for transpilation
        let file_spec = TranspileSpec::new(
            file_path.to_string_lossy().to_string(),
            output_path.to_string_lossy().to_string(),
        )
        .with_jsx_runtime(spec.jsx_runtime)
        .with_module(spec.module)
        .with_sourcemaps(spec.sourcemaps)
        .with_target(spec.target)
        .with_minify(spec.minify);

        // Transpile
        let output = match backend.transpile(&file_spec, &source) {
            Ok(out) => out,
            Err(e) => {
                errors.push(format!("{}: {e}", rel_path.display()));
                continue;
            }
        };

        // Write output with source map handling
        let code_with_sourcemap = if let Some(ref map) = output.source_map {
            match spec.sourcemaps {
                crate::compiler::SourceMapKind::Inline => {
                    let encoded = base64_encode(map.as_bytes());
                    format!(
                        "{}\n//# sourceMappingURL=data:application/json;base64,{}",
                        output.code, encoded
                    )
                }
                crate::compiler::SourceMapKind::External => {
                    let map_path = output_path.with_extension("js.map");
                    if let Err(e) = fs::write(&map_path, map) {
                        errors.push(format!(
                            "{}: failed to write source map: {e}",
                            rel_path.display()
                        ));
                        continue;
                    }
                    let map_filename = map_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("output.js.map");
                    format!("{}\n//# sourceMappingURL={}", output.code, map_filename)
                }
                crate::compiler::SourceMapKind::None => output.code.clone(),
            }
        } else {
            output.code.clone()
        };

        if let Err(e) = fs::write(&output_path, &code_with_sourcemap) {
            errors.push(format!(
                "{}: failed to write output: {e}",
                rel_path.display()
            ));
            continue;
        }

        transpiled_count += 1;
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    // Build result
    if !errors.is_empty() {
        let error_summary = if errors.len() > 5 {
            format!(
                "{} (and {} more errors)",
                errors[..5].join("; "),
                errors.len() - 5
            )
        } else {
            errors.join("; ")
        };

        return BuildNodeResult::failed(
            &node.id,
            hash,
            duration_ms,
            BuildErrorInfo::new(codes::BUILD_TRANSPILE_FAILED, error_summary),
        );
    }

    // Compute output fingerprint
    // v3.5: Skip fingerprint on first build (lazy fingerprinting)
    let fingerprint = if has_outputs && rebuild_reason != BuildNodeReason::FirstBuild {
        compute_fingerprint(&node.outputs, cwd).ok().flatten()
    } else {
        None
    };

    let mut result =
        BuildNodeResult::cache_miss_with_reason(&node.id, hash, duration_ms, rebuild_reason);
    result.cache = if options.force {
        CacheStatus::Bypass
    } else {
        CacheStatus::Miss
    };
    // Set structured file count (v3.1.2)
    result.files_count = Some(transpiled_count as u32);

    // Update cache with fingerprint
    if let Some(cache) = cache {
        cache.set_with_fingerprint(&node.id, hash, true, fingerprint);
    }

    result
}

/// Simple base64 encoding for source maps.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let mut buffer = [0u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);

        let n = u32::from(buffer[0]) << 16 | u32::from(buffer[1]) << 8 | u32::from(buffer[2]);

        result.push(ALPHABET[(n >> 18 & 0x3F) as usize] as char);
        result.push(ALPHABET[(n >> 12 & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[(n >> 6 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Execute a build graph.
///
/// Executes nodes in topological order, skipping nodes whose dependencies failed.
/// For graphs containing Transpile nodes, use `execute_graph_with_backend` instead.
///
/// # Errors
/// Returns an error if hash computation fails.
#[allow(clippy::cast_possible_truncation)]
pub fn execute_graph(
    graph: &BuildGraph,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> super::hash::HashResult<BuildRunResult> {
    execute_graph_with_backend(graph, cache, options, None)
}

/// Execute a build graph with an optional compiler backend.
///
/// Executes nodes in topological order, skipping nodes whose dependencies failed.
/// Transpile nodes require a compiler backend to be provided.
///
/// # Errors
/// Returns an error if hash computation fails.
#[allow(clippy::cast_possible_truncation)]
pub fn execute_graph_with_backend(
    graph: &BuildGraph,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
    backend: Option<&dyn CompilerBackend>,
) -> super::hash::HashResult<BuildRunResult> {
    execute_graph_with_file_cache(graph, cache, options, backend, None)
}

/// Execute a build graph with optional compiler backend and file hash cache.
///
/// The file hash cache avoids re-reading unchanged files during hash computation,
/// significantly speeding up repeated builds.
///
/// ## Target Filtering
///
/// When `options.targets` is non-empty, only nodes whose IDs are in the targets
/// list will be executed. Other nodes are skipped (not even their hashes are
/// computed for execution purposes). This allows running a subset of the build
/// graph, e.g., transpile-only without typecheck.
///
/// # Errors
/// Returns an error if hash computation fails.
#[allow(clippy::cast_possible_truncation)]
pub fn execute_graph_with_file_cache(
    graph: &BuildGraph,
    mut cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
    backend: Option<&dyn CompilerBackend>,
    file_cache: Option<&dyn super::hash::FileHashCache>,
) -> super::hash::HashResult<BuildRunResult> {
    let cwd = Path::new(&graph.cwd);
    let mut result = BuildRunResult::new(&graph.cwd);

    // Compute hashes for all nodes (using file cache if provided)
    let hash_ctx = match file_cache {
        Some(fc) => super::hash::HashContext::with_cache(fc),
        None => super::hash::HashContext::empty(),
    };
    let hashes = super::hash::hash_graph_with_ctx(graph, &hash_ctx)?;

    // Get execution order
    let order = graph.toposort();

    // Build target set for filtering (if targets specified)
    let target_set: std::collections::HashSet<&str> =
        options.targets.iter().map(String::as_str).collect();
    let filter_by_targets = !target_set.is_empty();

    // Track which nodes succeeded
    let mut succeeded: HashMap<&str, bool> = HashMap::new();

    // Execute nodes in order
    // Note: For v2.0, we execute sequentially. Parallel execution can be added later.
    for node_id in order {
        // Skip nodes not in target set (when filtering is enabled)
        if filter_by_targets && !target_set.contains(node_id) {
            // Mark as succeeded (not a failure) but don't execute
            succeeded.insert(node_id, true);
            continue;
        }
        let Some(node) = graph.get_node(node_id) else {
            continue;
        };

        let hash = hashes.get(node_id).map_or("", String::as_str);

        // Check if all dependencies succeeded
        let deps_ok = node
            .deps
            .iter()
            .all(|dep| succeeded.get(dep.as_str()).copied().unwrap_or(false));

        if !deps_ok {
            // Skip this node - dependency failed
            let skipped = BuildNodeResult::skipped(node_id);
            succeeded.insert(node_id, false);
            result.add_result(skipped);
            continue;
        }

        // Execute the node based on its kind
        let node_result = match node.kind {
            BuildNodeKind::Transpile => {
                // Transpile nodes require a backend and spec
                if let (Some(backend), Some(spec)) = (backend, &node.transpile) {
                    // Use batch or single-file execution based on spec
                    if spec.is_batch() {
                        if let Some(ref mut c) = cache {
                            execute_transpile_batch(
                                node,
                                cwd,
                                hash,
                                spec,
                                backend,
                                Some(*c),
                                options,
                            )
                        } else {
                            execute_transpile_batch(node, cwd, hash, spec, backend, None, options)
                        }
                    } else if let Some(ref mut c) = cache {
                        execute_transpile(node, cwd, hash, spec, backend, Some(*c), options)
                    } else {
                        execute_transpile(node, cwd, hash, spec, backend, None, options)
                    }
                } else if node.transpile.is_none() {
                    BuildNodeResult::failed(
                        &node.id,
                        hash,
                        0,
                        BuildErrorInfo::new(
                            codes::BUILD_TRANSPILE_FAILED,
                            "Transpile node missing transpile specification",
                        ),
                    )
                } else {
                    BuildNodeResult::failed(
                        &node.id,
                        hash,
                        0,
                        BuildErrorInfo::new(
                            codes::BUILD_NO_COMPILER_BACKEND,
                            "No compiler backend available for transpilation",
                        ),
                    )
                }
            }
            BuildNodeKind::Typecheck => {
                // Typecheck nodes run tsc --noEmit
                if let Some(ref mut c) = cache {
                    execute_typecheck(node, cwd, hash, Some(*c), options)
                } else {
                    execute_typecheck(node, cwd, hash, None, options)
                }
            }
            // Script and other node types use the regular execute_node
            _ => {
                if let Some(ref mut c) = cache {
                    execute_node(node, cwd, hash, Some(*c), options)
                } else {
                    execute_node(node, cwd, hash, None, options)
                }
            }
        };

        succeeded.insert(node_id, node_result.ok);
        result.add_result(node_result);
    }

    result.finalize(graph.nodes.len() as u32);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_memory_cache_basic() {
        let mut cache = MemoryCache::new();

        // Initial - no entry
        assert!(cache.get("node1", "hash1").is_none());

        // Set entry
        cache.set("node1", "hash1", true);
        assert_eq!(cache.get("node1", "hash1"), Some(true));

        // Different hash - no match
        assert!(cache.get("node1", "hash2").is_none());

        // Invalidate
        cache.invalidate("node1");
        assert!(cache.get("node1", "hash1").is_none());
    }

    #[test]
    fn test_run_script_success() {
        let dir = tempdir().unwrap();
        let cmd = "echo hello";

        let output = run_script(cmd, dir.path()).unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn test_run_script_failure() {
        let dir = tempdir().unwrap();
        let cmd = "exit 1";

        let output = run_script(cmd, dir.path()).unwrap();
        assert_ne!(output.exit_code, 0);
    }

    #[test]
    fn test_execute_node_cache_hit() {
        let dir = tempdir().unwrap();
        let mut cache = MemoryCache::new();

        let node = BuildNode::script("test", "echo test");
        let hash = "abc123";

        // Pre-populate cache
        cache.set(&node.id, hash, true);

        let options = ExecOptions::new();
        let result = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

        assert!(result.ok);
        assert_eq!(result.cache, CacheStatus::Hit);
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_execute_node_force_bypasses_cache() {
        let dir = tempdir().unwrap();
        let mut cache = MemoryCache::new();

        let node = BuildNode::script("test", "echo test");
        let hash = "abc123";

        // Pre-populate cache
        cache.set(&node.id, hash, true);

        let options = ExecOptions::new().with_force(true);
        let result = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

        assert!(result.ok);
        assert_eq!(result.cache, CacheStatus::Bypass);
    }

    #[test]
    fn test_execute_node_dry_run() {
        let dir = tempdir().unwrap();

        let node = BuildNode::script("test", "echo test");
        let hash = "abc123";

        let options = ExecOptions::new().with_dry_run(true);
        let result = execute_node(&node, dir.path(), hash, None, &options);

        assert!(result.ok);
        assert!(result.notes.iter().any(|n| n.contains("dry run")));
    }

    #[test]
    fn test_execute_graph_simple() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        let cmd = "echo built";
        let node = BuildNode::script("build", cmd);
        graph.add_node(node);
        graph.add_default("script:build");
        graph.normalize();

        let options = ExecOptions::new();
        let result = execute_graph(&graph, None, &options).unwrap();

        assert!(result.ok);
        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].ok);
    }

    #[test]
    fn test_execute_graph_dependency_failure_skips() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        // First node fails
        let fail_cmd = "exit 1";
        let node1 = BuildNode::script("first", fail_cmd);
        graph.add_node(node1);

        // Second node depends on first
        let mut node2 = BuildNode::script("second", "echo second");
        node2.deps = vec!["script:first".to_string()];
        graph.add_node(node2);

        graph.add_default("script:second");
        graph.normalize();

        let options = ExecOptions::new();
        let result = execute_graph(&graph, None, &options).unwrap();

        assert!(!result.ok);

        // Second node should be skipped
        let second_result = result
            .results
            .iter()
            .find(|r| r.id == "script:second")
            .unwrap();
        assert!(!second_result.ok);
        assert_eq!(second_result.cache, CacheStatus::Skipped);
    }

    // ============================================================
    // v2.2 Output Fingerprinting Tests
    // ============================================================

    use super::super::graph::BuildOutput;

    #[test]
    fn test_cache_entry_with_fingerprint() {
        let mut cache = MemoryCache::new();

        // Create entry with fingerprint
        let fingerprint = OutputFingerprint {
            schema_version: 1,
            hash: "abc123".to_string(),
            output_count: 2,
            total_size: 1024,
        };

        cache.set_with_fingerprint("node1", "hash1", true, Some(fingerprint.clone()));

        // Get entry should return full entry
        let entry = cache.get_entry("node1", "hash1").unwrap();
        assert!(entry.ok);
        assert_eq!(entry.hash, "hash1");
        assert!(entry.fingerprint.is_some());
        assert_eq!(entry.fingerprint.as_ref().unwrap().hash, "abc123");
    }

    #[test]
    fn test_fingerprint_cache_hit_when_outputs_unchanged() {
        let dir = tempdir().unwrap();
        let output_file = dir.path().join("output.txt");

        // Create a node with outputs
        let mut node = BuildNode::script("build", "echo built > output.txt");
        node.outputs.push(BuildOutput::file("output.txt"));

        // First execution - creates the output file
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        let result1 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result1.ok);
        assert_eq!(result1.cache, CacheStatus::Miss);

        // Verify output file exists
        assert!(output_file.exists());

        // Second execution - should be cache hit (outputs unchanged)
        let result2 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result2.ok);
        assert_eq!(result2.cache, CacheStatus::Hit);
    }

    #[test]
    fn test_fingerprint_cache_miss_when_outputs_modified() {
        let dir = tempdir().unwrap();
        let output_file = dir.path().join("output.txt");

        // Create a node with outputs
        let mut node = BuildNode::script("build", "echo built > output.txt");
        node.outputs.push(BuildOutput::file("output.txt"));

        // First execution - creates the output file
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        let result1 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result1.ok);
        assert_eq!(result1.cache, CacheStatus::Miss);

        // Modify the output file (simulate external modification)
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&output_file, "modified content").unwrap();

        // Second execution - should be cache miss (fingerprint changed)
        let result2 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result2.ok);
        assert_eq!(result2.cache, CacheStatus::Miss);
    }

    #[test]
    fn test_fingerprint_cache_miss_when_outputs_deleted() {
        let dir = tempdir().unwrap();
        let output_file = dir.path().join("output.txt");

        // Create a node with outputs
        let mut node = BuildNode::script("build", "echo built > output.txt");
        node.outputs.push(BuildOutput::file("output.txt"));

        // First execution - creates the output file
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        let result1 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result1.ok);
        assert_eq!(result1.cache, CacheStatus::Miss);

        // Delete the output file
        std::fs::remove_file(&output_file).unwrap();

        // Second execution - should be cache miss (output deleted)
        let result2 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result2.ok);
        assert_eq!(result2.cache, CacheStatus::Miss);
    }

    #[test]
    fn test_legacy_cache_hit_without_outputs() {
        let dir = tempdir().unwrap();

        // Create a node WITHOUT outputs (legacy behavior)
        let node = BuildNode::script("build", "echo built");

        // First execution
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        let result1 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result1.ok);
        assert_eq!(result1.cache, CacheStatus::Miss);

        // Second execution - should be cache hit (no fingerprint check needed)
        let result2 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result2.ok);
        assert_eq!(result2.cache, CacheStatus::Hit);
    }

    #[test]
    fn test_fingerprint_stored_after_successful_build() {
        let dir = tempdir().unwrap();

        // Create a node with outputs
        let mut node = BuildNode::script("build", "echo built > output.txt");
        node.outputs.push(BuildOutput::file("output.txt"));

        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        let _ = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

        // Verify fingerprint was stored
        let entry = cache.get_raw("script:build").unwrap();
        assert!(entry.fingerprint.is_some());
        let fp = entry.fingerprint.as_ref().unwrap();
        assert_eq!(fp.schema_version, 1);
        assert_eq!(fp.output_count, 1);
    }

    // ============================================================
    // v2.3 Build Reason Tests
    // ============================================================

    use super::super::graph::BuildNodeReason;

    #[test]
    fn test_reason_first_build_on_cache_cold() {
        let dir = tempdir().unwrap();

        let node = BuildNode::script("build", "echo built");
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        // First execution - cache is cold
        let result = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result.ok);
        assert_eq!(result.reason, Some(BuildNodeReason::FirstBuild));
    }

    #[test]
    fn test_reason_forced_on_force_flag() {
        let dir = tempdir().unwrap();

        let node = BuildNode::script("build", "echo built");
        let hash = "abc123";
        let mut cache = MemoryCache::new();

        // Pre-populate cache
        cache.set(&node.id, hash, true);

        // Execute with --force
        let options = ExecOptions::new().with_force(true);
        let result = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

        assert!(result.ok);
        assert_eq!(result.reason, Some(BuildNodeReason::Forced));
        assert_eq!(result.cache, CacheStatus::Bypass);
    }

    #[test]
    fn test_reason_outputs_changed_on_fingerprint_mismatch() {
        let dir = tempdir().unwrap();
        let output_file = dir.path().join("output.txt");

        // Create a node with outputs
        let mut node = BuildNode::script("build", "echo built > output.txt");
        node.outputs.push(BuildOutput::file("output.txt"));

        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        // First execution
        let result1 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result1.ok);
        assert_eq!(result1.reason, Some(BuildNodeReason::FirstBuild));

        // Modify the output file
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&output_file, "modified content").unwrap();

        // Second execution - should show OutputsChanged reason
        let result2 = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result2.ok);
        assert_eq!(result2.reason, Some(BuildNodeReason::OutputsChanged));
    }

    #[test]
    fn test_reason_cache_hit() {
        let dir = tempdir().unwrap();

        let node = BuildNode::script("build", "echo built");
        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        // First execution
        let _ = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

        // Second execution - cache hit
        let result = execute_node(&node, dir.path(), hash, Some(&mut cache), &options);
        assert!(result.ok);
        assert_eq!(result.cache, CacheStatus::Hit);
        assert_eq!(result.reason, Some(BuildNodeReason::CacheHit));
    }

    #[test]
    fn test_reason_human_readable() {
        assert_eq!(
            BuildNodeReason::FirstBuild.to_human_string(),
            "first build (cache cold)"
        );
        assert_eq!(
            BuildNodeReason::Forced.to_human_string(),
            "forced rebuild (--force)"
        );
        assert_eq!(
            BuildNodeReason::OutputsChanged.to_human_string(),
            "outputs changed (fingerprint mismatch)"
        );
        assert_eq!(BuildNodeReason::CacheHit.to_human_string(), "cache hit");
    }

    // ============================================================
    // v3.1 Transpile Execution Tests
    // ============================================================

    use crate::compiler::{JsxRuntime, SwcBackend, TranspileSpec as CompilerTranspileSpec};

    #[test]
    fn test_execute_transpile_simple() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("app.js");
        std::fs::write(&input_file, "const x = 1;").unwrap();

        let spec = CompilerTranspileSpec::new("app.js", "dist/app.js");
        let node = BuildNode::transpile("app.js", "dist/app.js", spec.clone());
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result = execute_transpile(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(result.ok, "Transpile should succeed");
        assert_eq!(result.cache, CacheStatus::Miss);

        // Verify output file was created
        let output_file = dir.path().join("dist/app.js");
        assert!(output_file.exists(), "Output file should exist");

        let output_content = std::fs::read_to_string(&output_file).unwrap();
        assert!(output_content.contains("const x = 1;"));
    }

    #[test]
    fn test_execute_transpile_jsx() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("App.jsx");
        std::fs::write(&input_file, "const el = 1;").unwrap();

        let spec = CompilerTranspileSpec::new("App.jsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);
        let node = BuildNode::transpile("App.jsx", "dist/App.js", spec.clone());
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result = execute_transpile(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(result.ok, "Transpile should succeed");

        let output_file = dir.path().join("dist/App.js");
        assert!(output_file.exists());
    }

    #[test]
    fn test_execute_transpile_with_cache() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("app.js");
        std::fs::write(&input_file, "const x = 1;").unwrap();

        let spec = CompilerTranspileSpec::new("app.js", "dist/app.js");
        let node = BuildNode::transpile("app.js", "dist/app.js", spec.clone());
        let backend = SwcBackend::new();

        let hash = "abc123";
        let mut cache = MemoryCache::new();
        let options = ExecOptions::new();

        // First execution - cache miss
        let result1 = execute_transpile(
            &node,
            dir.path(),
            hash,
            &spec,
            &backend,
            Some(&mut cache),
            &options,
        );

        assert!(result1.ok);
        assert_eq!(result1.cache, CacheStatus::Miss);

        // Second execution - cache hit
        let result2 = execute_transpile(
            &node,
            dir.path(),
            hash,
            &spec,
            &backend,
            Some(&mut cache),
            &options,
        );

        assert!(result2.ok);
        assert_eq!(result2.cache, CacheStatus::Hit);
    }

    #[test]
    fn test_execute_transpile_missing_input() {
        let dir = tempdir().unwrap();
        // Don't create the input file

        let spec = CompilerTranspileSpec::new("missing.js", "dist/missing.js");
        let node = BuildNode::transpile("missing.js", "dist/missing.js", spec.clone());
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result = execute_transpile(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(!result.ok, "Should fail when input file is missing");
        assert!(result.error.is_some());
        assert_eq!(
            result.error.as_ref().unwrap().code,
            codes::BUILD_TRANSPILE_READ_ERROR
        );
    }

    #[test]
    fn test_execute_transpile_dry_run() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("app.js");
        std::fs::write(&input_file, "const x = 1;").unwrap();

        let spec = CompilerTranspileSpec::new("app.js", "dist/app.js");
        let node = BuildNode::transpile("app.js", "dist/app.js", spec.clone());
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new().with_dry_run(true);

        let result = execute_transpile(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(result.ok);
        assert!(result.notes.iter().any(|n| n.contains("dry run")));

        // Output file should NOT exist
        let output_file = dir.path().join("dist/app.js");
        assert!(!output_file.exists());
    }

    #[test]
    fn test_execute_graph_with_transpile_node() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("app.js");
        std::fs::write(&input_file, "const x = 1;").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        let spec = CompilerTranspileSpec::new("app.js", "dist/app.js");
        let node = BuildNode::transpile("app.js", "dist/app.js", spec);
        graph.add_node(node);
        graph.normalize();

        let backend = SwcBackend::new();
        let options = ExecOptions::new();

        let result = execute_graph_with_backend(&graph, None, &options, Some(&backend)).unwrap();

        assert!(result.ok);
        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].ok);

        let output_file = dir.path().join("dist/app.js");
        assert!(output_file.exists());
    }

    #[test]
    fn test_execute_graph_transpile_no_backend_fails() {
        let dir = tempdir().unwrap();
        let input_file = dir.path().join("app.js");
        std::fs::write(&input_file, "const x = 1;").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        let spec = CompilerTranspileSpec::new("app.js", "dist/app.js");
        let node = BuildNode::transpile("app.js", "dist/app.js", spec);
        graph.add_node(node);
        graph.normalize();

        let options = ExecOptions::new();

        // No backend provided
        let result = execute_graph_with_backend(&graph, None, &options, None).unwrap();

        assert!(!result.ok);
        assert!(result.results[0].error.is_some());
        assert_eq!(
            result.results[0].error.as_ref().unwrap().code,
            codes::BUILD_NO_COMPILER_BACKEND
        );
    }

    // ============================================================
    // v3.1.1 Batch Transpile Execution Tests
    // ============================================================

    #[test]
    fn test_execute_transpile_batch_simple() {
        let dir = tempdir().unwrap();

        // Create src/ directory with multiple files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();
        std::fs::write(dir.path().join("src/utils.ts"), "export const y = 2;").unwrap();

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result =
            execute_transpile_batch(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(
            result.ok,
            "Batch transpile should succeed: {:?}",
            result.error
        );

        // Verify output files were created
        let index_output = dir.path().join("dist/index.js");
        let utils_output = dir.path().join("dist/utils.js");
        assert!(index_output.exists(), "dist/index.js should exist");
        assert!(utils_output.exists(), "dist/utils.js should exist");

        // Check structured file count (v3.1.2)
        assert_eq!(result.files_count, Some(2), "files_count should be 2");
    }

    #[test]
    fn test_execute_transpile_batch_nested_directories() {
        let dir = tempdir().unwrap();

        // Create nested directory structure
        std::fs::create_dir_all(dir.path().join("src/components")).unwrap();
        std::fs::create_dir_all(dir.path().join("src/utils")).unwrap();
        std::fs::write(
            dir.path().join("src/index.ts"),
            "export * from './components';",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/components/Button.tsx"),
            "export const Button = () => <button/>;",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/utils/helpers.ts"),
            "export const helper = () => {};",
        )
        .unwrap();

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result =
            execute_transpile_batch(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(result.ok, "Batch transpile should succeed");

        // Verify nested directory structure is preserved
        assert!(dir.path().join("dist/index.js").exists());
        assert!(dir.path().join("dist/components/Button.js").exists());
        assert!(dir.path().join("dist/utils/helpers.js").exists());
    }

    #[test]
    fn test_execute_transpile_batch_empty_src() {
        let dir = tempdir().unwrap();

        // Create empty src/ directory
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result =
            execute_transpile_batch(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(result.ok, "Batch transpile with empty src should succeed");
        assert!(result
            .notes
            .iter()
            .any(|n| n.contains("no files to transpile")));
    }

    #[test]
    fn test_execute_transpile_batch_missing_src() {
        let dir = tempdir().unwrap();

        // Don't create src/ directory
        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();

        let hash = "abc123";
        let options = ExecOptions::new();

        let result =
            execute_transpile_batch(&node, dir.path(), hash, &spec, &backend, None, &options);

        assert!(!result.ok, "Batch transpile should fail with missing src");
        assert!(result.error.is_some());
        assert_eq!(
            result.error.as_ref().unwrap().code,
            codes::BUILD_TRANSPILE_READ_ERROR
        );
    }

    #[test]
    fn test_execute_graph_with_batch_transpile_node() {
        let dir = tempdir().unwrap();

        // Create src/ directory with files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        graph.add_node(node);
        graph.normalize();

        let backend = SwcBackend::new();
        let options = ExecOptions::new();

        let result = execute_graph_with_backend(&graph, None, &options, Some(&backend)).unwrap();

        assert!(result.ok);
        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].ok);

        let output_file = dir.path().join("dist/index.js");
        assert!(output_file.exists(), "Output file should be created");
    }

    // ============================================================
    // Sanity Locks for Benchmarking
    // ============================================================

    #[test]
    fn test_batch_transpile_deterministic_ordering() {
        // Ensure batch transpile processes files in stable, deterministic order
        let dir = tempdir().unwrap();

        // Create files with names that would sort differently by different criteria
        std::fs::create_dir_all(dir.path().join("src/z")).unwrap();
        std::fs::create_dir_all(dir.path().join("src/a")).unwrap();
        std::fs::write(dir.path().join("src/z/last.ts"), "export const z = 1;").unwrap();
        std::fs::write(dir.path().join("src/a/first.ts"), "export const a = 2;").unwrap();
        std::fs::write(dir.path().join("src/middle.ts"), "export const m = 3;").unwrap();
        std::fs::write(dir.path().join("src/AAA.ts"), "export const aaa = 4;").unwrap();

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();
        let options = ExecOptions::new();

        // Run twice and ensure same output
        let result1 =
            execute_transpile_batch(&node, dir.path(), "hash1", &spec, &backend, None, &options);
        let result2 =
            execute_transpile_batch(&node, dir.path(), "hash2", &spec, &backend, None, &options);

        assert!(result1.ok);
        assert!(result2.ok);

        // Both should report same file count and have identical notes
        assert_eq!(
            result1.notes, result2.notes,
            "Batch transpile should be deterministic"
        );

        // All output files should exist
        assert!(dir.path().join("dist/a/first.js").exists());
        assert!(dir.path().join("dist/z/last.js").exists());
        assert!(dir.path().join("dist/middle.js").exists());
        assert!(dir.path().join("dist/AAA.js").exists());
    }

    #[test]
    fn test_sourcemap_paths_are_relative() {
        // Ensure sourcemaps don't contain absolute paths
        let dir = tempdir().unwrap();

        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/app.ts"), "export const x = 1;").unwrap();

        let spec = CompilerTranspileSpec::batch("src", "dist");
        let node = BuildNode::transpile_batch(&spec);
        let backend = SwcBackend::new();
        let options = ExecOptions::new();

        let result =
            execute_transpile_batch(&node, dir.path(), "hash", &spec, &backend, None, &options);
        assert!(result.ok);

        // Read the generated source map
        let map_path = dir.path().join("dist/app.js.map");
        assert!(map_path.exists(), "Source map should be created");

        let map_content = std::fs::read_to_string(&map_path).unwrap();

        // Parse and check sources field
        let map: serde_json::Value = serde_json::from_str(&map_content).unwrap();
        let sources = map.get("sources").and_then(|s| s.as_array());

        if let Some(sources) = sources {
            for source in sources {
                if let Some(path) = source.as_str() {
                    // Source paths should not be absolute (no leading / on Unix or C:\ on Windows)
                    assert!(
                        !path.starts_with('/') && !path.contains(":\\"),
                        "Source map should not contain absolute paths: {path}"
                    );
                }
            }
        }
    }
}
