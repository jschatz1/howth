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

#![allow(clippy::similar_names)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::manual_strip)]

pub mod ast_parser;
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
/// Uses the arena-based AST parser for accurate extraction.
/// Handles all edge cases including template literals, comments,
/// dynamic imports, and re-exports.
pub fn parse_imports(
    source: &str,
    path: &Path,
) -> Result<Vec<crate::bundler::Import>, CompilerError> {
    let imports = ast_parser::extract_imports_ast(source);

    // If AST parser returned nothing but source has import statements,
    // fall back to regex parser. This handles JSX/TSX files that the
    // arena parser can't parse yet.
    if imports.is_empty() && !source.is_empty() && source.contains("import ") {
        return parse_imports_regex(source, path);
    }

    Ok(imports)
}

/// Parse imports using the legacy regex-based approach.
/// Kept for benchmarking comparison.
#[allow(dead_code)]
pub fn parse_imports_regex(
    source: &str,
    _path: &Path,
) -> Result<Vec<crate::bundler::Import>, CompilerError> {
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
        if !before_from.is_empty() && !before_from.starts_with('{') && !before_from.starts_with('*')
        {
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

/// Transform JSX source using howth-parser (no SWC needed).
/// Returns (transformed_code, imports) in a single parse+codegen pass.
pub fn transform_jsx(source: &str) -> Result<(String, Vec<crate::bundler::Import>), CompilerError> {
    use howth_parser::{Parser, ParserOptions, Codegen, CodegenOptions};

    let parser_opts = ParserOptions {
        module: true,
        jsx: true,
        ..Default::default()
    };

    let ast = Parser::new(source, parser_opts)
        .parse()
        .map_err(|e| CompilerError::parse_error(e.to_string()))?;

    // Extract imports from the non-arena AST
    let mut imports = extract_imports_from_ast(&ast);

    // Add jsx runtime to dependency graph
    imports.push(crate::bundler::Import {
        specifier: "react/jsx-runtime".to_string(),
        dynamic: false,
        names: vec![
            crate::bundler::ImportedName { imported: "jsx".to_string(), local: "_jsx".to_string() },
            crate::bundler::ImportedName { imported: "jsxs".to_string(), local: "_jsxs".to_string() },
            crate::bundler::ImportedName { imported: "Fragment".to_string(), local: "_Fragment".to_string() },
        ],
    });

    // Generate transformed JS with JSX→_jsx() calls
    let code = Codegen::new(&ast, CodegenOptions::default()).generate();

    // Prepend jsx runtime import
    let code = format!(
        "import {{ jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment }} from \"react/jsx-runtime\";\n{}",
        code
    );

    Ok((code, imports))
}

/// Extract imports from a non-arena `Ast` (used by `transform_jsx` to avoid re-parsing).
fn extract_imports_from_ast(ast: &howth_parser::Ast) -> Vec<crate::bundler::Import> {
    use howth_parser::{StmtKind, ExportDecl, ImportSpecifier};
    use crate::bundler::{Import, ImportedName};

    let mut imports = Vec::new();

    for stmt in &ast.stmts {
        match &stmt.kind {
            StmtKind::Import(import_decl) => {
                // Skip type-only imports (TypeScript)
                if import_decl.is_type_only {
                    continue;
                }
                let mut names = Vec::new();
                for spec in &import_decl.specifiers {
                    match spec {
                        ImportSpecifier::Default { local, .. } => {
                            names.push(ImportedName {
                                imported: "default".to_string(),
                                local: local.clone(),
                            });
                        }
                        ImportSpecifier::Namespace { local, .. } => {
                            names.push(ImportedName {
                                imported: "*".to_string(),
                                local: local.clone(),
                            });
                        }
                        ImportSpecifier::Named { imported, local, is_type, .. } => {
                            // Skip type-only named imports
                            if *is_type { continue; }
                            names.push(ImportedName {
                                imported: imported.clone(),
                                local: local.clone(),
                            });
                        }
                    }
                }
                imports.push(Import {
                    specifier: import_decl.source.clone(),
                    dynamic: false,
                    names,
                });
            }
            StmtKind::Export(export_decl) => {
                match export_decl.as_ref() {
                    ExportDecl::All { source, .. } => {
                        imports.push(Import {
                            specifier: source.clone(),
                            dynamic: false,
                            names: vec![ImportedName {
                                imported: "*".to_string(),
                                local: "*".to_string(),
                            }],
                        });
                    }
                    ExportDecl::Named { source: Some(source), specifiers, .. } => {
                        let names = specifiers
                            .iter()
                            .map(|s| ImportedName {
                                imported: s.local.clone(),
                                local: s.exported.clone(),
                            })
                            .collect();
                        imports.push(Import {
                            specifier: source.clone(),
                            dynamic: false,
                            names,
                        });
                    }
                    _ => {}
                }
            }
            StmtKind::Expr(expr) => {
                extract_dynamic_imports_expr(expr, &mut imports);
            }
            StmtKind::Var { decls, .. } => {
                for decl in decls {
                    if let Some(init) = &decl.init {
                        extract_dynamic_imports_expr(init, &mut imports);
                    }
                }
            }
            StmtKind::Function(func) => {
                extract_dynamic_imports_stmts(&func.body, &mut imports);
            }
            StmtKind::Block(stmts) => {
                extract_dynamic_imports_stmts(stmts, &mut imports);
            }
            StmtKind::If { consequent, alternate, .. } => {
                extract_dynamic_imports_stmt(consequent, &mut imports);
                if let Some(alt) = alternate {
                    extract_dynamic_imports_stmt(alt, &mut imports);
                }
            }
            _ => {}
        }
    }

    imports
}

fn extract_dynamic_imports_stmts(stmts: &[howth_parser::Stmt], imports: &mut Vec<crate::bundler::Import>) {
    for stmt in stmts {
        extract_dynamic_imports_stmt(stmt, imports);
    }
}

fn extract_dynamic_imports_stmt(stmt: &howth_parser::Stmt, imports: &mut Vec<crate::bundler::Import>) {
    use howth_parser::StmtKind;
    match &stmt.kind {
        StmtKind::Expr(expr) => extract_dynamic_imports_expr(expr, imports),
        StmtKind::Var { decls, .. } => {
            for decl in decls {
                if let Some(init) = &decl.init {
                    extract_dynamic_imports_expr(init, imports);
                }
            }
        }
        StmtKind::Function(func) => extract_dynamic_imports_stmts(&func.body, imports),
        StmtKind::Block(stmts) => extract_dynamic_imports_stmts(stmts, imports),
        StmtKind::If { consequent, alternate, .. } => {
            extract_dynamic_imports_stmt(consequent, imports);
            if let Some(alt) = alternate {
                extract_dynamic_imports_stmt(alt, imports);
            }
        }
        StmtKind::Return { arg: Some(expr) } => extract_dynamic_imports_expr(expr, imports),
        _ => {}
    }
}

fn extract_dynamic_imports_expr(expr: &howth_parser::Expr, imports: &mut Vec<crate::bundler::Import>) {
    use howth_parser::ExprKind;
    use crate::bundler::Import;

    match &expr.kind {
        ExprKind::Import(source_expr) => {
            if let ExprKind::String(s) = &source_expr.kind {
                imports.push(Import {
                    specifier: s.clone(),
                    dynamic: true,
                    names: Vec::new(),
                });
            }
        }
        ExprKind::Call { callee, args, .. } => {
            extract_dynamic_imports_expr(callee, imports);
            for arg in args {
                extract_dynamic_imports_expr(arg, imports);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            extract_dynamic_imports_expr(left, imports);
            extract_dynamic_imports_expr(right, imports);
        }
        ExprKind::Conditional { test, consequent, alternate, .. } => {
            extract_dynamic_imports_expr(test, imports);
            extract_dynamic_imports_expr(consequent, imports);
            extract_dynamic_imports_expr(alternate, imports);
        }
        ExprKind::Arrow(arrow) => {
            if let howth_parser::ArrowBody::Expr(body) = &arrow.body {
                extract_dynamic_imports_expr(body, imports);
            }
        }
        ExprKind::Await(inner) => extract_dynamic_imports_expr(inner, imports),
        _ => {}
    }
}

/// Transform TypeScript source using howth-parser (no SWC needed).
/// Returns (transformed_code, imports) in a single parse+codegen pass.
pub fn transform_ts(source: &str) -> Result<(String, Vec<crate::bundler::Import>), CompilerError> {
    use howth_parser::{Parser, ParserOptions, Codegen, CodegenOptions};

    let parser_opts = ParserOptions {
        module: true,
        jsx: false,
        typescript: true,
        ..Default::default()
    };

    let ast = Parser::new(source, parser_opts)
        .parse()
        .map_err(|e| CompilerError::parse_error(e.to_string()))?;

    // Extract imports from the non-arena AST
    let imports = extract_imports_from_ast(&ast);

    // Generate JS with types stripped
    let code = Codegen::new(&ast, CodegenOptions::default()).generate();

    Ok((code, imports))
}

/// Transform TSX source using howth-parser (no SWC needed).
/// Returns (transformed_code, imports) in a single parse+codegen pass.
pub fn transform_tsx(source: &str) -> Result<(String, Vec<crate::bundler::Import>), CompilerError> {
    use howth_parser::{Parser, ParserOptions, Codegen, CodegenOptions};

    let parser_opts = ParserOptions {
        module: true,
        jsx: true,
        typescript: true,
        ..Default::default()
    };

    let ast = Parser::new(source, parser_opts)
        .parse()
        .map_err(|e| CompilerError::parse_error(e.to_string()))?;

    // Extract imports from the non-arena AST
    let mut imports = extract_imports_from_ast(&ast);

    // Add jsx runtime to dependency graph
    imports.push(crate::bundler::Import {
        specifier: "react/jsx-runtime".to_string(),
        dynamic: false,
        names: vec![
            crate::bundler::ImportedName { imported: "jsx".to_string(), local: "_jsx".to_string() },
            crate::bundler::ImportedName { imported: "jsxs".to_string(), local: "_jsxs".to_string() },
            crate::bundler::ImportedName { imported: "Fragment".to_string(), local: "_Fragment".to_string() },
        ],
    });

    // Generate transformed JS with types stripped and JSX→_jsx() calls
    let code = Codegen::new(&ast, CodegenOptions::default()).generate();

    // Prepend jsx runtime import
    let code = format!(
        "import {{ jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment }} from \"react/jsx-runtime\";\n{}",
        code
    );

    Ok((code, imports))
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
    fn transpile(
        &self,
        spec: &TranspileSpec,
        source: &str,
    ) -> Result<TranspileOutput, CompilerError>;

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
        let diag = Diagnostic::error("Missing semicolon").with_location(
            std::path::PathBuf::from("src/App.tsx"),
            10,
            5,
        );

        let error = CompilerError::parse_error("Parse failed").with_diagnostics(vec![diag]);

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

    #[test]
    fn test_transform_jsx_includes_runtime_import() {
        let source = r#"import { useState } from "react";
const App = () => <div>hello</div>;
export default App;"#;
        let (code, imports) = transform_jsx(source).unwrap();
        // Code should contain the jsx runtime import
        assert!(code.contains("react/jsx-runtime"), "code should have jsx runtime import");
        // Imports returned to bundler should include react/jsx-runtime for dependency tracking
        assert!(
            imports.iter().any(|i| i.specifier == "react/jsx-runtime"),
            "imports should include react/jsx-runtime for dependency graph"
        );
        // Original import should also be present
        assert!(
            imports.iter().any(|i| i.specifier == "react"),
            "imports should include react"
        );
    }

    #[test]
    fn test_transform_tsx_includes_runtime_import() {
        let source = r#"import { useState } from "react";
type Props = { name: string };
const App = (props: Props) => <div>{props.name}</div>;
export default App;"#;
        let (_code, imports) = transform_tsx(source).unwrap();
        assert!(
            imports.iter().any(|i| i.specifier == "react/jsx-runtime"),
            "TSX imports should include react/jsx-runtime for dependency graph"
        );
        assert!(
            imports.iter().any(|i| i.specifier == "react"),
            "TSX imports should include react"
        );
    }

    #[test]
    fn test_transform_ts_strips_types() {
        let source = r#"import { type Foo, bar } from "./mod";
interface Config { debug: boolean }
const x: number = bar();
export { x };"#;
        let (code, imports) = transform_ts(source).unwrap();
        assert!(!code.contains("interface"), "interface should be stripped");
        assert!(!code.contains(": number"), "type annotation should be stripped");
        assert!(code.contains("bar"), "runtime import preserved");
        assert!(!imports.is_empty(), "should have imports");
    }
}
