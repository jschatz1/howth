//! Vite-compatible unbundled development server.
//!
//! Serves individual ES modules on demand instead of a single bundle.
//! Each request triggers a resolve → load → transpile → transform → rewrite
//! pipeline, with results cached until the source file changes.

pub mod config;
pub mod hmr;
pub mod prebundle;
pub mod rewrite;
pub mod transform;

pub use config::{HowthConfig, load_config, find_config_file};
pub use hmr::{HmrEngine, HmrModuleGraph, HmrModuleNode};
pub use prebundle::PreBundler;
pub use rewrite::{ImportRewriter, extract_import_urls, is_self_accepting_module};
pub use transform::ModuleTransformer;
