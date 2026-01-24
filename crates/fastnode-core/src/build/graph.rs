//! BuildGraph types for the incremental build system.
//!
//! The BuildGraph represents the dependency graph of build nodes.
//! Each node has inputs (files, globs, env vars) and can depend on other nodes.
//!
//! ## Schema Version
//!
//! - Schema version 1 (v2.0): Initial build graph format

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema version for the BuildGraph format.
pub const BUILD_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Schema version for the BuildRunResult format.
pub const BUILD_RUN_SCHEMA_VERSION: u32 = 1;

/// Default environment variables included in hash.
/// Locked for v2.0.
pub const DEFAULT_ENV_ALLOWLIST: &[&str] = &["NODE_ENV", "CI"];

/// Default glob exclusions for source files.
/// Locked for v2.0.
pub const DEFAULT_GLOB_EXCLUSIONS: &[&str] = &[
    "node_modules/**",
    ".git/**",
    "dist/**",
    "build/**",
    ".howth/**",
];

/// Maximum stdout/stderr capture size per stream (256KB).
pub const MAX_OUTPUT_SIZE: usize = 256 * 1024;

/// Kind of build node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildNodeKind {
    /// npm script execution.
    Script,
    /// TypeScript compilation (reserved).
    Ts,
    /// Bundling (reserved).
    Bundle,
    /// Test execution (reserved).
    Test,
}

impl BuildNodeKind {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Ts => "ts",
            Self::Bundle => "bundle",
            Self::Test => "test",
        }
    }
}

impl std::fmt::Display for BuildNodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Script specification for a build node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildScriptSpec {
    /// Script name (e.g., "build").
    pub name: String,
    /// Resolved command line.
    pub command: String,
    /// Whether to run via shell (always true for npm scripts).
    #[serde(default = "default_shell")]
    pub shell: bool,
}

fn default_shell() -> bool {
    true
}

impl BuildScriptSpec {
    /// Create a new script spec.
    #[must_use]
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            shell: true,
        }
    }
}

/// A build input source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BuildInput {
    /// A single file.
    File {
        /// Absolute path to the file.
        path: String,
    },
    /// A glob pattern.
    Glob {
        /// Glob pattern (normalized).
        pattern: String,
        /// Root directory (absolute).
        root: String,
    },
    /// A package dependency.
    Package {
        /// Package name.
        name: String,
        /// Version from lockfile (if available).
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
    /// The lockfile.
    Lockfile {
        /// Absolute path to lockfile.
        path: String,
        /// Lockfile schema version.
        schema_version: u32,
    },
    /// An environment variable.
    Env {
        /// Environment variable key.
        key: String,
    },
}

impl BuildInput {
    /// Create a file input.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File { path: path.into() }
    }

    /// Create a glob input.
    #[must_use]
    pub fn glob(pattern: impl Into<String>, root: impl Into<String>) -> Self {
        Self::Glob {
            pattern: pattern.into(),
            root: root.into(),
        }
    }

    /// Create a package input.
    #[must_use]
    pub fn package(name: impl Into<String>, version: Option<String>) -> Self {
        Self::Package {
            name: name.into(),
            version,
        }
    }

    /// Create a lockfile input.
    #[must_use]
    pub fn lockfile(path: impl Into<String>, schema_version: u32) -> Self {
        Self::Lockfile {
            path: path.into(),
            schema_version,
        }
    }

    /// Create an environment variable input.
    #[must_use]
    pub fn env(key: impl Into<String>) -> Self {
        Self::Env { key: key.into() }
    }

    /// Get the type string for this input.
    #[must_use]
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Glob { .. } => "glob",
            Self::Package { .. } => "package",
            Self::Lockfile { .. } => "lockfile",
            Self::Env { .. } => "env",
        }
    }
}

/// A node in the build graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildNode {
    /// Stable identifier (e.g., "script:build").
    pub id: String,
    /// Node kind.
    pub kind: BuildNodeKind,
    /// Short human-readable label.
    pub label: String,
    /// Input sources (deterministically ordered).
    pub inputs: Vec<BuildInput>,
    /// Output paths (absolute, deterministically ordered).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// Environment variables to include in hash (sorted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allowlist: Vec<String>,
    /// Script specification (for script nodes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<BuildScriptSpec>,
    /// Node IDs this depends on (sorted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
}

impl BuildNode {
    /// Create a new script node.
    #[must_use]
    pub fn script(name: &str, command: &str) -> Self {
        Self {
            id: format!("script:{name}"),
            kind: BuildNodeKind::Script,
            label: format!("script:{name}"),
            inputs: Vec::new(),
            outputs: Vec::new(),
            env_allowlist: DEFAULT_ENV_ALLOWLIST.iter().map(|s| (*s).to_string()).collect(),
            script: Some(BuildScriptSpec::new(name, command)),
            deps: Vec::new(),
        }
    }

    /// Add an input to this node.
    pub fn add_input(&mut self, input: BuildInput) {
        self.inputs.push(input);
    }

    /// Add an output path to this node.
    pub fn add_output(&mut self, path: impl Into<String>) {
        self.outputs.push(path.into());
    }

    /// Sort inputs and deps for deterministic ordering.
    pub fn normalize(&mut self) {
        // Sort inputs by type then by content
        self.inputs.sort_by(|a, b| {
            let type_cmp = a.type_str().cmp(b.type_str());
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            // Sort by serialized form for stability
            let a_json = serde_json::to_string(a).unwrap_or_default();
            let b_json = serde_json::to_string(b).unwrap_or_default();
            a_json.cmp(&b_json)
        });

        self.outputs.sort();
        self.env_allowlist.sort();
        self.deps.sort();
    }
}

/// The complete build graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildGraph {
    /// Schema version.
    pub schema_version: u32,
    /// Absolute working directory.
    pub cwd: String,
    /// Build nodes (sorted by id).
    pub nodes: Vec<BuildNode>,
    /// Entrypoint node IDs (sorted).
    pub entrypoints: Vec<String>,
    /// Notes (always present, may be empty).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl BuildGraph {
    /// Create a new build graph.
    #[must_use]
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            schema_version: BUILD_GRAPH_SCHEMA_VERSION,
            cwd: cwd.into(),
            nodes: Vec::new(),
            entrypoints: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, mut node: BuildNode) {
        node.normalize();
        self.nodes.push(node);
    }

    /// Add an entrypoint.
    pub fn add_entrypoint(&mut self, id: impl Into<String>) {
        self.entrypoints.push(id.into());
    }

    /// Sort nodes and entrypoints for deterministic ordering.
    pub fn normalize(&mut self) {
        self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        self.entrypoints.sort();
    }

    /// Get a node by ID.
    #[must_use]
    pub fn get_node(&self, id: &str) -> Option<&BuildNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Get topologically sorted node IDs.
    /// Returns nodes in execution order (dependencies first).
    #[must_use]
    pub fn toposort(&self) -> Vec<&str> {
        // Build adjacency map
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

        for node in &self.nodes {
            in_degree.entry(node.id.as_str()).or_insert(0);
            for dep in &node.deps {
                *in_degree.entry(node.id.as_str()).or_insert(0) += 1;
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(node.id.as_str());
            }
        }

        // Kahn's algorithm with deterministic tie-breaking
        let mut result = Vec::new();
        let mut ready: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();
        ready.sort(); // Deterministic ordering

        while let Some(id) = ready.pop() {
            result.push(id);
            if let Some(deps) = dependents.get(id) {
                for &dep_id in deps {
                    if let Some(deg) = in_degree.get_mut(dep_id) {
                        *deg -= 1;
                        if *deg == 0 {
                            // Insert in sorted position for determinism
                            let pos = ready.binary_search(&dep_id).unwrap_or_else(|e| e);
                            ready.insert(pos, dep_id);
                        }
                    }
                }
            }
        }

        result
    }
}

impl Default for BuildGraph {
    fn default() -> Self {
        Self::new("")
    }
}

/// Error information for build failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildErrorInfo {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
    /// Additional detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl BuildErrorInfo {
    /// Create a new error info.
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    /// Create with detail.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Cache status for a build node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheStatus {
    /// Cache hit - node was skipped.
    Hit,
    /// Cache miss - node was executed.
    Miss,
    /// Cache bypassed (--force).
    Bypass,
    /// Node was skipped (dependency failed).
    Skipped,
}

impl CacheStatus {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Bypass => "bypass",
            Self::Skipped => "skipped",
        }
    }
}

impl std::fmt::Display for CacheStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result of executing a single build node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildNodeResult {
    /// Node ID.
    pub id: String,
    /// Whether the node succeeded.
    pub ok: bool,
    /// Cache status.
    pub cache: CacheStatus,
    /// Input hash (hex digest).
    pub hash: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Whether stdout was truncated.
    #[serde(default)]
    pub stdout_truncated: bool,
    /// Whether stderr was truncated.
    #[serde(default)]
    pub stderr_truncated: bool,
    /// Error information if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BuildErrorInfo>,
    /// Notes (always present).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl BuildNodeResult {
    /// Create a cache hit result.
    #[must_use]
    pub fn cache_hit(id: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: true,
            cache: CacheStatus::Hit,
            hash: hash.into(),
            duration_ms: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            error: None,
            notes: Vec::new(),
        }
    }

    /// Create a cache miss result.
    #[must_use]
    pub fn cache_miss(id: impl Into<String>, hash: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            id: id.into(),
            ok: true,
            cache: CacheStatus::Miss,
            hash: hash.into(),
            duration_ms,
            stdout_truncated: false,
            stderr_truncated: false,
            error: None,
            notes: Vec::new(),
        }
    }

    /// Create a failed result.
    #[must_use]
    pub fn failed(
        id: impl Into<String>,
        hash: impl Into<String>,
        duration_ms: u64,
        error: BuildErrorInfo,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            cache: CacheStatus::Miss,
            hash: hash.into(),
            duration_ms,
            stdout_truncated: false,
            stderr_truncated: false,
            error: Some(error),
            notes: Vec::new(),
        }
    }

    /// Create a skipped result.
    #[must_use]
    pub fn skipped(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: false,
            cache: CacheStatus::Skipped,
            hash: String::new(),
            duration_ms: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            error: None,
            notes: vec!["skipped due to dependency failure".to_string()],
        }
    }
}

/// Counts for build run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRunCounts {
    /// Info-level issues.
    pub info: u32,
    /// Warning-level issues.
    pub warn: u32,
    /// Error-level issues.
    pub error: u32,
}

/// Summary of a build run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRunSummary {
    /// Total nodes in graph.
    pub nodes_total: u32,
    /// Nodes that were executed.
    pub nodes_run: u32,
    /// Nodes that were skipped (cache hit or dependency failure).
    pub nodes_skipped: u32,
    /// Cache hits.
    pub cache_hits: u32,
    /// Cache misses.
    pub cache_misses: u32,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Worst severity.
    pub severity: String,
    /// Issue counts.
    pub counts: BuildRunCounts,
}

impl BuildRunSummary {
    /// Create a new summary.
    #[must_use]
    pub fn new() -> Self {
        Self {
            severity: "info".to_string(),
            ..Default::default()
        }
    }

    /// Update severity to the worst of current and new.
    pub fn update_severity(&mut self, has_error: bool, has_warning: bool) {
        if has_error {
            self.severity = "error".to_string();
        } else if has_warning && self.severity != "error" {
            self.severity = "warn".to_string();
        }
    }
}

/// Result of a complete build run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRunResult {
    /// Schema version.
    pub schema_version: u32,
    /// Absolute working directory.
    pub cwd: String,
    /// Whether the build succeeded.
    pub ok: bool,
    /// Node results (in execution order).
    pub results: Vec<BuildNodeResult>,
    /// Summary statistics.
    pub summary: BuildRunSummary,
    /// Notes (always present).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl BuildRunResult {
    /// Create a new build run result.
    #[must_use]
    pub fn new(cwd: impl Into<String>) -> Self {
        Self {
            schema_version: BUILD_RUN_SCHEMA_VERSION,
            cwd: cwd.into(),
            ok: true,
            results: Vec::new(),
            summary: BuildRunSummary::new(),
            notes: Vec::new(),
        }
    }

    /// Add a node result.
    pub fn add_result(&mut self, result: BuildNodeResult) {
        if !result.ok {
            self.ok = false;
            self.summary.counts.error += 1;
            self.summary.update_severity(true, false);
        }

        match result.cache {
            CacheStatus::Hit => {
                self.summary.cache_hits += 1;
                self.summary.nodes_skipped += 1;
            }
            CacheStatus::Miss | CacheStatus::Bypass => {
                self.summary.cache_misses += 1;
                self.summary.nodes_run += 1;
            }
            CacheStatus::Skipped => {
                self.summary.nodes_skipped += 1;
            }
        }

        self.summary.duration_ms += result.duration_ms;
        self.results.push(result);
    }

    /// Finalize the summary.
    pub fn finalize(&mut self, total_nodes: u32) {
        self.summary.nodes_total = total_nodes;
    }
}

impl Default for BuildRunResult {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_versions_are_stable() {
        assert_eq!(BUILD_GRAPH_SCHEMA_VERSION, 1);
        assert_eq!(BUILD_RUN_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_build_node_script() {
        let node = BuildNode::script("build", "tsc -p tsconfig.json");
        assert_eq!(node.id, "script:build");
        assert_eq!(node.kind, BuildNodeKind::Script);
        assert_eq!(node.label, "script:build");
        assert!(node.script.is_some());
    }

    #[test]
    fn test_build_input_serialization() {
        let file = BuildInput::file("/path/to/file.ts");
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("\"type\":\"file\""));
        assert!(json.contains("/path/to/file.ts"));

        let glob = BuildInput::glob("**/*.ts", "/root");
        let json = serde_json::to_string(&glob).unwrap();
        assert!(json.contains("\"type\":\"glob\""));

        let env = BuildInput::env("NODE_ENV");
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"type\":\"env\""));
    }

    #[test]
    fn test_build_graph_normalize() {
        let mut graph = BuildGraph::new("/project");

        let mut node_b = BuildNode::script("b", "echo b");
        node_b.add_input(BuildInput::file("/z.ts"));
        node_b.add_input(BuildInput::file("/a.ts"));
        graph.add_node(node_b);

        let node_a = BuildNode::script("a", "echo a");
        graph.add_node(node_a);

        graph.normalize();

        // Nodes should be sorted by id
        assert_eq!(graph.nodes[0].id, "script:a");
        assert_eq!(graph.nodes[1].id, "script:b");
    }

    #[test]
    fn test_toposort_deterministic() {
        let mut graph = BuildGraph::new("/project");

        let mut node_c = BuildNode::script("c", "echo c");
        node_c.deps = vec!["script:a".to_string(), "script:b".to_string()];
        graph.add_node(node_c);

        let node_a = BuildNode::script("a", "echo a");
        graph.add_node(node_a);

        let node_b = BuildNode::script("b", "echo b");
        graph.add_node(node_b);

        graph.normalize();

        let sorted = graph.toposort();

        // a and b should come before c
        let a_pos = sorted.iter().position(|&x| x == "script:a").unwrap();
        let b_pos = sorted.iter().position(|&x| x == "script:b").unwrap();
        let c_pos = sorted.iter().position(|&x| x == "script:c").unwrap();

        assert!(a_pos < c_pos);
        assert!(b_pos < c_pos);

        // Run again - should be deterministic
        let sorted2 = graph.toposort();
        assert_eq!(sorted, sorted2);
    }

    #[test]
    fn test_build_node_result_cache_hit() {
        let result = BuildNodeResult::cache_hit("script:build", "abc123");
        assert!(result.ok);
        assert_eq!(result.cache, CacheStatus::Hit);
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn test_build_run_result_aggregation() {
        let mut run = BuildRunResult::new("/project");

        run.add_result(BuildNodeResult::cache_hit("script:lint", "aaa"));
        run.add_result(BuildNodeResult::cache_miss("script:build", "bbb", 100));

        assert!(run.ok);
        assert_eq!(run.summary.cache_hits, 1);
        assert_eq!(run.summary.cache_misses, 1);
        assert_eq!(run.summary.duration_ms, 100);
    }

    #[test]
    fn test_build_run_result_failure() {
        let mut run = BuildRunResult::new("/project");

        let error = BuildErrorInfo::new("BUILD_SCRIPT_FAILED", "Exit code 1");
        run.add_result(BuildNodeResult::failed("script:build", "abc", 50, error));

        assert!(!run.ok);
        assert_eq!(run.summary.counts.error, 1);
        assert_eq!(run.summary.severity, "error");
    }

    #[test]
    fn test_cache_status_serialization() {
        assert_eq!(
            serde_json::to_string(&CacheStatus::Hit).unwrap(),
            "\"hit\""
        );
        assert_eq!(
            serde_json::to_string(&CacheStatus::Miss).unwrap(),
            "\"miss\""
        );
    }
}
