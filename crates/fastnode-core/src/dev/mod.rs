//! Vite-compatible unbundled development server.
//!
//! Serves individual ES modules on demand instead of a single bundle.
//! Each request triggers a resolve → load → transpile → transform → rewrite
//! pipeline, with results cached until the source file changes.

pub mod config;
pub mod env;
pub mod hmr;
pub mod prebundle;
pub mod rewrite;
pub mod transform;

pub use config::{find_config_file, load_config, load_tsconfig_paths, HowthConfig};
pub use env::{client_env_replacements, load_env_files};
pub use hmr::{HmrEngine, HmrModuleGraph, HmrModuleNode};
pub use prebundle::PreBundler;
pub use rewrite::{extract_import_urls, is_self_accepting_module, ImportRewriter};
pub use transform::ModuleTransformer;
