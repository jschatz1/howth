use std::fmt::Write;

/// The current version, read from Cargo.toml at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Schema version for cache/data directories.
/// Bump this when changing formats that would break compatibility.
pub const SCHEMA_VERSION: u32 = 1;

/// Returns a formatted version string including build metadata if available.
#[must_use]
pub fn version_string() -> String {
    let mut s = format!("howth {VERSION}");

    if let Some(hash) = option_env!("HOWTH_BUILD_GIT_HASH") {
        let _ = write!(s, " ({hash})");
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_not_empty() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_version_string_contains_version() {
        let vs = version_string();
        assert!(vs.contains(VERSION));
        assert!(vs.starts_with("howth "));
    }

    #[test]
    fn test_schema_version_positive() {
        const { assert!(SCHEMA_VERSION > 0) };
    }
}
