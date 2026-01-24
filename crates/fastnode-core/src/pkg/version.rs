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

            // Parse as semver range
            let req = VersionReq::parse(range).map_err(|e| {
                PkgError::spec_invalid(format!("Invalid version range '{range}': {e}"))
            })?;

            // Collect and parse all versions
            let versions = get_versions(packument);
            let mut parsed: Vec<Version> = versions
                .iter()
                .filter_map(|v| Version::parse(v).ok())
                .collect();

            // Sort descending to get highest first
            parsed.sort_by(|a, b| b.cmp(a));

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
}
