//! Resolver v0/v1.1/v1.2 implementation.
//!
//! Supports:
//! - Relative specifiers: `./`, `../`
//! - Absolute filesystem specifiers
//! - Bare specifiers with `node_modules` lookup
//! - Extension probing
//! - Directory resolution (`index.*`, `package.json` main)
//! - v1.1: package.json exports field (root only)
//! - v1.1: package.json imports field (#-prefixed specifiers)
//! - v1.2: package.json exports subpath keys (`"./feature"`)
//! - v1.2: package.json exports pattern keys (`"./*"`)

use super::exports::{resolve_exports, resolve_exports_root, resolve_imports_map, ResolutionKind};
use super::pkg_json_cache::PkgJsonCache;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Default extensions for probing.
pub const DEFAULT_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json"];

/// Maximum number of tried paths to record.
const MAX_TRIED_PATHS: usize = 20;

/// Resolver configuration.
#[derive(Debug, Clone)]
pub struct ResolverConfig {
    /// Extensions to probe (in order).
    pub extensions: &'static [&'static str],
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            extensions: DEFAULT_EXTENSIONS,
        }
    }
}

/// Context for resolution.
#[derive(Debug, Clone)]
pub struct ResolveContext<'a> {
    /// Working directory.
    pub cwd: PathBuf,
    /// Directory containing the importing file.
    pub parent: PathBuf,
    /// Channel name.
    pub channel: String,
    /// Resolver configuration.
    pub config: &'a ResolverConfig,
    /// Optional package.json cache for v1.1 exports/imports.
    pub pkg_json_cache: Option<&'a dyn PkgJsonCache>,
}

/// Read and parse package.json, using cache if available.
fn read_pkg_json_cached(path: &Path, cache: Option<&dyn PkgJsonCache>) -> Option<Value> {
    // Try cache first
    if let Some(c) = cache {
        if let Some(value) = c.get(path) {
            return Some(value);
        }
    }

    // Read from disk
    let content = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;

    // Store in cache if available
    if let Some(c) = cache {
        c.set(path, value.clone());
    }

    Some(value)
}

/// Resolution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveStatus {
    Resolved,
    Unresolved,
}

/// Reason codes for unresolved imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveReasonCode {
    SpecifierInvalid,
    UnsupportedScheme,
    NotFound,
    IsDirectory,
    NodeModulesNotFound,
    PackageJsonInvalid,
    PackageMainNotFound,
    /// v1.1: exports field target not found (file doesn't exist)
    ExportsTargetNotFound,
    /// v1.1: imports field specifier not found
    ImportsNotFound,
    /// v1.2: exports field exists but no matching key for subpath
    ExportsNotFound,
}

impl std::fmt::Display for ResolveReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::SpecifierInvalid => "SPECIFIER_INVALID",
            Self::UnsupportedScheme => "UNSUPPORTED_SCHEME",
            Self::NotFound => "NOT_FOUND",
            Self::IsDirectory => "IS_DIRECTORY",
            Self::NodeModulesNotFound => "NODE_MODULES_NOT_FOUND",
            Self::PackageJsonInvalid => "PACKAGE_JSON_INVALID",
            Self::PackageMainNotFound => "PACKAGE_MAIN_NOT_FOUND",
            Self::ExportsTargetNotFound => "EXPORTS_TARGET_NOT_FOUND",
            Self::ImportsNotFound => "IMPORTS_NOT_FOUND",
            Self::ExportsNotFound => "EXPORTS_NOT_FOUND",
        };
        write!(f, "{s}")
    }
}

/// Resolution result.
#[derive(Debug, Clone)]
pub struct ResolveResult {
    /// Resolved absolute path (if successful).
    pub resolved: Option<PathBuf>,
    /// Status.
    pub status: ResolveStatus,
    /// Reason code if unresolved.
    pub reason: Option<ResolveReasonCode>,
    /// Candidate paths tried (capped).
    pub tried: Vec<PathBuf>,
}

impl ResolveResult {
    fn resolved(path: PathBuf, tried: Vec<PathBuf>) -> Self {
        Self {
            resolved: Some(path),
            status: ResolveStatus::Resolved,
            reason: None,
            tried,
        }
    }

    fn unresolved(reason: ResolveReasonCode, tried: Vec<PathBuf>) -> Self {
        Self {
            resolved: None,
            status: ResolveStatus::Unresolved,
            reason: Some(reason),
            tried,
        }
    }
}

/// Resolver cache trait for daemon caching.
pub trait ResolverCache: Send + Sync {
    /// Look up a cached result.
    fn get(&self, key: &ResolverCacheKey) -> Option<CachedResolveResult>;

    /// Store a result.
    fn set(&self, key: ResolverCacheKey, value: CachedResolveResult);
}

/// No-op cache implementation (always misses).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCache;

impl ResolverCache for NoCache {
    fn get(&self, _key: &ResolverCacheKey) -> Option<CachedResolveResult> {
        None
    }

    fn set(&self, _key: ResolverCacheKey, _value: CachedResolveResult) {
        // No-op
    }
}

/// Cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolverCacheKey {
    pub cwd: String,
    pub parent: String,
    pub specifier: String,
    pub channel: String,
}

/// Cached resolve result with file stamp.
#[derive(Debug, Clone)]
pub struct CachedResolveResult {
    pub resolved: Option<String>,
    pub status: String,
    pub reason: Option<String>,
    pub tried: Vec<String>,
    pub stamp: FileStamp,
}

/// File stamp for cache invalidation.
#[derive(Debug, Clone, Default)]
pub struct FileStamp {
    pub path: Option<String>,
    pub mtime_ms: Option<u64>,
    pub size: Option<u64>,
}

impl FileStamp {
    /// Create stamp from a path.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn from_path(path: &Path) -> Self {
        if let Ok(meta) = path.metadata() {
            // Safe to truncate: u64 millis since epoch is good for ~584 million years
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            Self {
                path: Some(path.to_string_lossy().into_owned()),
                mtime_ms,
                size: Some(meta.len()),
            }
        } else {
            Self {
                path: Some(path.to_string_lossy().into_owned()),
                mtime_ms: None,
                size: None,
            }
        }
    }

    /// Check if stamp is still valid.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn is_valid(&self) -> bool {
        let Some(ref path_str) = self.path else {
            // No path means unresolved - consider valid
            return true;
        };

        let path = Path::new(path_str);
        let Ok(meta) = path.metadata() else {
            // File no longer exists
            return false;
        };

        // Check mtime
        if let Some(expected_mtime) = self.mtime_ms {
            let current_mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            if current_mtime != Some(expected_mtime) {
                return false;
            }
        }

        // Check size
        if let Some(expected_size) = self.size {
            if meta.len() != expected_size {
                return false;
            }
        }

        true
    }
}

/// Resolve a specifier using v0 algorithm (defaults to Unknown resolution kind).
#[must_use]
pub fn resolve_v0(ctx: &ResolveContext<'_>, spec: &str) -> ResolveResult {
    resolve_with_kind(ctx, spec, ResolutionKind::Unknown)
}

/// Resolve a specifier with explicit resolution kind (v1.1).
///
/// The resolution kind affects which conditional exports are selected
/// from package.json exports field.
#[must_use]
pub fn resolve_with_kind(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
) -> ResolveResult {
    let mut tried = Vec::new();

    // Validate specifier
    if spec.is_empty() {
        return ResolveResult::unresolved(ResolveReasonCode::SpecifierInvalid, tried);
    }

    // Check for URL-like specifiers
    if spec.contains("://") || spec.starts_with("node:") || spec.starts_with("data:") {
        return ResolveResult::unresolved(ResolveReasonCode::UnsupportedScheme, tried);
    }

    // v1.1: Handle #-prefixed imports (package.json imports field)
    if spec.starts_with('#') {
        return resolve_hash_import(ctx, spec, kind, &mut tried);
    }

    // Relative specifier
    if spec.starts_with("./") || spec.starts_with("../") {
        return resolve_relative(ctx, spec, kind, &mut tried);
    }

    // Absolute specifier
    if is_absolute_path(spec) {
        return resolve_absolute(ctx, spec, kind, &mut tried);
    }

    // Bare specifier - node_modules lookup
    resolve_bare(ctx, spec, kind, &mut tried)
}

/// Check if a specifier is an absolute path.
fn is_absolute_path(spec: &str) -> bool {
    // Unix absolute
    if spec.starts_with('/') {
        return true;
    }

    // Windows absolute: C:\, D:\, etc.
    let chars: Vec<char> = spec.chars().collect();
    if chars.len() >= 3
        && chars[0].is_ascii_alphabetic()
        && chars[1] == ':'
        && (chars[2] == '\\' || chars[2] == '/')
    {
        return true;
    }

    // UNC path: \\server\share
    if spec.starts_with("\\\\") {
        return true;
    }

    false
}

/// Resolve a #-prefixed import using package.json imports field.
fn resolve_hash_import(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    // Find the nearest package.json by walking up from parent
    let mut current = Some(ctx.parent.as_path());

    while let Some(dir) = current {
        let pkg_json_path = dir.join("package.json");

        if pkg_json_path.is_file() {
            add_tried(tried, &pkg_json_path);

            if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
                if let Some(target) = resolve_imports_map(&pkg_json, spec, kind) {
                    // Target is relative to package root (dir)
                    let target_path = dir.join(target.trim_start_matches("./"));
                    return resolve_path(ctx, &target_path, kind, tried);
                }
            }

            // Found package.json but no matching import
            return ResolveResult::unresolved(ResolveReasonCode::ImportsNotFound, tried.clone());
        }

        current = dir.parent();
    }

    // No package.json found
    ResolveResult::unresolved(ResolveReasonCode::ImportsNotFound, tried.clone())
}

/// Resolve a relative specifier.
fn resolve_relative(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    let base = ctx.parent.join(spec);
    resolve_path(ctx, &base, kind, tried)
}

/// Resolve an absolute specifier.
fn resolve_absolute(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    let path = PathBuf::from(spec);
    resolve_path(ctx, &path, kind, tried)
}

/// Resolve a path (with extension probing and directory resolution).
fn resolve_path(
    ctx: &ResolveContext<'_>,
    base: &Path,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    // Try exact path first
    if base.is_file() {
        let canonical = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        return ResolveResult::resolved(canonical, tried.clone());
    }

    // If it's a directory, try directory resolution
    if base.is_dir() {
        return resolve_directory(ctx, base, kind, tried);
    }

    // Try extension probing
    for ext in ctx.config.extensions {
        let with_ext = base.with_extension(ext.trim_start_matches('.'));
        add_tried(tried, &with_ext);

        if with_ext.is_file() {
            let canonical = with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
            return ResolveResult::resolved(canonical, tried.clone());
        }
    }

    // Try as directory (if base + /index.* exists)
    if base.exists() {
        return resolve_directory(ctx, base, kind, tried);
    }

    // Also try base as directory even if it doesn't exist yet
    // (for cases like "./foo" where "./foo/index.js" exists)
    let dir_result = resolve_directory(ctx, base, kind, tried);
    if dir_result.status == ResolveStatus::Resolved {
        return dir_result;
    }

    ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone())
}

/// Resolve a directory (package.json exports > main > index.*).
fn resolve_directory(
    ctx: &ResolveContext<'_>,
    dir: &Path,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    let pkg_json_path = dir.join("package.json");

    if pkg_json_path.is_file() {
        add_tried(tried, &pkg_json_path);

        // Read and parse package.json (with cache if available)
        if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
            // v1.1: Try exports field first (for root resolution only)
            if let Some(exports_target) = resolve_exports_root(&pkg_json, kind) {
                let target_path = dir.join(exports_target.trim_start_matches("./"));
                add_tried(tried, &target_path);

                // Try exact path
                if target_path.is_file() {
                    let canonical = target_path
                        .canonicalize()
                        .unwrap_or_else(|_| target_path.clone());
                    return ResolveResult::resolved(canonical, tried.clone());
                }

                // Try with extension probing
                for ext in ctx.config.extensions {
                    let with_ext = target_path.with_extension(ext.trim_start_matches('.'));
                    add_tried(tried, &with_ext);

                    if with_ext.is_file() {
                        let canonical =
                            with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                        return ResolveResult::resolved(canonical, tried.clone());
                    }
                }

                // exports target specified but not found
                return ResolveResult::unresolved(
                    ResolveReasonCode::ExportsTargetNotFound,
                    tried.clone(),
                );
            }

            // Fall back to main field
            if let Some(main) = pkg_json.get("main").and_then(|v| v.as_str()) {
                let main_path = dir.join(main);

                // Try exact main path
                if main_path.is_file() {
                    let canonical = main_path
                        .canonicalize()
                        .unwrap_or_else(|_| main_path.clone());
                    return ResolveResult::resolved(canonical, tried.clone());
                }

                // Try main with extension probing
                for ext in ctx.config.extensions {
                    let with_ext = main_path.with_extension(ext.trim_start_matches('.'));
                    add_tried(tried, &with_ext);

                    if with_ext.is_file() {
                        let canonical =
                            with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                        return ResolveResult::resolved(canonical, tried.clone());
                    }
                }

                // Try main/index.*
                if main_path.is_dir() {
                    for ext in ctx.config.extensions {
                        let index = main_path.join(format!("index{ext}"));
                        add_tried(tried, &index);

                        if index.is_file() {
                            let canonical = index.canonicalize().unwrap_or_else(|_| index.clone());
                            return ResolveResult::resolved(canonical, tried.clone());
                        }
                    }
                }
            }
        }
    }

    // Try index.* files
    for ext in ctx.config.extensions {
        let index = dir.join(format!("index{ext}"));
        add_tried(tried, &index);

        if index.is_file() {
            let canonical = index.canonicalize().unwrap_or_else(|_| index.clone());
            return ResolveResult::resolved(canonical, tried.clone());
        }
    }

    // Directory exists but no resolvable entry
    if dir.is_dir() {
        return ResolveResult::unresolved(ResolveReasonCode::IsDirectory, tried.clone());
    }

    ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone())
}

/// Resolve a bare specifier via `node_modules`.
fn resolve_bare(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    // Parse package name from specifier
    // e.g., "lodash/fp" -> "lodash", "@scope/pkg/sub" -> "@scope/pkg"
    let (pkg_name, subpath) = parse_bare_specifier(spec);

    let mut found_node_modules = false;
    let mut found_package = false;
    let mut specific_error: Option<ResolveReasonCode> = None;
    let mut current = Some(ctx.parent.as_path());

    while let Some(dir) = current {
        let node_modules = dir.join("node_modules");

        if node_modules.is_dir() {
            found_node_modules = true;

            let pkg_dir = node_modules.join(pkg_name);
            add_tried(tried, &pkg_dir);

            if pkg_dir.is_dir() {
                found_package = true;

                // Package found - handle root vs subpath resolution
                if let Some(sub) = subpath {
                    // v1.2: Subpath resolution - try exports first
                    let result = resolve_package_subpath(ctx, &pkg_dir, sub, kind, tried);
                    if result.status == ResolveStatus::Resolved {
                        return result;
                    }

                    // Capture specific error codes
                    if let Some(reason) = result.reason {
                        match reason {
                            ResolveReasonCode::ExportsTargetNotFound
                            | ResolveReasonCode::ExportsNotFound
                            | ResolveReasonCode::ImportsNotFound
                            | ResolveReasonCode::PackageMainNotFound => {
                                specific_error = Some(reason);
                            }
                            _ => {}
                        }
                    }
                } else {
                    // Root package resolution - use resolve_directory (handles exports)
                    let result = resolve_path(ctx, &pkg_dir, kind, tried);
                    if result.status == ResolveStatus::Resolved {
                        return result;
                    }

                    // Capture specific error codes
                    if let Some(reason) = result.reason {
                        match reason {
                            ResolveReasonCode::ExportsTargetNotFound
                            | ResolveReasonCode::ImportsNotFound
                            | ResolveReasonCode::PackageMainNotFound => {
                                specific_error = Some(reason);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Walk up
        current = dir.parent();
    }

    // Return specific error if we found the package but resolution failed
    if let Some(error) = specific_error {
        return ResolveResult::unresolved(error, tried.clone());
    }

    if found_node_modules {
        if found_package {
            // Package was found but no entry point resolved
            ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone())
        } else {
            ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone())
        }
    } else {
        ResolveResult::unresolved(ResolveReasonCode::NodeModulesNotFound, tried.clone())
    }
}

/// Resolve a package subpath using exports field (v1.2).
///
/// Tries exports subpath/pattern resolution first, then falls back to
/// direct filesystem resolution if exports doesn't define the subpath.
fn resolve_package_subpath(
    ctx: &ResolveContext<'_>,
    pkg_dir: &Path,
    subpath: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
) -> ResolveResult {
    let pkg_json_path = pkg_dir.join("package.json");

    if pkg_json_path.is_file() {
        add_tried(tried, &pkg_json_path);

        if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
            // Check if exports field exists
            let has_exports = pkg_json.get("exports").is_some();

            if has_exports {
                // Convert subpath to exports format: "feature" -> "./feature"
                let exports_subpath = format!("./{subpath}");

                // v1.2: Try exports subpath/pattern resolution
                if let Some(exports_target) =
                    resolve_exports(&pkg_json, Some(&exports_subpath), kind)
                {
                    let target_path = pkg_dir.join(exports_target.trim_start_matches("./"));
                    add_tried(tried, &target_path);

                    // Try exact path
                    if target_path.is_file() {
                        let canonical = target_path
                            .canonicalize()
                            .unwrap_or_else(|_| target_path.clone());
                        return ResolveResult::resolved(canonical, tried.clone());
                    }

                    // Try with extension probing
                    for ext in ctx.config.extensions {
                        let with_ext = target_path.with_extension(ext.trim_start_matches('.'));
                        add_tried(tried, &with_ext);

                        if with_ext.is_file() {
                            let canonical =
                                with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                            return ResolveResult::resolved(canonical, tried.clone());
                        }
                    }

                    // exports target specified but file not found
                    return ResolveResult::unresolved(
                        ResolveReasonCode::ExportsTargetNotFound,
                        tried.clone(),
                    );
                }

                // exports exists but no matching key for this subpath
                return ResolveResult::unresolved(
                    ResolveReasonCode::ExportsNotFound,
                    tried.clone(),
                );
            }
        }
    }

    // No exports field or no package.json - fall back to direct filesystem resolution
    let target_path = pkg_dir.join(subpath);
    resolve_path(ctx, &target_path, kind, tried)
}

/// Parse a bare specifier into package name and optional subpath.
fn parse_bare_specifier(spec: &str) -> (&str, Option<&str>) {
    // Scoped package: @scope/pkg or @scope/pkg/subpath
    if spec.starts_with('@') {
        // Find second slash
        let mut slash_count = 0;
        for (i, c) in spec.char_indices() {
            if c == '/' {
                slash_count += 1;
                if slash_count == 2 {
                    return (&spec[..i], Some(&spec[i + 1..]));
                }
            }
        }
        // No subpath
        return (spec, None);
    }

    // Regular package: pkg or pkg/subpath
    if let Some(pos) = spec.find('/') {
        (&spec[..pos], Some(&spec[pos + 1..]))
    } else {
        (spec, None)
    }
}

/// Add a path to tried list (with cap).
fn add_tried(tried: &mut Vec<PathBuf>, path: &Path) {
    if tried.len() < MAX_TRIED_PATHS {
        tried.push(path.to_path_buf());
    }
}

// =============================================================================
// Resolution with tracing (v1.5)
// =============================================================================

use super::trace::{steps, warning_codes, ResolveTrace, ResolveTraceStep, TraceWarning};

/// Result of resolution with trace.
#[derive(Debug, Clone)]
pub struct ResolveResultWithTrace {
    /// The resolution result.
    pub result: ResolveResult,
    /// The resolution trace.
    pub trace: ResolveTrace,
}

/// Resolve a specifier with tracing enabled.
///
/// This performs the same resolution as `resolve_with_kind` but also
/// builds a detailed trace of each step for debugging and explaining
/// why a specifier resolves to a particular file.
#[must_use]
pub fn resolve_with_trace(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
) -> ResolveResultWithTrace {
    let mut trace = ResolveTrace::new();
    let mut tried = Vec::new();

    // Step 1: Parse specifier
    if spec.is_empty() {
        trace.failure(steps::PARSE_SPECIFIER, "Specifier is empty");
        return ResolveResultWithTrace {
            result: ResolveResult::unresolved(ResolveReasonCode::SpecifierInvalid, tried),
            trace,
        };
    }
    trace.success(steps::PARSE_SPECIFIER, format!("Parsed specifier: {spec}"));

    // Step 2: Classify specifier type
    let specifier_type =
        if spec.contains("://") || spec.starts_with("node:") || spec.starts_with("data:") {
            "url_scheme"
        } else if spec.starts_with('#') {
            "hash_import"
        } else if spec.starts_with("./") || spec.starts_with("../") {
            "relative"
        } else if is_absolute_path(spec) {
            "absolute"
        } else {
            "bare"
        };

    trace.add_step(ResolveTraceStep::new(
        steps::CLASSIFY_SPECIFIER,
        true,
        format!("Specifier type: {specifier_type}"),
    ));

    // Handle URL-like specifiers
    if specifier_type == "url_scheme" {
        trace.failure(
            steps::CLASSIFY_SPECIFIER,
            format!("URL schemes not supported: {spec}"),
        );
        return ResolveResultWithTrace {
            result: ResolveResult::unresolved(ResolveReasonCode::UnsupportedScheme, tried),
            trace,
        };
    }

    // Route to appropriate resolution
    match specifier_type {
        "hash_import" => resolve_hash_import_traced(ctx, spec, kind, &mut tried, &mut trace),
        "relative" => resolve_relative_traced(ctx, spec, kind, &mut tried, &mut trace),
        "absolute" => resolve_absolute_traced(ctx, spec, kind, &mut tried, &mut trace),
        "bare" => resolve_bare_traced(ctx, spec, kind, &mut tried, &mut trace),
        _ => ResolveResultWithTrace {
            result: ResolveResult::unresolved(ResolveReasonCode::SpecifierInvalid, tried),
            trace,
        },
    }
}

/// Resolve a #-prefixed import with tracing.
fn resolve_hash_import_traced(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    trace.add_step(ResolveTraceStep::new(
        steps::RESOLVE_HASH_IMPORT,
        true,
        format!("Resolving hash import: {spec}"),
    ));

    // Find the nearest package.json
    let mut current = Some(ctx.parent.as_path());

    while let Some(dir) = current {
        let pkg_json_path = dir.join("package.json");

        if pkg_json_path.is_file() {
            add_tried(tried, &pkg_json_path);
            trace.add_step(
                ResolveTraceStep::new(steps::FIND_PACKAGE_JSON, true, "Found package.json")
                    .with_path(&pkg_json_path),
            );

            if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
                // Check imports field
                if pkg_json.get("imports").is_some() {
                    trace.add_step(ResolveTraceStep::new(
                        steps::READ_IMPORTS_FIELD,
                        true,
                        "Package has imports field",
                    ));

                    if let Some(target) = resolve_imports_map(&pkg_json, spec, kind) {
                        trace.add_step(
                            ResolveTraceStep::new(
                                steps::MATCH_IMPORTS_KEY,
                                true,
                                format!("Matched imports key: {spec}"),
                            )
                            .with_key(spec)
                            .with_target(&target)
                            .with_condition(kind.to_string()),
                        );

                        // Target is relative to package root
                        let target_path = dir.join(target.trim_start_matches("./"));
                        return resolve_path_traced(ctx, &target_path, kind, tried, trace);
                    }
                    trace.add_step(ResolveTraceStep::new(
                        steps::MATCH_IMPORTS_KEY,
                        false,
                        format!("No matching imports key for: {spec}"),
                    ));
                } else {
                    trace.add_step(ResolveTraceStep::new(
                        steps::READ_IMPORTS_FIELD,
                        false,
                        "Package has no imports field",
                    ));
                }
            }

            // Found package.json but no matching import
            return ResolveResultWithTrace {
                result: ResolveResult::unresolved(
                    ResolveReasonCode::ImportsNotFound,
                    tried.clone(),
                ),
                trace: trace.clone(),
            };
        }

        current = dir.parent();
    }

    trace.failure(
        steps::FIND_PACKAGE_JSON,
        "No package.json found in parent directories",
    );
    ResolveResultWithTrace {
        result: ResolveResult::unresolved(ResolveReasonCode::ImportsNotFound, tried.clone()),
        trace: trace.clone(),
    }
}

/// Resolve a relative specifier with tracing.
fn resolve_relative_traced(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    trace.add_step(ResolveTraceStep::new(
        steps::RESOLVE_RELATIVE,
        true,
        format!("Resolving relative from parent: {}", ctx.parent.display()),
    ));

    let base = ctx.parent.join(spec);
    resolve_path_traced(ctx, &base, kind, tried, trace)
}

/// Resolve an absolute specifier with tracing.
fn resolve_absolute_traced(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    trace.success(steps::RESOLVE_RELATIVE, format!("Absolute path: {spec}"));
    let path = PathBuf::from(spec);
    resolve_path_traced(ctx, &path, kind, tried, trace)
}

/// Resolve a path with extension probing and directory resolution (traced).
fn resolve_path_traced(
    ctx: &ResolveContext<'_>,
    base: &Path,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    // Try exact path first
    if base.is_file() {
        trace.add_step(
            ResolveTraceStep::new(steps::FILE_EXISTS, true, "Exact file exists").with_path(base),
        );
        let canonical = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
        trace.add_step(
            ResolveTraceStep::new(steps::FINAL_PATH, true, "Resolution complete")
                .with_path(&canonical),
        );
        return ResolveResultWithTrace {
            result: ResolveResult::resolved(canonical, tried.clone()),
            trace: trace.clone(),
        };
    }

    // If it's a directory, try directory resolution
    if base.is_dir() {
        trace.add_step(
            ResolveTraceStep::new(steps::RESOLVE_DIRECTORY, true, "Path is a directory")
                .with_path(base),
        );
        return resolve_directory_traced(ctx, base, kind, tried, trace);
    }

    // Try extension probing
    for ext in ctx.config.extensions {
        let with_ext = base.with_extension(ext.trim_start_matches('.'));
        add_tried(tried, &with_ext);

        if with_ext.is_file() {
            trace.add_step(
                ResolveTraceStep::new(
                    steps::FILE_EXISTS,
                    true,
                    format!("Found with extension: {ext}"),
                )
                .with_path(&with_ext),
            );
            let canonical = with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
            trace.add_step(
                ResolveTraceStep::new(steps::FINAL_PATH, true, "Resolution complete")
                    .with_path(&canonical),
            );
            return ResolveResultWithTrace {
                result: ResolveResult::resolved(canonical, tried.clone()),
                trace: trace.clone(),
            };
        }
    }

    // Try as directory
    let dir_result = resolve_directory_traced(ctx, base, kind, tried, trace);
    if dir_result.result.status == ResolveStatus::Resolved {
        return dir_result;
    }

    trace.failure(
        steps::FILE_EXISTS,
        format!("File not found: {}", base.display()),
    );
    ResolveResultWithTrace {
        result: ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone()),
        trace: trace.clone(),
    }
}

/// Resolve a directory with tracing.
fn resolve_directory_traced(
    ctx: &ResolveContext<'_>,
    dir: &Path,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    let pkg_json_path = dir.join("package.json");

    if pkg_json_path.is_file() {
        add_tried(tried, &pkg_json_path);
        trace.add_step(
            ResolveTraceStep::new(steps::FIND_PACKAGE_JSON, true, "Found package.json")
                .with_path(&pkg_json_path),
        );

        if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
            // Try exports field first
            if let Some(exports_target) = resolve_exports_root(&pkg_json, kind) {
                trace.add_step(ResolveTraceStep::new(
                    steps::READ_EXPORTS_FIELD,
                    true,
                    "Package has exports field",
                ));
                trace.add_step(
                    ResolveTraceStep::new(steps::MATCH_EXPORTS_KEY, true, "Matched root exports")
                        .with_key(".")
                        .with_target(&exports_target)
                        .with_condition(kind.to_string()),
                );

                let target_path = dir.join(exports_target.trim_start_matches("./"));
                add_tried(tried, &target_path);

                // Try exact path
                if target_path.is_file() {
                    let canonical = target_path
                        .canonicalize()
                        .unwrap_or_else(|_| target_path.clone());
                    trace.add_step(
                        ResolveTraceStep::new(
                            steps::FINAL_PATH,
                            true,
                            "Resolution complete via exports",
                        )
                        .with_path(&canonical),
                    );
                    return ResolveResultWithTrace {
                        result: ResolveResult::resolved(canonical, tried.clone()),
                        trace: trace.clone(),
                    };
                }

                // Try with extension probing
                for ext in ctx.config.extensions {
                    let with_ext = target_path.with_extension(ext.trim_start_matches('.'));
                    add_tried(tried, &with_ext);

                    if with_ext.is_file() {
                        let canonical =
                            with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                        trace.add_step(
                            ResolveTraceStep::new(
                                steps::FINAL_PATH,
                                true,
                                format!("Resolution complete via exports with extension: {ext}"),
                            )
                            .with_path(&canonical),
                        );
                        return ResolveResultWithTrace {
                            result: ResolveResult::resolved(canonical, tried.clone()),
                            trace: trace.clone(),
                        };
                    }
                }

                // exports target not found
                trace.failure(
                    steps::FILE_EXISTS,
                    format!("Exports target not found: {}", target_path.display()),
                );
                return ResolveResultWithTrace {
                    result: ResolveResult::unresolved(
                        ResolveReasonCode::ExportsTargetNotFound,
                        tried.clone(),
                    ),
                    trace: trace.clone(),
                };
            }

            // Check if exports field exists but we didn't match
            if pkg_json.get("exports").is_some() {
                trace.add_step(ResolveTraceStep::new(
                    steps::READ_EXPORTS_FIELD,
                    true,
                    "Package has exports field",
                ));
                // Add warning - no exports but we'll fall back to main
                trace.add_warning(TraceWarning::new(
                    warning_codes::LEGACY_RESOLUTION,
                    "No matching exports condition, falling back to main field",
                ));
            }

            // Fall back to main field
            if let Some(main) = pkg_json.get("main").and_then(|v| v.as_str()) {
                trace.add_step(ResolveTraceStep::new(
                    steps::RESOLVE_MAIN,
                    true,
                    format!("Using main field: {main}"),
                ));

                let main_path = dir.join(main);

                // Try exact main path
                if main_path.is_file() {
                    let canonical = main_path
                        .canonicalize()
                        .unwrap_or_else(|_| main_path.clone());
                    trace.add_step(
                        ResolveTraceStep::new(
                            steps::FINAL_PATH,
                            true,
                            "Resolution complete via main",
                        )
                        .with_path(&canonical),
                    );
                    return ResolveResultWithTrace {
                        result: ResolveResult::resolved(canonical, tried.clone()),
                        trace: trace.clone(),
                    };
                }

                // Try main with extension probing
                for ext in ctx.config.extensions {
                    let with_ext = main_path.with_extension(ext.trim_start_matches('.'));
                    add_tried(tried, &with_ext);

                    if with_ext.is_file() {
                        let canonical =
                            with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                        trace.add_step(
                            ResolveTraceStep::new(
                                steps::FINAL_PATH,
                                true,
                                format!("Resolution complete via main with extension: {ext}"),
                            )
                            .with_path(&canonical),
                        );
                        return ResolveResultWithTrace {
                            result: ResolveResult::resolved(canonical, tried.clone()),
                            trace: trace.clone(),
                        };
                    }
                }

                // Try main/index.*
                if main_path.is_dir() {
                    for ext in ctx.config.extensions {
                        let index = main_path.join(format!("index{ext}"));
                        add_tried(tried, &index);

                        if index.is_file() {
                            let canonical = index.canonicalize().unwrap_or_else(|_| index.clone());
                            trace.add_step(
                                ResolveTraceStep::new(
                                    steps::FINAL_PATH,
                                    true,
                                    "Resolution complete via main/index",
                                )
                                .with_path(&canonical),
                            );
                            return ResolveResultWithTrace {
                                result: ResolveResult::resolved(canonical, tried.clone()),
                                trace: trace.clone(),
                            };
                        }
                    }
                }

                trace.failure(
                    steps::RESOLVE_MAIN,
                    format!("Main field target not found: {main}"),
                );
            }
        }
    }

    // Try index.* files
    trace.add_step(ResolveTraceStep::new(
        steps::RESOLVE_INDEX,
        true,
        "Trying index files",
    ));

    for ext in ctx.config.extensions {
        let index = dir.join(format!("index{ext}"));
        add_tried(tried, &index);

        if index.is_file() {
            let canonical = index.canonicalize().unwrap_or_else(|_| index.clone());
            trace.add_step(
                ResolveTraceStep::new(
                    steps::FINAL_PATH,
                    true,
                    format!("Resolution complete via index{ext}"),
                )
                .with_path(&canonical),
            );
            return ResolveResultWithTrace {
                result: ResolveResult::resolved(canonical, tried.clone()),
                trace: trace.clone(),
            };
        }
    }

    // Directory exists but no resolvable entry
    if dir.is_dir() {
        trace.failure(steps::RESOLVE_DIRECTORY, "Directory has no entry point");
        return ResolveResultWithTrace {
            result: ResolveResult::unresolved(ResolveReasonCode::IsDirectory, tried.clone()),
            trace: trace.clone(),
        };
    }

    trace.failure(steps::RESOLVE_DIRECTORY, "Not a directory");
    ResolveResultWithTrace {
        result: ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone()),
        trace: trace.clone(),
    }
}

/// Resolve a bare specifier with tracing.
fn resolve_bare_traced(
    ctx: &ResolveContext<'_>,
    spec: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    // Parse package name from specifier
    let (pkg_name, subpath) = parse_bare_specifier(spec);

    trace.add_step(ResolveTraceStep::new(
        steps::RESOLVE_BARE,
        true,
        format!("Bare specifier: package={pkg_name}, subpath={subpath:?}"),
    ));

    let mut found_node_modules = false;
    let mut found_package = false;
    let mut specific_error: Option<ResolveReasonCode> = None;
    let mut current = Some(ctx.parent.as_path());

    while let Some(dir) = current {
        let node_modules = dir.join("node_modules");

        if node_modules.is_dir() {
            found_node_modules = true;
            trace.add_step(ResolveTraceStep::new(
                steps::SEARCH_NODE_MODULES,
                true,
                format!("Found node_modules at: {}", node_modules.display()),
            ));

            let pkg_dir = node_modules.join(pkg_name);
            add_tried(tried, &pkg_dir);

            if pkg_dir.is_dir() {
                found_package = true;
                trace.add_step(
                    ResolveTraceStep::new(
                        steps::FIND_PACKAGE_DIR,
                        true,
                        format!("Found package: {pkg_name}"),
                    )
                    .with_path(&pkg_dir),
                );

                // Package found - handle root vs subpath resolution
                if let Some(sub) = subpath {
                    // Subpath resolution
                    let result =
                        resolve_package_subpath_traced(ctx, &pkg_dir, sub, kind, tried, trace);
                    if result.result.status == ResolveStatus::Resolved {
                        return result;
                    }

                    // Capture specific error codes
                    if let Some(reason) = result.result.reason {
                        match reason {
                            ResolveReasonCode::ExportsTargetNotFound
                            | ResolveReasonCode::ExportsNotFound
                            | ResolveReasonCode::ImportsNotFound
                            | ResolveReasonCode::PackageMainNotFound => {
                                specific_error = Some(reason);
                            }
                            _ => {}
                        }
                    }
                } else {
                    // Root package resolution
                    let result = resolve_path_traced(ctx, &pkg_dir, kind, tried, trace);
                    if result.result.status == ResolveStatus::Resolved {
                        return result;
                    }

                    // Capture specific error codes
                    if let Some(reason) = result.result.reason {
                        match reason {
                            ResolveReasonCode::ExportsTargetNotFound
                            | ResolveReasonCode::ImportsNotFound
                            | ResolveReasonCode::PackageMainNotFound => {
                                specific_error = Some(reason);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Walk up
        current = dir.parent();
    }

    // Return specific error if we found the package but resolution failed
    if let Some(error) = specific_error {
        return ResolveResultWithTrace {
            result: ResolveResult::unresolved(error, tried.clone()),
            trace: trace.clone(),
        };
    }

    if found_node_modules {
        if !found_package {
            trace.failure(
                steps::FIND_PACKAGE_DIR,
                format!("Package not found: {pkg_name}"),
            );
        }
        ResolveResultWithTrace {
            result: ResolveResult::unresolved(ResolveReasonCode::NotFound, tried.clone()),
            trace: trace.clone(),
        }
    } else {
        trace.failure(
            steps::SEARCH_NODE_MODULES,
            "No node_modules directory found",
        );
        ResolveResultWithTrace {
            result: ResolveResult::unresolved(
                ResolveReasonCode::NodeModulesNotFound,
                tried.clone(),
            ),
            trace: trace.clone(),
        }
    }
}

/// Resolve a package subpath with tracing.
fn resolve_package_subpath_traced(
    ctx: &ResolveContext<'_>,
    pkg_dir: &Path,
    subpath: &str,
    kind: ResolutionKind,
    tried: &mut Vec<PathBuf>,
    trace: &mut ResolveTrace,
) -> ResolveResultWithTrace {
    let pkg_json_path = pkg_dir.join("package.json");

    if pkg_json_path.is_file() {
        add_tried(tried, &pkg_json_path);

        if let Some(pkg_json) = read_pkg_json_cached(&pkg_json_path, ctx.pkg_json_cache) {
            // Check if exports field exists
            let has_exports = pkg_json.get("exports").is_some();

            if has_exports {
                trace.add_step(ResolveTraceStep::new(
                    steps::READ_EXPORTS_FIELD,
                    true,
                    "Package has exports field",
                ));

                // Convert subpath to exports format: "feature" -> "./feature"
                let exports_subpath = format!("./{subpath}");

                if let Some(exports_target) =
                    resolve_exports(&pkg_json, Some(&exports_subpath), kind)
                {
                    trace.add_step(
                        ResolveTraceStep::new(
                            steps::MATCH_EXPORTS_KEY,
                            true,
                            format!("Matched exports key: {exports_subpath}"),
                        )
                        .with_key(&exports_subpath)
                        .with_target(&exports_target)
                        .with_condition(kind.to_string()),
                    );

                    let target_path = pkg_dir.join(exports_target.trim_start_matches("./"));
                    add_tried(tried, &target_path);

                    // Try exact path
                    if target_path.is_file() {
                        let canonical = target_path
                            .canonicalize()
                            .unwrap_or_else(|_| target_path.clone());
                        trace.add_step(
                            ResolveTraceStep::new(
                                steps::FINAL_PATH,
                                true,
                                "Resolution complete via exports subpath",
                            )
                            .with_path(&canonical),
                        );
                        return ResolveResultWithTrace {
                            result: ResolveResult::resolved(canonical, tried.clone()),
                            trace: trace.clone(),
                        };
                    }

                    // Try with extension probing
                    for ext in ctx.config.extensions {
                        let with_ext = target_path.with_extension(ext.trim_start_matches('.'));
                        add_tried(tried, &with_ext);

                        if with_ext.is_file() {
                            let canonical =
                                with_ext.canonicalize().unwrap_or_else(|_| with_ext.clone());
                            trace.add_step(
                                ResolveTraceStep::new(steps::FINAL_PATH, true, format!("Resolution complete via exports subpath with extension: {ext}"))
                                    .with_path(&canonical)
                            );
                            return ResolveResultWithTrace {
                                result: ResolveResult::resolved(canonical, tried.clone()),
                                trace: trace.clone(),
                            };
                        }
                    }

                    // exports target specified but file not found
                    trace.failure(
                        steps::FILE_EXISTS,
                        format!("Exports target not found: {}", target_path.display()),
                    );
                    return ResolveResultWithTrace {
                        result: ResolveResult::unresolved(
                            ResolveReasonCode::ExportsTargetNotFound,
                            tried.clone(),
                        ),
                        trace: trace.clone(),
                    };
                }

                // exports exists but no matching key for this subpath
                trace.add_step(ResolveTraceStep::new(
                    steps::MATCH_EXPORTS_KEY,
                    false,
                    format!("No matching exports key for: {exports_subpath}"),
                ));
                return ResolveResultWithTrace {
                    result: ResolveResult::unresolved(
                        ResolveReasonCode::ExportsNotFound,
                        tried.clone(),
                    ),
                    trace: trace.clone(),
                };
            }
            trace.add_warning(TraceWarning::new(
                warning_codes::MISSING_EXPORTS,
                "Package has no exports field, using legacy resolution",
            ));
        }
    }

    // No exports field or no package.json - fall back to direct filesystem resolution
    trace.add_step(ResolveTraceStep::new(
        steps::RESOLVE_RELATIVE,
        true,
        format!("Falling back to filesystem resolution for subpath: {subpath}"),
    ));
    let target_path = pkg_dir.join(subpath);
    resolve_path_traced(ctx, &target_path, kind, tried, trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    /// Normalize path for cross-platform comparison (replace backslashes with forward slashes)
    fn normalize_path_for_test(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn test_relative_file_exists() {
        let dir = tempdir().unwrap();
        let dep = dir.path().join("dep.js");
        fs::write(&dep, "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "./dep.js");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(result.resolved.is_some());
    }

    #[test]
    fn test_relative_extension_probing() {
        let dir = tempdir().unwrap();
        let dep = dir.path().join("dep.ts");
        fs::write(&dep, "export {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "./dep");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("dep.ts"));
    }

    #[test]
    fn test_relative_not_found() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "./nonexistent");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::NotFound));
    }

    #[test]
    fn test_directory_index() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("utils");
        fs::create_dir(&subdir).unwrap();
        let index = subdir.join("index.js");
        fs::write(&index, "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "./utils");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("index.js"));
    }

    #[test]
    fn test_directory_package_json_main() {
        let dir = tempdir().unwrap();
        let pkg_dir = dir.path().join("my-pkg");
        fs::create_dir(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "my-pkg", "main": "lib/index.js"}"#,
        )
        .unwrap();
        let lib = pkg_dir.join("lib");
        fs::create_dir(&lib).unwrap();
        fs::write(lib.join("index.js"), "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "./my-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().contains("lib"));
    }

    #[test]
    fn test_bare_specifier_node_modules() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let lodash = nm.join("lodash");
        fs::create_dir(&lodash).unwrap();
        fs::write(
            lodash.join("package.json"),
            r#"{"name": "lodash", "main": "index.js"}"#,
        )
        .unwrap();
        fs::write(lodash.join("index.js"), "module.exports = _").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "lodash");
        assert_eq!(result.status, ResolveStatus::Resolved);
    }

    #[test]
    fn test_bare_specifier_no_node_modules() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "react");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::NodeModulesNotFound));
    }

    #[test]
    fn test_bare_specifier_subpath() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let lodash = nm.join("lodash");
        fs::create_dir(&lodash).unwrap();
        let fp = lodash.join("fp");
        fs::create_dir(&fp).unwrap();
        fs::write(fp.join("index.js"), "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "lodash/fp");
        assert_eq!(result.status, ResolveStatus::Resolved);
    }

    #[test]
    fn test_scoped_package() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let scope = nm.join("@my-scope");
        fs::create_dir(&scope).unwrap();
        let pkg = scope.join("my-pkg");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("index.js"), "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "@my-scope/my-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
    }

    #[test]
    fn test_empty_specifier() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::SpecifierInvalid));
    }

    #[test]
    fn test_url_specifier() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "https://example.com/mod.js");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::UnsupportedScheme));
    }

    #[test]
    fn test_node_builtin() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "node:fs");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::UnsupportedScheme));
    }

    #[test]
    fn test_parse_bare_specifier_simple() {
        assert_eq!(parse_bare_specifier("lodash"), ("lodash", None));
    }

    #[test]
    fn test_parse_bare_specifier_subpath() {
        assert_eq!(parse_bare_specifier("lodash/fp"), ("lodash", Some("fp")));
    }

    #[test]
    fn test_parse_bare_specifier_scoped() {
        assert_eq!(parse_bare_specifier("@scope/pkg"), ("@scope/pkg", None));
    }

    #[test]
    fn test_parse_bare_specifier_scoped_subpath() {
        assert_eq!(
            parse_bare_specifier("@scope/pkg/sub"),
            ("@scope/pkg", Some("sub"))
        );
    }

    // v1.1 exports tests

    #[test]
    fn test_exports_string_root() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("test-pkg");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("dist")).unwrap();

        // Package with exports as string
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "test-pkg", "exports": "./dist/index.js"}"#,
        )
        .unwrap();
        fs::write(pkg.join("dist/index.js"), "export default 42").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "test-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(normalize_path_for_test(&resolved).contains("dist/index.js"));
    }

    #[test]
    fn test_exports_dot_key() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("dot-pkg");
        fs::create_dir(&pkg).unwrap();

        // Package with exports: { ".": "./main.js" }
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "dot-pkg", "exports": { ".": "./main.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("main.js"), "export default 1").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "dot-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("main.js"));
    }

    #[test]
    fn test_exports_conditions_import() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("cond-pkg");
        fs::create_dir(&pkg).unwrap();

        // Package with conditional exports
        fs::write(
            pkg.join("package.json"),
            r#"{
                "name": "cond-pkg",
                "exports": {
                    ".": {
                        "import": "./esm.js",
                        "require": "./cjs.cjs",
                        "default": "./default.js"
                    }
                }
            }"#,
        )
        .unwrap();
        fs::write(pkg.join("esm.js"), "export default 'esm'").unwrap();
        fs::write(pkg.join("cjs.cjs"), "module.exports = 'cjs'").unwrap();
        fs::write(pkg.join("default.js"), "export default 'default'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Import kind should select esm.js
        let result = resolve_with_kind(&ctx, "cond-pkg", ResolutionKind::Import);
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("esm.js"));

        // Require kind should select cjs.cjs
        let result = resolve_with_kind(&ctx, "cond-pkg", ResolutionKind::Require);
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("cjs.cjs"));

        // Unknown kind should select default.js
        let result = resolve_with_kind(&ctx, "cond-pkg", ResolutionKind::Unknown);
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(resolved.to_string_lossy().ends_with("default.js"));
    }

    #[test]
    fn test_exports_fallback_to_main() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("main-pkg");
        fs::create_dir(&pkg).unwrap();

        // Package without exports, just main
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "main-pkg", "main": "./lib/index.js"}"#,
        )
        .unwrap();
        fs::create_dir(pkg.join("lib")).unwrap();
        fs::write(pkg.join("lib/index.js"), "module.exports = {}").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "main-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(normalize_path_for_test(&resolved).contains("lib/index.js"));
    }

    #[test]
    fn test_exports_target_not_found() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("missing-target");
        fs::create_dir(&pkg).unwrap();

        // Package with exports pointing to non-existent file
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "missing-target", "exports": "./nonexistent.js"}"#,
        )
        .unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "missing-target");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(
            result.reason,
            Some(ResolveReasonCode::ExportsTargetNotFound)
        );
    }

    #[test]
    fn test_exports_takes_precedence_over_main() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("both-pkg");
        fs::create_dir(&pkg).unwrap();

        // Package with both exports and main - exports should win
        fs::write(
            pkg.join("package.json"),
            r#"{
                "name": "both-pkg",
                "main": "./main.js",
                "exports": "./exports.js"
            }"#,
        )
        .unwrap();
        fs::write(pkg.join("main.js"), "// main").unwrap();
        fs::write(pkg.join("exports.js"), "// exports").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "both-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        // exports.js should be chosen, not main.js
        assert!(resolved.to_string_lossy().ends_with("exports.js"));
    }

    #[test]
    fn test_hash_import() {
        let dir = tempdir().unwrap();

        // Create package with imports field
        let pkg_json = serde_json::json!({
            "name": "my-project",
            "imports": {
                "#utils": "./src/utils.js"
            }
        });
        fs::write(
            dir.path().join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/utils.js"), "export const x = 1").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "#utils");
        assert_eq!(result.status, ResolveStatus::Resolved);
        let resolved = result.resolved.unwrap();
        assert!(normalize_path_for_test(&resolved).contains("src/utils.js"));
    }

    #[test]
    fn test_hash_import_not_found() {
        let dir = tempdir().unwrap();

        // Create package with imports field
        let pkg_json = serde_json::json!({
            "name": "my-project",
            "imports": {
                "#foo": "./foo.js"
            }
        });
        fs::write(
            dir.path().join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // #bar is not defined
        let result = resolve_v0(&ctx, "#bar");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::ImportsNotFound));
    }

    #[test]
    fn test_hash_import_with_conditions() {
        let dir = tempdir().unwrap();

        // Create package with conditional imports
        let pkg_json = serde_json::json!({
            "name": "my-project",
            "imports": {
                "#dep": {
                    "import": "./esm-dep.js",
                    "require": "./cjs-dep.cjs"
                }
            }
        });
        fs::write(
            dir.path().join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();
        fs::write(dir.path().join("esm-dep.js"), "export default 1").unwrap();
        fs::write(dir.path().join("cjs-dep.cjs"), "module.exports = 1").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Import kind
        let result = resolve_with_kind(&ctx, "#dep", ResolutionKind::Import);
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(result
            .resolved
            .unwrap()
            .to_string_lossy()
            .ends_with("esm-dep.js"));

        // Require kind
        let result = resolve_with_kind(&ctx, "#dep", ResolutionKind::Require);
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(result
            .resolved
            .unwrap()
            .to_string_lossy()
            .ends_with("cjs-dep.cjs"));
    }

    // v1.2 exports subpath tests

    #[test]
    fn test_exports_subpath_string() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("test-pkg");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("dist")).unwrap();

        // Package with exports subpath
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "test-pkg", "exports": { ".": "./index.js", "./feature": "./dist/feature.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();
        fs::write(pkg.join("dist/feature.js"), "export default 'feature'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Root resolution
        let result = resolve_v0(&ctx, "test-pkg");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(result
            .resolved
            .unwrap()
            .to_string_lossy()
            .ends_with("index.js"));

        // Subpath resolution
        let result = resolve_v0(&ctx, "test-pkg/feature");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/feature.js"));
    }

    #[test]
    fn test_exports_subpath_conditional() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("cond-sub");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("esm")).unwrap();
        fs::create_dir(pkg.join("cjs")).unwrap();

        // Package with conditional exports for subpath
        let pkg_json = serde_json::json!({
            "name": "cond-sub",
            "exports": {
                ".": "./index.js",
                "./utils": {
                    "import": "./esm/utils.js",
                    "require": "./cjs/utils.cjs"
                }
            }
        });
        fs::write(
            pkg.join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();
        fs::write(pkg.join("esm/utils.js"), "export default 'esm'").unwrap();
        fs::write(pkg.join("cjs/utils.cjs"), "module.exports = 'cjs'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Import kind should select esm
        let result = resolve_with_kind(&ctx, "cond-sub/utils", ResolutionKind::Import);
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).ends_with("esm/utils.js"));

        // Require kind should select cjs
        let result = resolve_with_kind(&ctx, "cond-sub/utils", ResolutionKind::Require);
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).ends_with("cjs/utils.cjs"));
    }

    #[test]
    fn test_exports_subpath_not_found() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("limited-exports");
        fs::create_dir(&pkg).unwrap();

        // Package with limited exports
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "limited-exports", "exports": { ".": "./index.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Subpath not in exports should fail
        let result = resolve_v0(&ctx, "limited-exports/utils");
        assert_eq!(result.status, ResolveStatus::Unresolved);
        assert_eq!(result.reason, Some(ResolveReasonCode::ExportsNotFound));
    }

    #[test]
    fn test_exports_pattern_simple() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("pattern-pkg");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("dist")).unwrap();

        // Package with pattern exports
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "pattern-pkg", "exports": { ".": "./index.js", "./*": "./dist/*.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();
        fs::write(pkg.join("dist/foo.js"), "export default 'foo'").unwrap();
        fs::write(pkg.join("dist/bar.js"), "export default 'bar'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Pattern resolution
        let result = resolve_v0(&ctx, "pattern-pkg/foo");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/foo.js"));

        let result = resolve_v0(&ctx, "pattern-pkg/bar");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/bar.js"));
    }

    #[test]
    fn test_exports_pattern_nested() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("nested-pattern");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir_all(pkg.join("dist/features")).unwrap();

        // Package with nested pattern
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "nested-pattern", "exports": { "./features/*": "./dist/features/*.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("dist/features/auth.js"), "export default 'auth'").unwrap();
        fs::write(pkg.join("dist/features/user.js"), "export default 'user'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let result = resolve_v0(&ctx, "nested-pattern/features/auth");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("features/auth.js"));

        let result = resolve_v0(&ctx, "nested-pattern/features/user");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("features/user.js"));
    }

    #[test]
    fn test_exports_exact_beats_pattern() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("exact-pattern");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("dist")).unwrap();
        fs::create_dir(pkg.join("special")).unwrap();

        // Package with both exact and pattern exports
        let pkg_json = serde_json::json!({
            "name": "exact-pattern",
            "exports": {
                "./*": "./dist/*.js",
                "./special": "./special/index.js"
            }
        });
        fs::write(
            pkg.join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();
        fs::write(pkg.join("dist/special.js"), "export default 'pattern'").unwrap();
        fs::write(pkg.join("special/index.js"), "export default 'exact'").unwrap();
        fs::write(pkg.join("dist/other.js"), "export default 'other'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Exact match should take precedence
        let result = resolve_v0(&ctx, "exact-pattern/special");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("special/index.js"));

        // Pattern match for other paths
        let result = resolve_v0(&ctx, "exact-pattern/other");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/other.js"));
    }

    #[test]
    fn test_exports_pattern_specificity() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("specificity");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir_all(pkg.join("dist/features")).unwrap();
        fs::create_dir(pkg.join("features")).unwrap();

        // Package with patterns of different specificity
        let pkg_json = serde_json::json!({
            "name": "specificity",
            "exports": {
                "./*": "./dist/*.js",
                "./features/*": "./features/*.js"
            }
        });
        fs::write(
            pkg.join("package.json"),
            serde_json::to_string(&pkg_json).unwrap(),
        )
        .unwrap();
        fs::write(pkg.join("dist/features/auth.js"), "export default 'dist'").unwrap();
        fs::write(pkg.join("features/auth.js"), "export default 'features'").unwrap();
        fs::write(pkg.join("dist/utils.js"), "export default 'utils'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // "./features/auth" should use "./features/*" (more specific)
        let result = resolve_v0(&ctx, "specificity/features/auth");
        assert_eq!(result.status, ResolveStatus::Resolved);
        // Should resolve to ./features/auth.js, not ./dist/features/auth.js
        let resolved = result.resolved.unwrap();
        assert!(normalize_path_for_test(&resolved).ends_with("features/auth.js"));
        assert!(!normalize_path_for_test(&resolved).contains("dist/features"));

        // "./utils" should use "./*"
        let result = resolve_v0(&ctx, "specificity/utils");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/utils.js"));
    }

    #[test]
    fn test_exports_subpath_fallback_no_exports() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("no-exports");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("lib")).unwrap();

        // Package without exports field
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "no-exports", "main": "./index.js"}"#,
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();
        fs::write(pkg.join("lib/utils.js"), "export default 'utils'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Without exports, direct filesystem resolution should work
        let result = resolve_v0(&ctx, "no-exports/lib/utils");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("lib/utils.js"));
    }

    #[test]
    fn test_scoped_package_subpath() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let scope = nm.join("@myorg");
        fs::create_dir(&scope).unwrap();
        let pkg = scope.join("my-pkg");
        fs::create_dir(&pkg).unwrap();
        fs::create_dir(pkg.join("dist")).unwrap();

        // Scoped package with exports
        fs::write(
            pkg.join("package.json"),
            r#"{"name": "@myorg/my-pkg", "exports": { ".": "./index.js", "./feature": "./dist/feature.js" }}"#,
        )
        .unwrap();
        fs::write(pkg.join("index.js"), "export default 'main'").unwrap();
        fs::write(pkg.join("dist/feature.js"), "export default 'feature'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        // Scoped package subpath resolution
        let result = resolve_v0(&ctx, "@myorg/my-pkg/feature");
        assert_eq!(result.status, ResolveStatus::Resolved);
        assert!(normalize_path_for_test(&result.resolved.unwrap()).contains("dist/feature.js"));
    }

    // v1.5 resolve_with_trace tests

    #[test]
    fn test_resolve_with_trace_bare_specifier() {
        let dir = tempdir().unwrap();
        let nm = dir.path().join("node_modules");
        fs::create_dir(&nm).unwrap();
        let pkg = nm.join("trace-pkg");
        fs::create_dir(&pkg).unwrap();

        fs::write(
            pkg.join("package.json"),
            r#"{"name": "trace-pkg", "exports": { ".": { "import": "./esm.js", "require": "./cjs.js" } }}"#,
        )
        .unwrap();
        fs::write(pkg.join("esm.js"), "export default 'esm'").unwrap();
        fs::write(pkg.join("cjs.js"), "module.exports = 'cjs'").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let traced = resolve_with_trace(&ctx, "trace-pkg", ResolutionKind::Import);
        assert_eq!(traced.result.status, ResolveStatus::Resolved);
        assert!(traced
            .result
            .resolved
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .ends_with("esm.js"));

        // Verify trace has steps
        assert!(!traced.trace.steps.is_empty());

        // Check for expected steps
        let step_names: Vec<_> = traced.trace.steps.iter().map(|s| s.step).collect();
        assert!(step_names.contains(&"parse_specifier"));
        assert!(step_names.contains(&"classify_specifier"));
        assert!(step_names.contains(&"final_path"));
    }

    #[test]
    fn test_resolve_with_trace_unresolved() {
        let dir = tempdir().unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let traced = resolve_with_trace(&ctx, "nonexistent-package", ResolutionKind::Unknown);
        assert_eq!(traced.result.status, ResolveStatus::Unresolved);

        // Verify trace has steps and captures the failure
        assert!(!traced.trace.steps.is_empty());

        // Last step should be a failure
        let last_failed_step = traced.trace.steps.iter().rev().find(|s| !s.ok);
        assert!(last_failed_step.is_some());
    }

    #[test]
    fn test_resolve_with_trace_relative() {
        let dir = tempdir().unwrap();
        let dep = dir.path().join("dep.js");
        fs::write(&dep, "export default 42").unwrap();

        let config = ResolverConfig::default();
        let ctx = ResolveContext {
            cwd: dir.path().to_path_buf(),
            parent: dir.path().to_path_buf(),
            channel: "stable".to_string(),
            config: &config,
            pkg_json_cache: None,
        };

        let traced = resolve_with_trace(&ctx, "./dep.js", ResolutionKind::Import);
        assert_eq!(traced.result.status, ResolveStatus::Resolved);
        assert!(traced.result.resolved.is_some());

        // Verify trace
        let step_names: Vec<_> = traced.trace.steps.iter().map(|s| s.step).collect();
        assert!(step_names.contains(&"resolve_relative"));
    }
}
