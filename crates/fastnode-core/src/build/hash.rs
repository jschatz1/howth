//! Deterministic hashing for the build system.
//!
//! Uses blake3 for fast, cryptographic hashing.
//! All hashing is deterministic across platforms.
//!
//! ## Hashing Rules (v1)
//!
//! - Paths are normalized to forward slashes
//! - Files are hashed by content
//! - Globs expand deterministically (sorted by path)
//! - Environment variables are hashed by allowlist only

use super::graph::{BuildInput, BuildNode, DEFAULT_GLOB_EXCLUSIONS};
use blake3::Hasher;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Result type for hashing operations.
pub type HashResult<T> = Result<T, HashError>;

/// Error during hashing.
#[derive(Debug)]
pub struct HashError {
    /// Error code.
    pub code: &'static str,
    /// Error message.
    pub message: String,
    /// Path that caused the error (if applicable).
    pub path: Option<PathBuf>,
}

impl HashError {
    /// Create a new hash error.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
        }
    }

    /// Create with path.
    #[must_use]
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Create an I/O error.
    #[must_use]
    pub fn io(path: &Path, err: io::Error) -> Self {
        Self {
            code: super::codes::BUILD_HASH_IO_ERROR,
            message: err.to_string(),
            path: Some(path.to_path_buf()),
        }
    }
}

impl std::fmt::Display for HashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}: {} ({})", self.code, self.message, path.display())
        } else {
            write!(f, "{}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for HashError {}

/// Normalize a path for hashing.
///
/// - Converts to absolute path (if not already)
/// - Normalizes separators to forward slashes
/// - Strips trailing slashes
/// - Keeps case as-is (no lowercasing)
#[must_use]
pub fn normalize_path(path: &Path) -> String {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(path)
    };

    let mut normalized = abs_path
        .to_string_lossy()
        .replace('\\', "/");

    // Strip trailing slash
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }

    normalized
}

/// Hash a file by its contents.
pub fn hash_file(path: &Path) -> HashResult<String> {
    let contents = fs::read(path).map_err(|e| HashError::io(path, e))?;
    Ok(hash_bytes(&contents))
}

/// Hash raw bytes.
#[must_use]
pub fn hash_bytes(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    hash.to_hex().to_string()
}

/// Hash a string.
#[must_use]
pub fn hash_string(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

/// Expand a glob pattern and return sorted file paths.
///
/// Files are sorted by normalized path for determinism.
pub fn expand_glob(pattern: &str, root: &Path, exclusions: &[&str]) -> HashResult<Vec<PathBuf>> {
    let mut files = Vec::new();

    // Walk the directory
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Check exclusions
            let path = e.path();
            let rel = path.strip_prefix(root).unwrap_or(path);
            let rel_str = rel.to_string_lossy();

            // Skip excluded patterns
            for exclusion in exclusions {
                if let Some(prefix) = exclusion.strip_suffix("/**") {
                    if rel_str.starts_with(prefix) || rel_str == prefix.trim_end_matches('/') {
                        return false;
                    }
                } else if glob::Pattern::new(exclusion)
                    .map(|p| p.matches(&rel_str))
                    .unwrap_or(false)
                {
                    return false;
                }
            }
            true
        })
    {
        let entry = entry.map_err(|e| {
            HashError::new(
                super::codes::BUILD_HASH_IO_ERROR,
                format!("Failed to walk directory: {e}"),
            )
        })?;

        if entry.file_type().is_file() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(path);
            let rel_str = rel.to_string_lossy();

            // Check if matches pattern
            let matches = if pattern == "**/*" {
                true
            } else {
                glob::Pattern::new(pattern)
                    .map(|p| p.matches(&rel_str))
                    .unwrap_or(false)
            };

            if matches {
                files.push(path.to_path_buf());
            }
        }
    }

    // Sort by normalized path for determinism
    files.sort_by(|a, b| {
        let a_norm = normalize_path(a);
        let b_norm = normalize_path(b);
        a_norm.cmp(&b_norm)
    });

    Ok(files)
}

/// Hash all files matched by a glob pattern.
pub fn hash_glob(pattern: &str, root: &Path, exclusions: &[&str]) -> HashResult<String> {
    let files = expand_glob(pattern, root, exclusions)?;

    let mut hasher = Hasher::new();

    for file in &files {
        let normalized = normalize_path(file);
        hasher.update(normalized.as_bytes());
        hasher.update(b"\0");

        match fs::read(file) {
            Ok(contents) => {
                hasher.update(&contents);
            }
            Err(_) => {
                // File missing - include marker
                hasher.update(b"<missing>");
            }
        }
        hasher.update(b"\0");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Hash environment variables from allowlist.
#[must_use]
pub fn hash_env(allowlist: &[String]) -> String {
    let mut hasher = Hasher::new();

    // Sort allowlist for determinism
    let mut sorted: Vec<_> = allowlist.iter().collect();
    sorted.sort();

    for key in sorted {
        let value = std::env::var(key).unwrap_or_default();
        hasher.update(key.as_bytes());
        hasher.update(b"=");
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }

    hasher.finalize().to_hex().to_string()
}

/// Canonical encoding for a build input.
fn encode_input(input: &BuildInput) -> Vec<u8> {
    let mut buf = Vec::new();

    match input {
        BuildInput::File { path, optional } => {
            buf.extend_from_slice(b"file\0");
            buf.extend_from_slice(path.as_bytes());
            buf.push(0);
            buf.extend_from_slice(if *optional { b"optional" } else { b"required" });
            buf.push(0);
        }
        BuildInput::Glob { pattern, root, optional } => {
            buf.extend_from_slice(b"glob\0");
            buf.extend_from_slice(pattern.as_bytes());
            buf.push(0);
            buf.extend_from_slice(root.as_bytes());
            buf.push(0);
            buf.extend_from_slice(if *optional { b"optional" } else { b"required" });
            buf.push(0);
        }
        BuildInput::Dir { path, optional } => {
            buf.extend_from_slice(b"dir\0");
            buf.extend_from_slice(path.as_bytes());
            buf.push(0);
            buf.extend_from_slice(if *optional { b"optional" } else { b"required" });
            buf.push(0);
        }
        BuildInput::Package { name, version } => {
            buf.extend_from_slice(b"package\0");
            buf.extend_from_slice(name.as_bytes());
            buf.push(0);
            buf.extend_from_slice(version.as_deref().unwrap_or("unknown").as_bytes());
            buf.push(0);
        }
        BuildInput::Lockfile { path, schema_version } => {
            buf.extend_from_slice(b"lockfile\0");
            buf.extend_from_slice(path.as_bytes());
            buf.push(0);
            buf.extend_from_slice(schema_version.to_string().as_bytes());
            buf.push(0);
        }
        BuildInput::Env { key } => {
            buf.extend_from_slice(b"env\0");
            buf.extend_from_slice(key.as_bytes());
            buf.push(0);
        }
        BuildInput::Node { id } => {
            buf.extend_from_slice(b"node\0");
            buf.extend_from_slice(id.as_bytes());
            buf.push(0);
        }
    }

    buf
}

/// Hash a single build input.
pub fn hash_input(input: &BuildInput, cwd: &Path) -> HashResult<String> {
    match input {
        BuildInput::File { path, optional } => {
            let full_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                cwd.join(path)
            };

            if full_path.exists() {
                hash_file(&full_path)
            } else if *optional {
                // Optional missing file - stable marker
                Ok(hash_string(&format!("optional-missing:{}", normalize_path(&full_path))))
            } else {
                // Required missing file - include marker in hash
                Ok(hash_string(&format!("missing:{}", normalize_path(&full_path))))
            }
        }
        BuildInput::Glob { pattern, root, optional } => {
            let root_path = if Path::new(root).is_absolute() {
                PathBuf::from(root)
            } else {
                cwd.join(root)
            };

            if !root_path.exists() && *optional {
                // Optional glob with missing root - stable marker
                return Ok(hash_string(&format!("optional-missing-glob:{}", normalize_path(&root_path))));
            }

            hash_glob(pattern, &root_path, DEFAULT_GLOB_EXCLUSIONS)
        }
        BuildInput::Dir { path, optional } => {
            let full_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                cwd.join(path)
            };

            if full_path.exists() && full_path.is_dir() {
                // Hash directory contents as glob
                hash_glob("**/*", &full_path, DEFAULT_GLOB_EXCLUSIONS)
            } else if *optional {
                Ok(hash_string(&format!("optional-missing-dir:{}", normalize_path(&full_path))))
            } else {
                Ok(hash_string(&format!("missing-dir:{}", normalize_path(&full_path))))
            }
        }
        BuildInput::Package { name, version } => {
            Ok(hash_string(&format!(
                "package:{}:{}",
                name,
                version.as_deref().unwrap_or("unknown")
            )))
        }
        BuildInput::Lockfile { path, .. } => {
            let full_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                cwd.join(path)
            };

            if full_path.exists() {
                hash_file(&full_path)
            } else {
                Ok(hash_string("lockfile:missing"))
            }
        }
        BuildInput::Env { key } => {
            let value = std::env::var(key).unwrap_or_default();
            Ok(hash_string(&format!("env:{}={}", key, value)))
        }
        BuildInput::Node { id } => {
            // Node inputs are handled separately in hash_input_with_deps
            // This fallback is for backwards compatibility
            Ok(hash_string(&format!("node:{}", id)))
        }
    }
}

/// Hash a single build input with dependency hash resolution.
///
/// For `BuildInput::Node` inputs, looks up the actual hash of the dependency
/// from the provided map. This enables cache invalidation propagation.
pub fn hash_input_with_deps(
    input: &BuildInput,
    cwd: &Path,
    dep_hashes: &BTreeMap<String, String>,
) -> HashResult<String> {
    match input {
        BuildInput::Node { id } => {
            // Look up the actual hash of the dependency node
            if let Some(dep_hash) = dep_hashes.get(id) {
                Ok(hash_string(&format!("dep:{}:{}", id, dep_hash)))
            } else {
                // Dependency not yet computed - use placeholder
                Ok(hash_string(&format!("node:{}", id)))
            }
        }
        // All other inputs delegate to the base function
        other => hash_input(other, cwd),
    }
}

/// Compute the hash for a build node (without dependency hash inclusion).
///
/// The hash is computed from:
/// - Schema version
/// - Node kind
/// - Node label
/// - All input hashes
/// - Environment hash
/// - Script specification
/// - Dependencies
///
/// For dep-hash inclusion (v2.1), use `hash_node_with_deps` instead.
pub fn hash_node(node: &BuildNode, cwd: &Path) -> HashResult<String> {
    hash_node_with_deps(node, cwd, &BTreeMap::new())
}

/// Compute the hash for a build node with dependency hash inclusion (v2.1).
///
/// For `BuildInput::Node` inputs, substitutes the actual hash of the dependency
/// from the provided map. This enables cache invalidation to propagate through
/// the DAG - when a dependency changes, all dependents also get new hashes.
///
/// The `dep_hashes` map should contain the already-computed hashes of all
/// dependency nodes. Compute hashes in topological order to build this map.
pub fn hash_node_with_deps(
    node: &BuildNode,
    cwd: &Path,
    dep_hashes: &BTreeMap<String, String>,
) -> HashResult<String> {
    let mut hasher = Hasher::new();

    // Schema version
    hasher.update(b"schema:");
    hasher.update(super::graph::BUILD_GRAPH_SCHEMA_VERSION.to_string().as_bytes());
    hasher.update(b"\0");

    // Kind
    hasher.update(b"kind:");
    hasher.update(node.kind.as_str().as_bytes());
    hasher.update(b"\0");

    // Label
    hasher.update(b"label:");
    hasher.update(node.label.as_bytes());
    hasher.update(b"\0");

    // Inputs (sorted by canonical encoding)
    // Use hash_input_with_deps to include dependency hashes
    let mut input_hashes: Vec<(Vec<u8>, String)> = Vec::new();
    for input in &node.inputs {
        let encoded = encode_input(input);
        let hash = hash_input_with_deps(input, cwd, dep_hashes)?;
        input_hashes.push((encoded, hash));
    }
    // Sort by encoding for determinism
    input_hashes.sort_by(|a, b| a.0.cmp(&b.0));

    hasher.update(b"inputs:");
    for (encoded, hash) in &input_hashes {
        hasher.update(encoded);
        hasher.update(hash.as_bytes());
        hasher.update(b"\0");
    }

    // Environment
    hasher.update(b"env:");
    hasher.update(hash_env(&node.env_allowlist).as_bytes());
    hasher.update(b"\0");

    // Script
    if let Some(script) = &node.script {
        hasher.update(b"script:");
        hasher.update(script.name.as_bytes());
        hasher.update(b"\0");
        hasher.update(script.command.as_bytes());
        hasher.update(b"\0");
        hasher.update(if script.shell { b"shell" } else { b"noshell" });
        hasher.update(b"\0");
    }

    // Dependencies (sorted) - include dep hashes for additional invalidation
    let mut deps = node.deps.clone();
    deps.sort();
    hasher.update(b"deps:");
    for dep in &deps {
        hasher.update(dep.as_bytes());
        hasher.update(b":");
        // Include the dependency's hash if available
        if let Some(dep_hash) = dep_hashes.get(dep) {
            hasher.update(dep_hash.as_bytes());
        }
        hasher.update(b"\0");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute hashes for all nodes in a graph (v2.1 with dep-hash inclusion).
///
/// Computes hashes in topological order so that dependency hashes are
/// available when computing each node's hash. This ensures cache invalidation
/// propagates correctly through the DAG.
pub fn hash_graph(graph: &super::graph::BuildGraph) -> HashResult<BTreeMap<String, String>> {
    let cwd = Path::new(&graph.cwd);
    let mut hashes = BTreeMap::new();

    // Get nodes in topological order (dependencies first)
    let sorted_ids = graph.toposort();

    // Build a map from id to node for fast lookup
    let node_map: BTreeMap<&str, &super::graph::BuildNode> =
        graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Compute hashes in topological order
    for id in sorted_ids {
        if let Some(node) = node_map.get(id) {
            let hash = hash_node_with_deps(node, cwd, &hashes)?;
            hashes.insert(id.to_string(), hash);
        }
    }

    Ok(hashes)
}

#[cfg(test)]
mod tests {
    use super::super::graph::BuildGraph;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_normalize_path_forward_slashes() {
        // This test verifies that paths use forward slashes
        let path = Path::new("/some/path/to/file.ts");
        let normalized = normalize_path(path);
        assert!(!normalized.contains('\\'));
        assert!(normalized.starts_with('/'));
    }

    #[test]
    fn test_normalize_path_strips_trailing_slash() {
        let path = Path::new("/some/path/");
        let normalized = normalize_path(path);
        assert!(!normalized.ends_with('/') || normalized == "/");
    }

    #[test]
    fn test_hash_bytes_deterministic() {
        let data = b"hello world";
        let hash1 = hash_bytes(data);
        let hash2 = hash_bytes(data);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // blake3 produces 256-bit hash = 64 hex chars
    }

    #[test]
    fn test_hash_string_deterministic() {
        let s = "test string";
        let hash1 = hash_string(s);
        let hash2 = hash_string(s);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_file_deterministic() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "file content").unwrap();

        let hash1 = hash_file(&file).unwrap();
        let hash2 = hash_file(&file).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_file_changes_with_content() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");

        std::fs::write(&file, "content v1").unwrap();
        let hash1 = hash_file(&file).unwrap();

        std::fs::write(&file, "content v2").unwrap();
        let hash2 = hash_file(&file).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_env_deterministic() {
        let allowlist = vec!["PATH".to_string(), "HOME".to_string()];
        let hash1 = hash_env(&allowlist);
        let hash2 = hash_env(&allowlist);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_env_sorted() {
        // Order shouldn't matter
        let list1 = vec!["A".to_string(), "B".to_string()];
        let list2 = vec!["B".to_string(), "A".to_string()];
        let hash1 = hash_env(&list1);
        let hash2 = hash_env(&list2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_expand_glob_deterministic() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.ts"), "a").unwrap();
        std::fs::write(dir.path().join("b.ts"), "b").unwrap();
        std::fs::write(dir.path().join("c.ts"), "c").unwrap();

        let files1 = expand_glob("*.ts", dir.path(), &[]).unwrap();
        let files2 = expand_glob("*.ts", dir.path(), &[]).unwrap();

        assert_eq!(files1, files2);
        assert_eq!(files1.len(), 3);
    }

    #[test]
    fn test_expand_glob_exclusions() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("a.ts"), "a").unwrap();
        std::fs::write(dir.path().join("node_modules/b.ts"), "b").unwrap();

        let files = expand_glob("**/*.ts", dir.path(), &["node_modules/**"]).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().contains("a.ts"));
    }

    #[test]
    fn test_hash_node_deterministic() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut node = BuildNode::script("build", "echo hello");
        node.add_input(BuildInput::file(
            dir.path().join("package.json").to_string_lossy().to_string(),
        ));

        let hash1 = hash_node(&node, dir.path()).unwrap();
        let hash2 = hash_node(&node, dir.path()).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_node_changes_on_input_change() {
        let dir = tempdir().unwrap();
        let pkg_json = dir.path().join("package.json");
        std::fs::write(&pkg_json, "{}").unwrap();

        let mut node = BuildNode::script("build", "echo hello");
        node.add_input(BuildInput::file(pkg_json.to_string_lossy().to_string()));

        let hash1 = hash_node(&node, dir.path()).unwrap();

        // Change file content
        std::fs::write(&pkg_json, "{\"name\": \"test\"}").unwrap();

        let hash2 = hash_node(&node, dir.path()).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_input_missing_file() {
        let dir = tempdir().unwrap();
        let input = BuildInput::file("/nonexistent/file.txt");
        let hash = hash_input(&input, dir.path()).unwrap();

        // Should not error, but include "missing" marker
        assert!(!hash.is_empty());
    }

    #[test]
    fn test_hash_node_with_deps_includes_dep_hash() {
        
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        // Create a node with a dependency
        let mut node = BuildNode::script("test", "npm test");
        node.add_dep("script:build");

        // Hash without dep hashes
        let hash_no_deps = hash_node(&node, dir.path()).unwrap();

        // Hash with dep hashes
        let mut dep_hashes = BTreeMap::new();
        dep_hashes.insert("script:build".to_string(), "abc123".to_string());

        let hash_with_deps = hash_node_with_deps(&node, dir.path(), &dep_hashes).unwrap();

        // Should be different because dep hash is included
        assert_ne!(hash_no_deps, hash_with_deps);
    }

    #[test]
    fn test_hash_graph_with_dependencies_propagates() {
        
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        // Build depends on nothing
        let build_node = BuildNode::script("build", "echo build");
        graph.add_node(build_node);

        // Test depends on build
        let mut test_node = BuildNode::script("test", "echo test");
        test_node.add_dep("script:build");
        graph.add_node(test_node);

        graph.normalize();

        // Compute hashes
        let hashes = hash_graph(&graph).unwrap();

        assert!(hashes.contains_key("script:build"));
        assert!(hashes.contains_key("script:test"));

        // Both should have valid hashes
        assert_eq!(hashes["script:build"].len(), 64);
        assert_eq!(hashes["script:test"].len(), 64);
    }

    #[test]
    fn test_dep_hash_changes_when_dependency_changes() {
        
        let dir = tempdir().unwrap();
        let pkg_json = dir.path().join("package.json");
        std::fs::write(&pkg_json, "{}").unwrap();

        let mut graph = BuildGraph::new(dir.path().to_string_lossy().to_string());

        // Build node with package.json input
        let mut build_node = BuildNode::script("build", "echo build");
        build_node.add_input(BuildInput::file(pkg_json.to_string_lossy().to_string()));
        graph.add_node(build_node);

        // Test depends on build
        let mut test_node = BuildNode::script("test", "echo test");
        test_node.add_dep("script:build");
        graph.add_node(test_node);

        graph.normalize();

        // Get initial hashes
        let hashes1 = hash_graph(&graph).unwrap();
        let test_hash1 = hashes1["script:test"].clone();

        // Modify the file that build depends on
        std::fs::write(&pkg_json, "{\"name\": \"changed\"}").unwrap();

        // Get new hashes
        let hashes2 = hash_graph(&graph).unwrap();
        let test_hash2 = hashes2["script:test"].clone();

        // Build hash should change
        assert_ne!(hashes1["script:build"], hashes2["script:build"]);

        // Test hash should ALSO change (dep-hash propagation)
        assert_ne!(test_hash1, test_hash2);
    }
}
