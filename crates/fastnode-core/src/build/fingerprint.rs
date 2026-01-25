//! Output fingerprinting for build correctness (v2.2).
//!
//! Fingerprinting ensures cache hits are correct even when outputs
//! are mutated externally. A cache hit requires:
//! - input_hash match AND
//! - output_fingerprint match (when outputs are declared)
//!
//! ## Fingerprint Algorithm
//!
//! 1. Enumerate all declared outputs deterministically
//! 2. For each output, record: existence, size, mtime
//! 3. Hash the canonical encoding via blake3
//!
//! ## Determinism Rules
//!
//! - Paths normalized to forward slashes, relative to project root
//! - Outputs sorted lexicographically by normalized path
//! - Fixed-endian encoding for all integers

use super::graph::BuildOutput;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

/// Schema version for output fingerprints (v2.2).
pub const FINGERPRINT_SCHEMA_VERSION: u32 = 1;

/// Output fingerprint for cache validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputFingerprint {
    /// Schema version for format evolution.
    pub schema_version: u32,
    /// Blake3 hash of the canonical fingerprint encoding.
    pub hash: String,
    /// Number of outputs fingerprinted.
    pub output_count: u32,
    /// Total size of all outputs in bytes.
    pub total_size: u64,
}

impl OutputFingerprint {
    /// Check if this fingerprint matches another.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version && self.hash == other.hash
    }
}

/// Fingerprinting mode for a build node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FingerprintMode {
    /// Fingerprinting disabled (legacy behavior).
    Disabled,
    /// Fingerprinting enabled when outputs are declared (default).
    #[default]
    Enabled,
}

/// Metadata for a single output entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputEntry {
    /// Normalized path (forward slashes, relative).
    path: String,
    /// Output kind: "file", "dir", or "glob".
    kind: String,
    /// Whether the output exists.
    exists: bool,
    /// File/directory size in bytes (0 if missing).
    size: u64,
    /// Modification time as Unix timestamp in millis (0 if missing).
    mtime_ms: u64,
    /// For directories: sorted list of child paths.
    children: Vec<String>,
}

impl OutputEntry {
    /// Encode this entry for hashing.
    fn encode(&self, hasher: &mut Hasher) {
        // Kind
        hasher.update(b"kind:");
        hasher.update(self.kind.as_bytes());
        hasher.update(b"\0");

        // Path
        hasher.update(b"path:");
        hasher.update(self.path.as_bytes());
        hasher.update(b"\0");

        // Exists
        hasher.update(b"exists:");
        hasher.update(if self.exists { b"1" } else { b"0" });
        hasher.update(b"\0");

        if self.exists {
            // Size (little-endian u64)
            hasher.update(b"size:");
            hasher.update(&self.size.to_le_bytes());
            hasher.update(b"\0");

            // Mtime (little-endian u64)
            hasher.update(b"mtime:");
            hasher.update(&self.mtime_ms.to_le_bytes());
            hasher.update(b"\0");

            // Children (for directories)
            if !self.children.is_empty() {
                hasher.update(b"children:");
                for child in &self.children {
                    hasher.update(child.as_bytes());
                    hasher.update(b"\0");
                }
            }
        }

        hasher.update(b"\n"); // Entry separator
    }
}

/// Result type for fingerprint operations.
pub type FingerprintResult<T> = Result<T, FingerprintError>;

/// Error during fingerprinting.
#[derive(Debug)]
pub struct FingerprintError {
    /// Error code.
    pub code: &'static str,
    /// Error message.
    pub message: String,
}

impl FingerprintError {
    /// Create a new fingerprint error.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Create an I/O error.
    #[must_use]
    pub fn io(err: io::Error) -> Self {
        Self {
            code: "FINGERPRINT_IO_ERROR",
            message: err.to_string(),
        }
    }
}

impl std::fmt::Display for FingerprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FingerprintError {}

/// Normalize a path for fingerprinting.
///
/// - Converts to forward slashes
/// - Makes relative to project root
/// - Removes trailing slashes
#[must_use]
pub fn normalize_output_path(path: &Path, root: &Path) -> String {
    let rel_path = if path.is_absolute() {
        path.strip_prefix(root).unwrap_or(path)
    } else {
        path
    };

    let mut normalized = rel_path
        .to_string_lossy()
        .replace('\\', "/");

    // Remove trailing slash
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }

    normalized
}

/// Get file metadata for fingerprinting.
fn get_file_metadata(path: &Path) -> (bool, u64, u64) {
    match fs::metadata(path) {
        Ok(meta) => {
            let size = meta.len();
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            (true, size, mtime_ms)
        }
        Err(_) => (false, 0, 0),
    }
}

/// Enumerate directory children recursively for fingerprinting.
fn enumerate_dir_children(dir: &Path, root: &Path) -> Vec<String> {
    let mut children = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let normalized = normalize_output_path(&path, root);
            children.push(normalized.clone());

            // Recurse into subdirectories
            if path.is_dir() {
                children.extend(enumerate_dir_children(&path, root));
            }
        }
    }

    // Sort for determinism
    children.sort();
    children
}

/// Compute the fingerprint for a set of outputs.
///
/// Returns `None` if there are no outputs to fingerprint.
pub fn compute_fingerprint(
    outputs: &[BuildOutput],
    cwd: &Path,
) -> FingerprintResult<Option<OutputFingerprint>> {
    if outputs.is_empty() {
        return Ok(None);
    }

    let mut entries: Vec<OutputEntry> = Vec::new();
    let mut total_size: u64 = 0;

    for output in outputs {
        let full_path = if Path::new(&output.path).is_absolute() {
            std::path::PathBuf::from(&output.path)
        } else {
            cwd.join(&output.path)
        };

        let normalized_path = normalize_output_path(&full_path, cwd);

        match output.kind.as_str() {
            "file" => {
                let (exists, size, mtime_ms) = get_file_metadata(&full_path);
                if exists {
                    total_size += size;
                }
                entries.push(OutputEntry {
                    path: normalized_path,
                    kind: "file".to_string(),
                    exists,
                    size,
                    mtime_ms,
                    children: Vec::new(),
                });
            }
            "dir" => {
                let (exists, _size, mtime_ms) = get_file_metadata(&full_path);
                let children = if exists && full_path.is_dir() {
                    enumerate_dir_children(&full_path, cwd)
                } else {
                    Vec::new()
                };

                // For directories, compute total size of contents
                let mut dir_size = 0u64;
                if exists {
                    for child in &children {
                        let child_path = cwd.join(child);
                        if child_path.is_file() {
                            if let Ok(meta) = fs::metadata(&child_path) {
                                dir_size += meta.len();
                            }
                        }
                    }
                    total_size += dir_size;
                }

                entries.push(OutputEntry {
                    path: normalized_path,
                    kind: "dir".to_string(),
                    exists,
                    size: dir_size,
                    mtime_ms,
                    children,
                });
            }
            "glob" => {
                // For globs, expand and fingerprint each matched file
                // Use the path as pattern, cwd as root
                let pattern = &output.path;
                let matched_files = expand_glob_for_fingerprint(pattern, cwd);

                for file_path in matched_files {
                    let (exists, size, mtime_ms) = get_file_metadata(&file_path);
                    if exists {
                        total_size += size;
                    }
                    let normalized = normalize_output_path(&file_path, cwd);
                    entries.push(OutputEntry {
                        path: normalized,
                        kind: "file".to_string(),
                        exists,
                        size,
                        mtime_ms,
                        children: Vec::new(),
                    });
                }
            }
            _ => {
                // Unknown kind, treat as file
                let (exists, size, mtime_ms) = get_file_metadata(&full_path);
                if exists {
                    total_size += size;
                }
                entries.push(OutputEntry {
                    path: normalized_path,
                    kind: output.kind.clone(),
                    exists,
                    size,
                    mtime_ms,
                    children: Vec::new(),
                });
            }
        }
    }

    // Sort entries by path for determinism
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    // Compute hash
    let mut hasher = Hasher::new();

    // Schema version
    hasher.update(b"fingerprint_schema:");
    hasher.update(&FINGERPRINT_SCHEMA_VERSION.to_le_bytes());
    hasher.update(b"\0");

    // Output count
    hasher.update(b"output_count:");
    hasher.update(&(entries.len() as u32).to_le_bytes());
    hasher.update(b"\0");

    // Encode each entry
    for entry in &entries {
        entry.encode(&mut hasher);
    }

    let hash = hasher.finalize().to_hex().to_string();

    Ok(Some(OutputFingerprint {
        schema_version: FINGERPRINT_SCHEMA_VERSION,
        hash,
        output_count: entries.len() as u32,
        total_size,
    }))
}

/// Expand a glob pattern for fingerprinting.
fn expand_glob_for_fingerprint(pattern: &str, root: &Path) -> Vec<std::path::PathBuf> {
    use walkdir::WalkDir;

    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip common excluded directories
            let name = e.file_name().to_string_lossy();
            !matches!(name.as_ref(), "node_modules" | ".git" | ".howth")
        })
    {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                let path = entry.path();
                let rel = path.strip_prefix(root).unwrap_or(path);
                let rel_str = rel.to_string_lossy();

                // Check if matches pattern
                let matches = if pattern == "**/*" {
                    true
                } else if let Ok(glob_pattern) = glob::Pattern::new(pattern) {
                    glob_pattern.matches(&rel_str)
                } else {
                    false
                };

                if matches {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    // Sort for determinism
    files.sort();
    files
}

/// Compare two fingerprints and return whether they match.
#[must_use]
pub fn fingerprints_match(a: Option<&OutputFingerprint>, b: Option<&OutputFingerprint>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(fp_a), Some(fp_b)) => fp_a.matches(fp_b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_fingerprint_schema_version() {
        assert_eq!(FINGERPRINT_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_normalize_output_path() {
        let root = Path::new("/project");
        let path = Path::new("/project/dist/bundle.js");
        assert_eq!(normalize_output_path(path, root), "dist/bundle.js");
    }

    #[test]
    fn test_normalize_output_path_backslashes() {
        let root = Path::new("/project");
        let path = Path::new("dist\\output\\file.js");
        let normalized = normalize_output_path(path, root);
        assert!(!normalized.contains('\\'));
    }

    #[test]
    fn test_fingerprint_empty_outputs() {
        let dir = tempdir().unwrap();
        let result = compute_fingerprint(&[], dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_fingerprint_single_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("output.txt");
        std::fs::write(&file, "hello world").unwrap();

        let outputs = vec![BuildOutput::file("output.txt")];
        let fp = compute_fingerprint(&outputs, dir.path()).unwrap();

        assert!(fp.is_some());
        let fp = fp.unwrap();
        assert_eq!(fp.schema_version, FINGERPRINT_SCHEMA_VERSION);
        assert_eq!(fp.output_count, 1);
        assert!(fp.total_size > 0);
        assert!(!fp.hash.is_empty());
    }

    #[test]
    fn test_fingerprint_deterministic() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("output.txt");
        std::fs::write(&file, "hello world").unwrap();

        let outputs = vec![BuildOutput::file("output.txt")];

        let fp1 = compute_fingerprint(&outputs, dir.path()).unwrap();
        let fp2 = compute_fingerprint(&outputs, dir.path()).unwrap();

        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_fingerprint_changes_on_content_change() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("output.txt");
        std::fs::write(&file, "hello world").unwrap();

        let outputs = vec![BuildOutput::file("output.txt")];

        let fp1 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Modify file - size changes
        std::fs::write(&file, "hello world, this is different!").unwrap();

        let fp2 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Hash should be different due to size change
        assert_ne!(fp1.hash, fp2.hash);
    }

    #[test]
    fn test_fingerprint_changes_on_delete() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("output.txt");
        std::fs::write(&file, "hello world").unwrap();

        let outputs = vec![BuildOutput::file("output.txt")];

        let fp1 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Delete file
        std::fs::remove_file(&file).unwrap();

        let fp2 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Hash should be different
        assert_ne!(fp1.hash, fp2.hash);
    }

    #[test]
    fn test_fingerprint_directory() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("dist");
        std::fs::create_dir(&out_dir).unwrap();
        std::fs::write(out_dir.join("a.js"), "a").unwrap();
        std::fs::write(out_dir.join("b.js"), "b").unwrap();

        let outputs = vec![BuildOutput::dir("dist")];

        let fp = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();
        assert_eq!(fp.output_count, 1);
        assert!(fp.total_size > 0);
    }

    #[test]
    fn test_fingerprint_directory_changes_on_new_file() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("dist");
        std::fs::create_dir(&out_dir).unwrap();
        std::fs::write(out_dir.join("a.js"), "a").unwrap();

        let outputs = vec![BuildOutput::dir("dist")];

        let fp1 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Add new file
        std::fs::write(out_dir.join("b.js"), "b").unwrap();

        let fp2 = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Hash should be different
        assert_ne!(fp1.hash, fp2.hash);
    }

    #[test]
    fn test_fingerprint_missing_file_optional() {
        let dir = tempdir().unwrap();

        // File doesn't exist but is declared
        let outputs = vec![BuildOutput::file("missing.txt")];

        let fp = compute_fingerprint(&outputs, dir.path()).unwrap().unwrap();

        // Should still produce a fingerprint
        assert_eq!(fp.output_count, 1);
        assert_eq!(fp.total_size, 0);
    }

    #[test]
    fn test_fingerprints_match() {
        let fp1 = OutputFingerprint {
            schema_version: 1,
            hash: "abc123".to_string(),
            output_count: 1,
            total_size: 100,
        };
        let fp2 = OutputFingerprint {
            schema_version: 1,
            hash: "abc123".to_string(),
            output_count: 1,
            total_size: 100,
        };
        let fp3 = OutputFingerprint {
            schema_version: 1,
            hash: "different".to_string(),
            output_count: 1,
            total_size: 100,
        };

        assert!(fingerprints_match(Some(&fp1), Some(&fp2)));
        assert!(!fingerprints_match(Some(&fp1), Some(&fp3)));
        assert!(fingerprints_match(None, None));
        assert!(!fingerprints_match(Some(&fp1), None));
    }

    #[test]
    fn test_fingerprint_sorted_order() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("z.txt"), "z").unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(dir.path().join("m.txt"), "m").unwrap();

        let outputs1 = vec![
            BuildOutput::file("z.txt"),
            BuildOutput::file("a.txt"),
            BuildOutput::file("m.txt"),
        ];
        let outputs2 = vec![
            BuildOutput::file("a.txt"),
            BuildOutput::file("m.txt"),
            BuildOutput::file("z.txt"),
        ];

        let fp1 = compute_fingerprint(&outputs1, dir.path()).unwrap();
        let fp2 = compute_fingerprint(&outputs2, dir.path()).unwrap();

        // Order shouldn't matter - should produce same fingerprint
        assert_eq!(fp1, fp2);
    }
}
