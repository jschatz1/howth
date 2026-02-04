//! Workspace support for monorepos.
//!
//! Parses the `workspaces` field from package.json and discovers workspace packages.
//! Supports glob patterns like `packages/*` and `apps/*`.

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A discovered workspace package.
#[derive(Debug, Clone)]
pub struct WorkspacePackage {
    /// Package name from package.json
    pub name: String,
    /// Absolute path to the workspace directory
    pub path: PathBuf,
    /// Version from package.json
    pub version: String,
}

/// Workspace configuration from root package.json.
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Root directory of the monorepo
    pub root: PathBuf,
    /// Map of package name -> workspace info
    pub packages: HashMap<String, WorkspacePackage>,
}

impl WorkspaceConfig {
    /// Check if a package name is a workspace package.
    #[must_use] 
    pub fn is_workspace_package(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// Get workspace package info by name.
    #[must_use] 
    pub fn get_package(&self, name: &str) -> Option<&WorkspacePackage> {
        self.packages.get(name)
    }
}

/// Detect and parse workspace configuration from a project root.
///
/// Returns `None` if the project doesn't use workspaces.
#[must_use] 
pub fn detect_workspaces(project_root: &Path) -> Option<WorkspaceConfig> {
    let package_json_path = project_root.join("package.json");
    let content = std::fs::read_to_string(&package_json_path).ok()?;
    let package: Value = serde_json::from_str(&content).ok()?;

    // Check for workspaces field
    let workspaces = package.get("workspaces")?;

    // Workspaces can be an array or an object with "packages" field
    let patterns: Vec<String> = match workspaces {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Value::Object(obj) => {
            // { "packages": ["packages/*"] } format (yarn-style)
            obj.get("packages")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => return None,
    };

    if patterns.is_empty() {
        return None;
    }

    // Expand glob patterns and discover packages
    let packages = discover_workspace_packages(project_root, &patterns);

    if packages.is_empty() {
        return None;
    }

    Some(WorkspaceConfig {
        root: project_root.to_path_buf(),
        packages,
    })
}

/// Expand glob patterns and discover workspace packages.
fn discover_workspace_packages(
    root: &Path,
    patterns: &[String],
) -> HashMap<String, WorkspacePackage> {
    let mut packages = HashMap::new();

    for pattern in patterns {
        // Expand the glob pattern
        let full_pattern = root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        // Use glob to expand the pattern
        if let Ok(entries) = glob::glob(&pattern_str) {
            for entry in entries.flatten() {
                if let Some(pkg) = read_workspace_package(&entry) {
                    packages.insert(pkg.name.clone(), pkg);
                }
            }
        }
    }

    packages
}

/// Read package info from a workspace directory.
fn read_workspace_package(dir: &Path) -> Option<WorkspacePackage> {
    if !dir.is_dir() {
        return None;
    }

    let package_json_path = dir.join("package.json");
    let content = std::fs::read_to_string(&package_json_path).ok()?;
    let package: Value = serde_json::from_str(&content).ok()?;

    let name = package.get("name")?.as_str()?.to_string();
    let version = package
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    Some(WorkspacePackage {
        name,
        path: dir.to_path_buf(),
        version,
    })
}

/// Link all workspace packages into a project's `node_modules`.
///
/// This creates symlinks from `node_modules`/<pkg-name> to the workspace directory.
pub fn link_workspace_packages(
    project_root: &Path,
    config: &WorkspaceConfig,
) -> Result<Vec<String>, super::error::PkgError> {
    use super::link::link_into_node_modules;

    let mut linked = Vec::new();

    for (name, pkg) in &config.packages {
        // Don't link a package into its own node_modules
        if project_root == pkg.path {
            continue;
        }

        link_into_node_modules(project_root, name, &pkg.path)?;
        linked.push(name.clone());
    }

    Ok(linked)
}

/// Find the workspace root by walking up the directory tree.
///
/// Returns the first directory containing a package.json with a "workspaces" field.
#[must_use] 
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();

    loop {
        let package_json = current.join("package.json");
        if package_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&package_json) {
                if let Ok(package) = serde_json::from_str::<Value>(&content) {
                    if package.get("workspaces").is_some() {
                        return Some(current);
                    }
                }
            }
        }

        if !current.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_detect_workspaces_array_format() {
        let root = tempdir().unwrap();

        // Create root package.json with workspaces
        fs::write(
            root.path().join("package.json"),
            r#"{"name": "monorepo", "workspaces": ["packages/*"]}"#,
        )
        .unwrap();

        // Create a workspace package
        let pkg_dir = root.path().join("packages").join("my-lib");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "@myorg/my-lib", "version": "1.0.0"}"#,
        )
        .unwrap();

        let config = detect_workspaces(root.path()).unwrap();
        assert!(config.is_workspace_package("@myorg/my-lib"));
        assert_eq!(config.packages.len(), 1);
    }

    #[test]
    fn test_detect_workspaces_object_format() {
        let root = tempdir().unwrap();

        // Create root package.json with yarn-style workspaces
        fs::write(
            root.path().join("package.json"),
            r#"{"name": "monorepo", "workspaces": {"packages": ["packages/*"]}}"#,
        )
        .unwrap();

        // Create a workspace package
        let pkg_dir = root.path().join("packages").join("utils");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "utils", "version": "2.0.0"}"#,
        )
        .unwrap();

        let config = detect_workspaces(root.path()).unwrap();
        assert!(config.is_workspace_package("utils"));
    }

    #[test]
    fn test_no_workspaces() {
        let root = tempdir().unwrap();

        fs::write(
            root.path().join("package.json"),
            r#"{"name": "regular-project"}"#,
        )
        .unwrap();

        assert!(detect_workspaces(root.path()).is_none());
    }

    #[test]
    fn test_find_workspace_root() {
        let root = tempdir().unwrap();

        // Create root with workspaces
        fs::write(
            root.path().join("package.json"),
            r#"{"name": "monorepo", "workspaces": ["packages/*"]}"#,
        )
        .unwrap();

        // Create nested package
        let nested = root.path().join("packages").join("nested").join("deep");
        fs::create_dir_all(&nested).unwrap();

        let found = find_workspace_root(&nested).unwrap();
        assert_eq!(found, root.path());
    }
}
