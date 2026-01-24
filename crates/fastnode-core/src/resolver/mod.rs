//! Module resolver for JavaScript/TypeScript.
//!
//! Implements v0 resolution: relative, absolute, and bare specifiers.
//! v1.1 adds package.json exports and imports support.
//! v1.5 adds resolution tracing for pkg explain command.

mod exports;
mod pkg_json_cache;
pub mod trace;
mod v0;

pub use exports::{
    read_package_json, resolve_exports, resolve_exports_pattern, resolve_exports_root,
    resolve_exports_subpath, resolve_imports_map, ResolutionKind,
};
pub use pkg_json_cache::{CachedPkgJson, NoPkgJsonCache, PkgJsonCache, PkgJsonStamp};
pub use trace::{
    steps as trace_steps, warning_codes as trace_warning_codes, ResolveTrace, ResolveTraceStep,
    TraceWarning, PKG_EXPLAIN_SCHEMA_VERSION,
};
pub use v0::{
    resolve_v0, resolve_with_kind, resolve_with_trace, CachedResolveResult, FileStamp, NoCache,
    ResolveContext, ResolveReasonCode, ResolveResult, ResolveResultWithTrace, ResolveStatus,
    ResolverCache, ResolverCacheKey, ResolverConfig,
};
