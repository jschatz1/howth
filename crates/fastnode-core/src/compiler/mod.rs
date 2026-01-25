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
use std::path::Path;

/// Target ES version (alias for compatibility).
pub type Target = EsTarget;

/// Import information extracted from source code.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The import specifier.
    pub specifier: String,
    /// Whether this is a dynamic import().
    pub dynamic: bool,
}

/// Parse import statements from source code.
///
/// Uses a simple regex-based approach for now.
/// TODO: Use SWC AST parsing for accuracy.
pub fn parse_imports(source: &str, _path: &Path) -> Result<Vec<crate::bundler::Import>, CompilerError> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Static imports
        if trimmed.starts_with("import ") {
            if let Some(spec) = extract_import_specifier(trimmed) {
                let names = extract_import_names(trimmed);
                imports.push(crate::bundler::Import {
                    specifier: spec,
                    dynamic: false,
                    names,
                });
            }
        }

        // Dynamic imports
        if trimmed.contains("import(") {
            if let Some(spec) = extract_dynamic_import(trimmed) {
                imports.push(crate::bundler::Import {
                    specifier: spec,
                    dynamic: true,
                    names: Vec::new(), // Dynamic imports don't have static names
                });
            }
        }

        // Re-exports
        if trimmed.starts_with("export ") && trimmed.contains(" from ") {
            if let Some(spec) = extract_import_specifier(trimmed) {
                let names = extract_reexport_names(trimmed);
                imports.push(crate::bundler::Import {
                    specifier: spec,
                    dynamic: false,
                    names,
                });
            }
        }
    }

    Ok(imports)
}

/// Extract imported names from an import statement.
fn extract_import_names(line: &str) -> Vec<crate::bundler::ImportedName> {
    let mut names = Vec::new();

    // Named imports: import { foo, bar as baz } from '...'
    if let Some(brace_start) = line.find('{') {
        if let Some(brace_end) = line.find('}') {
            let named = &line[brace_start + 1..brace_end];
            for part in named.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if part.contains(" as ") {
                    let parts: Vec<&str> = part.split(" as ").collect();
                    if parts.len() == 2 {
                        names.push(crate::bundler::ImportedName {
                            imported: parts[0].trim().to_string(),
                            local: parts[1].trim().to_string(),
                        });
                    }
                } else {
                    names.push(crate::bundler::ImportedName {
                        imported: part.to_string(),
                        local: part.to_string(),
                    });
                }
            }
        }
    }

    // Default import: import foo from '...'
    // Check for default import before 'from'
    if let Some(from_idx) = line.find(" from ") {
        let before_from = &line[7..from_idx].trim(); // after "import "
        if !before_from.is_empty() && !before_from.starts_with('{') && !before_from.starts_with('*') {
            // Could be: "foo" or "foo, { bar }"
            let default_name = if before_from.contains(',') {
                before_from.split(',').next().map(|s| s.trim())
            } else if before_from.contains('{') {
                before_from.split('{').next().map(|s| s.trim())
            } else {
                Some(*before_from)
            };
            if let Some(name) = default_name {
                if !name.is_empty() && !name.starts_with('{') && !name.starts_with('*') {
                    names.push(crate::bundler::ImportedName {
                        imported: "default".to_string(),
                        local: name.to_string(),
                    });
                }
            }
        }
    }

    // Namespace import: import * as foo from '...'
    if line.contains("* as ") {
        if let Some(star_idx) = line.find("* as ") {
            let after_star = &line[star_idx + 5..];
            if let Some(space_idx) = after_star.find(' ') {
                let ns_name = after_star[..space_idx].trim();
                names.push(crate::bundler::ImportedName {
                    imported: "*".to_string(),
                    local: ns_name.to_string(),
                });
            }
        }
    }

    names
}

/// Extract re-exported names from an export statement.
fn extract_reexport_names(line: &str) -> Vec<crate::bundler::ImportedName> {
    let mut names = Vec::new();

    // export { foo, bar } from '...'
    if let Some(brace_start) = line.find('{') {
        if let Some(brace_end) = line.find('}') {
            let named = &line[brace_start + 1..brace_end];
            for part in named.split(',') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if part.contains(" as ") {
                    let parts: Vec<&str> = part.split(" as ").collect();
                    if parts.len() == 2 {
                        names.push(crate::bundler::ImportedName {
                            imported: parts[0].trim().to_string(),
                            local: parts[1].trim().to_string(),
                        });
                    }
                } else {
                    names.push(crate::bundler::ImportedName {
                        imported: part.to_string(),
                        local: part.to_string(),
                    });
                }
            }
        }
    }

    // export * from '...' - namespace re-export
    if line.contains("export *") && !line.contains(" as ") {
        names.push(crate::bundler::ImportedName {
            imported: "*".to_string(),
            local: "*".to_string(),
        });
    }

    names
}

/// Extract import specifier from an import/export statement.
fn extract_import_specifier(line: &str) -> Option<String> {
    // Look for 'xxx' or "xxx" after "from"
    if let Some(from_idx) = line.find(" from ") {
        let after_from = &line[from_idx + 6..];
        return extract_string_literal(after_from);
    }

    // Side-effect import: import 'xxx' or import "xxx"
    if line.starts_with("import '") || line.starts_with("import \"") {
        return extract_string_literal(&line[7..]);
    }

    None
}

/// Extract specifier from dynamic import().
fn extract_dynamic_import(line: &str) -> Option<String> {
    if let Some(start) = line.find("import(") {
        let after = &line[start + 7..];
        return extract_string_literal(after);
    }
    None
}

/// Extract a string literal from the start of a string.
fn extract_string_literal(s: &str) -> Option<String> {
    let s = s.trim();

    if s.starts_with('\'') {
        let end = s[1..].find('\'')?;
        return Some(s[1..=end].to_string());
    }

    if s.starts_with('"') {
        let end = s[1..].find('"')?;
        return Some(s[1..=end].to_string());
    }

    None
}

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
