//! Lockfile types for deterministic package installation.
//!
//! The lockfile records exact versions and integrity hashes for every package
//! in the dependency tree, enabling reproducible installs across environments.
//!
//! ## Schema Version
//!
//! - Schema version 1 (v1.9.0): Initial lockfile format
//!
//! ## File Format
//!
//! The lockfile is a JSON file named `howth.lock` with the following structure:
//!
//! ```json
//! {
//!   "lockfile_version": 1,
//!   "root": { "name": "my-project", "version": "1.0.0" },
//!   "packages": { ... },
//!   "dependencies": { ... }
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// Schema version for the lockfile format.
///
/// This is the contract version for the lockfile JSON structure.
/// Changes to this version indicate breaking changes to the format.
pub const PKG_LOCK_SCHEMA_VERSION: u32 = 1;

/// Lockfile filename.
pub const LOCKFILE_NAME: &str = "howth.lock";

/// Lockfile error codes.
pub mod codes {
    /// Lockfile not found at the expected path.
    pub const PKG_LOCK_NOT_FOUND: &str = "PKG_LOCK_NOT_FOUND";
    /// Lockfile has invalid JSON.
    pub const PKG_LOCK_INVALID_JSON: &str = "PKG_LOCK_INVALID_JSON";
    /// Lockfile schema version mismatch.
    pub const PKG_LOCK_VERSION_MISMATCH: &str = "PKG_LOCK_VERSION_MISMATCH";
    /// Lockfile integrity mismatch during install.
    pub const PKG_LOCK_INTEGRITY_MISMATCH: &str = "PKG_LOCK_INTEGRITY_MISMATCH";
    /// Required package missing from lockfile.
    pub const PKG_LOCK_PACKAGE_MISSING: &str = "PKG_LOCK_PACKAGE_MISSING";
    /// Lockfile write failed.
    pub const PKG_LOCK_WRITE_FAILED: &str = "PKG_LOCK_WRITE_FAILED";
    /// Lockfile is out of date (package.json changed).
    pub const PKG_LOCK_STALE: &str = "PKG_LOCK_STALE";
    /// Conflicting dependency requirements.
    pub const PKG_LOCK_CONFLICT: &str = "PKG_LOCK_CONFLICT";
}

/// Information about the root package (from package.json).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockRoot {
    /// Package name from package.json.
    pub name: String,
    /// Package version from package.json.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl LockRoot {
    /// Create a new root entry.
    #[must_use]
    pub fn new(name: impl Into<String>, version: Option<String>) -> Self {
        Self {
            name: name.into(),
            version,
        }
    }
}

/// Metadata about the lockfile itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LockMeta {
    /// When the lockfile was last updated (ISO 8601 timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
    /// Version of howth that generated this lockfile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub howth_version: Option<String>,
}

/// How a package was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LockResolution {
    /// Resolved from npm registry.
    Registry {
        /// Registry URL (empty string = default registry).
        #[serde(skip_serializing_if = "String::is_empty", default)]
        registry: String,
    },
    /// Resolved from a tarball URL.
    Tarball {
        /// URL to the tarball.
        url: String,
    },
    /// Resolved from a git repository.
    Git {
        /// Git URL.
        url: String,
        /// Commit hash or ref.
        #[serde(rename = "ref")]
        git_ref: String,
    },
    /// Resolved from the local filesystem.
    File {
        /// Relative path to the package.
        path: String,
    },
    /// Linked from another location (workspace package).
    Link {
        /// Relative path to the linked package.
        path: String,
    },
}

impl Default for LockResolution {
    fn default() -> Self {
        Self::Registry {
            registry: String::new(),
        }
    }
}

/// A dependency edge in the lockfile.
///
/// Represents a declared dependency from one package to another.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDepEdge {
    /// The version range as specified in package.json.
    pub range: String,
    /// Dependency kind: "dep", "dev", "optional", or "peer".
    pub kind: String,
}

impl LockDepEdge {
    /// Create a new dependency edge.
    #[must_use]
    pub fn new(range: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            range: range.into(),
            kind: kind.into(),
        }
    }

    /// Create a production dependency edge.
    #[must_use]
    pub fn production(range: impl Into<String>) -> Self {
        Self::new(range, "dep")
    }

    /// Create a dev dependency edge.
    #[must_use]
    pub fn dev(range: impl Into<String>) -> Self {
        Self::new(range, "dev")
    }

    /// Create an optional dependency edge.
    #[must_use]
    pub fn optional(range: impl Into<String>) -> Self {
        Self::new(range, "optional")
    }

    /// Create a peer dependency edge.
    #[must_use]
    pub fn peer(range: impl Into<String>) -> Self {
        Self::new(range, "peer")
    }
}

/// A locked package entry.
///
/// Contains the exact resolved version and integrity hash for a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockPackage {
    /// Resolved version string.
    pub version: String,
    /// Subresource integrity hash (e.g., "sha512-...").
    pub integrity: String,
    /// How this package was resolved.
    #[serde(default, skip_serializing_if = "is_default_resolution")]
    pub resolution: LockResolution,
    /// Real package name when this is an `npm:` alias.
    /// E.g., if the dep is `"string-width-cjs": "npm:string-width@^4.2.0"`,
    /// then the lockfile key is `string-width-cjs@4.2.3` and `alias_for` is `"string-width"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_for: Option<String>,
    /// Dependencies of this package (name -> version range).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, String>,
    /// Optional dependencies of this package.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub optional_dependencies: BTreeMap<String, String>,
    /// Peer dependencies of this package.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub peer_dependencies: BTreeMap<String, String>,
    /// Whether this package has install scripts.
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_scripts: bool,
    /// CPU architectures this package supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cpu: Vec<String>,
    /// Operating systems this package supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<String>,
}

fn is_default_resolution(r: &LockResolution) -> bool {
    matches!(r, LockResolution::Registry { registry } if registry.is_empty())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !b
}

impl LockPackage {
    /// Create a new locked package entry.
    #[must_use]
    pub fn new(version: impl Into<String>, integrity: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            integrity: integrity.into(),
            resolution: LockResolution::default(),
            alias_for: None,
            dependencies: BTreeMap::new(),
            optional_dependencies: BTreeMap::new(),
            peer_dependencies: BTreeMap::new(),
            has_scripts: false,
            cpu: Vec::new(),
            os: Vec::new(),
        }
    }

    /// Set the resolution method.
    #[must_use]
    pub fn with_resolution(mut self, resolution: LockResolution) -> Self {
        self.resolution = resolution;
        self
    }

    /// Add a dependency.
    pub fn add_dependency(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.dependencies.insert(name.into(), version.into());
    }

    /// Add an optional dependency.
    pub fn add_optional_dependency(&mut self, name: impl Into<String>, version: impl Into<String>) {
        self.optional_dependencies
            .insert(name.into(), version.into());
    }
}

/// A declared dependency from the root package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDep {
    /// The version range as specified in package.json.
    pub range: String,
    /// Dependency kind: "dep", "dev", "optional", or "peer".
    pub kind: String,
    /// The resolved version (matches a key in `packages`).
    pub resolved: String,
}

impl LockDep {
    /// Create a new root dependency.
    #[must_use]
    pub fn new(
        range: impl Into<String>,
        kind: impl Into<String>,
        resolved: impl Into<String>,
    ) -> Self {
        Self {
            range: range.into(),
            kind: kind.into(),
            resolved: resolved.into(),
        }
    }
}

/// The complete lockfile.
///
/// Records exact versions for deterministic installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    /// Schema version for the lockfile format.
    pub lockfile_version: u32,
    /// Metadata about the lockfile.
    #[serde(default, skip_serializing_if = "is_default_meta")]
    pub meta: LockMeta,
    /// Information about the root package.
    pub root: LockRoot,
    /// Root-level dependencies (name -> `LockDep`).
    /// `BTreeMap` ensures deterministic ordering.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, LockDep>,
    /// All locked packages (key = "name@version").
    /// `BTreeMap` ensures deterministic ordering.
    pub packages: BTreeMap<String, LockPackage>,
}

fn is_default_meta(m: &LockMeta) -> bool {
    m.generated_at.is_none() && m.howth_version.is_none()
}

impl Lockfile {
    /// Create a new empty lockfile.
    #[must_use]
    pub fn new(root: LockRoot) -> Self {
        Self {
            lockfile_version: PKG_LOCK_SCHEMA_VERSION,
            meta: LockMeta::default(),
            root,
            dependencies: BTreeMap::new(),
            packages: BTreeMap::new(),
        }
    }

    /// Get the package key for a name and version.
    #[must_use]
    pub fn package_key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    /// Add a package to the lockfile.
    pub fn add_package(&mut self, name: &str, pkg: LockPackage) {
        let key = Self::package_key(name, &pkg.version);
        self.packages.insert(key, pkg);
    }

    /// Add a root dependency.
    pub fn add_dependency(&mut self, name: impl Into<String>, dep: LockDep) {
        self.dependencies.insert(name.into(), dep);
    }

    /// Get a package by name and version.
    #[must_use]
    pub fn get_package(&self, name: &str, version: &str) -> Option<&LockPackage> {
        let key = Self::package_key(name, version);
        self.packages.get(&key)
    }

    /// Check if a package is in the lockfile.
    #[must_use]
    pub fn has_package(&self, name: &str, version: &str) -> bool {
        let key = Self::package_key(name, version);
        self.packages.contains_key(&key)
    }

    /// Read a lockfile from a path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn read_from(path: &Path) -> Result<Self, LockfileError> {
        let content = fs::read_to_string(path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                LockfileError::new(
                    codes::PKG_LOCK_NOT_FOUND,
                    format!("Lockfile not found: {}", path.display()),
                )
            } else {
                LockfileError::new(
                    codes::PKG_LOCK_INVALID_JSON,
                    format!("Failed to read lockfile: {e}"),
                )
            }
        })?;

        let lockfile: Self = serde_json::from_str(&content).map_err(|e| {
            LockfileError::new(
                codes::PKG_LOCK_INVALID_JSON,
                format!("Invalid lockfile JSON: {e}"),
            )
        })?;

        if lockfile.lockfile_version != PKG_LOCK_SCHEMA_VERSION {
            return Err(LockfileError::new(
                codes::PKG_LOCK_VERSION_MISMATCH,
                format!(
                    "Lockfile version {} not supported (expected {})",
                    lockfile.lockfile_version, PKG_LOCK_SCHEMA_VERSION
                ),
            ));
        }

        Ok(lockfile)
    }

    /// Write the lockfile to a path atomically.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write_to(&self, path: &Path) -> Result<(), LockfileError> {
        let content = serde_json::to_string_pretty(self).map_err(|e| {
            LockfileError::new(
                codes::PKG_LOCK_WRITE_FAILED,
                format!("Failed to serialize lockfile: {e}"),
            )
        })?;

        // Use atomic write to prevent corruption
        fastnode_util::fs::atomic_write(path, content.as_bytes()).map_err(|e| {
            LockfileError::new(
                codes::PKG_LOCK_WRITE_FAILED,
                format!("Failed to write lockfile: {e}"),
            )
        })
    }

    /// Serialize to JSON string.
    ///
    /// # Panics
    /// Panics if serialization fails (should not happen with valid data).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Lockfile serialization should not fail")
    }

    /// Deserialize from JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid.
    pub fn from_json(json: &str) -> Result<Self, LockfileError> {
        serde_json::from_str(json).map_err(|e| {
            LockfileError::new(
                codes::PKG_LOCK_INVALID_JSON,
                format!("Invalid lockfile JSON: {e}"),
            )
        })
    }

    /// Set the metadata.
    ///
    /// The `generated_at` should be an ISO 8601 timestamp (e.g., "2024-01-15T12:30:00Z").
    /// If `None`, no timestamp is recorded.
    pub fn set_meta(&mut self, howth_version: Option<String>, generated_at: Option<String>) {
        self.meta.howth_version = howth_version;
        self.meta.generated_at = generated_at;
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new(LockRoot::new("unknown", None))
    }
}

/// Lockfile error.
#[derive(Debug)]
pub struct LockfileError {
    code: &'static str,
    message: String,
}

impl LockfileError {
    /// Create a new error.
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
}

impl fmt::Display for LockfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LockfileError {}

/// Compute a deterministic hash of a lockfile's content.
///
/// Serializes the lockfile to JSON (BTreeMap guarantees deterministic order)
/// and returns a BLAKE3 hex digest. This is used to detect whether
/// `node_modules` is already up-to-date with the lockfile.
#[must_use]
pub fn lockfile_content_hash(lockfile: &Lockfile) -> String {
    let json = serde_json::to_string(lockfile).expect("Lockfile serialization should not fail");
    blake3::hash(json.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_schema_version_is_stable() {
        assert_eq!(PKG_LOCK_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_error_codes_are_uppercase() {
        let all_codes = [
            codes::PKG_LOCK_NOT_FOUND,
            codes::PKG_LOCK_INVALID_JSON,
            codes::PKG_LOCK_VERSION_MISMATCH,
            codes::PKG_LOCK_INTEGRITY_MISMATCH,
            codes::PKG_LOCK_PACKAGE_MISSING,
            codes::PKG_LOCK_WRITE_FAILED,
            codes::PKG_LOCK_STALE,
            codes::PKG_LOCK_CONFLICT,
        ];

        for code in all_codes {
            assert!(
                code.chars().all(|c| c.is_uppercase() || c == '_'),
                "Error code '{code}' should be SCREAMING_SNAKE_CASE"
            );
        }
    }

    #[test]
    fn test_lockfile_new() {
        let root = LockRoot::new("my-project", Some("1.0.0".to_string()));
        let lockfile = Lockfile::new(root);

        assert_eq!(lockfile.lockfile_version, PKG_LOCK_SCHEMA_VERSION);
        assert_eq!(lockfile.root.name, "my-project");
        assert_eq!(lockfile.root.version, Some("1.0.0".to_string()));
        assert!(lockfile.dependencies.is_empty());
        assert!(lockfile.packages.is_empty());
    }

    #[test]
    fn test_lockfile_add_package() {
        let mut lockfile = Lockfile::new(LockRoot::new("test", None));

        let pkg = LockPackage::new("1.0.0", "sha512-abc123");
        lockfile.add_package("lodash", pkg);

        assert!(lockfile.has_package("lodash", "1.0.0"));
        assert!(!lockfile.has_package("lodash", "2.0.0"));
    }

    #[test]
    fn test_lockfile_package_key() {
        assert_eq!(Lockfile::package_key("lodash", "4.17.21"), "lodash@4.17.21");
        assert_eq!(
            Lockfile::package_key("@types/node", "20.0.0"),
            "@types/node@20.0.0"
        );
    }

    #[test]
    fn test_lockfile_json_roundtrip() {
        let mut lockfile = Lockfile::new(LockRoot::new("my-project", Some("1.0.0".to_string())));

        let mut pkg = LockPackage::new("4.17.21", "sha512-abc123");
        pkg.add_dependency("lodash.isequal", "4.5.0");
        lockfile.add_package("lodash", pkg);

        lockfile.add_dependency("lodash", LockDep::new("^4.17.0", "dep", "4.17.21"));

        let json = lockfile.to_json();
        let parsed = Lockfile::from_json(&json).unwrap();

        assert_eq!(lockfile, parsed);
    }

    #[test]
    fn test_lockfile_deterministic_ordering() {
        let mut lockfile = Lockfile::new(LockRoot::new("test", None));

        // Add packages in random order
        lockfile.add_package("zod", LockPackage::new("3.0.0", "sha512-z"));
        lockfile.add_package("axios", LockPackage::new("1.0.0", "sha512-a"));
        lockfile.add_package("lodash", LockPackage::new("4.0.0", "sha512-l"));

        // JSON output should be deterministically ordered
        let json1 = lockfile.to_json();
        let json2 = lockfile.to_json();

        assert_eq!(json1, json2);

        // Verify alphabetical ordering in output
        let axios_pos = json1.find("axios@").unwrap();
        let lodash_pos = json1.find("lodash@").unwrap();
        let zod_pos = json1.find("zod@").unwrap();

        assert!(axios_pos < lodash_pos);
        assert!(lodash_pos < zod_pos);
    }

    #[test]
    fn test_lock_package_with_dependencies() {
        let mut pkg = LockPackage::new("1.0.0", "sha512-hash");
        pkg.add_dependency("dep-a", "1.0.0");
        pkg.add_dependency("dep-b", "2.0.0");
        pkg.add_optional_dependency("opt-a", "1.0.0");

        assert_eq!(pkg.dependencies.len(), 2);
        assert_eq!(pkg.optional_dependencies.len(), 1);
    }

    #[test]
    fn test_lock_resolution_variants() {
        let registry = LockResolution::Registry {
            registry: String::new(),
        };
        let tarball = LockResolution::Tarball {
            url: "https://example.com/pkg.tgz".to_string(),
        };
        let git = LockResolution::Git {
            url: "git+ssh://git@github.com/user/repo.git".to_string(),
            git_ref: "abc123".to_string(),
        };
        let file = LockResolution::File {
            path: "../local-pkg".to_string(),
        };
        let link = LockResolution::Link {
            path: "packages/shared".to_string(),
        };

        // Test serialization
        let pkg_registry = LockPackage::new("1.0.0", "sha512-a").with_resolution(registry);
        let pkg_tarball = LockPackage::new("1.0.0", "sha512-b").with_resolution(tarball);
        let pkg_git = LockPackage::new("1.0.0", "sha512-c").with_resolution(git);
        let pkg_file = LockPackage::new("1.0.0", "sha512-d").with_resolution(file);
        let pkg_link = LockPackage::new("1.0.0", "sha512-e").with_resolution(link);

        // Default resolution (registry with empty string) should be omitted
        let json = serde_json::to_string(&pkg_registry).unwrap();
        assert!(!json.contains("resolution"));

        // Other resolutions should be included
        let json = serde_json::to_string(&pkg_tarball).unwrap();
        assert!(json.contains("tarball"));

        let json = serde_json::to_string(&pkg_git).unwrap();
        assert!(json.contains("git"));

        let json = serde_json::to_string(&pkg_file).unwrap();
        assert!(json.contains("file"));

        let json = serde_json::to_string(&pkg_link).unwrap();
        assert!(json.contains("link"));
    }

    #[test]
    fn test_lock_dep_edge_constructors() {
        let dep = LockDepEdge::production("^1.0.0");
        assert_eq!(dep.kind, "dep");
        assert_eq!(dep.range, "^1.0.0");

        let dev = LockDepEdge::dev("^2.0.0");
        assert_eq!(dev.kind, "dev");

        let optional = LockDepEdge::optional("^3.0.0");
        assert_eq!(optional.kind, "optional");

        let peer = LockDepEdge::peer("^4.0.0");
        assert_eq!(peer.kind, "peer");
    }

    #[test]
    fn test_lockfile_error() {
        let err = LockfileError::new(codes::PKG_LOCK_NOT_FOUND, "File not found");
        assert_eq!(err.code(), codes::PKG_LOCK_NOT_FOUND);
        assert_eq!(err.message(), "File not found");
        assert!(err.to_string().contains("PKG_LOCK_NOT_FOUND"));
    }

    #[test]
    fn test_lockfile_read_not_found() {
        let result = Lockfile::read_from(Path::new("/nonexistent/howth.lock"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::PKG_LOCK_NOT_FOUND);
    }

    #[test]
    fn test_lockfile_read_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("howth.lock");
        std::fs::write(&path, "not valid json").unwrap();

        let result = Lockfile::read_from(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::PKG_LOCK_INVALID_JSON);
    }

    #[test]
    fn test_lockfile_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("howth.lock");

        let mut lockfile = Lockfile::new(LockRoot::new("test-project", Some("1.0.0".to_string())));
        lockfile.add_package("lodash", LockPackage::new("4.17.21", "sha512-abc"));

        lockfile.write_to(&path).unwrap();

        let loaded = Lockfile::read_from(&path).unwrap();
        assert_eq!(lockfile, loaded);
    }
}
