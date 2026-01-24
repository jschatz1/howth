//! Package spec parsing.
//!
//! Parses package specifications like:
//! - `react`
//! - `react@18.2.0`
//! - `react@^18.0.0`
//! - `@types/node`
//! - `@types/node@^20`

use super::error::PkgError;

/// A parsed package specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSpec {
    /// Full package name (e.g., "@scope/name" or "name").
    pub name: String,
    /// Scope without the @ prefix, if scoped.
    pub scope: Option<String>,
    /// Version range or tag (None means latest).
    pub range: Option<String>,
}

impl PackageSpec {
    /// Parse a package specification string.
    ///
    /// # Errors
    /// Returns an error if the spec is invalid.
    pub fn parse(input: &str) -> Result<Self, PkgError> {
        let input = input.trim();

        if input.is_empty() {
            return Err(PkgError::spec_invalid("Empty package spec"));
        }

        // Check for scoped package
        if input.starts_with('@') {
            Self::parse_scoped(input)
        } else {
            Self::parse_unscoped(input)
        }
    }

    fn parse_scoped(input: &str) -> Result<Self, PkgError> {
        // Must have at least @scope/name
        let Some(slash_pos) = input.find('/') else {
            return Err(PkgError::spec_invalid(format!(
                "Invalid scoped package: missing '/' in '{input}'"
            )));
        };

        if slash_pos == 1 {
            return Err(PkgError::spec_invalid(format!(
                "Invalid scoped package: empty scope in '{input}'"
            )));
        }

        let scope = &input[1..slash_pos];
        let after_slash = &input[slash_pos + 1..];

        if after_slash.is_empty() {
            return Err(PkgError::spec_invalid(format!(
                "Invalid scoped package: empty name in '{input}'"
            )));
        }

        // Check for version range after the slash part
        // The version delimiter is @ after the package name part
        if let Some(at_pos) = after_slash.find('@') {
            let pkg_name = &after_slash[..at_pos];
            let range = &after_slash[at_pos + 1..];

            if pkg_name.is_empty() {
                return Err(PkgError::spec_invalid(format!(
                    "Invalid scoped package: empty name in '{input}'"
                )));
            }

            if range.is_empty() {
                return Err(PkgError::spec_invalid(format!(
                    "Invalid package spec: empty version range in '{input}'"
                )));
            }

            Ok(Self {
                name: format!("@{scope}/{pkg_name}"),
                scope: Some(scope.to_string()),
                range: Some(range.to_string()),
            })
        } else {
            // No version range
            Ok(Self {
                name: input.to_string(),
                scope: Some(scope.to_string()),
                range: None,
            })
        }
    }

    fn parse_unscoped(input: &str) -> Result<Self, PkgError> {
        if let Some(at_pos) = input.find('@') {
            let name = &input[..at_pos];
            let range = &input[at_pos + 1..];

            if name.is_empty() {
                return Err(PkgError::spec_invalid(format!(
                    "Invalid package spec: empty name in '{input}'"
                )));
            }

            if range.is_empty() {
                return Err(PkgError::spec_invalid(format!(
                    "Invalid package spec: empty version range in '{input}'"
                )));
            }

            // Validate name doesn't contain invalid characters
            Self::validate_name(name)?;

            Ok(Self {
                name: name.to_string(),
                scope: None,
                range: Some(range.to_string()),
            })
        } else {
            Self::validate_name(input)?;

            Ok(Self {
                name: input.to_string(),
                scope: None,
                range: None,
            })
        }
    }

    fn validate_name(name: &str) -> Result<(), PkgError> {
        if name.is_empty() {
            return Err(PkgError::spec_invalid("Empty package name"));
        }

        // Basic validation: no spaces, no special chars except - and _
        for c in name.chars() {
            if !c.is_alphanumeric() && c != '-' && c != '_' && c != '.' {
                return Err(PkgError::spec_invalid(format!(
                    "Invalid character '{c}' in package name '{name}'"
                )));
            }
        }

        Ok(())
    }

    /// Check if this is a scoped package.
    #[must_use]
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    /// Get the unscoped portion of the name.
    ///
    /// For `@scope/name`, returns `name`.
    /// For `react`, returns `react`.
    #[must_use]
    pub fn unscoped_name(&self) -> &str {
        if let Some(ref scope) = self.scope {
            // Skip @scope/
            &self.name[scope.len() + 2..]
        } else {
            &self.name
        }
    }

    /// URL-encode the package name for registry requests.
    ///
    /// For scoped packages, encodes the `/` as `%2F`.
    #[must_use]
    pub fn url_encoded_name(&self) -> String {
        if self.is_scoped() {
            self.name.replace('/', "%2F")
        } else {
            self.name.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let spec = PackageSpec::parse("react").unwrap();
        assert_eq!(spec.name, "react");
        assert_eq!(spec.scope, None);
        assert_eq!(spec.range, None);
    }

    #[test]
    fn test_parse_with_version() {
        let spec = PackageSpec::parse("react@18.2.0").unwrap();
        assert_eq!(spec.name, "react");
        assert_eq!(spec.scope, None);
        assert_eq!(spec.range, Some("18.2.0".to_string()));
    }

    #[test]
    fn test_parse_with_range() {
        let spec = PackageSpec::parse("react@^18.0.0").unwrap();
        assert_eq!(spec.name, "react");
        assert_eq!(spec.range, Some("^18.0.0".to_string()));
    }

    #[test]
    fn test_parse_scoped() {
        let spec = PackageSpec::parse("@types/node").unwrap();
        assert_eq!(spec.name, "@types/node");
        assert_eq!(spec.scope, Some("types".to_string()));
        assert_eq!(spec.range, None);
    }

    #[test]
    fn test_parse_scoped_with_version() {
        let spec = PackageSpec::parse("@types/node@20.0.0").unwrap();
        assert_eq!(spec.name, "@types/node");
        assert_eq!(spec.scope, Some("types".to_string()));
        assert_eq!(spec.range, Some("20.0.0".to_string()));
    }

    #[test]
    fn test_parse_scoped_with_range() {
        let spec = PackageSpec::parse("@types/node@^20").unwrap();
        assert_eq!(spec.name, "@types/node");
        assert_eq!(spec.scope, Some("types".to_string()));
        assert_eq!(spec.range, Some("^20".to_string()));
    }

    #[test]
    fn test_parse_empty_fails() {
        assert!(PackageSpec::parse("").is_err());
        assert!(PackageSpec::parse("   ").is_err());
    }

    #[test]
    fn test_parse_at_only_fails() {
        assert!(PackageSpec::parse("@").is_err());
    }

    #[test]
    fn test_parse_scope_only_fails() {
        assert!(PackageSpec::parse("@scope").is_err());
        assert!(PackageSpec::parse("@scope/").is_err());
    }

    #[test]
    fn test_parse_empty_range_fails() {
        assert!(PackageSpec::parse("react@").is_err());
        assert!(PackageSpec::parse("@types/node@").is_err());
    }

    #[test]
    fn test_unscoped_name() {
        let spec = PackageSpec::parse("react").unwrap();
        assert_eq!(spec.unscoped_name(), "react");

        let spec = PackageSpec::parse("@types/node").unwrap();
        assert_eq!(spec.unscoped_name(), "node");
    }

    #[test]
    fn test_url_encoded_name() {
        let spec = PackageSpec::parse("react").unwrap();
        assert_eq!(spec.url_encoded_name(), "react");

        let spec = PackageSpec::parse("@types/node").unwrap();
        assert_eq!(spec.url_encoded_name(), "@types%2Fnode");
    }

    #[test]
    fn test_is_scoped() {
        let spec = PackageSpec::parse("react").unwrap();
        assert!(!spec.is_scoped());

        let spec = PackageSpec::parse("@types/node").unwrap();
        assert!(spec.is_scoped());
    }
}
