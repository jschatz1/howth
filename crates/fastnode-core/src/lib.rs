#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::return_self_not_must_use)]

pub mod bench;
pub mod build;
pub mod bundler;
pub mod compiler;
pub mod config;
pub mod doctor;
pub mod error;
pub mod imports;
pub mod paths;
pub mod pkg;
pub mod resolver;
pub mod runplan;
pub mod version;

pub use config::Config;
pub use error::Error;
pub use imports::{scan_imports, ImportSpecCore};
pub use resolver::{
    resolve_v0, NoCache, ResolveContext, ResolveReasonCode, ResolveResult, ResolveStatus,
    ResolverCache, ResolverConfig,
};
pub use runplan::{
    build_run_plan, build_run_plan_with_cache, codes as runplan_codes, ImportSpecOutput,
    ResolvedImportOutput, ResolverInfoOutput, RunPlanError, RunPlanInput, RunPlanOutput,
    RESOLVER_SCHEMA_VERSION, RUNPLAN_SCHEMA_VERSION,
};
pub use version::VERSION;
