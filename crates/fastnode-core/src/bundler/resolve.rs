//! Import specifier resolution.
//!
//! Resolves import specifiers to absolute file paths.
//!
//! ## Specifier Types
//!
//! - Relative: `./utils`, `../lib/foo`
//! - Absolute: `/abs/path/to/module`
//! - Bare: `lodash`, `@scope/pkg`, `react/jsx-runtime`

#![allow(clippy::manual_strip)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::unused_self)]
#![allow(clippy::self_only_used_in_recursion)]

use std::path::{Path, PathBuf};

/// Result of resolving an import specifier.
#[derive(Debug, Clone)]
pub enum ResolveResult {
    /// Successfully resolved to a file path.
    Found(PathBuf),
    /// External module (should not be bundled).
    External(String),
    /// Built-in module (node:fs, etc.).
    Builtin(String),
}

/// Error during resolution.
#[derive(Debug)]
pub struct ResolveError {
    pub specifier: String,
    pub from: String,
    pub message: String,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cannot resolve '{}' from '{}': {}",
            self.specifier, self.from, self.message
        )
    }
}

impl std::error::Error for ResolveError {}

/// Import resolver.
#[derive(Debug, Default)]
pub struct Resolver {
    /// Cached resolutions for performance.
    cache: std::sync::RwLock<std::collections::HashMap<(String, String), ResolveResult>>,
}

impl Resolver {
    /// Create a new resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve an import specifier.
    ///
    /// # Arguments
    /// - `specifier`: The import specifier (e.g., "./utils", "lodash")
    /// - `from`: The file containing the import
    /// - `cwd`: The project root directory
    pub fn resolve(
        &self,
        specifier: &str,
        from: &Path,
        cwd: &Path,
    ) -> Result<ResolveResult, ResolveError> {
        // Check cache
        let cache_key = (specifier.to_string(), from.display().to_string());
        if let Some(cached) = self.cache.read().unwrap().get(&cache_key) {
            return Ok(cached.clone());
        }

        let result = self.resolve_uncached(specifier, from, cwd)?;

        // Cache result
        self.cache
            .write()
            .unwrap()
            .insert(cache_key, result.clone());

        Ok(result)
    }

    fn resolve_uncached(
        &self,
        specifier: &str,
        from: &Path,
        cwd: &Path,
    ) -> Result<ResolveResult, ResolveError> {
        // Handle built-in modules
        if specifier.starts_with("node:") {
            return Ok(ResolveResult::Builtin(specifier.to_string()));
        }

        // Handle relative imports
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_relative(specifier, from);
        }

        // Handle absolute imports
        if specifier.starts_with('/') {
            return self.resolve_absolute(specifier);
        }

        // Handle bare specifiers (node_modules)
        self.resolve_bare(specifier, from, cwd)
    }

    /// Resolve a relative import.
    fn resolve_relative(
        &self,
        specifier: &str,
        from: &Path,
    ) -> Result<ResolveResult, ResolveError> {
        let from_dir = from.parent().unwrap_or(Path::new("."));
        let target = from_dir.join(specifier);

        self.resolve_file_or_directory(&target, specifier, from)
    }

    /// Resolve an absolute import.
    fn resolve_absolute(&self, specifier: &str) -> Result<ResolveResult, ResolveError> {
        let target = PathBuf::from(specifier);

        if target.exists() {
            let resolved = dunce::canonicalize(target).map_err(|e| ResolveError {
                specifier: specifier.to_string(),
                from: String::new(),
                message: e.to_string(),
            })?;
            return Ok(ResolveResult::Found(resolved));
        }

        // Try with extensions
        for ext in &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"] {
            let with_ext = target.with_extension(&ext[1..]);
            if with_ext.exists() {
                let resolved = dunce::canonicalize(with_ext).map_err(|e| ResolveError {
                    specifier: specifier.to_string(),
                    from: "".to_string(),
                    message: e.to_string(),
                })?;
                return Ok(ResolveResult::Found(resolved));
            }
        }

        Err(ResolveError {
            specifier: specifier.to_string(),
            from: "".to_string(),
            message: "File not found".to_string(),
        })
    }

    /// Resolve a bare specifier (node_modules lookup).
    fn resolve_bare(
        &self,
        specifier: &str,
        from: &Path,
        cwd: &Path,
    ) -> Result<ResolveResult, ResolveError> {
        // Split package name from subpath
        let (pkg_name, subpath) = self.parse_bare_specifier(specifier);

        // Walk up from `from` looking for node_modules
        let mut current = from.parent();
        while let Some(dir) = current {
            let node_modules = dir.join("node_modules").join(&pkg_name);

            if node_modules.exists() {
                // Found the package directory
                let pkg_json = node_modules.join("package.json");

                if pkg_json.exists() {
                    // Read package.json to find entry point
                    if let Ok(entry) =
                        self.resolve_package_entry(&node_modules, &pkg_json, subpath.as_deref())
                    {
                        return Ok(ResolveResult::Found(entry));
                    }
                }

                // Fallback: try index.js or subpath directly
                if let Some(ref sub) = subpath {
                    let target = node_modules.join(sub);
                    if let Ok(result) = self.resolve_file_or_directory(&target, specifier, from) {
                        return Ok(result);
                    }
                } else {
                    // Try common entry points
                    for entry in &["index.js", "index.ts", "index.mjs"] {
                        let target = node_modules.join(entry);
                        if target.exists() {
                            return Ok(ResolveResult::Found(dunce::canonicalize(target).unwrap()));
                        }
                    }
                }
            }

            // Stop at project root
            if dir == cwd {
                break;
            }
            current = dir.parent();
        }

        // Not found - might be external or missing
        Err(ResolveError {
            specifier: specifier.to_string(),
            from: from.display().to_string(),
            message: format!("Cannot find package '{}' in node_modules", pkg_name),
        })
    }

    /// Parse a bare specifier into package name and subpath.
    fn parse_bare_specifier(&self, specifier: &str) -> (String, Option<String>) {
        if specifier.starts_with('@') {
            // Scoped package: @scope/pkg or @scope/pkg/subpath
            let parts: Vec<&str> = specifier.splitn(3, '/').collect();
            if parts.len() >= 2 {
                let pkg = format!("{}/{}", parts[0], parts[1]);
                let subpath = if parts.len() > 2 {
                    Some(parts[2].to_string())
                } else {
                    None
                };
                (pkg, subpath)
            } else {
                (specifier.to_string(), None)
            }
        } else {
            // Regular package: pkg or pkg/subpath
            let parts: Vec<&str> = specifier.splitn(2, '/').collect();
            let pkg = parts[0].to_string();
            let subpath = parts.get(1).map(|s| s.to_string());
            (pkg, subpath)
        }
    }

    /// Resolve the entry point from package.json.
    fn resolve_package_entry(
        &self,
        pkg_dir: &Path,
        pkg_json: &Path,
        subpath: Option<&str>,
    ) -> Result<PathBuf, ResolveError> {
        let content = std::fs::read_to_string(pkg_json).map_err(|e| ResolveError {
            specifier: "".to_string(),
            from: pkg_json.display().to_string(),
            message: e.to_string(),
        })?;

        let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| ResolveError {
            specifier: "".to_string(),
            from: pkg_json.display().to_string(),
            message: e.to_string(),
        })?;

        // Handle subpath exports
        if let Some(sub) = subpath {
            // Check exports field
            if let Some(exports) = json.get("exports") {
                if let Some(entry) = self.resolve_exports(exports, &format!("./{}", sub)) {
                    let target = pkg_dir.join(entry);
                    if target.exists() {
                        return dunce::canonicalize(target).map_err(|e| ResolveError {
                            specifier: "".to_string(),
                            from: pkg_json.display().to_string(),
                            message: e.to_string(),
                        });
                    }
                }
            }

            // Fallback: try direct path
            let target = pkg_dir.join(sub);
            return self
                .resolve_file_or_directory(&target, sub, pkg_dir)
                .map(|r| match r {
                    ResolveResult::Found(p) => p,
                    _ => unreachable!(),
                });
        }

        // Main entry point
        // Check exports["."]
        if let Some(exports) = json.get("exports") {
            if let Some(entry) = self.resolve_exports(exports, ".") {
                let target = pkg_dir.join(entry);
                if target.exists() {
                    return dunce::canonicalize(target).map_err(|e| ResolveError {
                        specifier: "".to_string(),
                        from: pkg_json.display().to_string(),
                        message: e.to_string(),
                    });
                }
            }
        }

        // Check module field (ESM)
        if let Some(module) = json.get("module").and_then(|v| v.as_str()) {
            let target = pkg_dir.join(module);
            if target.exists() {
                return dunce::canonicalize(target).map_err(|e| ResolveError {
                    specifier: "".to_string(),
                    from: pkg_json.display().to_string(),
                    message: e.to_string(),
                });
            }
        }

        // Check main field
        if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
            let target = pkg_dir.join(main);
            if target.exists() {
                return dunce::canonicalize(target).map_err(|e| ResolveError {
                    specifier: "".to_string(),
                    from: pkg_json.display().to_string(),
                    message: e.to_string(),
                });
            }
        }

        // Fallback to index.js
        let index = pkg_dir.join("index.js");
        if index.exists() {
            return dunce::canonicalize(index).map_err(|e| ResolveError {
                specifier: "".to_string(),
                from: pkg_json.display().to_string(),
                message: e.to_string(),
            });
        }

        Err(ResolveError {
            specifier: "".to_string(),
            from: pkg_json.display().to_string(),
            message: "No entry point found in package.json".to_string(),
        })
    }

    /// Resolve exports field (simplified).
    fn resolve_exports(&self, exports: &serde_json::Value, subpath: &str) -> Option<String> {
        match exports {
            serde_json::Value::String(s) => {
                if subpath == "." {
                    Some(s.clone())
                } else {
                    None
                }
            }
            serde_json::Value::Object(map) => {
                // Check for exact subpath match
                if let Some(value) = map.get(subpath) {
                    return self.resolve_export_value(value);
                }
                // Check for "." entry with conditions
                if subpath == "." {
                    if let Some(value) = map.get(".") {
                        return self.resolve_export_value(value);
                    }
                    // Check for conditional exports at root
                    if let Some(value) = map.get("import").or(map.get("default")) {
                        return self.resolve_export_value(value);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Resolve a single export value (handles conditions).
    fn resolve_export_value(&self, value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(map) => {
                // Prefer: import > default > require
                map.get("import")
                    .or(map.get("default"))
                    .or(map.get("require"))
                    .and_then(|v| self.resolve_export_value(v))
            }
            _ => None,
        }
    }

    /// Resolve a path that might be a file or directory.
    fn resolve_file_or_directory(
        &self,
        target: &Path,
        specifier: &str,
        from: &Path,
    ) -> Result<ResolveResult, ResolveError> {
        // If it exists as-is
        if target.is_file() {
            return Ok(ResolveResult::Found(dunce::canonicalize(target).unwrap()));
        }

        // Try with extensions
        for ext in &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"] {
            let with_ext = PathBuf::from(format!("{}{}", target.display(), ext));
            if with_ext.is_file() {
                return Ok(ResolveResult::Found(dunce::canonicalize(with_ext).unwrap()));
            }
        }

        // Try as directory with index file
        if target.is_dir() {
            for index in &["index.ts", "index.tsx", "index.js", "index.jsx"] {
                let index_path = target.join(index);
                if index_path.is_file() {
                    return Ok(ResolveResult::Found(
                        dunce::canonicalize(index_path).unwrap(),
                    ));
                }
            }
        }

        Err(ResolveError {
            specifier: specifier.to_string(),
            from: from.display().to_string(),
            message: "File not found".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_bare_specifier() {
        let resolver = Resolver::new();

        let (pkg, sub) = resolver.parse_bare_specifier("lodash");
        assert_eq!(pkg, "lodash");
        assert!(sub.is_none());

        let (pkg, sub) = resolver.parse_bare_specifier("lodash/get");
        assert_eq!(pkg, "lodash");
        assert_eq!(sub, Some("get".to_string()));

        let (pkg, sub) = resolver.parse_bare_specifier("@types/node");
        assert_eq!(pkg, "@types/node");
        assert!(sub.is_none());

        let (pkg, sub) = resolver.parse_bare_specifier("@babel/core/lib/parse");
        assert_eq!(pkg, "@babel/core");
        assert_eq!(sub, Some("lib/parse".to_string()));
    }

    #[test]
    fn test_resolve_relative() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir(&src).unwrap();

        // Create files
        std::fs::write(src.join("index.ts"), "import './utils';").unwrap();
        std::fs::write(src.join("utils.ts"), "export const x = 1;").unwrap();

        let resolver = Resolver::new();
        let result = resolver.resolve("./utils", &src.join("index.ts"), dir.path());

        assert!(result.is_ok());
        if let ResolveResult::Found(path) = result.unwrap() {
            assert!(path.ends_with("utils.ts"));
        } else {
            panic!("Expected Found result");
        }
    }
}
