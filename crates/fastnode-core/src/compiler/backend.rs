//! howth-parser compiler backend implementation.
//!
//! This module provides the howth-parser-based implementation of the `CompilerBackend` trait.
//! It handles JS/TS/JSX/TSX transpilation without any SWC dependency.

use super::spec::{JsxRuntime, SourceMapKind};
use super::{CompilerBackend, CompilerError, TranspileOutput, TranspileSpec};

/// howth-parser-based compiler backend.
///
/// Provides fast, in-process JavaScript/TypeScript transpilation using howth-parser.
///
/// ## Thread Safety
///
/// `HowthBackend` is `Send + Sync` and can be shared across threads.
/// Each call to `transpile` is independent and thread-safe.
#[derive(Debug, Clone, Default)]
pub struct HowthBackend {
    _private: (),
}

impl HowthBackend {
    /// Create a new howth-parser backend with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    fn is_typescript(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "ts" | "tsx" | "mts" | "cts"))
    }

    fn is_jsx(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "jsx" | "tsx"))
    }
}

impl CompilerBackend for HowthBackend {
    fn name(&self) -> &'static str {
        "howth"
    }

    fn transpile(
        &self,
        spec: &TranspileSpec,
        source: &str,
    ) -> Result<TranspileOutput, CompilerError> {
        use howth_parser::{Codegen, CodegenOptions, Parser, ParserOptions};

        if source.is_empty() {
            return Ok(TranspileOutput::new(""));
        }

        let is_ts = Self::is_typescript(&spec.input_path);
        let is_jsx = Self::is_jsx(&spec.input_path);

        let parser_opts = ParserOptions {
            module: true,
            jsx: is_jsx,
            typescript: is_ts,
        };

        let ast = Parser::new(source, parser_opts)
            .parse()
            .map_err(|e| CompilerError::parse_error(e.to_string()))?;

        let codegen_opts = CodegenOptions {
            minify: spec.minify,
            ..Default::default()
        };
        let mut code = Codegen::new(&ast, codegen_opts).generate();

        // Prepend JSX runtime import for JSX/TSX files (automatic mode only)
        if is_jsx && spec.jsx_runtime == JsxRuntime::Automatic {
            code = format!(
                "import {{ jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment }} from \"react/jsx-runtime\";\n{code}"
            );
        }

        let mut output = TranspileOutput::new(code);

        // Generate placeholder source map if requested
        if matches!(
            spec.sourcemaps,
            SourceMapKind::Inline | SourceMapKind::External
        ) {
            let filename = spec
                .input_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let map =
                format!(r#"{{"version":3,"sources":["{filename}"],"names":[],"mappings":"AAAA"}}"#);
            output = output.with_source_map(map);
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::super::spec::{JsxRuntime, SourceMapKind};
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_backend_name() {
        let backend = HowthBackend::new();
        assert_eq!(backend.name(), "howth");
    }

    #[test]
    fn test_is_typescript() {
        assert!(HowthBackend::is_typescript(&PathBuf::from("app.ts")));
        assert!(HowthBackend::is_typescript(&PathBuf::from("app.tsx")));
        assert!(HowthBackend::is_typescript(&PathBuf::from("app.mts")));
        assert!(HowthBackend::is_typescript(&PathBuf::from("app.cts")));
        assert!(!HowthBackend::is_typescript(&PathBuf::from("app.js")));
        assert!(!HowthBackend::is_typescript(&PathBuf::from("app.jsx")));
    }

    #[test]
    fn test_is_jsx() {
        assert!(HowthBackend::is_jsx(&PathBuf::from("app.jsx")));
        assert!(HowthBackend::is_jsx(&PathBuf::from("app.tsx")));
        assert!(!HowthBackend::is_jsx(&PathBuf::from("app.js")));
        assert!(!HowthBackend::is_jsx(&PathBuf::from("app.ts")));
    }

    #[test]
    fn test_transpile_empty_source() {
        let backend = HowthBackend::new();
        let spec = TranspileSpec::new("src/app.ts", "dist/app.js");
        let output = backend.transpile(&spec, "").unwrap();
        assert_eq!(output.code, "");
    }

    #[test]
    fn test_transpile_simple_js() {
        let backend = HowthBackend::new();
        let spec = TranspileSpec::new("src/app.js", "dist/app.js");
        let source = "const x = 1;";
        let output = backend.transpile(&spec, source).unwrap();
        assert!(output.code.contains("const x = 1"));
    }

    #[test]
    fn test_transpile_typescript() {
        let backend = HowthBackend::new();
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
        assert!(!output.code.contains("interface"));
        assert!(!output.code.contains(": User"));
        assert!(!output.code.contains(": string"));
        assert!(!output.code.contains(": number"));
        assert!(output.code.contains("const user"));
        assert!(output.code.contains("console.log"));
    }

    #[test]
    fn test_transpile_jsx() {
        let backend = HowthBackend::new();
        let spec = TranspileSpec::new("src/App.jsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);

        let source = r#"
            function App() {
                return <div className="app">Hello World</div>;
            }
        "#;

        let output = backend.transpile(&spec, source).unwrap();
        assert!(output.code.contains("_jsx"));
        assert!(!output.code.contains("<div"));
        assert!(output.code.contains("jsx-runtime"));
    }

    #[test]
    fn test_transpile_tsx() {
        let backend = HowthBackend::new();
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
        assert!(!output.code.contains("interface"));
        assert!(!output.code.contains(": Props"));
        assert!(!output.code.contains("<h1>"));
        assert!(output.code.contains("function Greeting"));
    }

    #[test]
    fn test_transpile_with_sourcemap() {
        let backend = HowthBackend::new();
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
        let backend = HowthBackend::new();
        assert!(backend.supports_extension("js"));
        assert!(backend.supports_extension("jsx"));
        assert!(backend.supports_extension("ts"));
        assert!(backend.supports_extension("tsx"));
        assert!(!backend.supports_extension("css"));
    }
}
