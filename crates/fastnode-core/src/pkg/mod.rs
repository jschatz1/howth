//! Package manager functionality.
//!
//! Provides utilities for:
//! - Parsing package specifications (name@version)
//! - Fetching package metadata from npm registry
//! - Resolving version ranges using semver
//! - Downloading and extracting tarballs
//! - Managing the global package cache
//! - Creating symlinks/junctions in `node_modules`
//! - Reading dependencies from package.json (v1.3)
//! - Building dependency graphs from `node_modules` (v1.4)
//! - Explaining why packages are installed (v1.6)
//! - Health diagnostics for installed packages (v1.7)
//! - Deterministic lockfile generation and installation (v1.9)

pub mod cache;
pub mod deps;
pub mod doctor;
pub mod error;
pub mod explain;
pub mod graph;
pub mod link;
pub mod lockfile;
pub mod registry;
pub mod spec;
pub mod tarball;
pub mod version;

pub use cache::PackageCache;
pub use deps::{read_package_deps, PackageDeps, PkgDepError};
pub use doctor::{
    build_doctor_report, codes as doctor_codes, DoctorCounts, DoctorFinding, DoctorOptions,
    DoctorSeverity, DoctorSummary, PkgDoctorReport, PKG_DOCTOR_SCHEMA_VERSION,
};
pub use error::{codes as pkg_codes, PkgError};
pub use explain::{
    parse_why_arg, why_codes, why_from_graph, ParsedWhyArg, PkgWhyResult, WhyArgKind, WhyChain,
    WhyErrorInfo, WhyLink, WhyOptions, WhyTarget, PKG_WHY_SCHEMA_VERSION,
};
pub use graph::{
    build_pkg_graph, codes as graph_codes, DepEdge, GraphErrorInfo, GraphOptions, PackageGraph,
    PackageId, PackageNode, PKG_GRAPH_SCHEMA_VERSION,
};
pub use link::link_into_node_modules;
pub use lockfile::{
    codes as lockfile_codes, LockDep, LockDepEdge, LockMeta, LockPackage, LockResolution, LockRoot,
    Lockfile, LockfileError, LOCKFILE_NAME, PKG_LOCK_SCHEMA_VERSION,
};
pub use registry::{get_tarball_url, RegistryClient, DEFAULT_REGISTRY, REGISTRY_ENV};
pub use spec::PackageSpec;
pub use tarball::{download_tarball, extract_tgz_atomic, MAX_TARBALL_SIZE};
pub use version::resolve_version;
