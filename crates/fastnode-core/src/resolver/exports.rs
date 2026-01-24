//! Package.json exports field evaluation.
//!
//! Implements Node.js-compatible exports resolution:
//! - Root exports (v1.1)
//! - Subpath exports (v1.2)
//! - Pattern exports with `*` wildcards (v1.2)
//! - Conditional exports (import/require/default)

use serde_json::Value;

/// Resolution kind determines which conditional export to prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolutionKind {
    /// ESM import (prefer "import" condition)
    Import,
    /// CJS require (prefer "require" condition)
    Require,
    /// Unknown (prefer "default", then "import", then "require")
    #[default]
    Unknown,
}

impl ResolutionKind {
    /// Convert from import kind string (from import scanner).
    #[must_use]
    pub fn from_import_kind(kind: &str) -> Self {
        match kind {
            "esm_import" | "esm_export" | "dynamic_import" => Self::Import,
            "cjs_require" => Self::Require,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for ResolutionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Import => write!(f, "import"),
            Self::Require => write!(f, "require"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Resolve exports for any subpath (unified entry point for v1.2).
///
/// - If `subpath` is `None`, resolves root exports (equivalent to `resolve_exports_root`)
/// - If `subpath` is `Some("./feature")`, resolves subpath exports
///
/// Returns the target path (starting with "./") if found, None otherwise.
#[must_use]
pub fn resolve_exports(
    pkg_json: &Value,
    subpath: Option<&str>,
    kind: ResolutionKind,
) -> Option<String> {
    match subpath {
        None => resolve_exports_root(pkg_json, kind),
        Some(sub) => {
            // First try exact subpath match
            if let Some(target) = resolve_exports_subpath(pkg_json, sub, kind) {
                return Some(target);
            }
            // Then try pattern match
            resolve_exports_pattern(pkg_json, sub, kind)
        }
    }
}

/// Resolve the root export from package.json exports field.
///
/// Returns the target path (starting with "./") if found, None otherwise.
/// Caller should fall back to "main" field if None is returned.
///
/// Supported shapes:
/// - `exports: "./path"` - string shorthand
/// - `exports: { ".": "./path" }` - explicit root
/// - `exports: { ".": { "import": "./esm.js", "require": "./cjs.js", "default": "./d.js" } }`
/// - `exports: { "import": "./esm.js", "require": "./cjs.js", "default": "./d.js" }` - root conditions
#[must_use]
pub fn resolve_exports_root(pkg_json: &Value, kind: ResolutionKind) -> Option<String> {
    let exports = pkg_json.get("exports")?;

    // Case A: exports is a string
    if let Some(s) = exports.as_str() {
        return validate_export_path(s);
    }

    // exports must be an object
    let obj = exports.as_object()?;

    // Case B: exports has "." key
    if let Some(dot) = obj.get(".") {
        return resolve_export_target(dot, kind);
    }

    // Case D: exports is a conditions object at root level (import/require/default)
    // Check if any key is a condition (not starting with ".")
    if obj.contains_key("import") || obj.contains_key("require") || obj.contains_key("default") {
        return resolve_conditions(exports, kind);
    }

    None
}

/// Resolve an exact subpath export from package.json exports field.
///
/// The `subpath` must be in `"./..."` format (e.g., `"./feature"`).
///
/// Supported shapes:
/// - `exports: { "./feature": "./dist/feature.js" }`
/// - `exports: { "./feature": { "import": "./esm.js", "require": "./cjs.js" } }`
#[must_use]
pub fn resolve_exports_subpath(
    pkg_json: &Value,
    subpath: &str,
    kind: ResolutionKind,
) -> Option<String> {
    // Subpath must start with "./"
    if !subpath.starts_with("./") {
        return None;
    }

    let exports = pkg_json.get("exports")?;

    // exports must be an object for subpath resolution
    let obj = exports.as_object()?;

    // If exports is a string or root conditions object, subpaths are not supported
    if !has_subpath_keys(obj) {
        return None;
    }

    // Look for exact subpath match
    let target = obj.get(subpath)?;
    resolve_export_target(target, kind)
}

/// Resolve a pattern export from package.json exports field.
///
/// Handles patterns like `"./*"` or `"./features/*"`.
/// The `subpath` must be in `"./..."` format.
///
/// Rules:
/// - Only one `*` in pattern key is supported
/// - Target must also contain `*` for substitution
/// - Most specific pattern wins (longest key length)
#[must_use]
pub fn resolve_exports_pattern(
    pkg_json: &Value,
    subpath: &str,
    kind: ResolutionKind,
) -> Option<String> {
    // Subpath must start with "./"
    if !subpath.starts_with("./") {
        return None;
    }

    let exports = pkg_json.get("exports")?;
    let obj = exports.as_object()?;

    // Collect matching patterns with their specificity
    let mut matches: Vec<(&str, &Value, String)> = Vec::new();

    for (key, value) in obj {
        // Skip non-pattern keys (must contain exactly one *)
        let star_count = key.chars().filter(|&c| c == '*').count();
        if star_count != 1 {
            continue;
        }

        // Key must start with "./"
        if !key.starts_with("./") {
            continue;
        }

        // Try to match the pattern
        if let Some(star_value) = match_pattern(key, subpath) {
            matches.push((key.as_str(), value, star_value));
        }
    }

    if matches.is_empty() {
        return None;
    }

    // Sort by specificity: longest key first, then lexicographic for ties
    matches.sort_by(|a, b| {
        let len_cmp = b.0.len().cmp(&a.0.len());
        if len_cmp == std::cmp::Ordering::Equal {
            a.0.cmp(b.0)
        } else {
            len_cmp
        }
    });

    // Take the most specific match
    let (_, target_value, star_value) = &matches[0];

    // Resolve the target (may be string or conditions object)
    let target_str = resolve_export_target(target_value, kind)?;

    // Substitute * in target with the matched value
    substitute_star(&target_str, star_value)
}

/// Check if exports object has subpath keys (keys starting with "./").
fn has_subpath_keys(obj: &serde_json::Map<String, Value>) -> bool {
    obj.keys().any(|k| k.starts_with("./") && k != ".")
}

/// Match a pattern key against a subpath.
///
/// Returns the `*` substitution value if matched.
/// E.g., pattern `"./features/*"` with subpath `"./features/foo"` returns `Some("foo")`.
fn match_pattern(pattern: &str, subpath: &str) -> Option<String> {
    let star_pos = pattern.find('*')?;

    let prefix = &pattern[..star_pos];
    let suffix = &pattern[star_pos + 1..];

    // Check if subpath matches the pattern
    if !subpath.starts_with(prefix) {
        return None;
    }

    if !suffix.is_empty() && !subpath.ends_with(suffix) {
        return None;
    }

    // Extract the * value
    let start = prefix.len();
    let end = subpath.len() - suffix.len();

    if start > end {
        return None;
    }

    let star_value = &subpath[start..end];

    // Reject empty star values
    if star_value.is_empty() {
        return None;
    }

    Some(star_value.to_string())
}

/// Substitute `*` in target with the star value.
///
/// Returns None if:
/// - Target doesn't contain `*`
/// - Result contains path traversal (`..`)
fn substitute_star(target: &str, star_value: &str) -> Option<String> {
    // Target must contain exactly one *
    if target.chars().filter(|&c| c == '*').count() != 1 {
        return None;
    }

    let result = target.replace('*', star_value);

    // Validate: must still start with "./"
    if !result.starts_with("./") {
        return None;
    }

    // Validate: no path traversal
    if result.split('/').any(|segment| segment == "..") {
        return None;
    }

    Some(result)
}

/// Resolve an export target which can be a string or conditions object.
fn resolve_export_target(target: &Value, kind: ResolutionKind) -> Option<String> {
    // String target
    if let Some(s) = target.as_str() {
        return validate_export_path(s);
    }

    // Conditions object
    resolve_conditions(target, kind)
}

/// Resolve conditions object: { "import": ..., "require": ..., "default": ... }
fn resolve_conditions(obj: &Value, kind: ResolutionKind) -> Option<String> {
    let conditions = obj.as_object()?;

    // Select condition based on resolution kind
    let target = match kind {
        ResolutionKind::Import => conditions
            .get("import")
            .or_else(|| conditions.get("default")),
        ResolutionKind::Require => conditions
            .get("require")
            .or_else(|| conditions.get("default")),
        ResolutionKind::Unknown => conditions
            .get("default")
            .or_else(|| conditions.get("import"))
            .or_else(|| conditions.get("require")),
    };

    let target = target?;

    // Target can be a string or nested conditions (we only support one level)
    if let Some(s) = target.as_str() {
        return validate_export_path(s);
    }

    // Try nested conditions (one more level)
    if let Some(nested) = target.as_object() {
        let nested_target = match kind {
            ResolutionKind::Import => nested.get("import").or_else(|| nested.get("default")),
            ResolutionKind::Require => nested.get("require").or_else(|| nested.get("default")),
            ResolutionKind::Unknown => nested
                .get("default")
                .or_else(|| nested.get("import"))
                .or_else(|| nested.get("require")),
        };
        if let Some(s) = nested_target.and_then(|v| v.as_str()) {
            return validate_export_path(s);
        }
    }

    None
}

/// Validate that an export path starts with "./" as required by Node.
fn validate_export_path(path: &str) -> Option<String> {
    if path.starts_with("./") {
        Some(path.to_string())
    } else {
        // Invalid export path - must be relative starting with ./
        None
    }
}

/// Resolve a #-prefixed import from package.json imports field.
///
/// Returns the target path (starting with "./") if found, None otherwise.
///
/// Supported shapes:
/// - `imports: { "#foo": "./src/foo.js" }`
/// - `imports: { "#foo": { "import": "./esm.js", "require": "./cjs.js", "default": "./d.js" } }`
#[must_use]
pub fn resolve_imports_map(pkg_json: &Value, spec: &str, kind: ResolutionKind) -> Option<String> {
    // Only handle #-prefixed specifiers
    if !spec.starts_with('#') {
        return None;
    }

    let imports = pkg_json.get("imports")?.as_object()?;

    // Exact match only (no pattern matching for v1.1)
    let target = imports.get(spec)?;

    resolve_export_target(target, kind)
}

/// Read and parse package.json, extracting relevant fields.
///
/// Returns None if file doesn't exist or is invalid JSON.
#[must_use]
pub fn read_package_json(path: &std::path::Path) -> Option<Value> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_exports_string_root() {
        let pkg = json!({
            "name": "test",
            "exports": "./dist/index.js"
        });
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Import),
            Some("./dist/index.js".to_string())
        );
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Require),
            Some("./dist/index.js".to_string())
        );
    }

    #[test]
    fn test_exports_dot_string() {
        let pkg = json!({
            "name": "test",
            "exports": { ".": "./a.js" }
        });
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Import),
            Some("./a.js".to_string())
        );
    }

    #[test]
    fn test_exports_conditions_import() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": {
                    "import": "./esm.js",
                    "require": "./cjs.cjs",
                    "default": "./d.js"
                }
            }
        });
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Import),
            Some("./esm.js".to_string())
        );
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Require),
            Some("./cjs.cjs".to_string())
        );
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Unknown),
            Some("./d.js".to_string())
        );
    }

    #[test]
    fn test_exports_conditions_at_root() {
        // Pattern: exports: { "import": ..., "require": ... } without "." key
        let pkg = json!({
            "name": "test",
            "exports": {
                "import": "./esm.js",
                "require": "./cjs.js",
                "default": "./default.js"
            }
        });
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Import),
            Some("./esm.js".to_string())
        );
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Require),
            Some("./cjs.js".to_string())
        );
    }

    #[test]
    fn test_exports_fallback_to_default() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": {
                    "default": "./fallback.js"
                }
            }
        });
        // Both Import and Require should fall back to default
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Import),
            Some("./fallback.js".to_string())
        );
        assert_eq!(
            resolve_exports_root(&pkg, ResolutionKind::Require),
            Some("./fallback.js".to_string())
        );
    }

    #[test]
    fn test_exports_invalid_path_ignored() {
        let pkg = json!({
            "name": "test",
            "exports": "https://example.com/x"
        });
        // Invalid path (not starting with ./) should return None
        assert_eq!(resolve_exports_root(&pkg, ResolutionKind::Import), None);
    }

    #[test]
    fn test_exports_absolute_path_ignored() {
        let pkg = json!({
            "name": "test",
            "exports": "/absolute/path.js"
        });
        assert_eq!(resolve_exports_root(&pkg, ResolutionKind::Import), None);
    }

    #[test]
    fn test_exports_bare_specifier_ignored() {
        let pkg = json!({
            "name": "test",
            "exports": "lodash"
        });
        assert_eq!(resolve_exports_root(&pkg, ResolutionKind::Import), None);
    }

    #[test]
    fn test_no_exports_field() {
        let pkg = json!({
            "name": "test",
            "main": "./index.js"
        });
        assert_eq!(resolve_exports_root(&pkg, ResolutionKind::Import), None);
    }

    #[test]
    fn test_imports_exact_match() {
        let pkg = json!({
            "name": "test",
            "imports": {
                "#foo": "./src/foo.js"
            }
        });
        assert_eq!(
            resolve_imports_map(&pkg, "#foo", ResolutionKind::Import),
            Some("./src/foo.js".to_string())
        );
    }

    #[test]
    fn test_imports_with_conditions() {
        let pkg = json!({
            "name": "test",
            "imports": {
                "#foo": {
                    "import": "./src/foo.mjs",
                    "require": "./src/foo.cjs",
                    "default": "./src/foo.js"
                }
            }
        });
        assert_eq!(
            resolve_imports_map(&pkg, "#foo", ResolutionKind::Import),
            Some("./src/foo.mjs".to_string())
        );
        assert_eq!(
            resolve_imports_map(&pkg, "#foo", ResolutionKind::Require),
            Some("./src/foo.cjs".to_string())
        );
    }

    #[test]
    fn test_imports_not_found() {
        let pkg = json!({
            "name": "test",
            "imports": {
                "#bar": "./bar.js"
            }
        });
        assert_eq!(
            resolve_imports_map(&pkg, "#foo", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_imports_non_hash_specifier() {
        let pkg = json!({
            "name": "test",
            "imports": {
                "#foo": "./foo.js"
            }
        });
        // Non-# specifier should return None
        assert_eq!(
            resolve_imports_map(&pkg, "foo", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_resolution_kind_from_import_kind() {
        assert_eq!(
            ResolutionKind::from_import_kind("esm_import"),
            ResolutionKind::Import
        );
        assert_eq!(
            ResolutionKind::from_import_kind("esm_export"),
            ResolutionKind::Import
        );
        assert_eq!(
            ResolutionKind::from_import_kind("dynamic_import"),
            ResolutionKind::Import
        );
        assert_eq!(
            ResolutionKind::from_import_kind("cjs_require"),
            ResolutionKind::Require
        );
        assert_eq!(
            ResolutionKind::from_import_kind("unknown"),
            ResolutionKind::Unknown
        );
    }

    // v1.2 subpath tests

    #[test]
    fn test_exports_subpath_string() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./feature": "./dist/feature.js"
            }
        });
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Import),
            Some("./dist/feature.js".to_string())
        );
    }

    #[test]
    fn test_exports_subpath_conditional() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./feature": {
                    "import": "./esm/feature.js",
                    "require": "./cjs/feature.cjs",
                    "default": "./dist/feature.js"
                }
            }
        });
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Import),
            Some("./esm/feature.js".to_string())
        );
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Require),
            Some("./cjs/feature.cjs".to_string())
        );
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Unknown),
            Some("./dist/feature.js".to_string())
        );
    }

    #[test]
    fn test_exports_subpath_not_found() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./feature": "./dist/feature.js"
            }
        });
        // "./other" is not defined in exports
        assert_eq!(
            resolve_exports_subpath(&pkg, "./other", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_exports_subpath_string_exports_not_supported() {
        // When exports is a string (root-only), subpaths are not supported
        let pkg = json!({
            "name": "test",
            "exports": "./index.js"
        });
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_exports_subpath_root_conditions_not_supported() {
        // When exports is a root conditions object, subpaths are not supported
        let pkg = json!({
            "name": "test",
            "exports": {
                "import": "./esm/index.js",
                "require": "./cjs/index.cjs"
            }
        });
        assert_eq!(
            resolve_exports_subpath(&pkg, "./feature", ResolutionKind::Import),
            None
        );
    }

    // v1.2 pattern tests

    #[test]
    fn test_exports_pattern_simple() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./*": "./dist/*.js"
            }
        });
        assert_eq!(
            resolve_exports_pattern(&pkg, "./foo", ResolutionKind::Import),
            Some("./dist/foo.js".to_string())
        );
        assert_eq!(
            resolve_exports_pattern(&pkg, "./bar", ResolutionKind::Import),
            Some("./dist/bar.js".to_string())
        );
    }

    #[test]
    fn test_exports_pattern_nested() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./features/*": "./dist/features/*.js"
            }
        });
        assert_eq!(
            resolve_exports_pattern(&pkg, "./features/auth", ResolutionKind::Import),
            Some("./dist/features/auth.js".to_string())
        );
        assert_eq!(
            resolve_exports_pattern(&pkg, "./features/user", ResolutionKind::Import),
            Some("./dist/features/user.js".to_string())
        );
    }

    #[test]
    fn test_exports_pattern_conditional() {
        let pkg = json!({
            "name": "test",
            "exports": {
                "./*": {
                    "import": "./esm/*.mjs",
                    "require": "./cjs/*.cjs"
                }
            }
        });
        assert_eq!(
            resolve_exports_pattern(&pkg, "./utils", ResolutionKind::Import),
            Some("./esm/utils.mjs".to_string())
        );
        assert_eq!(
            resolve_exports_pattern(&pkg, "./utils", ResolutionKind::Require),
            Some("./cjs/utils.cjs".to_string())
        );
    }

    #[test]
    fn test_exports_pattern_specificity() {
        // More specific pattern should win
        let pkg = json!({
            "name": "test",
            "exports": {
                "./*": "./dist/*.js",
                "./features/*": "./dist/features/*.js"
            }
        });
        // "./features/a" should match "./features/*" (more specific), not "./*"
        assert_eq!(
            resolve_exports_pattern(&pkg, "./features/auth", ResolutionKind::Import),
            Some("./dist/features/auth.js".to_string())
        );
        // "./utils" should match "./*"
        assert_eq!(
            resolve_exports_pattern(&pkg, "./utils", ResolutionKind::Import),
            Some("./dist/utils.js".to_string())
        );
    }

    #[test]
    fn test_exports_pattern_no_match() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./features/*": "./dist/features/*.js"
            }
        });
        // "./utils" doesn't match "./features/*"
        assert_eq!(
            resolve_exports_pattern(&pkg, "./utils", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_exports_pattern_invalid_target() {
        // Target must start with "./"
        let pkg = json!({
            "name": "test",
            "exports": {
                "./*": "dist/*.js"
            }
        });
        assert_eq!(
            resolve_exports_pattern(&pkg, "./foo", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_exports_pattern_path_traversal_rejected() {
        let pkg = json!({
            "name": "test",
            "exports": {
                "./*": "./*.js"
            }
        });
        // Trying to inject ".." in the star value should be rejected
        assert_eq!(
            resolve_exports_pattern(&pkg, "./../secret", ResolutionKind::Import),
            None
        );
    }

    #[test]
    fn test_exports_pattern_empty_star_rejected() {
        let pkg = json!({
            "name": "test",
            "exports": {
                "./features/*": "./dist/features/*.js"
            }
        });
        // Empty star value (trying to match "./features/") should be rejected
        assert_eq!(
            resolve_exports_pattern(&pkg, "./features/", ResolutionKind::Import),
            None
        );
    }

    // v1.2 unified resolve_exports tests

    #[test]
    fn test_resolve_exports_root() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./feature": "./dist/feature.js"
            }
        });
        // None subpath = root resolution
        assert_eq!(
            resolve_exports(&pkg, None, ResolutionKind::Import),
            Some("./index.js".to_string())
        );
    }

    #[test]
    fn test_resolve_exports_subpath() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./feature": "./dist/feature.js"
            }
        });
        assert_eq!(
            resolve_exports(&pkg, Some("./feature"), ResolutionKind::Import),
            Some("./dist/feature.js".to_string())
        );
    }

    #[test]
    fn test_resolve_exports_exact_before_pattern() {
        // Exact subpath match should take precedence over pattern
        let pkg = json!({
            "name": "test",
            "exports": {
                "./*": "./dist/*.js",
                "./special": "./special/index.js"
            }
        });
        // "./special" has exact match, should not use pattern
        assert_eq!(
            resolve_exports(&pkg, Some("./special"), ResolutionKind::Import),
            Some("./special/index.js".to_string())
        );
        // "./other" has no exact match, should use pattern
        assert_eq!(
            resolve_exports(&pkg, Some("./other"), ResolutionKind::Import),
            Some("./dist/other.js".to_string())
        );
    }

    #[test]
    fn test_resolve_exports_pattern_fallback() {
        let pkg = json!({
            "name": "test",
            "exports": {
                ".": "./index.js",
                "./*": "./dist/*.js"
            }
        });
        // No exact match for "./utils", should use pattern
        assert_eq!(
            resolve_exports(&pkg, Some("./utils"), ResolutionKind::Import),
            Some("./dist/utils.js".to_string())
        );
    }
}
