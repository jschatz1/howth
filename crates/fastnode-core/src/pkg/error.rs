//! Package manager error types.

use std::fmt;
use std::io;

/// Package manager error codes.
pub mod codes {
    pub const PKG_SPEC_INVALID: &str = "PKG_SPEC_INVALID";
    pub const PKG_NOT_FOUND: &str = "PKG_NOT_FOUND";
    pub const PKG_VERSION_NOT_FOUND: &str = "PKG_VERSION_NOT_FOUND";
    pub const PKG_REGISTRY_ERROR: &str = "PKG_REGISTRY_ERROR";
    pub const PKG_DOWNLOAD_FAILED: &str = "PKG_DOWNLOAD_FAILED";
    pub const PKG_EXTRACT_FAILED: &str = "PKG_EXTRACT_FAILED";
    pub const PKG_LINK_FAILED: &str = "PKG_LINK_FAILED";
    pub const NODE_MODULES_WRITE_FAILED: &str = "NODE_MODULES_WRITE_FAILED";
    pub const PKG_CACHE_ERROR: &str = "PKG_CACHE_ERROR";

    // v1.3: --deps flag error codes
    pub const PKG_ARGS_INVALID: &str = "PKG_ARGS_INVALID";
    pub const PKG_PACKAGE_JSON_NOT_FOUND: &str = "PKG_PACKAGE_JSON_NOT_FOUND";
    pub const PKG_PACKAGE_JSON_INVALID: &str = "PKG_PACKAGE_JSON_INVALID";
    pub const PKG_DEP_RANGE_INVALID: &str = "PKG_DEP_RANGE_INVALID";
}

/// Package manager error.
#[derive(Debug)]
pub struct PkgError {
    code: &'static str,
    message: String,
}

impl PkgError {
    /// Create a new error with the given code and message.
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Get the error code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Get the error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Create a spec invalid error.
    pub fn spec_invalid(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_SPEC_INVALID, msg)
    }

    /// Create a package not found error.
    #[must_use]
    pub fn not_found(name: &str) -> Self {
        Self::new(codes::PKG_NOT_FOUND, format!("Package not found: {name}"))
    }

    /// Create a version not found error.
    #[must_use]
    pub fn version_not_found(name: &str, range: &str) -> Self {
        Self::new(
            codes::PKG_VERSION_NOT_FOUND,
            format!("No version of {name} satisfies range: {range}"),
        )
    }

    /// Create a registry error.
    pub fn registry(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_REGISTRY_ERROR, msg)
    }

    /// Create a download failed error.
    pub fn download_failed(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_DOWNLOAD_FAILED, msg)
    }

    /// Create an extraction failed error.
    pub fn extract_failed(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_EXTRACT_FAILED, msg)
    }

    /// Create a link failed error.
    pub fn link_failed(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_LINK_FAILED, msg)
    }

    /// Create a `node_modules` write failed error.
    pub fn node_modules_write_failed(msg: impl Into<String>) -> Self {
        Self::new(codes::NODE_MODULES_WRITE_FAILED, msg)
    }

    /// Create a cache error.
    pub fn cache_error(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_CACHE_ERROR, msg)
    }

    /// Create an args invalid error (v1.3).
    pub fn args_invalid(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_ARGS_INVALID, msg)
    }

    /// Create a package.json not found error (v1.3).
    #[must_use]
    pub fn package_json_not_found(path: &std::path::Path) -> Self {
        Self::new(
            codes::PKG_PACKAGE_JSON_NOT_FOUND,
            format!("package.json not found: {}", path.display()),
        )
    }

    /// Create a package.json invalid error (v1.3).
    pub fn package_json_invalid(msg: impl Into<String>) -> Self {
        Self::new(codes::PKG_PACKAGE_JSON_INVALID, msg)
    }

    /// Create a dependency range invalid error (v1.3).
    #[must_use]
    pub fn dep_range_invalid(name: &str, actual_type: &str) -> Self {
        Self::new(
            codes::PKG_DEP_RANGE_INVALID,
            format!("Invalid range for '{name}': expected string, got {actual_type}"),
        )
    }
}

impl fmt::Display for PkgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PkgError {}

impl From<io::Error> for PkgError {
    fn from(e: io::Error) -> Self {
        Self::new(codes::PKG_CACHE_ERROR, e.to_string())
    }
}

impl From<reqwest::Error> for PkgError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::new(codes::PKG_REGISTRY_ERROR, format!("Request timed out: {e}"))
        } else if e.is_connect() {
            Self::new(codes::PKG_REGISTRY_ERROR, format!("Connection failed: {e}"))
        } else {
            Self::new(codes::PKG_REGISTRY_ERROR, e.to_string())
        }
    }
}

impl From<serde_json::Error> for PkgError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(codes::PKG_REGISTRY_ERROR, format!("Invalid JSON: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_format() {
        let err = PkgError::spec_invalid("bad spec");
        assert_eq!(err.code(), codes::PKG_SPEC_INVALID);
        assert!(err.to_string().contains(codes::PKG_SPEC_INVALID));
    }

    #[test]
    fn test_error_codes_uppercase() {
        // All codes should be SCREAMING_SNAKE_CASE
        let all_codes = [
            codes::PKG_SPEC_INVALID,
            codes::PKG_NOT_FOUND,
            codes::PKG_VERSION_NOT_FOUND,
            codes::PKG_REGISTRY_ERROR,
            codes::PKG_DOWNLOAD_FAILED,
            codes::PKG_EXTRACT_FAILED,
            codes::PKG_LINK_FAILED,
            codes::NODE_MODULES_WRITE_FAILED,
            codes::PKG_CACHE_ERROR,
            // v1.3 codes
            codes::PKG_ARGS_INVALID,
            codes::PKG_PACKAGE_JSON_NOT_FOUND,
            codes::PKG_PACKAGE_JSON_INVALID,
            codes::PKG_DEP_RANGE_INVALID,
        ];

        for code in all_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Error code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }
}
