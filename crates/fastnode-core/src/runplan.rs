//! Execution plan builder for `fastnode run`.
//!
//! This module validates and resolves entry points without executing them.

use crate::config::Channel;
use crate::imports::ImportSpecCore;
use crate::resolver::{ResolveContext, ResolveResult, ResolverConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Schema version for `RunPlanOutput`. Bump when changing the format.
pub const RUNPLAN_SCHEMA_VERSION: u32 = 2;

/// Resolver schema version.
pub const RESOLVER_SCHEMA_VERSION: u32 = 1;

/// Stable error codes for run plan errors.
pub mod codes {
    pub const ENTRY_NOT_FOUND: &str = "ENTRY_NOT_FOUND";
    pub const ENTRY_IS_DIR: &str = "ENTRY_IS_DIR";
    pub const ENTRY_INVALID: &str = "ENTRY_INVALID";
    pub const CWD_INVALID: &str = "CWD_INVALID";
}

/// Input for building a run plan.
#[derive(Debug, Clone)]
pub struct RunPlanInput {
    /// Working directory.
    pub cwd: PathBuf,
    /// Entry point (relative or absolute).
    pub entry: PathBuf,
    /// Arguments to pass to the script.
    pub args: Vec<String>,
    /// Channel (dev, stable, nightly).
    pub channel: Channel,
}

/// Discovered import from source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportSpecOutput {
    /// Specifier exactly as found.
    pub raw: String,
    /// Kind of import (one of the import kind constants).
    pub kind: String,
    /// Line number (1-indexed, best-effort).
    pub line: Option<u32>,
}

impl From<ImportSpecCore> for ImportSpecOutput {
    fn from(spec: ImportSpecCore) -> Self {
        Self {
            raw: spec.raw,
            kind: spec.kind,
            line: spec.line,
        }
    }
}

/// Resolution result for an import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedImportOutput {
    /// Original specifier.
    pub raw: String,
    /// Resolved absolute path (if resolved).
    pub resolved: Option<String>,
    /// Status: "resolved" or "unresolved".
    pub status: String,
    /// Reason code if unresolved.
    pub reason: Option<String>,
    /// Whether served from cache.
    pub from_cache: bool,
    /// Candidate paths tried.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tried: Vec<String>,
}

impl ResolvedImportOutput {
    /// Create a resolved import.
    #[must_use]
    pub fn resolved(raw: impl Into<String>, path: impl Into<String>, from_cache: bool) -> Self {
        Self {
            raw: raw.into(),
            resolved: Some(path.into()),
            status: "resolved".to_string(),
            reason: None,
            from_cache,
            tried: Vec::new(),
        }
    }

    /// Create an unresolved import.
    #[must_use]
    pub fn unresolved(raw: impl Into<String>, reason: impl Into<String>, from_cache: bool) -> Self {
        Self {
            raw: raw.into(),
            resolved: None,
            status: "unresolved".to_string(),
            reason: Some(reason.into()),
            from_cache,
            tried: Vec::new(),
        }
    }

    /// Add tried paths.
    #[must_use]
    pub fn with_tried(mut self, tried: Vec<String>) -> Self {
        self.tried = tried;
        self
    }
}

impl From<(String, ResolveResult, bool)> for ResolvedImportOutput {
    fn from((raw, result, from_cache): (String, ResolveResult, bool)) -> Self {
        let tried = result
            .tried
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        match result.resolved {
            Some(path) => Self::resolved(raw, path.to_string_lossy().into_owned(), from_cache)
                .with_tried(tried),
            None => Self::unresolved(
                raw,
                result
                    .reason
                    .map_or("UNKNOWN".to_string(), |r| r.to_string()),
                from_cache,
            )
            .with_tried(tried),
        }
    }
}

/// Resolver info for output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolverInfoOutput {
    /// Schema version.
    pub schema_version: u32,
    /// Extension probing order.
    pub extensions: Vec<String>,
    /// Node modules resolution mode.
    pub node_modules_mode: String,
}

impl Default for ResolverInfoOutput {
    fn default() -> Self {
        Self {
            schema_version: RESOLVER_SCHEMA_VERSION,
            extensions: ResolverConfig::default()
                .extensions
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            node_modules_mode: "best_effort_v0".to_string(),
        }
    }
}

/// Output of building a run plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlanOutput {
    /// Schema version for this structure.
    pub schema_version: u32,
    /// Resolved working directory (absolute path).
    pub resolved_cwd: String,
    /// Original entry point as requested.
    pub requested_entry: String,
    /// Canonicalized absolute path if entry exists.
    pub resolved_entry: Option<String>,
    /// Entry kind: "file", "dir", "missing", or "unknown".
    pub entry_kind: String,
    /// Arguments to pass to the script.
    pub args: Vec<String>,
    /// Channel (e.g., "dev", "stable", "nightly").
    pub channel: String,
    /// Human-helpful hints (stable-ish but OK to evolve).
    pub notes: Vec<String>,
    /// Discovered imports from entry file.
    pub imports: Vec<ImportSpecOutput>,
    /// Resolution results for discovered imports.
    pub resolved_imports: Vec<ResolvedImportOutput>,
    /// Resolver configuration info.
    pub resolver: ResolverInfoOutput,
}

/// Error type for run plan building.
#[derive(Error, Debug)]
pub enum RunPlanError {
    #[error("working directory is invalid: {path}")]
    CwdInvalid { path: PathBuf },

    #[error("entry point not found: {path}")]
    EntryNotFound { path: PathBuf },

    #[error("entry point is a directory: {path}")]
    EntryIsDir { path: PathBuf },

    #[error("entry point is invalid: {path}: {reason}")]
    EntryInvalid { path: PathBuf, reason: String },
}

impl RunPlanError {
    /// Get the stable error code for this error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::CwdInvalid { .. } => codes::CWD_INVALID,
            Self::EntryNotFound { .. } => codes::ENTRY_NOT_FOUND,
            Self::EntryIsDir { .. } => codes::ENTRY_IS_DIR,
            Self::EntryInvalid { .. } => codes::ENTRY_INVALID,
        }
    }
}

/// Build an execution plan for a given entry point.
///
/// # Errors
/// Returns an error if:
/// - The working directory cannot be canonicalized
/// - The entry point does not exist
/// - The entry point is a directory
pub fn build_run_plan(input: RunPlanInput) -> Result<RunPlanOutput, RunPlanError> {
    build_run_plan_with_cache::<crate::resolver::NoCache>(input, None)
}

/// Build an execution plan with optional resolver cache.
///
/// # Errors
/// Returns an error if:
/// - The working directory cannot be canonicalized
/// - The entry point does not exist
/// - The entry point is a directory
pub fn build_run_plan_with_cache<C>(
    input: RunPlanInput,
    _cache: Option<&C>,
) -> Result<RunPlanOutput, RunPlanError>
where
    C: crate::resolver::ResolverCache,
{
    // Resolve cwd to absolute, canonicalize if possible
    let resolved_cwd = dunce::canonicalize(&input.cwd)
        .map_err(|_| RunPlanError::CwdInvalid {
            path: input.cwd.clone(),
        })?;

    // Resolve entry path
    let entry_path = if input.entry.is_absolute() {
        input.entry.clone()
    } else {
        resolved_cwd.join(&input.entry)
    };

    // Check if entry exists and determine kind
    let (resolved_entry, entry_kind, canonical_entry) = match entry_path.metadata() {
        Ok(meta) => {
            if meta.is_dir() {
                return Err(RunPlanError::EntryIsDir { path: entry_path });
            } else if meta.is_file() {
                // Try to canonicalize for the resolved path
                let canonical = dunce::canonicalize(&entry_path)
                    .unwrap_or_else(|_| entry_path.clone());
                (
                    Some(canonical.to_string_lossy().into_owned()),
                    "file",
                    Some(canonical),
                )
            } else {
                // Symlink or other - treat as unknown but present
                let canonical = dunce::canonicalize(&entry_path)
                    .unwrap_or_else(|_| entry_path.clone());
                (
                    Some(canonical.to_string_lossy().into_owned()),
                    "unknown",
                    Some(canonical),
                )
            }
        }
        Err(_) => {
            return Err(RunPlanError::EntryNotFound { path: entry_path });
        }
    };

    // Generate notes based on entry extension
    let mut notes = generate_notes(&input.entry);

    // Scan imports and resolve them
    let (imports, resolved_imports) = if let Some(ref entry_canonical) = canonical_entry {
        scan_and_resolve_imports(entry_canonical, &resolved_cwd, input.channel, &mut notes)
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(RunPlanOutput {
        schema_version: RUNPLAN_SCHEMA_VERSION,
        resolved_cwd: resolved_cwd.to_string_lossy().into_owned(),
        requested_entry: input.entry.to_string_lossy().into_owned(),
        resolved_entry,
        entry_kind: entry_kind.to_string(),
        args: input.args,
        channel: input.channel.as_str().to_string(),
        notes,
        imports,
        resolved_imports,
        resolver: ResolverInfoOutput::default(),
    })
}

/// Scan entry file for imports and resolve them.
fn scan_and_resolve_imports(
    entry_path: &Path,
    cwd: &Path,
    channel: Channel,
    notes: &mut Vec<String>,
) -> (Vec<ImportSpecOutput>, Vec<ResolvedImportOutput>) {
    use crate::imports::scan_imports;
    use crate::resolver::{resolve_v0, ResolverConfig};

    // Read entry file
    let source = match std::fs::read_to_string(entry_path) {
        Ok(s) => s,
        Err(e) => {
            notes.push(format!("Failed to read entry file: {e}"));
            return (Vec::new(), Vec::new());
        }
    };

    // Scan imports
    let import_specs = scan_imports(&source);
    let imports: Vec<ImportSpecOutput> = import_specs.iter().cloned().map(Into::into).collect();

    // Resolve each import
    let entry_dir = entry_path.parent().unwrap_or(cwd);
    let config = ResolverConfig::default();
    let ctx = ResolveContext {
        cwd: cwd.to_path_buf(),
        parent: entry_dir.to_path_buf(),
        channel: channel.as_str().to_string(),
        config: &config,
        pkg_json_cache: None,
    };

    let resolved_imports: Vec<ResolvedImportOutput> = import_specs
        .into_iter()
        .map(|spec| {
            let result = resolve_v0(&ctx, &spec.raw);
            (spec.raw, result, false).into()
        })
        .collect();

    (imports, resolved_imports)
}

/// Generate notes based on the entry point extension.
fn generate_notes(entry: &Path) -> Vec<String> {
    let ext = entry
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let note = match ext.as_str() {
        "ts" | "tsx" => "TypeScript entry (execution not implemented yet)",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript entry (execution not implemented yet)",
        _ => "Unknown entry type (execution not implemented yet)",
    };

    vec![note.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_missing_entry_returns_correct_code() {
        let dir = tempdir().unwrap();
        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("nonexistent.js"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::ENTRY_NOT_FOUND);
    }

    #[test]
    fn test_directory_entry_returns_correct_code() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("subdir"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::ENTRY_IS_DIR);
    }

    #[test]
    fn test_invalid_cwd_returns_correct_code() {
        let input = RunPlanInput {
            cwd: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            entry: PathBuf::from("main.js"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::CWD_INVALID);
    }

    #[test]
    fn test_relative_entry_resolves_against_cwd() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("main.js"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input).unwrap();
        assert!(result.resolved_entry.is_some());
        let resolved = result.resolved_entry.unwrap();
        assert!(resolved.ends_with("main.js"));
    }

    #[test]
    fn test_plan_schema_version_is_2() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("index.ts");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("index.ts"),
            args: vec![],
            channel: Channel::Dev,
        };

        let result = build_run_plan(input).unwrap();
        assert_eq!(result.schema_version, 2);
        assert_eq!(result.schema_version, RUNPLAN_SCHEMA_VERSION);
        // New fields should be present
        assert!(result.imports.is_empty()); // No imports in "// test"
        assert!(result.resolved_imports.is_empty());
        assert_eq!(result.resolver.schema_version, RESOLVER_SCHEMA_VERSION);
    }

    #[test]
    fn test_typescript_entry_gets_correct_note() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("app.ts");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("app.ts"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input).unwrap();
        assert!(!result.notes.is_empty());
        assert!(result.notes[0].contains("TypeScript"));
    }

    #[test]
    fn test_javascript_entry_gets_correct_note() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("app.js");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("app.js"),
            args: vec![],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input).unwrap();
        assert!(!result.notes.is_empty());
        assert!(result.notes[0].contains("JavaScript"));
    }

    #[test]
    fn test_args_are_preserved() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("main.js"),
            args: vec!["--port".to_string(), "3000".to_string()],
            channel: Channel::Stable,
        };

        let result = build_run_plan(input).unwrap();
        assert_eq!(result.args, vec!["--port", "3000"]);
    }

    #[test]
    fn test_channel_is_preserved() {
        let dir = tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        fs::write(&entry_path, "// test").unwrap();

        let input = RunPlanInput {
            cwd: dir.path().to_path_buf(),
            entry: PathBuf::from("main.js"),
            args: vec![],
            channel: Channel::Nightly,
        };

        let result = build_run_plan(input).unwrap();
        assert_eq!(result.channel, "nightly");
    }
}
