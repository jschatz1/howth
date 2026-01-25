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

use super::codes;
use super::fingerprint::{compute_fingerprint, OutputFingerprint};
use super::graph::{
    BuildErrorInfo, BuildGraph, BuildNode, BuildNodeReason, BuildNodeResult, BuildRunResult,
    CacheStatus, MAX_OUTPUT_SIZE,
};
use std::collections::HashMap;
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
}

/// Get number of CPUs (clamped to 1..=64).
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
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
    /// Map of node_id -> CacheEntry
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
        for line in reader.lines() {
            if let Ok(line) = line {
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
    }

    // Read stderr
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
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
/// ## Build Reasons (v2.3)
///
/// The function tracks why a node was rebuilt:
/// - `Forced`: --force flag was used
/// - `FirstBuild`: No cache entry existed
/// - `OutputsChanged`: Fingerprint mismatch (outputs modified externally)
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
            BuildErrorInfo::new(codes::BUILD_SCRIPT_NOT_FOUND, "No script specified for node"),
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
        .with_detail(if !output.stderr.is_empty() {
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
        } else {
            String::new()
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
        compute_fingerprint(&node.outputs, cwd)
            .ok()
            .flatten()
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

/// Execute a build graph.
///
/// Executes nodes in topological order, skipping nodes whose dependencies failed.
pub fn execute_graph(
    graph: &BuildGraph,
    mut cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> super::hash::HashResult<BuildRunResult> {
    let cwd = Path::new(&graph.cwd);
    let mut result = BuildRunResult::new(&graph.cwd);

    // Compute hashes for all nodes
    let hashes = super::hash::hash_graph(graph)?;

    // Get execution order
    let order = graph.toposort();

    // Track which nodes succeeded
    let mut succeeded: HashMap<&str, bool> = HashMap::new();

    // Execute nodes in order
    // Note: For v2.0, we execute sequentially. Parallel execution can be added later.
    for node_id in order {
        let Some(node) = graph.get_node(node_id) else {
            continue;
        };

        let hash = hashes.get(node_id).map(String::as_str).unwrap_or("");

        // Check if all dependencies succeeded
        let deps_ok = node.deps.iter().all(|dep| {
            succeeded.get(dep.as_str()).copied().unwrap_or(false)
        });

        if !deps_ok {
            // Skip this node - dependency failed
            let skipped = BuildNodeResult::skipped(node_id);
            succeeded.insert(node_id, false);
            result.add_result(skipped);
            continue;
        }

        // Execute the node
        // Note: We need to handle the mutable borrow carefully
        let node_result = if let Some(ref mut c) = cache {
            execute_node(node, cwd, hash, Some(*c), options)
        } else {
            execute_node(node, cwd, hash, None, options)
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

        let cmd = if cfg!(windows) {
            "echo hello"
        } else {
            "echo hello"
        };

        let output = run_script(cmd, dir.path()).unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn test_run_script_failure() {
        let dir = tempdir().unwrap();

        let cmd = if cfg!(windows) {
            "exit 1"
        } else {
            "exit 1"
        };

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

        let cmd = if cfg!(windows) {
            "echo built"
        } else {
            "echo built"
        };
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
        let fail_cmd = if cfg!(windows) { "exit 1" } else { "exit 1" };
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
        let second_result = result.results.iter().find(|r| r.id == "script:second").unwrap();
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

        execute_node(&node, dir.path(), hash, Some(&mut cache), &options);

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
}
