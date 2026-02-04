//! Import rewriting for unbundled dev serving.
//!
//! Scans transformed JavaScript for import/export statements and rewrites:
//! - Bare specifiers (`react`) → `/@modules/react`
//! - Relative imports (`./App`) → `/src/App.tsx` (resolved absolute from project root)
//! - CSS imports (`./style.css`) → `/@style/src/style.css` (CSS injection module)

use crate::bundler::PluginContainer;
use std::path::{Path, PathBuf};

/// Import rewriter for dev server module serving.
pub struct ImportRewriter {
    /// Project root directory.
    root: PathBuf,
}

impl ImportRewriter {
    /// Create a new import rewriter.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Rewrite imports in transformed JavaScript source code.
    ///
    /// `module_path` is the absolute path of the module being served.
    /// `plugins` is used to resolve aliases via `resolve_id` before falling
    /// through to bare specifier handling.
    #[must_use]
    pub fn rewrite(&self, code: &str, module_path: &Path, plugins: &PluginContainer) -> String {
        let mut result = String::with_capacity(code.len());
        let module_dir = module_path.parent().unwrap_or(Path::new("/"));

        for line in code.lines() {
            let trimmed = line.trim();

            if is_import_line(trimmed) || is_export_from_line(trimmed) {
                result.push_str(&self.rewrite_import_line(line, module_dir, plugins));
            } else if trimmed.contains("import(") {
                result.push_str(&self.rewrite_dynamic_import_line(line, module_dir, plugins));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }

        // Remove trailing newline if original didn't have one
        if !code.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        result
    }

    /// Rewrite a single static import/export line.
    fn rewrite_import_line(
        &self,
        line: &str,
        module_dir: &Path,
        plugins: &PluginContainer,
    ) -> String {
        // Find the string literal in from 'xxx' or from "xxx"
        if let Some((before, specifier, after, quote)) = extract_from_specifier(line) {
            let rewritten = self.rewrite_specifier(&specifier, module_dir, plugins);
            format!("{before}{quote}{rewritten}{quote}{after}")
        } else if let Some((before, specifier, after, quote)) = extract_side_effect_import(line) {
            // Side-effect import: import 'xxx'
            let rewritten = self.rewrite_specifier(&specifier, module_dir, plugins);
            format!("{before}{quote}{rewritten}{quote}{after}")
        } else {
            line.to_string()
        }
    }

    /// Rewrite dynamic `import()` expressions in a line.
    fn rewrite_dynamic_import_line(
        &self,
        line: &str,
        module_dir: &Path,
        plugins: &PluginContainer,
    ) -> String {
        let mut result = String::with_capacity(line.len());
        let mut remaining = line;

        while let Some(import_start) = remaining.find("import(") {
            result.push_str(&remaining[..import_start]);
            let after_import = &remaining[import_start + 7..];

            if let Some((specifier, quote, rest)) = extract_string_from_start(after_import) {
                let rewritten = self.rewrite_specifier(&specifier, module_dir, plugins);
                result.push_str("import(");
                result.push(quote);
                result.push_str(&rewritten);
                result.push(quote);
                remaining = rest;
            } else {
                // Not a string literal, leave as-is
                result.push_str("import(");
                remaining = after_import;
            }
        }

        result.push_str(remaining);
        result
    }

    /// Rewrite a single import specifier.
    fn rewrite_specifier(
        &self,
        specifier: &str,
        module_dir: &Path,
        plugins: &PluginContainer,
    ) -> String {
        // Virtual modules - leave as-is
        if specifier.starts_with('\0') {
            return specifier.to_string();
        }

        // Already rewritten (starts with /@modules/ or /@style/)
        if specifier.starts_with("/@modules/") || specifier.starts_with("/@style/") {
            return specifier.to_string();
        }

        // Absolute path from root (starts with /)
        if specifier.starts_with('/') {
            return specifier.to_string();
        }

        // CSS imports → /@style/ prefix
        if specifier.ends_with(".css") {
            let resolved = self.resolve_to_root_path(specifier, module_dir);
            return format!("/@style{resolved}");
        }

        // Asset imports → append ?import so the server returns a JS module
        if is_asset_extension(specifier) {
            let resolved = if specifier.starts_with("./") || specifier.starts_with("../") {
                self.resolve_to_root_path(specifier, module_dir)
            } else if specifier.starts_with('/') {
                specifier.to_string()
            } else {
                specifier.to_string()
            };
            return format!("{resolved}?import");
        }

        // Relative imports → resolved absolute from project root
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_to_root_path(specifier, module_dir);
        }

        // Try plugin resolve_id (handles aliases like @components/Button → ./src/components/Button)
        if let Ok(Some(resolved)) =
            plugins.resolve_id(specifier, Some(&module_dir.display().to_string()))
        {
            if !resolved.external {
                let resolved_path =
                    if resolved.id.starts_with("./") || resolved.id.starts_with("../") {
                        // Relative path from root (e.g., ./src/components/Button)
                        self.root.join(&resolved.id)
                    } else {
                        PathBuf::from(&resolved.id)
                    };
                if let Ok(rel) = resolved_path.strip_prefix(&self.root) {
                    let rel_str = format!("/{}", rel.display());
                    return rel_str;
                }
                // If the resolved path is absolute but outside root, use it directly
                if resolved_path.is_absolute() {
                    return resolved_path.display().to_string();
                }
            }
        }

        // Bare specifiers → /@modules/pkg
        format!("/@modules/{specifier}")
    }

    /// Resolve a relative import to an absolute path from the project root.
    ///
    /// E.g., `./App` from `/project/src/main.tsx` → `/src/App.tsx`
    fn resolve_to_root_path(&self, specifier: &str, module_dir: &Path) -> String {
        let resolved = module_dir.join(specifier);

        // Try to canonicalize, falling back to the joined path
        let absolute = resolved.canonicalize().unwrap_or(resolved);

        // Strip project root to get root-relative path
        if let Ok(relative) = absolute.strip_prefix(&self.root) {
            let rel_str = format!("/{}", relative.display());
            // Try adding extension if not present
            if !has_js_extension(&rel_str) {
                // Try common extensions
                for ext in &[".ts", ".tsx", ".js", ".jsx", ".mjs"] {
                    let with_ext = format!("{}{}", absolute.display(), ext);
                    if Path::new(&with_ext).exists() {
                        if let Ok(rel) = Path::new(&with_ext).strip_prefix(&self.root) {
                            return format!("/{}", rel.display());
                        }
                    }
                }
                // Try index files
                for index in &["index.ts", "index.tsx", "index.js", "index.jsx"] {
                    let index_path = absolute.join(index);
                    if index_path.exists() {
                        if let Ok(rel) = index_path.strip_prefix(&self.root) {
                            return format!("/{}", rel.display());
                        }
                    }
                }
            }
            return rel_str;
        }

        // Fallback: return the specifier as-is
        specifier.to_string()
    }
}

/// Check if a line is a static import statement.
fn is_import_line(trimmed: &str) -> bool {
    trimmed.starts_with("import ")
        && (trimmed.contains(" from ") || trimmed.contains('\'') || trimmed.contains('"'))
}

/// Check if a line is an `export ... from` re-export.
fn is_export_from_line(trimmed: &str) -> bool {
    trimmed.starts_with("export ") && trimmed.contains(" from ")
}

/// Check if a path has a JS/TS/CSS/JSON/asset extension (i.e., should not have extensions appended).
fn has_js_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
        || lower.ends_with(".css")
        || lower.ends_with(".json")
        || is_asset_extension(&lower)
}

/// Known asset file extensions that should be served as `export default url` when imported.
const ASSET_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".webp", ".avif", ".mp4", ".webm", ".ogg",
    ".mp3", ".wav", ".flac", ".aac", ".woff", ".woff2", ".eot", ".ttf", ".otf", ".wasm", ".pdf",
];

/// Check if a specifier ends with a known asset extension.
fn is_asset_extension(specifier: &str) -> bool {
    let lower = specifier.to_lowercase();
    ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Check if a specifier ends with a known asset extension (public).
#[must_use]
pub fn is_asset_import(specifier: &str) -> bool {
    is_asset_extension(specifier)
}

/// Extract the `from 'specifier'` portion of an import/export line.
///
/// Returns (`before_quote`, specifier, `after_quote`, `quote_char`).
fn extract_from_specifier(line: &str) -> Option<(String, String, String, char)> {
    let from_idx = line.find(" from ")?;
    let after_from = &line[from_idx + 6..];
    let after_from_trimmed = after_from.trim_start();
    let quote = after_from_trimmed.chars().next()?;

    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &after_from_trimmed[1..];
    let end_idx = inner.find(quote)?;
    let specifier = inner[..end_idx].to_string();

    // Reconstruct before and after
    let before = format!("{} from ", &line[..from_idx]);
    // Everything after the closing quote
    let after_specifier = &inner[end_idx + 1..];

    Some((before, specifier, after_specifier.to_string(), quote))
}

/// Extract specifier from a side-effect import: `import 'xxx'` or `import "xxx"`.
fn extract_side_effect_import(line: &str) -> Option<(String, String, String, char)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }

    let after_import = &trimmed[7..].trim_start();
    let quote = after_import.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &after_import[1..];
    let end_idx = inner.find(quote)?;
    let specifier = inner[..end_idx].to_string();
    let after = inner[end_idx + 1..].to_string();

    // Preserve leading whitespace from original line
    let leading_ws: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let before = format!("{leading_ws}import ");

    Some((before, specifier, after, quote))
}

/// Extract a string literal from the start of a string slice.
///
/// Returns (specifier, `quote_char`, `rest_of_string`).
fn extract_string_from_start(s: &str) -> Option<(String, char, &str)> {
    let trimmed = s.trim_start();
    let quote = trimmed.chars().next()?;

    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &trimmed[1..];
    let end_idx = inner.find(quote)?;
    let specifier = inner[..end_idx].to_string();
    let rest = &inner[end_idx + 1..];

    Some((specifier, quote, rest))
}

/// Extract all import URLs from rewritten JavaScript code.
///
/// Scans for static imports (`import ... from '...'`), side-effect imports
/// (`import '...'`), re-exports (`export ... from '...'`), and dynamic
/// imports (`import('...')`). Returns deduplicated URL paths.
#[must_use]
pub fn extract_import_urls(code: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in code.lines() {
        let trimmed = line.trim();

        // Static imports and re-exports
        if is_import_line(trimmed) || is_export_from_line(trimmed) {
            if let Some((_, specifier, _, _)) = extract_from_specifier(line) {
                if !specifier.starts_with("/@modules/")
                    && !specifier.starts_with('\0')
                    && seen.insert(specifier.clone())
                {
                    urls.push(specifier);
                }
            } else if let Some((_, specifier, _, _)) = extract_side_effect_import(line) {
                if !specifier.starts_with("/@modules/")
                    && !specifier.starts_with('\0')
                    && seen.insert(specifier.clone())
                {
                    urls.push(specifier);
                }
            }
        }

        // Dynamic imports
        if trimmed.contains("import(") {
            let mut remaining = trimmed;
            while let Some(idx) = remaining.find("import(") {
                let after = &remaining[idx + 7..];
                if let Some((specifier, _, rest)) = extract_string_from_start(after) {
                    if !specifier.starts_with("/@modules/")
                        && !specifier.starts_with('\0')
                        && seen.insert(specifier.clone())
                    {
                        urls.push(specifier);
                    }
                    remaining = rest;
                } else {
                    break;
                }
            }
        }
    }

    urls
}

/// Check if transformed code contains a self-accepting HMR call.
///
/// Uses a simple heuristic: scans for `.hot.accept(` calls and checks whether
/// the first argument is a string or array (dep-accepting) or not (self-accepting).
///
/// This is a best-effort detection at serve time. The authoritative source is
/// the client runtime, which sends an `accept` WebSocket message back to the
/// server when `import.meta.hot.accept()` actually executes. This function
/// provides an early hint so the module graph has edges before the browser
/// even loads the module.
///
/// Vite uses `es-module-lexer` for AST-level detection. We use line scanning
/// which is fast but can have false positives (e.g., inside comments/strings).
/// For howth's use case this is acceptable — a false positive just means we
/// attempt HMR when we'd otherwise do a full reload, and the worst outcome is
/// the client falls back to reload anyway.
#[must_use]
pub fn is_self_accepting_module(code: &str) -> bool {
    for line in code.lines() {
        let trimmed = line.trim();

        // Skip obvious comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }

        if !trimmed.contains(".hot.accept") && !trimmed.contains(".hot?.accept") {
            continue;
        }

        for pattern in &[".hot.accept(", ".hot?.accept("] {
            if let Some(idx) = trimmed.find(pattern) {
                let after = &trimmed[idx + pattern.len()..];
                let after = after.trim();
                // Dep-accepting starts with a string or array literal
                if after.starts_with('\'') || after.starts_with('"') || after.starts_with('[') {
                    continue;
                }
                // Everything else is self-accepting:
                // accept(), accept(cb), accept(() => ...), accept(mod => ...)
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_plugins() -> PluginContainer {
        PluginContainer::new(PathBuf::from("/project"))
    }

    #[test]
    fn test_rewrite_bare_specifier() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = r#"import React from 'react';
import { useState } from 'react';
import lodash from "lodash";"#;

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        assert!(result.contains("from '/@modules/react'"));
        assert!(result.contains("from '/@modules/react'"));
        assert!(result.contains("from \"/@modules/lodash\""));
    }

    #[test]
    fn test_rewrite_css_import() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import './styles.css';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        assert!(result.contains("/@style/"));
    }

    #[test]
    fn test_rewrite_dynamic_import() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "const mod = import('lodash');";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        assert!(result.contains("import('/@modules/lodash')"));
    }

    #[test]
    fn test_rewrite_export_from() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "export { foo } from 'bar';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        assert!(result.contains("from '/@modules/bar'"));
    }

    #[test]
    fn test_extract_import_urls() {
        let code = r#"import { useState } from '/@modules/react';
import App from '/src/App.tsx';
import '/src/styles.css';
export { foo } from '/src/utils.ts';
const lazy = import('/src/Lazy.tsx');"#;

        let urls = extract_import_urls(code);
        // Should only include local modules, not /@modules/
        assert!(urls.contains(&"/src/App.tsx".to_string()));
        assert!(urls.contains(&"/src/styles.css".to_string()));
        assert!(urls.contains(&"/src/utils.ts".to_string()));
        assert!(urls.contains(&"/src/Lazy.tsx".to_string()));
        assert!(!urls.iter().any(|u| u.contains("/@modules/")));
    }

    #[test]
    fn test_is_self_accepting() {
        assert!(is_self_accepting_module("import.meta.hot.accept();"));
        assert!(is_self_accepting_module(
            "import.meta.hot.accept(mod => { });"
        ));
        assert!(is_self_accepting_module(
            "if (import.meta.hot) { import.meta.hot.accept(); }"
        ));
        assert!(!is_self_accepting_module(
            "import.meta.hot.accept('./dep', cb);"
        ));
        assert!(!is_self_accepting_module(
            "import.meta.hot.accept(['./a', './b'], cb);"
        ));
        assert!(!is_self_accepting_module("const x = 42;"));
    }

    // ========================================================================
    // Asset import tests
    // ========================================================================

    /// 1: Asset imports get ?import appended.
    #[test]
    fn test_rewrite_asset_import_png() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import logo from './logo.png';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);
        assert!(
            result.contains("?import"),
            "Expected ?import suffix, got: {}",
            result
        );
    }

    /// 1: SVG asset import.
    #[test]
    fn test_rewrite_asset_import_svg() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import icon from './icon.svg';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);
        assert!(
            result.contains("?import"),
            "Expected ?import suffix, got: {}",
            result
        );
    }

    /// 1: Font asset import.
    #[test]
    fn test_rewrite_asset_import_woff2() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import font from './font.woff2';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);
        assert!(
            result.contains("?import"),
            "Expected ?import suffix, got: {}",
            result
        );
    }

    /// 0: Non-asset extensions should NOT get ?import.
    #[test]
    fn test_rewrite_non_asset_no_import_suffix() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import App from '/@modules/react';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);
        assert!(
            !result.contains("?import"),
            "Should not have ?import: {}",
            result
        );
    }

    /// -1: CSS imports should use /@style, not ?import.
    #[test]
    fn test_rewrite_css_not_asset_import() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import './styles.css';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);
        assert!(
            result.contains("/@style/"),
            "CSS should use /@style, got: {}",
            result
        );
        assert!(
            !result.contains("?import"),
            "CSS should not have ?import: {}",
            result
        );
    }

    /// 1: is_asset_extension detects various asset types.
    #[test]
    fn test_is_asset_extension() {
        assert!(is_asset_extension("logo.png"));
        assert!(is_asset_extension("photo.jpg"));
        assert!(is_asset_extension("image.jpeg"));
        assert!(is_asset_extension("anim.gif"));
        assert!(is_asset_extension("icon.svg"));
        assert!(is_asset_extension("favicon.ico"));
        assert!(is_asset_extension("pic.webp"));
        assert!(is_asset_extension("font.woff"));
        assert!(is_asset_extension("font.woff2"));
        assert!(is_asset_extension("font.ttf"));
        assert!(is_asset_extension("module.wasm"));
        assert!(is_asset_extension("video.mp4"));
        assert!(is_asset_extension("audio.mp3"));
        assert!(is_asset_extension("doc.pdf"));
    }

    /// 0: Non-asset extensions.
    #[test]
    fn test_is_not_asset_extension() {
        assert!(!is_asset_extension("file.ts"));
        assert!(!is_asset_extension("file.tsx"));
        assert!(!is_asset_extension("file.js"));
        assert!(!is_asset_extension("file.css"));
        assert!(!is_asset_extension("file.json"));
        assert!(!is_asset_extension("file.html"));
        assert!(!is_asset_extension("file"));
    }

    /// -1: Case-insensitive asset detection.
    #[test]
    fn test_is_asset_extension_case_insensitive() {
        assert!(is_asset_extension("LOGO.PNG"));
        assert!(is_asset_extension("Photo.JPG"));
        assert!(is_asset_extension("icon.SVG"));
    }

    #[test]
    fn test_rewrite_already_rewritten() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let plugins = empty_plugins();
        let code = "import React from '/@modules/react';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        assert!(result.contains("from '/@modules/react'"));
        // Should not double-rewrite
        assert!(!result.contains("/@modules//@modules/"));
    }

    /// 1: Alias resolved via plugin resolve_id.
    #[test]
    fn test_rewrite_alias_via_plugin() {
        use crate::bundler::AliasPlugin;
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let mut plugins = PluginContainer::new(PathBuf::from("/project"));
        plugins.add(Box::new(
            AliasPlugin::new().alias("@components", "/project/src/components"),
        ));

        let code = "import { Button } from '@components/Button';";
        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"), &plugins);

        // Should resolve to root-relative path, not /@modules/
        assert!(
            !result.contains("/@modules/"),
            "Alias should not be treated as bare specifier, got: {}",
            result
        );
        assert!(
            result.contains("/src/components/Button"),
            "Should resolve to src/components/Button, got: {}",
            result
        );
    }
}
