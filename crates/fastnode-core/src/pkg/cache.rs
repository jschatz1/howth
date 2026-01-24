//! Package cache management.
//!
//! Manages the global package cache where downloaded packages are stored.

use super::error::PkgError;
use crate::config::Channel;
use crate::paths::cache_dir;
use std::fs;
use std::path::{Path, PathBuf};

/// Package cache manager.
#[derive(Debug, Clone)]
pub struct PackageCache {
    /// Root directory for the package cache.
    root: PathBuf,
}

impl PackageCache {
    /// Create a new package cache for the given channel.
    #[must_use]
    pub fn new(channel: Channel) -> Self {
        let root = cache_dir(channel).join("packages").join("npm");
        Self { root }
    }

    /// Get the cache root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the path for a cached packument.
    ///
    /// Scoped package names are URL-encoded.
    #[must_use]
    pub fn packument_path(&self, name: &str) -> PathBuf {
        let encoded = Self::encode_name(name);
        self.root.join("packuments").join(format!("{encoded}.json"))
    }

    /// Get the directory path for a cached package version.
    ///
    /// This is the directory containing the extracted package.
    #[must_use]
    pub fn package_dir(&self, name: &str, version: &str) -> PathBuf {
        if name.starts_with('@') {
            // Scoped: @scope/name -> @scope/name/version/package
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            if parts.len() == 2 {
                self.root
                    .join(parts[0])
                    .join(parts[1])
                    .join(version)
                    .join("package")
            } else {
                self.root.join(name).join(version).join("package")
            }
        } else {
            self.root.join(name).join(version).join("package")
        }
    }

    /// Get the version directory (parent of package/).
    #[must_use]
    pub fn version_dir(&self, name: &str, version: &str) -> PathBuf {
        if name.starts_with('@') {
            let parts: Vec<&str> = name.splitn(2, '/').collect();
            if parts.len() == 2 {
                self.root.join(parts[0]).join(parts[1]).join(version)
            } else {
                self.root.join(name).join(version)
            }
        } else {
            self.root.join(name).join(version)
        }
    }

    /// Check if a package version is already cached.
    #[must_use]
    pub fn is_cached(&self, name: &str, version: &str) -> bool {
        let pkg_dir = self.package_dir(name, version);
        pkg_dir.exists() && pkg_dir.is_dir()
    }

    /// List all cached packages.
    ///
    /// Returns a vector of (name, version) tuples.
    ///
    /// # Errors
    /// Returns an error if the cache directory cannot be read.
    pub fn list_cached(&self) -> Result<Vec<(String, String)>, PkgError> {
        let mut result = Vec::new();

        if !self.root.exists() {
            return Ok(result);
        }

        // Enumerate packages
        Self::scan_packages(&self.root, None, &mut result)?;

        Ok(result)
    }

    fn scan_packages(
        dir: &Path,
        scope: Option<&str>,
        result: &mut Vec<(String, String)>,
    ) -> Result<(), PkgError> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(());
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Skip special directories
            if name_str == "packuments" || name_str.starts_with('.') {
                continue;
            }

            // Check for scope
            if name_str.starts_with('@') {
                // This is a scope directory, recurse
                Self::scan_packages(&path, Some(&name_str), result)?;
            } else {
                // This is a package directory
                let pkg_name = if let Some(scope) = scope {
                    format!("{scope}/{name_str}")
                } else {
                    name_str.to_string()
                };

                // Scan for versions
                if let Ok(version_entries) = fs::read_dir(&path) {
                    for version_entry in version_entries.flatten() {
                        let version_path = version_entry.path();
                        if !version_path.is_dir() {
                            continue;
                        }

                        let version_name = version_entry.file_name();
                        let version_str = version_name.to_string_lossy();

                        // Skip non-semver looking directories
                        if version_str.starts_with('.') {
                            continue;
                        }

                        // Check if package/ subdirectory exists
                        if version_path.join("package").is_dir() {
                            result.push((pkg_name.clone(), version_str.to_string()));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Ensure cache directories exist.
    ///
    /// # Errors
    /// Returns an error if directories cannot be created.
    pub fn ensure_dirs(&self) -> Result<(), PkgError> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(self.root.join("packuments"))?;
        Ok(())
    }

    /// URL-encode a package name for use as a filename.
    fn encode_name(name: &str) -> String {
        // Replace / with %2F for scoped packages
        name.replace('/', "%2F")
    }
}

/// Info about a cached package for listing.
#[derive(Debug, Clone)]
pub struct CachedPackageInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_package_dir_unscoped() {
        let cache = PackageCache::new(Channel::Stable);
        let path = cache.package_dir("react", "18.2.0");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("react"));
        assert!(path_str.contains("18.2.0"));
        assert!(path_str.ends_with("package"));
    }

    #[test]
    fn test_package_dir_scoped() {
        let cache = PackageCache::new(Channel::Stable);
        let path = cache.package_dir("@types/node", "20.0.0");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("@types"));
        assert!(path_str.contains("node"));
        assert!(path_str.contains("20.0.0"));
        assert!(path_str.ends_with("package"));
    }

    #[test]
    fn test_packument_path_unscoped() {
        let cache = PackageCache::new(Channel::Stable);
        let path = cache.packument_path("react");
        assert!(path.to_string_lossy().ends_with("react.json"));
    }

    #[test]
    fn test_packument_path_scoped() {
        let cache = PackageCache::new(Channel::Stable);
        let path = cache.packument_path("@types/node");
        assert!(path.to_string_lossy().ends_with("@types%2Fnode.json"));
    }

    #[test]
    fn test_is_cached_false_when_missing() {
        let cache = PackageCache::new(Channel::Stable);
        assert!(!cache.is_cached("nonexistent", "1.0.0"));
    }

    #[test]
    fn test_list_empty_cache() {
        let dir = tempdir().unwrap();
        let cache = PackageCache {
            root: dir.path().to_path_buf(),
        };
        let cached = cache.list_cached().unwrap();
        assert!(cached.is_empty());
    }

    #[test]
    fn test_list_with_packages() {
        let dir = tempdir().unwrap();
        let cache = PackageCache {
            root: dir.path().to_path_buf(),
        };

        // Create a fake cached package
        let pkg_dir = dir.path().join("react").join("18.2.0").join("package");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("package.json"), "{}").unwrap();

        let cached = cache.list_cached().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0], ("react".to_string(), "18.2.0".to_string()));
    }

    #[test]
    fn test_list_with_scoped_packages() {
        let dir = tempdir().unwrap();
        let cache = PackageCache {
            root: dir.path().to_path_buf(),
        };

        // Create a fake scoped cached package
        let pkg_dir = dir
            .path()
            .join("@types")
            .join("node")
            .join("20.0.0")
            .join("package");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("package.json"), "{}").unwrap();

        let cached = cache.list_cached().unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0], ("@types/node".to_string(), "20.0.0".to_string()));
    }
}
