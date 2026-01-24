//! Import discovery for JavaScript/TypeScript files.
//!
//! Provides a simple scanner to detect import/require specifiers.

mod scan;

pub use scan::{scan_imports, ImportSpecCore};
