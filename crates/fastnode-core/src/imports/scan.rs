//! Import specifier scanner.
//!
//! Scans JavaScript/TypeScript source code for import/require specifiers
//! without full parsing.

use std::collections::HashSet;

/// Import specifier found in source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSpecCore {
    /// Specifier exactly as found.
    pub raw: String,
    /// Kind of import (one of the import kind constants).
    pub kind: String,
    /// Line number (1-indexed, best-effort).
    pub line: Option<u32>,
}

impl ImportSpecCore {
    /// Create a new import spec.
    #[must_use]
    pub fn new(raw: impl Into<String>, kind: impl Into<String>, line: Option<u32>) -> Self {
        Self {
            raw: raw.into(),
            kind: kind.into(),
            line,
        }
    }
}

/// Scan source code for import/require specifiers.
///
/// Returns discovered imports in first-appearance order, deduplicated by `raw`.
#[must_use]
pub fn scan_imports(source: &str) -> Vec<ImportSpecCore> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    let mut line_num: u32 = 1;
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Track line numbers
        if chars[i] == '\n' {
            line_num += 1;
            i += 1;
            continue;
        }

        // Skip single-line comments
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            // Skip to end of line
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Skip block comments
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line_num += 1;
                }
                i += 1;
            }
            i += 2; // Skip */
            continue;
        }

        // Check for import statement: import ... from "..."
        if matches_keyword(&chars, i, "import") {
            let start_i = i;
            i += 6;
            if let Some((spec, end)) = scan_import_statement(&chars, i, &mut line_num) {
                if !spec.is_empty() && seen.insert(spec.clone()) {
                    results.push(ImportSpecCore::new(&spec, "esm_import", Some(line_num)));
                }
                i = end;
                continue;
            }
            i = start_i + 1;
            continue;
        }

        // Check for export ... from "..."
        if matches_keyword(&chars, i, "export") {
            let start_i = i;
            i += 6;
            if let Some((spec, end)) = scan_export_from(&chars, i, &mut line_num) {
                if !spec.is_empty() && seen.insert(spec.clone()) {
                    results.push(ImportSpecCore::new(&spec, "esm_export", Some(line_num)));
                }
                i = end;
                continue;
            }
            i = start_i + 1;
            continue;
        }

        // Check for require("...")
        if matches_keyword(&chars, i, "require") {
            let start_i = i;
            i += 7;
            if let Some((spec, end)) = scan_require_call(&chars, i) {
                if !spec.is_empty() && seen.insert(spec.clone()) {
                    results.push(ImportSpecCore::new(&spec, "cjs_require", Some(line_num)));
                }
                i = end;
                continue;
            }
            i = start_i + 1;
            continue;
        }

        // Check for dynamic import: import("...")
        // This is tricky - we look for "import" followed by "("
        // But we already checked for "import" keyword above
        // So here we check for import( pattern that wasn't caught above

        i += 1;
    }

    results
}

/// Check if chars at position match a keyword (with word boundary).
fn matches_keyword(chars: &[char], pos: usize, keyword: &str) -> bool {
    let kw: Vec<char> = keyword.chars().collect();
    let len = kw.len();

    if pos + len > chars.len() {
        return false;
    }

    // Check preceding character is not alphanumeric
    if pos > 0 && (chars[pos - 1].is_alphanumeric() || chars[pos - 1] == '_') {
        return false;
    }

    // Check keyword matches
    for (j, &c) in kw.iter().enumerate() {
        if chars[pos + j] != c {
            return false;
        }
    }

    // Check following character is not alphanumeric
    if pos + len < chars.len() && (chars[pos + len].is_alphanumeric() || chars[pos + len] == '_') {
        return false;
    }

    true
}

/// Scan an import statement for the "from" specifier.
/// Returns (specifier, end position).
fn scan_import_statement(
    chars: &[char],
    start: usize,
    line_num: &mut u32,
) -> Option<(String, usize)> {
    let len = chars.len();
    let mut i = start;

    // Skip whitespace
    while i < len && chars[i].is_whitespace() {
        if chars[i] == '\n' {
            *line_num += 1;
        }
        i += 1;
    }

    // Check for dynamic import: import("...")
    if i < len && chars[i] == '(' {
        i += 1;
        // Skip whitespace
        while i < len && chars[i].is_whitespace() {
            if chars[i] == '\n' {
                *line_num += 1;
            }
            i += 1;
        }
        // Expect string
        if i < len && (chars[i] == '"' || chars[i] == '\'' || chars[i] == '`') {
            let quote = chars[i];
            i += 1;
            let spec_start = i;
            while i < len && chars[i] != quote {
                if chars[i] == '\n' {
                    *line_num += 1;
                }
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            let spec: String = chars[spec_start..i].iter().collect();
            i += 1; // Skip closing quote
            return Some((spec, i));
        }
        return None;
    }

    // Regular import: scan until we find "from"
    while i < len {
        if chars[i] == '\n' {
            *line_num += 1;
        }

        // Check for "from"
        if matches_keyword(chars, i, "from") {
            i += 4;
            // Skip whitespace
            while i < len && chars[i].is_whitespace() {
                if chars[i] == '\n' {
                    *line_num += 1;
                }
                i += 1;
            }
            // Expect string
            if i < len && (chars[i] == '"' || chars[i] == '\'' || chars[i] == '`') {
                let quote = chars[i];
                i += 1;
                let spec_start = i;
                while i < len && chars[i] != quote {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                let spec: String = chars[spec_start..i].iter().collect();
                i += 1; // Skip closing quote
                return Some((spec, i));
            }
        }

        // Direct import: import "specifier"
        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let quote = chars[i];
            i += 1;
            let spec_start = i;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            let spec: String = chars[spec_start..i].iter().collect();
            i += 1; // Skip closing quote
            return Some((spec, i));
        }

        // Stop at semicolon or newline for side-effect imports
        if chars[i] == ';' {
            break;
        }

        i += 1;

        // Safety limit to avoid infinite loops
        if i > start + 1000 {
            break;
        }
    }

    None
}

/// Scan an export ... from statement.
fn scan_export_from(chars: &[char], start: usize, line_num: &mut u32) -> Option<(String, usize)> {
    let len = chars.len();
    let mut i = start;

    // Look for "from" within reasonable distance
    let limit = (start + 500).min(len);
    while i < limit {
        if chars[i] == '\n' {
            *line_num += 1;
        }

        if matches_keyword(chars, i, "from") {
            i += 4;
            // Skip whitespace
            while i < len && chars[i].is_whitespace() {
                if chars[i] == '\n' {
                    *line_num += 1;
                }
                i += 1;
            }
            // Expect string
            if i < len && (chars[i] == '"' || chars[i] == '\'' || chars[i] == '`') {
                let quote = chars[i];
                i += 1;
                let spec_start = i;
                while i < len && chars[i] != quote {
                    if chars[i] == '\\' && i + 1 < len {
                        i += 2;
                        continue;
                    }
                    i += 1;
                }
                let spec: String = chars[spec_start..i].iter().collect();
                i += 1;
                return Some((spec, i));
            }
        }

        i += 1;
    }

    None
}

/// Scan a require("...") call.
fn scan_require_call(chars: &[char], start: usize) -> Option<(String, usize)> {
    let len = chars.len();
    let mut i = start;

    // Skip whitespace
    while i < len && chars[i].is_whitespace() && chars[i] != '\n' {
        i += 1;
    }

    // Expect (
    if i >= len || chars[i] != '(' {
        return None;
    }
    i += 1;

    // Skip whitespace
    while i < len && chars[i].is_whitespace() && chars[i] != '\n' {
        i += 1;
    }

    // Expect string
    if i >= len || (chars[i] != '"' && chars[i] != '\'' && chars[i] != '`') {
        return None;
    }

    let quote = chars[i];
    i += 1;
    let spec_start = i;

    while i < len && chars[i] != quote {
        if chars[i] == '\\' && i + 1 < len {
            i += 2;
            continue;
        }
        if chars[i] == '\n' {
            // Newline in string - likely not a valid require
            return None;
        }
        i += 1;
    }

    let spec: String = chars[spec_start..i].iter().collect();
    i += 1; // Skip closing quote

    // Skip whitespace and expect )
    while i < len && chars[i].is_whitespace() && chars[i] != '\n' {
        i += 1;
    }

    if i < len && chars[i] == ')' {
        i += 1;
        return Some((spec, i));
    }

    // Even without closing paren, we got the specifier
    Some((spec, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_esm_import_from() {
        let source = r#"import { foo } from "./dep";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dep");
        assert_eq!(imports[0].kind, "esm_import");
    }

    #[test]
    fn test_esm_import_default() {
        let source = r#"import foo from "lodash";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "lodash");
        assert_eq!(imports[0].kind, "esm_import");
    }

    #[test]
    fn test_esm_import_side_effect() {
        let source = r#"import "./polyfill";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./polyfill");
        assert_eq!(imports[0].kind, "esm_import");
    }

    #[test]
    fn test_esm_import_star() {
        let source = r#"import * as utils from "./utils";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./utils");
        assert_eq!(imports[0].kind, "esm_import");
    }

    #[test]
    fn test_dynamic_import() {
        let source = r#"const mod = await import("./dynamic");"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dynamic");
        assert_eq!(imports[0].kind, "esm_import");
    }

    #[test]
    fn test_cjs_require() {
        let source = r#"const dep = require("./dep");"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dep");
        assert_eq!(imports[0].kind, "cjs_require");
    }

    #[test]
    fn test_esm_export_from() {
        let source = r#"export { foo } from "./dep";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dep");
        assert_eq!(imports[0].kind, "esm_export");
    }

    #[test]
    fn test_export_star_from() {
        let source = r#"export * from "./dep";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dep");
        assert_eq!(imports[0].kind, "esm_export");
    }

    #[test]
    fn test_ignores_line_comment() {
        let source = r#"
// import foo from "commented"
import bar from "./real";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./real");
    }

    #[test]
    fn test_ignores_block_comment() {
        let source = r#"
/* import foo from "commented" */
import bar from "./real";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./real");
    }

    #[test]
    fn test_ignores_multiline_block_comment() {
        let source = r#"
/*
import foo from "commented"
import baz from "also-commented"
*/
import bar from "./real";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./real");
    }

    #[test]
    fn test_multiple_imports_stable_order() {
        let source = r#"
import a from "./a";
import b from "./b";
import c from "./c";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].raw, "./a");
        assert_eq!(imports[1].raw, "./b");
        assert_eq!(imports[2].raw, "./c");
    }

    #[test]
    fn test_deduplicates_imports() {
        let source = r#"
import a from "./dep";
import b from "./dep";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./dep");
    }

    #[test]
    fn test_mixed_esm_and_cjs() {
        let source = r#"
import esm from "./esm";
const cjs = require("./cjs");
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].raw, "./esm");
        assert_eq!(imports[0].kind, "esm_import");
        assert_eq!(imports[1].raw, "./cjs");
        assert_eq!(imports[1].kind, "cjs_require");
    }

    #[test]
    fn test_single_quotes() {
        let source = r"import foo from './single-quoted';";
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "./single-quoted");
    }

    #[test]
    fn test_line_numbers() {
        let source = r#"
import a from "./a";

import b from "./b";
"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 2);
        // Line 2 for "./a"
        assert_eq!(imports[0].line, Some(2));
        // Line 4 for "./b"
        assert_eq!(imports[1].line, Some(4));
    }

    #[test]
    fn test_empty_source() {
        let source = "";
        let imports = scan_imports(source);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_no_imports() {
        let source = "console.log('hello');";
        let imports = scan_imports(source);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_bare_specifier() {
        let source = r#"import React from "react";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "react");
    }

    #[test]
    fn test_scoped_package() {
        let source = r#"import test from "@scope/package";"#;
        let imports = scan_imports(source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].raw, "@scope/package");
    }
}
