//! Per-request module transformation pipeline for unbundled dev serving.
//!
//! Handles: resolve → load → transpile → plugin transform → import rewrite.

use crate::bundler::{LoadResult, PluginContainer, ResolveIdResult};
use crate::dev::rewrite::ImportRewriter;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Cached transform result for a module.
#[derive(Debug, Clone)]
pub struct TransformedModule {
    /// The transformed source code (ready to serve).
    pub code: String,
    /// Content-Type to serve with.
    pub content_type: &'static str,
    /// The original file path.
    pub file_path: String,
    /// Timestamp when this was last transformed.
    pub timestamp: u64,
}

/// Per-request module transformation pipeline.
///
/// Caches transformed modules and invalidates on file change.
pub struct ModuleTransformer {
    /// Project root.
    root: PathBuf,
    /// Import rewriter.
    rewriter: ImportRewriter,
    /// Module cache: file_path → TransformedModule.
    cache: RwLock<HashMap<String, TransformedModule>>,
}

impl ModuleTransformer {
    /// Create a new module transformer.
    pub fn new(root: PathBuf) -> Self {
        let rewriter = ImportRewriter::new(root.clone());
        Self {
            root,
            rewriter,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Transform a module for serving.
    ///
    /// This runs the full pipeline: resolve → load → transpile → transform → rewrite.
    /// Results are cached until invalidated.
    pub fn transform_module(
        &self,
        url_path: &str,
        plugins: &PluginContainer,
    ) -> Result<TransformedModule, ModuleTransformError> {
        // Check cache first
        if let Some(cached) = self.get_cached(url_path) {
            return Ok(cached);
        }

        // Resolve URL path to file path
        let file_path = self.resolve_url_to_file(url_path, plugins)?;
        let file_path_str = file_path.display().to_string();

        // Load the module (plugin load hook or file system)
        let source = self.load_module(&file_path_str, plugins)?;

        // Determine content type and whether to transpile
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (code, content_type) = match ext {
            "ts" | "tsx" | "jsx" | "mts" | "cts" => {
                let transpiled = self.transpile(&source, &file_path)?;
                let transformed = self.apply_plugin_transforms(&transpiled, &file_path_str, plugins)?;
                let rewritten = self.rewriter.rewrite(&transformed, &file_path);
                (rewritten, "application/javascript")
            }
            "js" | "mjs" | "cjs" => {
                let transformed = self.apply_plugin_transforms(&source, &file_path_str, plugins)?;
                let rewritten = self.rewriter.rewrite(&transformed, &file_path);
                (rewritten, "application/javascript")
            }
            "css" => {
                // CSS is served as a JS module that injects a <style> tag
                let css_module = create_css_module(&source);
                (css_module, "application/javascript")
            }
            "json" => {
                let json_module = format!("export default {};", source.trim());
                (json_module, "application/javascript")
            }
            _ => {
                return Err(ModuleTransformError {
                    message: format!("Unsupported file type: .{}", ext),
                    file: Some(file_path_str),
                });
            }
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let module = TransformedModule {
            code,
            content_type,
            file_path: file_path_str.clone(),
            timestamp,
        };

        // Cache the result
        self.cache
            .write()
            .unwrap()
            .insert(url_path.to_string(), module.clone());

        Ok(module)
    }

    /// Invalidate cache for a changed file.
    ///
    /// Returns the list of URL paths that were invalidated.
    pub fn invalidate(&self, file_path: &str) -> Vec<String> {
        let mut cache = self.cache.write().unwrap();
        let mut invalidated = Vec::new();

        // Remove all cache entries that came from this file
        cache.retain(|url_path, module| {
            if module.file_path == file_path {
                invalidated.push(url_path.clone());
                false
            } else {
                true
            }
        });

        invalidated
    }

    /// Invalidate all cache entries.
    pub fn invalidate_all(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get a cached module.
    fn get_cached(&self, url_path: &str) -> Option<TransformedModule> {
        self.cache.read().unwrap().get(url_path).cloned()
    }

    /// Resolve a URL path to an absolute file path.
    fn resolve_url_to_file(
        &self,
        url_path: &str,
        plugins: &PluginContainer,
    ) -> Result<PathBuf, ModuleTransformError> {
        // Try plugin resolve first
        if let Ok(Some(ResolveIdResult { id, external: false })) =
            plugins.resolve_id(url_path, None)
        {
            let path = PathBuf::from(&id);
            if path.exists() {
                return Ok(path);
            }
        }

        // URL path is root-relative: /src/App.tsx → {root}/src/App.tsx
        let stripped = url_path.strip_prefix('/').unwrap_or(url_path);
        let file_path = self.root.join(stripped);

        // Try exact path
        if file_path.exists() {
            return Ok(file_path);
        }

        // Try with extensions
        for ext in &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"] {
            let with_ext = PathBuf::from(format!("{}{}", file_path.display(), ext));
            if with_ext.exists() {
                return Ok(with_ext);
            }
        }

        // Try as directory with index file
        for index in &["index.ts", "index.tsx", "index.js", "index.jsx"] {
            let index_path = file_path.join(index);
            if index_path.exists() {
                return Ok(index_path);
            }
        }

        Err(ModuleTransformError {
            message: format!("Module not found: {}", url_path),
            file: None,
        })
    }

    /// Load a module's source code.
    fn load_module(
        &self,
        file_path: &str,
        plugins: &PluginContainer,
    ) -> Result<String, ModuleTransformError> {
        // Try plugin load hook first
        if let Ok(Some(LoadResult { code, .. })) = plugins.load(file_path) {
            return Ok(code);
        }

        // Fall back to file system
        std::fs::read_to_string(file_path).map_err(|e| ModuleTransformError {
            message: format!("Failed to read {}: {}", file_path, e),
            file: Some(file_path.to_string()),
        })
    }

    /// Transpile TypeScript/JSX to JavaScript using SWC.
    fn transpile(
        &self,
        source: &str,
        file_path: &Path,
    ) -> Result<String, ModuleTransformError> {
        use crate::compiler::{
            CompilerBackend, JsxRuntime, ModuleKind, SourceMapKind, SwcBackend, TranspileSpec,
        };

        let backend = SwcBackend::new();
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("js");

        let input_name = file_path.display().to_string();
        let mut spec = TranspileSpec::new(&input_name, &input_name);
        spec.module = ModuleKind::ESM;
        spec.sourcemaps = SourceMapKind::None;

        // Enable JSX for .tsx and .jsx files
        if ext == "tsx" || ext == "jsx" {
            spec.jsx_runtime = JsxRuntime::Automatic;
        }

        let output = backend.transpile(&spec, source).map_err(|e| {
            ModuleTransformError {
                message: format!("Transpile error: {}", e),
                file: Some(input_name),
            }
        })?;

        Ok(output.code)
    }

    /// Apply plugin transform hooks.
    fn apply_plugin_transforms(
        &self,
        code: &str,
        id: &str,
        plugins: &PluginContainer,
    ) -> Result<String, ModuleTransformError> {
        plugins.transform(code, id).map_err(|e| {
            ModuleTransformError {
                message: format!("Plugin transform error: {}", e),
                file: Some(id.to_string()),
            }
        })
    }
}

/// Create a CSS-as-JS module that injects a <style> tag.
fn create_css_module(css: &str) -> String {
    let escaped = css
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");

    format!(
        r#"const css = `{}`;
const style = document.createElement('style');
style.setAttribute('data-howth-css', '');
style.textContent = css;
document.head.appendChild(style);

// HMR support: remove old style on update
if (import.meta.hot) {{
  import.meta.hot.accept();
  import.meta.hot.dispose(() => {{
    style.remove();
  }});
}}

export default css;
"#,
        escaped
    )
}

/// Error during module transformation.
#[derive(Debug)]
pub struct ModuleTransformError {
    /// Human-readable error message.
    pub message: String,
    /// File path (if applicable).
    pub file: Option<String>,
}

impl std::fmt::Display for ModuleTransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{} ({})", self.message, file)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ModuleTransformError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_css_module() {
        let css = "body { color: red; }";
        let module = create_css_module(css);

        assert!(module.contains("body { color: red; }"));
        assert!(module.contains("document.createElement('style')"));
        assert!(module.contains("export default css"));
    }
}
