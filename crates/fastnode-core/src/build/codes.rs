//! Stable error codes for the build system.
//!
//! All codes are SCREAMING_SNAKE_CASE and stable across versions.

/// Working directory is invalid or does not exist.
pub const BUILD_CWD_INVALID: &str = "BUILD_CWD_INVALID";

/// No "build" script found in package.json.
pub const BUILD_SCRIPT_NOT_FOUND: &str = "BUILD_SCRIPT_NOT_FOUND";

/// Build script exited with non-zero status.
pub const BUILD_SCRIPT_FAILED: &str = "BUILD_SCRIPT_FAILED";

/// I/O error during hash computation.
pub const BUILD_HASH_IO_ERROR: &str = "BUILD_HASH_IO_ERROR";

/// Watch mode error.
pub const BUILD_WATCH_ERROR: &str = "BUILD_WATCH_ERROR";

/// Internal graph construction error.
pub const BUILD_GRAPH_INTERNAL_ERROR: &str = "BUILD_GRAPH_INTERNAL_ERROR";

/// Package.json parse error.
pub const BUILD_PACKAGE_JSON_INVALID: &str = "BUILD_PACKAGE_JSON_INVALID";

/// Package.json not found.
pub const BUILD_PACKAGE_JSON_NOT_FOUND: &str = "BUILD_PACKAGE_JSON_NOT_FOUND";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_codes_are_screaming_snake_case() {
        let codes = [
            BUILD_CWD_INVALID,
            BUILD_SCRIPT_NOT_FOUND,
            BUILD_SCRIPT_FAILED,
            BUILD_HASH_IO_ERROR,
            BUILD_WATCH_ERROR,
            BUILD_GRAPH_INTERNAL_ERROR,
            BUILD_PACKAGE_JSON_INVALID,
            BUILD_PACKAGE_JSON_NOT_FOUND,
        ];

        for code in codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }
}
