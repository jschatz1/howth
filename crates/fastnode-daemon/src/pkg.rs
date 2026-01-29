//! Package manager handlers for the daemon.
//!
//! Handles `PkgAdd`, `PkgCacheList`, `PkgCachePrune`, `PkgGraph`, `PkgExplain`, `PkgWhy`, `PkgDoctor`,
//! and `PkgInstall` requests.

use fastnode_core::config::Channel;
use fastnode_core::pkg::{
    add_dependency_to_package_json, build_doctor_report, build_pkg_graph, detect_workspaces,
    download_tarball, extract_tgz_atomic, find_workspace_root, format_pnpm_key, get_tarball_url,
    link_into_node_modules, link_into_node_modules_direct, link_into_node_modules_with_version,
    link_package_binaries, link_package_dependencies, lockfile_content_hash, read_package_deps,
    remove_dependency_from_package_json, resolve_dependencies, resolve_version, version_satisfies,
    why_from_graph, write_lockfile, DoctorOptions, DoctorSeverity, GraphOptions, LockPackage, Lockfile,
    PackageCache, PackageSpec, PkgError, PkgWhyResult as CorePkgWhyResult, RegistryClient,
    ResolveOptions, WhyOptions, LOCKFILE_NAME, MAX_TARBALL_SIZE,
};
use fastnode_core::resolver::{
    resolve_with_trace, PkgJsonCache, ResolutionKind, ResolveContext, ResolverConfig,
};
use fastnode_proto::{
    codes, CachedPackage, DoctorCounts, DoctorFinding, DoctorSummary, GraphDepEdge, GraphErrorInfo,
    GraphPackageId, GraphPackageNode, InstallPackageError, InstallPackageInfo, InstallSummary,
    InstalledPackage, PackageGraph, PkgDoctorReport, PkgErrorInfo, PkgExplainResult,
    PkgExplainTraceStep, PkgExplainWarning, PkgInstallResult, PkgWhyChain, PkgWhyErrorInfo,
    PkgWhyLink, PkgWhyResult, PkgWhyTarget, Response, UpdatedPackage, PKG_DOCTOR_SCHEMA_VERSION,
    PKG_EXPLAIN_SCHEMA_VERSION, PKG_GRAPH_SCHEMA_VERSION, PKG_INSTALL_SCHEMA_VERSION,
    PKG_WHY_SCHEMA_VERSION,
};
use std::path::Path;
use tracing::{debug, warn};

/// Find the best matching version for a dependency in the lockfile.
///
/// When multiple versions of the same package exist (e.g. `entities@4.5.0` and
/// `entities@6.0.1`), uses the semver range to pick the correct one. Falls back
/// to the first name match if range parsing fails.
fn find_best_match(lockfile: &Lockfile, dep_name: &str, dep_range: &str) -> Option<String> {
    let candidates: Vec<&LockPackage> = lockfile
        .packages
        .iter()
        .filter_map(|(k, pkg)| {
            k.rsplit_once('@')
                .and_then(|(n, _)| if n == dep_name { Some(pkg) } else { None })
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].version.clone());
    }

    // Multiple versions — use semver to find the right one
    for pkg in &candidates {
        if version_satisfies(&pkg.version, dep_range) {
            return Some(pkg.version.clone());
        }
    }

    // Fallback: return first match (legacy behavior)
    Some(candidates[0].version.clone())
}

/// Parse a channel string to Channel enum.
fn parse_channel(channel: &str) -> Channel {
    match channel.to_lowercase().as_str() {
        "dev" => Channel::Dev,
        "nightly" => Channel::Nightly,
        _ => Channel::Stable,
    }
}

/// Handle a PkgAdd request.
pub async fn handle_pkg_add(specs: &[String], cwd: &str, channel: &str, save_dev: bool) -> Response {
    let project_root = Path::new(cwd);
    let package_json_path = project_root.join("package.json");

    // Create package cache for this channel
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);

    // Create registry client with persistent packument cache and .npmrc support
    let registry = match RegistryClient::from_env_with_cache(cache.clone()) {
        Ok(r) => r.with_npmrc(project_root),
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let mut reused_cache = 0u32;

    for spec_str in specs {
        match add_single_package(spec_str, project_root, &cache, &registry).await {
            Ok((pkg, from_cache, version_range)) => {
                // Update package.json with the dependency
                let dep_section = if save_dev { "devDependencies" } else { "dependencies" };
                debug!(
                    name = %pkg.name,
                    version = %pkg.version,
                    range = %version_range,
                    section = dep_section,
                    "Adding to package.json"
                );

                if let Err(e) = add_dependency_to_package_json(
                    &package_json_path,
                    &pkg.name,
                    &version_range,
                    save_dev,
                ) {
                    warn!(error = %e, "Failed to update package.json");
                    errors.push(PkgErrorInfo {
                        spec: spec_str.clone(),
                        code: e.code().to_string(),
                        message: format!("Installed but failed to update package.json: {e}"),
                    });
                }

                if from_cache {
                    reused_cache += 1;
                }
                installed.push(pkg);
            }
            Err(e) => {
                errors.push(PkgErrorInfo {
                    spec: spec_str.clone(),
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Regenerate lockfile if any packages were installed
    if !installed.is_empty() {
        debug!("Regenerating lockfile after adding packages");

        let resolve_opts = ResolveOptions {
            include_dev: true,
            include_optional: false,
        };

        match resolve_dependencies(project_root, &registry, &resolve_opts).await {
            Ok(result) => {
                if let Err(e) = write_lockfile(project_root, &result.lockfile) {
                    warn!(error = %e, "Failed to write lockfile");
                    // Don't add to errors - packages were installed successfully
                } else {
                    debug!(
                        resolved = result.resolved_count,
                        fetched = result.fetched_count,
                        "Lockfile regenerated"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to resolve dependencies for lockfile");
            }
        }
    }

    Response::PkgAddResult {
        installed,
        errors,
        reused_cache,
    }
}

/// Add a single package. Returns (InstalledPackage, was_cached, version_range_for_package_json).
async fn add_single_package(
    spec_str: &str,
    project_root: &Path,
    cache: &PackageCache,
    registry: &RegistryClient,
) -> Result<(InstalledPackage, bool, String), PkgError> {
    // Parse the spec
    let spec = PackageSpec::parse(spec_str)?;

    debug!(name = %spec.name, range = ?spec.range, "Adding package");

    // Fetch packument
    let packument = registry.fetch_packument(&spec.name).await?;

    // Resolve version
    let version = resolve_version(&packument, spec.range.as_deref())?;

    debug!(name = %spec.name, version = %version, "Resolved version");

    // Determine version range for package.json
    // If user specified a range, use it; otherwise use "^{resolved_version}"
    let version_range = spec
        .range
        .clone()
        .unwrap_or_else(|| format!("^{version}"));

    // Check if already cached
    let package_dir = cache.package_dir(&spec.name, &version);
    let was_cached = cache.is_cached(&spec.name, &version);

    if was_cached {
        debug!(path = %package_dir.display(), "Using cached package");
    } else {
        // Get tarball URL
        let tarball_url = get_tarball_url(&packument, &version).ok_or_else(|| {
            PkgError::download_failed(format!("No tarball URL for {}@{}", spec.name, version))
        })?;

        debug!(url = %tarball_url, "Downloading tarball");

        // Download tarball (with auth token for scoped registries)
        let auth_token = registry.auth_token_for(&spec.name);
        let bytes = download_tarball(registry.http(), tarball_url, MAX_TARBALL_SIZE, auth_token).await?;

        debug!(size = bytes.len(), "Downloaded tarball");

        // Extract to cache (offload CPU-bound decompression to thread pool)
        let extract_bytes = bytes.clone();
        let extract_dest = package_dir.clone();
        tokio::task::spawn_blocking(move || {
            extract_tgz_atomic(&extract_bytes, &extract_dest)
        })
        .await
        .map_err(|e| PkgError::extract_failed(format!("Extraction task failed: {e}")))??;

        debug!(path = %package_dir.display(), "Extracted to cache");
    }

    // Link into node_modules
    let link_path = link_into_node_modules(project_root, &spec.name, &package_dir)?;

    debug!(link = %link_path.display(), "Linked into node_modules");

    // Derive the .pnpm content path so binary symlinks resolve transitive deps
    let pnpm_pkg_dir = project_root
        .join("node_modules/.pnpm")
        .join(format_pnpm_key(&spec.name, &version))
        .join("node_modules")
        .join(&spec.name);

    // Link binaries into .bin
    if let Ok(binaries) = link_package_binaries(project_root, &spec.name, &package_dir, Some(&pnpm_pkg_dir)) {
        for bin in &binaries {
            debug!(bin = %bin.display(), "Linked binary");
        }
    }

    Ok((
        InstalledPackage {
            name: spec.name,
            version: version.clone(),
            link_path: link_path.to_string_lossy().into_owned(),
            cache_path: package_dir.to_string_lossy().into_owned(),
        },
        was_cached,
        version_range,
    ))
}

/// Handle a PkgRemove request.
pub async fn handle_pkg_remove(packages: &[String], cwd: &str, channel: &str) -> Response {
    let project_root = Path::new(cwd);
    let package_json_path = project_root.join("package.json");
    let node_modules = project_root.join("node_modules");

    // Create package cache and registry client with persistent packument cache and .npmrc support
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);
    let registry = match RegistryClient::from_env_with_cache(cache) {
        Ok(r) => r.with_npmrc(project_root),
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    let mut removed = Vec::new();
    let mut errors = Vec::new();

    for pkg_name in packages {
        debug!(name = %pkg_name, "Removing package");

        // Remove from package.json
        match remove_dependency_from_package_json(&package_json_path, pkg_name) {
            Ok(was_removed) => {
                if was_removed {
                    debug!(name = %pkg_name, "Removed from package.json");

                    // Remove from node_modules
                    let pkg_path = node_modules.join(pkg_name);
                    if pkg_path.exists() {
                        if let Err(e) = std::fs::remove_dir_all(&pkg_path) {
                            warn!(name = %pkg_name, error = %e, "Failed to remove from node_modules");
                            // Don't fail the whole operation, package.json was updated
                        } else {
                            debug!(name = %pkg_name, "Removed from node_modules");
                        }
                    }

                    removed.push(pkg_name.clone());
                } else {
                    errors.push(PkgErrorInfo {
                        spec: pkg_name.clone(),
                        code: "PKG_NOT_FOUND".to_string(),
                        message: format!("Package '{}' not found in package.json", pkg_name),
                    });
                }
            }
            Err(e) => {
                errors.push(PkgErrorInfo {
                    spec: pkg_name.clone(),
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Regenerate lockfile if any packages were removed
    if !removed.is_empty() {
        debug!("Regenerating lockfile after removing packages");

        let resolve_opts = ResolveOptions {
            include_dev: true,
            include_optional: false,
        };

        match resolve_dependencies(project_root, &registry, &resolve_opts).await {
            Ok(result) => {
                if let Err(e) = write_lockfile(project_root, &result.lockfile) {
                    warn!(error = %e, "Failed to write lockfile");
                } else {
                    debug!(
                        resolved = result.resolved_count,
                        fetched = result.fetched_count,
                        "Lockfile regenerated"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to resolve dependencies for lockfile");
            }
        }
    }

    // Suppress unused variable warning
    let _ = channel;

    Response::PkgRemoveResult { removed, errors }
}

/// Handle a PkgUpdate request.
pub async fn handle_pkg_update(
    packages: &[String],
    cwd: &str,
    channel: &str,
    latest: bool,
) -> Response {
    let project_root = Path::new(cwd);
    let package_json_path = project_root.join("package.json");

    // Create package cache and registry client with persistent packument cache and .npmrc support
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);
    let registry = match RegistryClient::from_env_with_cache(cache) {
        Ok(r) => r.with_npmrc(project_root),
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    // Read current lockfile to get installed versions
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    let lockfile: Option<Lockfile> = if lockfile_path.exists() {
        match Lockfile::read_from(&lockfile_path) {
            Ok(lf) => Some(lf),
            Err(e) => {
                warn!(error = %e, "Failed to read lockfile, will resolve all");
                None
            }
        }
    } else {
        None
    };

    // Read dependencies from package.json
    let deps_result = read_package_deps(&package_json_path, true, true);
    let all_deps = match deps_result {
        Ok(result) => result.deps,
        Err(e) => {
            return Response::error(e.code().to_string(), e.to_string());
        }
    };

    // Filter to specific packages if provided
    let deps_to_check: Vec<(String, String)> = if packages.is_empty() {
        all_deps
    } else {
        all_deps
            .into_iter()
            .filter(|(name, _)| packages.contains(name))
            .collect()
    };

    let mut updated = Vec::new();
    let mut up_to_date = Vec::new();
    let mut errors = Vec::new();

    for (name, range) in deps_to_check {
        // Get current installed version from lockfile
        let current_version = lockfile
            .as_ref()
            .and_then(|lf| lf.dependencies.get(&name))
            .and_then(|dep| {
                // Parse version from resolved like "package@1.0.0"
                dep.resolved.split('@').last().map(|s| s.to_string())
            });

        // Fetch packument to check for updates
        match registry.fetch_packument(&name).await {
            Ok(packument) => {
                // Resolve the best version for the range
                let target_range = if latest {
                    // Use "latest" tag or "*" to get the newest version
                    None
                } else {
                    Some(range.as_str())
                };

                match resolve_version(&packument, target_range) {
                    Ok(new_version) => {
                        let needs_update = current_version
                            .as_ref()
                            .map(|cv| cv != &new_version)
                            .unwrap_or(true);

                        if needs_update {
                            debug!(
                                name = %name,
                                from = ?current_version,
                                to = %new_version,
                                "Package needs update"
                            );

                            // If --latest, update package.json with new range
                            if latest {
                                let new_range = format!("^{}", new_version);
                                if let Err(e) = add_dependency_to_package_json(
                                    &package_json_path,
                                    &name,
                                    &new_range,
                                    false, // We don't know if it was dev, but add to deps
                                ) {
                                    warn!(error = %e, "Failed to update package.json");
                                }
                            }

                            updated.push(UpdatedPackage {
                                name: name.clone(),
                                from_version: current_version.unwrap_or_else(|| "none".to_string()),
                                to_version: new_version,
                            });
                        } else {
                            up_to_date.push(name.clone());
                        }
                    }
                    Err(e) => {
                        errors.push(PkgErrorInfo {
                            spec: name.clone(),
                            code: e.code().to_string(),
                            message: e.to_string(),
                        });
                    }
                }
            }
            Err(e) => {
                errors.push(PkgErrorInfo {
                    spec: name.clone(),
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Regenerate lockfile if any packages were updated
    if !updated.is_empty() {
        debug!("Regenerating lockfile after update");

        let resolve_opts = ResolveOptions {
            include_dev: true,
            include_optional: false,
        };

        match resolve_dependencies(project_root, &registry, &resolve_opts).await {
            Ok(result) => {
                if let Err(e) = write_lockfile(project_root, &result.lockfile) {
                    warn!(error = %e, "Failed to write lockfile");
                } else {
                    debug!(
                        resolved = result.resolved_count,
                        fetched = result.fetched_count,
                        "Lockfile regenerated"
                    );
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to resolve dependencies for lockfile");
            }
        }
    }

    // Suppress unused variable warning
    let _ = channel;

    Response::PkgUpdateResult {
        updated,
        up_to_date,
        errors,
    }
}

/// Handle a PkgOutdated request.
pub async fn handle_pkg_outdated(cwd: &str, channel: &str) -> Response {
    use fastnode_proto::OutdatedPackage;

    let project_root = Path::new(cwd);
    let package_json_path = project_root.join("package.json");

    // Create package cache and registry client with persistent packument cache and .npmrc support
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);
    let registry = match RegistryClient::from_env_with_cache(cache) {
        Ok(r) => r.with_npmrc(project_root),
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    // Read current lockfile to get installed versions
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    let lockfile: Option<Lockfile> = if lockfile_path.exists() {
        match Lockfile::read_from(&lockfile_path) {
            Ok(lf) => Some(lf),
            Err(e) => {
                warn!(error = %e, "Failed to read lockfile");
                None
            }
        }
    } else {
        return Response::error(
            codes::PKG_LOCKFILE_NOT_FOUND,
            "No lockfile found. Run 'howth install' first.".to_string(),
        );
    };

    // Read dependencies from package.json
    let deps_result = read_package_deps(&package_json_path, true, true);
    let (all_deps, dev_deps) = match deps_result {
        Ok(result) => {
            // Also read dev deps to classify them
            let dev_deps_result = read_package_deps(&package_json_path, true, false);
            let dev_names: std::collections::HashSet<String> = dev_deps_result
                .map(|r| r.deps.into_iter().map(|(n, _)| n).collect())
                .unwrap_or_default();
            (result.deps, dev_names)
        }
        Err(e) => {
            return Response::error(e.code().to_string(), e.to_string());
        }
    };

    let mut outdated = Vec::new();
    let mut up_to_date_count = 0u32;

    for (name, range) in all_deps {
        // Get current installed version from lockfile
        let current_version = lockfile
            .as_ref()
            .and_then(|lf| lf.dependencies.get(&name))
            .and_then(|dep| dep.resolved.split('@').last().map(|s| s.to_string()));

        let current = current_version.unwrap_or_else(|| "none".to_string());

        // Fetch packument to check for updates
        match registry.fetch_packument(&name).await {
            Ok(packument) => {
                // Resolve wanted version (within semver range)
                let wanted = resolve_version(&packument, Some(&range))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| current.clone());

                // Resolve latest version (any version)
                let latest = resolve_version(&packument, None)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| current.clone());

                // Determine dep type
                let dep_type = if dev_deps.contains(&name) {
                    "dev"
                } else {
                    "dep"
                };

                // Check if outdated
                if current != wanted || current != latest {
                    outdated.push(OutdatedPackage {
                        name,
                        current,
                        wanted,
                        latest,
                        dep_type: dep_type.to_string(),
                    });
                } else {
                    up_to_date_count += 1;
                }
            }
            Err(e) => {
                warn!(name = %name, error = %e, "Failed to fetch packument for outdated check");
            }
        }
    }

    // Suppress unused variable warning
    let _ = channel;

    Response::PkgOutdatedResult {
        outdated,
        up_to_date_count,
    }
}

/// Handle a PkgPublish request.
///
/// Uses npm CLI under the hood for reliable publishing.
pub async fn handle_pkg_publish(
    cwd: &str,
    registry_url: Option<&str>,
    token: Option<&str>,
    dry_run: bool,
    tag: Option<&str>,
    access: Option<&str>,
) -> Response {
    use std::process::Command;

    let project_root = Path::new(cwd);
    let package_json_path = project_root.join("package.json");

    // Read package.json
    let package_json_content = match std::fs::read_to_string(&package_json_path) {
        Ok(c) => c,
        Err(e) => {
            return Response::PkgPublishResult {
                ok: false,
                name: String::new(),
                version: String::new(),
                registry: String::new(),
                tag: String::new(),
                tarball_size: 0,
                files_count: 0,
                error: Some(format!("Failed to read package.json: {e}")),
            };
        }
    };

    let package_json: serde_json::Value = match serde_json::from_str(&package_json_content) {
        Ok(v) => v,
        Err(e) => {
            return Response::PkgPublishResult {
                ok: false,
                name: String::new(),
                version: String::new(),
                registry: String::new(),
                tag: String::new(),
                tarball_size: 0,
                files_count: 0,
                error: Some(format!("Failed to parse package.json: {e}")),
            };
        }
    };

    let name = package_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = package_json
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if name.is_empty() || version.is_empty() {
        return Response::PkgPublishResult {
            ok: false,
            name,
            version,
            registry: String::new(),
            tag: String::new(),
            tarball_size: 0,
            files_count: 0,
            error: Some("package.json must have name and version fields".to_string()),
        };
    }

    let registry = registry_url.unwrap_or("https://registry.npmjs.org");
    let tag = tag.unwrap_or("latest");

    // Build npm publish command
    let mut cmd = Command::new("npm");
    cmd.arg("publish");
    cmd.current_dir(project_root);

    if dry_run {
        cmd.arg("--dry-run");
    }

    cmd.arg("--tag").arg(tag);

    if let Some(reg) = registry_url {
        cmd.arg("--registry").arg(reg);
    }

    if let Some(acc) = access {
        cmd.arg("--access").arg(acc);
    }

    // Set token via environment if provided
    if let Some(tok) = token {
        cmd.env("NPM_TOKEN", tok);
    }

    debug!(
        name = %name,
        version = %version,
        dry_run = dry_run,
        "Running npm publish"
    );

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                // Try to parse npm pack output for file count/size (best effort)
                let files_count = stdout
                    .lines()
                    .filter(|l| l.contains("files:"))
                    .next()
                    .and_then(|l| l.split_whitespace().last())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0u32);

                let tarball_size = stdout
                    .lines()
                    .filter(|l| l.contains("size:") || l.contains("unpacked size"))
                    .next()
                    .and_then(|l| {
                        l.split_whitespace()
                            .find(|s| s.chars().all(|c| c.is_ascii_digit()))
                    })
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0u64);

                Response::PkgPublishResult {
                    ok: true,
                    name,
                    version,
                    registry: registry.to_string(),
                    tag: tag.to_string(),
                    tarball_size,
                    files_count,
                    error: None,
                }
            } else {
                Response::PkgPublishResult {
                    ok: false,
                    name,
                    version,
                    registry: registry.to_string(),
                    tag: tag.to_string(),
                    tarball_size: 0,
                    files_count: 0,
                    error: Some(format!(
                        "npm publish failed: {}",
                        if stderr.is_empty() {
                            stdout.to_string()
                        } else {
                            stderr.to_string()
                        }
                    )),
                }
            }
        }
        Err(e) => Response::PkgPublishResult {
            ok: false,
            name,
            version,
            registry: registry.to_string(),
            tag: tag.to_string(),
            tarball_size: 0,
            files_count: 0,
            error: Some(format!(
                "Failed to run npm publish (is npm installed?): {e}"
            )),
        },
    }
}

/// Handle a PkgCacheList request.
pub fn handle_pkg_cache_list(channel: &str) -> Response {
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);

    match cache.list_cached() {
        Ok(packages) => {
            let mut total_size_bytes = 0u64;
            let cached_packages: Vec<CachedPackage> = packages
                .into_iter()
                .map(|(name, version)| {
                    let path = cache.package_dir(&name, &version);
                    // Calculate size (best effort)
                    let size = calculate_dir_size(&path).unwrap_or(0);
                    total_size_bytes += size;

                    CachedPackage {
                        name,
                        version,
                        size_bytes: size,
                        path: path.to_string_lossy().into_owned(),
                    }
                })
                .collect();

            Response::PkgCacheListResult {
                packages: cached_packages,
                total_size_bytes,
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to list cache");
            Response::PkgCacheListResult {
                packages: Vec::new(),
                total_size_bytes: 0,
            }
        }
    }
}

/// Handle a PkgCachePrune request (stub for now).
#[allow(unused_variables)]
pub fn handle_pkg_cache_prune(channel: &str) -> Response {
    // TODO: Implement cache pruning logic
    // For now, return empty result
    Response::PkgCachePruneResult {
        removed_count: 0,
        freed_bytes: 0,
    }
}

/// Handle a PkgInstall request (v1.9).
///
/// Installs packages from the lockfile (`howth.lock`).
/// If `progress_tx` is provided, sends `PkgInstallProgress` events per package.
pub async fn handle_pkg_install(
    cwd: &str,
    channel: &str,
    frozen: bool,
    include_dev: bool,
    include_optional: bool,
) -> Response {
    handle_pkg_install_with_progress(cwd, channel, frozen, include_dev, include_optional, None)
        .await
}

/// Handle a PkgInstall request with optional streaming progress.
///
/// When `progress_tx` is `Some`, sends `PkgInstallProgress` events as each
/// package completes. The final `PkgInstallResult` is always returned.
pub async fn handle_pkg_install_with_progress(
    cwd: &str,
    channel: &str,
    frozen: bool,
    include_dev: bool,
    include_optional: bool,
    progress_tx: Option<tokio::sync::mpsc::Sender<Response>>,
) -> Response {
    use std::path::PathBuf;

    let project_root = PathBuf::from(cwd);

    // Canonicalize the path
    let project_root = match project_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::CWD_INVALID,
                format!("Invalid working directory '{cwd}': {e}"),
            );
        }
    };

    // Create package cache for this channel (needed for registry client)
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);

    // Create registry client with persistent packument cache and .npmrc support
    let registry = match RegistryClient::from_env_with_cache(cache.clone()) {
        Ok(r) => r.with_npmrc(&project_root),
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    // Check for lockfile
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    let lockfile = if !lockfile_path.exists() {
        if frozen {
            return Response::error(
                codes::PKG_INSTALL_LOCKFILE_NOT_FOUND,
                format!(
                    "Lockfile not found at '{}' (--frozen requires lockfile)",
                    lockfile_path.display()
                ),
            );
        }

        // No lockfile - generate one from package.json
        debug!("No lockfile found, resolving dependencies...");

        let resolve_opts = ResolveOptions {
            include_dev,
            include_optional,
        };

        match resolve_dependencies(&project_root, &registry, &resolve_opts).await {
            Ok(result) => {
                debug!(
                    resolved = result.resolved_count,
                    fetched = result.fetched_count,
                    "Dependencies resolved"
                );

                // Write lockfile
                if let Err(e) = write_lockfile(&project_root, &result.lockfile) {
                    return Response::error(
                        codes::PKG_INSTALL_LOCKFILE_INVALID,
                        format!("Failed to write lockfile: {e}"),
                    );
                }

                debug!(path = %lockfile_path.display(), "Wrote lockfile");
                result.lockfile
            }
            Err(e) => {
                return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
            }
        }
    } else {
        // Read existing lockfile
        let lf = match Lockfile::read_from(&lockfile_path) {
            Ok(lf) => lf,
            Err(e) => {
                return Response::error(codes::PKG_INSTALL_LOCKFILE_INVALID, e.to_string());
            }
        };

        // Check if package.json has diverged from the lockfile.
        // Compare both dependency names AND version ranges, so that upgrading
        // a package (e.g. googleapis from ^105 to ^170) triggers re-resolution.
        let package_json_path = project_root.join("package.json");
        let pkg_deps_result = read_package_deps(&package_json_path, include_dev, include_optional);
        let needs_re_resolve = match pkg_deps_result {
            Ok(pkg_deps) => {
                // Reconstruct the original ranges (with npm: prefix for aliases)
                // to match the format stored in the lockfile.
                let pj_ranges: std::collections::HashMap<&str, String> = pkg_deps
                    .deps
                    .iter()
                    .map(|(name, range)| {
                        let original = if let Some(real_name) = pkg_deps.aliases.get(name) {
                            format!("npm:{}@{}", real_name, range)
                        } else {
                            range.clone()
                        };
                        (name.as_str(), original)
                    })
                    .collect();
                let lf_ranges: std::collections::HashMap<&str, &str> = lf
                    .dependencies
                    .iter()
                    .map(|(n, d)| (n.as_str(), d.range.as_str()))
                    .collect();
                // Different number of deps, or any name/range mismatch
                if pj_ranges.len() != lf_ranges.len() {
                    true
                } else {
                    pj_ranges.iter().any(|(name, range)| {
                        lf_ranges.get(name).map_or(true, |lf_range| range.as_str() != *lf_range)
                    })
                }
            }
            Err(_) => false, // If we can't read package.json, proceed with existing lockfile
        };

        if needs_re_resolve {
            if frozen {
                return Response::error(
                    codes::PKG_INSTALL_LOCKFILE_INVALID,
                    "Lockfile is out of date with package.json (--frozen disallows re-resolving)",
                );
            }

            debug!("package.json has changed, re-resolving dependencies...");

            let resolve_opts = ResolveOptions {
                include_dev,
                include_optional,
            };

            match resolve_dependencies(&project_root, &registry, &resolve_opts).await {
                Ok(result) => {
                    if let Err(e) = write_lockfile(&project_root, &result.lockfile) {
                        return Response::error(
                            codes::PKG_INSTALL_LOCKFILE_INVALID,
                            format!("Failed to write lockfile: {e}"),
                        );
                    }
                    debug!(
                        resolved = result.resolved_count,
                        fetched = result.fetched_count,
                        "Dependencies re-resolved"
                    );
                    result.lockfile
                }
                Err(e) => {
                    return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
                }
            }
        } else {
            lf
        }
    };

    debug!(
        lockfile = %lockfile_path.display(),
        packages = lockfile.packages.len(),
        "Using lockfile"
    );

    // Check if node_modules is already up-to-date with the lockfile
    let content_hash = lockfile_content_hash(&lockfile);
    let state_file = project_root.join("node_modules/.howth-state");
    let pnpm_dir = project_root.join("node_modules/.pnpm");

    if pnpm_dir.is_dir() {
        if let Ok(stored_hash) = std::fs::read_to_string(&state_file) {
            if stored_hash.trim() == content_hash {
                debug!("node_modules is up-to-date, skipping install");
                return Response::PkgInstallResult {
                    result: PkgInstallResult {
                        schema_version: PKG_INSTALL_SCHEMA_VERSION,
                        cwd: project_root.to_string_lossy().into_owned(),
                        ok: true,
                        summary: InstallSummary {
                            total_packages: 0,
                            downloaded: 0,
                            cached: 0,
                            linked: 0,
                            failed: 0,
                            workspace_linked: 0,
                        },
                        installed: Vec::new(),
                        errors: Vec::new(),
                        notes: vec!["already up-to-date".to_string()],
                    },
                };
            }
        }
    }

    // Detect workspaces for local package linking
    let workspace_root = find_workspace_root(&project_root);
    let workspace_config = workspace_root
        .as_ref()
        .and_then(|root| detect_workspaces(root));

    if let Some(ref config) = workspace_config {
        debug!(
            workspace_root = %config.root.display(),
            packages = config.packages.len(),
            "Detected workspace"
        );
    }

    use futures::stream::{self, StreamExt};

    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let mut downloaded = 0u32;
    let mut cached = 0u32;
    let mut linked = 0u32;
    let mut workspace_linked = 0u32;
    let mut completed = 0u32;

    // Count total packages to install (for progress reporting)
    let total_packages = lockfile.packages.len() as u32;

    // Separate workspace packages from registry packages
    // Workspace packages are linked locally (fast), registry packages need download (parallelized)
    let mut registry_packages = Vec::new();

    for (key, lock_pkg) in &lockfile.packages {
        // Parse package name from key (format: "name@version")
        let name = key.rsplit_once('@').map_or(key.as_str(), |(n, _)| n);

        // Check dependency kind - skip dev/optional if not requested
        let is_root_dep = lockfile.dependencies.contains_key(name);
        if is_root_dep {
            if let Some(dep) = lockfile.dependencies.get(name) {
                if dep.kind == "dev" && !include_dev {
                    continue;
                }
                if dep.kind == "optional" && !include_optional {
                    continue;
                }
            }
        }

        // Check if this is a workspace package
        if let Some(ref config) = workspace_config {
            if let Some(ws_pkg) = config.get_package(name).filter(|ws| ws.version == lock_pkg.version) {
                // Link workspace package directly instead of fetching from registry
                debug!(name = %name, path = %ws_pkg.path.display(), "Linking workspace package");

                // Use direct linking for workspace packages (not pnpm layout)
                match link_into_node_modules_direct(&project_root, name, &ws_pkg.path) {
                    Ok(link_path) => {
                        // Link binaries for workspace package (no pnpm layout)
                        if let Ok(binaries) = link_package_binaries(&project_root, name, &ws_pkg.path, None) {
                            for bin in &binaries {
                                debug!(bin = %bin.display(), "Linked workspace binary");
                            }
                        }
                        workspace_linked += 1;
                        linked += 1;
                        completed += 1;
                        installed.push(InstallPackageInfo {
                            name: name.to_string(),
                            version: ws_pkg.version.clone(),
                            from_cache: false,
                            link_path: link_path.to_string_lossy().into_owned(),
                            cache_path: ws_pkg.path.to_string_lossy().into_owned(),
                            is_workspace: true,
                        });

                        // Send progress event
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(Response::PkgInstallProgress {
                                name: name.to_string(),
                                version: ws_pkg.version.clone(),
                                status: "workspace".to_string(),
                                completed,
                                total: total_packages,
                            }).await;
                        }

                        continue;
                    }
                    Err(e) => {
                        errors.push(InstallPackageError {
                            name: name.to_string(),
                            version: ws_pkg.version.clone(),
                            code: e.code().to_string(),
                            message: e.to_string(),
                        });
                        continue;
                    }
                }
            }
        }

        // Collect registry packages for parallel download
        registry_packages.push((name.to_string(), lock_pkg.clone()));
    }

    // Install registry packages in parallel
    const MAX_CONCURRENT_DOWNLOADS: usize = 32;

    let mut stream = stream::iter(registry_packages)
        .map(|(name, lock_pkg)| {
            let project_root = project_root.clone();
            let cache = cache.clone();
            let registry = registry.clone();
            async move {
                let result = install_from_lockfile(&name, &lock_pkg, &project_root, &cache, &registry).await;
                (name, lock_pkg.version.clone(), result)
            }
        })
        .buffer_unordered(MAX_CONCURRENT_DOWNLOADS);

    // Process results one at a time, sending progress for each
    while let Some((name, version, result)) = stream.next().await {
        match result {
            Ok((pkg_info, from_cache)) => {
                let status = if from_cache { "cached" } else { "downloaded" };
                if from_cache {
                    cached += 1;
                } else {
                    downloaded += 1;
                }
                linked += 1;
                completed += 1;

                // Send progress event
                if let Some(ref tx) = progress_tx {
                    let _ = tx.send(Response::PkgInstallProgress {
                        name: name.clone(),
                        version: version.clone(),
                        status: status.to_string(),
                        completed,
                        total: total_packages,
                    }).await;
                }

                installed.push(pkg_info);
            }
            Err(e) => {
                completed += 1;
                errors.push(InstallPackageError {
                    name,
                    version,
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    // Phase 2: Link package dependencies (pnpm-style)
    // This must happen after all packages are installed so the targets exist.
    // First resolve all dependency versions from the lockfile (cheap, single-threaded),
    // then create symlinks in parallel via rayon.
    debug!("Linking package dependencies (pnpm layout)");

    // Collect work items: (name, version, resolved_deps)
    let mut link_work: Vec<(String, String, std::collections::BTreeMap<String, String>)> = Vec::new();
    for (key, lock_pkg) in &lockfile.packages {
        if lock_pkg.dependencies.is_empty() && lock_pkg.peer_dependencies.is_empty() {
            continue;
        }

        let name = key.rsplit_once('@').map_or(key.as_str(), |(n, _)| n);
        let version = &lock_pkg.version;

        let mut resolved_deps = std::collections::BTreeMap::new();
        for (dep_name, dep_range) in &lock_pkg.dependencies {
            if let Some(version) = find_best_match(&lockfile, dep_name, dep_range) {
                resolved_deps.insert(dep_name.clone(), version);
            } else {
                debug!(
                    pkg = %name,
                    dep = %dep_name,
                    range = %dep_range,
                    "Dependency not found in lockfile, skipping"
                );
            }
        }

        for (dep_name, dep_range) in &lock_pkg.peer_dependencies {
            if resolved_deps.contains_key(dep_name) {
                continue;
            }
            if let Some(version) = find_best_match(&lockfile, dep_name, dep_range) {
                resolved_deps.insert(dep_name.clone(), version);
            }
        }

        if !resolved_deps.is_empty() {
            link_work.push((name.to_string(), version.clone(), resolved_deps));
        }
    }

    // Execute symlink creation in parallel
    {
        let project_root = project_root.clone();
        let link_errors: Vec<String> = tokio::task::block_in_place(|| {
            use rayon::prelude::*;
            link_work
                .par_iter()
                .filter_map(|(name, version, resolved_deps)| {
                    match link_package_dependencies(&project_root, name, version, resolved_deps) {
                        Ok(()) => None,
                        Err(e) => Some(format!("{name}@{version}: {e}")),
                    }
                })
                .collect()
        });

        for msg in &link_errors {
            warn!(error = %msg, "Failed to link package dependencies");
        }
    }

    let ok = errors.is_empty();

    debug!(
        total = total_packages,
        downloaded,
        cached,
        linked,
        workspace_linked,
        failed = errors.len(),
        "Install completed"
    );

    // Write sentinel so the next install can skip if nothing changed
    if ok {
        if let Err(e) = std::fs::write(&state_file, &content_hash) {
            warn!(error = %e, "Failed to write .howth-state sentinel");
        }
    }

    let mut notes = vec![];
    if workspace_linked > 0 {
        notes.push(format!(
            "{} workspace package(s) linked locally",
            workspace_linked
        ));
    }

    Response::PkgInstallResult {
        result: PkgInstallResult {
            schema_version: PKG_INSTALL_SCHEMA_VERSION,
            cwd: project_root.to_string_lossy().into_owned(),
            ok,
            summary: InstallSummary {
                total_packages,
                downloaded,
                cached,
                linked,
                failed: errors.len() as u32,
                workspace_linked,
            },
            installed,
            errors,
            notes,
        },
    }
}

/// Install a single package from lockfile.
async fn install_from_lockfile(
    name: &str,
    lock_pkg: &LockPackage,
    project_root: &Path,
    cache: &PackageCache,
    registry: &RegistryClient,
) -> Result<(InstallPackageInfo, bool), PkgError> {
    let version = &lock_pkg.version;
    // For npm: aliases, use the real package name for registry/cache operations
    let fetch_name = lock_pkg.alias_for.as_deref().unwrap_or(name);

    debug!(name = %name, fetch_name = %fetch_name, version = %version, "Installing package from lockfile");

    // Check if already cached (use real package name for cache)
    let package_dir = cache.package_dir(fetch_name, version);
    let was_cached = cache.is_cached(fetch_name, version);

    if was_cached {
        debug!(path = %package_dir.display(), "Using cached package");
    } else {
        // Get tarball URL: prefer lockfile (avoids packument fetch), fall back to registry
        let tarball_url = if let Some(ref url) = lock_pkg.tarball_url {
            debug!(url = %url, "Using tarball URL from lockfile");
            url.clone()
        } else {
            let packument = registry.fetch_packument(fetch_name).await?;
            get_tarball_url(&packument, version)
                .ok_or_else(|| {
                    PkgError::download_failed(format!("No tarball URL for {}@{}", fetch_name, version))
                })?
                .to_string()
        };

        debug!(url = %tarball_url, "Downloading tarball");

        // Download tarball (with auth token for scoped registries)
        let auth_token = registry.auth_token_for(fetch_name);
        let bytes = download_tarball(registry.http(), &tarball_url, MAX_TARBALL_SIZE, auth_token).await?;

        // TODO: Verify integrity hash matches lock_pkg.integrity
        // For now, just extract

        debug!(size = bytes.len(), "Downloaded tarball");

        // Extract to cache (offload CPU-bound decompression to thread pool)
        let extract_bytes = bytes.clone();
        let extract_dest = package_dir.clone();
        tokio::task::spawn_blocking(move || {
            extract_tgz_atomic(&extract_bytes, &extract_dest)
        })
        .await
        .map_err(|e| PkgError::extract_failed(format!("Extraction task failed: {e}")))??;

        debug!(path = %package_dir.display(), "Extracted to cache");
    }

    // Link into node_modules using pnpm-style layout
    // Use the alias name so the module is accessible under the alias
    let link_path = link_into_node_modules_with_version(project_root, name, version, &package_dir)?;

    // Derive the .pnpm content path so binary symlinks resolve transitive deps
    let pnpm_pkg_dir = project_root
        .join("node_modules/.pnpm")
        .join(format_pnpm_key(name, version))
        .join("node_modules")
        .join(name);

    // Link binaries into .bin
    if let Ok(binaries) = link_package_binaries(project_root, name, &package_dir, Some(&pnpm_pkg_dir)) {
        for bin in &binaries {
            debug!(bin = %bin.display(), "Linked binary");
        }
    }

    Ok((
        InstallPackageInfo {
            name: name.to_string(),
            version: version.to_string(),
            from_cache: was_cached,
            link_path: link_path.to_string_lossy().into_owned(),
            cache_path: package_dir.to_string_lossy().into_owned(),
            is_workspace: false,
        },
        was_cached,
    ))
}

/// Handle a PkgGraph request.
pub fn handle_pkg_graph(
    cwd: &str,
    include_dev_root: bool,
    include_optional: bool,
    max_depth: u32,
    pkg_json_cache: &dyn PkgJsonCache,
) -> Response {
    let project_root = Path::new(cwd);

    // Canonicalize the path
    let project_root = match project_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::CWD_INVALID,
                format!("Invalid working directory '{cwd}': {e}"),
            );
        }
    };

    let opts = GraphOptions {
        max_depth: max_depth as usize,
        include_optional,
        include_dev_root,
    };

    debug!(
        cwd = %project_root.display(),
        include_dev_root,
        include_optional,
        max_depth,
        "Building package graph"
    );

    // Build the graph using core function
    let core_graph = build_pkg_graph(&project_root, &opts, pkg_json_cache);

    debug!(
        nodes = core_graph.nodes.len(),
        orphans = core_graph.orphans.len(),
        errors = core_graph.errors.len(),
        "Package graph built"
    );

    // Convert core types to proto types
    let proto_graph = convert_graph_to_proto(core_graph);

    Response::PkgGraphResult { graph: proto_graph }
}

/// Convert core graph types to protocol types.
fn convert_graph_to_proto(core: fastnode_core::pkg::PackageGraph) -> PackageGraph {
    PackageGraph {
        schema_version: PKG_GRAPH_SCHEMA_VERSION,
        root: core.root,
        nodes: core
            .nodes
            .into_iter()
            .map(|node| GraphPackageNode {
                id: convert_package_id(node.id),
                dependencies: node
                    .dependencies
                    .into_iter()
                    .map(|edge| GraphDepEdge {
                        name: edge.name,
                        req: edge.req,
                        to: edge.to.map(convert_package_id),
                        kind: edge.kind,
                    })
                    .collect(),
            })
            .collect(),
        orphans: core.orphans.into_iter().map(convert_package_id).collect(),
        errors: core
            .errors
            .into_iter()
            .map(|e| GraphErrorInfo {
                code: e.code,
                path: e.path,
                message: e.message,
            })
            .collect(),
    }
}

/// Convert a core PackageId to proto GraphPackageId.
fn convert_package_id(id: fastnode_core::pkg::PackageId) -> GraphPackageId {
    GraphPackageId {
        name: id.name,
        version: id.version,
        path: id.path,
        integrity: id.integrity,
    }
}

/// Calculate the size of a directory recursively.
fn calculate_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut size = 0;

    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                size += calculate_dir_size(&path)?;
            } else {
                size += entry.metadata()?.len();
            }
        }
    } else {
        size = std::fs::metadata(path)?.len();
    }

    Ok(size)
}

/// Handle a PkgExplain request.
pub fn handle_pkg_explain(
    specifier: &str,
    cwd: &str,
    parent: &str,
    channel: &str,
    kind: &str,
    pkg_json_cache: &dyn PkgJsonCache,
) -> Response {
    use std::path::PathBuf;

    // Validate specifier
    if specifier.is_empty() {
        return Response::error(
            codes::PKG_EXPLAIN_SPECIFIER_INVALID,
            "Specifier cannot be empty",
        );
    }

    // Validate and parse cwd
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Response::error(
            codes::PKG_EXPLAIN_CWD_INVALID,
            format!("Working directory does not exist: {}", cwd),
        );
    }
    let cwd_canonical = match cwd_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::PKG_EXPLAIN_CWD_INVALID,
                format!("Cannot canonicalize working directory '{}': {}", cwd, e),
            );
        }
    };

    // Validate and parse parent
    let parent_path = PathBuf::from(parent);
    if !parent_path.is_dir() {
        return Response::error(
            codes::PKG_EXPLAIN_PARENT_INVALID,
            format!("Parent directory does not exist: {}", parent),
        );
    }
    let parent_canonical = match parent_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::PKG_EXPLAIN_PARENT_INVALID,
                format!("Cannot canonicalize parent directory '{}': {}", parent, e),
            );
        }
    };

    // Parse resolution kind
    let resolution_kind = match kind.to_lowercase().as_str() {
        "import" => ResolutionKind::Import,
        "require" => ResolutionKind::Require,
        "auto" | "unknown" | "" => ResolutionKind::Unknown,
        _ => {
            return Response::error(
                codes::PKG_EXPLAIN_KIND_INVALID,
                format!(
                    "Invalid resolution kind '{}'. Expected 'import', 'require', or 'auto'.",
                    kind
                ),
            );
        }
    };

    debug!(
        specifier = specifier,
        cwd = %cwd_canonical.display(),
        parent = %parent_canonical.display(),
        kind = ?resolution_kind,
        "Explaining module resolution"
    );

    // Create resolver context
    let config = ResolverConfig::default();
    let ctx = ResolveContext {
        cwd: cwd_canonical.clone(),
        parent: parent_canonical.clone(),
        channel: channel.to_string(),
        config: &config,
        pkg_json_cache: Some(pkg_json_cache),
    };

    // Perform resolution with tracing
    let traced_result = resolve_with_trace(&ctx, specifier, resolution_kind);

    // Convert to protocol types
    let trace_steps: Vec<PkgExplainTraceStep> = traced_result
        .trace
        .steps
        .into_iter()
        .map(|step| PkgExplainTraceStep {
            step: step.step.to_string(),
            ok: step.ok,
            detail: step.detail,
            path: step.path.map(|p| p.to_string_lossy().into_owned()),
            condition: step.condition,
            key: step.key,
            target: step.target,
            notes: step.notes,
        })
        .collect();

    let warnings: Vec<PkgExplainWarning> = traced_result
        .trace
        .warnings
        .into_iter()
        .map(|w| PkgExplainWarning {
            code: w.code,
            message: w.message,
        })
        .collect();

    let tried: Vec<String> = traced_result
        .result
        .tried
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    let (status, resolved, error_code, error_message) = match traced_result.result.resolved {
        Some(path) => (
            "resolved".to_string(),
            Some(path.to_string_lossy().into_owned()),
            None,
            None,
        ),
        None => {
            let reason = traced_result.result.reason;
            let code = reason.map(|r| r.to_string());
            let message = reason.map(|r| format!("Resolution failed: {}", r));
            ("unresolved".to_string(), None, code, message)
        }
    };

    let kind_str = match resolution_kind {
        ResolutionKind::Import => "import",
        ResolutionKind::Require => "require",
        ResolutionKind::Unknown => "unknown",
    };

    let result = PkgExplainResult {
        schema_version: PKG_EXPLAIN_SCHEMA_VERSION,
        specifier: specifier.to_string(),
        resolved,
        status,
        error_code,
        error_message,
        kind: kind_str.to_string(),
        parent: parent_canonical.to_string_lossy().into_owned(),
        trace: trace_steps,
        warnings,
        tried,
    };

    debug!(
        specifier = specifier,
        status = %result.status,
        resolved = ?result.resolved,
        trace_steps = result.trace.len(),
        "Module resolution explained"
    );

    Response::PkgExplainResult { result }
}

/// Options for why request.
pub struct WhyRequestOptions<'a> {
    pub arg: &'a str,
    pub cwd: &'a str,
    pub include_dev_root: bool,
    pub include_optional: bool,
    pub max_depth: u32,
    pub max_chains: u32,
    pub include_trace: bool,
    pub trace_kind: Option<&'a str>,
    pub trace_parent: Option<&'a str>,
}

/// Handle a PkgWhy request.
pub fn handle_pkg_why(opts: WhyRequestOptions<'_>, pkg_json_cache: &dyn PkgJsonCache) -> Response {
    use std::path::PathBuf;

    let arg = opts.arg;
    let cwd = opts.cwd;

    // Validate arg
    if arg.is_empty() {
        return Response::error(
            codes::PKG_WHY_ARGS_INVALID,
            "Package argument cannot be empty",
        );
    }

    // Validate max_chains range (1..=50)
    let max_chains = opts.max_chains.clamp(1, 50) as usize;

    // Validate and parse cwd
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Response::error(
            codes::CWD_INVALID,
            format!("Working directory does not exist: {}", cwd),
        );
    }
    let cwd_canonical = match cwd_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::CWD_INVALID,
                format!("Cannot canonicalize working directory '{}': {}", cwd, e),
            );
        }
    };

    debug!(
        arg = arg,
        cwd = %cwd_canonical.display(),
        include_dev_root = opts.include_dev_root,
        include_optional = opts.include_optional,
        max_depth = opts.max_depth,
        max_chains,
        include_trace = opts.include_trace,
        "Explaining why package is installed"
    );

    // Build the graph using the same options as pkg graph command
    let graph_opts = GraphOptions {
        max_depth: opts.max_depth as usize,
        include_optional: opts.include_optional,
        include_dev_root: opts.include_dev_root,
    };

    let core_graph = build_pkg_graph(&cwd_canonical, &graph_opts, pkg_json_cache);

    debug!(
        nodes = core_graph.nodes.len(),
        orphans = core_graph.orphans.len(),
        errors = core_graph.errors.len(),
        "Package graph built for why query"
    );

    // Compute why result
    let why_opts = WhyOptions {
        max_chains,
        prefer_shortest: true,
    };

    let core_result = why_from_graph(&core_graph, arg, &why_opts);

    // Convert core types to protocol types
    let mut result = convert_why_result_to_proto(core_result);

    // Optionally include resolver trace
    if opts.include_trace && result.found_in_node_modules {
        // Determine trace parent: use provided or default to cwd
        let trace_parent = opts.trace_parent.map(PathBuf::from).unwrap_or_else(|| {
            // Default parent policy: use cwd/index.js if exists, else cwd
            let index_js = cwd_canonical.join("index.js");
            if index_js.exists() {
                result
                    .notes
                    .push(format!("trace parent defaulted to: {}", index_js.display()));
                cwd_canonical.clone()
            } else {
                result.notes.push(format!(
                    "trace parent defaulted to: {}",
                    cwd_canonical.display()
                ));
                cwd_canonical.clone()
            }
        });

        let trace_parent_canonical = trace_parent.canonicalize().unwrap_or(trace_parent);
        let trace_kind = opts.trace_kind.unwrap_or("auto");

        // Call explain for the trace
        let trace_response = handle_pkg_explain(
            arg,
            &cwd_canonical.to_string_lossy(),
            &trace_parent_canonical.to_string_lossy(),
            "stable",
            trace_kind,
            pkg_json_cache,
        );

        if let Response::PkgExplainResult {
            result: explain_result,
        } = trace_response
        {
            result.trace = Some(explain_result);
        }
    }

    debug!(
        arg = arg,
        found = result.found_in_node_modules,
        is_orphan = result.is_orphan,
        chains = result.chains.len(),
        has_trace = result.trace.is_some(),
        "Why query completed"
    );

    Response::PkgWhyResult { result }
}

/// Convert core why result to protocol types.
fn convert_why_result_to_proto(core: CorePkgWhyResult) -> PkgWhyResult {
    PkgWhyResult {
        schema_version: PKG_WHY_SCHEMA_VERSION,
        cwd: core.cwd,
        target: PkgWhyTarget {
            name: core.target.name,
            version: core.target.version,
            path: core.target.path,
            input: core.target.input,
        },
        found_in_node_modules: core.found_in_node_modules,
        is_orphan: core.is_orphan,
        chains: core
            .chains
            .into_iter()
            .map(|chain| PkgWhyChain {
                links: chain
                    .links
                    .into_iter()
                    .map(|link| PkgWhyLink {
                        from: link.from,
                        to: link.to,
                        req: link.req,
                        resolved_version: link.resolved_version,
                        resolved_path: link.resolved_path,
                        kind: link.kind,
                    })
                    .collect(),
            })
            .collect(),
        notes: core.notes,
        errors: core
            .errors
            .into_iter()
            .map(|e| PkgWhyErrorInfo {
                code: e.code,
                message: e.message,
                path: e.path,
            })
            .collect(),
        trace: None, // Set by caller if trace is requested
    }
}

/// Options for doctor request.
pub struct DoctorRequestOptions<'a> {
    pub cwd: &'a str,
    pub include_dev_root: bool,
    pub include_optional: bool,
    pub max_depth: u32,
    pub min_severity: &'a str,
    pub max_items: u32,
}

/// Handle a PkgDoctor request.
pub fn handle_pkg_doctor(
    opts: DoctorRequestOptions<'_>,
    pkg_json_cache: &dyn PkgJsonCache,
) -> Response {
    use std::path::PathBuf;

    let cwd = opts.cwd;

    // Validate and parse cwd
    let cwd_path = PathBuf::from(cwd);
    if !cwd_path.is_dir() {
        return Response::error(
            codes::PKG_DOCTOR_CWD_INVALID,
            format!("Working directory does not exist: {}", cwd),
        );
    }
    let cwd_canonical = match cwd_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::PKG_DOCTOR_CWD_INVALID,
                format!("Cannot canonicalize working directory '{}': {}", cwd, e),
            );
        }
    };

    // Parse min_severity
    let min_severity = match DoctorSeverity::parse(opts.min_severity) {
        Some(s) => s,
        None => {
            return Response::error(
                codes::PKG_DOCTOR_SEVERITY_INVALID,
                format!(
                    "Invalid severity '{}'. Expected 'info', 'warn', or 'error'.",
                    opts.min_severity
                ),
            );
        }
    };

    // Clamp max_items to valid range
    let max_items = (opts.max_items as usize).clamp(1, 2000);

    debug!(
        cwd = %cwd_canonical.display(),
        include_dev_root = opts.include_dev_root,
        include_optional = opts.include_optional,
        max_depth = opts.max_depth,
        min_severity = ?min_severity,
        max_items,
        "Running package doctor"
    );

    // Build the graph
    let graph_opts = GraphOptions {
        max_depth: opts.max_depth as usize,
        include_optional: opts.include_optional,
        include_dev_root: opts.include_dev_root,
    };

    let core_graph = build_pkg_graph(&cwd_canonical, &graph_opts, pkg_json_cache);

    debug!(
        nodes = core_graph.nodes.len(),
        orphans = core_graph.orphans.len(),
        errors = core_graph.errors.len(),
        "Package graph built for doctor"
    );

    // Build doctor report
    let doctor_opts = DoctorOptions {
        include_dev_root: opts.include_dev_root,
        include_optional: opts.include_optional,
        max_depth: opts.max_depth as usize,
        max_items,
        min_severity,
    };

    let core_report =
        build_doctor_report(&core_graph, &cwd_canonical.to_string_lossy(), &doctor_opts);

    debug!(
        severity = ?core_report.summary.severity,
        findings = core_report.findings.len(),
        orphans = core_report.summary.orphans,
        missing_edges = core_report.summary.missing_edges,
        "Doctor report generated"
    );

    // Convert to protocol types
    let proto_report = convert_doctor_report_to_proto(core_report);

    Response::PkgDoctorResult {
        report: proto_report,
    }
}

/// Convert core doctor report to protocol types.
fn convert_doctor_report_to_proto(core: fastnode_core::pkg::PkgDoctorReport) -> PkgDoctorReport {
    PkgDoctorReport {
        schema_version: PKG_DOCTOR_SCHEMA_VERSION,
        cwd: core.cwd,
        summary: DoctorSummary {
            severity: core.summary.severity.as_str().to_string(),
            counts: DoctorCounts {
                info: core.summary.counts.info,
                warn: core.summary.counts.warn,
                error: core.summary.counts.error,
            },
            packages_indexed: core.summary.packages_indexed,
            reachable_packages: core.summary.reachable_packages,
            orphans: core.summary.orphans,
            missing_edges: core.summary.missing_edges,
            invalid_packages: core.summary.invalid_packages,
        },
        findings: core
            .findings
            .into_iter()
            .map(|f| DoctorFinding {
                code: f.code,
                severity: f.severity.as_str().to_string(),
                message: f.message,
                package: f.package,
                path: f.path,
                detail: f.detail,
                related: f.related,
            })
            .collect(),
        notes: core.notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_calculate_dir_size() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("file1.txt");
        let file2 = dir.path().join("file2.txt");

        std::fs::write(&file1, "hello").unwrap();
        std::fs::write(&file2, "world!").unwrap();

        let size = calculate_dir_size(dir.path()).unwrap();
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
    }

    #[test]
    fn test_handle_pkg_cache_list_empty() {
        let resp = handle_pkg_cache_list("test-channel");

        match resp {
            Response::PkgCacheListResult {
                packages,
                total_size_bytes,
            } => {
                // Result is valid regardless of package count
                let _ = packages.len();
                let _ = total_size_bytes;
            }
            _ => panic!("Expected PkgCacheListResult"),
        }
    }

    #[test]
    fn test_handle_pkg_cache_prune_stub() {
        let resp = handle_pkg_cache_prune("test-channel");

        match resp {
            Response::PkgCachePruneResult {
                removed_count,
                freed_bytes,
            } => {
                assert_eq!(removed_count, 0);
                assert_eq!(freed_bytes, 0);
            }
            _ => panic!("Expected PkgCachePruneResult"),
        }
    }
}
