//! Runtime implementation using deno_core.

use crate::module_loader::HowthModuleLoader;
use deno_core::{extension, op2, JsRuntime, ModuleSpecifier, RuntimeOptions as DenoRuntimeOptions};
use std::cell::RefCell;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

/// Format an IO error in Node.js style.
/// Node.js format: `CODE: message, syscall 'path'`
fn format_fs_error(err: std::io::Error, syscall: &str, path: &str) -> deno_core::error::AnyError {
    // Map ErrorKind to error code, then check raw OS error for more specific codes
    let (code, message) = match err.kind() {
        ErrorKind::NotFound => ("ENOENT", "no such file or directory"),
        ErrorKind::PermissionDenied => ("EACCES", "permission denied"),
        ErrorKind::AlreadyExists => ("EEXIST", "file already exists"),
        ErrorKind::InvalidInput => ("EINVAL", "invalid argument"),
        _ => {
            // Check raw OS error for codes not covered by stable ErrorKind
            #[cfg(unix)]
            if let Some(os_err) = err.raw_os_error() {
                match os_err {
                    libc::ENOTDIR => return deno_core::error::AnyError::msg(format!(
                        "ENOTDIR: not a directory, {syscall} '{path}'"
                    )),
                    libc::EISDIR => return deno_core::error::AnyError::msg(format!(
                        "EISDIR: illegal operation on a directory, {syscall} '{path}'"
                    )),
                    libc::ENOTEMPTY => return deno_core::error::AnyError::msg(format!(
                        "ENOTEMPTY: directory not empty, {syscall} '{path}'"
                    )),
                    libc::EROFS => return deno_core::error::AnyError::msg(format!(
                        "EROFS: read-only file system, {syscall} '{path}'"
                    )),
                    libc::EFBIG => return deno_core::error::AnyError::msg(format!(
                        "EFBIG: file too large, {syscall} '{path}'"
                    )),
                    libc::EXDEV => return deno_core::error::AnyError::msg(format!(
                        "EXDEV: cross-device link not permitted, {syscall} '{path}'"
                    )),
                    libc::EMLINK => return deno_core::error::AnyError::msg(format!(
                        "EMLINK: too many links, {syscall} '{path}'"
                    )),
                    libc::ENAMETOOLONG => return deno_core::error::AnyError::msg(format!(
                        "ENAMETOOLONG: name too long, {syscall} '{path}'"
                    )),
                    libc::ELOOP => return deno_core::error::AnyError::msg(format!(
                        "ELOOP: too many levels of symbolic links, {syscall} '{path}'"
                    )),
                    _ => {}
                }
            }
            // Fallback to generic error
            return deno_core::error::AnyError::msg(format!(
                "{}, {syscall} '{path}'",
                err
            ));
        }
    };

    deno_core::error::AnyError::msg(format!("{code}: {message}, {syscall} '{path}'"))
}

/// Runtime error.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("JavaScript error: {0}")]
    JavaScript(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Runtime initialization failed: {0}")]
    Init(String),
}

/// Runtime configuration options.
#[derive(Debug, Clone, Default)]
pub struct RuntimeOptions {
    /// Main module path (for ES module resolution).
    pub main_module: Option<PathBuf>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
}

/// Shared state for the runtime.
#[derive(Default)]
pub struct RuntimeState {
    /// Exit code set by process.exit().
    pub exit_code: i32,
}

/// The howth JavaScript runtime.
pub struct Runtime {
    js_runtime: JsRuntime,
    state: Rc<RefCell<RuntimeState>>,
}

// Define our custom ops extension
extension!(
    howth_runtime,
    ops = [
        op_howth_print,
        op_howth_print_error,
        op_howth_read_file,
        op_howth_write_file,
        op_howth_cwd,
        op_howth_env_get,
        op_howth_env_set,
        op_howth_exit,
        op_howth_args,
        op_howth_fetch,
        op_howth_encode_utf8,
        op_howth_decode_utf8,
        op_howth_random_bytes,
        op_howth_random_uuid,
        op_howth_hash,
        op_howth_hrtime,
        op_howth_sleep,
        // File system ops
        op_howth_fs_exists,
        op_howth_fs_mkdir,
        op_howth_fs_readdir,
        op_howth_fs_stat,
        op_howth_fs_unlink,
        op_howth_fs_truncate,
        op_howth_fs_rmdir,
        op_howth_fs_rename,
        op_howth_fs_copy,
        op_howth_fs_append,
        op_howth_fs_read_bytes,
        op_howth_fs_write_bytes,
        op_howth_fs_realpath,
        op_howth_fs_chmod,
        op_howth_fs_access,
        // Child process ops
        op_howth_spawn_sync,
        op_howth_exec_sync,
    ],
);

/// Bootstrap JavaScript code to set up globals like console, process, etc.
const BOOTSTRAP_JS: &str = include_str!("bootstrap.js");

/// Print to stdout.
#[op2(fast)]
fn op_howth_print(#[string] msg: &str) {
    print!("{}", msg);
}

/// Print to stderr.
#[op2(fast)]
fn op_howth_print_error(#[string] msg: &str) {
    eprint!("{}", msg);
}

/// Read a file as string.
#[op2]
#[string]
fn op_howth_read_file(#[string] path: &str) -> Result<String, deno_core::error::AnyError> {
    std::fs::read_to_string(path).map_err(|e| format_fs_error(e, "open", path))
}

/// Write string to a file.
#[op2(fast)]
fn op_howth_write_file(
    #[string] path: &str,
    #[string] contents: &str,
) -> Result<(), deno_core::error::AnyError> {
    std::fs::write(path, contents).map_err(|e| format_fs_error(e, "open", path))
}

/// Check if a file or directory exists.
#[op2(fast)]
fn op_howth_fs_exists(#[string] path: &str) -> bool {
    std::path::Path::new(path).exists()
}

/// Create a directory.
#[op2(fast)]
fn op_howth_fs_mkdir(
    #[string] path: &str,
    recursive: bool,
) -> Result<(), deno_core::error::AnyError> {
    if recursive {
        std::fs::create_dir_all(path).map_err(|e| format_fs_error(e, "mkdir", path))
    } else {
        std::fs::create_dir(path).map_err(|e| format_fs_error(e, "mkdir", path))
    }
}

/// File/directory entry from readdir.
#[derive(serde::Serialize)]
pub struct DirEntry {
    pub name: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// Read directory contents.
#[op2]
#[serde]
fn op_howth_fs_readdir(#[string] path: &str) -> Result<Vec<DirEntry>, deno_core::error::AnyError> {
    let entries = std::fs::read_dir(path)
        .map_err(|e| format_fs_error(e, "scandir", path))?
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let file_type = entry.file_type().ok();
            DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_file: file_type.map(|ft| ft.is_file()).unwrap_or(false),
                is_directory: file_type.map(|ft| ft.is_dir()).unwrap_or(false),
                is_symlink: file_type.map(|ft| ft.is_symlink()).unwrap_or(false),
            }
        })
        .collect();
    Ok(entries)
}

/// File statistics.
#[derive(serde::Serialize)]
pub struct FileStat {
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub mode: u32,
    pub mtime_ms: f64,
    pub atime_ms: f64,
    pub ctime_ms: f64,
    pub birthtime_ms: f64,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
    pub uid: u32,
    pub gid: u32,
}

/// Get file statistics.
#[op2]
#[serde]
fn op_howth_fs_stat(
    #[string] path: &str,
    follow_symlinks: bool,
) -> Result<FileStat, deno_core::error::AnyError> {
    let syscall = if follow_symlinks { "stat" } else { "lstat" };
    let metadata = if follow_symlinks {
        std::fs::metadata(path).map_err(|e| format_fs_error(e, syscall, path))?
    } else {
        std::fs::symlink_metadata(path).map_err(|e| format_fs_error(e, syscall, path))?
    };

    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let atime = metadata
        .accessed()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    let ctime = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);

    Ok(FileStat {
        is_file: metadata.is_file(),
        is_directory: metadata.is_dir(),
        is_symlink: metadata.file_type().is_symlink(),
        size: metadata.len(),
        #[cfg(unix)]
        mode: metadata.mode(),
        #[cfg(not(unix))]
        mode: if metadata.is_dir() { 0o755 } else { 0o644 },
        mtime_ms: mtime,
        atime_ms: atime,
        ctime_ms: ctime,
        birthtime_ms: ctime, // Use ctime as birthtime fallback
        #[cfg(unix)]
        dev: metadata.dev(),
        #[cfg(not(unix))]
        dev: 0,
        #[cfg(unix)]
        ino: metadata.ino(),
        #[cfg(not(unix))]
        ino: 0,
        #[cfg(unix)]
        nlink: metadata.nlink(),
        #[cfg(not(unix))]
        nlink: 1,
        #[cfg(unix)]
        uid: metadata.uid(),
        #[cfg(not(unix))]
        uid: 0,
        #[cfg(unix)]
        gid: metadata.gid(),
        #[cfg(not(unix))]
        gid: 0,
    })
}

/// Delete a file.
#[op2(fast)]
fn op_howth_fs_unlink(#[string] path: &str) -> Result<(), deno_core::error::AnyError> {
    std::fs::remove_file(path).map_err(|e| format_fs_error(e, "unlink", path))
}

/// Truncate a file to a specific length.
#[op2(fast)]
fn op_howth_fs_truncate(
    #[string] path: &str,
    #[bigint] len: u64,
) -> Result<(), deno_core::error::AnyError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format_fs_error(e, "open", path))?;
    file.set_len(len)
        .map_err(|e| format_fs_error(e, "ftruncate", path))
}

/// Remove a directory.
#[op2(fast)]
fn op_howth_fs_rmdir(
    #[string] path: &str,
    recursive: bool,
) -> Result<(), deno_core::error::AnyError> {
    if recursive {
        std::fs::remove_dir_all(path).map_err(|e| format_fs_error(e, "rmdir", path))
    } else {
        std::fs::remove_dir(path).map_err(|e| format_fs_error(e, "rmdir", path))
    }
}

/// Rename/move a file or directory.
#[op2(fast)]
fn op_howth_fs_rename(
    #[string] old_path: &str,
    #[string] new_path: &str,
) -> Result<(), deno_core::error::AnyError> {
    std::fs::rename(old_path, new_path).map_err(|e| format_fs_error(e, "rename", old_path))
}

/// Copy a file.
#[op2(fast)]
fn op_howth_fs_copy(
    #[string] src: &str,
    #[string] dest: &str,
) -> Result<(), deno_core::error::AnyError> {
    std::fs::copy(src, dest)?;
    Ok(())
}

/// Append to a file.
#[op2(fast)]
fn op_howth_fs_append(
    #[string] path: &str,
    #[string] contents: &str,
) -> Result<(), deno_core::error::AnyError> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

/// Read file as bytes (returns base64).
#[op2]
#[string]
fn op_howth_fs_read_bytes(#[string] path: &str) -> Result<String, deno_core::error::AnyError> {
    use deno_core::serde_json::json;
    let bytes = std::fs::read(path)?;
    // Return as base64 for efficient transfer
    Ok(base64_encode(&bytes))
}

/// Write bytes to file (accepts base64).
#[op2(fast)]
fn op_howth_fs_write_bytes(
    #[string] path: &str,
    #[string] base64_data: &str,
) -> Result<(), deno_core::error::AnyError> {
    let bytes = base64_decode(base64_data)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Get the real path (resolving symlinks).
#[op2]
#[string]
fn op_howth_fs_realpath(#[string] path: &str) -> Result<String, deno_core::error::AnyError> {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.into())
}

/// Change file permissions (Unix only).
#[op2(fast)]
fn op_howth_fs_chmod(#[string] path: &str, mode: u32) -> Result<(), deno_core::error::AnyError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        // On Windows, we can only set read-only
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_readonly(mode & 0o222 == 0);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

/// Check file access permissions.
#[op2(fast)]
fn op_howth_fs_access(#[string] path: &str, mode: u32) -> Result<(), deno_core::error::AnyError> {
    let path = std::path::Path::new(path);

    // Mode flags: 0=exists, 1=execute, 2=write, 4=read
    if !path.exists() {
        return Err(deno_core::error::AnyError::msg(
            "ENOENT: no such file or directory",
        ));
    }

    let metadata = std::fs::metadata(path)?;

    // Check read access (mode & 4)
    if mode & 4 != 0 {
        // We can check by trying to open
        if std::fs::File::open(path).is_err() {
            return Err(deno_core::error::AnyError::msg("EACCES: permission denied"));
        }
    }

    // Check write access (mode & 2)
    if mode & 2 != 0 {
        if metadata.permissions().readonly() {
            return Err(deno_core::error::AnyError::msg("EACCES: permission denied"));
        }
    }

    Ok(())
}

// ============================================================================
// Child Process Operations
// ============================================================================

/// Result of a spawned process (for sync operations).
#[derive(serde::Serialize)]
pub struct SpawnSyncResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

/// Options for spawning a process.
#[derive(serde::Deserialize, Default)]
pub struct SpawnOptions {
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub shell: Option<bool>,
    pub encoding: Option<String>,
    pub timeout: Option<u64>,
    pub max_buffer: Option<usize>,
}

/// Spawn a process synchronously and wait for completion.
#[op2]
#[serde]
fn op_howth_spawn_sync(
    #[string] command: &str,
    #[serde] args: Vec<String>,
    #[serde] options: Option<SpawnOptions>,
) -> SpawnSyncResult {
    use std::process::{Command, Stdio};

    let opts = options.unwrap_or_default();
    let use_shell = opts.shell.unwrap_or(false);

    let mut cmd = if use_shell {
        #[cfg(windows)]
        {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(command);
            for arg in &args {
                c.arg(arg);
            }
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = Command::new("/bin/sh");
            let full_cmd = if args.is_empty() {
                command.to_string()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            c.arg("-c").arg(full_cmd);
            c
        }
    } else {
        let mut c = Command::new(command);
        c.args(&args);
        c
    };

    // Set working directory
    if let Some(cwd) = opts.cwd {
        cmd.current_dir(cwd);
    }

    // Set environment variables
    if let Some(env) = opts.env {
        cmd.envs(env);
    }

    // Capture stdout and stderr
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let status = output.status.code().unwrap_or(-1);

            SpawnSyncResult {
                status,
                stdout,
                stderr,
                error: None,
            }
        }
        Err(e) => SpawnSyncResult {
            status: -1,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(e.to_string()),
        },
    }
}

/// Execute a command in a shell synchronously (convenience wrapper).
#[op2]
#[serde]
fn op_howth_exec_sync(
    #[string] command: &str,
    #[serde] options: Option<SpawnOptions>,
) -> SpawnSyncResult {
    use std::process::{Command, Stdio};

    let opts = options.unwrap_or_default();

    #[cfg(windows)]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(command);
        c
    };

    // Set working directory
    if let Some(cwd) = opts.cwd {
        cmd.current_dir(cwd);
    }

    // Set environment variables
    if let Some(env) = opts.env {
        cmd.envs(env);
    }

    // Capture stdout and stderr
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    match cmd.output() {
        Ok(output) => {
            let max_buffer = opts.max_buffer.unwrap_or(1024 * 1024); // 1MB default

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check buffer size
            if stdout.len() > max_buffer || stderr.len() > max_buffer {
                return SpawnSyncResult {
                    status: -1,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some("ENOBUFS: stdout/stderr maxBuffer exceeded".to_string()),
                };
            }

            let status = output.status.code().unwrap_or(-1);

            SpawnSyncResult {
                status,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                error: None,
            }
        }
        Err(e) => SpawnSyncResult {
            status: -1,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("ENOENT: spawn error: {}", e)),
        },
    }
}

/// Base64 encode helper.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Base64 decode helper.
fn base64_decode(data: &str) -> Result<Vec<u8>, deno_core::error::AnyError> {
    fn decode_char(c: char) -> Option<u8> {
        match c {
            'A'..='Z' => Some(c as u8 - b'A'),
            'a'..='z' => Some(c as u8 - b'a' + 26),
            '0'..='9' => Some(c as u8 - b'0' + 52),
            '+' => Some(62),
            '/' => Some(63),
            '=' => Some(0),
            _ => None,
        }
    }

    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    let mut result = Vec::new();

    for chunk in data.as_bytes().chunks(4) {
        if chunk.len() != 4 {
            return Err(deno_core::error::AnyError::msg("Invalid base64"));
        }

        let b0 = decode_char(chunk[0] as char)
            .ok_or_else(|| deno_core::error::AnyError::msg("Invalid base64"))?;
        let b1 = decode_char(chunk[1] as char)
            .ok_or_else(|| deno_core::error::AnyError::msg("Invalid base64"))?;
        let b2 = decode_char(chunk[2] as char)
            .ok_or_else(|| deno_core::error::AnyError::msg("Invalid base64"))?;
        let b3 = decode_char(chunk[3] as char)
            .ok_or_else(|| deno_core::error::AnyError::msg("Invalid base64"))?;

        result.push((b0 << 2) | (b1 >> 4));
        if chunk[2] != b'=' {
            result.push((b1 << 4) | (b2 >> 2));
        }
        if chunk[3] != b'=' {
            result.push((b2 << 6) | b3);
        }
    }

    Ok(result)
}

/// Get current working directory.
#[op2]
#[string]
fn op_howth_cwd() -> Result<String, deno_core::error::AnyError> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.into())
}

/// Get environment variable.
#[op2]
#[string]
fn op_howth_env_get(#[string] key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Set environment variable.
#[op2(fast)]
fn op_howth_env_set(#[string] key: &str, #[string] value: &str) {
    std::env::set_var(key, value);
}

/// Exit the process.
#[op2(fast)]
fn op_howth_exit(code: i32) {
    std::process::exit(code);
}

/// Get command line arguments.
#[op2]
#[serde]
fn op_howth_args() -> Vec<String> {
    std::env::args().collect()
}

/// Fetch response from a URL.
#[derive(serde::Serialize)]
pub struct FetchResponse {
    pub ok: bool,
    pub status: u16,
    pub status_text: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Fetch request options.
#[derive(serde::Deserialize, Default)]
pub struct FetchOptions {
    pub method: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
}

/// Fetch a URL (synchronous via blocking client, exposed as async op).
/// Uses reqwest::blocking to make HTTP requests.
#[op2(async)]
#[serde]
async fn op_howth_fetch(
    #[string] url: String,
    #[serde] options: Option<FetchOptions>,
) -> Result<FetchResponse, deno_core::error::AnyError> {
    // Use std::thread::spawn since tokio spawn_blocking doesn't work well with current_thread
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let result = (|| {
            let client = reqwest::blocking::Client::new();
            let opts = options.unwrap_or_default();

            let method = opts.method.as_deref().unwrap_or("GET").to_uppercase();

            let mut request = match method.as_str() {
                "GET" => client.get(&url),
                "POST" => client.post(&url),
                "PUT" => client.put(&url),
                "DELETE" => client.delete(&url),
                "PATCH" => client.patch(&url),
                "HEAD" => client.head(&url),
                _ => {
                    return Err(deno_core::error::AnyError::msg(format!(
                        "Unsupported method: {}",
                        method
                    )))
                }
            };

            // Add headers
            if let Some(headers) = opts.headers {
                for (key, value) in headers {
                    request = request.header(&key, &value);
                }
            }

            // Add body
            if let Some(body) = opts.body {
                request = request.body(body);
            }

            let response = request.send()?;

            let status = response.status();
            let response_url = response.url().to_string();

            let mut headers = std::collections::HashMap::new();
            for (key, value) in response.headers().iter() {
                headers.insert(key.to_string(), value.to_str().unwrap_or("").to_string());
            }

            let body = response.text()?;

            Ok(FetchResponse {
                ok: status.is_success(),
                status: status.as_u16(),
                status_text: status.canonical_reason().unwrap_or("").to_string(),
                headers,
                body,
                url: response_url,
            })
        })();

        let _ = tx.send(result);
    });

    rx.await
        .map_err(|_| deno_core::error::AnyError::msg("Fetch cancelled"))?
}

/// Encode string to UTF-8 bytes.
#[op2]
#[serde]
fn op_howth_encode_utf8(#[string] text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// Decode UTF-8 bytes to string.
#[op2]
#[string]
fn op_howth_decode_utf8(#[buffer] bytes: &[u8]) -> Result<String, deno_core::error::AnyError> {
    String::from_utf8(bytes.to_vec()).map_err(|e| e.into())
}

/// Generate random bytes.
#[op2]
#[serde]
fn op_howth_random_bytes(len: u32) -> Vec<u8> {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let mut bytes = vec![0u8; len as usize];

    // Use multiple random sources for better entropy
    let state = RandomState::new();
    let mut hasher = state.build_hasher();

    for chunk in bytes.chunks_mut(8) {
        hasher.write_usize(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as usize,
        );
        let random = hasher.finish();
        let random_bytes = random.to_le_bytes();
        for (i, byte) in chunk.iter_mut().enumerate() {
            *byte = random_bytes[i % 8];
        }
        // Re-seed hasher
        hasher = state.build_hasher();
        hasher.write(&random_bytes);
    }

    bytes
}

/// Generate a random UUID v4.
#[op2]
#[string]
fn op_howth_random_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_usize(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as usize,
    );

    let mut bytes = [0u8; 16];
    let hash1 = hasher.finish();
    hasher.write_u64(hash1);
    let hash2 = hasher.finish();

    bytes[0..8].copy_from_slice(&hash1.to_le_bytes());
    bytes[8..16].copy_from_slice(&hash2.to_le_bytes());

    // Set version (4) and variant (RFC 4122)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Hash data using various algorithms.
#[op2]
#[serde]
fn op_howth_hash(
    #[string] algorithm: &str,
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple hash implementation using Rust's built-in hasher
    // For production, you'd want to use ring or sha2 crates
    match algorithm.to_lowercase().as_str() {
        "sha-1" | "sha1" => {
            // Simple simulation - in production use a proper SHA-1 implementation
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let h1 = hasher.finish();
            hasher.write_u64(h1);
            let h2 = hasher.finish();
            hasher.write_u64(h2);
            let h3 = hasher.finish();
            let mut result = Vec::with_capacity(20);
            result.extend_from_slice(&h1.to_be_bytes());
            result.extend_from_slice(&h2.to_be_bytes());
            result.extend_from_slice(&h3.to_be_bytes()[0..4]);
            Ok(result)
        }
        "sha-256" | "sha256" => {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let h1 = hasher.finish();
            hasher.write_u64(h1);
            let h2 = hasher.finish();
            hasher.write_u64(h2);
            let h3 = hasher.finish();
            hasher.write_u64(h3);
            let h4 = hasher.finish();
            let mut result = Vec::with_capacity(32);
            result.extend_from_slice(&h1.to_be_bytes());
            result.extend_from_slice(&h2.to_be_bytes());
            result.extend_from_slice(&h3.to_be_bytes());
            result.extend_from_slice(&h4.to_be_bytes());
            Ok(result)
        }
        "sha-384" | "sha384" => {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let mut result = Vec::with_capacity(48);
            for _ in 0..6 {
                let h = hasher.finish();
                result.extend_from_slice(&h.to_be_bytes());
                hasher.write_u64(h);
            }
            Ok(result)
        }
        "sha-512" | "sha512" => {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let mut result = Vec::with_capacity(64);
            for _ in 0..8 {
                let h = hasher.finish();
                result.extend_from_slice(&h.to_be_bytes());
                hasher.write_u64(h);
            }
            Ok(result)
        }
        "md5" => {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            let h1 = hasher.finish();
            hasher.write_u64(h1);
            let h2 = hasher.finish();
            let mut result = Vec::with_capacity(16);
            result.extend_from_slice(&h1.to_be_bytes());
            result.extend_from_slice(&h2.to_be_bytes());
            Ok(result)
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported algorithm: {}",
            algorithm
        ))),
    }
}

/// High-resolution time in nanoseconds.
#[op2(fast)]
#[bigint]
fn op_howth_hrtime() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

/// Sleep for a duration (async).
#[op2(async)]
#[serde]
async fn op_howth_sleep(ms: u32) -> () {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

impl Runtime {
    /// Create a new runtime.
    pub fn new(options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let state = Rc::new(RefCell::new(RuntimeState::default()));

        let cwd = options
            .cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let module_loader = Rc::new(HowthModuleLoader::new(cwd.clone()));

        let mut js_runtime = JsRuntime::new(DenoRuntimeOptions {
            extensions: vec![howth_runtime::init_ops()],
            module_loader: Some(module_loader),
            ..Default::default()
        });

        // Set up cwd if provided
        if let Some(cwd) = options.cwd {
            std::env::set_current_dir(&cwd)
                .map_err(|e| RuntimeError::Init(format!("Failed to set cwd: {}", e)))?;
        }

        // Execute bootstrap code to set up globals
        js_runtime
            .execute_script("<howth:bootstrap>", BOOTSTRAP_JS.to_string())
            .map_err(|e| RuntimeError::Init(format!("Bootstrap failed: {}", e)))?;

        Ok(Self { js_runtime, state })
    }

    /// Execute a script (non-module code).
    pub async fn execute_script(&mut self, code: &str) -> Result<(), RuntimeError> {
        self.js_runtime
            .execute_script("<howth>", code.to_string())
            .map_err(|e| RuntimeError::JavaScript(e.to_string()))?;
        Ok(())
    }

    /// Execute an ES module from a file path.
    pub async fn execute_module(&mut self, path: &std::path::Path) -> Result<(), RuntimeError> {
        let specifier = ModuleSpecifier::from_file_path(path)
            .map_err(|_| RuntimeError::Io(format!("Invalid path: {}", path.display())))?;

        // Set up the main module context for require
        let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let main_module_path = abs_path.to_string_lossy().to_string();
        let main_module_dir = abs_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        // Set up globals for both ESM and CommonJS compatibility
        let setup_code = format!(
            r#"
            globalThis.__howth_main_module_path = '{}';
            globalThis.__filename = '{}';
            globalThis.__dirname = '{}';
            "#,
            main_module_path.replace('\\', "\\\\").replace('\'', "\\'"),
            main_module_path.replace('\\', "\\\\").replace('\'', "\\'"),
            main_module_dir.replace('\\', "\\\\").replace('\'', "\\'")
        );
        self.js_runtime
            .execute_script("<howth:setup>", setup_code)
            .map_err(|e| RuntimeError::JavaScript(format!("Setup failed: {}", e)))?;

        let module_id = self
            .js_runtime
            .load_main_es_module(&specifier)
            .await
            .map_err(|e| RuntimeError::JavaScript(format!("Failed to load module: {}", e)))?;

        // mod_evaluate returns a receiver - we need to run the event loop
        // while waiting for the module to complete
        let mut receiver = self.js_runtime.mod_evaluate(module_id);

        // Poll both the event loop and the module evaluation receiver
        loop {
            tokio::select! {
                biased;

                // Check if module evaluation completed
                maybe_result = &mut receiver => {
                    match maybe_result {
                        Ok(()) => break,
                        Err(e) => return Err(RuntimeError::JavaScript(format!("Module evaluation failed: {}", e))),
                    }
                }

                // Drive the event loop
                event_loop_result = self.js_runtime.run_event_loop(Default::default()) => {
                    event_loop_result
                        .map_err(|e| RuntimeError::JavaScript(format!("Event loop error: {}", e)))?;
                }
            }
        }

        // Emit the exit event before returning
        let _ = self.js_runtime.execute_script(
            "<howth:exit>",
            "globalThis.process?.emit?.('exit', globalThis.process?.exitCode || 0);".to_string(),
        );

        Ok(())
    }

    /// Run the event loop until completion.
    pub async fn run_event_loop(&mut self) -> Result<(), RuntimeError> {
        self.js_runtime
            .run_event_loop(Default::default())
            .await
            .map_err(|e| RuntimeError::JavaScript(e.to_string()))?;
        Ok(())
    }

    /// Get the exit code.
    pub fn exit_code(&self) -> i32 {
        self.state.borrow().exit_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_execution() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime.execute_script("1 + 1").await.unwrap();
    }

    #[tokio::test]
    async fn test_console_log() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script("console.log('hello')")
            .await
            .unwrap();
        runtime.run_event_loop().await.unwrap();
    }

    #[tokio::test]
    async fn test_process_env() {
        std::env::set_var("HOWTH_TEST_VAR", "test_value");
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (process.env.HOWTH_TEST_VAR !== 'test_value') {
                    throw new Error('env var not found');
                }
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_process_cwd() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: None,
        })
        .unwrap();
        runtime
            .execute_script(
                r#"
                const cwd = process.cwd();
                if (typeof cwd !== 'string' || cwd.length === 0) {
                    throw new Error('cwd failed');
                }
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_variables() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const x = 10;
                const y = 20;
                const sum = x + y;
                if (sum !== 30) throw new Error('math failed');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_text_encoder_decoder() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const encoder = new TextEncoder();
                const decoder = new TextDecoder();
                const encoded = encoder.encode('Hello');
                if (encoded.length !== 5) throw new Error('encode failed');
                if (encoded[0] !== 72) throw new Error('wrong byte');
                const decoded = decoder.decode(encoded);
                if (decoded !== 'Hello') throw new Error('decode failed');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_url() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const url = new URL('https://example.com:8080/path?foo=bar#hash');
                if (url.hostname !== 'example.com') throw new Error('hostname');
                if (url.port !== '8080') throw new Error('port');
                if (url.pathname !== '/path') throw new Error('pathname');
                if (url.search !== '?foo=bar') throw new Error('search');
                if (url.hash !== '#hash') throw new Error('hash');
                if (url.searchParams.get('foo') !== 'bar') throw new Error('searchParams');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_url_search_params() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const params = new URLSearchParams('a=1&b=2&a=3');
                if (params.get('a') !== '1') throw new Error('get first');
                if (params.get('b') !== '2') throw new Error('get b');
                const all = params.getAll('a');
                if (all.length !== 2) throw new Error('getAll length');
                if (all[0] !== '1' || all[1] !== '3') throw new Error('getAll values');
                params.set('c', '4');
                if (!params.has('c')) throw new Error('has');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_headers() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const headers = new Headers({'Content-Type': 'application/json'});
                if (headers.get('content-type') !== 'application/json') throw new Error('get');
                headers.set('X-Custom', 'value');
                if (!headers.has('x-custom')) throw new Error('has');
                headers.append('X-Custom', 'value2');
                if (headers.get('x-custom') !== 'value, value2') throw new Error('append');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_atob_btoa() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const original = 'Hello, World!';
                const encoded = btoa(original);
                if (encoded !== 'SGVsbG8sIFdvcmxkIQ==') throw new Error('btoa');
                const decoded = atob(encoded);
                if (decoded !== original) throw new Error('atob');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_request_response() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const req = new Request('https://example.com', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: '{"test": true}'
                });
                if (req.method !== 'POST') throw new Error('request method');
                if (req.url !== 'https://example.com') throw new Error('request url');

                const res = new Response('body', { status: 201, statusText: 'Created' });
                if (res.status !== 201) throw new Error('response status');
                if (!res.ok) throw new Error('response ok');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_fetch_mock() {
        // This test verifies fetch is callable but doesn't make real network requests
        // A full integration test would require a mock server
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (typeof fetch !== 'function') throw new Error('fetch not defined');
                if (typeof Request !== 'function') throw new Error('Request not defined');
                if (typeof Response !== 'function') throw new Error('Response not defined');
                if (typeof Headers !== 'function') throw new Error('Headers not defined');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_set_timeout() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                let called = false;
                setTimeout(() => { called = true; }, 10);
                "#,
            )
            .await
            .unwrap();
        runtime.run_event_loop().await.unwrap();
    }

    #[tokio::test]
    async fn test_crypto_random() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const bytes = new Uint8Array(16);
                crypto.getRandomValues(bytes);
                // Check that not all bytes are zero (extremely unlikely if random)
                let sum = 0;
                for (const b of bytes) sum += b;
                if (sum === 0) throw new Error('all zeros');

                const uuid = crypto.randomUUID();
                if (uuid.length !== 36) throw new Error('invalid uuid length');
                if (uuid[8] !== '-' || uuid[13] !== '-') throw new Error('invalid uuid format');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_abort_controller() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const controller = new AbortController();
                if (controller.signal.aborted) throw new Error('should not be aborted');
                controller.abort();
                if (!controller.signal.aborted) throw new Error('should be aborted');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_event_target() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const target = new EventTarget();
                let fired = false;
                target.addEventListener('test', () => { fired = true; });
                target.dispatchEvent(new Event('test'));
                if (!fired) throw new Error('event not fired');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_blob() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const blob = new Blob(['Hello, ', 'World!'], { type: 'text/plain' });
                if (blob.size !== 13) throw new Error('wrong size');
                if (blob.type !== 'text/plain') throw new Error('wrong type');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_file() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const file = new File(['content'], 'test.txt', { type: 'text/plain' });
                if (file.name !== 'test.txt') throw new Error('wrong name');
                if (file.size !== 7) throw new Error('wrong size');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_form_data() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const form = new FormData();
                form.append('name', 'value');
                if (!form.has('name')) throw new Error('missing key');
                if (form.get('name') !== 'value') throw new Error('wrong value');
                form.delete('name');
                if (form.has('name')) throw new Error('should be deleted');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_readable_stream() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const stream = new ReadableStream({
                    start(controller) {
                        controller.enqueue('chunk');
                        controller.close();
                    }
                });
                if (typeof stream.getReader !== 'function') throw new Error('no getReader');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_performance() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const now = performance.now();
                if (typeof now !== 'number') throw new Error('not a number');
                if (typeof performance.timeOrigin !== 'number') throw new Error('no timeOrigin');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_structured_clone() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const obj = { a: 1, b: { c: 2 } };
                const clone = structuredClone(obj);
                clone.b.c = 3;
                if (obj.b.c !== 2) throw new Error('original modified');
                if (clone.b.c !== 3) throw new Error('clone not modified');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_process_hrtime() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const t = process.hrtime.bigint();
                if (typeof t !== 'bigint') throw new Error('not bigint');
                if (t <= 0n) throw new Error('should be positive');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_process_next_tick() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                let called = false;
                process.nextTick(() => { called = true; });
                // nextTick uses queueMicrotask, check it was queued
                if (typeof process.nextTick !== 'function') throw new Error('no nextTick');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_writable_stream() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (typeof WritableStream !== 'function') throw new Error('no WritableStream');
                const chunks = [];
                const writable = new WritableStream({
                    write(chunk) { chunks.push(chunk); }
                });
                if (!writable.getWriter) throw new Error('no getWriter');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_transform_stream() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (typeof TransformStream !== 'function') throw new Error('no TransformStream');
                const transform = new TransformStream({
                    transform(chunk, controller) {
                        controller.enqueue(chunk.toUpperCase());
                    }
                });
                if (!transform.readable) throw new Error('no readable');
                if (!transform.writable) throw new Error('no writable');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_buffer() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (typeof Buffer !== 'function') throw new Error('no Buffer');

                // Buffer.from string
                const buf1 = Buffer.from("Hello");
                if (buf1.toString() !== "Hello") throw new Error('from string failed');
                if (buf1.length !== 5) throw new Error('wrong length');

                // Buffer.from array
                const buf2 = Buffer.from([72, 105]);
                if (buf2.toString() !== "Hi") throw new Error('from array failed');

                // Buffer.alloc
                const buf3 = Buffer.alloc(3, 0x41);
                if (buf3.toString() !== "AAA") throw new Error('alloc failed');

                // Buffer.concat
                const buf4 = Buffer.concat([Buffer.from("He"), Buffer.from("llo")]);
                if (buf4.toString() !== "Hello") throw new Error('concat failed');

                // Base64
                const buf5 = Buffer.from("Hello");
                if (buf5.toString("base64") !== "SGVsbG8=") throw new Error('base64 encode failed');
                const buf6 = Buffer.from("SGVsbG8=", "base64");
                if (buf6.toString() !== "Hello") throw new Error('base64 decode failed');

                // Hex
                if (buf5.toString("hex") !== "48656c6c6f") throw new Error('hex encode failed');
                const buf7 = Buffer.from("48656c6c6f", "hex");
                if (buf7.toString() !== "Hello") throw new Error('hex decode failed');

                // Buffer.isBuffer
                if (!Buffer.isBuffer(buf1)) throw new Error('isBuffer failed');
                if (Buffer.isBuffer(new Uint8Array(5))) throw new Error('isBuffer should be false for Uint8Array');

                // Read/write integers
                const buf8 = Buffer.alloc(4);
                buf8.writeUInt32LE(0x12345678, 0);
                if (buf8.readUInt32LE(0) !== 0x12345678) throw new Error('UInt32LE failed');

                // indexOf/includes
                const buf9 = Buffer.from("Hello World");
                if (buf9.indexOf("World") !== 6) throw new Error('indexOf failed');
                if (!buf9.includes("World")) throw new Error('includes failed');

                // equals
                if (!Buffer.from("test").equals(Buffer.from("test"))) throw new Error('equals failed');
                if (Buffer.from("test").equals(Buffer.from("other"))) throw new Error('equals should be false');

                // compare
                if (Buffer.from("a").compare(Buffer.from("b")) !== -1) throw new Error('compare less failed');
                if (Buffer.from("b").compare(Buffer.from("a")) !== 1) throw new Error('compare greater failed');
                if (Buffer.from("a").compare(Buffer.from("a")) !== 0) throw new Error('compare equal failed');

                // slice
                if (buf9.slice(0, 5).toString() !== "Hello") throw new Error('slice failed');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_bare_specifier_import() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a simple package in node_modules
        let pkg_dir = temp.path().join("node_modules/test-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "test-pkg", "main": "index.js"}"#,
        )
        .unwrap();
        fs::write(pkg_dir.join("index.js"), "export const value = 42;").unwrap();

        // Create main entry file that imports from the package
        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            import { value } from 'test-pkg';
            if (value !== 42) throw new Error('import failed: ' + value);
            console.log('Bare specifier import works! value =', value);
        "#,
        )
        .unwrap();

        // Run with the temp directory as cwd
        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_scoped_package_import() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a scoped package
        let pkg_dir = temp.path().join("node_modules/@myorg/utils");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("package.json"), r#"{"name": "@myorg/utils"}"#).unwrap();
        fs::write(
            pkg_dir.join("index.js"),
            "export function add(a, b) { return a + b; }",
        )
        .unwrap();

        // Create main file
        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            import { add } from '@myorg/utils';
            if (add(2, 3) !== 5) throw new Error('scoped import failed');
            console.log('Scoped package import works!');
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_subpath_import() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a package with subpath
        let pkg_dir = temp.path().join("node_modules/mylib");
        fs::create_dir_all(pkg_dir.join("utils")).unwrap();
        fs::write(pkg_dir.join("package.json"), r#"{"name": "mylib"}"#).unwrap();
        fs::write(pkg_dir.join("index.js"), "export const main = true;").unwrap();
        fs::write(pkg_dir.join("utils/math.js"), "export const PI = 3.14159;").unwrap();

        // Create main file
        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            import { PI } from 'mylib/utils/math';
            if (Math.abs(PI - 3.14159) > 0.0001) throw new Error('subpath import failed');
            console.log('Subpath import works! PI =', PI);
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_path_module() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        fs::write(&main_file, r#"
            import path from 'node:path';

            // Test join
            const joined = path.join('foo', 'bar', 'baz');
            if (joined !== 'foo/bar/baz') throw new Error('join failed: ' + joined);

            // Test dirname
            const dir = path.dirname('/foo/bar/baz.txt');
            if (dir !== '/foo/bar') throw new Error('dirname failed: ' + dir);

            // Test basename
            const base = path.basename('/foo/bar/baz.txt');
            if (base !== 'baz.txt') throw new Error('basename failed: ' + base);

            const baseNoExt = path.basename('/foo/bar/baz.txt', '.txt');
            if (baseNoExt !== 'baz') throw new Error('basename with ext failed: ' + baseNoExt);

            // Test extname
            const ext = path.extname('/foo/bar/baz.txt');
            if (ext !== '.txt') throw new Error('extname failed: ' + ext);

            // Test isAbsolute
            if (!path.isAbsolute('/foo/bar')) throw new Error('isAbsolute should be true for /foo/bar');
            if (path.isAbsolute('foo/bar')) throw new Error('isAbsolute should be false for foo/bar');

            // Test normalize
            const normalized = path.normalize('/foo/bar/../baz');
            if (normalized !== '/foo/baz') throw new Error('normalize failed: ' + normalized);

            // Test parse
            const parsed = path.parse('/foo/bar/baz.txt');
            if (parsed.root !== '/') throw new Error('parse root failed');
            if (parsed.base !== 'baz.txt') throw new Error('parse base failed');
            if (parsed.ext !== '.txt') throw new Error('parse ext failed');
            if (parsed.name !== 'baz') throw new Error('parse name failed');

            // Test format
            const formatted = path.format({ dir: '/foo/bar', base: 'baz.txt' });
            if (formatted !== '/foo/bar/baz.txt') throw new Error('format failed: ' + formatted);

            // Test sep and delimiter
            if (path.sep !== '/') throw new Error('sep failed: ' + path.sep);
            if (path.delimiter !== ':') throw new Error('delimiter failed: ' + path.delimiter);

            // Test posix/win32 variants exist
            if (!path.posix) throw new Error('posix not found');
            if (!path.win32) throw new Error('win32 not found');

            console.log('✓ node:path module works!');
        "#).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_path_resolve() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let temp_path = temp.path().to_string_lossy();

        fs::write(&main_file, format!(r#"
            import path from 'node:path';

            // Test resolve with absolute path
            const abs = path.resolve('/foo', 'bar', 'baz');
            if (abs !== '/foo/bar/baz') throw new Error('resolve absolute failed: ' + abs);

            // Test resolve includes cwd for relative paths
            const rel = path.resolve('foo', 'bar');
            // Should start with cwd
            if (!rel.startsWith('/')) throw new Error('resolve relative should be absolute: ' + rel);

            console.log('✓ path.resolve works!');
        "#)).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_path_relative() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            import { relative } from 'node:path';

            // Test relative path calculation
            const rel = relative('/foo/bar', '/foo/bar/baz/qux');
            if (rel !== 'baz/qux') throw new Error('relative failed: ' + rel);

            const rel2 = relative('/foo/bar/baz', '/foo/qux');
            if (rel2 !== '../../qux') throw new Error('relative up failed: ' + rel2);

            console.log('✓ path.relative works!');
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_fs_read_write() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let test_file = temp.path().join("test.txt");
        let test_path = test_file.to_string_lossy();

        fs::write(&main_file, format!(r#"
            import fs from 'node:fs';

            // Test writeFileSync
            fs.writeFileSync('{}', 'Hello, World!');

            // Test existsSync
            if (!fs.existsSync('{}')) throw new Error('file should exist');

            // Test readFileSync
            const content = fs.readFileSync('{}', 'utf8');
            if (content !== 'Hello, World!') throw new Error('content mismatch: ' + content);

            // Test appendFileSync
            fs.appendFileSync('{}', ' More text.');
            const updated = fs.readFileSync('{}', 'utf8');
            if (updated !== 'Hello, World! More text.') throw new Error('append failed: ' + updated);

            console.log('✓ fs read/write works!');
        "#, test_path, test_path, test_path, test_path, test_path)).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_fs_directory_ops() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let test_dir = temp.path().join("testdir");
        let test_path = test_dir.to_string_lossy();

        fs::write(&main_file, format!(r#"
            import fs from 'node:fs';

            // Test mkdirSync
            fs.mkdirSync('{}');
            if (!fs.existsSync('{}')) throw new Error('dir should exist');

            // Test mkdirSync recursive
            fs.mkdirSync('{}/sub/deep', {{ recursive: true }});

            // Write a file in the directory
            fs.writeFileSync('{}/file.txt', 'test');

            // Test readdirSync
            const entries = fs.readdirSync('{}');
            if (!entries.includes('file.txt')) throw new Error('readdirSync failed');
            if (!entries.includes('sub')) throw new Error('readdirSync should include sub');

            // Test readdirSync with withFileTypes
            const dirents = fs.readdirSync('{}', {{ withFileTypes: true }});
            const fileDirent = dirents.find(d => d.name === 'file.txt');
            if (!fileDirent || !fileDirent.isFile()) throw new Error('Dirent.isFile failed');
            const subDirent = dirents.find(d => d.name === 'sub');
            if (!subDirent || !subDirent.isDirectory()) throw new Error('Dirent.isDirectory failed');

            // Test rmdirSync recursive
            fs.rmdirSync('{}', {{ recursive: true }});
            if (fs.existsSync('{}')) throw new Error('dir should be removed');

            console.log('✓ fs directory ops work!');
        "#, test_path, test_path, test_path, test_path, test_path, test_path, test_path, test_path)).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_fs_stat() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let test_file = temp.path().join("test.txt");
        let test_path = test_file.to_string_lossy();

        // Create the test file
        fs::write(&test_file, "Hello").unwrap();

        fs::write(&main_file, format!(r#"
            import fs from 'node:fs';

            // Test statSync
            const stat = fs.statSync('{}');

            if (!stat.isFile()) throw new Error('should be a file');
            if (stat.isDirectory()) throw new Error('should not be a directory');
            if (stat.size !== 5) throw new Error('size should be 5: ' + stat.size);
            if (typeof stat.mtimeMs !== 'number') throw new Error('mtimeMs should be number');
            if (!(stat.mtime instanceof Date)) throw new Error('mtime should be Date');

            // Test that Stats has all expected properties
            const props = ['dev', 'ino', 'mode', 'nlink', 'uid', 'gid', 'size', 'atime', 'mtime', 'ctime'];
            for (const prop of props) {{
                if (!(prop in stat)) throw new Error('missing property: ' + prop);
            }}

            console.log('✓ fs.statSync works!');
        "#, test_path)).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_fs_promises() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let test_file = temp.path().join("async-test.txt");
        let test_path = test_file.to_string_lossy();

        fs::write(
            &main_file,
            format!(
                r#"
            import {{ promises as fsp }} from 'node:fs';

            // Test promises API
            await fsp.writeFile('{}', 'Async content');
            const content = await fsp.readFile('{}', 'utf8');
            if (content !== 'Async content') throw new Error('async content mismatch');

            const stat = await fsp.stat('{}');
            if (!stat.isFile()) throw new Error('should be file');

            await fsp.unlink('{}');

            console.log('✓ fs/promises works!');
        "#,
                test_path, test_path, test_path, test_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_fs_copy_rename() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        let src_file = temp.path().join("src.txt");
        let dst_file = temp.path().join("dst.txt");
        let renamed_file = temp.path().join("renamed.txt");
        let src_path = src_file.to_string_lossy();
        let dst_path = dst_file.to_string_lossy();
        let renamed_path = renamed_file.to_string_lossy();

        fs::write(&src_file, "Copy me!").unwrap();

        fs::write(
            &main_file,
            format!(
                r#"
            import fs from 'node:fs';

            // Test copyFileSync
            fs.copyFileSync('{}', '{}');
            if (!fs.existsSync('{}')) throw new Error('copy should exist');
            const copied = fs.readFileSync('{}', 'utf8');
            if (copied !== 'Copy me!') throw new Error('copy content mismatch');

            // Test renameSync
            fs.renameSync('{}', '{}');
            if (fs.existsSync('{}')) throw new Error('original should not exist');
            if (!fs.existsSync('{}')) throw new Error('renamed should exist');
            const renamed = fs.readFileSync('{}', 'utf8');
            if (renamed !== 'Copy me!') throw new Error('renamed content mismatch');

            console.log('✓ fs copy/rename works!');
        "#,
                src_path,
                dst_path,
                dst_path,
                dst_path,
                dst_path,
                renamed_path,
                dst_path,
                renamed_path,
                renamed_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_basic() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a CommonJS module
        let lib_file = temp.path().join("lib.js");
        fs::write(
            &lib_file,
            r#"
            module.exports = {
                add: function(a, b) { return a + b; },
                PI: 3.14159
            };
        "#,
        )
        .unwrap();

        // Create main file that uses require
        let main_file = temp.path().join("main.js");
        let lib_path = lib_file.to_string_lossy();
        fs::write(
            &main_file,
            format!(
                r#"
            const lib = require('{}');

            if (typeof lib.add !== 'function') throw new Error('add should be function');
            if (lib.add(2, 3) !== 5) throw new Error('add failed');
            if (lib.PI !== 3.14159) throw new Error('PI failed');

            console.log('✓ CommonJS basic require works!');
        "#,
                lib_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_exports_shorthand() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a module using exports shorthand
        let lib_file = temp.path().join("utils.js");
        fs::write(
            &lib_file,
            r#"
            exports.multiply = function(a, b) { return a * b; };
            exports.VERSION = "1.0.0";
        "#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        let lib_path = lib_file.to_string_lossy();
        fs::write(
            &main_file,
            format!(
                r#"
            const utils = require('{}');

            if (utils.multiply(3, 4) !== 12) throw new Error('multiply failed');
            if (utils.VERSION !== "1.0.0") throw new Error('VERSION failed');

            console.log('✓ CommonJS exports shorthand works!');
        "#,
                lib_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_dirname_filename() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let lib_file = temp.path().join("mymodule.js");
        let temp_path = temp.path().to_string_lossy().to_string();
        fs::write(
            &lib_file,
            r#"
            module.exports = {
                dirname: __dirname,
                filename: __filename
            };
        "#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        let lib_path = lib_file.to_string_lossy();
        fs::write(
            &main_file,
            format!(
                r#"
            const mod = require('{}');

            // __dirname should be the directory containing the module
            if (!mod.dirname.includes('{}') && !mod.dirname.includes('/private{}')) {{
                throw new Error('__dirname wrong: ' + mod.dirname);
            }}

            // __filename should be the full path to the module
            if (!mod.filename.endsWith('mymodule.js')) {{
                throw new Error('__filename wrong: ' + mod.filename);
            }}

            console.log('✓ __dirname and __filename work!');
        "#,
                lib_path, temp_path, temp_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_json() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a JSON file
        let json_file = temp.path().join("config.json");
        fs::write(
            &json_file,
            r#"{"name": "test", "version": "1.0.0", "count": 42}"#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        let json_path = json_file.to_string_lossy();
        fs::write(
            &main_file,
            format!(
                r#"
            const config = require('{}');

            if (config.name !== 'test') throw new Error('name failed');
            if (config.version !== '1.0.0') throw new Error('version failed');
            if (config.count !== 42) throw new Error('count failed');

            console.log('✓ JSON require works!');
        "#,
                json_path
            ),
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_node_modules() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a package in node_modules
        let pkg_dir = temp.path().join("node_modules/my-cjs-pkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(
            pkg_dir.join("package.json"),
            r#"{"name": "my-cjs-pkg", "main": "index.js"}"#,
        )
        .unwrap();
        fs::write(
            pkg_dir.join("index.js"),
            r#"
            module.exports = function greet(name) {
                return 'Hello, ' + name + '!';
            };
        "#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            const greet = require('my-cjs-pkg');

            const result = greet('World');
            if (result !== 'Hello, World!') throw new Error('greet failed: ' + result);

            console.log('✓ node_modules require works!');
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_relative_require() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a nested structure
        let lib_dir = temp.path().join("lib");
        fs::create_dir_all(&lib_dir).unwrap();

        fs::write(
            lib_dir.join("math.js"),
            r#"
            exports.square = function(x) { return x * x; };
        "#,
        )
        .unwrap();

        fs::write(
            lib_dir.join("utils.js"),
            r#"
            const math = require('./math');
            exports.squareSum = function(a, b) {
                return math.square(a) + math.square(b);
            };
        "#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            const utils = require('./lib/utils');

            const result = utils.squareSum(3, 4);
            if (result !== 25) throw new Error('squareSum failed: ' + result);

            console.log('✓ Relative require works!');
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_builtin_modules() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        let main_file = temp.path().join("main.js");
        fs::write(
            &main_file,
            r#"
            const path = require('node:path');
            const fs = require('node:fs');

            // Test path
            const joined = path.join('foo', 'bar');
            if (joined !== 'foo/bar') throw new Error('path.join failed');

            // Test fs
            if (typeof fs.readFileSync !== 'function') throw new Error('fs.readFileSync missing');
            if (typeof fs.existsSync !== 'function') throw new Error('fs.existsSync missing');

            console.log('✓ Built-in modules via require work!');
        "#,
        )
        .unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }

    #[tokio::test]
    async fn test_commonjs_module_caching() {
        use std::fs;
        let temp = tempfile::TempDir::new().unwrap();

        // Create a module with a counter to verify caching
        let counter_file = temp.path().join("counter.js");
        fs::write(
            &counter_file,
            r#"
            let count = 0;
            module.exports = {
                increment: function() { return ++count; },
                getCount: function() { return count; }
            };
        "#,
        )
        .unwrap();

        let main_file = temp.path().join("main.js");
        let counter_path = counter_file.to_string_lossy();
        fs::write(&main_file, format!(r#"
            const counter1 = require('{}');
            const counter2 = require('{}');

            counter1.increment();
            counter1.increment();
            counter2.increment();

            // Both should reference the same module instance
            if (counter1.getCount() !== 3) throw new Error('caching failed: ' + counter1.getCount());
            if (counter2.getCount() !== 3) throw new Error('caching failed for counter2');
            if (counter1 !== counter2) throw new Error('modules should be identical');

            console.log('✓ Module caching works!');
        "#, counter_path, counter_path)).unwrap();

        let mut runtime = Runtime::new(RuntimeOptions {
            cwd: Some(temp.path().to_path_buf()),
            main_module: Some(main_file.clone()),
        })
        .unwrap();

        runtime.execute_module(&main_file).await.unwrap();
    }
}
