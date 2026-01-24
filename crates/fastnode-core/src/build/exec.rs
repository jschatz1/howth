//! Build execution engine.
//!
//! Executes build nodes, respecting dependencies and caching.

use super::codes;
use super::graph::{
    BuildErrorInfo, BuildGraph, BuildNode, BuildNodeResult, BuildRunResult, CacheStatus,
    MAX_OUTPUT_SIZE,
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

/// Cache interface for build results.
pub trait BuildCache {
    /// Check if a node hash is cached and was successful.
    fn get(&self, node_id: &str, hash: &str) -> Option<bool>;

    /// Store a result for a node.
    fn set(&mut self, node_id: &str, hash: &str, ok: bool);

    /// Invalidate cache for a node.
    fn invalidate(&mut self, node_id: &str);

    /// Clear all cache entries.
    fn clear(&mut self);
}

/// In-memory build cache.
#[derive(Debug, Default)]
pub struct MemoryCache {
    /// Map of node_id -> (hash, ok)
    entries: HashMap<String, (String, bool)>,
}

impl MemoryCache {
    /// Create a new memory cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl BuildCache for MemoryCache {
    fn get(&self, node_id: &str, hash: &str) -> Option<bool> {
        self.entries
            .get(node_id)
            .filter(|(cached_hash, _)| cached_hash == hash)
            .map(|(_, ok)| *ok)
    }

    fn set(&mut self, node_id: &str, hash: &str, ok: bool) {
        self.entries.insert(node_id.to_string(), (hash.to_string(), ok));
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
pub fn execute_node(
    node: &BuildNode,
    cwd: &Path,
    hash: &str,
    cache: Option<&mut dyn BuildCache>,
    options: &ExecOptions,
) -> BuildNodeResult {
    // Check cache unless force
    if !options.force {
        if let Some(cache) = cache.as_ref() {
            if let Some(ok) = cache.get(&node.id, hash) {
                if ok {
                    return BuildNodeResult::cache_hit(&node.id, hash);
                }
            }
        }
    }

    // Dry run - don't execute
    if options.dry_run {
        let mut result = BuildNodeResult::cache_miss(&node.id, hash, 0);
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

        // Update cache with failure
        if let Some(cache) = cache {
            cache.set(&node.id, hash, false);
        }

        return result;
    }

    // Success
    let mut result = BuildNodeResult::cache_miss(&node.id, hash, duration_ms);
    result.stdout_truncated = output.stdout_truncated;
    result.stderr_truncated = output.stderr_truncated;
    result.cache = if options.force {
        CacheStatus::Bypass
    } else {
        CacheStatus::Miss
    };

    // Update cache
    if let Some(cache) = cache {
        cache.set(&node.id, hash, true);
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
        graph.add_entrypoint("script:build");
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

        graph.add_entrypoint("script:second");
        graph.normalize();

        let options = ExecOptions::new();
        let result = execute_graph(&graph, None, &options).unwrap();

        assert!(!result.ok);

        // Second node should be skipped
        let second_result = result.results.iter().find(|r| r.id == "script:second").unwrap();
        assert!(!second_result.ok);
        assert_eq!(second_result.cache, CacheStatus::Skipped);
    }
}
