//! Build system for incremental, cached builds.
//!
//! The build system provides:
//! - Deterministic build graphs
//! - Content-based hashing for cache keys
//! - Incremental execution with caching
//! - Watch mode integration
//!
//! ## Overview
//!
//! The build system works by:
//! 1. Constructing a `BuildGraph` from package.json scripts
//! 2. Computing content hashes for all inputs
//! 3. Checking the cache for matching hashes
//! 4. Executing only nodes with changed inputs
//!
//! ## Schema Versions
//!
//! - Graph schema v1 (v2.0): Initial graph format (single node)
//! - Graph schema v2 (v2.1): Multi-node graph with defaults + targets
//! - Fingerprint schema v1 (v2.2): Output fingerprinting for cache correctness

pub mod codes;
pub mod exec;
pub mod fingerprint;
pub mod graph;
pub mod hash;

pub use codes::*;
pub use exec::{
    execute_graph, execute_node, run_script, BuildCache, CacheEntry, ExecOptions, MemoryCache,
};
pub use graph::{
    resolve_target_alias, BuildErrorInfo, BuildGraph, BuildInput, BuildNode, BuildNodeKind,
    BuildNodeReason, BuildNodeResult, BuildOutput, BuildPlan, BuildRunCounts, BuildRunResult,
    BuildRunSummary, BuildScriptSpec, CacheStatus, BUILD_GRAPH_SCHEMA_VERSION,
    BUILD_RUN_SCHEMA_VERSION, DEFAULT_ENV_ALLOWLIST, DEFAULT_GLOB_EXCLUSIONS, MAX_OUTPUT_SIZE,
    TARGET_ALIASES,
};
pub use hash::{
    expand_glob, hash_bytes, hash_env, hash_file, hash_glob, hash_graph, hash_input,
    hash_input_with_deps, hash_node, hash_node_with_deps, hash_string, normalize_path, HashError,
    HashResult,
};
pub use fingerprint::{
    compute_fingerprint, fingerprints_match, normalize_output_path, FingerprintError,
    FingerprintMode, FingerprintResult, OutputFingerprint, FINGERPRINT_SCHEMA_VERSION,
};

use crate::pkg::LOCKFILE_NAME;
use std::collections::BTreeMap;
use std::path::Path;

/// Build a graph from a project directory (v2.1 multi-node).
///
/// Reads package.json and constructs a build graph with nodes for all scripts.
/// Sets default targets based on what scripts exist.
pub fn build_graph_from_project(cwd: &Path) -> Result<BuildGraph, BuildGraphError> {
    let cwd_str = cwd.to_string_lossy().to_string();
    let mut graph = BuildGraph::new(&cwd_str);

    // Read package.json
    let pkg_json_path = cwd.join("package.json");
    if !pkg_json_path.exists() {
        return Err(BuildGraphError::new(
            codes::BUILD_PACKAGE_JSON_NOT_FOUND,
            format!("package.json not found at {}", pkg_json_path.display()),
        ));
    }

    let pkg_json_content = std::fs::read_to_string(&pkg_json_path).map_err(|e| {
        BuildGraphError::new(
            codes::BUILD_PACKAGE_JSON_INVALID,
            format!("Failed to read package.json: {e}"),
        )
    })?;

    let pkg_json: serde_json::Value = serde_json::from_str(&pkg_json_content).map_err(|e| {
        BuildGraphError::new(
            codes::BUILD_PACKAGE_JSON_INVALID,
            format!("Invalid package.json: {e}"),
        )
    })?;

    // Set graph metadata
    if let Some(name) = pkg_json.get("name").and_then(|n| n.as_str()) {
        let meta = graph.meta.get_or_insert_with(Default::default);
        meta.name = Some(name.to_string());
    }
    if let Some(version) = pkg_json.get("version").and_then(|v| v.as_str()) {
        let meta = graph.meta.get_or_insert_with(Default::default);
        meta.version = Some(version.to_string());
    }

    // Get all scripts from package.json
    let scripts = pkg_json
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|cmd| (k.clone(), cmd.to_string())))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    if scripts.is_empty() {
        return Err(BuildGraphError::new(
            codes::BUILD_SCRIPT_NOT_FOUND,
            "No scripts found in package.json",
        ));
    }

    // Common inputs for all nodes
    let pkg_json_input = BuildInput::file(pkg_json_path.to_string_lossy().to_string());
    let lockfile_input = {
        let lockfile_path = cwd.join(LOCKFILE_NAME);
        if lockfile_path.exists() {
            Some(BuildInput::lockfile(
                lockfile_path.to_string_lossy().to_string(),
                crate::pkg::PKG_LOCK_SCHEMA_VERSION,
            ))
        } else {
            None
        }
    };
    let tsconfig_input = {
        let tsconfig_path = cwd.join("tsconfig.json");
        if tsconfig_path.exists() {
            Some(BuildInput::file(
                tsconfig_path.to_string_lossy().to_string(),
            ))
        } else {
            None
        }
    };
    let source_glob = BuildInput::glob("**/*".to_string(), cwd_str.clone());

    // Create nodes for each script
    for (name, command) in &scripts {
        let mut node = BuildNode::script(name, command);

        // Add common inputs
        node.add_input(pkg_json_input.clone());
        if let Some(ref lf) = lockfile_input {
            node.add_input(lf.clone());
        }
        if let Some(ref ts) = tsconfig_input {
            node.add_input(ts.clone());
        }
        node.add_input(source_glob.clone());

        // Add environment inputs
        for env_key in DEFAULT_ENV_ALLOWLIST {
            node.add_input(BuildInput::env((*env_key).to_string()));
        }

        graph.add_node(node);
    }

    // Set default targets: "build" if it exists
    if scripts.contains_key("build") {
        graph.add_default("script:build");
    }

    graph.normalize();

    Ok(graph)
}

/// Error building a graph.
#[derive(Debug)]
pub struct BuildGraphError {
    /// Error code.
    pub code: &'static str,
    /// Error message.
    pub message: String,
}

impl BuildGraphError {
    /// Create a new error.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BuildGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BuildGraphError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_graph_from_project_no_package_json() {
        let dir = tempdir().unwrap();
        let result = build_graph_from_project(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, codes::BUILD_PACKAGE_JSON_NOT_FOUND);
    }

    #[test]
    fn test_build_graph_from_project_no_scripts() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {}}"#,
        )
        .unwrap();

        let result = build_graph_from_project(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, codes::BUILD_SCRIPT_NOT_FOUND);
    }

    #[test]
    fn test_build_graph_from_project_success() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "script:build");
        assert!(graph.nodes[0].script.is_some());
        assert_eq!(
            graph.nodes[0].script.as_ref().unwrap().command,
            "echo building"
        );
        // Default should be script:build
        assert!(graph.defaults.contains(&"script:build".to_string()));
    }

    #[test]
    fn test_build_graph_multi_node() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build", "test": "echo test", "lint": "echo lint"}}"#,
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        assert_eq!(graph.nodes.len(), 3);
        assert!(graph.has_node("script:build"));
        assert!(graph.has_node("script:test"));
        assert!(graph.has_node("script:lint"));
        // Default should be script:build
        assert_eq!(graph.defaults, vec!["script:build"]);
    }

    #[test]
    fn test_build_graph_no_build_script_no_default() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"test": "echo test", "lint": "echo lint"}}"#,
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        // No "build" script, so no defaults
        assert!(graph.defaults.is_empty());
    }

    #[test]
    fn test_build_graph_includes_lockfile_if_exists() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("howth.lock"), "{}").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have lockfile input
        let has_lockfile = graph.nodes[0].inputs.iter().any(|i| {
            matches!(i, BuildInput::Lockfile { .. })
        });
        assert!(has_lockfile);
    }

    #[test]
    fn test_build_graph_includes_tsconfig_if_exists() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "tsc"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have tsconfig input
        let has_tsconfig = graph.nodes[0].inputs.iter().any(|i| {
            if let BuildInput::File { path, .. } = i {
                path.contains("tsconfig.json")
            } else {
                false
            }
        });
        assert!(has_tsconfig);
    }

    #[test]
    fn test_build_graph_metadata() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "my-app", "version": "1.2.3", "scripts": {"build": "echo"}}"#,
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        assert!(graph.meta.is_some());
        let meta = graph.meta.as_ref().unwrap();
        assert_eq!(meta.name, Some("my-app".to_string()));
        assert_eq!(meta.version, Some("1.2.3".to_string()));
    }

    #[test]
    fn test_build_graph_deterministic_ordering() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"z": "echo z", "a": "echo a", "m": "echo m"}}"#,
        )
        .unwrap();

        let graph1 = build_graph_from_project(dir.path()).unwrap();
        let graph2 = build_graph_from_project(dir.path()).unwrap();

        // Should produce identical JSON
        let json1 = serde_json::to_string(&graph1).unwrap();
        let json2 = serde_json::to_string(&graph2).unwrap();
        assert_eq!(json1, json2);

        // Nodes should be sorted by id
        assert_eq!(graph1.nodes[0].id, "script:a");
        assert_eq!(graph1.nodes[1].id, "script:m");
        assert_eq!(graph1.nodes[2].id, "script:z");
    }
}
