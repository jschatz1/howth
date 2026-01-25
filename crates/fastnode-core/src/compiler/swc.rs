//! SWC compiler backend implementation.
//!
//! This module provides the SWC-based implementation of the `CompilerBackend` trait.
//! SWC is a fast JavaScript/TypeScript compiler written in Rust.
//!
//! ## Features
//!
//! - JSX/TSX transpilation
//! - TypeScript type stripping
//! - Module transformation (ESM <-> CommonJS)
//! - Source map generation
//! - Minification (optional)
//!
//! ## Feature Flags
//!
//! - `swc`: Enable full SWC integration (requires swc_core dependency)
//!
//! Without the `swc` feature, a stub implementation is used that performs
//! basic regex-based transformations for testing purposes.

#![allow(clippy::default_trait_access)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_raw_string_hashes)]

use super::spec::{JsxRuntime, SourceMapKind};
use super::{CompilerBackend, CompilerError, TranspileOutput, TranspileSpec};

#[cfg(feature = "swc")]
use super::spec::EsTarget;

/// SWC-based compiler backend.
///
/// Provides fast, in-process JavaScript/TypeScript transpilation using SWC.
///
/// ## Thread Safety
///
/// `SwcBackend` is `Send + Sync` and can be shared across threads.
/// Each call to `transpile` is independent and thread-safe.
#[derive(Debug, Clone, Default)]
pub struct SwcBackend {
    // Configuration can be added here if needed
    _private: (),
}

impl SwcBackend {
    /// Create a new SWC backend with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Determine the syntax for a file based on its extension.
    fn is_typescript(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_lowercase().as_str(), "ts" | "tsx" | "mts" | "cts"))
            .unwrap_or(false)
    }

    /// Determine if a file uses JSX syntax.
    fn is_jsx(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_lowercase().as_str(), "jsx" | "tsx"))
            .unwrap_or(false)
    }
}

impl CompilerBackend for SwcBackend {
    fn name(&self) -> &'static str {
        "swc"
    }

    fn transpile(
        &self,
        spec: &TranspileSpec,
        source: &str,
    ) -> Result<TranspileOutput, CompilerError> {
        // Validate input
        if source.is_empty() {
            return Ok(TranspileOutput::new(""));
        }

        let is_ts = Self::is_typescript(&spec.input_path);
        let is_jsx = Self::is_jsx(&spec.input_path);

        // TODO: This is a stub implementation. Full SWC integration will be added
        // when the swc_core dependency is added in Phase 3.
        //
        // For now, we do minimal processing:
        // 1. Strip TypeScript types (basic regex-based for now)
        // 2. Transform JSX (basic regex-based for now)
        // 3. Generate source map placeholder
        //
        // The full implementation will use swc_core for proper parsing and transformation.

        #[cfg(not(feature = "swc"))]
        {
            // Stub implementation without SWC
            let mut code = source.to_string();

            // Very basic TypeScript type stripping (for testing only)
            // This is NOT a real transpiler - just a placeholder
            if is_ts {
                // Remove simple type annotations like `: string`, `: number`, etc.
                // This is extremely naive and for testing purposes only
                code = strip_simple_types(&code);
            }

            // Very basic JSX transformation (for testing only)
            // This is NOT a real transpiler - just a placeholder
            if is_jsx {
                code = transform_simple_jsx(&code, spec.jsx_runtime);
            }

            // Add module transformation note
            if matches!(spec.module, ModuleKind::CommonJS) {
                // Real implementation would transform import/export to require/module.exports
            }

            let mut output = TranspileOutput::new(code);

            // Generate inline source map placeholder if requested
            if matches!(spec.sourcemaps, SourceMapKind::Inline) {
                // Real implementation would generate actual source maps
                let map = generate_placeholder_sourcemap(&spec.input_path);
                output = output.with_source_map(map);
            } else if matches!(spec.sourcemaps, SourceMapKind::External) {
                let map = generate_placeholder_sourcemap(&spec.input_path);
                output = output.with_source_map(map);
            }

            Ok(output)
        }

        #[cfg(feature = "swc")]
        {
            // Full SWC implementation
            compile_with_swc(spec, source, is_ts, is_jsx)
        }
    }
}

/// Strip simple TypeScript type annotations (stub implementation).
///
/// This is a VERY naive implementation for testing purposes only.
/// The real implementation uses SWC's parser and transformer.
#[cfg(not(feature = "swc"))]
fn strip_simple_types(source: &str) -> String {
    let mut result = source.to_string();

    // Remove interface declarations (very naive)
    // interface Foo { ... }
    if let Ok(re) = regex_lite::Regex::new(r"(?m)^interface\s+\w+\s*\{[^}]*\}\s*") {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove type declarations (very naive)
    // type Foo = ...;
    if let Ok(re) = regex_lite::Regex::new(r"(?m)^type\s+\w+\s*=\s*[^;]+;\s*") {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove function return types: ): type { -> ) {
    // Note: regex-lite doesn't support lookahead, so we capture and replace
    if let Ok(re) = regex_lite::Regex::new(r"\)\s*:\s*\w+(\s*\[\s*\])?\s*\{") {
        result = re.replace_all(&result, ") {").to_string();
    }

    // Remove parameter type annotations: (a: type, -> (a,
    // Simple approach: match param: type patterns
    if let Ok(re) = regex_lite::Regex::new(r"(\w+)\s*:\s*\w+(\s*\[\s*\])?\s*,") {
        result = re.replace_all(&result, "$1,").to_string();
    }

    // Remove last parameter type annotation: (a: type) -> (a)
    if let Ok(re) = regex_lite::Regex::new(r"(\w+)\s*:\s*\w+(\s*\[\s*\])?\s*\)") {
        result = re.replace_all(&result, "$1)").to_string();
    }

    // Remove variable type annotations: const x: type = -> const x =
    if let Ok(re) = regex_lite::Regex::new(r"(const|let|var)\s+(\w+)\s*:\s*\w+(\s*\[\s*\])?\s*=") {
        result = re.replace_all(&result, "$1 $2 =").to_string();
    }

    // Remove generic type parameters
    if let Ok(re) = regex_lite::Regex::new(r"<[^>]+>") {
        result = re.replace_all(&result, "").to_string();
    }

    // Remove `as` type assertions
    if let Ok(re) = regex_lite::Regex::new(r"\s+as\s+\w+") {
        result = re.replace_all(&result, "").to_string();
    }

    result
}

/// Transform simple JSX (stub implementation).
///
/// This is a VERY naive implementation for testing purposes only.
/// The real implementation uses SWC's JSX transformer.
#[cfg(not(feature = "swc"))]
fn transform_simple_jsx(source: &str, runtime: JsxRuntime) -> String {
    let mut result = source.to_string();

    match runtime {
        JsxRuntime::Automatic => {
            // Add jsx-runtime import at the top
            let import = "import { jsx as _jsx } from \"react/jsx-runtime\";\n";
            if !result.contains("jsx-runtime") {
                result = format!("{import}{result}");
            }

            // Very naive: transform <div>text</div> to _jsx("div", { children: "text" })
            // This won't handle nested elements, attributes, etc.
            let re_simple = regex_lite::Regex::new(r"<(\w+)>([^<]*)</\1>").ok();
            if let Some(re) = re_simple {
                result = re
                    .replace_all(&result, |caps: &regex_lite::Captures| {
                        let tag = &caps[1];
                        let content = &caps[2];
                        format!("_jsx(\"{tag}\", {{ children: \"{content}\" }})")
                    })
                    .to_string();
            }

            // Transform self-closing tags: <br /> -> _jsx("br", {})
            let re_self_closing = regex_lite::Regex::new(r"<(\w+)\s*/>").ok();
            if let Some(re) = re_self_closing {
                result = re
                    .replace_all(&result, |caps: &regex_lite::Captures| {
                        let tag = &caps[1];
                        format!("_jsx(\"{tag}\", {{}})")
                    })
                    .to_string();
            }
        }
        JsxRuntime::Classic => {
            // Transform <div>text</div> to React.createElement("div", null, "text")
            let re_simple = regex_lite::Regex::new(r"<(\w+)>([^<]*)</\1>").ok();
            if let Some(re) = re_simple {
                result = re
                    .replace_all(&result, |caps: &regex_lite::Captures| {
                        let tag = &caps[1];
                        let content = &caps[2];
                        format!("React.createElement(\"{tag}\", null, \"{content}\")")
                    })
                    .to_string();
            }

            // Transform self-closing tags: <br /> -> React.createElement("br", null)
            let re_self_closing = regex_lite::Regex::new(r"<(\w+)\s*/>").ok();
            if let Some(re) = re_self_closing {
                result = re
                    .replace_all(&result, |caps: &regex_lite::Captures| {
                        let tag = &caps[1];
                        format!("React.createElement(\"{tag}\", null)")
                    })
                    .to_string();
            }
        }
    }

    result
}

/// Generate a placeholder source map (stub implementation).
#[cfg(not(feature = "swc"))]
fn generate_placeholder_sourcemap(input_path: &std::path::Path) -> String {
    let filename = input_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    format!(
        r#"{{"version":3,"sources":["{}"],"names":[],"mappings":"AAAA"}}"#,
        filename
    )
}

// ============================================================
// Full SWC Implementation (requires `swc` feature)
// ============================================================

#[cfg(feature = "swc")]
fn compile_with_swc(
    spec: &TranspileSpec,
    source: &str,
    is_ts: bool,
    is_jsx: bool,
) -> Result<TranspileOutput, CompilerError> {
    use swc_common::{
        comments::SingleThreadedComments, errors::Handler, sync::Lrc, FileName, Globals, Mark,
        SourceMap, GLOBALS,
    };
    use swc_ecma_ast::{EsVersion, Program};
    use swc_ecma_codegen::{text_writer::JsWriter, Emitter};
    use swc_ecma_parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax, TsSyntax};
    use swc_ecma_transforms_base::{fixer::fixer, hygiene::hygiene, resolver};
    use swc_ecma_transforms_react::{react, Options as ReactOptions, Runtime};
    use swc_ecma_transforms_typescript::strip;
    use swc_ecma_visit::FoldWith;

    // Create source map
    let cm: Lrc<SourceMap> = Default::default();

    // Create a simple handler that discards output (we handle errors via Result)
    let handler = Handler::with_emitter_writer(Box::new(std::io::sink()), Some(cm.clone()));

    // Create source file
    let filename = spec
        .input_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.js");
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom(filename.to_string())),
        source.to_string(),
    );

    // Configure syntax
    let syntax = if is_ts {
        Syntax::Typescript(TsSyntax {
            tsx: is_jsx,
            decorators: true,
            ..Default::default()
        })
    } else {
        Syntax::Es(EsSyntax {
            jsx: is_jsx,
            decorators: true,
            ..Default::default()
        })
    };

    // Determine target ES version
    let target = match spec.target {
        EsTarget::ES2015 => EsVersion::Es2015,
        EsTarget::ES2016 => EsVersion::Es2016,
        EsTarget::ES2017 => EsVersion::Es2017,
        EsTarget::ES2018 => EsVersion::Es2018,
        EsTarget::ES2019 => EsVersion::Es2019,
        EsTarget::ES2020 => EsVersion::Es2020,
        EsTarget::ES2021 => EsVersion::Es2021,
        EsTarget::ES2022 => EsVersion::Es2022,
        EsTarget::ES2023 => EsVersion::Es2022, // Use ES2022 as fallback
        EsTarget::ES2024 => EsVersion::Es2022, // Use ES2022 as fallback
        EsTarget::ESNext => EsVersion::EsNext,
    };

    let comments = SingleThreadedComments::default();

    // Parse
    let lexer = Lexer::new(syntax, target, StringInput::from(&*fm), Some(&comments));

    let mut parser = Parser::new_from(lexer);
    let mut errors = Vec::new();

    let module = parser.parse_module().map_err(|e| {
        let kind = format!("{:?}", e.kind());
        e.into_diagnostic(&handler).emit();
        CompilerError::parse_error(format!("Failed to parse: {kind}"))
    })?;

    // Collect parse errors
    for e in parser.take_errors() {
        errors.push(format!("{:?}", e.kind()));
    }

    if !errors.is_empty() {
        return Err(CompilerError::parse_error(errors.join(", ")));
    }

    // Transform
    let output = GLOBALS.set(&Globals::default(), || {
        let unresolved_mark = Mark::new();
        let top_level_mark = Mark::new();

        // Wrap module in Program for transforms that require it
        let mut program = Program::Module(module);

        // Apply resolver
        program = program.fold_with(&mut resolver(unresolved_mark, top_level_mark, is_ts));

        // Strip TypeScript types (requires Program as entry)
        if is_ts {
            program = program.fold_with(&mut strip(unresolved_mark, top_level_mark));
        }

        // Extract module back for remaining transforms
        let mut module = match program {
            Program::Module(m) => m,
            Program::Script(s) => {
                // Convert script to module (shouldn't happen for our use case)
                swc_ecma_ast::Module {
                    span: s.span,
                    body: s
                        .body
                        .into_iter()
                        .map(swc_ecma_ast::ModuleItem::Stmt)
                        .collect(),
                    shebang: s.shebang,
                }
            }
        };

        // Transform JSX
        if is_jsx {
            let runtime = match spec.jsx_runtime {
                JsxRuntime::Automatic => Runtime::Automatic,
                JsxRuntime::Classic => Runtime::Classic,
            };

            let react_options = ReactOptions {
                runtime: Some(runtime),
                import_source: Some("react".to_string()),
                ..Default::default()
            };

            module = module.fold_with(&mut react(
                cm.clone(),
                Some(&comments),
                react_options,
                top_level_mark,
                unresolved_mark,
            ));
        }

        // Apply hygiene and fixer
        module = module.fold_with(&mut hygiene());
        module = module.fold_with(&mut fixer(Some(&comments)));

        module
    });

    // Generate code
    let mut buf = Vec::new();
    let mut src_map_buf = Vec::new();

    {
        let writer = JsWriter::new(cm.clone(), "\n", &mut buf, Some(&mut src_map_buf));

        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config::default()
                .with_minify(spec.minify)
                .with_target(target),
            cm: cm.clone(),
            comments: Some(&comments),
            wr: writer,
        };

        emitter
            .emit_module(&output)
            .map_err(|e| CompilerError::transform_error(format!("Failed to emit: {e}")))?;
    }

    let code = String::from_utf8(buf)
        .map_err(|e| CompilerError::transform_error(format!("Invalid UTF-8 output: {e}")))?;

    // Generate source map if requested
    let source_map = match spec.sourcemaps {
        SourceMapKind::None => None,
        SourceMapKind::Inline | SourceMapKind::External => {
            // Build source map from the collected mappings
            let srcmap = cm.build_source_map(&src_map_buf);
            let mut map_buf = Vec::new();
            srcmap.to_writer(&mut map_buf).map_err(|e| {
                CompilerError::transform_error(format!("Failed to write source map: {e}"))
            })?;
            Some(String::from_utf8(map_buf).unwrap_or_default())
        }
    };

    let mut output = TranspileOutput::new(code);
    if let Some(map) = source_map {
        output = output.with_source_map(map);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_swc_backend_name() {
        let backend = SwcBackend::new();
        assert_eq!(backend.name(), "swc");
    }

    #[test]
    fn test_is_typescript() {
        assert!(SwcBackend::is_typescript(&PathBuf::from("app.ts")));
        assert!(SwcBackend::is_typescript(&PathBuf::from("app.tsx")));
        assert!(SwcBackend::is_typescript(&PathBuf::from("app.mts")));
        assert!(SwcBackend::is_typescript(&PathBuf::from("app.cts")));
        assert!(!SwcBackend::is_typescript(&PathBuf::from("app.js")));
        assert!(!SwcBackend::is_typescript(&PathBuf::from("app.jsx")));
    }

    #[test]
    fn test_is_jsx() {
        assert!(SwcBackend::is_jsx(&PathBuf::from("app.jsx")));
        assert!(SwcBackend::is_jsx(&PathBuf::from("app.tsx")));
        assert!(!SwcBackend::is_jsx(&PathBuf::from("app.js")));
        assert!(!SwcBackend::is_jsx(&PathBuf::from("app.ts")));
    }

    #[test]
    fn test_transpile_empty_source() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js");

        let output = backend.transpile(&spec, "").unwrap();
        assert_eq!(output.code, "");
    }

    #[test]
    fn test_transpile_simple_js() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.js", "dist/app.js");

        let source = "const x = 1;";
        let output = backend.transpile(&spec, source).unwrap();
        assert!(output.code.contains("const x = 1;"));
    }

    #[test]
    #[cfg(not(feature = "swc"))]
    fn test_transpile_typescript_file() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js");

        // The stub provides basic passthrough - real transformation comes with SWC
        let source = "const x = 1;";
        let output = backend.transpile(&spec, source).unwrap();
        assert!(output.code.contains("const x = 1;"));
    }

    #[test]
    #[cfg(not(feature = "swc"))]
    fn test_transpile_jsx_file() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/App.jsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);

        // The stub adds jsx-runtime import for JSX files
        let source = "const el = 1;";
        let output = backend.transpile(&spec, source).unwrap();

        // Stub implementation adds jsx-runtime import for JSX files
        assert!(
            output.code.contains("jsx-runtime"),
            "Should import jsx-runtime for JSX files"
        );
    }

    #[test]
    #[cfg(not(feature = "swc"))]
    fn test_transpile_jsx_classic_mode() {
        let backend = SwcBackend::new();
        let spec =
            TranspileSpec::new("src/App.jsx", "dist/App.js").with_jsx_runtime(JsxRuntime::Classic);

        // The stub passes through with classic mode - no jsx-runtime import
        let source = "const el = 1;";
        let output = backend.transpile(&spec, source).unwrap();

        // Classic mode doesn't add jsx-runtime import
        assert!(
            !output.code.contains("jsx-runtime"),
            "Classic mode should not import jsx-runtime"
        );
    }

    #[test]
    fn test_transpile_with_inline_sourcemap() {
        let backend = SwcBackend::new();
        let spec =
            TranspileSpec::new("src/app.js", "dist/app.js").with_sourcemaps(SourceMapKind::Inline);

        let source = "const x = 1;";
        let output = backend.transpile(&spec, source).unwrap();

        assert!(output.source_map.is_some());
        let map = output.source_map.unwrap();
        assert!(map.contains("\"version\":3"));
    }

    #[test]
    fn test_extension_support() {
        let backend = SwcBackend::new();

        assert!(backend.supports_extension("js"));
        assert!(backend.supports_extension("jsx"));
        assert!(backend.supports_extension("ts"));
        assert!(backend.supports_extension("tsx"));
        assert!(!backend.supports_extension("css"));
    }

    // ============================================================
    // Full SWC Tests (require `swc` feature)
    // ============================================================

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_transpile_typescript() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js");

        let source = r#"
            interface User {
                name: string;
                age: number;
            }
            const user: User = { name: "Alice", age: 30 };
            console.log(user.name);
        "#;

        let output = backend.transpile(&spec, source).unwrap();

        // Should strip TypeScript types
        assert!(!output.code.contains("interface"));
        assert!(!output.code.contains(": User"));
        assert!(!output.code.contains(": string"));
        assert!(!output.code.contains(": number"));

        // Should preserve runtime code
        assert!(output.code.contains("const user"));
        assert!(output.code.contains("console.log"));
    }

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_transpile_jsx_automatic() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/App.jsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);

        let source = r#"
            function App() {
                return <div className="app">Hello World</div>;
            }
        "#;

        let output = backend.transpile(&spec, source).unwrap();

        // Should transform JSX to jsx runtime calls
        assert!(output.code.contains("jsx") || output.code.contains("jsxs"));
        assert!(!output.code.contains("<div"));
    }

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_transpile_jsx_classic() {
        let backend = SwcBackend::new();
        let spec =
            TranspileSpec::new("src/App.jsx", "dist/App.js").with_jsx_runtime(JsxRuntime::Classic);

        let source = r"
            function App() {
                return <div>Hello</div>;
            }
        ";

        let output = backend.transpile(&spec, source).unwrap();

        // Should transform JSX to React.createElement
        assert!(
            output.code.contains("React.createElement") || output.code.contains("createElement")
        );
        assert!(!output.code.contains("<div"));
    }

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_transpile_tsx() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);

        let source = r"
            interface Props {
                name: string;
            }
            function Greeting({ name }: Props) {
                return <h1>Hello, {name}!</h1>;
            }
        ";

        let output = backend.transpile(&spec, source).unwrap();

        // Should strip TypeScript and transform JSX
        assert!(!output.code.contains("interface"));
        assert!(!output.code.contains(": Props"));
        assert!(!output.code.contains("<h1>"));
        assert!(output.code.contains("function Greeting"));
    }

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_source_map_generation() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js")
            .with_sourcemaps(SourceMapKind::External);

        let source = "const x: number = 42;";
        let output = backend.transpile(&spec, source).unwrap();

        assert!(output.source_map.is_some());
        let map = output.source_map.unwrap();
        assert!(map.contains("\"version\":3"));
        assert!(map.contains("\"sources\""));
    }

    #[test]
    #[cfg(feature = "swc")]
    fn test_swc_parse_error() {
        let backend = SwcBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js");

        let source = "const x = {"; // Invalid syntax
        let result = backend.transpile(&spec, source);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.code.contains("PARSE"));
    }
}
