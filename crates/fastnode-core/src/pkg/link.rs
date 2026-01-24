//! Symlink/junction creation for `node_modules`.

use super::error::PkgError;
use std::fs;
use std::path::{Path, PathBuf};

/// Link a cached package into a project's `node_modules`.
///
/// Creates a symlink (Unix) or junction (Windows) from
/// `<project>/node_modules/<name>` to the cached package directory.
///
/// # Errors
/// Returns an error if the link cannot be created.
pub fn link_into_node_modules(
    project_root: &Path,
    pkg_name: &str,
    cached_pkg_dir: &Path,
) -> Result<PathBuf, PkgError> {
    let node_modules = project_root.join("node_modules");

    // Ensure node_modules exists
    fs::create_dir_all(&node_modules).map_err(|e| {
        PkgError::node_modules_write_failed(format!("Failed to create node_modules directory: {e}"))
    })?;

    // Determine link path
    let link_path = if pkg_name.starts_with('@') {
        // Scoped package: @scope/name
        let parts: Vec<&str> = pkg_name.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(PkgError::link_failed(format!(
                "Invalid scoped package name: {pkg_name}"
            )));
        }

        let scope_dir = node_modules.join(parts[0]);
        fs::create_dir_all(&scope_dir).map_err(|e| {
            PkgError::node_modules_write_failed(format!(
                "Failed to create scope directory {}: {e}",
                parts[0]
            ))
        })?;

        scope_dir.join(parts[1])
    } else {
        node_modules.join(pkg_name)
    };

    // Remove existing link/directory if present
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        remove_link_or_dir(&link_path)?;
    }

    // Create the link
    create_dir_link(cached_pkg_dir, &link_path)?;

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

        // Link into project
        let link_path = link_into_node_modules(project.path(), "react", &cached_pkg).unwrap();

        assert!(link_path.exists());
        assert_eq!(link_path, project.path().join("node_modules").join("react"));

        // Verify the link target is accessible
        assert!(link_path.join("package.json").exists());
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
        let link_path = link_into_node_modules(project.path(), "@types/node", &cached_pkg).unwrap();

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
}
