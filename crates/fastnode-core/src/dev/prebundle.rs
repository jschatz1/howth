//! Dependency pre-bundling for dev serving.
//!
//! Scans entry points for bare imports (node_modules packages) and bundles
//! each dependency into `.howth/deps/` so the browser doesn't need to make
//! hundreds of requests for individual node_modules files.
//!
//! Pre-bundled deps are served at `/@modules/{pkg}` URLs.

use crate::bundler::{BundleFormat, BundleOptions, Bundler};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Pre-bundled dependency.
#[derive(Debug, Clone)]
pub struct PreBundledDep {
    /// Package name (e.g., "react", "@scope/pkg").
    pub name: String,
    /// Path to the pre-bundled file.
    pub output_path: PathBuf,
    /// Bundled source code (cached in memory for fast serving).
    pub code: String,
}

/// Dependency pre-bundler.
///
/// Scans project source for bare imports and pre-bundles each npm dependency
/// into a single ESM file for efficient browser loading.
pub struct PreBundler {
    /// Project root directory.
    root: PathBuf,
    /// Output directory for pre-bundled deps.
    deps_dir: PathBuf,
    /// Pre-bundled deps cache: package name → PreBundledDep.
    deps: HashMap<String, PreBundledDep>,
}

impl PreBundler {
    /// Create a new pre-bundler.
    pub fn new(root: PathBuf) -> Self {
        let deps_dir = root.join(".howth").join("deps");
        Self {
            root,
            deps_dir,
            deps: HashMap::new(),
        }
    }

    /// Scan entry source code for bare import specifiers.
    ///
    /// Returns a set of package names found.
    pub fn scan_bare_imports(&self, source: &str) -> HashSet<String> {
        let mut bare_imports = HashSet::new();

        for line in source.lines() {
            let trimmed = line.trim();

            // Static imports and re-exports
            if (trimmed.starts_with("import ") || trimmed.starts_with("export "))
                && trimmed.contains(" from ")
            {
                if let Some(specifier) = extract_specifier_from_line(trimmed) {
                    if is_bare_specifier(&specifier) {
                        let pkg = package_name_from_specifier(&specifier);
                        bare_imports.insert(pkg);
                    }
                }
            }

            // Dynamic imports
            if trimmed.contains("import(") {
                if let Some(specifier) = extract_dynamic_specifier(trimmed) {
                    if is_bare_specifier(&specifier) {
                        let pkg = package_name_from_specifier(&specifier);
                        bare_imports.insert(pkg);
                    }
                }
            }
        }

        bare_imports
    }

    /// Scan a file and all its dependencies recursively for bare imports.
    pub fn scan_file_recursive(&self, entry: &Path) -> HashSet<String> {
        let mut bare_imports = HashSet::new();
        let mut visited = HashSet::new();
        let mut queue = vec![entry.to_path_buf()];

        while let Some(path) = queue.pop() {
            let path_str = path.display().to_string();
            if visited.contains(&path_str) {
                continue;
            }
            visited.insert(path_str);

            if let Ok(source) = std::fs::read_to_string(&path) {
                let found = self.scan_bare_imports(&source);
                bare_imports.extend(found);

                // Also follow relative imports to scan more files
                for line in source.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("import ") && trimmed.contains(" from ") {
                        if let Some(specifier) = extract_specifier_from_line(trimmed) {
                            if specifier.starts_with("./") || specifier.starts_with("../") {
                                if let Some(parent) = path.parent() {
                                    let resolved = parent.join(&specifier);
                                    // Try common extensions
                                    for ext in &["", ".ts", ".tsx", ".js", ".jsx"] {
                                        let with_ext = if ext.is_empty() {
                                            resolved.clone()
                                        } else {
                                            PathBuf::from(format!("{}{}", resolved.display(), ext))
                                        };
                                        if with_ext.exists() && with_ext.is_file() {
                                            queue.push(with_ext);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        bare_imports
    }

    /// Pre-bundle all discovered dependencies.
    ///
    /// Creates `.howth/deps/{pkg}.js` files and populates the in-memory cache.
    pub fn bundle_deps(&mut self, packages: &HashSet<String>) -> Result<(), PreBundleError> {
        if packages.is_empty() {
            return Ok(());
        }

        // Create output directory
        std::fs::create_dir_all(&self.deps_dir).map_err(|e| PreBundleError {
            message: format!("Failed to create deps dir: {}", e),
            package: None,
        })?;

        let bundler = Bundler::with_cwd(&self.root);
        let options = BundleOptions {
            format: BundleFormat::Esm,
            treeshake: false, // Don't treeshake deps (we need all exports)
            minify: false,    // No minification in dev
            ..Default::default()
        };

        for pkg in packages {
            match self.bundle_single_dep(pkg, &bundler, &options) {
                Ok(dep) => {
                    self.deps.insert(pkg.clone(), dep);
                }
                Err(e) => {
                    // Log but don't fail the whole process
                    eprintln!("  Warning: Failed to pre-bundle '{}': {}", pkg, e.message);
                }
            }
        }

        Ok(())
    }

    /// Bundle a single dependency.
    fn bundle_single_dep(
        &self,
        pkg: &str,
        bundler: &Bundler,
        options: &BundleOptions,
    ) -> Result<PreBundledDep, PreBundleError> {
        // Find the package entry point in node_modules
        let node_modules = self.root.join("node_modules").join(pkg);
        if !node_modules.exists() {
            return Err(PreBundleError {
                message: format!("Package not found in node_modules: {}", pkg),
                package: Some(pkg.to_string()),
            });
        }

        // Create a virtual entry that re-exports everything
        let entry_code = format!("export * from '{}';", pkg);
        let entry_path = self
            .deps_dir
            .join(format!("_entry_{}.js", sanitize_pkg_name(pkg)));

        std::fs::write(&entry_path, &entry_code).map_err(|e| PreBundleError {
            message: format!("Failed to write entry: {}", e),
            package: Some(pkg.to_string()),
        })?;

        // Bundle it
        let result = bundler
            .bundle(&entry_path, &self.root, options)
            .map_err(|e| PreBundleError {
                message: format!("Bundle error: {}", e),
                package: Some(pkg.to_string()),
            })?;

        // Write output
        let output_path = self.deps_dir.join(format!("{}.js", sanitize_pkg_name(pkg)));
        std::fs::write(&output_path, &result.code).map_err(|e| PreBundleError {
            message: format!("Failed to write output: {}", e),
            package: Some(pkg.to_string()),
        })?;

        // Clean up entry
        let _ = std::fs::remove_file(&entry_path);

        Ok(PreBundledDep {
            name: pkg.to_string(),
            output_path,
            code: result.code,
        })
    }

    /// Get a pre-bundled dependency by package name.
    pub fn get(&self, pkg: &str) -> Option<&PreBundledDep> {
        self.deps.get(pkg)
    }

    /// Check if a package has been pre-bundled.
    pub fn has(&self, pkg: &str) -> bool {
        self.deps.contains_key(pkg)
    }

    /// Get all pre-bundled package names.
    pub fn packages(&self) -> impl Iterator<Item = &String> {
        self.deps.keys()
    }
}

/// Extract the string literal from a `from 'xxx'` clause.
fn extract_specifier_from_line(line: &str) -> Option<String> {
    let from_idx = line.find(" from ")?;
    let after_from = &line[from_idx + 6..];
    let trimmed = after_from.trim();

    let quote = trimmed.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &trimmed[1..];
    let end_idx = inner.find(quote)?;
    Some(inner[..end_idx].to_string())
}

/// Extract specifier from a dynamic `import('xxx')` call.
fn extract_dynamic_specifier(line: &str) -> Option<String> {
    let start = line.find("import(")?;
    let after = &line[start + 7..];
    let trimmed = after.trim();

    let quote = trimmed.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &trimmed[1..];
    let end_idx = inner.find(quote)?;
    Some(inner[..end_idx].to_string())
}

/// Check if a specifier is a bare import (not relative, not absolute).
fn is_bare_specifier(specifier: &str) -> bool {
    !specifier.starts_with('.')
        && !specifier.starts_with('/')
        && !specifier.starts_with('\0')
        && !specifier.starts_with("node:")
        && !specifier.starts_with("data:")
}

/// Get the package name from a specifier (handles subpaths and scoped packages).
fn package_name_from_specifier(specifier: &str) -> String {
    if specifier.starts_with('@') {
        // Scoped: @scope/pkg or @scope/pkg/subpath
        let parts: Vec<&str> = specifier.splitn(3, '/').collect();
        if parts.len() >= 2 {
            format!("{}/{}", parts[0], parts[1])
        } else {
            specifier.to_string()
        }
    } else {
        // Regular: pkg or pkg/subpath
        specifier.split('/').next().unwrap_or(specifier).to_string()
    }
}

/// Sanitize a package name for use as a filename.
fn sanitize_pkg_name(pkg: &str) -> String {
    pkg.replace('/', "__").replace('@', "")
}

/// Error during pre-bundling.
#[derive(Debug)]
pub struct PreBundleError {
    pub message: String,
    pub package: Option<String>,
}

impl std::fmt::Display for PreBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pkg) = &self.package {
            write!(f, "{} (package: {})", self.message, pkg)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for PreBundleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_bare_imports() {
        let prebundler = PreBundler::new(PathBuf::from("/project"));
        let source = r#"
import React from 'react';
import { useState, useEffect } from 'react';
import lodash from 'lodash';
import './App.css';
import { Button } from './components/Button';
import path from 'node:path';
export { helper } from '@scope/utils';
const lazy = import('lazy-module');
"#;

        let imports = prebundler.scan_bare_imports(source);

        assert!(imports.contains("react"));
        assert!(imports.contains("lodash"));
        assert!(imports.contains("@scope/utils"));
        assert!(imports.contains("lazy-module"));
        assert!(!imports.contains("./App.css"));
        assert!(!imports.contains("./components/Button"));
        assert!(!imports.contains("node:path"));
    }

    #[test]
    fn test_package_name_from_specifier() {
        assert_eq!(package_name_from_specifier("react"), "react");
        assert_eq!(package_name_from_specifier("react/jsx-runtime"), "react");
        assert_eq!(package_name_from_specifier("@scope/pkg"), "@scope/pkg");
        assert_eq!(
            package_name_from_specifier("@scope/pkg/utils"),
            "@scope/pkg"
        );
    }

    #[test]
    fn test_is_bare_specifier() {
        assert!(is_bare_specifier("react"));
        assert!(is_bare_specifier("@scope/pkg"));
        assert!(!is_bare_specifier("./local"));
        assert!(!is_bare_specifier("../parent"));
        assert!(!is_bare_specifier("/absolute"));
        assert!(!is_bare_specifier("node:fs"));
    }

    #[test]
    fn test_sanitize_pkg_name() {
        assert_eq!(sanitize_pkg_name("react"), "react");
        assert_eq!(sanitize_pkg_name("@scope/pkg"), "scope__pkg");
    }
}
