#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! Protocol types for fastnode IPC/RPC communication.
//!
//! This crate defines the request/response types used for communication
//! between the CLI and daemon.
//!
//! ## Wire format
//! Messages use length-prefixed JSON:
//! - 4-byte little-endian u32 length prefix
//! - JSON payload bytes

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// Protocol schema version. Bump when changing message format.
pub const PROTO_SCHEMA_VERSION: u32 = 1;

/// `RunPlan` schema version. Bump when changing `RunPlan` format.
pub const RUNPLAN_SCHEMA_VERSION: u32 = 2;

/// Resolver schema version.
pub const RESOLVER_SCHEMA_VERSION: u32 = 1;

/// Package graph schema version.
pub const PKG_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Package explain schema version.
pub const PKG_EXPLAIN_SCHEMA_VERSION: u32 = 1;

/// Package why schema version.
pub const PKG_WHY_SCHEMA_VERSION: u32 = 1;

/// Package doctor schema version.
pub const PKG_DOCTOR_SCHEMA_VERSION: u32 = 1;

/// Package install schema version.
pub const PKG_INSTALL_SCHEMA_VERSION: u32 = 1;

/// Build graph schema version (v2.0).
pub const BUILD_GRAPH_SCHEMA_VERSION: u32 = 1;

/// Build run result schema version (v2.0).
pub const BUILD_RUN_SCHEMA_VERSION: u32 = 1;

/// Error codes for protocol errors.
pub mod codes {
    pub const PROTO_VERSION_MISMATCH: &str = "PROTO_VERSION_MISMATCH";
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";

    // Run-specific error codes
    pub const ENTRY_NOT_FOUND: &str = "ENTRY_NOT_FOUND";
    pub const ENTRY_IS_DIR: &str = "ENTRY_IS_DIR";
    pub const ENTRY_INVALID: &str = "ENTRY_INVALID";
    pub const CWD_INVALID: &str = "CWD_INVALID";

    // Watch-specific error codes
    pub const WATCH_UNSUPPORTED: &str = "WATCH_UNSUPPORTED";
    pub const WATCH_INVALID_ROOT: &str = "WATCH_INVALID_ROOT";
    pub const WATCH_ALREADY_RUNNING: &str = "WATCH_ALREADY_RUNNING";
    pub const WATCH_NOT_RUNNING: &str = "WATCH_NOT_RUNNING";

    // Package-specific error codes
    pub const PKG_SPEC_INVALID: &str = "PKG_SPEC_INVALID";
    pub const PKG_NOT_FOUND: &str = "PKG_NOT_FOUND";
    pub const PKG_VERSION_NOT_FOUND: &str = "PKG_VERSION_NOT_FOUND";
    pub const PKG_REGISTRY_ERROR: &str = "PKG_REGISTRY_ERROR";
    pub const PKG_DOWNLOAD_FAILED: &str = "PKG_DOWNLOAD_FAILED";
    pub const PKG_EXTRACT_FAILED: &str = "PKG_EXTRACT_FAILED";
    pub const PKG_LINK_FAILED: &str = "PKG_LINK_FAILED";
    pub const NODE_MODULES_WRITE_FAILED: &str = "NODE_MODULES_WRITE_FAILED";

    // v1.3: --deps flag error codes
    pub const PKG_ARGS_INVALID: &str = "PKG_ARGS_INVALID";
    pub const PKG_PACKAGE_JSON_NOT_FOUND: &str = "PKG_PACKAGE_JSON_NOT_FOUND";
    pub const PKG_PACKAGE_JSON_INVALID: &str = "PKG_PACKAGE_JSON_INVALID";
    pub const PKG_DEP_RANGE_INVALID: &str = "PKG_DEP_RANGE_INVALID";

    // v1.4: pkg graph error codes
    pub const PKG_GRAPH_NODE_MODULES_NOT_FOUND: &str = "PKG_GRAPH_NODE_MODULES_NOT_FOUND";
    pub const PKG_GRAPH_PACKAGE_JSON_INVALID: &str = "PKG_GRAPH_PACKAGE_JSON_INVALID";
    pub const PKG_GRAPH_PACKAGE_JSON_MISSING: &str = "PKG_GRAPH_PACKAGE_JSON_MISSING";
    pub const PKG_GRAPH_IO_ERROR: &str = "PKG_GRAPH_IO_ERROR";
    pub const PKG_GRAPH_DEPTH_LIMIT_REACHED: &str = "PKG_GRAPH_DEPTH_LIMIT_REACHED";

    // v1.5: pkg explain error codes
    pub const PKG_EXPLAIN_SPECIFIER_INVALID: &str = "PKG_EXPLAIN_SPECIFIER_INVALID";
    pub const PKG_EXPLAIN_KIND_INVALID: &str = "PKG_EXPLAIN_KIND_INVALID";
    pub const PKG_EXPLAIN_CWD_INVALID: &str = "PKG_EXPLAIN_CWD_INVALID";
    pub const PKG_EXPLAIN_PARENT_INVALID: &str = "PKG_EXPLAIN_PARENT_INVALID";

    // v1.6: pkg why error codes
    pub const PKG_WHY_ARGS_INVALID: &str = "PKG_WHY_ARGS_INVALID";
    pub const PKG_WHY_TARGET_NOT_FOUND: &str = "PKG_WHY_TARGET_NOT_FOUND";
    pub const PKG_WHY_TARGET_AMBIGUOUS: &str = "PKG_WHY_TARGET_AMBIGUOUS";
    pub const PKG_WHY_GRAPH_UNAVAILABLE: &str = "PKG_WHY_GRAPH_UNAVAILABLE";
    pub const PKG_WHY_MAX_CHAINS_REACHED: &str = "PKG_WHY_MAX_CHAINS_REACHED";

    // v1.7: pkg doctor error codes
    pub const PKG_DOCTOR_ARGS_INVALID: &str = "PKG_DOCTOR_ARGS_INVALID";
    pub const PKG_DOCTOR_CWD_INVALID: &str = "PKG_DOCTOR_CWD_INVALID";
    pub const PKG_DOCTOR_SEVERITY_INVALID: &str = "PKG_DOCTOR_SEVERITY_INVALID";
    pub const PKG_DOCTOR_FORMAT_INVALID: &str = "PKG_DOCTOR_FORMAT_INVALID";

    // v1.9: pkg install error codes
    pub const PKG_INSTALL_LOCKFILE_NOT_FOUND: &str = "PKG_INSTALL_LOCKFILE_NOT_FOUND";
    pub const PKG_INSTALL_LOCKFILE_INVALID: &str = "PKG_INSTALL_LOCKFILE_INVALID";
    pub const PKG_INSTALL_LOCKFILE_STALE: &str = "PKG_INSTALL_LOCKFILE_STALE";
    pub const PKG_INSTALL_INTEGRITY_MISMATCH: &str = "PKG_INSTALL_INTEGRITY_MISMATCH";
    pub const PKG_INSTALL_PACKAGE_MISSING: &str = "PKG_INSTALL_PACKAGE_MISSING";

    // v2.0: build error codes
    pub const BUILD_CWD_INVALID: &str = "BUILD_CWD_INVALID";
    pub const BUILD_SCRIPT_NOT_FOUND: &str = "BUILD_SCRIPT_NOT_FOUND";
    pub const BUILD_SCRIPT_FAILED: &str = "BUILD_SCRIPT_FAILED";
    pub const BUILD_HASH_IO_ERROR: &str = "BUILD_HASH_IO_ERROR";
    pub const BUILD_WATCH_ERROR: &str = "BUILD_WATCH_ERROR";
    pub const BUILD_GRAPH_INTERNAL_ERROR: &str = "BUILD_GRAPH_INTERNAL_ERROR";
    pub const BUILD_PACKAGE_JSON_INVALID: &str = "BUILD_PACKAGE_JSON_INVALID";
    pub const BUILD_PACKAGE_JSON_NOT_FOUND: &str = "BUILD_PACKAGE_JSON_NOT_FOUND";

    // v2.1: build target error codes
    pub const BUILD_TARGET_INVALID: &str = "BUILD_TARGET_INVALID";
    pub const BUILD_NO_DEFAULT_TARGETS: &str = "BUILD_NO_DEFAULT_TARGETS";

    // v3.0: watch build error codes
    pub const BUILD_WATCH_JSON_UNSUPPORTED: &str = "BUILD_WATCH_JSON_UNSUPPORTED";
    pub const BUILD_WATCH_ALREADY_ACTIVE: &str = "BUILD_WATCH_ALREADY_ACTIVE";
}

/// Resolver reason codes for unresolved imports.
pub mod resolve_codes {
    pub const ENTRY_READ_FAILED: &str = "ENTRY_READ_FAILED";
    pub const SPECIFIER_INVALID: &str = "SPECIFIER_INVALID";
    pub const UNSUPPORTED_SCHEME: &str = "UNSUPPORTED_SCHEME";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const IS_DIRECTORY: &str = "IS_DIRECTORY";
    pub const NODE_MODULES_NOT_FOUND: &str = "NODE_MODULES_NOT_FOUND";
    pub const PACKAGE_JSON_INVALID: &str = "PACKAGE_JSON_INVALID";
    pub const PACKAGE_MAIN_NOT_FOUND: &str = "PACKAGE_MAIN_NOT_FOUND";
}

/// Client hello message sent at connection start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientHello {
    pub proto_schema_version: u32,
    pub client_version: String,
}

impl ClientHello {
    #[must_use]
    pub fn new(client_version: impl Into<String>) -> Self {
        Self {
            proto_schema_version: PROTO_SCHEMA_VERSION,
            client_version: client_version.into(),
        }
    }
}

/// Server hello message sent in response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    pub proto_schema_version: u32,
    pub server_version: String,
}

impl ServerHello {
    #[must_use]
    pub fn new(server_version: impl Into<String>) -> Self {
        Self {
            proto_schema_version: PROTO_SCHEMA_VERSION,
            server_version: server_version.into(),
        }
    }
}

/// A request from client to daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Ping the daemon to check if it's alive.
    Ping {
        /// Nonce for request/response matching.
        nonce: u64,
    },

    /// Request daemon shutdown.
    Shutdown,

    /// Request an execution plan for a script.
    Run {
        /// Entry point path (relative or absolute).
        entry: String,
        /// Arguments to pass to the script.
        args: Vec<String>,
        /// Working directory (optional; daemon uses its own logic if omitted).
        cwd: Option<String>,
    },

    /// Start watching directories for file changes.
    WatchStart {
        /// Root directories to watch.
        roots: Vec<String>,
    },

    /// Stop watching for file changes.
    WatchStop,

    /// Query the current watch status.
    WatchStatus,

    /// Add packages to the project.
    PkgAdd {
        /// Package specs (e.g., "react", "lodash@^4.17.0").
        specs: Vec<String>,
        /// Working directory (project root).
        cwd: String,
        /// Channel for cache directory.
        channel: String,
    },

    /// List cached packages.
    PkgCacheList {
        /// Channel for cache directory.
        channel: String,
    },

    /// Prune unused cached packages.
    PkgCachePrune {
        /// Channel for cache directory.
        channel: String,
    },

    /// Get the package dependency graph.
    PkgGraph {
        /// Working directory (project root).
        cwd: String,
        /// Channel for cache directory.
        channel: String,
        /// Include root devDependencies.
        include_dev_root: bool,
        /// Include optionalDependencies.
        include_optional: bool,
        /// Maximum traversal depth.
        max_depth: u32,
        /// Output format: "tree" or "list".
        format: String,
    },

    /// Explain why a specifier resolves to a file.
    PkgExplain {
        /// The specifier to explain.
        specifier: String,
        /// Working directory.
        cwd: String,
        /// Parent directory (directory of importing file).
        parent: String,
        /// Channel for configuration.
        channel: String,
        /// Resolution kind: "import", "require", or "auto".
        kind: String,
    },

    /// Explain why a package is installed (dependency chain).
    PkgWhy {
        /// The package argument (name, name@version, or path).
        arg: String,
        /// Working directory (project root).
        cwd: String,
        /// Channel for configuration.
        channel: String,
        /// Include root devDependencies in graph.
        include_dev_root: bool,
        /// Include optionalDependencies in graph.
        include_optional: bool,
        /// Maximum graph traversal depth.
        max_depth: u32,
        /// Maximum number of chains to return (1..=50, default 5).
        #[serde(default = "default_max_chains")]
        max_chains: u32,
        /// Include resolver trace for specifier resolution.
        #[serde(default)]
        include_trace: bool,
        /// Resolution kind for trace: "import", "require", or "auto".
        #[serde(default)]
        trace_kind: Option<String>,
        /// Parent directory for trace resolution.
        #[serde(default)]
        trace_parent: Option<String>,
        /// Output format: "tree" or "list".
        #[serde(default = "default_why_format")]
        format: String,
    },

    /// Run package health diagnostics.
    PkgDoctor {
        /// Working directory (project root).
        cwd: String,
        /// Channel for configuration.
        channel: String,
        /// Include root devDependencies in graph.
        include_dev_root: bool,
        /// Include optionalDependencies in graph.
        include_optional: bool,
        /// Maximum graph traversal depth.
        max_depth: u32,
        /// Output format: "summary" or "list".
        #[serde(default = "default_doctor_format")]
        format: String,
        /// Minimum severity to include: "info", "warn", or "error".
        #[serde(default = "default_doctor_severity")]
        min_severity: String,
        /// Maximum number of findings to return (1..=2000, default 200).
        #[serde(default = "default_doctor_max_items")]
        max_items: u32,
    },

    /// Install packages from lockfile (v1.9).
    PkgInstall {
        /// Working directory (project root).
        cwd: String,
        /// Channel for cache directory.
        channel: String,
        /// Frozen mode: fail if lockfile is out of date or missing.
        #[serde(default)]
        frozen: bool,
        /// Include devDependencies.
        #[serde(default = "default_install_include_dev")]
        include_dev: bool,
        /// Include optionalDependencies.
        #[serde(default = "default_install_include_optional")]
        include_optional: bool,
    },

    /// Execute a build (v2.0, targets in v2.1).
    Build {
        /// Working directory (project root with package.json).
        cwd: String,
        /// Force rebuild (bypass cache).
        #[serde(default)]
        force: bool,
        /// Dry run (don't execute, just plan).
        #[serde(default)]
        dry_run: bool,
        /// Maximum parallel jobs.
        #[serde(default = "default_build_max_parallel")]
        max_parallel: u32,
        /// Include profiling information.
        #[serde(default)]
        profile: bool,
        /// Target nodes to build (v2.1). Empty = use defaults.
        #[serde(default)]
        targets: Vec<String>,
    },

    /// Watch for file changes and rebuild (v3.0).
    /// Streams `BuildResult` responses for each rebuild wave.
    WatchBuild {
        /// Working directory (project root with package.json).
        cwd: String,
        /// Target nodes to build. Empty = use defaults.
        #[serde(default)]
        targets: Vec<String>,
        /// Debounce delay in milliseconds (default 100ms).
        #[serde(default = "default_watch_debounce_ms")]
        debounce_ms: u32,
        /// Maximum parallel jobs.
        #[serde(default = "default_build_max_parallel")]
        max_parallel: u32,
    },
}

fn default_max_chains() -> u32 {
    5
}

fn default_why_format() -> String {
    "tree".to_string()
}

fn default_doctor_format() -> String {
    "summary".to_string()
}

fn default_doctor_severity() -> String {
    "info".to_string()
}

fn default_doctor_max_items() -> u32 {
    200
}

fn default_install_include_dev() -> bool {
    true
}

fn default_install_include_optional() -> bool {
    true
}

#[allow(clippy::cast_possible_truncation)]
fn default_build_max_parallel() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
        .clamp(1, 64)
}

fn default_watch_debounce_ms() -> u32 {
    100
}

/// Import specifier found in source code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportSpec {
    /// Specifier exactly as found in source.
    pub raw: String,
    /// Kind of import (one of the `import_kind` constants).
    pub kind: String,
    /// Line number (1-indexed, best-effort).
    pub line: Option<u32>,
}

impl ImportSpec {
    /// Create a new `ImportSpec`.
    #[must_use]
    pub fn new(raw: impl Into<String>, kind: impl Into<String>, line: Option<u32>) -> Self {
        Self {
            raw: raw.into(),
            kind: kind.into(),
            line,
        }
    }
}

/// Resolution result for an import specifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedImport {
    /// Original specifier.
    pub raw: String,
    /// Resolved absolute path (if resolved).
    pub resolved: Option<String>,
    /// Status: "resolved" or "unresolved".
    pub status: String,
    /// Reason code if unresolved.
    pub reason: Option<String>,
    /// Whether this result was served from cache.
    pub from_cache: bool,
    /// Candidate paths tried (optional, may be empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tried: Vec<String>,
}

impl ResolvedImport {
    /// Create a resolved import.
    #[must_use]
    pub fn resolved(raw: impl Into<String>, path: impl Into<String>, from_cache: bool) -> Self {
        Self {
            raw: raw.into(),
            resolved: Some(path.into()),
            status: "resolved".to_string(),
            reason: None,
            from_cache,
            tried: Vec::new(),
        }
    }

    /// Create an unresolved import.
    #[must_use]
    pub fn unresolved(raw: impl Into<String>, reason: impl Into<String>, from_cache: bool) -> Self {
        Self {
            raw: raw.into(),
            resolved: None,
            status: "unresolved".to_string(),
            reason: Some(reason.into()),
            from_cache,
            tried: Vec::new(),
        }
    }

    /// Add tried paths.
    #[must_use]
    pub fn with_tried(mut self, tried: Vec<String>) -> Self {
        self.tried = tried;
        self
    }
}

/// Resolver configuration information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolverInfo {
    /// Schema version for resolver output.
    pub schema_version: u32,
    /// Extension probing order.
    pub extensions: Vec<String>,
    /// Node modules resolution mode.
    pub node_modules_mode: String,
}

impl Default for ResolverInfo {
    fn default() -> Self {
        Self {
            schema_version: RESOLVER_SCHEMA_VERSION,
            extensions: vec![
                ".ts".to_string(),
                ".tsx".to_string(),
                ".js".to_string(),
                ".jsx".to_string(),
                ".mjs".to_string(),
                ".cjs".to_string(),
                ".json".to_string(),
            ],
            node_modules_mode: "best_effort_v0".to_string(),
        }
    }
}

/// Execution plan returned by Run request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    /// Schema version for this structure.
    pub schema_version: u32,
    /// Resolved working directory (absolute path).
    pub resolved_cwd: String,
    /// Original entry point as requested.
    pub requested_entry: String,
    /// Canonicalized absolute path if entry exists.
    pub resolved_entry: Option<String>,
    /// Entry kind: "file", "dir", "missing", or "unknown".
    pub entry_kind: String,
    /// Arguments to pass to the script.
    pub args: Vec<String>,
    /// Channel (e.g., "dev", "stable", "nightly").
    pub channel: String,
    /// Human-helpful hints (stable-ish but OK to evolve).
    pub notes: Vec<String>,
    /// Discovered imports from entry file.
    pub imports: Vec<ImportSpec>,
    /// Resolution results for discovered imports.
    pub resolved_imports: Vec<ResolvedImport>,
    /// Resolver configuration info.
    pub resolver: ResolverInfo,
}

impl RunPlan {
    /// Create a new `RunPlan` with schema version set.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resolved_cwd: String,
        requested_entry: String,
        resolved_entry: Option<String>,
        entry_kind: String,
        args: Vec<String>,
        channel: String,
        notes: Vec<String>,
    ) -> Self {
        Self {
            schema_version: RUNPLAN_SCHEMA_VERSION,
            resolved_cwd,
            requested_entry,
            resolved_entry,
            entry_kind,
            args,
            channel,
            notes,
            imports: Vec::new(),
            resolved_imports: Vec::new(),
            resolver: ResolverInfo::default(),
        }
    }

    /// Set imports and resolved imports.
    #[must_use]
    pub fn with_imports(
        mut self,
        imports: Vec<ImportSpec>,
        resolved_imports: Vec<ResolvedImport>,
    ) -> Self {
        self.imports = imports;
        self.resolved_imports = resolved_imports;
        self
    }

    /// Set resolver info.
    #[must_use]
    pub fn with_resolver(mut self, resolver: ResolverInfo) -> Self {
        self.resolver = resolver;
        self
    }
}

/// Information about an installed package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackage {
    /// Package name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// Path to linked package in `node_modules`.
    pub link_path: String,
    /// Path to cached package.
    pub cache_path: String,
}

/// Information about a cached package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedPackage {
    /// Package name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Path to cached package.
    pub path: String,
}

/// Error information for a package operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgErrorInfo {
    /// Package spec that failed.
    pub spec: String,
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
}

/// Unique identifier for an installed package in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphPackageId {
    /// Package name (e.g., "react" or "@types/node").
    pub name: String,
    /// Package version (e.g., "18.2.0").
    pub version: String,
    /// Absolute path to the package root directory.
    pub path: String,
    /// Optional integrity hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
}

/// A dependency edge in the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphDepEdge {
    /// Dependency name as specified in package.json.
    pub name: String,
    /// Version range from package.json (if present and valid string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub req: Option<String>,
    /// Resolved installed target if found in `node_modules`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<GraphPackageId>,
    /// Dependency kind: "dep", "dev", "optional", or "peer".
    pub kind: String,
}

/// A node in the package graph representing an installed package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphPackageNode {
    /// Package identifier.
    pub id: GraphPackageId,
    /// Dependencies as an adjacency list (sorted by name).
    pub dependencies: Vec<GraphDepEdge>,
}

/// Error information for graph construction issues.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphErrorInfo {
    /// Stable error code.
    pub code: String,
    /// Path where the error occurred.
    pub path: String,
    /// Human-readable error message.
    pub message: String,
}

/// The complete package dependency graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageGraph {
    /// Schema version for this output format.
    pub schema_version: u32,
    /// Absolute path to the project root.
    pub root: String,
    /// All package nodes in the graph (sorted deterministically).
    pub nodes: Vec<GraphPackageNode>,
    /// Packages in `node_modules` not reachable from root deps (sorted).
    pub orphans: Vec<GraphPackageId>,
    /// Errors encountered during graph construction (sorted).
    pub errors: Vec<GraphErrorInfo>,
}

// =============================================================================
// Package Explain types (v1.5)
// =============================================================================

/// A single step in the resolution trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgExplainTraceStep {
    /// Step name (e.g., `parse_specifier`, `resolve_exports`, `file_exists`).
    pub step: String,
    /// Whether this step succeeded.
    pub ok: bool,
    /// Human-readable description of what happened.
    pub detail: String,
    /// File path involved in this step, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Export/import condition used (e.g., "import", "require", "default").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Package.json exports/imports key matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Target value from exports/imports map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Additional notes for this step.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,
}

/// Warning generated during resolution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgExplainWarning {
    /// Warning code (e.g., `deprecated_main`, `missing_exports`).
    pub code: String,
    /// Human-readable warning message.
    pub message: String,
}

/// Result of package explain operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgExplainResult {
    /// Schema version for this output format.
    pub schema_version: u32,
    /// The specifier that was resolved.
    pub specifier: String,
    /// The resolved absolute path, if successful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    /// Resolution status: "resolved" or "unresolved".
    pub status: String,
    /// Error code if unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Error message if unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Resolution kind used: "import", "require", or "unknown".
    pub kind: String,
    /// Parent directory used for resolution.
    pub parent: String,
    /// Resolution trace steps.
    pub trace: Vec<PkgExplainTraceStep>,
    /// Warnings generated during resolution.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<PkgExplainWarning>,
    /// Candidate paths tried during resolution.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tried: Vec<String>,
}

// =============================================================================
// Package Why types (v1.6)
// =============================================================================

/// The target package being explained.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgWhyTarget {
    /// Package name.
    pub name: String,
    /// Package version if resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Absolute package root path if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Original input argument.
    pub input: String,
}

/// A single link in the dependency chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgWhyLink {
    /// Source package name (or "<root>").
    pub from: String,
    /// Target package name.
    pub to: String,
    /// Version range requirement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub req: Option<String>,
    /// Resolved version of the target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
    /// Resolved path of the target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    /// Dependency kind: "dep", "dev", "optional", "peer".
    pub kind: String,
}

/// A complete chain from root to target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgWhyChain {
    /// Links in order from root to target.
    pub links: Vec<PkgWhyLink>,
}

/// Error information for why operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgWhyErrorInfo {
    /// Stable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Related path if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Result of package why operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgWhyResult {
    /// Schema version.
    pub schema_version: u32,
    /// Working directory used.
    pub cwd: String,
    /// The target being explained.
    pub target: PkgWhyTarget,
    /// Whether target was found in `node_modules`.
    pub found_in_node_modules: bool,
    /// Whether target is an orphan (installed but not reachable).
    pub is_orphan: bool,
    /// Dependency chains from root to target.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub chains: Vec<PkgWhyChain>,
    /// Additional notes.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,
    /// Errors encountered.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<PkgWhyErrorInfo>,
    /// Optional resolver trace (if `include_trace` was true).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub trace: Option<PkgExplainResult>,
}

// =============================================================================
// Package Doctor types (v1.7)
// =============================================================================

/// Counts of findings by severity.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorCounts {
    /// Number of info-level findings.
    pub info: u32,
    /// Number of warn-level findings.
    pub warn: u32,
    /// Number of error-level findings.
    pub error: u32,
}

/// Summary of the doctor report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorSummary {
    /// Overall severity (worst of all findings).
    pub severity: String,
    /// Counts by severity.
    pub counts: DoctorCounts,
    /// Number of packages indexed in graph.
    pub packages_indexed: u32,
    /// Number of reachable packages.
    pub reachable_packages: u32,
    /// Number of orphan packages.
    pub orphans: u32,
    /// Number of missing edge targets.
    pub missing_edges: u32,
    /// Number of invalid packages.
    pub invalid_packages: u32,
}

/// A single diagnostic finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorFinding {
    /// Stable error code.
    pub code: String,
    /// Severity level: "info", "warn", or "error".
    pub severity: String,
    /// Human-readable message.
    pub message: String,
    /// Package name (and optionally @version) when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Absolute path when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Small deterministic detail payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Small list of related names/paths for context.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub related: Vec<String>,
}

/// The complete doctor report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgDoctorReport {
    /// Schema version for this output format.
    pub schema_version: u32,
    /// Absolute working directory.
    pub cwd: String,
    /// Summary statistics.
    pub summary: DoctorSummary,
    /// All findings (sorted deterministically).
    pub findings: Vec<DoctorFinding>,
    /// Notes (always present, may be empty array).
    /// **LOCKED (v1.7.1+):** This field is always serialized.
    #[serde(default)]
    pub notes: Vec<String>,
}

// =============================================================================
// Package Install types (v1.9)
// =============================================================================

/// Summary of an install operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallSummary {
    /// Total packages in lockfile.
    pub total_packages: u32,
    /// Packages downloaded from registry.
    pub downloaded: u32,
    /// Packages reused from cache.
    pub cached: u32,
    /// Packages linked into `node_modules`.
    pub linked: u32,
    /// Packages that failed.
    pub failed: u32,
}

/// Information about a package that was installed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPackageInfo {
    /// Package name.
    pub name: String,
    /// Resolved version.
    pub version: String,
    /// Whether this came from cache.
    pub from_cache: bool,
    /// Path in `node_modules`.
    pub link_path: String,
}

/// Error for a specific package during install.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallPackageError {
    /// Package name.
    pub name: String,
    /// Version that was attempted.
    pub version: String,
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
}

/// Result of a package install operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PkgInstallResult {
    /// Schema version.
    pub schema_version: u32,
    /// Working directory used.
    pub cwd: String,
    /// Whether the operation succeeded overall.
    pub ok: bool,
    /// Summary statistics.
    pub summary: InstallSummary,
    /// Successfully installed packages.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub installed: Vec<InstallPackageInfo>,
    /// Packages that failed to install.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<InstallPackageError>,
    /// Notes/warnings.
    #[serde(default)]
    pub notes: Vec<String>,
}

// =============================================================================
// Build types (v2.0)
// =============================================================================

/// Cache status for a build node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BuildCacheStatus {
    /// Cache hit - result reused from previous build.
    Hit,
    /// Cache miss - node was executed.
    Miss,
    /// Cache bypassed (--force).
    Bypass,
    /// Node was skipped due to dependency failure.
    Skipped,
}

/// Reason why a build node was executed or skipped (v2.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildNodeReason {
    /// Node was served from cache.
    CacheHit,
    /// Forced rebuild (--force).
    Forced,
    /// Input files changed.
    InputChanged,
    /// A dependency changed.
    DepChanged,
    /// Dependency failed, node skipped.
    DepFailed,
    /// First build (no cache entry).
    FirstBuild,
    /// Output fingerprint mismatch (v2.2+).
    OutputsChanged,
}

impl BuildNodeReason {
    /// Get a human-readable description.
    #[must_use]
    pub fn to_human_string(&self) -> &'static str {
        match self {
            Self::CacheHit => "cache hit",
            Self::Forced => "forced rebuild (--force)",
            Self::InputChanged => "inputs changed",
            Self::DepChanged => "dependency changed",
            Self::DepFailed => "dependency failed",
            Self::FirstBuild => "first build (cache cold)",
            Self::OutputsChanged => "outputs changed (fingerprint mismatch)",
        }
    }
}

/// Result of executing a single build node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct BuildNodeResult {
    /// Node ID (e.g., "script:build").
    pub id: String,
    /// Whether the node succeeded.
    pub ok: bool,
    /// Cache status.
    pub cache: BuildCacheStatus,
    /// Content hash for this node.
    pub hash: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Reason for the execution status (v2.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<BuildNodeReason>,
    /// Error information if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BuildErrorInfo>,
    /// Whether stdout was truncated.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub stdout_truncated: bool,
    /// Whether stderr was truncated.
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub stderr_truncated: bool,
    /// Additional notes about the execution.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notes: Vec<String>,
    /// Number of files processed (for batch transpile nodes, v3.1.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_count: Option<u32>,
    /// Whether this node was auto-discovered (v3.1.2).
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub auto_discovered: bool,
}

/// Error information for a build failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildErrorInfo {
    /// Stable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Additional detail (e.g., last 20 lines of stderr).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Summary of a build run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildRunSummary {
    /// Total execution time in milliseconds.
    pub total_duration_ms: u64,
    /// Execution time saved by cache hits.
    pub saved_duration_ms: u64,
}

/// Counts of build nodes by status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildRunCounts {
    /// Total number of nodes.
    pub total: u32,
    /// Nodes that succeeded.
    pub succeeded: u32,
    /// Nodes that failed.
    pub failed: u32,
    /// Nodes that were skipped.
    pub skipped: u32,
    /// Nodes served from cache.
    pub cache_hits: u32,
    /// Nodes that executed (cache miss or bypass).
    pub executed: u32,
}

/// Result of executing a build graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildRunResult {
    /// Schema version.
    pub schema_version: u32,
    /// Working directory.
    pub cwd: String,
    /// Whether the build succeeded overall.
    pub ok: bool,
    /// Node execution counts.
    pub counts: BuildRunCounts,
    /// Summary statistics.
    pub summary: BuildRunSummary,
    /// Results for each node (in execution order).
    pub results: Vec<BuildNodeResult>,
    /// Notes (always present, may be empty).
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A response from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Pong response to ping.
    Pong {
        /// Echoed nonce from request.
        nonce: u64,
        /// Server time in milliseconds since Unix epoch (optional).
        server_time_unix_ms: Option<u64>,
    },

    /// Shutdown acknowledged.
    ShutdownAck,

    /// Execution plan response.
    RunPlan {
        /// The execution plan (boxed to reduce enum size).
        plan: Box<RunPlan>,
    },

    /// Operation failed with error.
    Error {
        /// Stable error code.
        code: String,
        /// Human-readable message.
        message: String,
    },

    /// Watch started successfully.
    WatchStarted {
        /// Root directories being watched.
        roots: Vec<String>,
    },

    /// Watch stopped successfully.
    WatchStopped,

    /// Current watch status.
    WatchStatus {
        /// Root directories being watched.
        roots: Vec<String>,
        /// Whether the watcher is running.
        running: bool,
        /// Timestamp of last file event (milliseconds since Unix epoch).
        last_event_unix_ms: Option<u64>,
    },

    /// Result of package add operation.
    PkgAddResult {
        /// Successfully installed packages.
        installed: Vec<InstalledPackage>,
        /// Packages that failed to install.
        errors: Vec<PkgErrorInfo>,
        /// Number of packages reused from cache.
        reused_cache: u32,
    },

    /// Result of cache list operation.
    PkgCacheListResult {
        /// Cached packages.
        packages: Vec<CachedPackage>,
        /// Total size in bytes.
        total_size_bytes: u64,
    },

    /// Result of cache prune operation.
    PkgCachePruneResult {
        /// Number of packages removed.
        removed_count: u32,
        /// Bytes freed.
        freed_bytes: u64,
    },

    /// Result of package graph request.
    PkgGraphResult {
        /// The dependency graph.
        graph: PackageGraph,
    },

    /// Result of package explain request.
    PkgExplainResult {
        /// The explain result.
        result: PkgExplainResult,
    },

    /// Result of package why request.
    PkgWhyResult {
        /// The why result.
        result: PkgWhyResult,
    },

    /// Result of package doctor request.
    PkgDoctorResult {
        /// The doctor report.
        report: PkgDoctorReport,
    },

    /// Result of package install request (v1.9).
    PkgInstallResult {
        /// The install result.
        result: PkgInstallResult,
    },

    /// Result of build request (v2.0).
    BuildResult {
        /// The build result.
        result: BuildRunResult,
    },

    /// Watch build session started (v3.0).
    /// After this, `BuildResult` responses will be streamed for each rebuild wave.
    WatchBuildStarted {
        /// Working directory being watched.
        cwd: String,
        /// Targets being built.
        targets: Vec<String>,
        /// Debounce delay in milliseconds.
        debounce_ms: u32,
    },

    /// Watch build session ended (v3.0).
    /// Sent when watch mode is terminated (client disconnect, error, etc).
    WatchBuildStopped {
        /// Reason for stopping.
        reason: String,
    },
}

impl Response {
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn pong(nonce: u64) -> Self {
        // Truncation is intentional: milliseconds since epoch fits in u64 for millennia
        let server_time_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64);

        Self::Pong {
            nonce,
            server_time_unix_ms,
        }
    }

    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Client request frame (hello + request).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub hello: ClientHello,
    pub request: Request,
}

impl Frame {
    #[must_use]
    pub fn new(client_version: impl Into<String>, request: Request) -> Self {
        Self {
            hello: ClientHello::new(client_version),
            request,
        }
    }
}

/// Server response frame (hello + response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameResponse {
    pub hello: ServerHello,
    pub response: Response,
}

impl FrameResponse {
    #[must_use]
    pub fn new(server_version: impl Into<String>, response: Response) -> Self {
        Self {
            hello: ServerHello::new(server_version),
            response,
        }
    }
}

/// Encode a frame to bytes with length prefix.
///
/// Format: 4-byte little-endian length + JSON bytes
///
/// # Errors
/// Returns an error if serialization fails.
pub fn encode_frame<T: Serialize>(frame: &T) -> io::Result<Vec<u8>> {
    let json =
        serde_json::to_vec(frame).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let len = u32::try_from(json.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;

    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);

    Ok(buf)
}

/// Decode a frame from bytes (without length prefix).
///
/// # Errors
/// Returns an error if deserialization fails.
pub fn decode_frame<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> io::Result<T> {
    serde_json::from_slice(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Write a length-prefixed frame to a writer.
///
/// # Errors
/// Returns an error if encoding or writing fails.
pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, frame: &T) -> io::Result<()> {
    let encoded = encode_frame(frame)?;
    writer.write_all(&encoded)?;
    writer.flush()
}

/// Maximum frame size for sanity checking (16 MiB).
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Read a length-prefixed frame from a reader.
///
/// # Errors
/// Returns an error if reading or decoding fails.
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> io::Result<T> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;

    // Sanity check
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }

    // Read JSON payload
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    decode_frame(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proto_schema_version_is_stable() {
        assert_eq!(PROTO_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_client_hello_serialization() {
        let hello = ClientHello::new("0.1.0");
        let json = serde_json::to_string(&hello).unwrap();
        assert!(json.contains("proto_schema_version"));
        assert!(json.contains("0.1.0"));
    }

    #[test]
    fn test_request_ping_serialization() {
        let req = Request::Ping { nonce: 12345 };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ping"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_response_pong_serialization() {
        let resp = Response::pong(12345);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pong"));
        assert!(json.contains("12345"));
    }

    #[test]
    fn test_frame_roundtrip() {
        let frame = Frame::new("0.1.0", Request::Ping { nonce: 42 });

        let encoded = encode_frame(&frame).unwrap();

        // Decode (skip length prefix)
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        assert_eq!(decoded.hello.proto_schema_version, PROTO_SCHEMA_VERSION);
        assert_eq!(decoded.hello.client_version, "0.1.0");

        match decoded.request {
            Request::Ping { nonce } => assert_eq!(nonce, 42),
            _ => panic!("Expected Ping"),
        }
    }

    #[test]
    fn test_frame_response_roundtrip() {
        let frame = FrameResponse::new("0.1.0", Response::pong(42));

        let encoded = encode_frame(&frame).unwrap();
        let decoded: FrameResponse = decode_frame(&encoded[4..]).unwrap();

        assert_eq!(decoded.hello.proto_schema_version, PROTO_SCHEMA_VERSION);

        match decoded.response {
            Response::Pong { nonce, .. } => assert_eq!(nonce, 42),
            _ => panic!("Expected Pong"),
        }
    }

    #[test]
    fn test_write_read_frame() {
        let frame = Frame::new("0.1.0", Request::Shutdown);

        let mut buf = Vec::new();
        write_frame(&mut buf, &frame).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let decoded: Frame = read_frame(&mut cursor).unwrap();

        matches!(decoded.request, Request::Shutdown);
    }

    #[test]
    fn test_error_codes_are_uppercase() {
        let error_codes = [
            codes::PROTO_VERSION_MISMATCH,
            codes::INVALID_REQUEST,
            codes::INTERNAL_ERROR,
            codes::ENTRY_NOT_FOUND,
            codes::ENTRY_IS_DIR,
            codes::ENTRY_INVALID,
            codes::CWD_INVALID,
        ];

        for code in error_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Error code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_runplan_schema_version_is_stable() {
        assert_eq!(RUNPLAN_SCHEMA_VERSION, 2);
    }

    #[test]
    fn test_resolver_schema_version_is_stable() {
        assert_eq!(RESOLVER_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_resolve_codes_are_uppercase() {
        let codes = [
            resolve_codes::ENTRY_READ_FAILED,
            resolve_codes::SPECIFIER_INVALID,
            resolve_codes::UNSUPPORTED_SCHEME,
            resolve_codes::NOT_FOUND,
            resolve_codes::IS_DIRECTORY,
            resolve_codes::NODE_MODULES_NOT_FOUND,
            resolve_codes::PACKAGE_JSON_INVALID,
            resolve_codes::PACKAGE_MAIN_NOT_FOUND,
        ];

        for code in codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Resolve code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_import_spec_serialization() {
        let spec = ImportSpec::new("./dep", "esm_import", Some(5));
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("./dep"));
        assert!(json.contains("esm_import"));
        assert!(json.contains('5'));
    }

    #[test]
    fn test_resolved_import_resolved() {
        let resolved = ResolvedImport::resolved("./dep", "/abs/path/dep.js", false);
        assert_eq!(resolved.status, "resolved");
        assert_eq!(resolved.resolved, Some("/abs/path/dep.js".to_string()));
        assert!(resolved.reason.is_none());
        assert!(!resolved.from_cache);
    }

    #[test]
    fn test_resolved_import_unresolved() {
        let unresolved =
            ResolvedImport::unresolved("react", resolve_codes::NODE_MODULES_NOT_FOUND, true);
        assert_eq!(unresolved.status, "unresolved");
        assert!(unresolved.resolved.is_none());
        assert_eq!(
            unresolved.reason,
            Some("NODE_MODULES_NOT_FOUND".to_string())
        );
        assert!(unresolved.from_cache);
    }

    #[test]
    fn test_resolver_info_default() {
        let info = ResolverInfo::default();
        assert_eq!(info.schema_version, RESOLVER_SCHEMA_VERSION);
        assert_eq!(info.node_modules_mode, "best_effort_v0");
        assert!(info.extensions.contains(&".ts".to_string()));
        assert!(info.extensions.contains(&".js".to_string()));
    }

    #[test]
    fn test_request_run_serialization() {
        let req = Request::Run {
            entry: "main.js".to_string(),
            args: vec!["--flag".to_string(), "value".to_string()],
            cwd: Some("/home/user/project".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("run"));
        assert!(json.contains("main.js"));
        assert!(json.contains("--flag"));
    }

    #[test]
    fn test_request_run_roundtrip() {
        let frame = Frame::new(
            "0.1.0",
            Request::Run {
                entry: "src/index.ts".to_string(),
                args: vec!["arg1".to_string()],
                cwd: Some("/tmp".to_string()),
            },
        );

        let encoded = encode_frame(&frame).unwrap();
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        match decoded.request {
            Request::Run { entry, args, cwd } => {
                assert_eq!(entry, "src/index.ts");
                assert_eq!(args, vec!["arg1"]);
                assert_eq!(cwd, Some("/tmp".to_string()));
            }
            _ => panic!("Expected Run"),
        }
    }

    #[test]
    fn test_runplan_new_sets_schema_version() {
        let plan = RunPlan::new(
            "/home/user".to_string(),
            "main.js".to_string(),
            Some("/home/user/main.js".to_string()),
            "file".to_string(),
            vec![],
            "stable".to_string(),
            vec!["JavaScript entry".to_string()],
        );
        assert_eq!(plan.schema_version, RUNPLAN_SCHEMA_VERSION);
        assert_eq!(plan.schema_version, 2);
        // New fields should be present but empty
        assert!(plan.imports.is_empty());
        assert!(plan.resolved_imports.is_empty());
        assert_eq!(plan.resolver.schema_version, 1);
    }

    #[test]
    fn test_response_runplan_roundtrip() {
        let plan = RunPlan::new(
            "/home/user".to_string(),
            "main.js".to_string(),
            Some("/home/user/main.js".to_string()),
            "file".to_string(),
            vec!["--port".to_string(), "3000".to_string()],
            "dev".to_string(),
            vec!["JS entry".to_string()],
        );
        let response = Response::RunPlan {
            plan: Box::new(plan),
        };
        let frame = FrameResponse::new("0.1.0", response);

        let encoded = encode_frame(&frame).unwrap();
        let decoded: FrameResponse = decode_frame(&encoded[4..]).unwrap();

        match decoded.response {
            Response::RunPlan { plan } => {
                assert_eq!(plan.schema_version, 2);
                assert_eq!(plan.resolved_cwd, "/home/user");
                assert_eq!(plan.requested_entry, "main.js");
                assert_eq!(plan.resolved_entry, Some("/home/user/main.js".to_string()));
                assert_eq!(plan.entry_kind, "file");
                assert_eq!(plan.args, vec!["--port", "3000"]);
                assert_eq!(plan.channel, "dev");
                // New fields present
                assert!(plan.imports.is_empty());
                assert!(plan.resolved_imports.is_empty());
            }
            _ => panic!("Expected RunPlan"),
        }
    }

    #[test]
    fn test_runplan_with_imports_roundtrip() {
        let imports = vec![
            ImportSpec::new("./dep", "esm_import", Some(1)),
            ImportSpec::new("react", "esm_import", Some(2)),
        ];
        let resolved = vec![
            ResolvedImport::resolved("./dep", "/home/user/dep.js", false),
            ResolvedImport::unresolved("react", resolve_codes::NODE_MODULES_NOT_FOUND, false),
        ];

        let plan = RunPlan::new(
            "/home/user".to_string(),
            "main.js".to_string(),
            Some("/home/user/main.js".to_string()),
            "file".to_string(),
            vec![],
            "stable".to_string(),
            vec![],
        )
        .with_imports(imports, resolved);

        let json = serde_json::to_string(&plan).unwrap();
        let decoded: RunPlan = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.imports.len(), 2);
        assert_eq!(decoded.imports[0].raw, "./dep");
        assert_eq!(decoded.resolved_imports.len(), 2);
        assert_eq!(decoded.resolved_imports[0].status, "resolved");
        assert_eq!(decoded.resolved_imports[1].status, "unresolved");
    }

    #[test]
    fn test_watch_start_serialization() {
        let req = Request::WatchStart {
            roots: vec!["/home/user/project".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("watch_start"));
        assert!(json.contains("/home/user/project"));
    }

    #[test]
    fn test_watch_stop_serialization() {
        let req = Request::WatchStop;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("watch_stop"));
    }

    #[test]
    fn test_watch_status_request_serialization() {
        let req = Request::WatchStatus;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("watch_status"));
    }

    #[test]
    fn test_watch_started_response() {
        let resp = Response::WatchStarted {
            roots: vec!["/home/user/project".to_string()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("watch_started"));
        assert!(json.contains("/home/user/project"));
    }

    #[test]
    fn test_watch_stopped_response() {
        let resp = Response::WatchStopped;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("watch_stopped"));
    }

    #[test]
    fn test_watch_status_response() {
        let resp = Response::WatchStatus {
            roots: vec!["/home/user/project".to_string()],
            running: true,
            last_event_unix_ms: Some(1_234_567_890),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("watch_status"));
        assert!(json.contains("running"));
        assert!(json.contains("1234567890"));
    }

    #[test]
    fn test_watch_codes_are_uppercase() {
        let watch_codes = [
            codes::WATCH_UNSUPPORTED,
            codes::WATCH_INVALID_ROOT,
            codes::WATCH_ALREADY_RUNNING,
            codes::WATCH_NOT_RUNNING,
        ];

        for code in watch_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Watch code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_pkg_codes_are_uppercase() {
        let pkg_codes = [
            codes::PKG_SPEC_INVALID,
            codes::PKG_NOT_FOUND,
            codes::PKG_VERSION_NOT_FOUND,
            codes::PKG_REGISTRY_ERROR,
            codes::PKG_DOWNLOAD_FAILED,
            codes::PKG_EXTRACT_FAILED,
            codes::PKG_LINK_FAILED,
            codes::NODE_MODULES_WRITE_FAILED,
            // v1.3 codes
            codes::PKG_ARGS_INVALID,
            codes::PKG_PACKAGE_JSON_NOT_FOUND,
            codes::PKG_PACKAGE_JSON_INVALID,
            codes::PKG_DEP_RANGE_INVALID,
        ];

        for code in pkg_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Pkg code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_pkg_add_request_serialization() {
        let req = Request::PkgAdd {
            specs: vec!["react".to_string(), "lodash@^4.17.0".to_string()],
            cwd: "/home/user/project".to_string(),
            channel: "stable".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_add"));
        assert!(json.contains("react"));
        assert!(json.contains("lodash@^4.17.0"));
    }

    #[test]
    fn test_pkg_cache_list_request_serialization() {
        let req = Request::PkgCacheList {
            channel: "stable".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_cache_list"));
        assert!(json.contains("stable"));
    }

    #[test]
    fn test_pkg_cache_prune_request_serialization() {
        let req = Request::PkgCachePrune {
            channel: "stable".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_cache_prune"));
    }

    #[test]
    fn test_installed_package_serialization() {
        let pkg = InstalledPackage {
            name: "react".to_string(),
            version: "18.2.0".to_string(),
            link_path: "/home/user/project/node_modules/react".to_string(),
            cache_path: "/home/user/.cache/fastnode/v1/stable/packages/npm/react/18.2.0/package"
                .to_string(),
        };
        let json = serde_json::to_string(&pkg).unwrap();
        assert!(json.contains("react"));
        assert!(json.contains("18.2.0"));
    }

    #[test]
    fn test_cached_package_serialization() {
        let pkg = CachedPackage {
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            size_bytes: 1024 * 100,
            path: "/home/user/.cache/fastnode/v1/stable/packages/npm/lodash/4.17.21/package"
                .to_string(),
        };
        let json = serde_json::to_string(&pkg).unwrap();
        assert!(json.contains("lodash"));
        assert!(json.contains("4.17.21"));
        assert!(json.contains("102400"));
    }

    #[test]
    fn test_pkg_error_info_serialization() {
        let err = PkgErrorInfo {
            spec: "nonexistent-package".to_string(),
            code: codes::PKG_NOT_FOUND.to_string(),
            message: "Package not found in registry".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("nonexistent-package"));
        assert!(json.contains("PKG_NOT_FOUND"));
    }

    #[test]
    fn test_pkg_add_result_response() {
        let resp = Response::PkgAddResult {
            installed: vec![InstalledPackage {
                name: "react".to_string(),
                version: "18.2.0".to_string(),
                link_path: "/project/node_modules/react".to_string(),
                cache_path: "/cache/react/18.2.0/package".to_string(),
            }],
            errors: vec![],
            reused_cache: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pkg_add_result"));
        assert!(json.contains("react"));
        assert!(json.contains("reused_cache"));
    }

    #[test]
    fn test_pkg_cache_list_result_response() {
        let resp = Response::PkgCacheListResult {
            packages: vec![CachedPackage {
                name: "lodash".to_string(),
                version: "4.17.21".to_string(),
                size_bytes: 102_400,
                path: "/cache/lodash/4.17.21/package".to_string(),
            }],
            total_size_bytes: 102_400,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pkg_cache_list_result"));
        assert!(json.contains("lodash"));
        assert!(json.contains("total_size_bytes"));
    }

    #[test]
    fn test_pkg_cache_prune_result_response() {
        let resp = Response::PkgCachePruneResult {
            removed_count: 5,
            freed_bytes: 1024 * 1024 * 10,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pkg_cache_prune_result"));
        assert!(json.contains("removed_count"));
        assert!(json.contains("freed_bytes"));
    }

    #[test]
    fn test_pkg_add_request_roundtrip() {
        let frame = Frame::new(
            "0.1.0",
            Request::PkgAdd {
                specs: vec!["react@^18.0.0".to_string()],
                cwd: "/tmp/project".to_string(),
                channel: "dev".to_string(),
            },
        );

        let encoded = encode_frame(&frame).unwrap();
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        match decoded.request {
            Request::PkgAdd {
                specs,
                cwd,
                channel,
            } => {
                assert_eq!(specs, vec!["react@^18.0.0"]);
                assert_eq!(cwd, "/tmp/project");
                assert_eq!(channel, "dev");
            }
            _ => panic!("Expected PkgAdd"),
        }
    }

    #[test]
    fn test_pkg_graph_schema_version_is_stable() {
        assert_eq!(PKG_GRAPH_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_pkg_graph_codes_are_uppercase() {
        let graph_codes = [
            codes::PKG_GRAPH_NODE_MODULES_NOT_FOUND,
            codes::PKG_GRAPH_PACKAGE_JSON_INVALID,
            codes::PKG_GRAPH_PACKAGE_JSON_MISSING,
            codes::PKG_GRAPH_IO_ERROR,
            codes::PKG_GRAPH_DEPTH_LIMIT_REACHED,
        ];

        for code in graph_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Graph code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_pkg_graph_request_serialization() {
        let req = Request::PkgGraph {
            cwd: "/home/user/project".to_string(),
            channel: "stable".to_string(),
            include_dev_root: false,
            include_optional: true,
            max_depth: 25,
            format: "tree".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_graph"));
        assert!(json.contains("/home/user/project"));
        assert!(json.contains("include_optional"));
    }

    #[test]
    fn test_graph_package_id_serialization() {
        let id = GraphPackageId {
            name: "react".to_string(),
            version: "18.2.0".to_string(),
            path: "/home/user/project/node_modules/react".to_string(),
            integrity: None,
        };
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.contains("react"));
        assert!(json.contains("18.2.0"));
        assert!(!json.contains("integrity")); // Should be skipped if None
    }

    #[test]
    fn test_graph_dep_edge_serialization() {
        let edge = GraphDepEdge {
            name: "lodash".to_string(),
            req: Some("^4.17.0".to_string()),
            to: Some(GraphPackageId {
                name: "lodash".to_string(),
                version: "4.17.21".to_string(),
                path: "/node_modules/lodash".to_string(),
                integrity: None,
            }),
            kind: "dep".to_string(),
        };
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("lodash"));
        assert!(json.contains("^4.17.0"));
        assert!(json.contains("dep"));
    }

    #[test]
    fn test_package_graph_serialization() {
        let graph = PackageGraph {
            schema_version: PKG_GRAPH_SCHEMA_VERSION,
            root: "/home/user/project".to_string(),
            nodes: vec![GraphPackageNode {
                id: GraphPackageId {
                    name: "a".to_string(),
                    version: "1.0.0".to_string(),
                    path: "/node_modules/a".to_string(),
                    integrity: None,
                },
                dependencies: vec![],
            }],
            orphans: vec![],
            errors: vec![],
        };
        let json = serde_json::to_string(&graph).unwrap();
        assert!(json.contains("schema_version"));
        assert!(json.contains("/home/user/project"));
        assert!(json.contains("nodes"));
    }

    #[test]
    fn test_pkg_graph_result_response() {
        let resp = Response::PkgGraphResult {
            graph: PackageGraph {
                schema_version: PKG_GRAPH_SCHEMA_VERSION,
                root: "/project".to_string(),
                nodes: vec![],
                orphans: vec![],
                errors: vec![],
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pkg_graph_result"));
        assert!(json.contains("schema_version"));
    }

    #[test]
    fn test_pkg_graph_request_roundtrip() {
        let frame = Frame::new(
            "0.1.0",
            Request::PkgGraph {
                cwd: "/tmp/project".to_string(),
                channel: "stable".to_string(),
                include_dev_root: true,
                include_optional: false,
                max_depth: 10,
                format: "list".to_string(),
            },
        );

        let encoded = encode_frame(&frame).unwrap();
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        match decoded.request {
            Request::PkgGraph {
                cwd,
                channel,
                include_dev_root,
                include_optional,
                max_depth,
                format,
            } => {
                assert_eq!(cwd, "/tmp/project");
                assert_eq!(channel, "stable");
                assert!(include_dev_root);
                assert!(!include_optional);
                assert_eq!(max_depth, 10);
                assert_eq!(format, "list");
            }
            _ => panic!("Expected PkgGraph"),
        }
    }

    #[test]
    fn test_pkg_doctor_schema_version_is_stable() {
        assert_eq!(PKG_DOCTOR_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_pkg_doctor_codes_are_uppercase() {
        let doctor_codes = [
            codes::PKG_DOCTOR_ARGS_INVALID,
            codes::PKG_DOCTOR_CWD_INVALID,
            codes::PKG_DOCTOR_SEVERITY_INVALID,
            codes::PKG_DOCTOR_FORMAT_INVALID,
        ];

        for code in doctor_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Doctor code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_pkg_doctor_request_serialization() {
        let req = Request::PkgDoctor {
            cwd: "/home/user/project".to_string(),
            channel: "stable".to_string(),
            include_dev_root: false,
            include_optional: true,
            max_depth: 25,
            format: "summary".to_string(),
            min_severity: "info".to_string(),
            max_items: 200,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_doctor"));
        assert!(json.contains("/home/user/project"));
        assert!(json.contains("min_severity"));
    }

    #[test]
    fn test_doctor_report_serialization() {
        let report = PkgDoctorReport {
            schema_version: PKG_DOCTOR_SCHEMA_VERSION,
            cwd: "/home/user/project".to_string(),
            summary: DoctorSummary {
                severity: "warn".to_string(),
                counts: DoctorCounts {
                    info: 1,
                    warn: 2,
                    error: 0,
                },
                packages_indexed: 10,
                reachable_packages: 8,
                orphans: 2,
                missing_edges: 1,
                invalid_packages: 0,
            },
            findings: vec![DoctorFinding {
                code: "PKG_DOCTOR_ORPHAN_PACKAGE".to_string(),
                severity: "warn".to_string(),
                message: "installed but not reachable".to_string(),
                package: Some("orphan@1.0.0".to_string()),
                path: Some("/node_modules/orphan".to_string()),
                detail: None,
                related: vec![],
            }],
            notes: vec![],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("schema_version"));
        assert!(json.contains("PKG_DOCTOR_ORPHAN_PACKAGE"));
        assert!(json.contains("packages_indexed"));
    }

    #[test]
    fn test_pkg_doctor_request_roundtrip() {
        let frame = Frame::new(
            "0.1.0",
            Request::PkgDoctor {
                cwd: "/tmp/project".to_string(),
                channel: "stable".to_string(),
                include_dev_root: true,
                include_optional: false,
                max_depth: 10,
                format: "list".to_string(),
                min_severity: "warn".to_string(),
                max_items: 100,
            },
        );

        let encoded = encode_frame(&frame).unwrap();
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        match decoded.request {
            Request::PkgDoctor {
                cwd,
                channel,
                include_dev_root,
                include_optional,
                max_depth,
                format,
                min_severity,
                max_items,
            } => {
                assert_eq!(cwd, "/tmp/project");
                assert_eq!(channel, "stable");
                assert!(include_dev_root);
                assert!(!include_optional);
                assert_eq!(max_depth, 10);
                assert_eq!(format, "list");
                assert_eq!(min_severity, "warn");
                assert_eq!(max_items, 100);
            }
            _ => panic!("Expected PkgDoctor"),
        }
    }

    // v1.9: PkgInstall tests

    #[test]
    fn test_pkg_install_schema_version_is_stable() {
        assert_eq!(PKG_INSTALL_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_pkg_install_codes_are_uppercase() {
        let install_codes = [
            codes::PKG_INSTALL_LOCKFILE_NOT_FOUND,
            codes::PKG_INSTALL_LOCKFILE_INVALID,
            codes::PKG_INSTALL_LOCKFILE_STALE,
            codes::PKG_INSTALL_INTEGRITY_MISMATCH,
            codes::PKG_INSTALL_PACKAGE_MISSING,
        ];

        for code in install_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Install code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_pkg_install_request_serialization() {
        let req = Request::PkgInstall {
            cwd: "/home/user/project".to_string(),
            channel: "stable".to_string(),
            frozen: true,
            include_dev: true,
            include_optional: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("pkg_install"));
        assert!(json.contains("/home/user/project"));
        assert!(json.contains("frozen"));
    }

    #[test]
    fn test_pkg_install_request_roundtrip() {
        let frame = Frame::new(
            "0.1.0",
            Request::PkgInstall {
                cwd: "/tmp/project".to_string(),
                channel: "stable".to_string(),
                frozen: true,
                include_dev: false,
                include_optional: true,
            },
        );

        let encoded = encode_frame(&frame).unwrap();
        let decoded: Frame = decode_frame(&encoded[4..]).unwrap();

        match decoded.request {
            Request::PkgInstall {
                cwd,
                channel,
                frozen,
                include_dev,
                include_optional,
            } => {
                assert_eq!(cwd, "/tmp/project");
                assert_eq!(channel, "stable");
                assert!(frozen);
                assert!(!include_dev);
                assert!(include_optional);
            }
            _ => panic!("Expected PkgInstall"),
        }
    }

    #[test]
    fn test_install_summary_serialization() {
        let summary = InstallSummary {
            total_packages: 100,
            downloaded: 50,
            cached: 45,
            linked: 95,
            failed: 5,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("total_packages"));
        assert!(json.contains("100"));
        assert!(json.contains("downloaded"));
    }

    #[test]
    fn test_install_package_info_serialization() {
        let info = InstallPackageInfo {
            name: "lodash".to_string(),
            version: "4.17.21".to_string(),
            from_cache: true,
            link_path: "/project/node_modules/lodash".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("lodash"));
        assert!(json.contains("4.17.21"));
        assert!(json.contains("from_cache"));
    }

    #[test]
    fn test_install_package_error_serialization() {
        let err = InstallPackageError {
            name: "bad-package".to_string(),
            version: "1.0.0".to_string(),
            code: codes::PKG_INSTALL_INTEGRITY_MISMATCH.to_string(),
            message: "Integrity check failed".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("bad-package"));
        assert!(json.contains("PKG_INSTALL_INTEGRITY_MISMATCH"));
    }

    #[test]
    fn test_pkg_install_result_serialization() {
        let result = PkgInstallResult {
            schema_version: PKG_INSTALL_SCHEMA_VERSION,
            cwd: "/home/user/project".to_string(),
            ok: true,
            summary: InstallSummary {
                total_packages: 10,
                downloaded: 2,
                cached: 8,
                linked: 10,
                failed: 0,
            },
            installed: vec![InstallPackageInfo {
                name: "react".to_string(),
                version: "18.2.0".to_string(),
                from_cache: true,
                link_path: "/project/node_modules/react".to_string(),
            }],
            errors: vec![],
            notes: vec!["All packages installed successfully".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("schema_version"));
        assert!(json.contains("react"));
        assert!(json.contains("All packages installed"));
    }

    #[test]
    fn test_pkg_install_result_response() {
        let resp = Response::PkgInstallResult {
            result: PkgInstallResult {
                schema_version: PKG_INSTALL_SCHEMA_VERSION,
                cwd: "/project".to_string(),
                ok: true,
                summary: InstallSummary::default(),
                installed: vec![],
                errors: vec![],
                notes: vec![],
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("pkg_install_result"));
        assert!(json.contains("schema_version"));
    }
}
