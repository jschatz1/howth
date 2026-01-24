//! npm registry client.

use super::error::PkgError;
use reqwest::Client;
use std::time::Duration;
use url::Url;

/// Default npm registry URL.
pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";

/// Environment variable to override registry URL.
pub const REGISTRY_ENV: &str = "FASTNODE_NPM_REGISTRY";

/// Registry client for fetching package metadata.
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: Url,
    http: Client,
}

impl RegistryClient {
    /// Create a new registry client with the given base URL.
    ///
    /// # Errors
    /// Returns an error if the URL is invalid or the HTTP client cannot be created.
    pub fn new(base_url: &str) -> Result<Self, PkgError> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PkgError::registry(format!("Invalid registry URL '{base_url}': {e}")))?;

        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("howth/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| PkgError::registry(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self { base_url, http })
    }

    /// Create a client using the registry URL from environment or default.
    ///
    /// # Errors
    /// Returns an error if the client cannot be created.
    pub fn from_env() -> Result<Self, PkgError> {
        let url = std::env::var(REGISTRY_ENV).unwrap_or_else(|_| DEFAULT_REGISTRY.to_string());
        Self::new(&url)
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

    /// Fetch the packument (package metadata) for a package.
    ///
    /// # Errors
    /// Returns an error if the request fails or the package is not found.
    pub async fn fetch_packument(&self, name: &str) -> Result<serde_json::Value, PkgError> {
        // URL-encode the name for scoped packages
        let encoded_name = if name.starts_with('@') {
            name.replace('/', "%2F")
        } else {
            name.to_string()
        };

        let url = self
            .base_url
            .join(&encoded_name)
            .map_err(|e| PkgError::registry(format!("Failed to build URL for '{name}': {e}")))?;

        let response = self.http.get(url.as_str()).send().await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(PkgError::not_found(name));
        }

        if !response.status().is_success() {
            return Err(PkgError::registry(format!(
                "Registry returned status {} for '{name}'",
                response.status()
            )));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json)
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
}
