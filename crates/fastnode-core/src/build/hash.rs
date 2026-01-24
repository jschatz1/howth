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
        BuildInput::File { path } => {
            buf.extend_from_slice(b"file\0");
            buf.extend_from_slice(path.as_bytes());
            buf.push(0);
        }
        BuildInput::Glob { pattern, root } => {
            buf.extend_from_slice(b"glob\0");
            buf.extend_from_slice(pattern.as_bytes());
            buf.push(0);
            buf.extend_from_slice(root.as_bytes());
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
    }

    buf
}

/// Hash a single build input.
pub fn hash_input(input: &BuildInput, cwd: &Path) -> HashResult<String> {
    match input {
        BuildInput::File { path } => {
            let full_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                cwd.join(path)
            };

            if full_path.exists() {
                hash_file(&full_path)
            } else {
                // Missing file - include marker in hash
                Ok(hash_string(&format!("missing:{}", normalize_path(&full_path))))
            }
        }
        BuildInput::Glob { pattern, root } => {
            let root_path = if Path::new(root).is_absolute() {
                PathBuf::from(root)
            } else {
                cwd.join(root)
            };

            hash_glob(pattern, &root_path, DEFAULT_GLOB_EXCLUSIONS)
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
    }
}

/// Compute the hash for a build node.
///
/// The hash is computed from:
/// - Schema version
/// - Node kind
/// - Node label
/// - All input hashes
/// - Environment hash
/// - Script specification
/// - Dependencies
pub fn hash_node(node: &BuildNode, cwd: &Path) -> HashResult<String> {
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
    let mut input_hashes: Vec<(Vec<u8>, String)> = Vec::new();
    for input in &node.inputs {
        let encoded = encode_input(input);
        let hash = hash_input(input, cwd)?;
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

    // Dependencies (sorted)
    let mut deps = node.deps.clone();
    deps.sort();
    hasher.update(b"deps:");
    for dep in &deps {
        hasher.update(dep.as_bytes());
        hasher.update(b"\0");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute hashes for all nodes in a graph.
pub fn hash_graph(graph: &super::graph::BuildGraph) -> HashResult<BTreeMap<String, String>> {
    let cwd = Path::new(&graph.cwd);
    let mut hashes = BTreeMap::new();

    for node in &graph.nodes {
        let hash = hash_node(node, cwd)?;
        hashes.insert(node.id.clone(), hash);
    }

    Ok(hashes)
}

#[cfg(test)]
mod tests {
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
}
