//! Import rewriting for unbundled dev serving.
//!
//! Scans transformed JavaScript for import/export statements and rewrites:
//! - Bare specifiers (`react`) → `/@modules/react`
//! - Relative imports (`./App`) → `/src/App.tsx` (resolved absolute from project root)
//! - CSS imports (`./style.css`) → `/@style/src/style.css` (CSS injection module)

use std::path::{Path, PathBuf};

/// Import rewriter for dev server module serving.
pub struct ImportRewriter {
    /// Project root directory.
    root: PathBuf,
}

impl ImportRewriter {
    /// Create a new import rewriter.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Rewrite imports in transformed JavaScript source code.
    ///
    /// `module_path` is the absolute path of the module being served.
    pub fn rewrite(&self, code: &str, module_path: &Path) -> String {
        let mut result = String::with_capacity(code.len());
        let module_dir = module_path.parent().unwrap_or(Path::new("/"));

        for line in code.lines() {
            let trimmed = line.trim();

            if is_import_line(trimmed) || is_export_from_line(trimmed) {
                result.push_str(&self.rewrite_import_line(line, module_dir));
            } else if trimmed.contains("import(") {
                result.push_str(&self.rewrite_dynamic_import_line(line, module_dir));
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
    fn rewrite_import_line(&self, line: &str, module_dir: &Path) -> String {
        // Find the string literal in from 'xxx' or from "xxx"
        if let Some((before, specifier, after, quote)) = extract_from_specifier(line) {
            let rewritten = self.rewrite_specifier(&specifier, module_dir);
            format!("{}{}{}{}{}", before, quote, rewritten, quote, after)
        } else if let Some((before, specifier, after, quote)) =
            extract_side_effect_import(line)
        {
            // Side-effect import: import 'xxx'
            let rewritten = self.rewrite_specifier(&specifier, module_dir);
            format!("{}{}{}{}{}", before, quote, rewritten, quote, after)
        } else {
            line.to_string()
        }
    }

    /// Rewrite dynamic import() expressions in a line.
    fn rewrite_dynamic_import_line(&self, line: &str, module_dir: &Path) -> String {
        let mut result = String::with_capacity(line.len());
        let mut remaining = line;

        while let Some(import_start) = remaining.find("import(") {
            result.push_str(&remaining[..import_start]);
            let after_import = &remaining[import_start + 7..];

            if let Some((specifier, quote, rest)) = extract_string_from_start(after_import) {
                let rewritten = self.rewrite_specifier(&specifier, module_dir);
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
    fn rewrite_specifier(&self, specifier: &str, module_dir: &Path) -> String {
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
            return format!("/@style{}", resolved);
        }

        // Relative imports → resolved absolute from project root
        if specifier.starts_with("./") || specifier.starts_with("../") {
            return self.resolve_to_root_path(specifier, module_dir);
        }

        // Bare specifiers → /@modules/pkg
        format!("/@modules/{}", specifier)
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

/// Check if a path has a JS/TS extension.
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
}

/// Extract the `from 'specifier'` portion of an import/export line.
///
/// Returns (before_quote, specifier, after_quote, quote_char).
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
    let before = format!("{}import ", leading_ws);

    Some((before, specifier, after, quote))
}

/// Extract a string literal from the start of a string slice.
///
/// Returns (specifier, quote_char, rest_of_string).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_bare_specifier() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let code = r#"import React from 'react';
import { useState } from 'react';
import lodash from "lodash";"#;

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"));

        assert!(result.contains("from '/@modules/react'"));
        assert!(result.contains("from '/@modules/react'"));
        assert!(result.contains("from \"/@modules/lodash\""));
    }

    #[test]
    fn test_rewrite_css_import() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let code = "import './styles.css';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"));

        assert!(result.contains("/@style/"));
    }

    #[test]
    fn test_rewrite_dynamic_import() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let code = "const mod = import('lodash');";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"));

        assert!(result.contains("import('/@modules/lodash')"));
    }

    #[test]
    fn test_rewrite_export_from() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let code = "export { foo } from 'bar';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"));

        assert!(result.contains("from '/@modules/bar'"));
    }

    #[test]
    fn test_rewrite_already_rewritten() {
        let rewriter = ImportRewriter::new(PathBuf::from("/project"));
        let code = "import React from '/@modules/react';";

        let result = rewriter.rewrite(code, Path::new("/project/src/main.tsx"));

        assert!(result.contains("from '/@modules/react'"));
        // Should not double-rewrite
        assert!(!result.contains("/@modules//@modules/"));
    }
}
