//! Data collectors for doctor report.
//!
//! All collectors use only file reads and syscalls.
//! No subprocesses are spawned.

use super::capabilities::{
    detect_case_sensitivity, detect_hardlink_support, detect_symlink_support,
};
use super::{
    codes, Capabilities, HardwareInfo, OsInfo, PathsInfo, ProjectInfo, RuntimeInfo, Warning,
};
use crate::config::Channel;
use crate::paths;
use crate::version::{SCHEMA_VERSION, VERSION};
use std::fs;
use std::path::Path;

/// Collect runtime information.
#[must_use]
pub fn collect_runtime(channel: Channel) -> RuntimeInfo {
    RuntimeInfo {
        version: VERSION.to_string(),
        schema_version: SCHEMA_VERSION,
        channel: channel.as_str().to_string(),
    }
}

/// Collect OS information.
#[must_use]
pub fn collect_os(warnings: &mut Vec<Warning>) -> OsInfo {
    let name = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    let version = get_os_version();

    if version.is_none() {
        warnings.push(Warning::info(
            codes::UNKNOWN_OS_VERSION,
            "Could not determine OS version",
        ));
    }

    OsInfo {
        name,
        version,
        arch,
    }
}

/// Get OS version string via file reads (no subprocess).
#[cfg(target_os = "macos")]
fn get_os_version() -> Option<String> {
    // Read SystemVersion.plist (no subprocess!)
    // This file contains ProductVersion like "14.2.1"
    let plist_path = "/System/Library/CoreServices/SystemVersion.plist";
    let content = fs::read_to_string(plist_path).ok()?;

    // Simple XML parsing for ProductVersion
    // Format: <key>ProductVersion</key>\n<string>14.2.1</string>
    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        if line.contains("<key>ProductVersion</key>") {
            if let Some(version_line) = lines.next() {
                // Extract version from <string>X.Y.Z</string>
                let version = version_line
                    .trim()
                    .trim_start_matches("<string>")
                    .trim_end_matches("</string>");
                if !version.is_empty() && !version.contains('<') {
                    return Some(version.to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn get_os_version() -> Option<String> {
    // Try /etc/os-release (no subprocess!)
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            for line in content.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    return Some(
                        line.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string(),
                    );
                }
            }
            None
        })
}

#[cfg(target_os = "windows")]
fn get_os_version() -> Option<String> {
    // Would need WinAPI calls for proper version detection
    // Omit for now rather than spawn subprocess
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn get_os_version() -> Option<String> {
    None
}

/// Collect hardware information.
#[must_use]
pub fn collect_hardware() -> HardwareInfo {
    HardwareInfo {
        cpu_cores: std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1),
        cpu_cores_physical: None, // Would need sys-info crate for this
    }
}

/// Collect path information.
#[must_use]
pub fn collect_paths(cwd: &Path, channel: Channel, warnings: &mut Vec<Warning>) -> PathsInfo {
    let cache_dir = paths::cache_dir(channel);
    let data_dir = paths::data_dir(channel);

    let cache_writable = is_dir_writable(&cache_dir);
    let data_writable = is_dir_writable(&data_dir);

    if !cache_writable {
        warnings.push(Warning::warn(
            codes::CACHE_NOT_WRITABLE,
            format!("Cache directory is not writable: {}", cache_dir.display()),
        ));
    }

    if !data_writable {
        warnings.push(Warning::warn(
            codes::DATA_NOT_WRITABLE,
            format!("Data directory is not writable: {}", data_dir.display()),
        ));
    }

    PathsInfo {
        cwd: cwd.to_path_buf(),
        cache_dir,
        data_dir,
        cache_writable,
        data_writable,
    }
}

/// Check if a directory is writable (creates it if needed).
fn is_dir_writable(dir: &Path) -> bool {
    // Try to create the directory
    if fs::create_dir_all(dir).is_err() {
        return false;
    }

    // Try to write a test file
    let test_file = dir.join(".fastnode_write_test");
    let result = fs::write(&test_file, b"test").is_ok();
    let _ = fs::remove_file(&test_file);
    result
}

/// Collect project information.
#[must_use]
pub fn collect_project(cwd: &Path, warnings: &mut Vec<Warning>) -> ProjectInfo {
    let root = paths::project_root(cwd);

    if root.is_none() {
        warnings.push(Warning::info(
            codes::PROJECT_ROOT_NOT_FOUND,
            "No project root found (no package.json or .git)",
        ));
    }

    let (has_package_json, has_git) = root.as_ref().map_or((false, false), |r| {
        (r.join("package.json").exists(), r.join(".git").exists())
    });

    ProjectInfo {
        root,
        has_package_json,
        has_git,
    }
}

/// Collect filesystem capabilities.
#[must_use]
pub fn collect_capabilities(warnings: &mut Vec<Warning>) -> Capabilities {
    let temp_dir = std::env::temp_dir();

    let fs_case_sensitive = detect_case_sensitivity(&temp_dir);
    let symlink_supported = detect_symlink_support(&temp_dir);
    let hardlink_supported = detect_hardlink_support(&temp_dir);

    if !fs_case_sensitive {
        warnings.push(Warning::info(
            codes::FS_CASE_INSENSITIVE,
            "Filesystem is case-insensitive (may cause issues with some npm packages)",
        ));
    }

    if !symlink_supported {
        warnings.push(Warning::warn(
            codes::SYMLINK_UNAVAILABLE,
            "Symlinks are not supported (package installation may be slower)",
        ));
    }

    if !hardlink_supported {
        warnings.push(Warning::warn(
            codes::HARDLINK_UNAVAILABLE,
            "Hardlinks are not supported (package installation may use more disk space)",
        ));
    }

    #[cfg(unix)]
    let rlimit_nofile = get_rlimit_nofile(warnings);

    #[cfg(target_os = "linux")]
    let io_uring_supported = super::capabilities::detect_io_uring_support();

    Capabilities {
        fs_case_sensitive,
        symlink_supported,
        hardlink_supported,
        #[cfg(target_os = "linux")]
        io_uring_supported,
        #[cfg(unix)]
        rlimit_nofile,
    }
}

/// Get file descriptor limits (Unix only).
#[cfg(unix)]
fn get_rlimit_nofile(warnings: &mut Vec<Warning>) -> Option<super::RlimitInfo> {
    use std::mem::MaybeUninit;

    let mut rlim = MaybeUninit::<libc::rlimit>::uninit();

    // SAFETY: rlim is valid pointer, RLIMIT_NOFILE is valid resource
    let result = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, rlim.as_mut_ptr()) };

    if result == 0 {
        // SAFETY: getrlimit succeeded, rlim is initialized
        let rlim = unsafe { rlim.assume_init() };
        let soft = rlim.rlim_cur;
        let hard = rlim.rlim_max;

        // Warn if soft limit is low (< 1024)
        if soft < 1024 {
            warnings.push(Warning::warn(
                codes::LOW_NOFILE_LIMIT,
                format!("Low file descriptor limit ({soft}). Consider increasing with 'ulimit -n'"),
            ));
        }

        Some(super::RlimitInfo { soft, hard })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_collect_runtime() {
        let info = collect_runtime(Channel::Stable);
        assert!(!info.version.is_empty());
        assert_eq!(info.channel, "stable");
    }

    #[test]
    fn test_collect_os() {
        let mut warnings = Vec::new();
        let info = collect_os(&mut warnings);
        assert!(!info.name.is_empty());
        assert!(!info.arch.is_empty());
    }

    #[test]
    fn test_collect_hardware() {
        let info = collect_hardware();
        assert!(info.cpu_cores >= 1);
    }

    #[test]
    fn test_collect_project_in_temp_dir() {
        let dir = tempdir().unwrap();
        let mut warnings = Vec::new();
        let info = collect_project(dir.path(), &mut warnings);

        // Temp dir shouldn't have package.json or .git
        assert!(info.root.is_none());
        assert!(!info.has_package_json);
        assert!(!info.has_git);
    }

    #[test]
    fn test_collect_project_with_package_json() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let mut warnings = Vec::new();
        let info = collect_project(dir.path(), &mut warnings);

        assert_eq!(info.root, Some(dir.path().to_path_buf()));
        assert!(info.has_package_json);
        assert!(!info.has_git);
    }
}
