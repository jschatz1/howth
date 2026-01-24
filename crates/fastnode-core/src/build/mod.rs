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
//! - Graph schema v1 (v2.0): Initial graph format
//! - Run schema v1 (v2.0): Initial result format

pub mod codes;
pub mod exec;
pub mod graph;
pub mod hash;

pub use codes::*;
pub use exec::{execute_graph, execute_node, run_script, BuildCache, ExecOptions, MemoryCache};
pub use graph::{
    BuildErrorInfo, BuildGraph, BuildInput, BuildNode, BuildNodeKind, BuildNodeResult,
    BuildRunCounts, BuildRunResult, BuildRunSummary, BuildScriptSpec, CacheStatus,
    BUILD_GRAPH_SCHEMA_VERSION, BUILD_RUN_SCHEMA_VERSION, DEFAULT_ENV_ALLOWLIST,
    DEFAULT_GLOB_EXCLUSIONS, MAX_OUTPUT_SIZE,
};
pub use hash::{
    expand_glob, hash_bytes, hash_env, hash_file, hash_glob, hash_graph, hash_input, hash_node,
    hash_string, normalize_path, HashError, HashResult,
};

use crate::pkg::LOCKFILE_NAME;
use std::path::Path;

/// Build a graph from a project directory.
///
/// Reads package.json and constructs a build graph with the "build" script.
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

    // Get build script
    let build_script = pkg_json
        .get("scripts")
        .and_then(|s| s.get("build"))
        .and_then(|s| s.as_str());

    let Some(build_command) = build_script else {
        return Err(BuildGraphError::new(
            codes::BUILD_SCRIPT_NOT_FOUND,
            "No 'build' script found in package.json",
        ));
    };

    // Create build node
    let mut node = BuildNode::script("build", build_command);

    // Add package.json as input
    node.add_input(BuildInput::file(pkg_json_path.to_string_lossy().to_string()));

    // Add lockfile if exists
    let lockfile_path = cwd.join(LOCKFILE_NAME);
    if lockfile_path.exists() {
        node.add_input(BuildInput::lockfile(
            lockfile_path.to_string_lossy().to_string(),
            crate::pkg::PKG_LOCK_SCHEMA_VERSION,
        ));
    }

    // Add tsconfig.json if exists
    let tsconfig_path = cwd.join("tsconfig.json");
    if tsconfig_path.exists() {
        node.add_input(BuildInput::file(
            tsconfig_path.to_string_lossy().to_string(),
        ));
    }

    // Add source glob (excluding node_modules, .git, dist, build)
    node.add_input(BuildInput::glob("**/*".to_string(), cwd_str.clone()));

    // Add environment inputs
    for env_key in DEFAULT_ENV_ALLOWLIST {
        node.add_input(BuildInput::env((*env_key).to_string()));
    }

    graph.add_node(node);
    graph.add_entrypoint("script:build");
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
    fn test_build_graph_from_project_no_build_script() {
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
            if let BuildInput::File { path } = i {
                path.contains("tsconfig.json")
            } else {
                false
            }
        });
        assert!(has_tsconfig);
    }
}
