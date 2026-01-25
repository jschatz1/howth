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

#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::map_unwrap_or)]

pub mod codes;
pub mod exec;
pub mod fingerprint;
pub mod graph;
pub mod hash;

pub use codes::*;
pub use exec::{
    execute_graph, execute_graph_with_backend, execute_graph_with_file_cache, execute_node,
    execute_transpile, execute_transpile_batch, execute_typecheck, run_script, BuildCache,
    CacheEntry, ExecOptions, MemoryCache,
};
pub use fingerprint::{
    compute_fingerprint, fingerprints_match, normalize_output_path, FingerprintError,
    FingerprintMode, FingerprintResult, OutputFingerprint, FINGERPRINT_SCHEMA_VERSION,
};
pub use graph::{
    resolve_target_alias, BuildErrorInfo, BuildGraph, BuildInput, BuildNode, BuildNodeKind,
    BuildNodeReason, BuildNodeResult, BuildOutput, BuildPlan, BuildRunCounts, BuildRunResult,
    BuildRunSummary, BuildScriptSpec, CacheStatus, BUILD_GRAPH_SCHEMA_VERSION,
    BUILD_RUN_SCHEMA_VERSION, DEFAULT_ENV_ALLOWLIST, DEFAULT_GLOB_EXCLUSIONS, MAX_OUTPUT_SIZE,
    TARGET_ALIASES,
};
pub use hash::{
    expand_glob, hash_bytes, hash_env, hash_file, hash_file_with_ctx, hash_glob,
    hash_glob_with_ctx, hash_graph, hash_graph_with_ctx, hash_input, hash_input_with_ctx,
    hash_input_with_deps, hash_input_with_deps_ctx, hash_node, hash_node_with_deps,
    hash_node_with_deps_ctx, hash_string, normalize_path, FileHashCache, FileHashCacheStats,
    FileHashKey, HashContext, HashError, HashResult, InMemoryFileHashCache,
};

use crate::compiler::TranspileSpec;
use crate::pkg::LOCKFILE_NAME;
use std::collections::BTreeMap;
use std::path::Path;

/// Scripts that signal this is a typical JS project (for transpile auto-discovery).
const JS_PROJECT_SIGNAL_SCRIPTS: &[&str] = &["build", "dev", "test", "lint", "typecheck"];

/// File extensions that can be transpiled.
const TRANSPILABLE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx"];

/// Build a graph from a project directory (v2.1 multi-node).
///
/// Reads package.json and constructs a build graph with nodes for all scripts.
/// Sets default targets based on what scripts exist.
///
/// ## Automatic Transpile Discovery (v3.1.1)
///
/// If the project has a `src/` directory with transpilable files (`.ts`, `.tsx`,
/// `.js`, `.jsx`) and appears to be a typical JS project (has `build`, `dev`,
/// `test`, `lint`, or `typecheck` scripts), a `transpile` node is automatically
/// added to the graph.
///
/// The transpile node:
/// - Takes `src/` as input and outputs to `dist/`
/// - Uses sensible defaults: automatic JSX runtime, ESM output, external sourcemaps
/// - Is available via `howth build transpile`
/// - Is NOT added to defaults (scripts take precedence)
///
/// To disable auto-discovery, set `HOWTH_NO_TRANSPILE=1` environment variable.
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
    let jsconfig_input = {
        let jsconfig_path = cwd.join("jsconfig.json");
        if jsconfig_path.exists() {
            Some(BuildInput::file(
                jsconfig_path.to_string_lossy().to_string(),
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

    // Check for automatic typecheck discovery (v3.2)
    let should_add_typecheck = should_auto_discover_typecheck(cwd, &scripts);

    if should_add_typecheck {
        // Create the typecheck node
        let mut node = BuildNode::typecheck();

        // Add config file inputs
        node.add_input(pkg_json_input.clone());
        if let Some(ref ts) = tsconfig_input {
            node.add_input(ts.clone());
        }
        // Add src glob input for hash computation
        node.add_input(BuildInput::glob(
            "src/**/*.{ts,tsx}".to_string(),
            cwd_str.clone(),
        ));

        graph.add_node(node);
    }

    // Check for automatic transpile discovery (v3.1.1)
    let should_add_transpile = should_auto_discover_transpile(cwd, &scripts);

    if should_add_transpile {
        // Create the transpile node
        let spec = TranspileSpec::batch("src", "dist");

        let mut node = BuildNode::transpile_batch(&spec);

        // Add config file inputs
        node.add_input(pkg_json_input.clone());
        if let Some(ref ts) = tsconfig_input {
            node.add_input(ts.clone());
        }
        if let Some(ref js) = jsconfig_input {
            node.add_input(js.clone());
        }
        // Add src glob input for hash computation
        node.add_input(BuildInput::glob(
            "src/**/*.{ts,tsx,js,jsx}".to_string(),
            cwd_str.clone(),
        ));

        graph.add_node(node);
    }

    // Set default targets
    if scripts.contains_key("build") {
        // Script:build is the default when it exists
        graph.add_default("script:build");
    } else if should_add_transpile && scripts.is_empty() {
        // Only make transpile the default if there are no scripts
        graph.add_default("transpile");
    }

    // Ensure we have at least one node (either scripts or transpile)
    if graph.nodes.is_empty() {
        return Err(BuildGraphError::new(
            codes::BUILD_SCRIPT_NOT_FOUND,
            "No scripts found in package.json and no src/ directory for transpilation",
        ));
    }

    graph.normalize();

    Ok(graph)
}

/// Check if automatic typecheck discovery should be enabled (v3.2).
///
/// Returns true if all conditions are met:
/// 1. `tsconfig.json` exists in the project root
/// 2. `package.json` does NOT have a `typecheck` script
/// 3. `src/` directory exists with at least one TypeScript file
/// 4. `HOWTH_NO_TYPECHECK` env var is not set to "1"
fn should_auto_discover_typecheck(cwd: &Path, scripts: &BTreeMap<String, String>) -> bool {
    // Check for opt-out via environment variable
    if std::env::var("HOWTH_NO_TYPECHECK")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return false;
    }

    // Skip if there's already a typecheck script
    if scripts.contains_key("typecheck") {
        return false;
    }

    // Require tsconfig.json
    let tsconfig_path = cwd.join("tsconfig.json");
    if !tsconfig_path.exists() {
        return false;
    }

    // Check for src/ directory with TypeScript files
    let src_dir = cwd.join("src");
    if !src_dir.exists() || !src_dir.is_dir() {
        return false;
    }

    has_typescript_files(&src_dir)
}

/// Check if a directory contains at least one TypeScript file.
///
/// Scans recursively but stops as soon as one file is found.
fn has_typescript_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext.to_lowercase().as_str(), "ts" | "tsx"))
                .unwrap_or(false)
        })
}

/// Check if automatic transpile discovery should be enabled.
///
/// Returns true if all conditions are met:
/// 1. `src/` directory exists with at least one transpilable file
/// 2. `package.json` has at least one signal script (build, dev, test, lint, typecheck)
///    OR no scripts at all (transpile-only project)
/// 3. `HOWTH_NO_TRANSPILE` env var is not set to "1"
fn should_auto_discover_transpile(cwd: &Path, scripts: &BTreeMap<String, String>) -> bool {
    // Check for opt-out via environment variable
    if std::env::var("HOWTH_NO_TRANSPILE")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return false;
    }

    // Check for src/ directory
    let src_dir = cwd.join("src");
    if !src_dir.exists() || !src_dir.is_dir() {
        return false;
    }

    // Check for at least one transpilable file in src/ (stop early once found)
    let has_transpilable = has_transpilable_files(&src_dir);
    if !has_transpilable {
        return false;
    }

    // Check for signal scripts (or empty scripts = transpile-only project)
    if scripts.is_empty() {
        return true;
    }

    scripts
        .keys()
        .any(|name| JS_PROJECT_SIGNAL_SCRIPTS.contains(&name.as_str()))
}

/// Check if a directory contains at least one transpilable file.
///
/// Scans recursively but stops as soon as one file is found.
fn has_transpilable_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .any(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| TRANSPILABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
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
    use serial_test::serial;
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
        let has_lockfile = graph.nodes[0]
            .inputs
            .iter()
            .any(|i| matches!(i, BuildInput::Lockfile { .. }));
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

    // ============================================================
    // v3.1.1 Auto-Discovery Tests
    // ============================================================

    #[test]
    #[serial]
    fn test_build_graph_adds_transpile_node_when_src_has_tsx() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();

        // Create src/ directory with a .tsx file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/App.tsx"),
            "export const App = () => <div>Hello</div>;",
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have script:build and transpile nodes
        assert!(graph.has_node("script:build"));
        assert!(graph.has_node("transpile"));
        assert_eq!(graph.nodes.len(), 2);

        // Default should still be script:build
        assert_eq!(graph.defaults, vec!["script:build"]);
    }

    #[test]
    #[serial]
    fn test_build_graph_does_not_add_transpile_when_no_src_matches() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();

        // No src/ directory
        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should only have script:build
        assert!(graph.has_node("script:build"));
        assert!(!graph.has_node("transpile"));
        assert_eq!(graph.nodes.len(), 1);
    }

    #[test]
    #[serial]
    fn test_build_graph_does_not_add_transpile_when_src_has_no_transpilable_files() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();

        // Create src/ directory with only non-transpilable files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/data.json"), "{}").unwrap();
        std::fs::write(dir.path().join("src/style.css"), "body {}").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should only have script:build
        assert!(graph.has_node("script:build"));
        assert!(!graph.has_node("transpile"));
    }

    #[test]
    #[serial]
    fn test_build_graph_does_not_add_transpile_when_no_signal_scripts() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"custom": "echo custom"}}"#,
        )
        .unwrap();

        // Create src/ directory with .tsx file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/App.tsx"), "const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should only have script:custom (no signal scripts = no auto-transpile)
        assert!(graph.has_node("script:custom"));
        assert!(!graph.has_node("transpile"));
    }

    #[test]
    fn test_build_graph_default_targets_unchanged_when_scripts_present() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "tsc", "test": "jest"}}"#,
        )
        .unwrap();

        // Create src/ with transpilable files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have scripts and transpile
        assert!(graph.has_node("script:build"));
        assert!(graph.has_node("script:test"));
        assert!(graph.has_node("transpile"));

        // Default should be script:build (not transpile)
        assert_eq!(graph.defaults, vec!["script:build"]);
    }

    #[test]
    #[serial]
    fn test_build_graph_transpile_only_project() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "lib", "scripts": {}}"#,
        )
        .unwrap();

        // Create src/ with transpilable files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should only have transpile node
        assert!(graph.has_node("transpile"));
        assert_eq!(graph.nodes.len(), 1);

        // transpile should be the default
        assert_eq!(graph.defaults, vec!["transpile"]);
    }

    #[test]
    #[serial]
    fn test_build_graph_transpile_disabled_by_env_var() {
        // Ensure clean state - remove env var at start AND end
        // (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();

        // Create src/ with transpilable files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/App.tsx"), "const x = 1;").unwrap();

        // Set opt-out env var
        std::env::set_var("HOWTH_NO_TRANSPILE", "1");

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Clean up env var immediately
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        // Should NOT have transpile node
        assert!(graph.has_node("script:build"));
        assert!(!graph.has_node("transpile"));
    }

    #[test]
    #[serial]
    fn test_build_graph_transpile_node_has_correct_inputs() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        // Create src/ with transpilable files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        let transpile_node = graph.nodes.iter().find(|n| n.id == "transpile").unwrap();

        // Should have package.json input
        let has_pkg_json = transpile_node.inputs.iter().any(|i| {
            if let BuildInput::File { path, .. } = i {
                path.contains("package.json")
            } else {
                false
            }
        });
        assert!(
            has_pkg_json,
            "transpile node should have package.json input"
        );

        // Should have tsconfig.json input
        let has_tsconfig = transpile_node.inputs.iter().any(|i| {
            if let BuildInput::File { path, .. } = i {
                path.contains("tsconfig.json")
            } else {
                false
            }
        });
        assert!(
            has_tsconfig,
            "transpile node should have tsconfig.json input"
        );

        // Should have src glob input
        let has_src_glob = transpile_node.inputs.iter().any(|i| {
            if let BuildInput::Glob { pattern, .. } = i {
                pattern.contains("src/")
            } else {
                false
            }
        });
        assert!(has_src_glob, "transpile node should have src glob input");
    }

    #[test]
    #[serial]
    fn test_build_graph_transpile_node_has_batch_spec() {
        // Ensure clean state (handles parallel test pollution)
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo"}}"#,
        )
        .unwrap();

        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        let transpile_node = graph.nodes.iter().find(|n| n.id == "transpile").unwrap();

        // Should have transpile spec in batch mode
        assert!(transpile_node.transpile.is_some());
        let spec = transpile_node.transpile.as_ref().unwrap();
        assert!(spec.is_batch(), "transpile spec should be in batch mode");
        assert_eq!(spec.input_path.to_string_lossy(), "src");
        assert_eq!(spec.output_path.to_string_lossy(), "dist");
    }

    // ============================================================
    // v3.2 Typecheck Auto-Discovery Tests
    // ============================================================

    #[test]
    #[serial]
    fn test_build_graph_adds_typecheck_node_when_tsconfig_exists() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        // Create src/ directory with a .ts file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/index.ts"),
            "export const x: number = 1;",
        )
        .unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have script:build, transpile, and typecheck nodes
        assert!(graph.has_node("script:build"));
        assert!(graph.has_node("typecheck"));
    }

    #[test]
    #[serial]
    fn test_build_graph_does_not_add_typecheck_when_no_tsconfig() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();
        // No tsconfig.json

        // Create src/ directory with a .ts file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should NOT have typecheck node (no tsconfig.json)
        assert!(graph.has_node("script:build"));
        assert!(!graph.has_node("typecheck"));
    }

    #[test]
    #[serial]
    fn test_build_graph_does_not_add_typecheck_when_script_exists() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build", "typecheck": "tsc --noEmit"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        // Create src/ directory with a .ts file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Should have script:typecheck, NOT the auto-discovered typecheck node
        assert!(graph.has_node("script:typecheck"));
        assert!(!graph.has_node("typecheck"));
    }

    #[test]
    #[serial]
    fn test_build_graph_typecheck_disabled_by_env_var() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo build"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        // Create src/ directory with a .ts file
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        // Set opt-out env var
        std::env::set_var("HOWTH_NO_TYPECHECK", "1");

        let graph = build_graph_from_project(dir.path()).unwrap();

        // Clean up env var
        std::env::remove_var("HOWTH_NO_TYPECHECK");

        // Should NOT have typecheck node (disabled)
        assert!(graph.has_node("script:build"));
        assert!(!graph.has_node("typecheck"));
    }

    #[test]
    #[serial]
    fn test_build_graph_typecheck_node_has_correct_inputs() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        // Create src/ with TypeScript files
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        let typecheck_node = graph.nodes.iter().find(|n| n.id == "typecheck").unwrap();

        // Should have package.json input
        let has_pkg_json = typecheck_node.inputs.iter().any(|i| {
            if let BuildInput::File { path, .. } = i {
                path.contains("package.json")
            } else {
                false
            }
        });
        assert!(
            has_pkg_json,
            "typecheck node should have package.json input"
        );

        // Should have tsconfig.json input
        let has_tsconfig = typecheck_node.inputs.iter().any(|i| {
            if let BuildInput::File { path, .. } = i {
                path.contains("tsconfig.json")
            } else {
                false
            }
        });
        assert!(
            has_tsconfig,
            "typecheck node should have tsconfig.json input"
        );

        // Should have src glob input
        let has_src_glob = typecheck_node.inputs.iter().any(|i| {
            if let BuildInput::Glob { pattern, .. } = i {
                pattern.contains("src/")
            } else {
                false
            }
        });
        assert!(has_src_glob, "typecheck node should have src glob input");

        // Command is resolved at execution time (prefer local tsc, fallback to npx --no-install)
        assert!(
            typecheck_node.command.is_none(),
            "typecheck command is resolved at execution time"
        );
    }

    #[test]
    #[serial]
    fn test_build_graph_typecheck_node_has_no_outputs() {
        // Ensure clean state
        std::env::remove_var("HOWTH_NO_TYPECHECK");
        std::env::remove_var("HOWTH_NO_TRANSPILE");

        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test", "scripts": {"build": "echo"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/index.ts"), "export const x = 1;").unwrap();

        let graph = build_graph_from_project(dir.path()).unwrap();
        let typecheck_node = graph.nodes.iter().find(|n| n.id == "typecheck").unwrap();

        // Typecheck should have no outputs (validation only)
        assert!(
            typecheck_node.outputs.is_empty(),
            "typecheck node should have no outputs"
        );
    }
}
