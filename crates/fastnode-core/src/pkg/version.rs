//! Version resolution using semver.

use super::error::PkgError;
use super::registry::{get_latest_version, get_versions};
use semver::{Version, VersionReq};

/// Resolve a version range against a packument.
///
/// # Rules
/// - If `range` is `None`, returns `dist-tags.latest`
/// - If `range` is an exact version, returns it if present
/// - If `range` is a semver range, returns the highest satisfying version
/// - Supports OR ranges like `^1.0.0 || ^2.0.0`
///
/// # Errors
/// Returns an error if no version satisfies the range.
pub fn resolve_version(
    packument: &serde_json::Value,
    range: Option<&str>,
) -> Result<String, PkgError> {
    let name = packument
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match range {
        None => {
            // Use dist-tags.latest
            get_latest_version(packument)
                .map(String::from)
                .ok_or_else(|| {
                    PkgError::version_not_found(name, "latest (no dist-tags.latest found)")
                })
        }
        Some(range) => {
            // Try to parse as exact version first
            if let Ok(exact) = Version::parse(range) {
                let versions = get_versions(packument);
                if versions.contains(&range) {
                    return Ok(range.to_string());
                }
                // If exact version not found, try as range below
                let _ = exact; // suppress unused warning
            }

            // Collect and parse all versions (needed for matching)
            let versions = get_versions(packument);
            let mut parsed: Vec<Version> = versions
                .iter()
                .filter_map(|v| Version::parse(v).ok())
                .collect();

            // Sort descending to get highest first
            parsed.sort_by(|a, b| b.cmp(a));

            // Handle OR ranges (e.g., "^1.0.0 || ^2.0.0")
            if range.contains("||") {
                return resolve_or_range(name, range, &parsed);
            }

            // Parse as single semver range
            let req = parse_range(range)?;

            // Find first (highest) matching version
            for version in &parsed {
                if req.matches(version) {
                    return Ok(version.to_string());
                }
            }

            Err(PkgError::version_not_found(name, range))
        }
    }
}

/// Resolve an OR range like "^1.0.0 || ^2.0.0".
///
/// Returns the highest version matching any of the alternatives.
fn resolve_or_range(name: &str, range: &str, versions: &[Version]) -> Result<String, PkgError> {
    // Split by || and parse each alternative
    let alternatives: Vec<&str> = range.split("||").map(str::trim).collect();

    let mut reqs: Vec<VersionReq> = Vec::new();
    for alt in &alternatives {
        if alt.is_empty() {
            continue;
        }
        match parse_range(alt) {
            Ok(req) => reqs.push(req),
            Err(_) => {
                // Skip invalid alternatives, try others
                continue;
            }
        }
    }

    if reqs.is_empty() {
        return Err(PkgError::spec_invalid(format!(
            "Invalid version range '{range}': no valid alternatives"
        )));
    }

    // Find highest version matching any alternative
    for version in versions {
        for req in &reqs {
            if req.matches(version) {
                return Ok(version.to_string());
            }
        }
    }

    Err(PkgError::version_not_found(name, range))
}

/// Parse a single version range, handling npm-specific syntax.
///
/// Handles:
/// - Standard semver ranges: ^1.0.0, ~1.0.0, >=1.0.0, etc.
/// - Hyphen ranges: 1.0.0 - 2.0.0
/// - X-ranges: 1.x, 1.0.x, *
/// - Space-separated comparators: >= 2.1.2 < 3.0.0
fn parse_range(range: &str) -> Result<VersionReq, PkgError> {
    let range = range.trim();

    // Handle hyphen ranges: "1.0.0 - 2.0.0" -> ">=1.0.0, <=2.0.0"
    if let Some((start, end)) = parse_hyphen_range(range) {
        let converted = format!(">={start}, <={end}");
        return VersionReq::parse(&converted).map_err(|e| {
            PkgError::spec_invalid(format!("Invalid version range '{range}': {e}"))
        });
    }

    // Handle x-ranges: "1.x" -> ">=1.0.0, <2.0.0"
    if range.contains('x') || range.contains('X') || range == "*" {
        let converted = convert_x_range(range);
        return VersionReq::parse(&converted).map_err(|e| {
            PkgError::spec_invalid(format!("Invalid version range '{range}': {e}"))
        });
    }

    // Handle space-separated comparators: ">= 2.1.2 < 3.0.0" -> ">=2.1.2, <3.0.0"
    // npm allows spaces between comparators to mean AND
    let converted = convert_space_separated_comparators(range);

    // Standard semver range
    VersionReq::parse(&converted).map_err(|e| {
        PkgError::spec_invalid(format!("Invalid version range '{range}': {e}"))
    })
}

/// Parse a hyphen range like "1.0.0 - 2.0.0".
fn parse_hyphen_range(range: &str) -> Option<(String, String)> {
    // Look for " - " pattern (space-hyphen-space)
    let parts: Vec<&str> = range.split(" - ").collect();
    if parts.len() == 2 {
        let start = parts[0].trim();
        let end = parts[1].trim();
        // Validate both look like versions
        if !start.is_empty() && !end.is_empty() {
            return Some((start.to_string(), end.to_string()));
        }
    }
    None
}

/// Convert space-separated comparators to comma-separated.
///
/// npm allows: ">= 2.1.2 < 3.0.0" which means ">=2.1.2 AND <3.0.0"
/// Rust semver requires: ">=2.1.2, <3.0.0"
fn convert_space_separated_comparators(range: &str) -> String {
    let range = range.trim();

    // Regex-like parsing: split on spaces, but keep operators attached to versions
    // Comparator patterns: >=, <=, >, <, =, ~, ^, or bare version
    let mut result = String::new();
    let mut chars = range.chars().peekable();
    let mut current_token = String::new();
    let mut need_comma = false;

    while let Some(c) = chars.next() {
        match c {
            ' ' => {
                // End of current token
                if !current_token.is_empty() {
                    let trimmed = current_token.trim();
                    if !trimmed.is_empty() {
                        // Check if this looks like a complete comparator (has version number)
                        if token_has_version(trimmed) {
                            if need_comma {
                                result.push_str(", ");
                            }
                            result.push_str(trimmed);
                            need_comma = true;
                        } else {
                            // Operator without version, keep accumulating
                            if need_comma {
                                result.push_str(", ");
                                need_comma = false;
                            }
                            result.push_str(trimmed);
                        }
                    }
                    current_token.clear();
                }
            }
            _ => {
                current_token.push(c);
            }
        }
    }

    // Handle last token
    if !current_token.is_empty() {
        let trimmed = current_token.trim();
        if !trimmed.is_empty() {
            if token_has_version(trimmed) && need_comma {
                result.push_str(", ");
            }
            result.push_str(trimmed);
        }
    }

    // If nothing was parsed (no spaces), return original
    if result.is_empty() {
        return range.to_string();
    }

    result
}

/// Check if a token contains a version number (has digits).
fn token_has_version(token: &str) -> bool {
    token.chars().any(|c| c.is_ascii_digit())
}

/// Convert x-range to semver range.
fn convert_x_range(range: &str) -> String {
    let range = range.trim();

    if range == "*" || range == "x" || range == "X" {
        return ">=0.0.0".to_string();
    }

    // Replace x/X with 0 for parsing, then convert to appropriate range
    let parts: Vec<&str> = range.split('.').collect();

    match parts.as_slice() {
        [major, "x" | "X"] | [major, "*"] => {
            // "1.x" -> ">=1.0.0, <2.0.0"
            if let Ok(m) = major.parse::<u64>() {
                return format!(">={m}.0.0, <{}.0.0", m + 1);
            }
        }
        [major, minor, "x" | "X"] | [major, minor, "*"] => {
            // "1.2.x" -> ">=1.2.0, <1.3.0"
            if let (Ok(m), Ok(n)) = (major.parse::<u64>(), minor.parse::<u64>()) {
                return format!(">={m}.{n}.0, <{m}.{}.0", n + 1);
            }
        }
        _ => {}
    }

    // Fallback: just replace x with 0
    range.replace(['x', 'X'], "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packument(versions: &[&str], latest: &str) -> serde_json::Value {
        let versions_obj: serde_json::Map<String, serde_json::Value> = versions
            .iter()
            .map(|v| {
                (
                    (*v).to_string(),
                    serde_json::json!({
                        "version": v,
                        "dist": {
                            "tarball": format!("https://example.com/{v}.tgz")
                        }
                    }),
                )
            })
            .collect();

        serde_json::json!({
            "name": "test-pkg",
            "dist-tags": {
                "latest": latest
            },
            "versions": versions_obj
        })
    }

    #[test]
    fn test_resolve_latest() {
        let packument = make_packument(&["1.0.0", "2.0.0", "3.0.0"], "3.0.0");
        let version = resolve_version(&packument, None).unwrap();
        assert_eq!(version, "3.0.0");
    }

    #[test]
    fn test_resolve_exact_version() {
        let packument = make_packument(&["1.0.0", "2.0.0", "3.0.0"], "3.0.0");
        let version = resolve_version(&packument, Some("2.0.0")).unwrap();
        assert_eq!(version, "2.0.0");
    }

    #[test]
    fn test_resolve_caret_range() {
        let packument = make_packument(&["1.0.0", "1.5.0", "2.0.0", "2.5.0"], "2.5.0");
        let version = resolve_version(&packument, Some("^1.0.0")).unwrap();
        assert_eq!(version, "1.5.0");
    }

    #[test]
    fn test_resolve_tilde_range() {
        let packument = make_packument(&["1.0.0", "1.0.5", "1.1.0", "2.0.0"], "2.0.0");
        let version = resolve_version(&packument, Some("~1.0.0")).unwrap();
        assert_eq!(version, "1.0.5");
    }

    #[test]
    fn test_resolve_major_only() {
        let packument = make_packument(&["1.0.0", "1.5.0", "2.0.0", "2.5.0"], "2.5.0");
        // "2" should match ^2.0.0
        let version = resolve_version(&packument, Some("2")).unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_resolve_version_not_found() {
        let packument = make_packument(&["1.0.0", "2.0.0"], "2.0.0");
        let result = resolve_version(&packument, Some("^3.0.0"));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_prerelease() {
        let packument = make_packument(
            &["1.0.0", "2.0.0-alpha.1", "2.0.0-beta.1", "2.0.0"],
            "2.0.0",
        );
        // Caret range should not match prereleases by default
        let version = resolve_version(&packument, Some("^2.0.0")).unwrap();
        assert_eq!(version, "2.0.0");
    }

    #[test]
    fn test_invalid_range() {
        let packument = make_packument(&["1.0.0"], "1.0.0");
        let result = resolve_version(&packument, Some("not-a-range!!!"));
        assert!(result.is_err());
    }

    #[test]
    fn test_or_range_simple() {
        let packument = make_packument(&["1.0.0", "2.0.0", "3.0.0"], "3.0.0");
        // Should pick highest matching version (3.0.0 matches ^3.0.0)
        let version = resolve_version(&packument, Some("^1.0.0 || ^2.0.0 || ^3.0.0")).unwrap();
        assert_eq!(version, "3.0.0");
    }

    #[test]
    fn test_or_range_picks_highest() {
        let packument = make_packument(&["1.5.0", "2.5.0"], "2.5.0");
        // Both match, should pick highest (2.5.0)
        let version = resolve_version(&packument, Some("^1.0.0 || ^2.0.0")).unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_or_range_only_first_matches() {
        let packument = make_packument(&["1.0.0", "1.5.0"], "1.5.0");
        // Only ^1.0.0 matches, ^2.0.0 has no versions
        let version = resolve_version(&packument, Some("^1.0.0 || ^2.0.0")).unwrap();
        assert_eq!(version, "1.5.0");
    }

    #[test]
    fn test_or_range_only_second_matches() {
        let packument = make_packument(&["2.0.0", "2.5.0"], "2.5.0");
        // Only ^2.0.0 matches, ^1.0.0 has no versions
        let version = resolve_version(&packument, Some("^1.0.0 || ^2.0.0")).unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_or_range_with_spaces() {
        let packument = make_packument(&["14.0.0", "15.0.0"], "15.0.0");
        // Various spacing styles
        let version = resolve_version(&packument, Some("^14.0.0||^15.0.0")).unwrap();
        assert_eq!(version, "15.0.0");
    }

    #[test]
    fn test_or_range_complex() {
        let packument = make_packument(&["3.0.0", "4.0.0", "5.0.0"], "5.0.0");
        // Real-world example: "^3.0.0 || ^4.0.0"
        let version = resolve_version(&packument, Some("^3.0.0 || ^4.0.0")).unwrap();
        assert_eq!(version, "4.0.0");
    }

    #[test]
    fn test_or_range_no_match() {
        let packument = make_packument(&["1.0.0", "2.0.0"], "2.0.0");
        let result = resolve_version(&packument, Some("^3.0.0 || ^4.0.0"));
        assert!(result.is_err());
    }

    #[test]
    fn test_x_range() {
        let packument = make_packument(&["1.0.0", "1.5.0", "2.0.0"], "2.0.0");
        let version = resolve_version(&packument, Some("1.x")).unwrap();
        assert_eq!(version, "1.5.0");
    }

    #[test]
    fn test_hyphen_range() {
        let packument = make_packument(&["1.0.0", "1.5.0", "2.0.0", "3.0.0"], "3.0.0");
        let version = resolve_version(&packument, Some("1.0.0 - 2.0.0")).unwrap();
        assert_eq!(version, "2.0.0");
    }

    #[test]
    fn test_space_separated_comparators() {
        let packument = make_packument(&["2.0.0", "2.1.2", "2.5.0", "3.0.0"], "3.0.0");
        // ">= 2.1.2 < 3.0.0" should match 2.1.2 and 2.5.0, pick highest (2.5.0)
        let version = resolve_version(&packument, Some(">= 2.1.2 < 3.0.0")).unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_space_separated_comparators_no_spaces_around_ops() {
        let packument = make_packument(&["2.0.0", "2.1.2", "2.5.0", "3.0.0"], "3.0.0");
        // ">=2.1.2 <3.0.0" should also work
        let version = resolve_version(&packument, Some(">=2.1.2 <3.0.0")).unwrap();
        assert_eq!(version, "2.5.0");
    }

    #[test]
    fn test_space_separated_comparators_exact_boundary() {
        let packument = make_packument(&["2.1.2", "3.0.0"], "3.0.0");
        // Should include 2.1.2 but not 3.0.0
        let version = resolve_version(&packument, Some(">= 2.1.2 < 3.0.0")).unwrap();
        assert_eq!(version, "2.1.2");
    }
}
