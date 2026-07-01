//! OS-level sandboxing for the Node.js subprocess path (`howth run --node`,
//! and non-`native-runtime` builds).
//!
//! howth's op-level permission gates only bind code running in howth's own V8
//! runtime. When execution is delegated to the real `node` binary, the only way
//! to enforce `--sandbox`/`--allow-*`/`--deny-*` is to constrain the *process*
//! from the outside:
//!
//! - **env** — enforced in-process by controlling the child's environment
//!   (`Command::env_clear`/`env`). Works on every platform.
//! - **fs / net / run** — enforced by the OS sandbox: `sandbox-exec` (macOS) or
//!   `bwrap` (Linux, if installed). Where no mechanism is available we emit a
//!   clear warning rather than pretending to sandbox.
//!
//! Precedence matches the runtime's `build_permissions`: base (`--sandbox` =
//! deny-all, else allow-all) → `--allow-*` → `--deny-*` (deny wins).

use std::path::Path;
use std::process::Command;

use crate::PermissionFlags;

/// Resolution of one capability after applying base/allow/deny precedence.
enum Access {
    /// Unrestricted.
    All,
    /// Restricted to an explicit list (paths, hosts, or env-var names).
    List(Vec<String>),
    /// Fully denied.
    None,
}

impl Access {
    fn resolve(
        sandbox: bool,
        allow: &Option<String>,
        deny: bool,
    ) -> Access {
        let mut acc = if sandbox { Access::None } else { Access::All };
        if let Some(v) = allow {
            acc = if v.trim().is_empty() {
                Access::All
            } else {
                Access::List(
                    v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                )
            };
        }
        if deny {
            acc = Access::None;
        }
        acc
    }
}

/// Build a `Command` that runs `node <file> <args...>` constrained by `flags`.
///
/// `file` is the (already resolved / transpiled) JS file to execute. Returns a
/// ready-to-spawn command: either a bare `node`, or `node` wrapped by the
/// platform sandbox. Always applies env restrictions in-process.
pub fn node_command(
    node_bin: &Path,
    file: &Path,
    args: &[String],
    cwd: &Path,
    flags: &PermissionFlags,
) -> Command {
    let read = Access::resolve(flags.sandbox, &flags.allow_read, flags.deny_read);
    let write = Access::resolve(flags.sandbox, &flags.allow_write, flags.deny_write);
    let net = Access::resolve(flags.sandbox, &flags.allow_net, flags.deny_net);
    let run = Access::resolve(flags.sandbox, &flags.allow_run, flags.deny_run);

    let mut cmd = if !flags.any_set() {
        let mut c = Command::new(node_bin);
        c.arg(file).args(args);
        c
    } else {
        os_wrapped_command(node_bin, file, args, cwd, &read, &write, &net, &run)
    };

    apply_env(&mut cmd, flags);
    cmd
}

/// Restrict the child's environment per the env capability.
fn apply_env(cmd: &mut Command, flags: &PermissionFlags) {
    match Access::resolve(flags.sandbox, &flags.allow_env, flags.deny_env) {
        Access::All => {} // inherit everything (default)
        Access::None => {
            cmd.env_clear();
        }
        Access::List(keys) => {
            cmd.env_clear();
            for key in keys {
                if let Ok(val) = std::env::var(&key) {
                    cmd.env(key, val);
                }
            }
        }
    }
}

// ── macOS: sandbox-exec ─────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn os_wrapped_command(
    node_bin: &Path,
    file: &Path,
    args: &[String],
    cwd: &Path,
    read: &Access,
    write: &Access,
    net: &Access,
    run: &Access,
) -> Command {
    let profile = macos_profile(node_bin, file, cwd, read, write, net, run);
    let mut c = Command::new("/usr/bin/sandbox-exec");
    c.arg("-p").arg(profile).arg(node_bin).arg(file).args(args);
    c
}

/// Generate a Seatbelt (SBPL) profile: deny-by-default, allowing the minimum
/// system access `node` needs to start plus whatever the flags grant.
#[cfg(target_os = "macos")]
fn macos_profile(
    node_bin: &Path,
    file: &Path,
    cwd: &Path,
    read: &Access,
    write: &Access,
    net: &Access,
    run: &Access,
) -> String {
    let mut p = String::from("(version 1)\n(deny default)\n");

    // Baseline the runtime needs to even boot node/dyld. These grant no
    // filesystem/network/exec reach on their own — node aborts (SIGABRT/71)
    // without mach/iokit/process-info, so they are mandatory.
    p.push_str("(allow process-fork)\n");
    p.push_str("(allow process-info*)\n");
    p.push_str("(allow signal (target self))\n");
    p.push_str("(allow sysctl-read)\n");
    p.push_str("(allow mach*)\n");
    p.push_str("(allow iokit*)\n");
    p.push_str("(allow file-read-metadata)\n");
    p.push_str("(allow file-ioctl)\n");

    // Node reads the root directory entry itself at startup — without this it
    // aborts (SIGABRT) even when every subpath below is allowed.
    p.push_str("(allow file-read* (literal \"/\"))\n");
    // System paths required for the binary + dynamic linker + crypto/DNS.
    // Deliberately NOT /private/tmp or /Users — those stay denied so secrets
    // there require an explicit --allow-read grant. `/private/var` is narrowed
    // to the subtrees node needs (dyld cache, temp, select) to avoid exposing
    // user data elsewhere under /private/var.
    for sys in [
        "/usr", "/System", "/Library", "/bin", "/sbin", "/opt", "/dev",
        "/private/etc", "/private/var/db", "/private/var/folders", "/private/var/select",
    ] {
        p.push_str(&format!("(allow file-read* (subpath \"{sys}\"))\n"));
    }
    // Node writes to the OS temp dir (transpiled files, sockets) and to /dev
    // (tty/null). These are runtime plumbing, not user-visible writes.
    p.push_str("(allow file-write* (subpath \"/private/var/folders\"))\n");
    p.push_str("(allow file-write* (subpath \"/dev\"))\n");

    // Always allow executing the node binary itself (sandbox-exec's own
    // execvp needs it) and reading node's install prefix + the entry file,
    // regardless of the fs/run grants — those gate *user* access, not bootstrap.
    p.push_str(&format!(
        "(allow process-exec* (literal {}))\n",
        sbpl_quote(&node_bin.to_string_lossy())
    ));
    p.push_str(&format!(
        "(allow file-read* (subpath {}))\n",
        sbpl_quote(&node_prefix(node_bin))
    ));
    p.push_str(&format!(
        "(allow file-read* (literal {}))\n",
        sbpl_quote(&file.to_string_lossy())
    ));

    // User-granted filesystem reads.
    match read {
        Access::All => p.push_str("(allow file-read*)\n"),
        Access::List(paths) => {
            for path in paths {
                p.push_str(&format!(
                    "(allow file-read* (subpath {}))\n",
                    sbpl_quote(&abs(path, cwd))
                ));
            }
        }
        Access::None => {}
    }

    // User-granted filesystem writes.
    match write {
        Access::All => p.push_str("(allow file-write*)\n"),
        Access::List(paths) => {
            for path in paths {
                p.push_str(&format!(
                    "(allow file-write* (subpath {}))\n",
                    sbpl_quote(&abs(path, cwd))
                ));
            }
        }
        Access::None => {}
    }

    // Network. SBPL cannot filter by hostname, so any net grant opens network*;
    // a fully-denied net capability blocks all sockets.
    if !matches!(net, Access::None) {
        p.push_str("(allow network*)\n");
    }

    // Subprocess execution (children spawned by node).
    if !matches!(run, Access::None) {
        p.push_str("(allow process-exec*)\n");
        p.push_str("(allow process-fork)\n");
    }

    p
}

/// Quote a string as an SBPL literal, escaping backslashes and double-quotes.
#[cfg(target_os = "macos")]
fn sbpl_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The node install prefix: the grandparent of `.../bin/node` (e.g. an nvm
/// version dir), which holds ICU data, shared libs, etc. Falls back to the
/// immediate parent, then `/`.
#[cfg(target_os = "macos")]
fn node_prefix(node_bin: &Path) -> String {
    node_bin
        .parent()
        .and_then(Path::parent)
        .or_else(|| node_bin.parent())
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string())
}

/// Resolve a possibly-relative grant path to absolute using `cwd`.
#[cfg(target_os = "macos")]
fn abs(path: &str, cwd: &Path) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        cwd.join(p).to_string_lossy().into_owned()
    }
}

// ── Linux: bubblewrap if available, else warn ───────────────────────────────

#[cfg(target_os = "linux")]
fn os_wrapped_command(
    node_bin: &Path,
    file: &Path,
    args: &[String],
    cwd: &Path,
    read: &Access,
    write: &Access,
    net: &Access,
    run: &Access,
) -> Command {
    if which::which("bwrap").is_err() {
        eprintln!(
            "warning: OS sandbox unavailable (bubblewrap `bwrap` not found on PATH); \
             --sandbox/--allow-*/--deny-* filesystem & network limits are NOT enforced for the \
             node subprocess. env limits still apply. Install bubblewrap or use the native runtime."
        );
        let mut c = Command::new(node_bin);
        c.arg(file).args(args);
        return c;
    }

    let mut c = Command::new("bwrap");
    // Deny-by-default: only bind what is granted. Read-only binds for reads,
    // read-write binds for writes.
    c.arg("--unshare-all");
    if !matches!(net, Access::None) {
        c.arg("--share-net");
    }
    // Baseline system paths (read-only) so node can start.
    for sys in ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"] {
        if Path::new(sys).exists() {
            c.arg("--ro-bind").arg(sys).arg(sys);
        }
    }
    c.arg("--proc").arg("/proc").arg("--dev").arg("/dev");
    // node binary + entry file must be reachable.
    c.arg("--ro-bind").arg(node_bin).arg(node_bin);
    c.arg("--ro-bind").arg(file).arg(file);

    match read {
        Access::All => {
            c.arg("--ro-bind").arg("/").arg("/");
        }
        Access::List(paths) => {
            for path in paths {
                let ap = abs_path(path, cwd);
                c.arg("--ro-bind-try").arg(&ap).arg(&ap);
            }
        }
        Access::None => {}
    }
    match write {
        Access::All => {
            c.arg("--bind").arg("/").arg("/");
        }
        Access::List(paths) => {
            for path in paths {
                let ap = abs_path(path, cwd);
                c.arg("--bind-try").arg(&ap).arg(&ap);
            }
        }
        Access::None => {}
    }
    let _ = run; // bwrap cannot selectively allow specific child binaries; exec follows fs binds.

    c.arg(node_bin).arg(file).args(args);
    c
}

#[cfg(target_os = "linux")]
fn abs_path(path: &str, cwd: &Path) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        path.to_string()
    } else {
        cwd.join(p).to_string_lossy().into_owned()
    }
}

// ── Other platforms: no OS sandbox ──────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn os_wrapped_command(
    node_bin: &Path,
    file: &Path,
    args: &[String],
    _cwd: &Path,
    _read: &Access,
    _write: &Access,
    _net: &Access,
    _run: &Access,
) -> Command {
    eprintln!(
        "warning: OS sandbox unavailable on this platform; --sandbox/--allow-*/--deny-* \
         filesystem & network limits are NOT enforced for the node subprocess. env limits \
         still apply. Use the native runtime for full enforcement."
    );
    let mut c = Command::new(node_bin);
    c.arg(file).args(args);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags() -> PermissionFlags {
        PermissionFlags::default()
    }

    #[test]
    fn access_precedence_base_allow_deny() {
        // allow-all base
        assert!(matches!(
            Access::resolve(false, &None, false),
            Access::All
        ));
        // sandbox base = deny
        assert!(matches!(
            Access::resolve(true, &None, false),
            Access::None
        ));
        // allow with list overrides sandbox base
        assert!(matches!(
            Access::resolve(true, &Some("/a,/b".into()), false),
            Access::List(v) if v.len() == 2
        ));
        // empty allow value = all
        assert!(matches!(
            Access::resolve(true, &Some(String::new()), false),
            Access::All
        ));
        // deny wins over allow
        assert!(matches!(
            Access::resolve(false, &Some("/a".into()), true),
            Access::None
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_profile_denies_sensitive_and_allows_grants() {
        let node = Path::new("/opt/node/bin/node");
        let entry = Path::new("/tmp/app.js");
        let cwd = Path::new("/work");
        let prof = macos_profile(
            node,
            entry,
            cwd,
            &Access::List(vec!["/work/data".into()]),
            &Access::None,
            &Access::None,
            &Access::None,
        );
        // The root-dir read that node needs to boot must be present.
        assert!(prof.contains("(allow file-read* (literal \"/\"))"));
        // Granted read path is allowed (as an absolute subpath).
        assert!(prof.contains("(allow file-read* (subpath \"/work/data\"))"));
        // Sensitive trees are NOT blanket-allowed for reads.
        assert!(!prof.contains("(subpath \"/private/tmp\")"));
        assert!(!prof.contains("(subpath \"/Users\")"));
        // Denied net/run/write emit no broad allow.
        assert!(!prof.contains("(allow network*)"));
        assert!(!prof.contains("(allow process-exec*)\n")); // only the node literal, not broad
        assert!(!prof.contains("(allow file-write*)\n"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_profile_opens_up_on_grants() {
        let node = Path::new("/opt/node/bin/node");
        let entry = Path::new("/tmp/app.js");
        let cwd = Path::new("/work");
        let prof = macos_profile(
            node, entry, cwd,
            &Access::All, &Access::All, &Access::All, &Access::All,
        );
        assert!(prof.contains("(allow file-read*)\n"));
        assert!(prof.contains("(allow file-write*)\n"));
        assert!(prof.contains("(allow network*)"));
        assert!(prof.contains("(allow process-exec*)\n"));
    }

    #[test]
    fn no_flags_produces_bare_node() {
        // Not asserting the Command internals (no public accessor); just ensure
        // the no-op path doesn't panic and any_set stays false.
        let f = flags();
        assert!(!f.any_set());
        let _ = node_command(Path::new("/usr/bin/node"), Path::new("/tmp/a.js"), &[], Path::new("/"), &f);
    }
}
