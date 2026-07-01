//! Capability-based permissions for the howth runtime.
//!
//! howth is a Node-compatible runtime, so the default posture is **allow-all**
//! (Node code assumes unrestricted access to fs/net/env/subprocesses). Users
//! opt *into* a sandbox with `--sandbox` (deny-by-default) plus `--allow-*`
//! grants, or selectively revoke with `--deny-*`.
//!
//! Permissions are set once at startup and are read-only thereafter, so they
//! live in a process-global `OnceLock` — the same idiom the runtime already
//! uses for `SCRIPT_ARGS`. Boundary ops gate themselves with a single call:
//!
//! ```ignore
//! permissions::get().check_read(path)?;
//! ```
//!
//! Per-worker permission sets (for multi-tenant isolation) would instead live
//! in `OpState`; that is a deliberate future refinement, not what ships here.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use deno_core::error::AnyError;

/// A single capability's grant.
#[derive(Debug, Clone)]
enum Scope {
    /// Unrestricted (the Node-compatible default).
    All,
    /// Restricted to an explicit allowlist. For paths, entries are normalized
    /// absolute prefixes; for net, `host` or `host:port`; for env/run, exact keys.
    List(HashSet<String>),
    /// Fully denied.
    None,
}

impl Scope {
    /// Build a scope from an optional allowlist:
    /// - `None`         → `Scope::All`   (flag absent → capability unrestricted)
    /// - `Some(empty)`  → `Scope::All`   (`--allow-read` with no value → all)
    /// - `Some(values)` → `Scope::List`  (`--allow-read=/a,/b` → allowlist)
    fn from_allow(values: Option<Vec<String>>) -> Self {
        match values {
            None => Scope::All,
            Some(v) if v.is_empty() => Scope::All,
            Some(v) => Scope::List(v.into_iter().collect()),
        }
    }
}

/// The full permission set for a runtime process.
#[derive(Debug, Clone)]
pub struct Permissions {
    read: Scope,
    write: Scope,
    net: Scope,
    run: Scope,
    env: Scope,
}

impl Default for Permissions {
    /// Allow-all — the Node-compatible default. Nothing is gated until the user
    /// asks for a sandbox.
    fn default() -> Self {
        Permissions {
            read: Scope::All,
            write: Scope::All,
            net: Scope::All,
            run: Scope::All,
            env: Scope::All,
        }
    }
}

impl Permissions {
    /// Explicit allow-all (same as `default`), for readability at call sites.
    #[must_use]
    pub fn allow_all() -> Self {
        Permissions::default()
    }

    /// Sandboxed base: everything denied. Callers then grant with the
    /// `allow_*` builders. This is what `--sandbox` selects.
    #[must_use]
    pub fn sandboxed() -> Self {
        Permissions {
            read: Scope::None,
            write: Scope::None,
            net: Scope::None,
            run: Scope::None,
            env: Scope::None,
        }
    }

    /// Grant read access. `None` = all paths, `Some(list)` = allowlist.
    #[must_use]
    pub fn allow_read(mut self, paths: Option<Vec<String>>) -> Self {
        self.read = Scope::from_allow(paths.map(normalize_prefixes));
        self
    }

    /// Grant write access. `None` = all paths, `Some(list)` = allowlist.
    #[must_use]
    pub fn allow_write(mut self, paths: Option<Vec<String>>) -> Self {
        self.write = Scope::from_allow(paths.map(normalize_prefixes));
        self
    }

    /// Grant network access. `None` = all, `Some(list)` = `host`/`host:port` allowlist.
    #[must_use]
    pub fn allow_net(mut self, hosts: Option<Vec<String>>) -> Self {
        self.net = Scope::from_allow(hosts);
        self
    }

    /// Grant subprocess spawning. `None` = all, `Some(list)` = command allowlist.
    #[must_use]
    pub fn allow_run(mut self, cmds: Option<Vec<String>>) -> Self {
        self.run = Scope::from_allow(cmds);
        self
    }

    /// Grant env access. `None` = all, `Some(list)` = variable-name allowlist.
    #[must_use]
    pub fn allow_env(mut self, keys: Option<Vec<String>>) -> Self {
        self.env = Scope::from_allow(keys);
        self
    }

    /// Revoke a capability entirely (`--deny-*`). Applied after grants.
    #[must_use]
    pub fn deny_read(mut self) -> Self {
        self.read = Scope::None;
        self
    }
    #[must_use]
    pub fn deny_write(mut self) -> Self {
        self.write = Scope::None;
        self
    }
    #[must_use]
    pub fn deny_net(mut self) -> Self {
        self.net = Scope::None;
        self
    }
    #[must_use]
    pub fn deny_run(mut self) -> Self {
        self.run = Scope::None;
        self
    }
    #[must_use]
    pub fn deny_env(mut self) -> Self {
        self.env = Scope::None;
        self
    }

    // --- Checks (called from boundary ops) ---

    /// Permit reading `path`, or return a descriptive error naming the flag to add.
    pub fn check_read(&self, path: &str) -> Result<(), AnyError> {
        check_path(&self.read, path, "read from", "--allow-read")
    }

    /// Permit writing `path`, or error.
    pub fn check_write(&self, path: &str) -> Result<(), AnyError> {
        check_path(&self.write, path, "write to", "--allow-write")
    }

    /// Permit connecting/binding to `host:port`, or error.
    pub fn check_net(&self, host: &str, port: u16) -> Result<(), AnyError> {
        match &self.net {
            Scope::All => Ok(()),
            Scope::None => Err(denied(&format!("network access to \"{host}:{port}\""), "--allow-net")),
            Scope::List(set) => {
                if set.contains(host) || set.contains(&format!("{host}:{port}")) {
                    Ok(())
                } else {
                    Err(denied(
                        &format!("network access to \"{host}:{port}\""),
                        "--allow-net",
                    ))
                }
            }
        }
    }

    /// Permit host-level network access (DNS resolution, or a fetch whose port
    /// is implied by scheme). Matches a bare-host grant or any `host:port` grant
    /// for that host.
    pub fn check_net_host(&self, host: &str) -> Result<(), AnyError> {
        match &self.net {
            Scope::All => Ok(()),
            Scope::None => Err(denied(&format!("network access to \"{host}\""), "--allow-net")),
            Scope::List(set) => {
                let prefix = format!("{host}:");
                if set.contains(host) || set.iter().any(|e| e.starts_with(&prefix)) {
                    Ok(())
                } else {
                    Err(denied(&format!("network access to \"{host}\""), "--allow-net"))
                }
            }
        }
    }

    /// Permit spawning `cmd`, or error. Matches the allowlist against the full
    /// command, its basename, its first whitespace-delimited token (the program
    /// name in a shell string like `"echo hi"`), and that token's basename.
    pub fn check_run(&self, cmd: &str) -> Result<(), AnyError> {
        match &self.run {
            Scope::All => Ok(()),
            Scope::None => Err(denied(&format!("running subprocess \"{cmd}\""), "--allow-run")),
            Scope::List(set) => {
                let basename = |s: &str| {
                    Path::new(s)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(s)
                        .to_string()
                };
                let first_token = cmd.split_whitespace().next().unwrap_or(cmd);
                let candidates = [
                    cmd.to_string(),
                    basename(cmd),
                    first_token.to_string(),
                    basename(first_token),
                ];
                if candidates.iter().any(|c| set.contains(c)) {
                    Ok(())
                } else {
                    Err(denied(
                        &format!("running subprocess \"{cmd}\""),
                        "--allow-run",
                    ))
                }
            }
        }
    }

    /// Permit reading/writing env var `key`, or error.
    pub fn check_env(&self, key: &str) -> Result<(), AnyError> {
        match &self.env {
            Scope::All => Ok(()),
            Scope::None => Err(denied(&format!("access to env variable \"{key}\""), "--allow-env")),
            Scope::List(set) => {
                if set.contains(key) {
                    Ok(())
                } else {
                    Err(denied(&format!("access to env variable \"{key}\""), "--allow-env"))
                }
            }
        }
    }
}

/// Process-global permission set. Installed once by `init` before any JS runs.
static PERMISSIONS: OnceLock<Permissions> = OnceLock::new();

/// Install the process permission set. Call once, before executing user code.
/// Subsequent calls are ignored (the first set wins), matching `SCRIPT_ARGS`.
pub fn init(perms: Permissions) {
    let _ = PERMISSIONS.set(perms);
}

/// Get the active permissions. Falls back to allow-all if `init` was never
/// called (e.g. embedded/library use), preserving Node-compatible behavior.
pub fn get() -> &'static Permissions {
    static DEFAULT: OnceLock<Permissions> = OnceLock::new();
    PERMISSIONS
        .get()
        .unwrap_or_else(|| DEFAULT.get_or_init(Permissions::default))
}

/// Check a path against a path scope. Allowlist entries are normalized absolute
/// prefixes; access is granted if the (normalized) target is under any of them.
fn check_path(scope: &Scope, path: &str, action: &str, flag: &str) -> Result<(), AnyError> {
    match scope {
        Scope::All => Ok(()),
        Scope::None => Err(denied(&format!("{action} \"{path}\""), flag)),
        Scope::List(set) => {
            let target = normalize(path);
            if set.iter().any(|allowed| target.starts_with(Path::new(allowed))) {
                Ok(())
            } else {
                Err(denied(&format!("{action} \"{path}\""), flag))
            }
        }
    }
}

/// Build a `PermissionDenied` error whose message tells the user which flag grants access.
fn denied(what: &str, flag: &str) -> AnyError {
    AnyError::msg(format!(
        "PermissionDenied: requires {what}. Run with {flag} to grant access."
    ))
}

/// Normalize a set of path prefixes to absolute, lexically-cleaned form for storage.
fn normalize_prefixes(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .map(|p| normalize(&p).to_string_lossy().into_owned())
        .collect()
}

/// Resolve a path to an absolute, lexically-cleaned `PathBuf` without touching
/// the filesystem (no symlink resolution — purely syntactic, like the resolver's
/// `normalize_path`). This keeps checks cheap and side-effect free.
fn normalize(p: &str) -> PathBuf {
    let path = Path::new(p);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_all_default_permits_everything() {
        let p = Permissions::allow_all();
        assert!(p.check_read("/etc/passwd").is_ok());
        assert!(p.check_write("/tmp/x").is_ok());
        assert!(p.check_net("example.com", 443).is_ok());
        assert!(p.check_run("rm").is_ok());
        assert!(p.check_env("HOME").is_ok());
    }

    #[test]
    fn sandboxed_denies_everything() {
        let p = Permissions::sandboxed();
        assert!(p.check_read("/tmp/x").is_err());
        assert!(p.check_write("/tmp/x").is_err());
        assert!(p.check_net("example.com", 443).is_err());
        assert!(p.check_run("rm").is_err());
        assert!(p.check_env("HOME").is_err());
    }

    #[test]
    fn read_allowlist_matches_by_prefix() {
        let p = Permissions::sandboxed().allow_read(Some(vec!["/app".into()]));
        assert!(p.check_read("/app/src/index.js").is_ok());
        assert!(p.check_read("/app").is_ok());
        assert!(p.check_read("/etc/passwd").is_err());
        // A sibling that merely shares a name prefix must NOT match.
        assert!(p.check_read("/application/secret").is_err());
    }

    #[test]
    fn net_allowlist_matches_host_or_host_port() {
        let p = Permissions::sandboxed().allow_net(Some(vec![
            "api.example.com".into(),
            "db.internal:5432".into(),
        ]));
        assert!(p.check_net("api.example.com", 443).is_ok()); // host-only grant, any port
        assert!(p.check_net("db.internal", 5432).is_ok()); // exact host:port
        assert!(p.check_net("db.internal", 5433).is_err()); // wrong port
        assert!(p.check_net("evil.com", 443).is_err());
    }

    #[test]
    fn run_allowlist_matches_basename() {
        let p = Permissions::sandboxed().allow_run(Some(vec!["git".into()]));
        assert!(p.check_run("git").is_ok());
        assert!(p.check_run("/usr/bin/git").is_ok());
        assert!(p.check_run("rm").is_err());
    }

    #[test]
    fn run_allowlist_matches_shell_command_first_token() {
        // execSync("echo hi") passes the whole shell string; the program is the first token.
        let p = Permissions::sandboxed().allow_run(Some(vec!["echo".into()]));
        assert!(p.check_run("echo hi").is_ok());
        assert!(p.check_run("/bin/echo hi there").is_ok());
        assert!(p.check_run("rm -rf /").is_err());
    }

    #[test]
    fn env_allowlist_is_exact() {
        let p = Permissions::sandboxed().allow_env(Some(vec!["PATH".into()]));
        assert!(p.check_env("PATH").is_ok());
        assert!(p.check_env("SECRET").is_err());
    }

    #[test]
    fn empty_allow_list_means_all() {
        // `--allow-read` with no value grants everything.
        let p = Permissions::sandboxed().allow_read(Some(vec![]));
        assert!(p.check_read("/anywhere").is_ok());
    }
}
