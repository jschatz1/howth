//! Dependency resolution and lockfile generation.
//!
//! Resolves dependencies from package.json and generates a lockfile.

use super::deps::read_package_deps;
use super::error::PkgError;
use super::lockfile::{
    LockDep, LockMeta, LockPackage, LockResolution, LockRoot, Lockfile, LOCKFILE_NAME,
    PKG_LOCK_SCHEMA_VERSION,
};
use super::registry::RegistryClient;
use super::version::resolve_version;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Options for dependency resolution.
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    /// Include devDependencies.
    pub include_dev: bool,
    /// Include optionalDependencies.
    pub include_optional: bool,
}

/// Result of resolving dependencies.
#[derive(Debug)]
pub struct ResolveResult {
    /// The generated lockfile.
    pub lockfile: Lockfile,
    /// Packages that were resolved.
    pub resolved_count: usize,
    /// Packages fetched from registry (not cached).
    pub fetched_count: usize,
}

/// Resolve dependencies and generate a lockfile.
///
/// # Arguments
/// * `project_root` - Path to the project directory containing package.json
/// * `registry` - Registry client for fetching packuments
/// * `options` - Resolution options
///
/// # Returns
/// A `ResolveResult` containing the generated lockfile.
pub async fn resolve_dependencies(
    project_root: &Path,
    registry: &RegistryClient,
    options: &ResolveOptions,
) -> Result<ResolveResult, PkgError> {
    let package_json_path = project_root.join("package.json");

    // Read root package.json
    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| PkgError::package_json_invalid(format!("Failed to read: {e}")))?;

    let pkg_json: Value = serde_json::from_str(&content)
        .map_err(|e| PkgError::package_json_invalid(format!("Invalid JSON: {e}")))?;

    let root_name = pkg_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed")
        .to_string();

    let root_version = pkg_json
        .get("version")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Read dependencies from package.json
    let pkg_deps = read_package_deps(&package_json_path, options.include_dev, options.include_optional)?;

    // Track resolved packages
    let mut packages: BTreeMap<String, LockPackage> = BTreeMap::new();
    let mut dependencies: BTreeMap<String, LockDep> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut fetched_count = 0;

    // Resolve each root dependency
    for (name, range) in &pkg_deps.deps {
        let kind = get_dep_kind(&pkg_json, name);

        // Resolve the package and its transitive deps
        let (version, _) = resolve_package_tree(
            name,
            range,
            registry,
            &mut packages,
            &mut visited,
            &mut fetched_count,
            0,
        )
        .await?;

        // Add to root dependencies
        let key = format!("{}@{}", name, version);
        dependencies.insert(
            name.clone(),
            LockDep::new(range.clone(), kind, key),
        );
    }

    // Build lockfile
    let lockfile = Lockfile {
        lockfile_version: PKG_LOCK_SCHEMA_VERSION,
        meta: LockMeta {
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            howth_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
        root: LockRoot::new(root_name, root_version),
        dependencies,
        packages,
    };

    Ok(ResolveResult {
        resolved_count: lockfile.packages.len(),
        fetched_count,
        lockfile,
    })
}

/// Recursively resolve a package and its dependencies.
///
/// Returns (resolved_version, was_already_resolved).
async fn resolve_package_tree(
    name: &str,
    range: &str,
    registry: &RegistryClient,
    packages: &mut BTreeMap<String, LockPackage>,
    visited: &mut HashSet<String>,
    fetched_count: &mut usize,
    depth: usize,
) -> Result<(String, bool), PkgError> {
    // Depth limit to prevent infinite recursion
    if depth > 100 {
        return Err(PkgError::spec_invalid(format!(
            "Dependency depth limit exceeded for '{name}'"
        )));
    }

    // Fetch packument
    let packument = registry.fetch_packument(name).await?;
    *fetched_count += 1;

    // Resolve version
    let version = resolve_version(&packument, Some(range))?;
    let key = format!("{}@{}", name, version);

    // Check if already resolved
    if visited.contains(&key) {
        return Ok((version, true));
    }
    visited.insert(key.clone());

    // Get package metadata
    let version_data = packument
        .get("versions")
        .and_then(|v| v.get(&version))
        .ok_or_else(|| PkgError::version_not_found(name, &version))?;

    // Get integrity hash
    let integrity = version_data
        .get("dist")
        .and_then(|d| d.get("integrity"))
        .or_else(|| version_data.get("dist").and_then(|d| d.get("shasum")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Get dependencies
    let deps = version_data
        .get("dependencies")
        .and_then(|d| d.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();

    // Resolve transitive dependencies
    for (dep_name, dep_range) in &deps {
        let _ = Box::pin(resolve_package_tree(
            dep_name,
            dep_range,
            registry,
            packages,
            visited,
            fetched_count,
            depth + 1,
        ))
        .await?;
    }

    // Create lock package entry
    let lock_pkg = LockPackage {
        version: version.clone(),
        integrity,
        resolution: LockResolution::Registry {
            registry: String::new(),
        },
        dependencies: deps,
        optional_dependencies: BTreeMap::new(),
        has_scripts: version_data
            .get("scripts")
            .and_then(|s| s.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false),
        cpu: Vec::new(),
        os: Vec::new(),
    };

    packages.insert(key, lock_pkg);

    Ok((version, false))
}

/// Get the dependency kind for a package.
fn get_dep_kind(pkg_json: &Value, name: &str) -> String {
    if pkg_json
        .get("devDependencies")
        .and_then(|d| d.get(name))
        .is_some()
    {
        "dev".to_string()
    } else if pkg_json
        .get("optionalDependencies")
        .and_then(|d| d.get(name))
        .is_some()
    {
        "optional".to_string()
    } else if pkg_json
        .get("peerDependencies")
        .and_then(|d| d.get(name))
        .is_some()
    {
        "peer".to_string()
    } else {
        "dep".to_string()
    }
}

/// Write lockfile to disk.
pub fn write_lockfile(project_root: &Path, lockfile: &Lockfile) -> Result<(), PkgError> {
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    lockfile
        .write_to(&lockfile_path)
        .map_err(|e| PkgError::package_json_invalid(format!("Failed to write lockfile: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_dep_kind() {
        let pkg_json = serde_json::json!({
            "dependencies": { "react": "^18.0.0" },
            "devDependencies": { "typescript": "^5.0.0" },
            "optionalDependencies": { "fsevents": "^2.0.0" }
        });

        assert_eq!(get_dep_kind(&pkg_json, "react"), "dep");
        assert_eq!(get_dep_kind(&pkg_json, "typescript"), "dev");
        assert_eq!(get_dep_kind(&pkg_json, "fsevents"), "optional");
        assert_eq!(get_dep_kind(&pkg_json, "unknown"), "dep");
    }
}
