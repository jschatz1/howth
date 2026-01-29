//! Dependency resolution and lockfile generation.
//!
//! Resolves dependencies from package.json and generates a lockfile.
//! Uses parallel resolution with packument caching for performance.

use super::deps::{parse_npm_alias, read_package_deps};
use super::error::PkgError;
use super::lockfile::{
    LockDep, LockMeta, LockPackage, LockResolution, LockRoot, Lockfile, LOCKFILE_NAME,
    PKG_LOCK_SCHEMA_VERSION,
};
use super::registry::RegistryClient;
use super::version::resolve_version;
use futures::stream::{self, StreamExt};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

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

/// Maximum concurrent packument fetches.
const MAX_CONCURRENT_FETCHES: usize = 32;

/// Maximum resolution depth to prevent infinite loops.
const MAX_DEPTH: usize = 100;

/// Cached packument data.
type PackumentCache = Arc<RwLock<HashMap<String, Arc<Value>>>>;

/// State for parallel resolution.
struct ResolveState {
    /// Cached packuments (name -> packument JSON).
    packuments: PackumentCache,
    /// Resolved packages (key -> LockPackage).
    packages: RwLock<BTreeMap<String, LockPackage>>,
    /// Visited package keys to avoid re-resolution.
    visited: RwLock<HashSet<String>>,
    /// Counter for packages fetched from registry.
    fetch_count: RwLock<usize>,
}

impl ResolveState {
    fn new() -> Self {
        Self {
            packuments: Arc::new(RwLock::new(HashMap::new())),
            packages: RwLock::new(BTreeMap::new()),
            visited: RwLock::new(HashSet::new()),
            fetch_count: RwLock::new(0),
        }
    }
}

/// A dependency to resolve.
#[derive(Debug, Clone)]
struct PendingDep {
    /// Real package name (used for registry fetch).
    name: String,
    /// Local alias name, if this dep uses `npm:` protocol.
    alias: Option<String>,
    range: String,
    depth: usize,
}

/// Resolve dependencies and generate a lockfile.
///
/// Uses parallel resolution with packument caching for improved performance.
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
    let pkg_deps =
        read_package_deps(&package_json_path, options.include_dev, options.include_optional)?;

    // Initialize resolution state
    let state = Arc::new(ResolveState::new());

    // Queue root dependencies
    let mut pending: VecDeque<PendingDep> = pkg_deps
        .deps
        .iter()
        .map(|(name, range)| {
            if let Some(real_name) = pkg_deps.aliases.get(name) {
                PendingDep {
                    name: real_name.clone(),
                    alias: Some(name.clone()),
                    range: range.clone(),
                    depth: 0,
                }
            } else {
                PendingDep {
                    name: name.clone(),
                    alias: None,
                    range: range.clone(),
                    depth: 0,
                }
            }
        })
        .collect();

    // Resolve in waves until no more pending dependencies
    while !pending.is_empty() {
        // Take current batch
        let batch: Vec<PendingDep> = pending.drain(..).collect();

        // Resolve batch in parallel
        let new_deps = resolve_batch(&batch, registry, &state).await?;

        // Add newly discovered dependencies to pending queue
        for dep in new_deps {
            if dep.depth <= MAX_DEPTH {
                pending.push_back(dep);
            }
        }
    }

    // Phase 2: Auto-install non-optional peer dependencies (pnpm v8+ behavior).
    // This runs AFTER all regular transitive deps are resolved so we can
    // reliably check whether a compatible version already exists and avoid
    // pulling in duplicate major versions (e.g. react@19 when 18 is in the tree).
    resolve_missing_peers(&state, registry).await?;

    // Build root dependencies map
    let mut dependencies: BTreeMap<String, LockDep> = BTreeMap::new();
    let packages = state.packages.read().await;

    for (name, range) in &pkg_deps.deps {
        let kind = get_dep_kind(&pkg_json, name);

        // For npm: aliases, the original range in package.json uses npm: prefix
        // Reconstruct it for the lockfile dep entry
        let original_range = if let Some(real_name) = pkg_deps.aliases.get(name) {
            format!("npm:{}@{}", real_name, range)
        } else {
            range.clone()
        };

        // Find the resolved version for this root dependency
        // Lockfile key uses the alias name (or real name if no alias)
        let version = packages
            .keys()
            .find(|key| key.starts_with(&format!("{}@", name)))
            .and_then(|key| key.strip_prefix(&format!("{}@", name)))
            .map(String::from)
            .unwrap_or_default();

        if !version.is_empty() {
            let key = format!("{}@{}", name, version);
            dependencies.insert(name.clone(), LockDep::new(original_range, kind, key));
        }
    }

    let fetch_count = *state.fetch_count.read().await;

    // Build lockfile
    let lockfile = Lockfile {
        lockfile_version: PKG_LOCK_SCHEMA_VERSION,
        meta: LockMeta {
            generated_at: Some(chrono::Utc::now().to_rfc3339()),
            howth_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
        root: LockRoot::new(root_name, root_version),
        dependencies,
        packages: packages.clone(),
    };

    Ok(ResolveResult {
        resolved_count: lockfile.packages.len(),
        fetched_count: fetch_count,
        lockfile,
    })
}

/// Resolve a batch of dependencies in parallel.
///
/// Returns newly discovered transitive dependencies.
async fn resolve_batch(
    batch: &[PendingDep],
    registry: &RegistryClient,
    state: &Arc<ResolveState>,
) -> Result<Vec<PendingDep>, PkgError> {
    // Filter out already-visited packages and deduplicate by name
    // (we only need to fetch each packument once per batch)
    let mut names_to_fetch: HashSet<String> = HashSet::new();
    let mut deps_to_resolve: Vec<PendingDep> = Vec::new();

    {
        let packuments = state.packuments.read().await;

        for dep in batch {
            // Check if we need to fetch this packument
            if !packuments.contains_key(&dep.name) {
                names_to_fetch.insert(dep.name.clone());
            }
            deps_to_resolve.push(dep.clone());
        }
    }

    // Fetch all needed packuments in parallel
    let names_vec: Vec<String> = names_to_fetch.into_iter().collect();

    let fetch_results: Vec<Result<(String, Value), PkgError>> = stream::iter(names_vec)
        .map(|name| {
            let registry = registry.clone();
            async move {
                let packument = registry.fetch_packument(&name).await?;
                Ok((name, packument))
            }
        })
        .buffer_unordered(MAX_CONCURRENT_FETCHES)
        .collect()
        .await;

    // Store fetched packuments in cache
    {
        let mut packuments = state.packuments.write().await;
        let mut fetch_count = state.fetch_count.write().await;

        for result in fetch_results {
            let (name, packument) = result?;
            packuments.insert(name, Arc::new(packument));
            *fetch_count += 1;
        }
    }

    // Now resolve all dependencies using cached packuments
    let mut new_deps: Vec<PendingDep> = Vec::new();

    for dep in deps_to_resolve {
        let resolved = resolve_single_dep(&dep, state).await?;
        new_deps.extend(resolved);
    }

    Ok(new_deps)
}

/// Resolve a single dependency using cached packument.
///
/// Returns newly discovered transitive dependencies.
async fn resolve_single_dep(
    dep: &PendingDep,
    state: &Arc<ResolveState>,
) -> Result<Vec<PendingDep>, PkgError> {
    // Get packument from cache
    let packument = {
        let packuments = state.packuments.read().await;
        packuments
            .get(&dep.name)
            .cloned()
            .ok_or_else(|| PkgError::not_found(&dep.name))?
    };

    // Resolve version
    let version = resolve_version(&packument, Some(&dep.range))?;
    // Use alias name for the lockfile key so node_modules uses the alias
    let key_name = dep.alias.as_deref().unwrap_or(&dep.name);
    let key = format!("{}@{}", key_name, version);

    // Check if already resolved
    {
        let visited = state.visited.read().await;
        if visited.contains(&key) {
            return Ok(Vec::new());
        }
    }

    // Mark as visited
    {
        let mut visited = state.visited.write().await;
        if !visited.insert(key.clone()) {
            // Another task already resolved this
            return Ok(Vec::new());
        }
    }

    // Get package metadata
    let version_data = packument
        .get("versions")
        .and_then(|v| v.get(&version))
        .ok_or_else(|| PkgError::version_not_found(&dep.name, &version))?;

    // Get integrity hash
    let integrity = version_data
        .get("dist")
        .and_then(|d| d.get("integrity"))
        .or_else(|| version_data.get("dist").and_then(|d| d.get("shasum")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Get dependencies
    let deps: BTreeMap<String, String> = version_data
        .get("dependencies")
        .and_then(|d| d.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    // Get peer dependencies (excluding optional ones per peerDependenciesMeta)
    let all_peer_deps: BTreeMap<String, String> = version_data
        .get("peerDependencies")
        .and_then(|d| d.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let peer_deps: BTreeMap<String, String> = all_peer_deps
        .into_iter()
        .filter(|(name, _)| !is_peer_optional(version_data, name))
        .collect();

    // Create lock package entry
    let lock_pkg = LockPackage {
        version: version.clone(),
        integrity,
        resolution: LockResolution::Registry {
            registry: String::new(),
        },
        alias_for: dep.alias.as_ref().map(|_| dep.name.clone()),
        dependencies: deps.clone(),
        optional_dependencies: BTreeMap::new(),
        peer_dependencies: peer_deps,
        has_scripts: version_data
            .get("scripts")
            .and_then(|s| s.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false),
        cpu: Vec::new(),
        os: Vec::new(),
    };

    // Store resolved package
    {
        let mut packages = state.packages.write().await;
        packages.insert(key, lock_pkg);
    }

    // Return transitive dependencies for next wave
    let new_deps: Vec<PendingDep> = deps
        .into_iter()
        .map(|(name, range)| {
            // Transitive deps can also use npm: aliases
            if let Some((real_name, real_range)) = parse_npm_alias(&range) {
                PendingDep {
                    name: real_name.to_string(),
                    alias: Some(name),
                    range: real_range.to_string(),
                    depth: dep.depth + 1,
                }
            } else {
                PendingDep {
                    name,
                    alias: None,
                    range,
                    depth: dep.depth + 1,
                }
            }
        })
        .collect();

    Ok(new_deps)
}

/// Resolve peer dependencies that are not yet satisfied by any package in the
/// lockfile.  Runs after all regular transitive resolution is complete so we
/// can reliably detect existing versions and avoid duplicates.
async fn resolve_missing_peers(
    state: &Arc<ResolveState>,
    registry: &RegistryClient,
) -> Result<(), PkgError> {
    use super::version::version_satisfies;

    loop {
        let missing: Vec<(String, String)> = {
            let packages = state.packages.read().await;

            let mut seen = HashSet::new();
            let mut out = Vec::new();

            for (_key, lock_pkg) in packages.iter() {
                for (peer_name, peer_range) in &lock_pkg.peer_dependencies {
                    // Already satisfied by an existing package?
                    let satisfied = packages.iter().any(|(k, _pkg)| {
                        k.rsplit_once('@').map_or(false, |(n, v)| {
                            n == peer_name.as_str() && version_satisfies(v, peer_range)
                        })
                    });

                    if !satisfied {
                        let dedup_key = format!("{}@{}", peer_name, peer_range);
                        if seen.insert(dedup_key) {
                            out.push((peer_name.clone(), peer_range.clone()));
                        }
                    }
                }
            }

            out
        };

        if missing.is_empty() {
            break;
        }

        // Resolve missing peers as a batch
        let batch: Vec<PendingDep> = missing
            .into_iter()
            .map(|(name, range)| PendingDep {
                name,
                alias: None,
                range,
                depth: 1, // peers are shallow
            })
            .collect();

        let new_deps = resolve_batch(&batch, registry, state).await?;

        // Peers may themselves have transitive deps — resolve those too
        let mut pending: VecDeque<PendingDep> = new_deps.into_iter().collect();
        while !pending.is_empty() {
            let wave: Vec<PendingDep> = pending.drain(..).collect();
            let next = resolve_batch(&wave, registry, state).await?;
            for dep in next {
                if dep.depth <= MAX_DEPTH {
                    pending.push_back(dep);
                }
            }
        }

        // Loop back to check if the newly resolved packages introduced more
        // unsatisfied peers (rare but possible).
    }

    Ok(())
}

/// Check whether a peer dependency is optional according to peerDependenciesMeta
/// in the packument.  Used during the peer-resolution phase.
fn is_peer_optional(version_data: &Value, peer_name: &str) -> bool {
    version_data
        .get("peerDependenciesMeta")
        .and_then(|m| m.as_object())
        .and_then(|obj| obj.get(peer_name))
        .and_then(|v| v.get("optional"))
        .and_then(|o| o.as_bool())
        .unwrap_or(false)
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
