//! npm registry client with persistent packument caching.
//!
//! Features:
//! - Disk-based packument cache with `ETag` validation
//! - Skip network for recently cached packuments (< 5 min)
//! - In-memory cache shared across clones
//! - Abbreviated packuments for smaller downloads

#![allow(clippy::manual_let_else)]

use super::cache::PackageCache;
use super::error::PkgError;
use super::npmrc::{load_npmrc_files, resolve_scoped_registries, ScopedRegistry};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use url::Url;

/// Default npm registry URL.
pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";

/// Environment variable to override registry URL.
pub const REGISTRY_ENV: &str = "FASTNODE_NPM_REGISTRY";

/// How long to trust cached packuments without revalidation (5 minutes).
const CACHE_FRESH_DURATION_SECS: u64 = 300;

/// Accept header for abbreviated packuments (smaller, faster).
const ABBREVIATED_ACCEPT: &str = "application/vnd.npm.install-v1+json";

/// Cached packument with `ETag` for conditional requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPackument {
    /// The packument JSON data.
    pub data: Value,
    /// `ETag` from the server for conditional requests.
    pub etag: Option<String>,
    /// Unix timestamp when cached.
    pub cached_at: u64,
}

impl CachedPackument {
    /// Check if this cached packument is still fresh (< 5 min old).
    fn is_fresh(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.cached_at) < CACHE_FRESH_DURATION_SECS
    }
}

/// Shared state for registry client (persists across clones).
#[derive(Debug)]
struct SharedState {
    /// In-memory packument cache.
    memory_cache: RwLock<HashMap<String, CachedPackument>>,
    /// Optional disk cache.
    disk_cache: Option<PackageCache>,
}

/// Registry client for fetching package metadata with caching.
///
/// Clone this client freely - all clones share the same memory cache.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: Url,
    http: Client,
    /// Shared state (memory cache, disk cache).
    shared: Arc<SharedState>,
    /// Scoped registries loaded from `.npmrc` files.
    scoped_registries: Arc<Vec<ScopedRegistry>>,
}

impl RegistryClient {
    /// Create a new registry client with the given base URL.
    ///
    /// # Errors
    /// Returns an error if the URL is invalid or the HTTP client cannot be created.
    pub fn new(base_url: &str) -> Result<Self, PkgError> {
        Self::new_with_cache(base_url, None)
    }

    /// Create a new registry client with optional disk cache.
    fn new_with_cache(base_url: &str, disk_cache: Option<PackageCache>) -> Result<Self, PkgError> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PkgError::registry(format!("Invalid registry URL '{base_url}': {e}")))?;

        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(32) // More connections for parallel fetches
            .user_agent(concat!("howth/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| PkgError::registry(format!("Failed to create HTTP client: {e}")))?;

        // Ensure cache directories exist
        if let Some(ref cache) = disk_cache {
            let _ = cache.ensure_dirs();
        }

        Ok(Self {
            base_url,
            http,
            shared: Arc::new(SharedState {
                memory_cache: RwLock::new(HashMap::new()),
                disk_cache,
            }),
            scoped_registries: Arc::new(Vec::new()),
        })
    }

    /// Create a client with persistent cache.
    #[must_use]
    pub fn with_cache(self, cache: PackageCache) -> Self {
        // Ensure cache directories exist
        let _ = cache.ensure_dirs();

        Self {
            base_url: self.base_url,
            http: self.http,
            shared: Arc::new(SharedState {
                memory_cache: RwLock::new(HashMap::new()),
                disk_cache: Some(cache),
            }),
            scoped_registries: self.scoped_registries,
        }
    }

    /// Load `.npmrc` files from the project directory and configure scoped registries.
    #[must_use]
    pub fn with_npmrc(self, project_dir: &Path) -> Self {
        let config = load_npmrc_files(project_dir);
        let registries = resolve_scoped_registries(&config);

        Self {
            scoped_registries: Arc::new(registries),
            ..self
        }
    }

    /// Find a scoped registry for a package name (e.g., `@tiptap-pro/extension-foo`).
    #[must_use]
    pub fn find_scoped_registry(&self, name: &str) -> Option<&ScopedRegistry> {
        if !name.starts_with('@') {
            return None;
        }
        // Extract scope: "@scope/package" -> "@scope"
        let scope = name.split('/').next()?;
        self.scoped_registries.iter().find(|r| r.scope == scope)
    }

    /// Get the auth token for a package name, if it has a scoped registry with auth.
    #[must_use]
    pub fn auth_token_for(&self, name: &str) -> Option<&str> {
        self.find_scoped_registry(name)
            .and_then(|r| r.auth_token.as_deref())
    }

    /// Create a client using the registry URL from environment or default.
    ///
    /// # Errors
    /// Returns an error if the client cannot be created.
    pub fn from_env() -> Result<Self, PkgError> {
        let url = std::env::var(REGISTRY_ENV).unwrap_or_else(|_| DEFAULT_REGISTRY.to_string());
        Self::new(&url)
    }

    /// Create a client from environment with persistent cache.
    pub fn from_env_with_cache(cache: PackageCache) -> Result<Self, PkgError> {
        let url = std::env::var(REGISTRY_ENV).unwrap_or_else(|_| DEFAULT_REGISTRY.to_string());
        Self::new_with_cache(&url, Some(cache))
    }

    /// Get the base URL.
    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Get the HTTP client (for reuse in tarball downloads).
    #[must_use]
    pub fn http(&self) -> &Client {
        &self.http
    }

    /// Load cached packument from disk.
    fn load_cached_packument(&self, name: &str) -> Option<CachedPackument> {
        let cache = self.shared.disk_cache.as_ref()?;
        let path = cache.packument_path(name);

        if !path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save packument to disk cache.
    fn save_cached_packument(&self, name: &str, cached: &CachedPackument) {
        if let Some(cache) = &self.shared.disk_cache {
            let path = cache.packument_path(name);

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            // Write atomically via temp file
            let content = match serde_json::to_string(cached) {
                Ok(c) => c,
                Err(_) => return,
            };

            let temp_path = path.with_extension("json.tmp");
            if std::fs::write(&temp_path, &content).is_ok() {
                let _ = std::fs::rename(&temp_path, &path);
            }
        }
    }

    /// Fetch the packument (package metadata) for a package.
    ///
    /// Caching strategy:
    /// 1. Check memory cache - return immediately if found
    /// 2. Check disk cache - if fresh (< 5 min), return without network
    /// 3. If disk cache exists but stale, send `ETag` for 304 validation
    /// 4. Otherwise fetch full packument (abbreviated format)
    ///
    /// # Errors
    /// Returns an error if the request fails or the package is not found.
    pub async fn fetch_packument(&self, name: &str) -> Result<Value, PkgError> {
        // 1. Check memory cache first (fastest path)
        {
            let memory = self.shared.memory_cache.read().await;
            if let Some(cached) = memory.get(name) {
                return Ok(cached.data.clone());
            }
        }

        // 2. Check disk cache
        let disk_cached = self.load_cached_packument(name);

        // If disk cache is fresh, use it without network request
        if let Some(ref cached) = disk_cached {
            if cached.is_fresh() {
                // Update memory cache and return
                let mut memory = self.shared.memory_cache.write().await;
                memory.insert(name.to_string(), cached.clone());
                return Ok(cached.data.clone());
            }
        }

        // 3. Need network request - prepare conditional headers if we have cached data
        let cached_etag = disk_cached.as_ref().and_then(|c| c.etag.clone());

        // URL-encode the name for scoped packages
        let encoded_name = if name.starts_with('@') {
            name.replace('/', "%2F")
        } else {
            name.to_string()
        };

        // Check for a scoped registry override
        let scoped = self.find_scoped_registry(name);
        let base = scoped.map_or(&self.base_url, |r| &r.registry_url);

        let url = base
            .join(&encoded_name)
            .map_err(|e| PkgError::registry(format!("Failed to build URL for '{name}': {e}")))?;

        // Build request with abbreviated packument header and conditional ETag
        let mut request = self
            .http
            .get(url.as_str())
            .header("Accept", ABBREVIATED_ACCEPT);

        // Attach Bearer auth for scoped registries
        if let Some(reg) = scoped {
            if let Some(ref token) = reg.auth_token {
                request = request.header("Authorization", format!("Bearer {token}"));
            }
        }

        if let Some(etag) = &cached_etag {
            request = request.header("If-None-Match", etag);
        }

        let response = request.send().await?;
        let status = response.status();

        // 4. Handle 304 Not Modified - use cached data, update timestamp
        if status == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(mut cached) = disk_cached {
                // Update timestamp to mark as fresh
                cached.cached_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // Save updated timestamp to disk
                self.save_cached_packument(name, &cached);

                // Update memory cache
                {
                    let mut memory = self.shared.memory_cache.write().await;
                    memory.insert(name.to_string(), cached.clone());
                }
                return Ok(cached.data);
            }
            // Shouldn't happen, but fetch fresh if no cached data
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(PkgError::not_found(name));
        }

        if !status.is_success() {
            return Err(PkgError::registry(format!(
                "Registry returned status {status} for '{name}'"
            )));
        }

        // 5. Parse new packument
        let etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        let json: Value = response.json().await?;

        // Create cached entry
        let cached = CachedPackument {
            data: json.clone(),
            etag,
            cached_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        // Save to disk cache
        self.save_cached_packument(name, &cached);

        // Save to memory cache
        {
            let mut memory = self.shared.memory_cache.write().await;
            memory.insert(name.to_string(), cached);
        }

        Ok(json)
    }

    /// Clear the in-memory packument cache.
    pub async fn clear_memory_cache(&self) {
        let mut memory = self.shared.memory_cache.write().await;
        memory.clear();
    }

    /// Get stats about the cache.
    pub async fn cache_stats(&self) -> (usize, usize) {
        let memory_count = self.shared.memory_cache.read().await.len();
        let disk_count = self
            .shared
            .disk_cache
            .as_ref()
            .and_then(|c| {
                let path = c.root().join("packuments");
                std::fs::read_dir(path).ok()
            })
            .map_or(0, |entries| {
                entries.filter_map(std::result::Result::ok).count()
            });
        (memory_count, disk_count)
    }
}

/// Extract the latest version from a packument.
#[must_use]
pub fn get_latest_version(packument: &serde_json::Value) -> Option<&str> {
    packument.get("dist-tags")?.get("latest")?.as_str()
}

/// Extract the tarball URL for a specific version.
#[must_use]
pub fn get_tarball_url<'a>(packument: &'a serde_json::Value, version: &str) -> Option<&'a str> {
    packument
        .get("versions")?
        .get(version)?
        .get("dist")?
        .get("tarball")?
        .as_str()
}

/// Get all available version strings from a packument.
#[must_use]
pub fn get_versions(packument: &serde_json::Value) -> Vec<&str> {
    packument
        .get("versions")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_latest_version() {
        let packument = serde_json::json!({
            "name": "react",
            "dist-tags": {
                "latest": "18.2.0",
                "next": "19.0.0-rc.0"
            }
        });

        assert_eq!(get_latest_version(&packument), Some("18.2.0"));
    }

    #[test]
    fn test_get_tarball_url() {
        let packument = serde_json::json!({
            "name": "react",
            "versions": {
                "18.2.0": {
                    "dist": {
                        "tarball": "https://registry.npmjs.org/react/-/react-18.2.0.tgz",
                        "shasum": "abc123"
                    }
                }
            }
        });

        assert_eq!(
            get_tarball_url(&packument, "18.2.0"),
            Some("https://registry.npmjs.org/react/-/react-18.2.0.tgz")
        );
        assert_eq!(get_tarball_url(&packument, "17.0.0"), None);
    }

    #[test]
    fn test_get_versions() {
        let packument = serde_json::json!({
            "name": "react",
            "versions": {
                "18.2.0": {},
                "18.1.0": {},
                "17.0.2": {}
            }
        });

        let versions = get_versions(&packument);
        assert_eq!(versions.len(), 3);
        assert!(versions.contains(&"18.2.0"));
        assert!(versions.contains(&"18.1.0"));
        assert!(versions.contains(&"17.0.2"));
    }

    #[test]
    fn test_client_creation() {
        let client = RegistryClient::new(DEFAULT_REGISTRY);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_invalid_url() {
        let client = RegistryClient::new("not-a-url");
        assert!(client.is_err());
    }

    #[test]
    fn test_cached_packument_freshness() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Fresh cache (just created)
        let fresh = CachedPackument {
            data: serde_json::json!({}),
            etag: None,
            cached_at: now,
        };
        assert!(fresh.is_fresh());

        // Stale cache (10 minutes old)
        let stale = CachedPackument {
            data: serde_json::json!({}),
            etag: None,
            cached_at: now - 600,
        };
        assert!(!stale.is_fresh());
    }
}
