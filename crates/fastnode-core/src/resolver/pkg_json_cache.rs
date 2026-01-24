//! Package.json parse cache trait.
//!
//! Provides a trait for caching parsed package.json files with
//! mtime/size stamps for invalidation.

use serde_json::Value;
use std::path::Path;

/// File stamp for cache invalidation.
#[derive(Debug, Clone, Default)]
pub struct PkgJsonStamp {
    /// Modification time in milliseconds since epoch.
    pub mtime_ms: Option<u64>,
    /// File size in bytes.
    pub size: Option<u64>,
}

impl PkgJsonStamp {
    /// Create stamp from a path by reading its metadata.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn from_path(path: &Path) -> Self {
        if let Ok(meta) = path.metadata() {
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            Self {
                mtime_ms,
                size: Some(meta.len()),
            }
        } else {
            Self::default()
        }
    }

    /// Check if the stamp matches the current file state.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn matches(&self, path: &Path) -> bool {
        let Ok(meta) = path.metadata() else {
            return false;
        };

        // Check mtime
        if let Some(expected_mtime) = self.mtime_ms {
            let current_mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            if current_mtime != Some(expected_mtime) {
                return false;
            }
        }

        // Check size
        if let Some(expected_size) = self.size {
            if meta.len() != expected_size {
                return false;
            }
        }

        true
    }
}

/// Cached package.json entry.
#[derive(Debug, Clone)]
pub struct CachedPkgJson {
    /// The parsed package.json value.
    pub value: Value,
    /// File stamp for invalidation.
    pub stamp: PkgJsonStamp,
}

/// Trait for caching parsed package.json files.
///
/// Implementations should be thread-safe (Send + Sync).
pub trait PkgJsonCache: Send + Sync + std::fmt::Debug {
    /// Look up a cached package.json by path.
    ///
    /// Returns None if not cached or if the stamp is invalid.
    fn get(&self, path: &Path) -> Option<Value>;

    /// Store a parsed package.json in the cache.
    fn set(&self, path: &Path, value: Value);
}

/// No-op cache implementation (always misses, never stores).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPkgJsonCache;

impl PkgJsonCache for NoPkgJsonCache {
    fn get(&self, _path: &Path) -> Option<Value> {
        None
    }

    fn set(&self, _path: &Path, _value: Value) {
        // No-op
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_stamp_from_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let stamp = PkgJsonStamp::from_path(&file);
        assert!(stamp.mtime_ms.is_some());
        assert!(stamp.size.is_some());
    }

    #[test]
    fn test_stamp_matches() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let stamp = PkgJsonStamp::from_path(&file);
        assert!(stamp.matches(&file));
    }

    #[test]
    fn test_stamp_mismatch_after_write() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let stamp = PkgJsonStamp::from_path(&file);

        // Modify the file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&file, r#"{"name": "modified"}"#).unwrap();

        // Stamp should no longer match (size changed)
        assert!(!stamp.matches(&file));
    }

    #[test]
    fn test_stamp_nonexistent_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("nonexistent.json");

        let stamp = PkgJsonStamp::from_path(&file);
        assert!(stamp.mtime_ms.is_none());
        assert!(stamp.size.is_none());

        // matches() should return false for nonexistent file
        assert!(!stamp.matches(&file));
    }

    #[test]
    fn test_no_cache_always_misses() {
        let cache = NoPkgJsonCache;
        let path = Path::new("/fake/package.json");

        assert!(cache.get(path).is_none());

        // set does nothing
        cache.set(path, serde_json::json!({"name": "test"}));

        // still returns None
        assert!(cache.get(path).is_none());
    }
}
