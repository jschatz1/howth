//! Resolution tracing for pkg explain command.
//!
//! Provides step-by-step traces of module resolution for debugging
//! and understanding why a specifier resolves to a particular file.

use std::path::PathBuf;

/// Schema version for the explain output format.
/// Bump when the trace structure changes incompatibly.
pub const PKG_EXPLAIN_SCHEMA_VERSION: u32 = 1;

/// A single step in the resolution trace.
#[derive(Debug, Clone)]
pub struct ResolveTraceStep {
    /// Step name (e.g., "parse_specifier", "resolve_exports", "file_exists")
    pub step: &'static str,
    /// Whether this step succeeded
    pub ok: bool,
    /// Human-readable description of what happened
    pub detail: String,
    /// File path involved in this step, if any
    pub path: Option<PathBuf>,
    /// Export/import condition used (e.g., "import", "require", "default")
    pub condition: Option<String>,
    /// Package.json exports/imports key matched
    pub key: Option<String>,
    /// Target value from exports/imports map
    pub target: Option<String>,
    /// Additional notes for this step
    pub notes: Vec<String>,
}

impl ResolveTraceStep {
    /// Create a new trace step.
    pub fn new(step: &'static str, ok: bool, detail: impl Into<String>) -> Self {
        Self {
            step,
            ok,
            detail: detail.into(),
            path: None,
            condition: None,
            key: None,
            target: None,
            notes: Vec::new(),
        }
    }

    /// Set the path for this step.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the condition for this step.
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Set the key for this step.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set the target for this step.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Add a note to this step.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Warning generated during resolution.
#[derive(Debug, Clone)]
pub struct TraceWarning {
    /// Warning code (e.g., "deprecated_main", "missing_exports")
    pub code: String,
    /// Human-readable warning message
    pub message: String,
}

impl TraceWarning {
    /// Create a new warning.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Complete resolution trace.
#[derive(Debug, Clone, Default)]
pub struct ResolveTrace {
    /// Ordered list of resolution steps
    pub steps: Vec<ResolveTraceStep>,
    /// Warnings generated during resolution
    pub warnings: Vec<TraceWarning>,
}

impl ResolveTrace {
    /// Create a new empty trace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a step to the trace.
    pub fn add_step(&mut self, step: ResolveTraceStep) {
        self.steps.push(step);
    }

    /// Add a warning to the trace.
    pub fn add_warning(&mut self, warning: TraceWarning) {
        self.warnings.push(warning);
    }

    /// Add a simple success step.
    pub fn success(&mut self, step: &'static str, detail: impl Into<String>) {
        self.steps.push(ResolveTraceStep::new(step, true, detail));
    }

    /// Add a simple failure step.
    pub fn failure(&mut self, step: &'static str, detail: impl Into<String>) {
        self.steps.push(ResolveTraceStep::new(step, false, detail));
    }
}

/// Step names used in resolution tracing.
pub mod steps {
    pub const PARSE_SPECIFIER: &str = "parse_specifier";
    pub const CLASSIFY_SPECIFIER: &str = "classify_specifier";
    pub const RESOLVE_HASH_IMPORT: &str = "resolve_hash_import";
    pub const FIND_PACKAGE_JSON: &str = "find_package_json";
    pub const READ_IMPORTS_FIELD: &str = "read_imports_field";
    pub const MATCH_IMPORTS_KEY: &str = "match_imports_key";
    pub const RESOLVE_BARE: &str = "resolve_bare";
    pub const SEARCH_NODE_MODULES: &str = "search_node_modules";
    pub const FIND_PACKAGE_DIR: &str = "find_package_dir";
    pub const READ_EXPORTS_FIELD: &str = "read_exports_field";
    pub const MATCH_EXPORTS_KEY: &str = "match_exports_key";
    pub const RESOLVE_CONDITION: &str = "resolve_condition";
    pub const RESOLVE_MAIN: &str = "resolve_main";
    pub const RESOLVE_INDEX: &str = "resolve_index";
    pub const FILE_EXISTS: &str = "file_exists";
    pub const RESOLVE_RELATIVE: &str = "resolve_relative";
    pub const RESOLVE_DIRECTORY: &str = "resolve_directory";
    pub const FINAL_PATH: &str = "final_path";
}

/// Warning codes used in resolution tracing.
pub mod warning_codes {
    pub const DEPRECATED_MAIN: &str = "deprecated_main";
    pub const MISSING_EXPORTS: &str = "missing_exports";
    pub const LEGACY_RESOLUTION: &str = "legacy_resolution";
    pub const AMBIGUOUS_EXTENSION: &str = "ambiguous_extension";
}
