//! ES Module loader for howth runtime.
//!
//! Handles resolving and loading ES modules from the filesystem.

use deno_core::error::AnyError;
use deno_core::futures::FutureExt;
use deno_core::{ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, RequestedModuleType, ResolutionKind};
use std::path::{Path, PathBuf};

/// Howth's custom module loader.
pub struct HowthModuleLoader {
    /// Base directory for resolving relative imports.
    cwd: PathBuf,
}

impl HowthModuleLoader {
    /// Create a new module loader with the given working directory.
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }

    /// Resolve a module specifier to a file path.
    fn resolve_path(&self, specifier: &str, referrer: &ModuleSpecifier) -> Result<PathBuf, AnyError> {
        // Handle relative imports
        if specifier.starts_with("./") || specifier.starts_with("../") {
            let referrer_path = referrer.to_file_path().map_err(|_| {
                AnyError::msg(format!("Invalid referrer path: {}", referrer))
            })?;
            let referrer_dir = referrer_path.parent().unwrap_or(Path::new("."));
            let resolved = referrer_dir.join(specifier);
            return self.resolve_with_extensions(&resolved);
        }

        // Handle absolute imports
        if specifier.starts_with('/') {
            let resolved = PathBuf::from(specifier);
            return self.resolve_with_extensions(&resolved);
        }

        // Handle file:// URLs
        if specifier.starts_with("file://") {
            let url = ModuleSpecifier::parse(specifier)?;
            let path = url.to_file_path().map_err(|_| {
                AnyError::msg(format!("Invalid file URL: {}", specifier))
            })?;
            return self.resolve_with_extensions(&path);
        }

        // Bare specifiers (node_modules) - not yet supported
        Err(AnyError::msg(format!(
            "Bare specifiers not yet supported: '{}'. Use relative paths (./module) instead.",
            specifier
        )))
    }

    /// Try to resolve a path with various extensions.
    fn resolve_with_extensions(&self, path: &Path) -> Result<PathBuf, AnyError> {
        // If the path already has an extension and exists, use it
        if path.exists() && path.is_file() {
            return Ok(path.to_path_buf());
        }

        // Try common extensions
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"];
        for ext in extensions {
            let with_ext = path.with_extension(ext.trim_start_matches('.'));
            if with_ext.exists() && with_ext.is_file() {
                return Ok(with_ext);
            }

            // Also try appending extension (for paths without extension)
            let mut appended = path.as_os_str().to_owned();
            appended.push(ext);
            let appended_path = PathBuf::from(appended);
            if appended_path.exists() && appended_path.is_file() {
                return Ok(appended_path);
            }
        }

        // Try index files in directory
        if path.is_dir() {
            for ext in extensions {
                let index = path.join(format!("index{}", ext));
                if index.exists() && index.is_file() {
                    return Ok(index);
                }
            }
        }

        Err(AnyError::msg(format!(
            "Cannot find module: '{}'",
            path.display()
        )))
    }

    /// Load and optionally transpile a module.
    fn load_module(&self, path: &Path) -> Result<(String, ModuleType), AnyError> {
        let source = std::fs::read_to_string(path)?;

        // Determine if transpilation is needed
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let needs_transpile = matches!(ext.to_lowercase().as_str(), "ts" | "tsx" | "jsx" | "mts");

        let code = if needs_transpile {
            self.transpile(&source, path)?
        } else {
            source
        };

        Ok((code, ModuleType::JavaScript))
    }

    /// Transpile TypeScript/JSX to JavaScript using SWC.
    fn transpile(&self, source: &str, path: &Path) -> Result<String, AnyError> {
        use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};

        let backend = SwcBackend::new();
        let spec = TranspileSpec::new(path, "");

        let output = backend
            .transpile(&spec, source)
            .map_err(|e| AnyError::msg(format!("Transpilation failed: {}", e.message)))?;

        Ok(output.code)
    }
}

impl ModuleLoader for HowthModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, AnyError> {
        // Parse referrer as URL
        let referrer_url = if referrer == "." || referrer.is_empty() {
            // Entry point - use cwd
            ModuleSpecifier::from_file_path(&self.cwd.join("__entry__"))
                .map_err(|_| AnyError::msg("Invalid cwd"))?
        } else {
            ModuleSpecifier::parse(referrer)?
        };

        // Resolve the specifier to a path
        let resolved_path = self.resolve_path(specifier, &referrer_url)?;

        // Convert back to file:// URL
        ModuleSpecifier::from_file_path(&resolved_path)
            .map_err(|_| AnyError::msg(format!("Invalid path: {}", resolved_path.display())))
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        let specifier = module_specifier.clone();
        let cwd = self.cwd.clone();

        ModuleLoadResponse::Async(
            async move {
                let loader = HowthModuleLoader::new(cwd);

                let path = specifier.to_file_path().map_err(|_| {
                    AnyError::msg(format!("Invalid module specifier: {}", specifier))
                })?;

                let (code, module_type) = loader.load_module(&path)?;

                Ok(ModuleSource::new(
                    module_type,
                    ModuleSourceCode::String(code.into()),
                    &specifier,
                    None,
                ))
            }
            .boxed_local(),
        )
    }
}
