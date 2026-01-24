//! Caches for daemon.
//!
//! Provides thread-safe caches for resolver results and build results
//! with support for file-based invalidation via reverse index.

use fastnode_core::build::{BuildCache, MemoryCache};
use fastnode_core::resolver::{
    CachedResolveResult, FileStamp, PkgJsonCache, PkgJsonStamp, ResolveResult, ResolveStatus,
    ResolverCache, ResolverCacheKey,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use tracing::debug;

/// Daemon resolver cache with reverse index for invalidation.
#[derive(Debug, Default)]
pub struct DaemonResolverCache {
    /// Cache entries: key -> cached result
    entries: RwLock<HashMap<ResolverCacheKey, CachedResolveResult>>,
    /// Reverse index: resolved path -> set of cache keys that depend on it
    reverse_index: RwLock<HashMap<PathBuf, HashSet<ResolverCacheKey>>>,
}

impl DaemonResolverCache {
    /// Create a new empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a resolver result in the cache.
    ///
    /// Also updates the reverse index if the result resolved to a file.
    pub fn put(&self, key: ResolverCacheKey, result: &ResolveResult) {
        // Create cached result
        let cached = CachedResolveResult {
            resolved: result
                .resolved
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            status: match result.status {
                ResolveStatus::Resolved => "resolved".to_string(),
                ResolveStatus::Unresolved => "unresolved".to_string(),
            },
            reason: result.reason.map(|r| r.to_string()),
            tried: result
                .tried
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            stamp: result
                .resolved
                .as_ref()
                .map(|p| FileStamp::from_path(p))
                .unwrap_or_default(),
        };

        // Update reverse index if resolved to a path
        if let Some(ref resolved_path) = result.resolved {
            let mut index = self.reverse_index.write().unwrap();
            index
                .entry(resolved_path.clone())
                .or_default()
                .insert(key.clone());
        }

        // Store in cache
        let mut entries = self.entries.write().unwrap();
        entries.insert(key, cached);
    }

    /// Remove a cache entry by key.
    pub fn remove(&self, key: &ResolverCacheKey) {
        let mut entries = self.entries.write().unwrap();
        if let Some(cached) = entries.remove(key) {
            // Also remove from reverse index
            if let Some(ref resolved_path) = cached.resolved {
                let path = PathBuf::from(resolved_path);
                let mut index = self.reverse_index.write().unwrap();
                if let Some(keys) = index.get_mut(&path) {
                    keys.remove(key);
                    if keys.is_empty() {
                        index.remove(&path);
                    }
                }
            }
        }
    }

    /// Invalidate all cache entries that depend on a given file path.
    ///
    /// Returns the number of entries invalidated.
    pub fn invalidate_path(&self, path: &Path) -> usize {
        // Try to canonicalize the path for consistent matching
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Get keys to invalidate
        let keys_to_remove: Vec<ResolverCacheKey> = {
            let index = self.reverse_index.read().unwrap();
            index
                .get(&canonical)
                .map(|keys| keys.iter().cloned().collect())
                .unwrap_or_default()
        };

        let count = keys_to_remove.len();

        if count > 0 {
            debug!(path = %canonical.display(), count, "Invalidating cache entries for path");

            // Remove from cache
            let mut entries = self.entries.write().unwrap();
            for key in &keys_to_remove {
                entries.remove(key);
            }

            // Remove from reverse index
            let mut index = self.reverse_index.write().unwrap();
            index.remove(&canonical);
        }

        count
    }

    /// Get cache statistics.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let entries = self.entries.read().unwrap();
        let index = self.reverse_index.read().unwrap();
        CacheStats {
            entry_count: entries.len(),
            reverse_index_paths: index.len(),
        }
    }

    /// Clear all cache entries.
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        let mut index = self.reverse_index.write().unwrap();
        entries.clear();
        index.clear();
    }
}

impl ResolverCache for DaemonResolverCache {
    fn get(&self, key: &ResolverCacheKey) -> Option<CachedResolveResult> {
        let entries = self.entries.read().unwrap();
        let cached = entries.get(key)?;

        // Validate stamp before returning
        if cached.stamp.is_valid() {
            Some(cached.clone())
        } else {
            // Stamp is invalid, entry should be removed
            // But we can't remove during read, so return None
            // The entry will be overwritten on next resolution
            debug!(
                specifier = %key.specifier,
                "Cache entry stamp invalid, treating as miss"
            );
            None
        }
    }

    fn set(&self, key: ResolverCacheKey, value: CachedResolveResult) {
        // Update reverse index if resolved to a path
        if let Some(ref resolved_path) = value.resolved {
            let path = PathBuf::from(resolved_path);
            let mut index = self.reverse_index.write().unwrap();
            index.entry(path).or_default().insert(key.clone());
        }

        // Store in cache
        let mut entries = self.entries.write().unwrap();
        entries.insert(key, value);
    }
}

/// Cache statistics.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub entry_count: usize,
    pub reverse_index_paths: usize,
}

/// Cached package.json entry.
#[derive(Debug, Clone)]
struct CachedPkgJsonEntry {
    /// Parsed package.json value.
    value: Value,
    /// File stamp for invalidation.
    stamp: PkgJsonStamp,
}

/// Daemon package.json cache with mtime/size invalidation.
///
/// Caches parsed package.json files for faster resolution of exports/imports.
#[derive(Debug, Default)]
pub struct DaemonPkgJsonCache {
    /// Cache entries: canonical path -> cached entry
    entries: RwLock<HashMap<PathBuf, CachedPkgJsonEntry>>,
}

impl DaemonPkgJsonCache {
    /// Create a new empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Invalidate a package.json entry by path.
    ///
    /// Returns true if an entry was removed.
    pub fn invalidate(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut entries = self.entries.write().unwrap();
        entries.remove(&canonical).is_some()
    }

    /// Clear all cache entries.
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }

    /// Get cache statistics.
    #[must_use]
    pub fn stats(&self) -> PkgJsonCacheStats {
        let entries = self.entries.read().unwrap();
        PkgJsonCacheStats {
            entry_count: entries.len(),
        }
    }
}

impl PkgJsonCache for DaemonPkgJsonCache {
    fn get(&self, path: &Path) -> Option<Value> {
        let canonical = path.canonicalize().ok()?;
        let entries = self.entries.read().unwrap();
        let entry = entries.get(&canonical)?;

        // Validate stamp
        if entry.stamp.matches(&canonical) {
            Some(entry.value.clone())
        } else {
            // Stamp is invalid - entry will be overwritten on next resolution
            debug!(path = %canonical.display(), "pkg.json cache stamp invalid, treating as miss");
            None
        }
    }

    fn set(&self, path: &Path, value: Value) {
        let Ok(canonical) = path.canonicalize() else {
            return;
        };

        let stamp = PkgJsonStamp::from_path(&canonical);
        let entry = CachedPkgJsonEntry { value, stamp };

        let mut entries = self.entries.write().unwrap();
        entries.insert(canonical, entry);
    }
}

/// Package.json cache statistics.
#[derive(Debug, Clone, Copy)]
pub struct PkgJsonCacheStats {
    pub entry_count: usize,
}

/// Daemon build cache with thread-safe access.
///
/// Wraps the core `MemoryCache` with a RwLock for concurrent access
/// and provides path-based invalidation for file watcher integration.
#[derive(Debug, Default)]
pub struct DaemonBuildCache {
    /// Inner cache behind a RwLock.
    cache: RwLock<MemoryCache>,
    /// Reverse index: file path -> set of node IDs that include that file
    reverse_index: RwLock<HashMap<PathBuf, HashSet<String>>>,
}

impl DaemonBuildCache {
    /// Create a new empty build cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a node hash is cached and was successful.
    pub fn get(&self, node_id: &str, hash: &str) -> Option<bool> {
        let cache = self.cache.read().unwrap();
        cache.get(node_id, hash)
    }

    /// Store a result for a node.
    pub fn set(&self, node_id: &str, hash: &str, ok: bool) {
        let mut cache = self.cache.write().unwrap();
        cache.set(node_id, hash, ok);
    }

    /// Add a file path to the reverse index for a node.
    pub fn add_file_dependency(&self, node_id: &str, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut index = self.reverse_index.write().unwrap();
        index
            .entry(canonical)
            .or_default()
            .insert(node_id.to_string());
    }

    /// Invalidate all node caches that depend on a given file path.
    ///
    /// Returns the number of entries invalidated.
    pub fn invalidate_path(&self, path: &Path) -> usize {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Get node IDs to invalidate
        let node_ids: Vec<String> = {
            let index = self.reverse_index.read().unwrap();
            index
                .get(&canonical)
                .map(|ids| ids.iter().cloned().collect())
                .unwrap_or_default()
        };

        let count = node_ids.len();

        if count > 0 {
            debug!(path = %canonical.display(), count, "Invalidating build cache entries for path");

            // Invalidate each node
            let mut cache = self.cache.write().unwrap();
            for node_id in &node_ids {
                cache.invalidate(node_id);
            }

            // Remove from reverse index
            let mut index = self.reverse_index.write().unwrap();
            index.remove(&canonical);
        }

        count
    }

    /// Clear all cache entries.
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut index = self.reverse_index.write().unwrap();
        cache.clear();
        index.clear();
    }

    /// Get cache statistics.
    #[must_use]
    pub fn stats(&self) -> BuildCacheStats {
        let index = self.reverse_index.read().unwrap();
        BuildCacheStats {
            reverse_index_paths: index.len(),
        }
    }
}

/// Build cache statistics.
#[derive(Debug, Clone, Copy)]
pub struct BuildCacheStats {
    pub reverse_index_paths: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastnode_core::resolver::ResolveReasonCode;
    use std::fs;
    use tempfile::tempdir;

    fn make_key(specifier: &str) -> ResolverCacheKey {
        ResolverCacheKey {
            cwd: "/home/user".to_string(),
            parent: "/home/user/src".to_string(),
            specifier: specifier.to_string(),
            channel: "stable".to_string(),
        }
    }

    fn make_resolved_result(path: PathBuf) -> ResolveResult {
        ResolveResult {
            resolved: Some(path),
            status: ResolveStatus::Resolved,
            reason: None,
            tried: Vec::new(),
        }
    }

    fn make_unresolved_result() -> ResolveResult {
        ResolveResult {
            resolved: None,
            status: ResolveStatus::Unresolved,
            reason: Some(ResolveReasonCode::NotFound),
            tried: Vec::new(),
        }
    }

    #[test]
    fn test_cache_put_and_get() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("dep.js");
        fs::write(&file, "export const x = 1;").unwrap();

        let cache = DaemonResolverCache::new();
        let key = make_key("./dep");
        let result = make_resolved_result(file.clone());

        cache.put(key.clone(), &result);

        let cached = cache.get(&key);
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert_eq!(cached.status, "resolved");
        assert!(cached.resolved.is_some());
    }

    #[test]
    fn test_cache_miss() {
        let cache = DaemonResolverCache::new();
        let key = make_key("./nonexistent");

        let cached = cache.get(&key);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_unresolved() {
        let cache = DaemonResolverCache::new();
        let key = make_key("./missing");
        let result = make_unresolved_result();

        cache.put(key.clone(), &result);

        let cached = cache.get(&key);
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert_eq!(cached.status, "unresolved");
        assert!(cached.resolved.is_none());
    }

    #[test]
    fn test_reverse_index_created() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("dep.js");
        fs::write(&file, "export const x = 1;").unwrap();

        let cache = DaemonResolverCache::new();
        let key = make_key("./dep");
        let result = make_resolved_result(file.canonicalize().unwrap());

        cache.put(key, &result);

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.reverse_index_paths, 1);
    }

    #[test]
    fn test_invalidate_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("dep.js");
        fs::write(&file, "export const x = 1;").unwrap();
        let canonical = file.canonicalize().unwrap();

        let cache = DaemonResolverCache::new();
        let key = make_key("./dep");
        let result = make_resolved_result(canonical.clone());

        cache.put(key.clone(), &result);

        // Verify it's cached
        assert!(cache.get(&key).is_some());

        // Invalidate by path
        let count = cache.invalidate_path(&canonical);
        assert_eq!(count, 1);

        // Verify it's gone
        assert!(cache.get(&key).is_none());

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.reverse_index_paths, 0);
    }

    #[test]
    fn test_invalidate_multiple_keys_same_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("shared.js");
        fs::write(&file, "export const x = 1;").unwrap();
        let canonical = file.canonicalize().unwrap();

        let cache = DaemonResolverCache::new();

        // Two different specifiers resolving to same file
        let key1 = ResolverCacheKey {
            cwd: "/home/user".to_string(),
            parent: "/home/user/src".to_string(),
            specifier: "./shared".to_string(),
            channel: "stable".to_string(),
        };
        let key2 = ResolverCacheKey {
            cwd: "/home/user".to_string(),
            parent: "/home/user/lib".to_string(),
            specifier: "../src/shared".to_string(),
            channel: "stable".to_string(),
        };

        let result = make_resolved_result(canonical.clone());

        cache.put(key1.clone(), &result);
        cache.put(key2.clone(), &result);

        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_some());

        // Invalidate by path - should remove both
        let count = cache.invalidate_path(&canonical);
        assert_eq!(count, 2);

        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_none());
    }

    #[test]
    fn test_clear() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("dep.js");
        fs::write(&file, "export const x = 1;").unwrap();

        let cache = DaemonResolverCache::new();
        let key = make_key("./dep");
        let result = make_resolved_result(file);

        cache.put(key.clone(), &result);
        assert!(cache.get(&key).is_some());

        cache.clear();

        assert!(cache.get(&key).is_none());
        let stats = cache.stats();
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.reverse_index_paths, 0);
    }

    // DaemonPkgJsonCache tests

    #[test]
    fn test_pkg_json_cache_put_and_get() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test", "version": "1.0.0"}"#).unwrap();

        let cache = DaemonPkgJsonCache::new();
        let value = serde_json::json!({"name": "test", "version": "1.0.0"});

        cache.set(&file, value.clone());

        let cached = cache.get(&file);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap()["name"], "test");
    }

    #[test]
    fn test_pkg_json_cache_miss() {
        let cache = DaemonPkgJsonCache::new();
        let file = Path::new("/nonexistent/package.json");

        let cached = cache.get(file);
        assert!(cached.is_none());
    }

    #[test]
    fn test_pkg_json_cache_invalidate() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let cache = DaemonPkgJsonCache::new();
        let value = serde_json::json!({"name": "test"});

        cache.set(&file, value);
        assert!(cache.get(&file).is_some());

        let removed = cache.invalidate(&file);
        assert!(removed);

        assert!(cache.get(&file).is_none());
    }

    #[test]
    fn test_pkg_json_cache_clear() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let cache = DaemonPkgJsonCache::new();
        cache.set(&file, serde_json::json!({"name": "test"}));

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 1);

        cache.clear();

        let stats = cache.stats();
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_pkg_json_cache_stale_on_file_change() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("package.json");
        fs::write(&file, r#"{"name": "test"}"#).unwrap();

        let cache = DaemonPkgJsonCache::new();
        cache.set(&file, serde_json::json!({"name": "test"}));

        // Verify it's cached
        assert!(cache.get(&file).is_some());

        // Modify the file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&file, r#"{"name": "modified"}"#).unwrap();

        // Cache should return None due to stale stamp
        assert!(cache.get(&file).is_none());
    }
}
