//! Transpilation specification types.
//!
//! These types capture all options for reproducible builds.
//! The `TranspileSpec` is hashable and deterministic.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// JSX runtime mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum JsxRuntime {
    /// Classic JSX transform (React.createElement).
    Classic,
    /// Automatic JSX transform (React 17+ / jsx-runtime).
    #[default]
    Automatic,
}

impl JsxRuntime {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Automatic => "automatic",
        }
    }
}

impl std::fmt::Display for JsxRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Module output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
    /// ES Modules (import/export).
    #[default]
    #[serde(alias = "esm")]
    ESM,
    /// CommonJS (require/module.exports).
    #[serde(alias = "commonjs", alias = "cjs")]
    CommonJS,
    /// Preserve original module syntax.
    Preserve,
}

impl ModuleKind {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ESM => "esm",
            Self::CommonJS => "commonjs",
            Self::Preserve => "preserve",
        }
    }
}

impl std::fmt::Display for ModuleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Source map generation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceMapKind {
    /// No source map.
    #[default]
    None,
    /// Inline source map as data URL.
    Inline,
    /// External .map file.
    External,
}

impl SourceMapKind {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Inline => "inline",
            Self::External => "external",
        }
    }
}

impl std::fmt::Display for SourceMapKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// ECMAScript target version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EsTarget {
    /// ECMAScript 2015 (ES6).
    #[serde(rename = "es2015")]
    ES2015,
    /// ECMAScript 2016.
    #[serde(rename = "es2016")]
    ES2016,
    /// ECMAScript 2017.
    #[serde(rename = "es2017")]
    ES2017,
    /// ECMAScript 2018.
    #[serde(rename = "es2018")]
    ES2018,
    /// ECMAScript 2019.
    #[serde(rename = "es2019")]
    ES2019,
    /// ECMAScript 2020.
    #[serde(rename = "es2020")]
    ES2020,
    /// ECMAScript 2021.
    #[serde(rename = "es2021")]
    ES2021,
    /// ECMAScript 2022.
    #[default]
    #[serde(rename = "es2022")]
    ES2022,
    /// ECMAScript 2023.
    #[serde(rename = "es2023")]
    ES2023,
    /// ECMAScript 2024.
    #[serde(rename = "es2024")]
    ES2024,
    /// Latest ECMAScript features.
    #[serde(rename = "esnext")]
    ESNext,
}

impl EsTarget {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ES2015 => "es2015",
            Self::ES2016 => "es2016",
            Self::ES2017 => "es2017",
            Self::ES2018 => "es2018",
            Self::ES2019 => "es2019",
            Self::ES2020 => "es2020",
            Self::ES2021 => "es2021",
            Self::ES2022 => "es2022",
            Self::ES2023 => "es2023",
            Self::ES2024 => "es2024",
            Self::ESNext => "esnext",
        }
    }
}

impl std::fmt::Display for EsTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Informational message.
    Info,
    /// Warning message.
    Warning,
    /// Error message.
    Error,
}

impl DiagnosticSeverity {
    /// Get the string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// A compiler diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Error code (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Human-readable message.
    pub message: String,
    /// Source file path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// Line number (1-indexed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Column number (1-indexed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: None,
            message: message.into(),
            file: None,
            line: None,
            column: None,
        }
    }

    /// Create a new warning diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: None,
            message: message.into(),
            file: None,
            line: None,
            column: None,
        }
    }

    /// Set the error code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set the source location.
    #[must_use]
    pub fn with_location(mut self, file: PathBuf, line: u32, column: u32) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self.column = Some(column);
        self
    }
}

/// Transpilation specification.
///
/// Captures all options needed for deterministic, reproducible transpilation.
/// This struct is serializable and hashable for cache key generation.
///
/// ## Modes
///
/// - **Single file mode** (default): `input_path` and `output_path` are specific files.
/// - **Batch mode** (`batch: true`): `input_path` is a source directory (e.g., `src`),
///   `output_path` is an output directory (e.g., `dist`). All matching files in the
///   source directory will be transpiled to the output directory preserving structure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TranspileSpec {
    /// Input path (file or directory, relative to graph cwd).
    pub input_path: PathBuf,
    /// Output path (file or directory, relative to graph cwd).
    pub output_path: PathBuf,
    /// JSX runtime mode.
    #[serde(default)]
    pub jsx_runtime: JsxRuntime,
    /// Module output format.
    #[serde(default)]
    pub module: ModuleKind,
    /// Source map generation mode.
    #[serde(default)]
    pub sourcemaps: SourceMapKind,
    /// ECMAScript target version.
    #[serde(default)]
    pub target: EsTarget,
    /// Whether to minify the output.
    #[serde(default)]
    pub minify: bool,
    /// Batch mode: transpile all files in input directory to output directory.
    #[serde(default)]
    pub batch: bool,
}

impl TranspileSpec {
    /// Create a new transpile spec for a single file with default options.
    #[must_use]
    pub fn new(input_path: impl Into<PathBuf>, output_path: impl Into<PathBuf>) -> Self {
        Self {
            input_path: input_path.into(),
            output_path: output_path.into(),
            jsx_runtime: JsxRuntime::default(),
            module: ModuleKind::default(),
            sourcemaps: SourceMapKind::default(),
            target: EsTarget::default(),
            minify: false,
            batch: false,
        }
    }

    /// Create a new batch transpile spec for a directory.
    ///
    /// All matching files (`*.ts`, `*.tsx`, `*.js`, `*.jsx`) in `input_dir`
    /// will be transpiled to `output_dir`, preserving directory structure.
    #[must_use]
    pub fn batch(input_dir: impl Into<PathBuf>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            input_path: input_dir.into(),
            output_path: output_dir.into(),
            jsx_runtime: JsxRuntime::Automatic,
            module: ModuleKind::ESM,
            sourcemaps: SourceMapKind::External,
            target: EsTarget::ES2020,
            minify: false,
            batch: true,
        }
    }

    /// Check if this is a batch transpile spec.
    #[must_use]
    pub fn is_batch(&self) -> bool {
        self.batch
    }

    /// Set the JSX runtime mode.
    #[must_use]
    pub fn with_jsx_runtime(mut self, runtime: JsxRuntime) -> Self {
        self.jsx_runtime = runtime;
        self
    }

    /// Set the module output format.
    #[must_use]
    pub fn with_module(mut self, module: ModuleKind) -> Self {
        self.module = module;
        self
    }

    /// Set the source map mode.
    #[must_use]
    pub fn with_sourcemaps(mut self, sourcemaps: SourceMapKind) -> Self {
        self.sourcemaps = sourcemaps;
        self
    }

    /// Set the ECMAScript target.
    #[must_use]
    pub fn with_target(mut self, target: EsTarget) -> Self {
        self.target = target;
        self
    }

    /// Enable or disable minification.
    #[must_use]
    pub fn with_minify(mut self, minify: bool) -> Self {
        self.minify = minify;
        self
    }

    /// Get a deterministic canonical encoding for hashing.
    ///
    /// The encoding is stable and platform-independent.
    #[must_use]
    pub fn canonical_encoding(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Input path (normalized to forward slashes)
        buf.extend_from_slice(b"input:");
        buf.extend_from_slice(self.input_path.to_string_lossy().replace('\\', "/").as_bytes());
        buf.push(0);

        // Output path
        buf.extend_from_slice(b"output:");
        buf.extend_from_slice(self.output_path.to_string_lossy().replace('\\', "/").as_bytes());
        buf.push(0);

        // JSX runtime
        buf.extend_from_slice(b"jsx:");
        buf.extend_from_slice(self.jsx_runtime.as_str().as_bytes());
        buf.push(0);

        // Module kind
        buf.extend_from_slice(b"module:");
        buf.extend_from_slice(self.module.as_str().as_bytes());
        buf.push(0);

        // Sourcemaps
        buf.extend_from_slice(b"sourcemaps:");
        buf.extend_from_slice(self.sourcemaps.as_str().as_bytes());
        buf.push(0);

        // Target
        buf.extend_from_slice(b"target:");
        buf.extend_from_slice(self.target.as_str().as_bytes());
        buf.push(0);

        // Minify
        buf.extend_from_slice(b"minify:");
        buf.extend_from_slice(if self.minify { b"true" } else { b"false" });
        buf.push(0);

        // Batch mode
        buf.extend_from_slice(b"batch:");
        buf.extend_from_slice(if self.batch { b"true" } else { b"false" });
        buf.push(0);

        buf
    }
}

impl Default for TranspileSpec {
    fn default() -> Self {
        Self {
            input_path: PathBuf::new(),
            output_path: PathBuf::new(),
            jsx_runtime: JsxRuntime::default(),
            module: ModuleKind::default(),
            sourcemaps: SourceMapKind::default(),
            target: EsTarget::default(),
            minify: false,
            batch: false,
        }
    }
}

/// Output from a successful transpilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranspileOutput {
    /// Transpiled JavaScript code.
    pub code: String,
    /// Source map (if generated).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_map: Option<String>,
    /// Compiler diagnostics (warnings, info).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl TranspileOutput {
    /// Create a new transpile output.
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            source_map: None,
            diagnostics: Vec::new(),
        }
    }

    /// Set the source map.
    #[must_use]
    pub fn with_source_map(mut self, source_map: impl Into<String>) -> Self {
        self.source_map = Some(source_map.into());
        self
    }

    /// Add diagnostics.
    #[must_use]
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsx_runtime_serialization() {
        assert_eq!(
            serde_json::to_string(&JsxRuntime::Classic).unwrap(),
            "\"classic\""
        );
        assert_eq!(
            serde_json::to_string(&JsxRuntime::Automatic).unwrap(),
            "\"automatic\""
        );
    }

    #[test]
    fn test_module_kind_serialization() {
        assert_eq!(serde_json::to_string(&ModuleKind::ESM).unwrap(), "\"esm\"");
        assert_eq!(
            serde_json::to_string(&ModuleKind::CommonJS).unwrap(),
            "\"commonjs\""
        );
        assert_eq!(
            serde_json::to_string(&ModuleKind::Preserve).unwrap(),
            "\"preserve\""
        );
    }

    #[test]
    fn test_module_kind_aliases() {
        // ESM aliases
        let esm: ModuleKind = serde_json::from_str("\"esm\"").unwrap();
        assert_eq!(esm, ModuleKind::ESM);

        // CommonJS aliases
        let cjs: ModuleKind = serde_json::from_str("\"commonjs\"").unwrap();
        assert_eq!(cjs, ModuleKind::CommonJS);
        let cjs2: ModuleKind = serde_json::from_str("\"cjs\"").unwrap();
        assert_eq!(cjs2, ModuleKind::CommonJS);
    }

    #[test]
    fn test_es_target_serialization() {
        assert_eq!(
            serde_json::to_string(&EsTarget::ES2022).unwrap(),
            "\"es2022\""
        );
        assert_eq!(
            serde_json::to_string(&EsTarget::ESNext).unwrap(),
            "\"esnext\""
        );
    }

    #[test]
    fn test_transpile_spec_default() {
        let spec = TranspileSpec::default();
        assert_eq!(spec.jsx_runtime, JsxRuntime::Automatic);
        assert_eq!(spec.module, ModuleKind::ESM);
        assert_eq!(spec.sourcemaps, SourceMapKind::None);
        assert_eq!(spec.target, EsTarget::ES2022);
        assert!(!spec.minify);
    }

    #[test]
    fn test_transpile_spec_builder() {
        let spec = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Classic)
            .with_module(ModuleKind::CommonJS)
            .with_sourcemaps(SourceMapKind::Inline)
            .with_target(EsTarget::ES2020)
            .with_minify(true);

        assert_eq!(spec.input_path, PathBuf::from("src/App.tsx"));
        assert_eq!(spec.output_path, PathBuf::from("dist/App.js"));
        assert_eq!(spec.jsx_runtime, JsxRuntime::Classic);
        assert_eq!(spec.module, ModuleKind::CommonJS);
        assert_eq!(spec.sourcemaps, SourceMapKind::Inline);
        assert_eq!(spec.target, EsTarget::ES2020);
        assert!(spec.minify);
    }

    #[test]
    fn test_transpile_spec_serialization_roundtrip() {
        let spec = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic)
            .with_module(ModuleKind::ESM)
            .with_sourcemaps(SourceMapKind::External)
            .with_target(EsTarget::ES2022)
            .with_minify(false);

        let json = serde_json::to_string(&spec).unwrap();
        let parsed: TranspileSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(spec, parsed);
    }

    #[test]
    fn test_transpile_spec_deterministic_encoding() {
        let spec1 = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);
        let spec2 = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic);

        assert_eq!(spec1.canonical_encoding(), spec2.canonical_encoding());

        let spec3 = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Classic);

        assert_ne!(spec1.canonical_encoding(), spec3.canonical_encoding());
    }

    #[test]
    fn test_diagnostic_builder() {
        let diag = Diagnostic::error("Syntax error")
            .with_code("E001")
            .with_location(PathBuf::from("src/App.tsx"), 10, 5);

        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert_eq!(diag.code, Some("E001".to_string()));
        assert_eq!(diag.message, "Syntax error");
        assert_eq!(diag.file, Some(PathBuf::from("src/App.tsx")));
        assert_eq!(diag.line, Some(10));
        assert_eq!(diag.column, Some(5));
    }

    #[test]
    fn test_transpile_output_builder() {
        let output = TranspileOutput::new("const x = 1;")
            .with_source_map("{\"version\":3}")
            .with_diagnostics(vec![Diagnostic::warning("Unused variable")]);

        assert_eq!(output.code, "const x = 1;");
        assert_eq!(output.source_map, Some("{\"version\":3}".to_string()));
        assert_eq!(output.diagnostics.len(), 1);
    }

    #[test]
    fn test_transpile_spec_json_format() {
        let spec = TranspileSpec::new("src/App.tsx", "dist/App.js")
            .with_jsx_runtime(JsxRuntime::Automatic)
            .with_module(ModuleKind::ESM)
            .with_sourcemaps(SourceMapKind::Inline)
            .with_target(EsTarget::ES2022);

        let json = serde_json::to_string_pretty(&spec).unwrap();

        // Verify it matches expected format from plan
        assert!(json.contains("\"input_path\""));
        assert!(json.contains("\"output_path\""));
        assert!(json.contains("\"jsx_runtime\""));
        assert!(json.contains("\"module\""));
        assert!(json.contains("\"sourcemaps\""));
        assert!(json.contains("\"target\""));
        assert!(json.contains("\"minify\""));
    }
}
