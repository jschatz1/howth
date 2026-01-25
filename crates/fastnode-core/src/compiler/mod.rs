//! Compiler backend abstraction for transpilation.
//!
//! This module provides a trait-based abstraction over JavaScript/TypeScript
//! compilers like SWC. The design allows for swappable backends without
//! changing the rest of the build system.
//!
//! ## Design Goals
//!
//! 1. **Fast in-process transpilation** - No shell subprocess, direct library call
//! 2. **Deterministic** - `TranspileSpec` captures all options for reproducible builds
//! 3. **Swappable backend** - Rest of howth never calls SWC directly
//! 4. **Fits existing cache model** - Input hash + output fingerprint
//!
//! ## Usage
//!
//! ```ignore
//! use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};
//!
//! let backend = SwcBackend::new();
//! let spec = TranspileSpec::new("src/App.tsx", "dist/App.js")
//!     .with_jsx_runtime(JsxRuntime::Automatic);
//!
//! let output = backend.transpile(&spec)?;
//! println!("{}", output.code);
//! ```

pub mod spec;
pub mod swc;

pub use spec::{
    Diagnostic, DiagnosticSeverity, EsTarget, JsxRuntime, ModuleKind, SourceMapKind,
    TranspileOutput, TranspileSpec,
};
pub use swc::SwcBackend;

use std::fmt;

/// Error during compilation.
#[derive(Debug)]
pub struct CompilerError {
    /// Error code.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
    /// Compiler diagnostics (if available).
    pub diagnostics: Vec<Diagnostic>,
}

impl CompilerError {
    /// Create a new compiler error.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Create an error with diagnostics.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// Create a parse error.
    #[must_use]
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new("COMPILER_PARSE_ERROR", message)
    }

    /// Create a transform error.
    #[must_use]
    pub fn transform_error(message: impl Into<String>) -> Self {
        Self::new("COMPILER_TRANSFORM_ERROR", message)
    }

    /// Create an I/O error.
    #[must_use]
    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new("COMPILER_IO_ERROR", message)
    }

    /// Create an unsupported file type error.
    #[must_use]
    pub fn unsupported_file(message: impl Into<String>) -> Self {
        Self::new("COMPILER_UNSUPPORTED_FILE", message)
    }
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        for diag in &self.diagnostics {
            write!(f, "\n  - {}: {}", diag.severity.as_str(), diag.message)?;
            if let (Some(file), Some(line), Some(col)) = (&diag.file, diag.line, diag.column) {
                write!(f, " at {}:{}:{}", file.display(), line, col)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for CompilerError {}

/// Compiler backend trait for transpilation.
///
/// Implementations of this trait provide the actual transpilation logic.
/// The trait is `Send + Sync` to allow use across threads.
///
/// ## Implementations
///
/// - `SwcBackend` - Uses SWC for fast JavaScript/TypeScript transpilation
pub trait CompilerBackend: Send + Sync {
    /// Get the backend name (e.g., "swc", "oxc").
    fn name(&self) -> &'static str;

    /// Transpile a file according to the specification.
    ///
    /// # Arguments
    ///
    /// * `spec` - The transpilation specification
    /// * `source` - The source code to transpile
    ///
    /// # Errors
    ///
    /// Returns a `CompilerError` if:
    /// - The source code has syntax errors
    /// - The transformation fails
    /// - The file type is unsupported
    fn transpile(&self, spec: &TranspileSpec, source: &str) -> Result<TranspileOutput, CompilerError>;

    /// Check if this backend supports the given file extension.
    ///
    /// Returns `true` for extensions like "js", "jsx", "ts", "tsx", "mjs", "mts".
    fn supports_extension(&self, ext: &str) -> bool {
        matches!(
            ext.to_lowercase().as_str(),
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "mts" | "cjs" | "cts"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_error_display() {
        let error = CompilerError::parse_error("Unexpected token");
        assert!(error.to_string().contains("COMPILER_PARSE_ERROR"));
        assert!(error.to_string().contains("Unexpected token"));
    }

    #[test]
    fn test_compiler_error_with_diagnostics() {
        let diag = Diagnostic::error("Missing semicolon")
            .with_location(std::path::PathBuf::from("src/App.tsx"), 10, 5);

        let error = CompilerError::parse_error("Parse failed")
            .with_diagnostics(vec![diag]);

        let display = error.to_string();
        assert!(display.contains("src/App.tsx"));
        assert!(display.contains("Missing semicolon"));
    }

    #[test]
    fn test_default_extension_support() {
        // Test trait default implementation via SwcBackend
        let backend = SwcBackend::new();

        assert!(backend.supports_extension("js"));
        assert!(backend.supports_extension("jsx"));
        assert!(backend.supports_extension("ts"));
        assert!(backend.supports_extension("tsx"));
        assert!(backend.supports_extension("mjs"));
        assert!(backend.supports_extension("mts"));
        assert!(backend.supports_extension("cjs"));
        assert!(backend.supports_extension("cts"));

        // Case insensitive
        assert!(backend.supports_extension("TS"));
        assert!(backend.supports_extension("TSX"));

        // Unsupported
        assert!(!backend.supports_extension("css"));
        assert!(!backend.supports_extension("json"));
    }
}
