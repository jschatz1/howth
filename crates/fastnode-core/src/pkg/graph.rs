//! Package dependency graph construction.
//!
//! Provides read-only scanning of an existing `node_modules/` tree
//! to build a dependency graph from installed packages.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::resolver::PkgJsonCache;

/// Schema version for package graph output.
pub const PKG_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Graph error codes.
pub mod codes {
    pub const PKG_GRAPH_NODE_MODULES_NOT_FOUND: &str = "PKG_GRAPH_NODE_MODULES_NOT_FOUND";
    pub const PKG_GRAPH_PACKAGE_JSON_INVALID: &str = "PKG_GRAPH_PACKAGE_JSON_INVALID";
    pub const PKG_GRAPH_PACKAGE_JSON_MISSING: &str = "PKG_GRAPH_PACKAGE_JSON_MISSING";
    pub const PKG_GRAPH_IO_ERROR: &str = "PKG_GRAPH_IO_ERROR";
    pub const PKG_GRAPH_DEPTH_LIMIT_REACHED: &str = "PKG_GRAPH_DEPTH_LIMIT_REACHED";
}

/// Unique identifier for an installed package.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    /// Package name (e.g., "react" or "@types/node").
    pub name: String,
    /// Package version (e.g., "18.2.0").
    pub version: String,
    /// Absolute path to the package root directory.
    pub path: String,
    /// Optional integrity hash (not used in v1.4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
}

impl PackageId {
    /// Create a new package ID.
    #[must_use]
    pub fn new(name: String, version: String, path: String) -> Self {
        Self {
            name,
            version,
            path,
            integrity: None,
        }
    }

    /// Get a sort key for deterministic ordering.
    fn sort_key(&self) -> (&str, &str, &str) {
        (&self.name, &self.version, &self.path)
    }
}

/// A dependency edge in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEdge {
    /// Dependency name as specified in package.json.
    pub name: String,
    /// Version range from package.json (if present and valid string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub req: Option<String>,
    /// Resolved installed target if found in `node_modules`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<PackageId>,
    /// Dependency kind: "dep", "dev", "optional", or "peer".
    pub kind: String,
}

impl DepEdge {
    /// Create a new dependency edge.
    #[must_use]
    pub fn new(name: String, req: Option<String>, to: Option<PackageId>, kind: &str) -> Self {
        Self {
            name,
            req,
            to,
            kind: kind.to_string(),
        }
    }
}

/// A node in the package graph representing an installed package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageNode {
    /// Package identifier.
    pub id: PackageId,
    /// Dependencies as an adjacency list (sorted by name).
    pub dependencies: Vec<DepEdge>,
}

impl PackageNode {
    /// Create a new package node.
    #[must_use]
    pub fn new(id: PackageId, dependencies: Vec<DepEdge>) -> Self {
        Self { id, dependencies }
    }
}

/// Error information for graph construction issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphErrorInfo {
    /// Stable error code.
    pub code: String,
    /// Path where the error occurred.
    pub path: String,
    /// Human-readable error message.
    pub message: String,
}

impl GraphErrorInfo {
    /// Create a new graph error.
    #[must_use]
    pub fn new(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }

    /// Get a sort key for deterministic ordering.
    fn sort_key(&self) -> (&str, &str) {
        (&self.code, &self.path)
    }
}

/// The complete package dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageGraph {
    /// Schema version for this output format.
    pub schema_version: u32,
    /// Absolute path to the project root.
    pub root: String,
    /// All package nodes in the graph (sorted deterministically).
    pub nodes: Vec<PackageNode>,
    /// Packages in `node_modules` not reachable from root deps (sorted).
    pub orphans: Vec<PackageId>,
    /// Errors encountered during graph construction (sorted).
    pub errors: Vec<GraphErrorInfo>,
}

impl PackageGraph {
    /// Create an empty graph for the given root.
    #[must_use]
    pub fn empty(root: String) -> Self {
        Self {
            schema_version: PKG_GRAPH_SCHEMA_VERSION,
            root,
            nodes: Vec::new(),
            orphans: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// Options for graph construction.
#[derive(Debug, Clone)]
pub struct GraphOptions {
    /// Maximum traversal depth (default 25).
    pub max_depth: usize,
    /// Include optionalDependencies (default true).
    pub include_optional: bool,
    /// Include root devDependencies (default false).
    pub include_dev_root: bool,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            max_depth: 25,
            include_optional: true,
            include_dev_root: false,
        }
    }
}

/// Internal state for tracking indexed packages.
struct PackageIndex {
    /// All indexed packages by path.
    by_path: HashMap<PathBuf, PackageId>,
    /// Packages indexed by name for resolution.
    by_name: HashMap<String, Vec<PackageId>>,
}

impl PackageIndex {
    fn new() -> Self {
        Self {
            by_path: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    fn insert(&mut self, id: PackageId, path: PathBuf) {
        self.by_name
            .entry(id.name.clone())
            .or_default()
            .push(id.clone());
        self.by_path.insert(path, id);
    }

    fn get_by_path(&self, path: &Path) -> Option<&PackageId> {
        self.by_path.get(path)
    }

    fn all_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.by_path.keys()
    }
}

/// Build a package dependency graph from an installed `node_modules` tree.
///
/// # Arguments
/// * `cwd` - The project root directory (must be absolute).
/// * `opts` - Graph construction options.
/// * `cache` - Package.json cache for efficient parsing.
///
/// # Returns
/// A `PackageGraph` containing all reachable packages and any errors.
pub fn build_pkg_graph(cwd: &Path, opts: &GraphOptions, cache: &dyn PkgJsonCache) -> PackageGraph {
    let root_str = cwd.to_string_lossy().to_string();
    let mut graph = PackageGraph::empty(root_str.clone());

    let node_modules = cwd.join("node_modules");
    if !node_modules.exists() {
        graph.errors.push(GraphErrorInfo::new(
            codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND,
            node_modules.to_string_lossy(),
            "node_modules directory not found",
        ));
        return graph;
    }

    // Phase A: Index all installed packages
    let mut index = PackageIndex::new();
    let mut errors = Vec::new();
    index_node_modules(&node_modules, &mut index, &mut errors, cache);

    // Phase B: Traverse from root dependencies
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut nodes: Vec<PackageNode> = Vec::new();

    // Read root package.json
    let root_pkg_json = cwd.join("package.json");
    let root_deps = read_root_dependencies(&root_pkg_json, opts, &mut errors, cache);

    // BFS traversal
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();

    // Queue root dependencies
    for (dep_name, _) in &root_deps {
        if let Some(pkg_path) = find_package_in_node_modules(&node_modules, dep_name) {
            if !visited.contains(&pkg_path) {
                visited.insert(pkg_path.clone());
                queue.push_back((pkg_path, 1));
            }
        }
    }

    // Process queue
    while let Some((pkg_path, depth)) = queue.pop_front() {
        if depth > opts.max_depth {
            errors.push(GraphErrorInfo::new(
                codes::PKG_GRAPH_DEPTH_LIMIT_REACHED,
                pkg_path.to_string_lossy(),
                format!("Depth limit {} reached", opts.max_depth),
            ));
            continue;
        }

        let Some(pkg_id) = index.get_by_path(&pkg_path).cloned() else {
            continue;
        };

        // Read this package's dependencies
        let pkg_json_path = pkg_path.join("package.json");
        let pkg_deps = read_package_dependencies(&pkg_json_path, opts, &mut errors, cache);

        // Build edges
        let mut edges: Vec<DepEdge> = Vec::new();

        for (dep_name, dep_range, kind) in pkg_deps {
            let target = find_package_in_node_modules(&node_modules, &dep_name)
                .and_then(|p| index.get_by_path(&p).cloned());

            // Queue unvisited targets
            if let Some(ref target_id) = target {
                let target_path = PathBuf::from(&target_id.path);
                if !visited.contains(&target_path) {
                    visited.insert(target_path.clone());
                    queue.push_back((target_path, depth + 1));
                }
            }

            edges.push(DepEdge::new(dep_name, dep_range, target, &kind));
        }

        // Sort edges by name for determinism
        edges.sort_by(|a, b| a.name.cmp(&b.name));

        nodes.push(PackageNode::new(pkg_id, edges));
    }

    // Sort nodes by (name, version, path) for determinism
    nodes.sort_by(|a, b| a.id.sort_key().cmp(&b.id.sort_key()));

    // Find orphans: packages in index not visited
    let mut orphans: Vec<PackageId> = index
        .all_paths()
        .filter(|p| !visited.contains(*p))
        .filter_map(|p| index.get_by_path(p).cloned())
        .collect();
    orphans.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    // Sort errors
    errors.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    graph.nodes = nodes;
    graph.orphans = orphans;
    graph.errors = errors;

    graph
}

/// Index all packages in `node_modules` directory.
fn index_node_modules(
    node_modules: &Path,
    index: &mut PackageIndex,
    errors: &mut Vec<GraphErrorInfo>,
    cache: &dyn PkgJsonCache,
) {
    let entries = match fs::read_dir(node_modules) {
        Ok(e) => e,
        Err(e) => {
            errors.push(GraphErrorInfo::new(
                codes::PKG_GRAPH_IO_ERROR,
                node_modules.to_string_lossy(),
                format!("Failed to read node_modules: {e}"),
            ));
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Skip .bin and hidden directories
        if name_str == ".bin" || name_str.starts_with('.') {
            continue;
        }

        if !path.is_dir() {
            continue;
        }

        // Check for scoped package
        if name_str.starts_with('@') {
            // Scan scope directory
            if let Ok(scope_entries) = fs::read_dir(&path) {
                for scope_entry in scope_entries.flatten() {
                    let scope_path = scope_entry.path();
                    if scope_path.is_dir() {
                        let scoped_name =
                            format!("{}/{}", name_str, scope_entry.file_name().to_string_lossy());
                        index_single_package(&scope_path, &scoped_name, index, errors, cache);
                    }
                }
            }
        } else {
            // Regular package
            index_single_package(&path, &name_str, index, errors, cache);
        }
    }
}

/// Index a single package from its directory.
fn index_single_package(
    pkg_path: &Path,
    expected_name: &str,
    index: &mut PackageIndex,
    errors: &mut Vec<GraphErrorInfo>,
    cache: &dyn PkgJsonCache,
) {
    let pkg_json_path = pkg_path.join("package.json");

    if !pkg_json_path.exists() {
        errors.push(GraphErrorInfo::new(
            codes::PKG_GRAPH_PACKAGE_JSON_MISSING,
            pkg_path.to_string_lossy(),
            format!("No package.json found in {expected_name}"),
        ));
        return;
    }

    // Try to get from cache first
    let pkg_json = if let Some(cached) = cache.get(&pkg_json_path) {
        cached
    } else {
        // Read and parse manually
        match fs::read_to_string(&pkg_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => {
                    cache.set(&pkg_json_path, json.clone());
                    json
                }
                Err(e) => {
                    errors.push(GraphErrorInfo::new(
                        codes::PKG_GRAPH_PACKAGE_JSON_INVALID,
                        pkg_json_path.to_string_lossy(),
                        format!("Invalid JSON: {e}"),
                    ));
                    return;
                }
            },
            Err(e) => {
                errors.push(GraphErrorInfo::new(
                    codes::PKG_GRAPH_IO_ERROR,
                    pkg_json_path.to_string_lossy(),
                    format!("Failed to read: {e}"),
                ));
                return;
            }
        }
    };

    // Extract name and version
    let name = pkg_json
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from);
    let version = pkg_json
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    match (name, version) {
        (Some(name), Some(version)) => {
            let id = PackageId::new(name, version, pkg_path.to_string_lossy().to_string());
            index.insert(id, pkg_path.to_path_buf());
        }
        _ => {
            errors.push(GraphErrorInfo::new(
                codes::PKG_GRAPH_PACKAGE_JSON_INVALID,
                pkg_json_path.to_string_lossy(),
                "Missing name or version field",
            ));
        }
    }
}

/// Read root project dependencies based on options.
fn read_root_dependencies(
    pkg_json_path: &Path,
    opts: &GraphOptions,
    errors: &mut Vec<GraphErrorInfo>,
    cache: &dyn PkgJsonCache,
) -> Vec<(String, Option<String>)> {
    let mut deps = Vec::new();

    if !pkg_json_path.exists() {
        errors.push(GraphErrorInfo::new(
            codes::PKG_GRAPH_PACKAGE_JSON_MISSING,
            pkg_json_path.to_string_lossy(),
            "Root package.json not found",
        ));
        return deps;
    }

    let pkg_json = if let Some(cached) = cache.get(pkg_json_path) {
        cached
    } else {
        match fs::read_to_string(pkg_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => {
                    cache.set(pkg_json_path, json.clone());
                    json
                }
                Err(e) => {
                    errors.push(GraphErrorInfo::new(
                        codes::PKG_GRAPH_PACKAGE_JSON_INVALID,
                        pkg_json_path.to_string_lossy(),
                        format!("Invalid JSON: {e}"),
                    ));
                    return deps;
                }
            },
            Err(e) => {
                errors.push(GraphErrorInfo::new(
                    codes::PKG_GRAPH_IO_ERROR,
                    pkg_json_path.to_string_lossy(),
                    format!("Failed to read: {e}"),
                ));
                return deps;
            }
        }
    };

    // Extract dependencies
    extract_deps_from_json(&pkg_json, "dependencies", &mut deps);

    if opts.include_dev_root {
        extract_deps_from_json(&pkg_json, "devDependencies", &mut deps);
    }

    if opts.include_optional {
        extract_deps_from_json(&pkg_json, "optionalDependencies", &mut deps);
    }

    // Sort for determinism
    deps.sort_by(|a, b| a.0.cmp(&b.0));
    deps
}

/// Read package dependencies (not root).
fn read_package_dependencies(
    pkg_json_path: &Path,
    opts: &GraphOptions,
    _errors: &mut Vec<GraphErrorInfo>,
    cache: &dyn PkgJsonCache,
) -> Vec<(String, Option<String>, String)> {
    let mut deps = Vec::new();

    let pkg_json = if let Some(cached) = cache.get(pkg_json_path) {
        cached
    } else {
        match fs::read_to_string(pkg_json_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(json) => {
                    cache.set(pkg_json_path, json.clone());
                    json
                }
                Err(_) => return deps,
            },
            Err(_) => return deps,
        }
    };

    // Extract dependencies with kind
    extract_deps_with_kind(&pkg_json, "dependencies", "dep", &mut deps);

    if opts.include_optional {
        extract_deps_with_kind(&pkg_json, "optionalDependencies", "optional", &mut deps);
    }

    // Sort for determinism
    deps.sort_by(|a, b| a.0.cmp(&b.0));
    deps
}

/// Extract dependencies from a JSON object section.
fn extract_deps_from_json(
    pkg_json: &serde_json::Value,
    section: &str,
    deps: &mut Vec<(String, Option<String>)>,
) {
    if let Some(obj) = pkg_json.get(section).and_then(|v| v.as_object()) {
        for (name, range) in obj {
            let range_str = range.as_str().map(String::from);
            deps.push((name.clone(), range_str));
        }
    }
}

/// Extract dependencies with kind annotation.
fn extract_deps_with_kind(
    pkg_json: &serde_json::Value,
    section: &str,
    kind: &str,
    deps: &mut Vec<(String, Option<String>, String)>,
) {
    if let Some(obj) = pkg_json.get(section).and_then(|v| v.as_object()) {
        for (name, range) in obj {
            let range_str = range.as_str().map(String::from);
            deps.push((name.clone(), range_str, kind.to_string()));
        }
    }
}

/// Find a package path in `node_modules` by name.
fn find_package_in_node_modules(node_modules: &Path, name: &str) -> Option<PathBuf> {
    let pkg_path = if name.starts_with('@') {
        // Scoped package
        let parts: Vec<&str> = name.splitn(2, '/').collect();
        if parts.len() == 2 {
            node_modules.join(parts[0]).join(parts[1])
        } else {
            node_modules.join(name)
        }
    } else {
        node_modules.join(name)
    };

    if pkg_path.exists() && pkg_path.is_dir() {
        Some(pkg_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::NoPkgJsonCache;
    use std::fs;
    use tempfile::tempdir;

    fn create_package_json(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
        let mut package_json = serde_json::json!({
            "name": name,
            "version": version
        });

        if !deps.is_empty() {
            let deps_obj: serde_json::Map<String, serde_json::Value> = deps
                .iter()
                .map(|(n, v)| (n.to_string(), serde_json::json!(v)))
                .collect();
            package_json["dependencies"] = serde_json::Value::Object(deps_obj);
        }

        fs::write(
            dir.join("package.json"),
            serde_json::to_string_pretty(&package_json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn test_simple_graph() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json
        create_package_json(root, "my-project", "1.0.0", &[("a", "^1.0.0")]);

        // Create node_modules/a
        let a_dir = root.join("node_modules/a");
        fs::create_dir_all(&a_dir).unwrap();
        create_package_json(&a_dir, "a", "1.0.0", &[("b", "^1.0.0")]);

        // Create node_modules/b
        let b_dir = root.join("node_modules/b");
        fs::create_dir_all(&b_dir).unwrap();
        create_package_json(&b_dir, "b", "1.0.0", &[]);

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        assert_eq!(graph.schema_version, PKG_GRAPH_SCHEMA_VERSION);
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.errors.is_empty());

        // Find node a
        let node_a = graph.nodes.iter().find(|n| n.id.name == "a").unwrap();
        assert_eq!(node_a.id.version, "1.0.0");
        assert_eq!(node_a.dependencies.len(), 1);
        assert_eq!(node_a.dependencies[0].name, "b");
        assert!(node_a.dependencies[0].to.is_some());
    }

    #[test]
    fn test_scoped_package() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json
        create_package_json(root, "my-project", "1.0.0", &[("@scope/pkg", "^1.0.0")]);

        // Create node_modules/@scope/pkg
        let pkg_dir = root.join("node_modules/@scope/pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        create_package_json(&pkg_dir, "@scope/pkg", "1.0.0", &[]);

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id.name, "@scope/pkg");
    }

    #[test]
    fn test_deterministic_ordering() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json with deps in random order
        create_package_json(
            root,
            "my-project",
            "1.0.0",
            &[("zebra", "1.0.0"), ("alpha", "1.0.0"), ("beta", "1.0.0")],
        );

        // Create packages
        for name in ["zebra", "alpha", "beta"] {
            let pkg_dir = root.join(format!("node_modules/{name}"));
            fs::create_dir_all(&pkg_dir).unwrap();
            create_package_json(&pkg_dir, name, "1.0.0", &[]);
        }

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        // Nodes should be sorted alphabetically
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].id.name, "alpha");
        assert_eq!(graph.nodes[1].id.name, "beta");
        assert_eq!(graph.nodes[2].id.name, "zebra");
    }

    #[test]
    fn test_orphan_detection() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json with only one dep
        create_package_json(root, "my-project", "1.0.0", &[("a", "1.0.0")]);

        // Create node_modules/a (reachable)
        let a_dir = root.join("node_modules/a");
        fs::create_dir_all(&a_dir).unwrap();
        create_package_json(&a_dir, "a", "1.0.0", &[]);

        // Create node_modules/orphan (not reachable)
        let orphan_dir = root.join("node_modules/orphan");
        fs::create_dir_all(&orphan_dir).unwrap();
        create_package_json(&orphan_dir, "orphan", "1.0.0", &[]);

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.orphans.len(), 1);
        assert_eq!(graph.orphans[0].name, "orphan");
    }

    #[test]
    fn test_missing_node_modules() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json only
        create_package_json(root, "my-project", "1.0.0", &[]);

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        assert!(graph.nodes.is_empty());
        assert_eq!(graph.errors.len(), 1);
        assert_eq!(
            graph.errors[0].code,
            codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND
        );
    }

    #[test]
    fn test_invalid_package_json() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json
        create_package_json(root, "my-project", "1.0.0", &[("a", "1.0.0")]);

        // Create node_modules/a with invalid package.json
        let a_dir = root.join("node_modules/a");
        fs::create_dir_all(&a_dir).unwrap();
        fs::write(a_dir.join("package.json"), "not valid json {{{").unwrap();

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        // a should not be indexed due to invalid JSON
        assert!(graph.nodes.is_empty());
        assert!(!graph.errors.is_empty());
        assert!(graph
            .errors
            .iter()
            .any(|e| e.code == codes::PKG_GRAPH_PACKAGE_JSON_INVALID));
    }

    #[test]
    fn test_include_dev_dependencies() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json with dev dep
        let pkg_json = serde_json::json!({
            "name": "my-project",
            "version": "1.0.0",
            "dependencies": { "a": "1.0.0" },
            "devDependencies": { "dev-pkg": "1.0.0" }
        });
        fs::write(
            root.join("package.json"),
            serde_json::to_string_pretty(&pkg_json).unwrap(),
        )
        .unwrap();

        // Create packages
        for name in ["a", "dev-pkg"] {
            let pkg_dir = root.join(format!("node_modules/{name}"));
            fs::create_dir_all(&pkg_dir).unwrap();
            create_package_json(&pkg_dir, name, "1.0.0", &[]);
        }

        // Without dev deps
        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id.name, "a");
        assert_eq!(graph.orphans.len(), 1);
        assert_eq!(graph.orphans[0].name, "dev-pkg");

        // With dev deps
        let opts_with_dev = GraphOptions {
            include_dev_root: true,
            ..Default::default()
        };
        let graph_with_dev = build_pkg_graph(root, &opts_with_dev, &cache);
        assert_eq!(graph_with_dev.nodes.len(), 2);
        assert!(graph_with_dev.orphans.is_empty());
    }

    #[test]
    fn test_depth_limit() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create a chain: a -> b -> c
        create_package_json(root, "my-project", "1.0.0", &[("a", "1.0.0")]);

        let a_dir = root.join("node_modules/a");
        fs::create_dir_all(&a_dir).unwrap();
        create_package_json(&a_dir, "a", "1.0.0", &[("b", "1.0.0")]);

        let b_dir = root.join("node_modules/b");
        fs::create_dir_all(&b_dir).unwrap();
        create_package_json(&b_dir, "b", "1.0.0", &[("c", "1.0.0")]);

        let c_dir = root.join("node_modules/c");
        fs::create_dir_all(&c_dir).unwrap();
        create_package_json(&c_dir, "c", "1.0.0", &[]);

        // With depth limit of 2
        let cache = NoPkgJsonCache;
        let opts = GraphOptions {
            max_depth: 2,
            ..Default::default()
        };
        let graph = build_pkg_graph(root, &opts, &cache);

        // Should only reach a and b (depth 1 and 2)
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph
            .errors
            .iter()
            .any(|e| e.code == codes::PKG_GRAPH_DEPTH_LIMIT_REACHED));
    }

    #[test]
    fn test_unresolved_dependency() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create root package.json with dep that doesn't exist
        create_package_json(root, "my-project", "1.0.0", &[("a", "1.0.0")]);

        // Create a that depends on missing-pkg
        let a_dir = root.join("node_modules/a");
        fs::create_dir_all(&a_dir).unwrap();
        create_package_json(&a_dir, "a", "1.0.0", &[("missing-pkg", "1.0.0")]);

        let cache = NoPkgJsonCache;
        let opts = GraphOptions::default();
        let graph = build_pkg_graph(root, &opts, &cache);

        assert_eq!(graph.nodes.len(), 1);
        let node_a = &graph.nodes[0];
        assert_eq!(node_a.dependencies.len(), 1);
        assert_eq!(node_a.dependencies[0].name, "missing-pkg");
        assert!(node_a.dependencies[0].to.is_none()); // Unresolved
    }
}
