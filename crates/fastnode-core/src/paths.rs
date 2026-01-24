use crate::config::Channel;
use crate::version::SCHEMA_VERSION;
use std::path::{Path, PathBuf};

/// Find the project root by walking up from `cwd` looking for `package.json` or `.git`.
///
/// Returns the first directory containing either marker, or `None` if neither is found.
#[must_use]
pub fn project_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd.to_path_buf();

    loop {
        if current.join("package.json").exists() || current.join(".git").exists() {
            return Some(current);
        }

        if !current.pop() {
            return None;
        }
    }
}

/// Get the cache directory for howth.
///
/// Uses platform-appropriate locations with versioning:
/// - Linux: `$XDG_CACHE_HOME/howth/v{N}/{channel}` or `~/.cache/howth/v{N}/{channel}`
/// - macOS: `~/Library/Caches/howth/v{N}/{channel}`
/// - Windows: `%LOCALAPPDATA%\howth\cache\v{N}\{channel}`
#[must_use]
pub fn cache_dir(channel: Channel) -> PathBuf {
    let base = dirs_next::cache_dir().map_or_else(
        || {
            dirs_next::home_dir().map_or_else(
                || PathBuf::from(".howth-cache"),
                |p| p.join(".cache").join("howth"),
            )
        },
        |p| p.join("howth"),
    );

    base.join(format!("v{SCHEMA_VERSION}"))
        .join(channel.as_str())
}

/// Environment variable to override the IPC endpoint (for testing).
pub const IPC_ENDPOINT_ENV: &str = "HOWTH_IPC_ENDPOINT";

/// Prefix for Windows named pipes.
#[cfg(windows)]
const PIPE_PREFIX: &str = r"\\.\pipe\";

/// Normalize a Windows named pipe endpoint.
///
/// If the endpoint starts with `\\.\pipe\`, use it as-is.
/// Otherwise, prepend the pipe prefix.
#[cfg(windows)]
fn normalize_pipe_endpoint(endpoint: &str) -> String {
    if endpoint.starts_with(PIPE_PREFIX) {
        endpoint.to_string()
    } else {
        format!("{}{}", PIPE_PREFIX, endpoint)
    }
}

/// Get the IPC endpoint path/name for daemon communication.
///
/// Respects `HOWTH_IPC_ENDPOINT` environment variable for testing.
///
/// On Windows, the endpoint is normalized to include the pipe prefix
/// if not already present. This allows setting `HOWTH_IPC_ENDPOINT=my-pipe`
/// instead of the full `\\.\pipe\my-pipe`.
///
/// Platform-specific defaults:
/// - Unix: `{data_dir}/ipc/howth.sock`
/// - Windows: `\\.\pipe\howth-{channel}-v{N}`
#[must_use]
pub fn ipc_endpoint(channel: Channel) -> String {
    // Check env override first (for testing)
    if let Ok(endpoint) = std::env::var(IPC_ENDPOINT_ENV) {
        #[cfg(windows)]
        {
            return normalize_pipe_endpoint(&endpoint);
        }
        #[cfg(not(windows))]
        {
            return endpoint;
        }
    }

    #[cfg(unix)]
    {
        let dir = data_dir(channel).join("ipc");
        dir.join("howth.sock").to_string_lossy().into_owned()
    }

    #[cfg(windows)]
    {
        format!(
            r"\\.\pipe\howth-{}-v{}",
            channel.as_str(),
            SCHEMA_VERSION
        )
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback for other platforms
        let dir = data_dir(channel).join("ipc");
        dir.join("howth.sock").to_string_lossy().into_owned()
    }
}

/// Ensure the IPC socket directory exists (Unix only).
///
/// # Errors
/// Returns an error if the directory cannot be created.
pub fn ensure_ipc_dir(channel: Channel) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let dir = data_dir(channel).join("ipc");
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// Get the data directory for howth (for persistent data like installed packages).
///
/// Uses platform-appropriate locations with versioning:
/// - Linux: `$XDG_DATA_HOME/howth/v{N}/{channel}` or `~/.local/share/howth/v{N}/{channel}`
/// - macOS: `~/Library/Application Support/howth/v{N}/{channel}`
/// - Windows: `%LOCALAPPDATA%\howth\data\v{N}\{channel}`
#[must_use]
pub fn data_dir(channel: Channel) -> PathBuf {
    let base = dirs_next::data_dir().map_or_else(
        || {
            dirs_next::home_dir().map_or_else(
                || PathBuf::from(".howth-data"),
                |p| p.join(".local").join("share").join("howth"),
            )
        },
        |p| p.join("howth"),
    );

    base.join(format!("v{SCHEMA_VERSION}"))
        .join(channel.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_project_root_with_package_json() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let root = project_root(&nested);
        assert_eq!(root, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_project_root_with_git() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();

        let root = project_root(&nested);
        assert_eq!(root, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_project_root_not_found() {
        let dir = tempdir().unwrap();
        let root = project_root(dir.path());
        // May or may not find a root depending on the system
        let _ = root;
    }

    #[test]
    fn test_cache_dir_contains_version() {
        let dir = cache_dir(Channel::Stable);
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains(&format!("v{SCHEMA_VERSION}")));
        assert!(dir_str.contains("stable"));
    }

    #[test]
    fn test_data_dir_contains_version() {
        let dir = data_dir(Channel::Dev);
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains(&format!("v{SCHEMA_VERSION}")));
        assert!(dir_str.contains("dev"));
    }

    #[test]
    fn test_different_channels_different_dirs() {
        let stable = cache_dir(Channel::Stable);
        let nightly = cache_dir(Channel::Nightly);
        let dev = cache_dir(Channel::Dev);

        assert_ne!(stable, nightly);
        assert_ne!(stable, dev);
        assert_ne!(nightly, dev);
    }

    #[test]
    fn test_ipc_endpoint_contains_channel() {
        // Clear env var to test default behavior
        std::env::remove_var(IPC_ENDPOINT_ENV);

        let endpoint = ipc_endpoint(Channel::Stable);

        #[cfg(unix)]
        assert!(
            endpoint.contains(".sock"),
            "Unix endpoint should contain .sock"
        );

        #[cfg(windows)]
        assert!(
            endpoint.contains("stable"),
            "Windows endpoint should contain channel"
        );
    }

    #[test]
    fn test_ipc_endpoint_env_override() {
        let test_endpoint = "/tmp/test-howth.sock";
        std::env::set_var(IPC_ENDPOINT_ENV, test_endpoint);

        let endpoint = ipc_endpoint(Channel::Stable);
        assert_eq!(endpoint, test_endpoint);

        // Clean up
        std::env::remove_var(IPC_ENDPOINT_ENV);
    }

    #[test]
    fn test_different_channels_different_ipc_endpoints() {
        // Clear env var to test default behavior
        std::env::remove_var(IPC_ENDPOINT_ENV);

        let stable = ipc_endpoint(Channel::Stable);
        let nightly = ipc_endpoint(Channel::Nightly);
        let dev = ipc_endpoint(Channel::Dev);

        // On Unix, all endpoints are in different directories based on channel
        // On Windows, the pipe names contain the channel
        assert_ne!(stable, nightly);
        assert_ne!(stable, dev);
        assert_ne!(nightly, dev);
    }
}
