//! Package.json dependency extraction.
//!
//! Provides utilities for reading and parsing dependencies from package.json files.

use super::error::{codes, PkgError};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Extracted dependencies from package.json.
#[derive(Debug, Clone, Default)]
pub struct PackageDeps {
    /// Valid dependencies as (name, range) pairs, sorted by name.
    pub deps: Vec<(String, String)>,
    /// Errors encountered during extraction.
    pub errors: Vec<PkgDepError>,
}

/// Error encountered while extracting a dependency.
#[derive(Debug, Clone)]
pub struct PkgDepError {
    /// Package name (if known).
    pub name: String,
    /// Error code.
    pub code: &'static str,
    /// Error message.
    pub message: String,
}

impl PkgDepError {
    /// Create a new dependency error.
    #[must_use]
    pub fn new(name: impl Into<String>, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            code,
            message: message.into(),
        }
    }

    /// Create an invalid range error.
    #[must_use]
    pub fn invalid_range(name: &str, actual_type: &str) -> Self {
        Self::new(
            name,
            codes::PKG_DEP_RANGE_INVALID,
            format!("expected string, got {actual_type}"),
        )
    }

    /// Create an invalid section error.
    #[must_use]
    pub fn invalid_section(section: &str, actual_type: &str) -> Self {
        Self::new(
            section,
            codes::PKG_PACKAGE_JSON_INVALID,
            format!("'{section}' must be an object, got {actual_type}"),
        )
    }
}

/// Read and extract dependencies from a package.json file.
///
/// # Arguments
/// * `package_json_path` - Path to the package.json file
/// * `include_dev` - Whether to include devDependencies
/// * `include_optional` - Whether to include optionalDependencies
///
/// # Returns
/// A `PackageDeps` containing valid dependencies and any extraction errors.
///
/// # Errors
/// Returns `PkgError` if the file cannot be read or parsed as JSON.
pub fn read_package_deps(
    package_json_path: &Path,
    include_dev: bool,
    include_optional: bool,
) -> Result<PackageDeps, PkgError> {
    // Check file exists
    if !package_json_path.exists() {
        return Err(PkgError::package_json_not_found(package_json_path));
    }

    // Read file contents
    let content = fs::read_to_string(package_json_path)
        .map_err(|e| PkgError::package_json_invalid(format!("Failed to read: {e}")))?;

    // Parse JSON
    let pkg_json: Value = serde_json::from_str(&content)
        .map_err(|e| PkgError::package_json_invalid(format!("Invalid JSON: {e}")))?;

    // Ensure root is object
    let root = pkg_json
        .as_object()
        .ok_or_else(|| PkgError::package_json_invalid("package.json must be a JSON object"))?;

    let mut result = PackageDeps::default();

    // Use HashMap for deduplication; dependencies takes precedence
    let mut deps_map: HashMap<String, String> = HashMap::new();

    // Extract optionalDependencies first (lowest precedence)
    if include_optional {
        extract_section(
            root,
            "optionalDependencies",
            &mut deps_map,
            &mut result.errors,
        );
    }

    // Extract devDependencies (middle precedence)
    if include_dev {
        extract_section(root, "devDependencies", &mut deps_map, &mut result.errors);
    }

    // Extract dependencies last (highest precedence - overwrites others)
    extract_section(root, "dependencies", &mut deps_map, &mut result.errors);

    // Convert to sorted vec
    let mut deps_vec: Vec<(String, String)> = deps_map.into_iter().collect();
    deps_vec.sort_by(|a, b| a.0.cmp(&b.0));
    result.deps = deps_vec;

    Ok(result)
}

/// Extract dependencies from a specific section of package.json.
fn extract_section(
    root: &serde_json::Map<String, Value>,
    section: &str,
    deps_map: &mut HashMap<String, String>,
    errors: &mut Vec<PkgDepError>,
) {
    let Some(section_value) = root.get(section) else {
        return; // Section doesn't exist, that's fine
    };

    let Some(section_obj) = section_value.as_object() else {
        // Section exists but is not an object
        errors.push(PkgDepError::invalid_section(
            section,
            json_type_name(section_value),
        ));
        return;
    };

    for (name, range_value) in section_obj {
        if let Some(range) = range_value.as_str() {
            // Valid string range - insert (may overwrite from lower-precedence section)
            deps_map.insert(name.clone(), range.to_string());
        } else {
            // Invalid range type
            errors.push(PkgDepError::invalid_range(
                name,
                json_type_name(range_value),
            ));
        }
    }
}

/// Get a human-readable type name for a JSON value.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_package_json(dir: &Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("package.json");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_dependencies_only() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "name": "test",
                "dependencies": {
                    "a": "^1.0.0",
                    "b": "2.0.0"
                },
                "devDependencies": {
                    "c": "^3.0.0"
                }
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert_eq!(result.deps.len(), 2);
        assert_eq!(result.deps[0], ("a".to_string(), "^1.0.0".to_string()));
        assert_eq!(result.deps[1], ("b".to_string(), "2.0.0".to_string()));
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_include_dev_dependencies() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": { "a": "^1.0.0" },
                "devDependencies": { "c": "^3.0.0" }
            }"#,
        );

        let result = read_package_deps(&path, true, false).unwrap();

        assert_eq!(result.deps.len(), 2);
        assert!(result.deps.iter().any(|(n, _)| n == "a"));
        assert!(result.deps.iter().any(|(n, _)| n == "c"));
    }

    #[test]
    fn test_include_optional_dependencies() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": { "a": "^1.0.0" },
                "optionalDependencies": { "d": "^4.0.0" }
            }"#,
        );

        let result = read_package_deps(&path, false, true).unwrap();

        assert_eq!(result.deps.len(), 2);
        assert!(result.deps.iter().any(|(n, _)| n == "a"));
        assert!(result.deps.iter().any(|(n, _)| n == "d"));
    }

    #[test]
    fn test_all_sections() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": { "a": "^1.0.0" },
                "devDependencies": { "b": "^2.0.0" },
                "optionalDependencies": { "c": "^3.0.0" }
            }"#,
        );

        let result = read_package_deps(&path, true, true).unwrap();

        assert_eq!(result.deps.len(), 3);
        // Should be sorted
        assert_eq!(result.deps[0].0, "a");
        assert_eq!(result.deps[1].0, "b");
        assert_eq!(result.deps[2].0, "c");
    }

    #[test]
    fn test_dependencies_precedence() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": { "pkg": "1.0.0" },
                "devDependencies": { "pkg": "2.0.0" },
                "optionalDependencies": { "pkg": "3.0.0" }
            }"#,
        );

        let result = read_package_deps(&path, true, true).unwrap();

        // dependencies should take precedence
        assert_eq!(result.deps.len(), 1);
        assert_eq!(result.deps[0], ("pkg".to_string(), "1.0.0".to_string()));
    }

    #[test]
    fn test_dev_over_optional_precedence() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "devDependencies": { "pkg": "2.0.0" },
                "optionalDependencies": { "pkg": "3.0.0" }
            }"#,
        );

        let result = read_package_deps(&path, true, true).unwrap();

        // devDependencies should take precedence over optionalDependencies
        assert_eq!(result.deps.len(), 1);
        assert_eq!(result.deps[0], ("pkg".to_string(), "2.0.0".to_string()));
    }

    #[test]
    fn test_sorted_output() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": {
                    "zebra": "1.0.0",
                    "apple": "1.0.0",
                    "mango": "1.0.0"
                }
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert_eq!(result.deps[0].0, "apple");
        assert_eq!(result.deps[1].0, "mango");
        assert_eq!(result.deps[2].0, "zebra");
    }

    #[test]
    fn test_invalid_range_type() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": {
                    "good": "^1.0.0",
                    "bad": 123
                }
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        // good should be extracted
        assert_eq!(result.deps.len(), 1);
        assert_eq!(result.deps[0].0, "good");

        // bad should produce an error
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].name, "bad");
        assert_eq!(result.errors[0].code, codes::PKG_DEP_RANGE_INVALID);
    }

    #[test]
    fn test_invalid_section_type() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": "not an object"
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert!(result.deps.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].code, codes::PKG_PACKAGE_JSON_INVALID);
    }

    #[test]
    fn test_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("package.json");

        let result = read_package_deps(&path, false, false);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::PKG_PACKAGE_JSON_NOT_FOUND);
    }

    #[test]
    fn test_invalid_json() {
        let dir = tempdir().unwrap();
        let path = write_package_json(dir.path(), "not valid json {{{");

        let result = read_package_deps(&path, false, false);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::PKG_PACKAGE_JSON_INVALID);
    }

    #[test]
    fn test_non_object_root() {
        let dir = tempdir().unwrap();
        let path = write_package_json(dir.path(), "[1, 2, 3]");

        let result = read_package_deps(&path, false, false);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::PKG_PACKAGE_JSON_INVALID);
    }

    #[test]
    fn test_empty_dependencies() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "name": "test",
                "dependencies": {}
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert!(result.deps.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_no_dependencies_section() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "name": "test",
                "version": "1.0.0"
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert!(result.deps.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_scoped_packages() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": {
                    "@types/node": "^20.0.0",
                    "@scope/pkg": "1.0.0",
                    "regular": "^1.0.0"
                }
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        assert_eq!(result.deps.len(), 3);
        // Scoped packages should sort correctly
        assert_eq!(result.deps[0].0, "@scope/pkg");
        assert_eq!(result.deps[1].0, "@types/node");
        assert_eq!(result.deps[2].0, "regular");
    }

    #[test]
    fn test_multiple_invalid_ranges() {
        let dir = tempdir().unwrap();
        let path = write_package_json(
            dir.path(),
            r#"{
                "dependencies": {
                    "a": 123,
                    "b": true,
                    "c": null,
                    "d": "^1.0.0"
                }
            }"#,
        );

        let result = read_package_deps(&path, false, false).unwrap();

        // Only valid one should be extracted
        assert_eq!(result.deps.len(), 1);
        assert_eq!(result.deps[0].0, "d");

        // Three errors for invalid ranges
        assert_eq!(result.errors.len(), 3);
    }
}
