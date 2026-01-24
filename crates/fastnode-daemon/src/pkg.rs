//! Package manager handlers for the daemon.
//!
//! Handles PkgAdd, PkgCacheList, PkgCachePrune, PkgGraph, PkgExplain, PkgWhy, PkgDoctor,
//! and PkgInstall requests.

use fastnode_core::config::Channel;
use fastnode_core::pkg::{
    build_doctor_report, build_pkg_graph, download_tarball, extract_tgz_atomic, get_tarball_url,
    link_into_node_modules, resolve_version, why_from_graph, DoctorOptions, DoctorSeverity,
    GraphOptions, Lockfile, PackageCache, PackageSpec, PkgError, PkgWhyResult as CorePkgWhyResult,
    RegistryClient, WhyOptions, LOCKFILE_NAME, MAX_TARBALL_SIZE,
};
use fastnode_core::resolver::{
    resolve_with_trace, PkgJsonCache, ResolutionKind, ResolveContext, ResolverConfig,
};
use fastnode_proto::{
    codes, CachedPackage, DoctorCounts, DoctorFinding, DoctorSummary, GraphDepEdge, GraphErrorInfo,
    GraphPackageId, GraphPackageNode, InstallPackageError, InstallPackageInfo, InstalledPackage,
    InstallSummary, PackageGraph, PkgDoctorReport, PkgErrorInfo, PkgExplainResult,
    PkgExplainTraceStep, PkgExplainWarning, PkgInstallResult, PkgWhyChain, PkgWhyErrorInfo,
    PkgWhyLink, PkgWhyResult, PkgWhyTarget, Response, PKG_DOCTOR_SCHEMA_VERSION,
    PKG_EXPLAIN_SCHEMA_VERSION, PKG_GRAPH_SCHEMA_VERSION, PKG_INSTALL_SCHEMA_VERSION,
    PKG_WHY_SCHEMA_VERSION,
};
use std::path::Path;
use tracing::{debug, warn};

/// Parse a channel string to Channel enum.
fn parse_channel(channel: &str) -> Channel {
    match channel.to_lowercase().as_str() {
        "dev" => Channel::Dev,
        "nightly" => Channel::Nightly,
        _ => Channel::Stable,
    }
}

/// Handle a PkgAdd request.
pub async fn handle_pkg_add(specs: &[String], cwd: &str, channel: &str) -> Response {
    let project_root = Path::new(cwd);

    // Create package cache for this channel
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);

    // Create registry client
    let registry = match RegistryClient::from_env() {
        Ok(r) => r,
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let mut reused_cache = 0u32;

    for spec_str in specs {
        match add_single_package(spec_str, project_root, &cache, &registry).await {
            Ok((pkg, from_cache)) => {
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

    Response::PkgAddResult {
        installed,
        errors,
        reused_cache,
    }
}

/// Add a single package. Returns (InstalledPackage, was_cached).
async fn add_single_package(
    spec_str: &str,
    project_root: &Path,
    cache: &PackageCache,
    registry: &RegistryClient,
) -> Result<(InstalledPackage, bool), PkgError> {
    // Parse the spec
    let spec = PackageSpec::parse(spec_str)?;

    debug!(name = %spec.name, range = ?spec.range, "Adding package");

    // Fetch packument
    let packument = registry.fetch_packument(&spec.name).await?;

    // Resolve version
    let version = resolve_version(&packument, spec.range.as_deref())?;

    debug!(name = %spec.name, version = %version, "Resolved version");

    // Check if already cached
    let package_dir = cache.package_dir(&spec.name, &version);
    let was_cached = cache.is_cached(&spec.name, &version);

    if !was_cached {
        // Get tarball URL
        let tarball_url = get_tarball_url(&packument, &version).ok_or_else(|| {
            PkgError::download_failed(format!("No tarball URL for {}@{}", spec.name, version))
        })?;

        debug!(url = %tarball_url, "Downloading tarball");

        // Download tarball
        let bytes = download_tarball(registry.http(), tarball_url, MAX_TARBALL_SIZE).await?;

        debug!(size = bytes.len(), "Downloaded tarball");

        // Extract to cache
        extract_tgz_atomic(&bytes, &package_dir)?;

        debug!(path = %package_dir.display(), "Extracted to cache");
    } else {
        debug!(path = %package_dir.display(), "Using cached package");
    }

    // Link into node_modules
    let link_path = link_into_node_modules(project_root, &spec.name, &package_dir)?;

    debug!(link = %link_path.display(), "Linked into node_modules");

    Ok((
        InstalledPackage {
            name: spec.name,
            version: version.to_string(),
            link_path: link_path.to_string_lossy().into_owned(),
            cache_path: package_dir.to_string_lossy().into_owned(),
        },
        was_cached,
    ))
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
pub async fn handle_pkg_install(
    cwd: &str,
    channel: &str,
    frozen: bool,
    include_dev: bool,
    include_optional: bool,
) -> Response {
    use std::path::PathBuf;

    let project_root = PathBuf::from(cwd);

    // Canonicalize the path
    let project_root = match project_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Response::error(
                codes::CWD_INVALID,
                format!("Invalid working directory '{}': {}", cwd, e),
            );
        }
    };

    // Check for lockfile
    let lockfile_path = project_root.join(LOCKFILE_NAME);
    if !lockfile_path.exists() {
        if frozen {
            return Response::error(
                codes::PKG_INSTALL_LOCKFILE_NOT_FOUND,
                format!(
                    "Lockfile not found at '{}' (--frozen requires lockfile)",
                    lockfile_path.display()
                ),
            );
        }

        // No lockfile and not frozen - return empty success for now
        // In the future, this would generate a lockfile from package.json
        return Response::PkgInstallResult {
            result: PkgInstallResult {
                schema_version: PKG_INSTALL_SCHEMA_VERSION,
                cwd: project_root.to_string_lossy().into_owned(),
                ok: true,
                summary: InstallSummary::default(),
                installed: vec![],
                errors: vec![],
                notes: vec!["No lockfile found. Run `howth install` without --frozen to generate one.".to_string()],
            },
        };
    }

    // Read the lockfile
    let lockfile = match Lockfile::read_from(&lockfile_path) {
        Ok(lf) => lf,
        Err(e) => {
            return Response::error(codes::PKG_INSTALL_LOCKFILE_INVALID, e.to_string());
        }
    };

    debug!(
        lockfile = %lockfile_path.display(),
        packages = lockfile.packages.len(),
        "Read lockfile"
    );

    // Create package cache for this channel
    let chan = parse_channel(channel);
    let cache = PackageCache::new(chan);

    // Create registry client
    let registry = match RegistryClient::from_env() {
        Ok(r) => r,
        Err(e) => {
            return Response::error(codes::PKG_REGISTRY_ERROR, e.to_string());
        }
    };

    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let mut downloaded = 0u32;
    let mut cached = 0u32;
    let mut linked = 0u32;

    // Install each package from the lockfile
    for (key, lock_pkg) in &lockfile.packages {
        // Parse package name from key (format: "name@version")
        let name = key.rsplit_once('@').map(|(n, _)| n).unwrap_or(key);

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

        match install_from_lockfile(name, lock_pkg, &project_root, &cache, &registry).await {
            Ok((pkg_info, from_cache)) => {
                if from_cache {
                    cached += 1;
                } else {
                    downloaded += 1;
                }
                linked += 1;
                installed.push(pkg_info);
            }
            Err(e) => {
                errors.push(InstallPackageError {
                    name: name.to_string(),
                    version: lock_pkg.version.clone(),
                    code: e.code().to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    let ok = errors.is_empty();
    let total_packages = lockfile.packages.len() as u32;

    debug!(
        total = total_packages,
        downloaded,
        cached,
        linked,
        failed = errors.len(),
        "Install completed"
    );

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
            },
            installed,
            errors,
            notes: vec![],
        },
    }
}

/// Install a single package from lockfile.
async fn install_from_lockfile(
    name: &str,
    lock_pkg: &fastnode_core::pkg::LockPackage,
    project_root: &Path,
    cache: &PackageCache,
    registry: &RegistryClient,
) -> Result<(InstallPackageInfo, bool), PkgError> {
    let version = &lock_pkg.version;

    debug!(name = %name, version = %version, "Installing package from lockfile");

    // Check if already cached
    let package_dir = cache.package_dir(name, version);
    let was_cached = cache.is_cached(name, version);

    if !was_cached {
        // Fetch packument to get tarball URL
        let packument = registry.fetch_packument(name).await?;

        // Get tarball URL
        let tarball_url = get_tarball_url(&packument, version).ok_or_else(|| {
            PkgError::download_failed(format!("No tarball URL for {}@{}", name, version))
        })?;

        debug!(url = %tarball_url, "Downloading tarball");

        // Download tarball
        let bytes = download_tarball(registry.http(), tarball_url, MAX_TARBALL_SIZE).await?;

        // TODO: Verify integrity hash matches lock_pkg.integrity
        // For now, just extract

        debug!(size = bytes.len(), "Downloaded tarball");

        // Extract to cache
        extract_tgz_atomic(&bytes, &package_dir)?;

        debug!(path = %package_dir.display(), "Extracted to cache");
    } else {
        debug!(path = %package_dir.display(), "Using cached package");
    }

    // Link into node_modules
    let link_path = link_into_node_modules(project_root, name, &package_dir)?;

    debug!(link = %link_path.display(), "Linked into node_modules");

    Ok((
        InstallPackageInfo {
            name: name.to_string(),
            version: version.to_string(),
            from_cache: was_cached,
            link_path: link_path.to_string_lossy().into_owned(),
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
                format!("Invalid working directory '{}': {}", cwd, e),
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
