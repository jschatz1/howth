//! Filesystem capability detection.
//!
//! All detection is done via file operations or syscalls.
//! No subprocesses are spawned.

#![allow(clippy::doc_markdown)]
#![allow(clippy::redundant_closure_for_method_calls)]

use std::fs;
use std::path::Path;

/// Detect if the filesystem at `dir` is case-sensitive.
///
/// Creates temporary files with different cases and checks if they're the same.
#[must_use]
pub fn detect_case_sensitivity(dir: &Path) -> bool {
    let lower = dir.join(".fastnode_case_test_lower");
    let upper = dir.join(".fastnode_case_test_LOWER");

    // Clean up any existing test files
    let _ = fs::remove_file(&lower);
    let _ = fs::remove_file(&upper);

    // Create lowercase file
    if fs::write(&lower, b"lower").is_err() {
        return true; // Assume case-sensitive if we can't write
    }

    // Try to create uppercase file
    let result = if fs::write(&upper, b"upper").is_ok() {
        // If we can create both, check if they have different contents
        let lower_content = fs::read(&lower).unwrap_or_default();
        let upper_content = fs::read(&upper).unwrap_or_default();
        lower_content != upper_content
    } else {
        // Couldn't create uppercase - might be case-insensitive collision
        false
    };

    // Clean up
    let _ = fs::remove_file(&lower);
    let _ = fs::remove_file(&upper);

    result
}

/// Detect if symlinks are supported at `dir`.
#[must_use]
pub fn detect_symlink_support(dir: &Path) -> bool {
    let target = dir.join(".fastnode_symlink_target");
    let link = dir.join(".fastnode_symlink_test");

    // Clean up
    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&target);

    // Create target
    if fs::write(&target, b"target").is_err() {
        return false;
    }

    // Try to create symlink
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(&target, &link).is_ok();

    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(&target, &link).is_ok();

    #[cfg(not(any(unix, windows)))]
    let result = false;

    // Clean up
    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&target);

    result
}

/// Detect if hardlinks are supported at `dir`.
#[must_use]
pub fn detect_hardlink_support(dir: &Path) -> bool {
    let original = dir.join(".fastnode_hardlink_original");
    let link = dir.join(".fastnode_hardlink_test");

    // Clean up
    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&original);

    // Create original
    if fs::write(&original, b"original").is_err() {
        return false;
    }

    // Try to create hardlink
    let result = fs::hard_link(&original, &link).is_ok();

    // Clean up
    let _ = fs::remove_file(&link);
    let _ = fs::remove_file(&original);

    result
}

/// Detect if io_uring is supported (Linux only).
///
/// Reads `/proc/sys/kernel/osrelease` to check kernel version.
/// io_uring was added in Linux 5.1.
///
/// Returns `None` if detection fails (e.g., can't read procfs).
#[cfg(target_os = "linux")]
#[must_use]
pub fn detect_io_uring_support() -> Option<bool> {
    // Read kernel version from procfs (no subprocess!)
    let version = fs::read_to_string("/proc/sys/kernel/osrelease").ok()?;
    let version = version.trim();

    // Parse major.minor from version string like "5.15.0-generic"
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let major: u32 = parts[0].parse().ok()?;
    let minor: u32 = parts[1]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;

    // io_uring was added in Linux 5.1
    Some(major > 5 || (major == 5 && minor >= 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_case_sensitivity() {
        let dir = tempdir().unwrap();
        let result = detect_case_sensitivity(dir.path());
        // Just ensure it doesn't panic and returns a bool
        let _ = result;
    }

    #[test]
    fn test_detect_symlink_support() {
        let dir = tempdir().unwrap();
        let result = detect_symlink_support(dir.path());
        // On most Unix systems this should be true
        #[cfg(unix)]
        assert!(result);
        let _ = result;
    }

    #[test]
    fn test_detect_hardlink_support() {
        let dir = tempdir().unwrap();
        let result = detect_hardlink_support(dir.path());
        // On most systems this should be true
        assert!(result);
    }

    #[test]
    fn test_no_leftover_files() {
        let dir = tempdir().unwrap();

        let _ = detect_case_sensitivity(dir.path());
        let _ = detect_symlink_support(dir.path());
        let _ = detect_hardlink_support(dir.path());

        // Ensure no test files are left behind
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert!(entries.is_empty(), "Test files were not cleaned up");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_io_uring_detection_returns_option() {
        // Should return Some(bool), not panic
        let result = detect_io_uring_support();
        // On any modern Linux, this should succeed
        assert!(result.is_some());
    }
}
