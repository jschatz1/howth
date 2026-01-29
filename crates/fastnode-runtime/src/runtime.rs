//! Runtime implementation using deno_core.

use crate::module_loader::HowthModuleLoader;
use deno_core::{extension, op2, JsRuntime, ModuleSpecifier, RuntimeOptions as DenoRuntimeOptions};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write, Seek, SeekFrom};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::sync::mpsc;
use notify::{Watcher, RecommendedWatcher, RecursiveMode, Event};

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
    /// Command-line arguments for the script (simulates process.argv).
    pub args: Option<Vec<String>>,
}

/// Thread-local storage for script arguments (set before runtime creation).
static SCRIPT_ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

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
        op_howth_chdir,
        op_howth_platform,
        op_howth_arch,
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
        op_howth_hmac,
        op_howth_cipher,
        op_howth_cipher_gcm,
        op_howth_sign,
        op_howth_verify,
        op_howth_public_encrypt,
        op_howth_private_decrypt,
        op_howth_generate_rsa_keypair,
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
        op_howth_fs_readlink,
        op_howth_fs_symlink,
        op_howth_fs_chown,
        op_howth_fs_chmod,
        op_howth_fs_access,
        // File descriptor ops (for streaming)
        op_howth_fs_open_fd,
        op_howth_fs_read_fd,
        op_howth_fs_write_fd,
        op_howth_fs_close_fd,
        op_howth_fs_seek_fd,
        // File watching ops
        op_howth_fs_watch_start,
        op_howth_fs_watch_poll,
        op_howth_fs_watch_close,
        // Zlib compression ops
        op_howth_zlib_gzip,
        op_howth_zlib_gunzip,
        op_howth_zlib_deflate,
        op_howth_zlib_inflate,
        op_howth_zlib_deflate_raw,
        op_howth_zlib_inflate_raw,
        // Worker thread ops
        op_howth_worker_is_main_thread,
        op_howth_worker_thread_id,
        op_howth_worker_create,
        op_howth_worker_post_message,
        op_howth_worker_recv_message,
        op_howth_worker_parent_post,
        op_howth_worker_parent_recv,
        op_howth_worker_terminate,
        op_howth_worker_is_running,
        // Child process ops (sync)
        op_howth_spawn_sync,
        op_howth_exec_sync,
        // Child process ops (async with streams)
        op_howth_spawn_async,
        op_howth_spawn_read_stdout,
        op_howth_spawn_read_stderr,
        op_howth_spawn_write_stdin,
        op_howth_spawn_close_stdin,
        op_howth_spawn_wait,
        op_howth_spawn_kill,
        // HTTP server ops
        op_howth_http_listen,
        op_howth_http_accept,
        op_howth_http_respond,
        op_howth_http_close,
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
            let is_symlink = file_type.map(|ft| ft.is_symlink()).unwrap_or(false);
            // For symlinks, follow the link to determine the target type
            let (is_file, is_directory) = if is_symlink {
                match std::fs::metadata(entry.path()) {
                    Ok(meta) => (meta.is_file(), meta.is_dir()),
                    Err(_) => (false, false), // broken symlink
                }
            } else {
                (
                    file_type.map(|ft| ft.is_file()).unwrap_or(false),
                    file_type.map(|ft| ft.is_dir()).unwrap_or(false),
                )
            };
            DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_file,
                is_directory,
                is_symlink,
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

/// Read the target of a symbolic link.
#[op2]
#[string]
fn op_howth_fs_readlink(#[string] path: &str) -> Result<String, deno_core::error::AnyError> {
    std::fs::read_link(path)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format_fs_error(e, "readlink", path))
}

/// Create a symbolic link.
#[op2(fast)]
fn op_howth_fs_symlink(
    #[string] target: &str,
    #[string] path: &str,
    #[string] link_type: &str,
) -> Result<(), deno_core::error::AnyError> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, path)
            .map_err(|e| format_fs_error(e, "symlink", path))
    }
    #[cfg(windows)]
    {
        if link_type == "dir" || std::path::Path::new(target).is_dir() {
            std::os::windows::fs::symlink_dir(target, path)
                .map_err(|e| format_fs_error(e, "symlink", path))
        } else {
            std::os::windows::fs::symlink_file(target, path)
                .map_err(|e| format_fs_error(e, "symlink", path))
        }
    }
}

/// Change ownership of a file (Unix only).
#[op2(fast)]
fn op_howth_fs_chown(
    #[string] path: &str,
    uid: u32,
    gid: u32,
) -> Result<(), deno_core::error::AnyError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(std::path::Path::new(path).as_os_str().as_bytes())
            .map_err(|e| deno_core::error::AnyError::msg(format!("Invalid path: {}", e)))?;
        let result = unsafe { libc::chown(c_path.as_ptr(), uid, gid as libc::gid_t) };
        if result != 0 {
            return Err(format_fs_error(
                std::io::Error::last_os_error(),
                "chown",
                path,
            ));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        // chown is a no-op on Windows
        let _ = (path, uid, gid);
        Ok(())
    }
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
// File Descriptor Operations (for streaming)
// ============================================================================

/// Global file handle ID counter
static NEXT_FILE_ID: AtomicU32 = AtomicU32::new(1);

/// Storage for open file handles
lazy_static::lazy_static! {
    static ref FILE_HANDLES: Arc<std::sync::Mutex<HashMap<u32, std::fs::File>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
}

/// Result of opening a file
#[derive(serde::Serialize)]
pub struct FileOpenResult {
    pub fd: u32,
    pub error: Option<String>,
}

/// Open a file and return a handle ID
#[op2]
#[serde]
fn op_howth_fs_open_fd(
    #[string] path: &str,
    #[string] flags: &str,
    mode: u32,
) -> FileOpenResult {
    use std::fs::OpenOptions;

    let mut opts = OpenOptions::new();

    // Parse flags like Node.js: r, r+, w, w+, a, a+, etc.
    match flags {
        "r" => { opts.read(true); }
        "r+" | "rs+" => { opts.read(true).write(true); }
        "w" => { opts.write(true).create(true).truncate(true); }
        "w+" | "wx+" => { opts.read(true).write(true).create(true).truncate(true); }
        "a" => { opts.write(true).create(true).append(true); }
        "a+" | "ax+" => { opts.read(true).write(true).create(true).append(true); }
        _ => { opts.read(true); } // Default to read
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode);
    }

    match opts.open(path) {
        Ok(file) => {
            let id = NEXT_FILE_ID.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut handles) = FILE_HANDLES.lock() {
                handles.insert(id, file);
            }
            FileOpenResult {
                fd: id,
                error: None,
            }
        }
        Err(e) => FileOpenResult {
            fd: 0,
            error: Some(format!("{}", e)),
        },
    }
}

/// Read bytes from a file handle
#[op2]
#[serde]
fn op_howth_fs_read_fd(fd: u32, length: u32) -> Result<Option<Vec<u8>>, deno_core::error::AnyError> {
    let mut handles = FILE_HANDLES.lock()
        .map_err(|_| deno_core::error::AnyError::msg("lock error"))?;

    if let Some(file) = handles.get_mut(&fd) {
        let mut buf = vec![0u8; length as usize];
        match file.read(&mut buf) {
            Ok(0) => Ok(None), // EOF
            Ok(n) => {
                buf.truncate(n);
                Ok(Some(buf))
            }
            Err(e) => Err(deno_core::error::AnyError::msg(format!("read error: {}", e))),
        }
    } else {
        Err(deno_core::error::AnyError::msg("EBADF: bad file descriptor"))
    }
}

/// Write bytes to a file handle (accepts serde Vec<u8>)
#[op2]
fn op_howth_fs_write_fd(fd: u32, #[serde] data: Vec<u8>) -> Result<u32, deno_core::error::AnyError> {
    let mut handles = FILE_HANDLES.lock()
        .map_err(|_| deno_core::error::AnyError::msg("lock error"))?;

    if let Some(file) = handles.get_mut(&fd) {
        match file.write(&data) {
            Ok(n) => {
                // Flush to ensure data is written
                let _ = file.flush();
                Ok(n as u32)
            }
            Err(e) => Err(deno_core::error::AnyError::msg(format!("write error: {}", e))),
        }
    } else {
        Err(deno_core::error::AnyError::msg("EBADF: bad file descriptor"))
    }
}

/// Seek in a file handle (returns new position or -1 on error)
#[op2(fast)]
fn op_howth_fs_seek_fd(fd: u32, offset: f64, whence: u32) -> f64 {
    let mut handles = match FILE_HANDLES.lock() {
        Ok(h) => h,
        Err(_) => return -1.0,
    };

    if let Some(file) = handles.get_mut(&fd) {
        let pos = match whence {
            0 => SeekFrom::Start(offset as u64),       // SEEK_SET
            1 => SeekFrom::Current(offset as i64),     // SEEK_CUR
            2 => SeekFrom::End(offset as i64),         // SEEK_END
            _ => return -1.0,
        };
        match file.seek(pos) {
            Ok(p) => p as f64,
            Err(_) => -1.0,
        }
    } else {
        -1.0
    }
}

/// Close a file handle
#[op2(fast)]
fn op_howth_fs_close_fd(fd: u32) -> Result<(), deno_core::error::AnyError> {
    let mut handles = FILE_HANDLES.lock()
        .map_err(|_| deno_core::error::AnyError::msg("lock error"))?;

    if handles.remove(&fd).is_some() {
        Ok(())
    } else {
        Err(deno_core::error::AnyError::msg("EBADF: bad file descriptor"))
    }
}

// ============================================================================
// File System Watch Operations
// ============================================================================

static NEXT_WATCHER_ID: AtomicU32 = AtomicU32::new(1);

struct WatcherHandle {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<Result<Event, notify::Error>>,
    path: String,
}

lazy_static::lazy_static! {
    static ref WATCHERS: Arc<std::sync::Mutex<HashMap<u32, WatcherHandle>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
}

#[derive(serde::Serialize)]
pub struct WatchStartResult {
    pub id: u32,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct WatchEvent {
    pub event_type: String,
    pub filename: Option<String>,
}

/// Start watching a file or directory
#[op2]
#[serde]
fn op_howth_fs_watch_start(
    #[string] path: &str,
    recursive: bool,
) -> WatchStartResult {
    let (tx, rx) = mpsc::channel();

    let watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    });

    match watcher {
        Ok(mut w) => {
            let watch_path = std::path::Path::new(path);
            let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };

            if let Err(e) = w.watch(watch_path, mode) {
                return WatchStartResult {
                    id: 0,
                    error: Some(format!("{}", e)),
                };
            }

            let id = NEXT_WATCHER_ID.fetch_add(1, Ordering::SeqCst);

            if let Ok(mut watchers) = WATCHERS.lock() {
                watchers.insert(id, WatcherHandle {
                    _watcher: w,
                    receiver: rx,
                    path: path.to_string(),
                });
            }

            WatchStartResult { id, error: None }
        }
        Err(e) => WatchStartResult {
            id: 0,
            error: Some(format!("{}", e)),
        },
    }
}

/// Poll for watch events (non-blocking, returns None if no events)
#[op2]
#[serde]
fn op_howth_fs_watch_poll(id: u32) -> Option<WatchEvent> {
    let watchers = match WATCHERS.lock() {
        Ok(w) => w,
        Err(_) => return None,
    };

    if let Some(handle) = watchers.get(&id) {
        // Non-blocking poll
        match handle.receiver.try_recv() {
            Ok(Ok(event)) => {
                // Map notify event kind to Node.js event type
                let event_type = match event.kind {
                    notify::EventKind::Create(_) => "rename",
                    notify::EventKind::Remove(_) => "rename",
                    notify::EventKind::Modify(_) => "change",
                    notify::EventKind::Access(_) => "change",
                    _ => "change",
                };

                // Get filename relative to watched path
                let filename = event.paths.first().and_then(|p| {
                    let watched = std::path::Path::new(&handle.path);
                    p.strip_prefix(watched)
                        .ok()
                        .map(|rel| rel.to_string_lossy().to_string())
                        .or_else(|| p.file_name().map(|n| n.to_string_lossy().to_string()))
                });

                Some(WatchEvent { event_type: event_type.to_string(), filename })
            }
            Ok(Err(_)) => None, // Watcher error
            Err(mpsc::TryRecvError::Empty) => None, // No events
            Err(mpsc::TryRecvError::Disconnected) => None, // Channel closed
        }
    } else {
        None
    }
}

/// Stop watching
#[op2(fast)]
fn op_howth_fs_watch_close(id: u32) -> bool {
    if let Ok(mut watchers) = WATCHERS.lock() {
        watchers.remove(&id).is_some()
    } else {
        false
    }
}

// ============================================================================
// Zlib Compression Operations
// ============================================================================

/// Gzip compress data
#[op2]
#[serde]
fn op_howth_zlib_gzip(#[serde] data: Vec<u8>, level: i32) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let compression = match level {
        -1 => Compression::default(),
        0..=9 => Compression::new(level as u32),
        _ => Compression::default(),
    };

    let mut encoder = GzEncoder::new(Vec::new(), compression);
    encoder.write_all(&data)?;
    encoder.finish().map_err(|e| deno_core::error::AnyError::msg(format!("gzip error: {}", e)))
}

/// Gzip decompress data
#[op2]
#[serde]
fn op_howth_zlib_gunzip(#[serde] data: Vec<u8>) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    let mut decoder = GzDecoder::new(&data[..]);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result)
        .map_err(|e| deno_core::error::AnyError::msg(format!("gunzip error: {}", e)))?;
    Ok(result)
}

/// Deflate compress data
#[op2]
#[serde]
fn op_howth_zlib_deflate(#[serde] data: Vec<u8>, level: i32) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::Compression;
    use flate2::write::DeflateEncoder;
    use std::io::Write;

    let compression = match level {
        -1 => Compression::default(),
        0..=9 => Compression::new(level as u32),
        _ => Compression::default(),
    };

    let mut encoder = DeflateEncoder::new(Vec::new(), compression);
    encoder.write_all(&data)?;
    encoder.finish().map_err(|e| deno_core::error::AnyError::msg(format!("deflate error: {}", e)))
}

/// Inflate decompress data
#[op2]
#[serde]
fn op_howth_zlib_inflate(#[serde] data: Vec<u8>) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    let mut decoder = DeflateDecoder::new(&data[..]);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result)
        .map_err(|e| deno_core::error::AnyError::msg(format!("inflate error: {}", e)))?;
    Ok(result)
}

/// Deflate raw compress data (no zlib header)
#[op2]
#[serde]
fn op_howth_zlib_deflate_raw(#[serde] data: Vec<u8>, level: i32) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::Compression;
    use flate2::write::DeflateEncoder;
    use std::io::Write;

    let compression = match level {
        -1 => Compression::default(),
        0..=9 => Compression::new(level as u32),
        _ => Compression::default(),
    };

    let mut encoder = DeflateEncoder::new(Vec::new(), compression);
    encoder.write_all(&data)?;
    encoder.finish().map_err(|e| deno_core::error::AnyError::msg(format!("deflate error: {}", e)))
}

/// Inflate raw decompress data (no zlib header)
#[op2]
#[serde]
fn op_howth_zlib_inflate_raw(#[serde] data: Vec<u8>) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    let mut decoder = DeflateDecoder::new(&data[..]);
    let mut result = Vec::new();
    decoder.read_to_end(&mut result)
        .map_err(|e| deno_core::error::AnyError::msg(format!("inflate error: {}", e)))?;
    Ok(result)
}

// ============================================================================
// Worker Threads Operations
// ============================================================================

static NEXT_WORKER_ID: AtomicU32 = AtomicU32::new(1);

/// Thread-local storage for worker context (set when running as a worker)
thread_local! {
    static WORKER_CONTEXT: RefCell<Option<WorkerContext>> = RefCell::new(None);
}

struct WorkerContext {
    worker_id: u32,
    parent_tx: mpsc::Sender<String>,
    parent_rx: mpsc::Receiver<String>,
}

struct WorkerHandle {
    thread: Option<std::thread::JoinHandle<()>>,
    tx: mpsc::Sender<String>,
    rx: mpsc::Receiver<String>,
    terminated: bool,
}

lazy_static::lazy_static! {
    static ref WORKERS: Arc<std::sync::Mutex<HashMap<u32, WorkerHandle>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));

    /// Flag indicating if this is the main thread
    static ref IS_MAIN_THREAD: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(true);

    /// Storage for messages from parent (when running as worker)
    static ref WORKER_INBOX: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    /// Storage for messages to parent (when running as worker)
    static ref WORKER_OUTBOX: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
}

#[derive(serde::Serialize)]
pub struct WorkerCreateResult {
    pub id: u32,
    pub error: Option<String>,
}

/// Check if this is the main thread
#[op2(fast)]
fn op_howth_worker_is_main_thread() -> bool {
    IS_MAIN_THREAD.load(std::sync::atomic::Ordering::SeqCst)
}

/// Get the current worker's thread ID (0 if main thread)
#[op2(fast)]
fn op_howth_worker_thread_id() -> u32 {
    WORKER_CONTEXT.with(|ctx| {
        ctx.borrow().as_ref().map(|c| c.worker_id).unwrap_or(0)
    })
}

/// Create a new worker thread
#[op2]
#[serde]
fn op_howth_worker_create(
    #[string] filename: &str,
    #[string] worker_data: &str,
) -> WorkerCreateResult {
    let worker_id = NEXT_WORKER_ID.fetch_add(1, Ordering::SeqCst);
    let filename = filename.to_string();
    let worker_data = worker_data.to_string();

    // Create message channels: main -> worker and worker -> main
    let (main_tx, worker_rx) = mpsc::channel::<String>();
    let (worker_tx, main_rx) = mpsc::channel::<String>();

    // Clone for the thread
    let worker_tx_clone = worker_tx.clone();

    // Spawn the worker thread
    let handle = std::thread::spawn(move || {
        // Mark this as a worker thread
        IS_MAIN_THREAD.store(false, std::sync::atomic::Ordering::SeqCst);

        // Set up worker context
        WORKER_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = Some(WorkerContext {
                worker_id,
                parent_tx: worker_tx_clone,
                parent_rx: worker_rx,
            });
        });

        // Create a new runtime for this worker
        if let Err(e) = run_worker_script(&filename, &worker_data) {
            eprintln!("Worker {} error: {}", worker_id, e);
        }
    });

    // Store the worker handle
    if let Ok(mut workers) = WORKERS.lock() {
        workers.insert(worker_id, WorkerHandle {
            thread: Some(handle),
            tx: main_tx,
            rx: main_rx,
            terminated: false,
        });
    }

    WorkerCreateResult { id: worker_id, error: None }
}

/// Run a script in a worker context
fn run_worker_script(filename: &str, _worker_data: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::runtime::Runtime;

    // Create a new tokio runtime for this thread
    let rt = Runtime::new()?;

    rt.block_on(async {
        // Create the extension
        let ext = howth_runtime::init_ops();

        // Create module loader
        let module_loader = Rc::new(HowthModuleLoader::new(
            std::path::PathBuf::from(filename).parent().unwrap_or(std::path::Path::new(".")).to_path_buf()
        ));

        // Create runtime options
        let options = DenoRuntimeOptions {
            module_loader: Some(module_loader),
            extensions: vec![ext],
            ..Default::default()
        };

        // Create the JS runtime
        let mut runtime = JsRuntime::new(options);

        // Execute bootstrap
        let bootstrap_code = include_str!("bootstrap.js");
        runtime.execute_script("<howth:bootstrap>", bootstrap_code)?;

        // Read and execute the worker script
        let script_content = std::fs::read_to_string(filename)?;
        let specifier = ModuleSpecifier::from_file_path(filename)
            .map_err(|_| deno_core::error::AnyError::msg("Invalid file path"))?;

        let module_id = runtime.load_main_es_module(&specifier).await?;
        let result = runtime.mod_evaluate(module_id);
        runtime.run_event_loop(Default::default()).await?;
        result.await?;

        Ok::<(), deno_core::error::AnyError>(())
    })?;

    Ok(())
}

/// Post a message to a worker
#[op2(fast)]
fn op_howth_worker_post_message(worker_id: u32, #[string] message: &str) -> bool {
    if let Ok(workers) = WORKERS.lock() {
        if let Some(worker) = workers.get(&worker_id) {
            if !worker.terminated {
                return worker.tx.send(message.to_string()).is_ok();
            }
        }
    }
    false
}

/// Receive a message from a worker (non-blocking)
#[op2]
#[string]
fn op_howth_worker_recv_message(worker_id: u32) -> Option<String> {
    if let Ok(workers) = WORKERS.lock() {
        if let Some(worker) = workers.get(&worker_id) {
            return worker.rx.try_recv().ok();
        }
    }
    None
}

/// Post a message to the parent (from worker)
#[op2(fast)]
fn op_howth_worker_parent_post(#[string] message: &str) -> bool {
    WORKER_CONTEXT.with(|ctx| {
        if let Some(ref context) = *ctx.borrow() {
            return context.parent_tx.send(message.to_string()).is_ok();
        }
        false
    })
}

/// Receive a message from the parent (from worker, non-blocking)
#[op2]
#[string]
fn op_howth_worker_parent_recv() -> Option<String> {
    WORKER_CONTEXT.with(|ctx| {
        if let Some(ref context) = *ctx.borrow() {
            return context.parent_rx.try_recv().ok();
        }
        None
    })
}

/// Terminate a worker
#[op2(fast)]
fn op_howth_worker_terminate(worker_id: u32) -> bool {
    if let Ok(mut workers) = WORKERS.lock() {
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.terminated = true;
            // Note: We can't forcefully kill a thread in Rust
            // The worker should check for termination and exit gracefully
            return true;
        }
    }
    false
}

/// Check if a worker is still running
#[op2(fast)]
fn op_howth_worker_is_running(worker_id: u32) -> bool {
    if let Ok(workers) = WORKERS.lock() {
        if let Some(worker) = workers.get(&worker_id) {
            if let Some(ref handle) = worker.thread {
                return !handle.is_finished();
            }
        }
    }
    false
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

// ============================================================================
// Async Child Process Operations (for proper spawn with streams)
// ============================================================================

use tokio::sync::Mutex;
use tokio::process::{Child, ChildStdout, ChildStderr, ChildStdin, Command as TokioCommand};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Global child process ID counter
static NEXT_CHILD_ID: AtomicU32 = AtomicU32::new(1);

/// Storage for spawned child processes (just the Child for wait/kill)
lazy_static::lazy_static! {
    static ref CHILD_PROCESSES: Arc<Mutex<HashMap<u32, Child>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref CHILD_STDOUTS: Arc<Mutex<HashMap<u32, ChildStdout>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref CHILD_STDERRS: Arc<Mutex<HashMap<u32, ChildStderr>>> =
        Arc::new(Mutex::new(HashMap::new()));
    static ref CHILD_STDINS: Arc<Mutex<HashMap<u32, ChildStdin>>> =
        Arc::new(Mutex::new(HashMap::new()));
    // Store PIDs separately for sync kill operation
    static ref CHILD_PIDS: Arc<std::sync::Mutex<HashMap<u32, u32>>> =
        Arc::new(std::sync::Mutex::new(HashMap::new()));
}

/// Result of spawning a process asynchronously
#[derive(serde::Serialize)]
pub struct SpawnAsyncResult {
    pub id: u32,
    pub pid: u32,
    pub error: Option<String>,
}

/// Spawn a process asynchronously (returns immediately with handle)
#[op2(async)]
#[serde]
async fn op_howth_spawn_async(
    #[string] command: String,
    #[serde] args: Vec<String>,
    #[serde] options: Option<SpawnOptions>,
) -> SpawnAsyncResult {
    use std::process::Stdio;

    let opts = options.unwrap_or_default();
    let use_shell = opts.shell.unwrap_or(false);

    let mut cmd = if use_shell {
        #[cfg(windows)]
        {
            let mut c = TokioCommand::new("cmd");
            c.arg("/C").arg(&command);
            for arg in &args {
                c.arg(arg);
            }
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = TokioCommand::new("/bin/sh");
            let full_cmd = if args.is_empty() {
                command.clone()
            } else {
                format!("{} {}", command, args.join(" "))
            };
            c.arg("-c").arg(full_cmd);
            c
        }
    } else {
        let mut c = TokioCommand::new(&command);
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

    // Configure stdio for piping
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let pid = child.id().unwrap_or(0);
            let id = NEXT_CHILD_ID.fetch_add(1, Ordering::SeqCst);

            // Store PID for sync kill operation
            if let Ok(mut pids) = CHILD_PIDS.lock() {
                pids.insert(id, pid);
            }

            // Take ownership of stdio streams and store separately
            if let Some(stdout) = child.stdout.take() {
                CHILD_STDOUTS.lock().await.insert(id, stdout);
            }
            if let Some(stderr) = child.stderr.take() {
                CHILD_STDERRS.lock().await.insert(id, stderr);
            }
            if let Some(stdin) = child.stdin.take() {
                CHILD_STDINS.lock().await.insert(id, stdin);
            }

            CHILD_PROCESSES.lock().await.insert(id, child);

            SpawnAsyncResult {
                id,
                pid,
                error: None,
            }
        }
        Err(e) => SpawnAsyncResult {
            id: 0,
            pid: 0,
            error: Some(format!("ENOENT: spawn error: {}", e)),
        },
    }
}

/// Read from a child process's stdout
#[op2(async)]
#[serde]
async fn op_howth_spawn_read_stdout(id: u32) -> Result<Option<Vec<u8>>, deno_core::error::AnyError> {
    // Take the stdout temporarily
    let stdout_opt = CHILD_STDOUTS.lock().await.remove(&id);

    if let Some(mut stdout) = stdout_opt {
        let mut buf = vec![0u8; 8192];
        let result = stdout.read(&mut buf).await;

        // Put it back
        CHILD_STDOUTS.lock().await.insert(id, stdout);

        match result {
            Ok(0) => Ok(None), // EOF
            Ok(n) => {
                buf.truncate(n);
                Ok(Some(buf))
            }
            Err(e) => Err(deno_core::error::AnyError::msg(format!("read error: {}", e))),
        }
    } else {
        Ok(None) // stdout not available or already consumed
    }
}

/// Read from a child process's stderr
#[op2(async)]
#[serde]
async fn op_howth_spawn_read_stderr(id: u32) -> Result<Option<Vec<u8>>, deno_core::error::AnyError> {
    // Take the stderr temporarily
    let stderr_opt = CHILD_STDERRS.lock().await.remove(&id);

    if let Some(mut stderr) = stderr_opt {
        let mut buf = vec![0u8; 8192];
        let result = stderr.read(&mut buf).await;

        // Put it back
        CHILD_STDERRS.lock().await.insert(id, stderr);

        match result {
            Ok(0) => Ok(None), // EOF
            Ok(n) => {
                buf.truncate(n);
                Ok(Some(buf))
            }
            Err(e) => Err(deno_core::error::AnyError::msg(format!("read error: {}", e))),
        }
    } else {
        Ok(None) // stderr not available or already consumed
    }
}

/// Write to a child process's stdin
#[op2(async)]
async fn op_howth_spawn_write_stdin(id: u32, #[buffer(copy)] data: Vec<u8>) -> Result<u32, deno_core::error::AnyError> {
    // Take the stdin temporarily
    let stdin_opt = CHILD_STDINS.lock().await.remove(&id);

    if let Some(mut stdin) = stdin_opt {
        let result = stdin.write(&data).await;

        // Put it back
        CHILD_STDINS.lock().await.insert(id, stdin);

        match result {
            Ok(n) => Ok(n as u32),
            Err(e) => Err(deno_core::error::AnyError::msg(format!("write error: {}", e))),
        }
    } else {
        Err(deno_core::error::AnyError::msg("stdin not available"))
    }
}

/// Close a child process's stdin
#[op2(async)]
async fn op_howth_spawn_close_stdin(id: u32) -> Result<(), deno_core::error::AnyError> {
    // Remove and drop stdin to close it
    CHILD_STDINS.lock().await.remove(&id);
    Ok(())
}

/// Result of waiting for a child process
#[derive(serde::Serialize)]
pub struct SpawnWaitResult {
    pub code: Option<i32>,
    pub signal: Option<String>,
}

/// Wait for a child process to exit
#[op2(async)]
#[serde]
async fn op_howth_spawn_wait(id: u32) -> Result<SpawnWaitResult, deno_core::error::AnyError> {
    // Take ownership of the child from the map
    let child_opt = CHILD_PROCESSES.lock().await.remove(&id);

    if let Some(mut child) = child_opt {
        match child.wait().await {
            Ok(status) => {
                // Clean up stdio
                CHILD_STDOUTS.lock().await.remove(&id);
                CHILD_STDERRS.lock().await.remove(&id);
                CHILD_STDINS.lock().await.remove(&id);

                let code = status.code();
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt;
                    status.signal().map(|s| format!("{}", s))
                };
                #[cfg(not(unix))]
                let signal = None;

                Ok(SpawnWaitResult { code, signal })
            }
            Err(e) => Err(deno_core::error::AnyError::msg(format!("wait error: {}", e))),
        }
    } else {
        Err(deno_core::error::AnyError::msg("child process not found"))
    }
}

/// Kill a child process
#[op2]
fn op_howth_spawn_kill(id: u32, #[string] signal: Option<String>) -> Result<bool, deno_core::error::AnyError> {
    // Use the sync PID storage to avoid async mutex issues
    let pid_opt = CHILD_PIDS.lock().ok().and_then(|pids| pids.get(&id).copied());

    let result = if let Some(pid) = pid_opt {
        if pid == 0 {
            return Ok(false);
        }

        #[cfg(unix)]
        {
            let sig = match signal.as_deref() {
                Some("SIGTERM") | None => libc::SIGTERM,
                Some("SIGKILL") => libc::SIGKILL,
                Some("SIGINT") => libc::SIGINT,
                Some("SIGHUP") => libc::SIGHUP,
                _ => libc::SIGTERM,
            };
            let ret = unsafe { libc::kill(pid as i32, sig) };
            ret == 0
        }
        #[cfg(not(unix))]
        {
            let _ = signal;
            false // Windows: would need different implementation
        }
    } else {
        false
    };

    Ok(result)
}

// HTTP Server implementation using hyper

/// Global server ID counter
static NEXT_SERVER_ID: AtomicU32 = AtomicU32::new(1);

/// Represents an HTTP request from a client
#[derive(serde::Serialize, Clone)]
pub struct HttpRequest {
    pub id: u32,
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// Represents an HTTP response to send
#[derive(serde::Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
}

/// Server state stored in a global map
struct ServerState {
    listener: tokio::net::TcpListener,
    local_addr: std::net::SocketAddr,
}

/// Global map of active HTTP servers
lazy_static::lazy_static! {
    static ref HTTP_SERVERS: Mutex<HashMap<u32, Arc<Mutex<ServerState>>>> = Mutex::new(HashMap::new());
}

/// Start listening on a port and return a server ID
#[op2(async)]
#[serde]
async fn op_howth_http_listen(
    port: u16,
    #[string] hostname: String,
) -> Result<serde_json::Value, deno_core::error::AnyError> {
    let addr = format!("{}:{}", hostname, port);
    let listener = tokio::net::TcpListener::bind(&addr).await
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to bind: {}", e)))?;

    let local_addr = listener.local_addr()
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to get local addr: {}", e)))?;

    let server_id = NEXT_SERVER_ID.fetch_add(1, Ordering::SeqCst);

    let state = ServerState {
        listener,
        local_addr,
    };

    HTTP_SERVERS.lock().await.insert(server_id, Arc::new(Mutex::new(state)));

    Ok(serde_json::json!({
        "id": server_id,
        "address": local_addr.ip().to_string(),
        "port": local_addr.port(),
    }))
}

/// Accept a connection and read an HTTP request
#[op2(async)]
#[serde]
async fn op_howth_http_accept(server_id: u32) -> Result<Option<HttpRequest>, deno_core::error::AnyError> {
    let servers = HTTP_SERVERS.lock().await;
    let server = servers.get(&server_id)
        .ok_or_else(|| deno_core::error::AnyError::msg("Server not found"))?
        .clone();
    drop(servers);

    let mut state = server.lock().await;

    // Accept a connection
    let (mut stream, _addr) = match state.listener.accept().await {
        Ok(conn) => conn,
        Err(e) => {
            return Err(deno_core::error::AnyError::msg(format!("Accept failed: {}", e)));
        }
    };

    drop(state);

    // Read the HTTP request
    use tokio::io::AsyncReadExt;
    let mut buffer = vec![0u8; 8192];
    let n = stream.read(&mut buffer).await
        .map_err(|e| deno_core::error::AnyError::msg(format!("Read failed: {}", e)))?;

    if n == 0 {
        // Connection closed, stream drops here (no leak)
        return Ok(None);
    }

    // Clean up any stale connections (older than 60 seconds)
    {
        let mut connections = get_connections().lock().await;
        if connections.len() > 100 {
            // If we have too many pending connections, something is wrong
            // Drop the oldest ones to prevent FD exhaustion
            let ids_to_remove: Vec<u32> = connections.keys().copied().collect();
            let remove_count = connections.len().saturating_sub(50);
            for id in ids_to_remove.into_iter().take(remove_count) {
                connections.remove(&id);
            }
        }
    }

    buffer.truncate(n);
    let request_str = String::from_utf8_lossy(&buffer);

    // Parse the HTTP request
    let mut lines = request_str.lines();
    let request_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = request_line.split_whitespace().collect();

    if parts.len() < 2 {
        return Err(deno_core::error::AnyError::msg("Invalid HTTP request"));
    }

    let method = parts[0].to_string();
    let url = parts[1].to_string();

    // Parse headers
    let mut headers = HashMap::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    // Find body (after empty line)
    let body = if let Some(pos) = request_str.find("\r\n\r\n") {
        request_str[pos + 4..].to_string()
    } else if let Some(pos) = request_str.find("\n\n") {
        request_str[pos + 2..].to_string()
    } else {
        String::new()
    };

    // Store the stream for later response
    let request_id = NEXT_SERVER_ID.fetch_add(1, Ordering::SeqCst);

    // Store connection for response
    get_connections().lock().await.insert(request_id, stream);

    Ok(Some(HttpRequest {
        id: request_id,
        method,
        url,
        headers,
        body,
    }))
}

/// Connection storage for pending responses
static PENDING_CONNECTIONS: std::sync::OnceLock<Mutex<HashMap<u32, tokio::net::TcpStream>>> = std::sync::OnceLock::new();

fn get_connections() -> &'static Mutex<HashMap<u32, tokio::net::TcpStream>> {
    PENDING_CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Send an HTTP response
#[op2(async)]
async fn op_howth_http_respond(
    request_id: u32,
    #[serde] response: HttpResponse,
) -> Result<(), deno_core::error::AnyError> {
    use tokio::io::AsyncWriteExt;

    // Get the connection
    let mut connections = get_connections().lock().await;
    let mut stream = connections.remove(&request_id)
        .ok_or_else(|| deno_core::error::AnyError::msg("Connection not found"))?;
    drop(connections);

    // Build the HTTP response
    let status_text = match response.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    };

    let body = response.body.unwrap_or_default();
    let mut response_text = format!("HTTP/1.1 {} {}\r\n", response.status, status_text);

    // Add headers
    if let Some(headers) = response.headers {
        for (key, value) in headers {
            response_text.push_str(&format!("{}: {}\r\n", key, value));
        }
    }

    // Add content-length if not present
    if !response_text.to_lowercase().contains("content-length") {
        response_text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    // End headers
    response_text.push_str("\r\n");

    // Add body
    response_text.push_str(&body);

    // Write response
    stream.write_all(response_text.as_bytes()).await
        .map_err(|e| deno_core::error::AnyError::msg(format!("Write failed: {}", e)))?;

    stream.flush().await
        .map_err(|e| deno_core::error::AnyError::msg(format!("Flush failed: {}", e)))?;

    Ok(())
}

/// Close an HTTP server
#[op2(async)]
async fn op_howth_http_close(server_id: u32) -> Result<(), deno_core::error::AnyError> {
    let mut servers = HTTP_SERVERS.lock().await;
    servers.remove(&server_id);
    Ok(())
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

/// Change current working directory.
#[op2(fast)]
fn op_howth_chdir(#[string] path: &str) -> Result<(), deno_core::error::AnyError> {
    std::env::set_current_dir(path).map_err(|e| e.into())
}

/// Get the operating system platform (Node.js style).
#[op2]
#[string]
fn op_howth_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "freebsd") {
        "freebsd"
    } else {
        "unknown"
    }
}

/// Get the CPU architecture (Node.js style).
#[op2]
#[string]
fn op_howth_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "ia32"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        "unknown"
    }
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
    // Use script args if set, otherwise fall back to system args
    if let Some(args) = SCRIPT_ARGS.get() {
        args.clone()
    } else {
        std::env::args().collect()
    }
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

/// Hash data using various algorithms (cryptographic).
#[op2]
#[serde]
fn op_howth_hash(
    #[string] algorithm: &str,
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use digest::Digest;

    match algorithm.to_lowercase().as_str() {
        "sha-1" | "sha1" => {
            let mut hasher = sha1::Sha1::new();
            hasher.update(data);
            Ok(hasher.finalize().to_vec())
        }
        "sha-256" | "sha256" => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(data);
            Ok(hasher.finalize().to_vec())
        }
        "sha-384" | "sha384" => {
            let mut hasher = sha2::Sha384::new();
            hasher.update(data);
            Ok(hasher.finalize().to_vec())
        }
        "sha-512" | "sha512" => {
            let mut hasher = sha2::Sha512::new();
            hasher.update(data);
            Ok(hasher.finalize().to_vec())
        }
        "md5" => {
            // MD5 is not available via sha2 crate; keep a simple implementation
            // for backwards compat. For now, use the md-5 crate if available,
            // otherwise fall back to a non-cryptographic placeholder.
            // deno_node pulls in md-5 transitively.
            Err(deno_core::error::AnyError::msg(
                "MD5 hashing should be handled in JavaScript"
            ))
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported algorithm: {}",
            algorithm
        ))),
    }
}

/// HMAC computation using real HMAC crate.
#[op2]
#[serde]
fn op_howth_hmac(
    #[string] algorithm: &str,
    #[buffer] key: &[u8],
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use hmac::{Hmac, Mac};

    match algorithm.to_lowercase().as_str() {
        "sha-1" | "sha1" => {
            let mut mac = Hmac::<sha1::Sha1>::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("HMAC key error: {}", e)))?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "sha-256" | "sha256" => {
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("HMAC key error: {}", e)))?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "sha-384" | "sha384" => {
            let mut mac = Hmac::<sha2::Sha384>::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("HMAC key error: {}", e)))?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        "sha-512" | "sha512" => {
            let mut mac = Hmac::<sha2::Sha512>::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("HMAC key error: {}", e)))?;
            mac.update(data);
            Ok(mac.finalize().into_bytes().to_vec())
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported HMAC algorithm: {}",
            algorithm
        ))),
    }
}

/// Symmetric cipher operation (CBC, CTR modes).
#[op2]
#[serde]
fn op_howth_cipher(
    #[string] algorithm: &str,
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] data: &[u8],
    encrypt: bool,
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, StreamCipher};

    match algorithm.to_lowercase().as_str() {
        "aes-128-cbc" | "aes-256-cbc" => {
            use cipher::block_padding::Pkcs7;
            if key.len() == 16 {
                if encrypt {
                    let ct = cbc::Encryptor::<aes::Aes128>::new_from_slices(key, iv)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?
                        .encrypt_padded_vec_mut::<Pkcs7>(data);
                    Ok(ct)
                } else {
                    let pt = cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?
                        .decrypt_padded_vec_mut::<Pkcs7>(data)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Decryption error: {}", e)))?;
                    Ok(pt)
                }
            } else {
                if encrypt {
                    let ct = cbc::Encryptor::<aes::Aes256>::new_from_slices(key, iv)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?
                        .encrypt_padded_vec_mut::<Pkcs7>(data);
                    Ok(ct)
                } else {
                    let pt = cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?
                        .decrypt_padded_vec_mut::<Pkcs7>(data)
                        .map_err(|e| deno_core::error::AnyError::msg(format!("Decryption error: {}", e)))?;
                    Ok(pt)
                }
            }
        }
        "aes-128-ctr" | "aes-256-ctr" => {
            let mut buf = data.to_vec();
            if key.len() == 16 {
                let mut cipher = ctr::Ctr128BE::<aes::Aes128>::new_from_slices(key, iv)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?;
                cipher.apply_keystream(&mut buf);
            } else {
                let mut cipher = ctr::Ctr128BE::<aes::Aes256>::new_from_slices(key, iv)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("Cipher init error: {}", e)))?;
                cipher.apply_keystream(&mut buf);
            }
            Ok(buf)
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported cipher algorithm: {}",
            algorithm
        ))),
    }
}

/// AES-GCM cipher operation. Returns (ciphertext/plaintext, auth_tag).
/// For encrypt: returns (ciphertext, 16-byte auth_tag).
/// For decrypt: auth_tag is passed in via `tag` param; returns (plaintext, empty).
#[op2]
#[serde]
fn op_howth_cipher_gcm(
    #[string] algorithm: &str,
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] data: &[u8],
    #[buffer] aad: &[u8],
    #[buffer] tag: &[u8],
    encrypt: bool,
) -> Result<(Vec<u8>, Vec<u8>), deno_core::error::AnyError> {
    use aes_gcm::{Aes128Gcm, Aes256Gcm, KeyInit, AeadInPlace, Nonce, Tag};

    match algorithm.to_lowercase().as_str() {
        "aes-128-gcm" => {
            let cipher = Aes128Gcm::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("GCM key error: {}", e)))?;
            let nonce = Nonce::from_slice(iv);
            if encrypt {
                let mut buffer = data.to_vec();
                let auth_tag = cipher.encrypt_in_place_detached(nonce, aad, &mut buffer)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("GCM encrypt error: {}", e)))?;
                Ok((buffer, auth_tag.to_vec()))
            } else {
                let mut buffer = data.to_vec();
                let tag_arr = Tag::from_slice(tag);
                cipher.decrypt_in_place_detached(nonce, aad, &mut buffer, tag_arr)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("GCM decrypt error: {}", e)))?;
                Ok((buffer, vec![]))
            }
        }
        "aes-256-gcm" => {
            let cipher = Aes256Gcm::new_from_slice(key)
                .map_err(|e| deno_core::error::AnyError::msg(format!("GCM key error: {}", e)))?;
            let nonce = Nonce::from_slice(iv);
            if encrypt {
                let mut buffer = data.to_vec();
                let auth_tag = cipher.encrypt_in_place_detached(nonce, aad, &mut buffer)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("GCM encrypt error: {}", e)))?;
                Ok((buffer, auth_tag.to_vec()))
            } else {
                let mut buffer = data.to_vec();
                let tag_arr = Tag::from_slice(tag);
                cipher.decrypt_in_place_detached(nonce, aad, &mut buffer, tag_arr)
                    .map_err(|e| deno_core::error::AnyError::msg(format!("GCM decrypt error: {}", e)))?;
                Ok((buffer, vec![]))
            }
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported GCM algorithm: {}",
            algorithm
        ))),
    }
}

/// RSA sign with PKCS#1 v1.5 padding.
#[op2]
#[serde]
fn op_howth_sign(
    #[string] algorithm: &str,
    #[string] key_pem: &str,
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::SignerMut;

    let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(key_pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            rsa::RsaPrivateKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to parse private key: {}", e)))?;

    match algorithm.to_lowercase().as_str() {
        "sha256" | "sha-256" => {
            let mut signing_key = SigningKey::<sha2::Sha256>::new(private_key);
            let signature = signing_key.sign(data);
            use rsa::signature::SignatureEncoding;
            Ok(signature.to_bytes().to_vec())
        }
        "sha384" | "sha-384" => {
            let mut signing_key = SigningKey::<sha2::Sha384>::new(private_key);
            let signature = signing_key.sign(data);
            use rsa::signature::SignatureEncoding;
            Ok(signature.to_bytes().to_vec())
        }
        "sha512" | "sha-512" => {
            let mut signing_key = SigningKey::<sha2::Sha512>::new(private_key);
            let signature = signing_key.sign(data);
            use rsa::signature::SignatureEncoding;
            Ok(signature.to_bytes().to_vec())
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported signing algorithm: {}",
            algorithm
        ))),
    }
}

/// RSA verify with PKCS#1 v1.5 padding.
#[op2(fast)]
fn op_howth_verify(
    #[string] algorithm: &str,
    #[string] key_pem: &str,
    #[buffer] signature: &[u8],
    #[buffer] data: &[u8],
) -> Result<bool, deno_core::error::AnyError> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    let public_key = rsa::RsaPublicKey::from_public_key_pem(key_pem)
        .or_else(|_| {
            // Try parsing as PKCS#1 format
            use rsa::pkcs1::DecodeRsaPublicKey;
            rsa::RsaPublicKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to parse public key: {}", e)))?;

    let sig = Signature::try_from(signature)
        .map_err(|e| deno_core::error::AnyError::msg(format!("Invalid signature: {}", e)))?;

    match algorithm.to_lowercase().as_str() {
        "sha256" | "sha-256" => {
            let verifying_key = VerifyingKey::<sha2::Sha256>::new(public_key);
            Ok(verifying_key.verify(data, &sig).is_ok())
        }
        "sha384" | "sha-384" => {
            let verifying_key = VerifyingKey::<sha2::Sha384>::new(public_key);
            Ok(verifying_key.verify(data, &sig).is_ok())
        }
        "sha512" | "sha-512" => {
            let verifying_key = VerifyingKey::<sha2::Sha512>::new(public_key);
            Ok(verifying_key.verify(data, &sig).is_ok())
        }
        _ => Err(deno_core::error::AnyError::msg(format!(
            "Unsupported verify algorithm: {}",
            algorithm
        ))),
    }
}

/// RSA public encrypt (OAEP with SHA-1, Node.js default).
#[op2]
#[serde]
fn op_howth_public_encrypt(
    #[string] key_pem: &str,
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::Oaep;

    let public_key = rsa::RsaPublicKey::from_public_key_pem(key_pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPublicKey;
            rsa::RsaPublicKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to parse public key: {}", e)))?;

    let mut rng = rand::thread_rng();
    let padding = Oaep::new::<sha1::Sha1>();
    let encrypted = public_key
        .encrypt(&mut rng, padding, data)
        .map_err(|e| deno_core::error::AnyError::msg(format!("RSA encrypt error: {}", e)))?;
    Ok(encrypted)
}

/// RSA private decrypt (OAEP with SHA-1, Node.js default).
#[op2]
#[serde]
fn op_howth_private_decrypt(
    #[string] key_pem: &str,
    #[buffer] data: &[u8],
) -> Result<Vec<u8>, deno_core::error::AnyError> {
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::Oaep;

    let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(key_pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            rsa::RsaPrivateKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|e| deno_core::error::AnyError::msg(format!("Failed to parse private key: {}", e)))?;

    let padding = Oaep::new::<sha1::Sha1>();
    let decrypted = private_key
        .decrypt(padding, data)
        .map_err(|e| deno_core::error::AnyError::msg(format!("RSA decrypt error: {}", e)))?;
    Ok(decrypted)
}

/// Generate an RSA key pair (returns PEM strings).
#[op2]
#[serde]
fn op_howth_generate_rsa_keypair(
    modulus_length: u32,
) -> Result<(String, String), deno_core::error::AnyError> {
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::pkcs8::EncodePublicKey;

    let mut rng = rand::thread_rng();
    let private_key = rsa::RsaPrivateKey::new(&mut rng, modulus_length as usize)
        .map_err(|e| deno_core::error::AnyError::msg(format!("RSA keygen error: {}", e)))?;
    let public_key = rsa::RsaPublicKey::from(&private_key);

    let private_pem = private_key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| deno_core::error::AnyError::msg(format!("PEM encode error: {}", e)))?;
    let public_pem = public_key
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| deno_core::error::AnyError::msg(format!("PEM encode error: {}", e)))?;

    Ok((public_pem, private_pem.to_string()))
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

        // Set script args if provided (for process.argv)
        if let Some(args) = options.args {
            let _ = SCRIPT_ARGS.set(args);
        }

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
// Force rebuild Wed Jan 28 13:43:43 IST 2026
