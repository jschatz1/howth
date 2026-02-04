//! Symlink/junction creation for `node_modules`.
//!
//! Uses a pnpm-style layout to allow packages in the global cache
//! to properly resolve their dependencies:
//!
//! ```text
//! node_modules/
//!   .pnpm/
//!     lodash@4.17.21/
//!       node_modules/
//!         lodash -> (symlink to global cache)
//!     chalk@4.1.2/
//!       node_modules/
//!         chalk -> (symlink to global cache)
//!         ansi-styles -> ../../ansi-styles@4.3.0/node_modules/ansi-styles
//!         supports-color -> ../../supports-color@7.2.0/node_modules/supports-color
//!   lodash -> .pnpm/lodash@4.17.21/node_modules/lodash
//!   chalk -> .pnpm/chalk@4.1.2/node_modules/chalk
//! ```

#![allow(clippy::manual_let_else)]
#![allow(clippy::items_after_statements)]

use super::error::PkgError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Link package binaries into `node_modules/.bin/`.
///
/// Reads the package's `package.json` to find the `bin` field and creates
/// symlinks for each binary in `node_modules/.bin/`.
///
/// When `pnpm_pkg_dir` is `Some`, binary symlinks target that path (inside
/// `.pnpm/<name>@<version>/node_modules/<name>`) so that Node.js resolves
/// transitive dependencies through the pnpm layout. When `None` (e.g. for
/// workspace packages), the symlinks point directly at `cached_pkg_dir`.
///
/// # Errors
/// Returns an error if the binaries cannot be linked.
pub fn link_package_binaries(
    project_root: &Path,
    pkg_name: &str,
    cached_pkg_dir: &Path,
    pnpm_pkg_dir: Option<&Path>,
) -> Result<Vec<PathBuf>, PkgError> {
    let package_json_path = cached_pkg_dir.join("package.json");

    // Read and parse package.json
    let package_json_content = fs::read_to_string(&package_json_path).map_err(|e| {
        PkgError::link_failed(format!("Failed to read package.json for {pkg_name}: {e}"))
    })?;

    let package_json: Value = serde_json::from_str(&package_json_content).map_err(|e| {
        PkgError::link_failed(format!("Failed to parse package.json for {pkg_name}: {e}"))
    })?;

    // Get the bin field - can be string or object
    let bin_field = match package_json.get("bin") {
        Some(bin) => bin,
        None => return Ok(vec![]), // No binaries to link
    };

    let node_modules = project_root.join("node_modules");
    let bin_dir = node_modules.join(".bin");

    // Ensure .bin directory exists
    fs::create_dir_all(&bin_dir).map_err(|e| {
        PkgError::node_modules_write_failed(format!("Failed to create .bin directory: {e}"))
    })?;

    let mut linked_binaries = Vec::new();

    // Use the pnpm layout path when available so that binaries resolve
    // transitive deps via .pnpm/<name>@<version>/node_modules/.
    let target_base = pnpm_pkg_dir.unwrap_or(cached_pkg_dir);

    // Handle both string and object forms of bin field
    match bin_field {
        Value::String(bin_path) => {
            // Single binary: use package name as binary name
            let binary_name = pkg_name.split('/').next_back().unwrap_or(pkg_name);
            let link_path = link_binary(&bin_dir, binary_name, target_base, bin_path)?;
            linked_binaries.push(link_path);
        }
        Value::Object(bins) => {
            // Multiple binaries: each key is a binary name
            for (bin_name, bin_path) in bins {
                if let Value::String(path) = bin_path {
                    let link_path = link_binary(&bin_dir, bin_name, target_base, path)?;
                    linked_binaries.push(link_path);
                }
            }
        }
        _ => {
            // Invalid bin field format, skip
        }
    }

    Ok(linked_binaries)
}

/// Create a symlink for a single binary.
fn link_binary(
    bin_dir: &Path,
    bin_name: &str,
    pkg_dir: &Path,
    bin_path: &str,
) -> Result<PathBuf, PkgError> {
    let link_path = bin_dir.join(bin_name);
    let target_path = pkg_dir.join(bin_path);

    // Remove existing link if present
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        remove_link_or_dir(&link_path)?;
    }

    // Create the symlink
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target_path, &link_path).map_err(|e| {
            PkgError::link_failed(format!(
                "Failed to create binary symlink {} -> {}: {}",
                link_path.display(),
                target_path.display(),
                e
            ))
        })?;

        // Make the target executable
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&target_path) {
            let mut perms = metadata.permissions();
            let mode = perms.mode() | 0o111; // Add execute permission
            perms.set_mode(mode);
            let _ = fs::set_permissions(&target_path, perms);
        }
    }

    #[cfg(windows)]
    {
        // On Windows, create a cmd shim instead of a symlink
        create_cmd_shim(&link_path, &target_path)?;
    }

    Ok(link_path)
}

#[cfg(windows)]
fn create_cmd_shim(link_path: &Path, target_path: &Path) -> Result<(), PkgError> {
    // Create a .cmd file that runs the target
    let cmd_path = link_path.with_extension("cmd");
    let shim_content = format!("@ECHO off\r\nnode \"{}\" %*\r\n", target_path.display());

    fs::write(&cmd_path, shim_content).map_err(|e| {
        PkgError::link_failed(format!(
            "Failed to create cmd shim {}: {}",
            cmd_path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Link a cached package into a project's `node_modules` using pnpm-style layout.
///
/// Creates the following structure:
/// ```text
/// node_modules/
///   .pnpm/<name>@<version>/node_modules/<name> -> cached_pkg_dir
///   <name> -> .pnpm/<name>@<version>/node_modules/<name>
/// ```
///
/// This allows packages in the cache to find their dependencies via the
/// `.pnpm/<name>@<version>/node_modules/` directory.
///
/// # Arguments
/// * `project_root` - The project root directory
/// * `pkg_name` - Package name (e.g., "lodash" or "@types/node")
/// * `pkg_version` - Package version (e.g., "4.17.21")
/// * `cached_pkg_dir` - Path to the package in the global cache
///
/// # Returns
/// The path to the top-level symlink in `node_modules`.
///
/// # Errors
/// Returns an error if the links cannot be created.
pub fn link_into_node_modules(
    project_root: &Path,
    pkg_name: &str,
    cached_pkg_dir: &Path,
) -> Result<PathBuf, PkgError> {
    // Extract version from the cached path (format: .../name/version/package)
    // We need the version for the .pnpm directory name
    let version = cached_pkg_dir
        .parent() // package -> version
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("0.0.0");

    link_into_node_modules_with_version(project_root, pkg_name, version, cached_pkg_dir)
}

/// Link a cached package into a project's `node_modules` using pnpm-style layout.
///
/// This version takes an explicit version parameter.
///
/// The package content is hard-linked (or copied if not possible) into
/// `.pnpm/<name>@<version>/node_modules/<name>/` so that Node.js module
/// resolution works correctly (symlinks would resolve to the cache path).
pub fn link_into_node_modules_with_version(
    project_root: &Path,
    pkg_name: &str,
    pkg_version: &str,
    cached_pkg_dir: &Path,
) -> Result<PathBuf, PkgError> {
    let node_modules = project_root.join("node_modules");
    let pnpm_dir = node_modules.join(".pnpm");

    // Ensure directories exist
    fs::create_dir_all(&node_modules).map_err(|e| {
        PkgError::node_modules_write_failed(format!("Failed to create node_modules directory: {e}"))
    })?;
    fs::create_dir_all(&pnpm_dir).map_err(|e| {
        PkgError::node_modules_write_failed(format!("Failed to create .pnpm directory: {e}"))
    })?;

    // Create .pnpm/<name>@<version>/node_modules/<name>
    let pnpm_pkg_key = format_pnpm_key(pkg_name, pkg_version);
    let pnpm_pkg_dir = pnpm_dir.join(&pnpm_pkg_key).join("node_modules");
    fs::create_dir_all(&pnpm_pkg_dir).map_err(|e| {
        PkgError::node_modules_write_failed(format!(
            "Failed to create .pnpm package directory: {e}"
        ))
    })?;

    // Get the destination path for the package
    let pnpm_pkg_dest = get_package_link_path(&pnpm_pkg_dir, pkg_name)?;

    // Fast path: if the destination already has the same content (same inode on
    // package.json), skip the expensive recursive hard-link.  This avoids
    // thousands of syscalls on repeated installs.
    if needs_relink(cached_pkg_dir, &pnpm_pkg_dest) {
        // Remove existing content if present
        if pnpm_pkg_dest.exists() || pnpm_pkg_dest.symlink_metadata().is_ok() {
            remove_link_or_dir(&pnpm_pkg_dest)?;
        }

        // Hard-link or copy the package content (not symlink!)
        // This ensures Node.js sees the real path as within .pnpm, not the cache
        hard_link_or_copy_dir(cached_pkg_dir, &pnpm_pkg_dest)?;
    } else {
        // Content already matches — skip hard-linking.
    }

    // Create top-level link: node_modules/<name> -> .pnpm/<key>/node_modules/<name>
    let top_level_link = get_package_link_path(&node_modules, pkg_name)?;
    if top_level_link.exists() || top_level_link.symlink_metadata().is_ok() {
        remove_link_or_dir(&top_level_link)?;
    }
    create_dir_link(&pnpm_pkg_dest, &top_level_link)?;

    Ok(top_level_link)
}

/// Check whether the destination needs to be re-linked from the cache.
///
/// Returns `false` (no relink needed) when the destination's `package.json`
/// shares the same inode as the cache's — meaning the hard-links are already
/// in place from a previous install.
#[cfg(unix)]
fn needs_relink(cache_dir: &Path, dest_dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let cache_pkg = cache_dir.join("package.json");
    let dest_pkg = dest_dir.join("package.json");

    let (Ok(cache_meta), Ok(dest_meta)) = (fs::metadata(&cache_pkg), fs::metadata(&dest_pkg))
    else {
        return true; // Missing file — needs linking
    };

    // Same inode + same device = same hard-link
    cache_meta.ino() != dest_meta.ino() || cache_meta.dev() != dest_meta.dev()
}

#[cfg(not(unix))]
fn needs_relink(_cache_dir: &Path, dest_dir: &Path) -> bool {
    // On non-Unix, always relink (no inode check available)
    !dest_dir.join("package.json").exists()
}

/// Hard-link files from src to dst, falling back to copy if hard linking fails.
/// Directories are created, files are hard-linked or copied.
fn hard_link_or_copy_dir(src: &Path, dst: &Path) -> Result<(), PkgError> {
    fs::create_dir_all(dst).map_err(|e| {
        PkgError::link_failed(format!("Failed to create directory {}: {e}", dst.display()))
    })?;

    for entry in fs::read_dir(src).map_err(|e| {
        PkgError::link_failed(format!("Failed to read directory {}: {e}", src.display()))
    })? {
        let entry = entry.map_err(|e| {
            PkgError::link_failed(format!("Failed to read entry in {}: {e}", src.display()))
        })?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            hard_link_or_copy_dir(&src_path, &dst_path)?;
        } else {
            // Try hard link first, fall back to copy
            if fs::hard_link(&src_path, &dst_path).is_err() {
                fs::copy(&src_path, &dst_path).map_err(|e| {
                    PkgError::link_failed(format!(
                        "Failed to copy {} to {}: {e}",
                        src_path.display(),
                        dst_path.display()
                    ))
                })?;
            }
        }
    }

    Ok(())
}

/// Link a package's dependencies in its .pnpm `node_modules` directory.
///
/// For each dependency, creates:
/// `.pnpm/<pkg>@<version>/node_modules/<dep> -> .pnpm/<dep>@<dep_version>/node_modules/<dep>`
///
/// # Arguments
/// * `project_root` - The project root directory
/// * `pkg_name` - Package name
/// * `pkg_version` - Package version
/// * `dependencies` - Map of dependency name -> resolved version
///
/// # Errors
/// Returns an error if any link cannot be created.
pub fn link_package_dependencies(
    project_root: &Path,
    pkg_name: &str,
    pkg_version: &str,
    dependencies: &BTreeMap<String, String>,
) -> Result<(), PkgError> {
    if dependencies.is_empty() {
        return Ok(());
    }

    let node_modules = project_root.join("node_modules");
    let pnpm_dir = node_modules.join(".pnpm");

    // Get the package's node_modules directory in .pnpm
    let pnpm_pkg_key = format_pnpm_key(pkg_name, pkg_version);
    let pkg_node_modules = pnpm_dir.join(&pnpm_pkg_key).join("node_modules");

    // Link each dependency
    for (dep_name, dep_version) in dependencies {
        let dep_pnpm_key = format_pnpm_key(dep_name, dep_version);
        let dep_target = pnpm_dir
            .join(&dep_pnpm_key)
            .join("node_modules")
            .join(dep_name);

        // Handle scoped packages
        let dep_link = get_package_link_path(&pkg_node_modules, dep_name)?;

        // Only create link if target exists and link doesn't already point there
        if dep_target.exists() {
            if dep_link.exists() || dep_link.symlink_metadata().is_ok() {
                // Check if it's already pointing to the right place
                if let Ok(existing_target) = fs::read_link(&dep_link) {
                    if existing_target == dep_target {
                        continue; // Already correctly linked
                    }
                }
                remove_link_or_dir(&dep_link)?;
            }
            create_dir_link(&dep_target, &dep_link)?;
        }
    }

    Ok(())
}

/// Format a pnpm directory key for a package.
/// Handles scoped packages by replacing '/' with '+'.
#[must_use]
pub fn format_pnpm_key(name: &str, version: &str) -> String {
    if name.starts_with('@') {
        // @scope/name@version -> @scope+name@version
        format!("{}@{}", name.replace('/', "+"), version)
    } else {
        format!("{name}@{version}")
    }
}

/// Get the link path for a package, handling scoped packages.
fn get_package_link_path(parent_dir: &Path, pkg_name: &str) -> Result<PathBuf, PkgError> {
    if pkg_name.starts_with('@') {
        // Scoped package: @scope/name
        let parts: Vec<&str> = pkg_name.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(PkgError::link_failed(format!(
                "Invalid scoped package name: {pkg_name}"
            )));
        }

        let scope_dir = parent_dir.join(parts[0]);
        fs::create_dir_all(&scope_dir).map_err(|e| {
            PkgError::node_modules_write_failed(format!(
                "Failed to create scope directory {}: {e}",
                parts[0]
            ))
        })?;

        Ok(scope_dir.join(parts[1]))
    } else {
        Ok(parent_dir.join(pkg_name))
    }
}

/// Legacy function for simple linking (used for workspace packages).
///
/// Creates a direct symlink from `<project>/node_modules/<name>` to the target.
/// Does not use the pnpm layout.
pub fn link_into_node_modules_direct(
    project_root: &Path,
    pkg_name: &str,
    target_dir: &Path,
) -> Result<PathBuf, PkgError> {
    let node_modules = project_root.join("node_modules");

    // Ensure node_modules exists
    fs::create_dir_all(&node_modules).map_err(|e| {
        PkgError::node_modules_write_failed(format!("Failed to create node_modules directory: {e}"))
    })?;

    let link_path = get_package_link_path(&node_modules, pkg_name)?;

    // Remove existing link/directory if present
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        remove_link_or_dir(&link_path)?;
    }

    // Create the link
    create_dir_link(target_dir, &link_path)?;

    Ok(link_path)
}

/// Remove a symlink, junction, or directory.
fn remove_link_or_dir(path: &Path) -> Result<(), PkgError> {
    #[cfg(unix)]
    {
        // On Unix, remove_file handles symlinks
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                fs::remove_file(path).map_err(|e| {
                    PkgError::link_failed(format!("Failed to remove existing symlink: {e}"))
                })?;
                return Ok(());
            }
        }
    }

    #[cfg(windows)]
    {
        // On Windows, junctions are directories but need special handling
        use std::os::windows::fs::MetadataExt;

        if let Ok(metadata) = fs::symlink_metadata(path) {
            let file_attributes = metadata.file_attributes();
            // FILE_ATTRIBUTE_REPARSE_POINT = 0x400
            if file_attributes & 0x400 != 0 {
                // This is a junction or symlink
                fs::remove_dir(path).map_err(|e| {
                    PkgError::link_failed(format!("Failed to remove existing junction: {e}"))
                })?;
                return Ok(());
            }
        }
    }

    // Regular directory
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| {
            PkgError::link_failed(format!("Failed to remove existing directory: {e}"))
        })?;
    } else if path.exists() {
        fs::remove_file(path)
            .map_err(|e| PkgError::link_failed(format!("Failed to remove existing file: {e}")))?;
    }

    Ok(())
}

/// Create a directory link (symlink on Unix, junction on Windows).
fn create_dir_link(src: &Path, dst: &Path) -> Result<(), PkgError> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst).map_err(|e| {
            PkgError::link_failed(format!(
                "Failed to create symlink from {} to {}: {e}",
                dst.display(),
                src.display()
            ))
        })?;
    }

    #[cfg(windows)]
    {
        junction::create(src, dst).map_err(|e| {
            PkgError::link_failed(format!(
                "Failed to create junction from {} to {}: {e}",
                dst.display(),
                src.display()
            ))
        })?;
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback: copy directory
        copy_dir_all(src, dst)
            .map_err(|e| PkgError::link_failed(format!("Failed to copy directory: {e}")))?;
    }

    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_link_unscoped_package() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create a fake cached package
        let cached_pkg = cache.path().join("react").join("18.2.0").join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::write(cached_pkg.join("package.json"), "{}").unwrap();

        // Link into project using pnpm layout
        let link_path = link_into_node_modules(project.path(), "react", &cached_pkg).unwrap();

        assert!(link_path.exists());
        assert_eq!(link_path, project.path().join("node_modules").join("react"));

        // Verify the link target is accessible
        assert!(link_path.join("package.json").exists());

        // Verify pnpm structure exists
        let pnpm_pkg = project
            .path()
            .join("node_modules/.pnpm/react@18.2.0/node_modules/react");
        assert!(pnpm_pkg.exists());
        assert!(pnpm_pkg.join("package.json").exists());
    }

    #[test]
    fn test_link_scoped_package() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create a fake cached package
        let cached_pkg = cache
            .path()
            .join("@types")
            .join("node")
            .join("20.0.0")
            .join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::write(cached_pkg.join("package.json"), "{}").unwrap();

        // Link into project
        let link_path = link_into_node_modules_with_version(
            project.path(),
            "@types/node",
            "20.0.0",
            &cached_pkg,
        )
        .unwrap();

        assert!(link_path.exists());
        assert_eq!(
            link_path,
            project
                .path()
                .join("node_modules")
                .join("@types")
                .join("node")
        );

        // Verify the link target is accessible
        assert!(link_path.join("package.json").exists());

        // Verify pnpm structure with scoped package (@ scope + name)
        let pnpm_pkg = project
            .path()
            .join("node_modules/.pnpm/@types+node@20.0.0/node_modules/@types/node");
        assert!(pnpm_pkg.exists());
    }

    #[test]
    fn test_link_replaces_existing() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create node_modules with existing directory
        let existing = project.path().join("node_modules").join("react");
        fs::create_dir_all(&existing).unwrap();
        fs::write(existing.join("old.txt"), "old").unwrap();

        // Create a new cached package
        let cached_pkg = cache.path().join("react").join("18.2.0").join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::write(cached_pkg.join("package.json"), "{}").unwrap();

        // Link should replace the existing directory
        let link_path = link_into_node_modules(project.path(), "react", &cached_pkg).unwrap();

        assert!(link_path.exists());
        assert!(link_path.join("package.json").exists());
        // Old content should be gone
        assert!(!link_path.join("old.txt").exists());
    }

    #[test]
    fn test_link_idempotent() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        let cached_pkg = cache.path().join("react").join("18.2.0").join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::write(cached_pkg.join("package.json"), "{}").unwrap();

        // Link twice
        link_into_node_modules(project.path(), "react", &cached_pkg).unwrap();
        let link_path = link_into_node_modules(project.path(), "react", &cached_pkg).unwrap();

        assert!(link_path.exists());
        assert!(link_path.join("package.json").exists());
    }

    #[test]
    fn test_link_package_dependencies() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create chalk and its dependency ansi-styles in cache
        let chalk_pkg = cache.path().join("chalk").join("4.1.2").join("package");
        fs::create_dir_all(&chalk_pkg).unwrap();
        fs::write(chalk_pkg.join("package.json"), r#"{"name": "chalk"}"#).unwrap();

        let ansi_pkg = cache
            .path()
            .join("ansi-styles")
            .join("4.3.0")
            .join("package");
        fs::create_dir_all(&ansi_pkg).unwrap();
        fs::write(ansi_pkg.join("package.json"), r#"{"name": "ansi-styles"}"#).unwrap();

        // Link both packages first
        link_into_node_modules_with_version(project.path(), "chalk", "4.1.2", &chalk_pkg).unwrap();
        link_into_node_modules_with_version(project.path(), "ansi-styles", "4.3.0", &ansi_pkg)
            .unwrap();

        // Now link chalk's dependencies
        let mut deps = BTreeMap::new();
        deps.insert("ansi-styles".to_string(), "4.3.0".to_string());

        link_package_dependencies(project.path(), "chalk", "4.1.2", &deps).unwrap();

        // Verify ansi-styles is linked inside chalk's node_modules
        let dep_link = project
            .path()
            .join("node_modules/.pnpm/chalk@4.1.2/node_modules/ansi-styles");
        assert!(dep_link.exists());
        assert!(dep_link.join("package.json").exists());
    }

    #[test]
    fn test_link_package_binaries_string_form() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create a fake cached package with bin as string
        let cached_pkg = cache.path().join("prettier").join("3.0.0").join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::create_dir_all(cached_pkg.join("bin")).unwrap();
        fs::write(
            cached_pkg.join("bin/prettier.cjs"),
            "#!/usr/bin/env node\nconsole.log('prettier');",
        )
        .unwrap();
        fs::write(
            cached_pkg.join("package.json"),
            r#"{"name": "prettier", "bin": "./bin/prettier.cjs"}"#,
        )
        .unwrap();

        // Ensure node_modules exists for the link
        link_into_node_modules(project.path(), "prettier", &cached_pkg).unwrap();

        // Link binaries
        let binaries =
            link_package_binaries(project.path(), "prettier", &cached_pkg, None).unwrap();

        assert_eq!(binaries.len(), 1);
        assert!(project.path().join("node_modules/.bin/prettier").exists());
    }

    #[test]
    fn test_link_package_binaries_object_form() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create a fake cached package with bin as object
        let cached_pkg = cache
            .path()
            .join("typescript")
            .join("5.0.0")
            .join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::create_dir_all(cached_pkg.join("bin")).unwrap();
        fs::write(
            cached_pkg.join("bin/tsc"),
            "#!/usr/bin/env node\nconsole.log('tsc');",
        )
        .unwrap();
        fs::write(
            cached_pkg.join("bin/tsserver"),
            "#!/usr/bin/env node\nconsole.log('tsserver');",
        )
        .unwrap();
        fs::write(
            cached_pkg.join("package.json"),
            r#"{"name": "typescript", "bin": {"tsc": "./bin/tsc", "tsserver": "./bin/tsserver"}}"#,
        )
        .unwrap();

        // Ensure node_modules exists for the link
        link_into_node_modules(project.path(), "typescript", &cached_pkg).unwrap();

        // Link binaries
        let binaries =
            link_package_binaries(project.path(), "typescript", &cached_pkg, None).unwrap();

        assert_eq!(binaries.len(), 2);
        assert!(project.path().join("node_modules/.bin/tsc").exists());
        assert!(project.path().join("node_modules/.bin/tsserver").exists());
    }

    #[test]
    fn test_link_package_binaries_no_bin_field() {
        let project = tempdir().unwrap();
        let cache = tempdir().unwrap();

        // Create a package without bin field
        let cached_pkg = cache.path().join("lodash").join("4.0.0").join("package");
        fs::create_dir_all(&cached_pkg).unwrap();
        fs::write(cached_pkg.join("package.json"), r#"{"name": "lodash"}"#).unwrap();

        // Ensure node_modules exists for the link
        link_into_node_modules(project.path(), "lodash", &cached_pkg).unwrap();

        // Link binaries - should return empty vec
        let binaries = link_package_binaries(project.path(), "lodash", &cached_pkg, None).unwrap();

        assert!(binaries.is_empty());
        // .bin directory might exist from pnpm setup, that's ok
    }

    #[test]
    fn test_format_pnpm_key() {
        assert_eq!(format_pnpm_key("lodash", "4.17.21"), "lodash@4.17.21");
        assert_eq!(
            format_pnpm_key("@types/node", "20.0.0"),
            "@types+node@20.0.0"
        );
        assert_eq!(
            format_pnpm_key("@babel/core", "7.23.0"),
            "@babel+core@7.23.0"
        );
    }

    #[test]
    fn test_link_direct_for_workspace() {
        let project = tempdir().unwrap();
        let workspace_pkg = tempdir().unwrap();

        // Create a workspace package
        fs::write(
            workspace_pkg.path().join("package.json"),
            r#"{"name": "my-lib"}"#,
        )
        .unwrap();

        // Link directly (workspace style)
        let link_path =
            link_into_node_modules_direct(project.path(), "my-lib", workspace_pkg.path()).unwrap();

        assert!(link_path.exists());
        assert_eq!(
            link_path,
            project.path().join("node_modules").join("my-lib")
        );
        assert!(link_path.join("package.json").exists());

        // Should NOT have pnpm structure for direct links
        assert!(!project.path().join("node_modules/.pnpm").exists());
    }
}
