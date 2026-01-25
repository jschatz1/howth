//! BuildGraph types for the incremental build system.
//!
//! The BuildGraph represents the dependency graph of build nodes.
//! Each node has inputs (files, globs, env vars) and can depend on other nodes.
//!
//! ## Schema Versions
//!
//! - Schema version 1 (v2.0): Initial build graph format (single node)
//! - Schema version 2 (v2.1): Multi-node graph with defaults and targets

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Schema version for the BuildGraph format.
/// v2.1: Multi-node support with defaults.
pub const BUILD_GRAPH_SCHEMA_VERSION: u32 = 2;

/// Schema version for the BuildRunResult format.
pub const BUILD_RUN_SCHEMA_VERSION: u32 = 1;

/// Default environment variables included in hash.
/// Locked for v2.0+.
pub const DEFAULT_ENV_ALLOWLIST: &[&str] = &["NODE_ENV", "CI"];

/// Default glob exclusions for source files.
/// Locked for v2.0+.
pub const DEFAULT_GLOB_EXCLUSIONS: &[&str] = &[
    "node_modules/**",
    ".git/**",
    "dist/**",
    "build/**",
    ".howth/**",
];

/// Maximum stdout/stderr capture size per stream (256KB).
pub const MAX_OUTPUT_SIZE: usize = 256 * 1024;

/// Target aliases for common script names.
pub const TARGET_ALIASES: &[(&str, &str)] = &[
    ("build", "script:build"),
    ("test", "script:test"),
    ("lint", "script:lint"),
    ("typecheck", "script:typecheck"),
    ("dev", "script:dev"),
];

/// Resolve a target alias to its full node ID.
/// Returns the input unchanged if not an alias.
#[must_use]
pub fn resolve_target_alias(target: &str) -> &str {
    for (alias, full_id) in TARGET_ALIASES {
        if target == *alias {
            return full_id;
        }
    }
    target
}

/// Normalize a relative path for deterministic comparison.
/// - Uses forward slashes on all platforms
/// - Removes trailing slashes
/// - Collapses multiple slashes
/// - Removes `.` segments
#[must_use]
pub fn normalize_rel_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    let normalized = s
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect::<Vec<_>>()
        .join("/");
    normalized
}

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

/// Command specification for a build node (v2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCommand {
    /// Command arguments (first element is the command itself).
    pub argv: Vec<String>,
    /// Relative working directory (from graph cwd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd_rel: Option<String>,
    /// Whether to run via shell.
    #[serde(default = "default_shell")]
    pub shell: bool,
    /// Timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

fn default_shell() -> bool {
    true
}

impl BuildCommand {
    /// Create a new shell command from a command string.
    #[must_use]
    pub fn shell(command: impl Into<String>) -> Self {
        let cmd = command.into();
        Self {
            argv: vec![cmd],
            cwd_rel: None,
            shell: true,
            timeout_ms: None,
        }
    }

    /// Create a new direct command (no shell).
    #[must_use]
    pub fn direct(argv: Vec<String>) -> Self {
        Self {
            argv,
            cwd_rel: None,
            shell: false,
            timeout_ms: None,
        }
    }

    /// Get the command string for shell execution.
    #[must_use]
    pub fn command_str(&self) -> &str {
        self.argv.first().map(String::as_str).unwrap_or("")
    }
}

/// Cache policy for a build node (v2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCachePolicy {
    /// Whether caching is enabled.
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    /// Cache mode: "inputs_only" for v2.1.
    #[serde(default = "default_cache_mode")]
    pub mode: String,
}

fn default_cache_enabled() -> bool {
    true
}

fn default_cache_mode() -> String {
    "inputs_only".to_string()
}

impl Default for BuildCachePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "inputs_only".to_string(),
        }
    }
}

/// Script specification for a build node (legacy v2.0 compat).
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BuildInput {
    /// A single file.
    File {
        /// Path to the file (relative or absolute).
        path: String,
        /// Whether the file is optional.
        #[serde(default)]
        optional: bool,
    },
    /// A glob pattern.
    Glob {
        /// Glob pattern (normalized).
        pattern: String,
        /// Root directory (absolute).
        root: String,
        /// Whether the glob is optional.
        #[serde(default)]
        optional: bool,
    },
    /// A directory.
    Dir {
        /// Path to the directory.
        path: String,
        /// Whether the directory is optional.
        #[serde(default)]
        optional: bool,
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
    /// A dependency node (v2.1).
    Node {
        /// Node ID of the dependency.
        id: String,
    },
}

impl BuildInput {
    /// Create a file input.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File {
            path: path.into(),
            optional: false,
        }
    }

    /// Create an optional file input.
    #[must_use]
    pub fn file_optional(path: impl Into<String>) -> Self {
        Self::File {
            path: path.into(),
            optional: true,
        }
    }

    /// Create a glob input.
    #[must_use]
    pub fn glob(pattern: impl Into<String>, root: impl Into<String>) -> Self {
        Self::Glob {
            pattern: pattern.into(),
            root: root.into(),
            optional: false,
        }
    }

    /// Create an optional glob input.
    #[must_use]
    pub fn glob_optional(pattern: impl Into<String>, root: impl Into<String>) -> Self {
        Self::Glob {
            pattern: pattern.into(),
            root: root.into(),
            optional: true,
        }
    }

    /// Create a directory input.
    #[must_use]
    pub fn dir(path: impl Into<String>) -> Self {
        Self::Dir {
            path: path.into(),
            optional: false,
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

    /// Create a node dependency input (v2.1).
    #[must_use]
    pub fn node(id: impl Into<String>) -> Self {
        Self::Node { id: id.into() }
    }

    /// Get the type string for this input.
    #[must_use]
    pub fn type_str(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Glob { .. } => "glob",
            Self::Dir { .. } => "dir",
            Self::Package { .. } => "package",
            Self::Lockfile { .. } => "lockfile",
            Self::Env { .. } => "env",
            Self::Node { .. } => "node",
        }
    }

    /// Get the sort key for deterministic ordering.
    fn sort_key(&self) -> (u8, String, String, bool) {
        match self {
            Self::Dir { path, optional } => (0, path.clone(), String::new(), *optional),
            Self::Env { key } => (1, key.clone(), String::new(), false),
            Self::File { path, optional } => (2, path.clone(), String::new(), *optional),
            Self::Glob {
                pattern,
                root,
                optional,
            } => (3, pattern.clone(), root.clone(), *optional),
            Self::Lockfile {
                path,
                schema_version,
            } => (4, path.clone(), schema_version.to_string(), false),
            Self::Node { id } => (5, id.clone(), String::new(), false),
            Self::Package { name, version } => {
                (6, name.clone(), version.clone().unwrap_or_default(), false)
            }
        }
    }
}

/// A build output (v2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct BuildOutput {
    /// Output kind: "file", "glob", or "dir".
    pub kind: String,
    /// Path (relative to cwd).
    pub path: String,
    /// Whether the output is optional.
    #[serde(default)]
    pub optional: bool,
}

impl BuildOutput {
    /// Create a file output.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            kind: "file".to_string(),
            path: path.into(),
            optional: false,
        }
    }

    /// Create a directory output.
    #[must_use]
    pub fn dir(path: impl Into<String>) -> Self {
        Self {
            kind: "dir".to_string(),
            path: path.into(),
            optional: false,
        }
    }
}

/// Environment variable for a build node (v2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct BuildEnv {
    /// Environment variable key.
    pub key: String,
    /// Environment variable value.
    pub value: String,
}

impl BuildEnv {
    /// Create a new environment variable.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
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
    /// Output paths (deterministically ordered, v2.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<BuildOutput>,
    /// Environment variables to set (v2.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<BuildEnv>,
    /// Environment variables to include in hash (sorted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allowlist: Vec<String>,
    /// Command to execute (v2.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<BuildCommand>,
    /// Script specification (legacy v2.0 compat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<BuildScriptSpec>,
    /// Node IDs this depends on (sorted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deps: Vec<String>,
    /// Cache policy (v2.1).
    #[serde(default)]
    pub cache: BuildCachePolicy,
}

impl BuildNode {
    /// Create a new script node.
    #[must_use]
    pub fn script(name: &str, cmd: &str) -> Self {
        Self {
            id: format!("script:{name}"),
            kind: BuildNodeKind::Script,
            label: format!("script:{name}"),
            inputs: Vec::new(),
            outputs: Vec::new(),
            env: Vec::new(),
            env_allowlist: DEFAULT_ENV_ALLOWLIST
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            command: Some(BuildCommand::shell(cmd)),
            script: Some(BuildScriptSpec::new(name, cmd)),
            deps: Vec::new(),
            cache: BuildCachePolicy::default(),
        }
    }

    /// Add an input to this node.
    pub fn add_input(&mut self, input: BuildInput) {
        self.inputs.push(input);
    }

    /// Add an output to this node.
    pub fn add_output(&mut self, output: BuildOutput) {
        self.outputs.push(output);
    }

    /// Add a dependency on another node.
    pub fn add_dep(&mut self, node_id: impl Into<String>) {
        let id = node_id.into();
        self.deps.push(id.clone());
        self.inputs.push(BuildInput::node(id));
    }

    /// Sort inputs, outputs, env, and deps for deterministic ordering.
    pub fn normalize(&mut self) {
        // Sort inputs by sort key
        self.inputs.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

        // Sort outputs
        self.outputs.sort();

        // Sort env by key
        self.env.sort_by(|a, b| a.key.cmp(&b.key));

        // Sort env allowlist
        self.env_allowlist.sort();

        // Sort deps
        self.deps.sort();
    }
}

/// Optional metadata for the build graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildGraphMeta {
    /// Project name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Project version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
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
    /// Default targets to run (sorted, v2.1).
    #[serde(default)]
    pub defaults: Vec<String>,
    /// Optional metadata (does not affect hashing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<BuildGraphMeta>,
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
            defaults: Vec::new(),
            meta: None,
            notes: Vec::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, mut node: BuildNode) {
        node.normalize();
        self.nodes.push(node);
    }

    /// Add a default target.
    pub fn add_default(&mut self, id: impl Into<String>) {
        self.defaults.push(id.into());
    }

    /// Sort the graph for deterministic ordering.
    pub fn normalize(&mut self) {
        // Sort nodes by id
        self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        // Sort defaults
        self.defaults.sort();
    }

    /// Get a node by ID.
    #[must_use]
    pub fn get_node(&self, id: &str) -> Option<&BuildNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Check if a node ID exists in the graph.
    #[must_use]
    pub fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|n| n.id == id)
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

    /// Get topologically sorted node IDs with parallel execution levels.
    /// Returns (all_nodes_in_order, levels) where each level can run in parallel.
    #[must_use]
    pub fn toposort_levels(&self) -> (Vec<&str>, Vec<Vec<&str>>) {
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

        let mut all_nodes = Vec::new();
        let mut levels = Vec::new();

        // Collect nodes level by level
        loop {
            let mut current_level: Vec<&str> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(&id, _)| id)
                .collect();

            if current_level.is_empty() {
                break;
            }

            // Sort for determinism
            current_level.sort();

            // Remove from in_degree and update dependents
            for &id in &current_level {
                in_degree.remove(id);
                if let Some(deps) = dependents.get(id) {
                    for &dep_id in deps {
                        if let Some(deg) = in_degree.get_mut(dep_id) {
                            *deg -= 1;
                        }
                    }
                }
            }

            all_nodes.extend(current_level.iter().copied());
            levels.push(current_level);
        }

        (all_nodes, levels)
    }

    /// Plan which nodes to execute for the given targets.
    ///
    /// Resolves target aliases, computes the dependency closure, and returns
    /// the execution plan with nodes in topological order and parallel levels.
    ///
    /// Returns `Err` with the invalid target ID if any target is not found.
    pub fn plan_targets(&self, targets: &[String]) -> Result<BuildPlan, String> {
        use std::collections::HashSet;

        // Resolve aliases and validate targets
        let mut resolved_targets = Vec::new();
        for target in targets {
            let resolved = resolve_target_alias(target);
            if !self.has_node(resolved) {
                return Err(resolved.to_string());
            }
            resolved_targets.push(resolved.to_string());
        }

        // Compute closure of all dependencies
        let mut closure: HashSet<&str> = HashSet::new();
        let mut stack: Vec<&str> = resolved_targets.iter().map(|s| s.as_str()).collect();

        while let Some(id) = stack.pop() {
            if closure.contains(id) {
                continue;
            }
            closure.insert(id);

            if let Some(node) = self.get_node(id) {
                for dep in &node.deps {
                    if !closure.contains(dep.as_str()) {
                        stack.push(dep.as_str());
                    }
                }
            }
        }

        // Build a subgraph with only the closure nodes
        let subgraph_nodes: Vec<&BuildNode> = self
            .nodes
            .iter()
            .filter(|n| closure.contains(n.id.as_str()))
            .collect();

        // Compute toposort for the subgraph
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

        for node in &subgraph_nodes {
            in_degree.entry(node.id.as_str()).or_insert(0);
            for dep in &node.deps {
                if closure.contains(dep.as_str()) {
                    *in_degree.entry(node.id.as_str()).or_insert(0) += 1;
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(node.id.as_str());
                }
            }
        }

        // Collect nodes in parallel levels
        let mut all_nodes = Vec::new();
        let mut levels = Vec::new();

        loop {
            let mut current_level: Vec<String> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(&id, _)| id.to_string())
                .collect();

            if current_level.is_empty() {
                break;
            }

            current_level.sort();

            for id in &current_level {
                in_degree.remove(id.as_str());
                if let Some(deps) = dependents.get(id.as_str()) {
                    for &dep_id in deps {
                        if let Some(deg) = in_degree.get_mut(dep_id) {
                            *deg -= 1;
                        }
                    }
                }
            }

            all_nodes.extend(current_level.iter().cloned());
            levels.push(current_level);
        }

        Ok(BuildPlan {
            requested_targets: resolved_targets,
            nodes: all_nodes,
            levels,
        })
    }
}

/// Execution plan for a set of targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildPlan {
    /// The resolved target IDs that were requested.
    pub requested_targets: Vec<String>,
    /// All node IDs to execute, in topological order.
    pub nodes: Vec<String>,
    /// Nodes grouped by parallel execution level.
    /// Each level can be executed in parallel; levels must be executed in order.
    pub levels: Vec<Vec<String>>,
}

impl BuildPlan {
    /// Get the total number of nodes in the plan.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the plan is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
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

/// Reason for a node's execution status (v2.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildNodeReason {
    /// Node was served from cache.
    CacheHit,
    /// Forced rebuild (--force).
    Forced,
    /// Input files changed.
    InputChanged,
    /// A dependency changed.
    DepChanged,
    /// Dependency failed, node skipped.
    DepFailed,
    /// First build (no cache entry).
    FirstBuild,
    /// Output fingerprint mismatch (v2.2+).
    OutputsChanged,
}

impl BuildNodeReason {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CacheHit => "cache_hit",
            Self::Forced => "forced",
            Self::InputChanged => "input_changed",
            Self::DepChanged => "dep_changed",
            Self::DepFailed => "dep_failed",
            Self::FirstBuild => "first_build",
            Self::OutputsChanged => "outputs_changed",
        }
    }

    /// Get a human-readable description.
    #[must_use]
    pub fn to_human_string(&self) -> &'static str {
        match self {
            Self::CacheHit => "cache hit",
            Self::Forced => "forced rebuild (--force)",
            Self::InputChanged => "inputs changed",
            Self::DepChanged => "dependency changed",
            Self::DepFailed => "dependency failed",
            Self::FirstBuild => "first build (cache cold)",
            Self::OutputsChanged => "outputs changed (fingerprint mismatch)",
        }
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
    /// Reason for the execution status (v2.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<BuildNodeReason>,
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
            reason: Some(BuildNodeReason::CacheHit),
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
            reason: Some(BuildNodeReason::FirstBuild),
            stdout_truncated: false,
            stderr_truncated: false,
            error: None,
            notes: Vec::new(),
        }
    }

    /// Create a cache miss result with a reason.
    #[must_use]
    pub fn cache_miss_with_reason(
        id: impl Into<String>,
        hash: impl Into<String>,
        duration_ms: u64,
        reason: BuildNodeReason,
    ) -> Self {
        Self {
            id: id.into(),
            ok: true,
            cache: CacheStatus::Miss,
            hash: hash.into(),
            duration_ms,
            reason: Some(reason),
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
            reason: None,
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
            reason: Some(BuildNodeReason::DepFailed),
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
    /// Requested targets (v2.1).
    #[serde(default)]
    pub requested_targets: Vec<String>,
    /// Node results (sorted by id in execution order).
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
            requested_targets: Vec::new(),
            results: Vec::new(),
            summary: BuildRunSummary::new(),
            notes: Vec::new(),
        }
    }

    /// Set the requested targets.
    pub fn set_targets(&mut self, targets: Vec<String>) {
        self.requested_targets = targets;
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
        // Sort results by id for deterministic output
        self.results.sort_by(|a, b| a.id.cmp(&b.id));
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
        assert_eq!(BUILD_GRAPH_SCHEMA_VERSION, 2);
        assert_eq!(BUILD_RUN_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_build_node_script() {
        let node = BuildNode::script("build", "tsc -p tsconfig.json");
        assert_eq!(node.id, "script:build");
        assert_eq!(node.kind, BuildNodeKind::Script);
        assert_eq!(node.label, "script:build");
        assert!(node.script.is_some());
        assert!(node.command.is_some());
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

        let node = BuildInput::node("script:build");
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"type\":\"node\""));
        assert!(json.contains("script:build"));
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
    fn test_toposort_levels() {
        let mut graph = BuildGraph::new("/project");

        // c depends on a and b
        let mut node_c = BuildNode::script("c", "echo c");
        node_c.deps = vec!["script:a".to_string(), "script:b".to_string()];
        graph.add_node(node_c);

        let node_a = BuildNode::script("a", "echo a");
        graph.add_node(node_a);

        let node_b = BuildNode::script("b", "echo b");
        graph.add_node(node_b);

        graph.normalize();

        let (all_nodes, levels) = graph.toposort_levels();

        // Should have 2 levels: [a, b] and [c]
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].len(), 2);
        assert!(levels[0].contains(&"script:a"));
        assert!(levels[0].contains(&"script:b"));
        assert_eq!(levels[1], vec!["script:c"]);

        // all_nodes should have all 3
        assert_eq!(all_nodes.len(), 3);
    }

    #[test]
    fn test_build_node_result_cache_hit() {
        let result = BuildNodeResult::cache_hit("script:build", "abc123");
        assert!(result.ok);
        assert_eq!(result.cache, CacheStatus::Hit);
        assert_eq!(result.duration_ms, 0);
        assert_eq!(result.reason, Some(BuildNodeReason::CacheHit));
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

    #[test]
    fn test_resolve_target_alias() {
        assert_eq!(resolve_target_alias("build"), "script:build");
        assert_eq!(resolve_target_alias("test"), "script:test");
        assert_eq!(resolve_target_alias("lint"), "script:lint");
        assert_eq!(resolve_target_alias("script:custom"), "script:custom");
        assert_eq!(resolve_target_alias("unknown"), "unknown");
    }

    #[test]
    fn test_normalize_rel_path() {
        assert_eq!(normalize_rel_path(Path::new("src/foo/bar")), "src/foo/bar");
        assert_eq!(
            normalize_rel_path(Path::new("src\\foo\\bar")),
            "src/foo/bar"
        );
        assert_eq!(normalize_rel_path(Path::new("src/foo/")), "src/foo");
        assert_eq!(normalize_rel_path(Path::new("./src/foo")), "src/foo");
    }

    #[test]
    fn test_build_graph_v2_is_deterministic_ordering() {
        let mut graph1 = BuildGraph::new("/project");
        let mut graph2 = BuildGraph::new("/project");

        // Add nodes in different order
        let mut node_b = BuildNode::script("b", "echo b");
        node_b.add_input(BuildInput::env("Z_VAR"));
        node_b.add_input(BuildInput::env("A_VAR"));
        node_b.add_input(BuildInput::file("/z.ts"));
        node_b.add_input(BuildInput::file("/a.ts"));

        let node_a = BuildNode::script("a", "echo a");

        graph1.add_node(node_b.clone());
        graph1.add_node(node_a.clone());
        graph1.add_default("script:b");
        graph1.add_default("script:a");
        graph1.normalize();

        graph2.add_node(node_a);
        graph2.add_node(node_b);
        graph2.add_default("script:a");
        graph2.add_default("script:b");
        graph2.normalize();

        // Should produce identical JSON
        let json1 = serde_json::to_string(&graph1).unwrap();
        let json2 = serde_json::to_string(&graph2).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_build_run_result_targets() {
        let mut run = BuildRunResult::new("/project");
        run.set_targets(vec!["script:build".to_string(), "script:test".to_string()]);

        assert_eq!(run.requested_targets.len(), 2);
        assert!(run.requested_targets.contains(&"script:build".to_string()));
    }

    #[test]
    fn test_build_command_shell() {
        let cmd = BuildCommand::shell("npm run build");
        assert_eq!(cmd.command_str(), "npm run build");
        assert!(cmd.shell);
    }

    #[test]
    fn test_build_node_add_dep() {
        let mut node = BuildNode::script("test", "npm test");
        node.add_dep("script:build");

        assert!(node.deps.contains(&"script:build".to_string()));
        // Should also add a Node input
        assert!(node.inputs.iter().any(|i| matches!(i, BuildInput::Node { id } if id == "script:build")));
    }

    #[test]
    fn test_plan_targets_single_node() {
        let mut graph = BuildGraph::new("/project");
        graph.add_node(BuildNode::script("build", "echo build"));
        graph.normalize();

        let plan = graph.plan_targets(&["build".to_string()]).unwrap();

        assert_eq!(plan.requested_targets, vec!["script:build"]);
        assert_eq!(plan.nodes, vec!["script:build"]);
        assert_eq!(plan.levels, vec![vec!["script:build"]]);
    }

    #[test]
    fn test_plan_targets_with_dependencies() {
        let mut graph = BuildGraph::new("/project");

        let build_node = BuildNode::script("build", "echo build");
        graph.add_node(build_node);

        let mut test_node = BuildNode::script("test", "echo test");
        test_node.deps = vec!["script:build".to_string()];
        graph.add_node(test_node);

        graph.normalize();

        // Request test, should include build as dependency
        let plan = graph.plan_targets(&["test".to_string()]).unwrap();

        assert_eq!(plan.requested_targets, vec!["script:test"]);
        assert_eq!(plan.nodes.len(), 2);
        assert!(plan.nodes.contains(&"script:build".to_string()));
        assert!(plan.nodes.contains(&"script:test".to_string()));

        // build should come before test in toposort
        let build_pos = plan.nodes.iter().position(|x| x == "script:build").unwrap();
        let test_pos = plan.nodes.iter().position(|x| x == "script:test").unwrap();
        assert!(build_pos < test_pos);
    }

    #[test]
    fn test_plan_targets_parallel_levels() {
        let mut graph = BuildGraph::new("/project");

        let build_node = BuildNode::script("build", "echo build");
        graph.add_node(build_node);

        let lint_node = BuildNode::script("lint", "echo lint");
        graph.add_node(lint_node);

        // test depends on both build and lint
        let mut test_node = BuildNode::script("test", "echo test");
        test_node.deps = vec!["script:build".to_string(), "script:lint".to_string()];
        graph.add_node(test_node);

        graph.normalize();

        let plan = graph.plan_targets(&["test".to_string()]).unwrap();

        // Should have 2 levels: [build, lint] and [test]
        assert_eq!(plan.levels.len(), 2);
        assert_eq!(plan.levels[0].len(), 2);
        assert!(plan.levels[0].contains(&"script:build".to_string()));
        assert!(plan.levels[0].contains(&"script:lint".to_string()));
        assert_eq!(plan.levels[1], vec!["script:test"]);
    }

    #[test]
    fn test_plan_targets_alias_resolution() {
        let mut graph = BuildGraph::new("/project");
        graph.add_node(BuildNode::script("build", "echo build"));
        graph.normalize();

        // "build" should resolve to "script:build"
        let plan = graph.plan_targets(&["build".to_string()]).unwrap();
        assert_eq!(plan.requested_targets, vec!["script:build"]);
    }

    #[test]
    fn test_plan_targets_invalid_target() {
        let mut graph = BuildGraph::new("/project");
        graph.add_node(BuildNode::script("build", "echo build"));
        graph.normalize();

        let result = graph.plan_targets(&["nonexistent".to_string()]);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "nonexistent");
    }

    #[test]
    fn test_plan_targets_multiple_targets() {
        let mut graph = BuildGraph::new("/project");
        graph.add_node(BuildNode::script("build", "echo build"));
        graph.add_node(BuildNode::script("test", "echo test"));
        graph.add_node(BuildNode::script("lint", "echo lint"));
        graph.normalize();

        let plan = graph
            .plan_targets(&["build".to_string(), "test".to_string()])
            .unwrap();

        assert_eq!(plan.requested_targets.len(), 2);
        assert!(plan.requested_targets.contains(&"script:build".to_string()));
        assert!(plan.requested_targets.contains(&"script:test".to_string()));
        assert_eq!(plan.nodes.len(), 2);
    }

    #[test]
    fn test_build_plan_helpers() {
        let plan = BuildPlan {
            requested_targets: vec!["script:build".to_string()],
            nodes: vec!["script:build".to_string(), "script:test".to_string()],
            levels: vec![
                vec!["script:build".to_string()],
                vec!["script:test".to_string()],
            ],
        };

        assert_eq!(plan.node_count(), 2);
        assert!(!plan.is_empty());

        let empty_plan = BuildPlan {
            requested_targets: vec![],
            nodes: vec![],
            levels: vec![],
        };
        assert!(empty_plan.is_empty());
    }
}
