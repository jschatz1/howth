//! `.npmrc` file parser for scoped registry configuration.
//!
//! Parses `.npmrc` files to extract:
//! - `@scope:registry=URL` directives for routing scoped packages
//! - `//host/:_authToken=TOKEN` directives for registry authentication
//! - `${ENV_VAR}` expansion in token values

use std::collections::HashMap;
use std::path::Path;
use url::Url;

/// Parsed `.npmrc` configuration.
#[derive(Debug, Clone, Default)]
pub struct NpmrcConfig {
    /// Scope → registry URL mapping (e.g., `@tiptap-pro` → `https://registry.tiptap.dev/`).
    pub scoped_registries: HashMap<String, Url>,
    /// Host → auth token mapping (e.g., `registry.tiptap.dev` → `abc123`).
    pub auth_tokens: HashMap<String, String>,
}

/// A resolved scoped registry with its auth token.
#[derive(Debug, Clone)]
pub struct ScopedRegistry {
    /// The npm scope (e.g., `@tiptap-pro`).
    pub scope: String,
    /// The registry URL for this scope.
    pub registry_url: Url,
    /// Optional auth token for this registry.
    pub auth_token: Option<String>,
}

/// Parse a single `.npmrc` file's content.
///
/// Extracts `@scope:registry=URL` and `//host/:_authToken=TOKEN` directives.
/// Ignores comments (`#`, `;`) and blank lines. Supports `${ENV_VAR}` expansion
/// in token values.
#[must_use]
pub fn parse_npmrc(content: &str) -> NpmrcConfig {
    let mut config = NpmrcConfig::default();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        // Parse @scope:registry=URL
        if line.starts_with('@') {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                if let Some((scope, directive)) = key.split_once(':') {
                    if directive == "registry" {
                        // Ensure URL has trailing slash for proper joining
                        let url_str = if value.ends_with('/') {
                            value.to_string()
                        } else {
                            format!("{value}/")
                        };

                        if let Ok(url) = Url::parse(&url_str) {
                            config.scoped_registries.insert(scope.to_string(), url);
                        }
                    }
                }
            }
            continue;
        }

        // Parse //host/:_authToken=TOKEN  or  //host/path/:_authToken=TOKEN
        if line.starts_with("//") {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                // Strip leading "//" and trailing ":_authToken" (or "/:_authToken")
                if key.ends_with(":_authToken") {
                    let host_part = key
                        .strip_prefix("//")
                        .unwrap_or(key)
                        .strip_suffix(":_authToken")
                        .unwrap_or(key)
                        .trim_end_matches('/');

                    let token = expand_env_vars(value);
                    if !token.is_empty() {
                        config.auth_tokens.insert(host_part.to_string(), token);
                    }
                }
            }
        }
    }

    config
}

/// Load and merge `.npmrc` files from project directory up to home.
///
/// Priority order (first wins, no overwrite):
/// 1. `project_dir/.npmrc`
/// 2. Parent directories up to filesystem root
/// 3. `$HOME/.npmrc`
#[must_use]
pub fn load_npmrc_files(project_dir: &Path) -> NpmrcConfig {
    let mut merged = NpmrcConfig::default();

    // Walk from project_dir upward
    let mut dir = Some(project_dir.to_path_buf());
    while let Some(d) = dir {
        let npmrc_path = d.join(".npmrc");
        if npmrc_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&npmrc_path) {
                let parsed = parse_npmrc(&content);
                merge_config(&mut merged, &parsed);
            }
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }

    // Also check $HOME/.npmrc (may already be covered by the walk, but
    // handles cases where project_dir is not under HOME)
    if let Some(home) = home_dir() {
        let home_npmrc = home.join(".npmrc");
        if home_npmrc.is_file() {
            if let Ok(content) = std::fs::read_to_string(&home_npmrc) {
                let parsed = parse_npmrc(&content);
                merge_config(&mut merged, &parsed);
            }
        }
    }

    merged
}

/// Join scope→URL with host→token into final list of `ScopedRegistry`.
#[must_use]
pub fn resolve_scoped_registries(config: &NpmrcConfig) -> Vec<ScopedRegistry> {
    config
        .scoped_registries
        .iter()
        .map(|(scope, url)| {
            // Extract host from the registry URL to find the matching auth token
            let auth_token = url
                .host_str()
                .and_then(|host| {
                    // Try exact host match first, then host with path
                    let url_path = url.path().trim_end_matches('/');
                    let host_with_path = if url_path.is_empty() || url_path == "/" {
                        host.to_string()
                    } else {
                        format!("{host}{url_path}")
                    };

                    config
                        .auth_tokens
                        .get(&host_with_path)
                        .or_else(|| config.auth_tokens.get(host))
                })
                .cloned();

            ScopedRegistry {
                scope: scope.clone(),
                registry_url: url.clone(),
                auth_token,
            }
        })
        .collect()
}

/// Merge `source` into `target`, keeping existing entries (first wins).
fn merge_config(target: &mut NpmrcConfig, source: &NpmrcConfig) {
    for (scope, url) in &source.scoped_registries {
        target
            .scoped_registries
            .entry(scope.clone())
            .or_insert_with(|| url.clone());
    }
    for (host, token) in &source.auth_tokens {
        target
            .auth_tokens
            .entry(host.clone())
            .or_insert_with(|| token.clone());
    }
}

/// Expand `${ENV_VAR}` patterns in a string.
fn expand_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                var_name.push(ch);
            }
            if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            }
            // If env var not found, expand to empty string (matches npm behavior)
        } else {
            result.push(ch);
        }
    }

    result
}

/// Get the user's home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    // Try HOME first (Unix), then USERPROFILE (Windows)
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scoped_registry() {
        let content = "@tiptap-pro:registry=https://registry.tiptap.dev/\n";
        let config = parse_npmrc(content);
        assert_eq!(config.scoped_registries.len(), 1);
        assert_eq!(
            config.scoped_registries["@tiptap-pro"].as_str(),
            "https://registry.tiptap.dev/"
        );
    }

    #[test]
    fn test_parse_auth_token() {
        let content = "//registry.tiptap.dev/:_authToken=secret123\n";
        let config = parse_npmrc(content);
        assert_eq!(config.auth_tokens.len(), 1);
        assert_eq!(config.auth_tokens["registry.tiptap.dev"], "secret123");
    }

    #[test]
    fn test_parse_combined() {
        let content = "\
# Tiptap Pro
@tiptap-pro:registry=https://registry.tiptap.dev/
//registry.tiptap.dev/:_authToken=mytoken

; GitHub Packages
@myorg:registry=https://npm.pkg.github.com/
//npm.pkg.github.com/:_authToken=ghp_abc123
";
        let config = parse_npmrc(content);
        assert_eq!(config.scoped_registries.len(), 2);
        assert_eq!(config.auth_tokens.len(), 2);
        assert_eq!(
            config.scoped_registries["@myorg"].as_str(),
            "https://npm.pkg.github.com/"
        );
        assert_eq!(config.auth_tokens["npm.pkg.github.com"], "ghp_abc123");
    }

    #[test]
    fn test_comments_and_blank_lines() {
        let content = "\
# comment
; another comment

@scope:registry=https://example.com/
";
        let config = parse_npmrc(content);
        assert_eq!(config.scoped_registries.len(), 1);
    }

    #[test]
    fn test_trailing_slash_added() {
        let content = "@scope:registry=https://example.com\n";
        let config = parse_npmrc(content);
        assert_eq!(
            config.scoped_registries["@scope"].as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn test_env_var_expansion() {
        std::env::set_var("TEST_NPMRC_TOKEN", "expanded_value");
        let result = expand_env_vars("${TEST_NPMRC_TOKEN}");
        assert_eq!(result, "expanded_value");
        std::env::remove_var("TEST_NPMRC_TOKEN");
    }

    #[test]
    fn test_env_var_missing() {
        let result = expand_env_vars("${NONEXISTENT_VAR_12345}");
        assert_eq!(result, "");
    }

    #[test]
    fn test_resolve_scoped_registries() {
        let mut config = NpmrcConfig::default();
        config.scoped_registries.insert(
            "@tiptap-pro".to_string(),
            Url::parse("https://registry.tiptap.dev/").unwrap(),
        );
        config
            .auth_tokens
            .insert("registry.tiptap.dev".to_string(), "token123".to_string());

        let registries = resolve_scoped_registries(&config);
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0].scope, "@tiptap-pro");
        assert_eq!(
            registries[0].registry_url.as_str(),
            "https://registry.tiptap.dev/"
        );
        assert_eq!(registries[0].auth_token.as_deref(), Some("token123"));
    }

    #[test]
    fn test_resolve_scoped_registries_no_token() {
        let mut config = NpmrcConfig::default();
        config.scoped_registries.insert(
            "@public".to_string(),
            Url::parse("https://npm.example.com/").unwrap(),
        );

        let registries = resolve_scoped_registries(&config);
        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0].auth_token, None);
    }

    #[test]
    fn test_merge_first_wins() {
        let mut target = NpmrcConfig::default();
        target.scoped_registries.insert(
            "@scope".to_string(),
            Url::parse("https://first.com/").unwrap(),
        );

        let mut source = NpmrcConfig::default();
        source.scoped_registries.insert(
            "@scope".to_string(),
            Url::parse("https://second.com/").unwrap(),
        );

        merge_config(&mut target, &source);
        assert_eq!(
            target.scoped_registries["@scope"].as_str(),
            "https://first.com/"
        );
    }
}
