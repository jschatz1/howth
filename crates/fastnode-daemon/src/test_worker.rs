//! Warm Node.js test worker for `howth test`.
//!
//! Keeps a long-running Node.js child process that executes tests via `node:test`.
//! Communication is newline-delimited JSON over stdin/stdout pipes.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, warn};

/// The embedded test worker JavaScript source.
const WORKER_JS: &str = include_str!("test_worker.mjs");

/// A file to send to the worker (already transpiled).
#[derive(Debug, Clone, Serialize)]
pub struct TranspiledTestFile {
    /// Original source path (for display).
    pub path: String,
    /// Transpiled JavaScript code.
    pub code: String,
}

/// Message sent to the worker via stdin.
#[derive(Debug, Serialize)]
struct WorkerRequest {
    id: String,
    files: Vec<TranspiledTestFile>,
    #[serde(default)]
    force_exit: bool,
}

/// Message received from the worker via stdout.
#[derive(Debug, Deserialize)]
pub struct WorkerResponse {
    pub id: String,
    pub ok: bool,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_ms: f64,
    #[serde(default)]
    pub tests: Vec<WorkerTestCase>,
    #[serde(default)]
    pub diagnostics: String,
}

/// Individual test result from the worker.
#[derive(Debug, Deserialize)]
pub struct WorkerTestCase {
    pub name: String,
    #[serde(default)]
    pub file: String,
    pub status: String,
    #[serde(default)]
    pub duration_ms: f64,
    pub error: Option<String>,
}

/// Manages a warm Node.js child process for running tests.
pub struct NodeTestWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    worker_script_path: std::path::PathBuf,
    next_id: u64,
    /// Handle for the stderr drain task (keeps it alive).
    _stderr_drain: tokio::task::JoinHandle<()>,
}

impl NodeTestWorker {
    /// Spawn a new Node.js test worker process.
    pub async fn spawn() -> io::Result<Self> {
        // Write the worker script to a temp file
        let worker_script_path = std::env::temp_dir().join("howth-test-worker.mjs");
        tokio::fs::write(&worker_script_path, WORKER_JS).await?;

        let (child, stdin, stdout, stderr_drain) = Self::spawn_node(&worker_script_path)?;

        let pid: u32 = child.id().unwrap_or(0);
        debug!("spawned test worker (pid={})", pid);

        Ok(Self {
            child,
            stdin,
            stdout,
            worker_script_path,
            next_id: 0,
            _stderr_drain: stderr_drain,
        })
    }

    fn spawn_node(
        script_path: &Path,
    ) -> io::Result<(
        Child,
        ChildStdin,
        BufReader<ChildStdout>,
        tokio::task::JoinHandle<()>,
    )> {
        let mut child = Command::new("node")
            .arg(script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stderr"))?;

        // Drain stderr continuously to prevent pipe buffer deadlock.
        // Without this, any stdout/stderr writes from test code (e.g. console.log
        // during module loading of googleapis, @sentry/node, etc.) fill the 16KB
        // macOS pipe buffer and block the entire Node process.
        let stderr_drain = tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(stderr);
            let mut buf = [0u8; 8192];
            loop {
                match tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {} // discard
                }
            }
        });

        Ok((child, stdin, BufReader::new(stdout), stderr_drain))
    }

    /// Check if the worker process is still alive.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Respawn the worker if it has died.
    async fn ensure_alive(&mut self) -> io::Result<()> {
        if !self.is_alive() {
            warn!("test worker died, respawning");
            let (child, stdin, stdout, stderr_drain) = Self::spawn_node(&self.worker_script_path)?;
            self.child = child;
            self.stdin = stdin;
            self.stdout = stdout;
            self._stderr_drain = stderr_drain;
            debug!(
                "respawned test worker (pid={})",
                self.child.id().unwrap_or(0)
            );
        }
        Ok(())
    }

    /// Run tests on the warm worker.
    ///
    /// Sends transpiled files to the worker and waits for results.
    /// On timeout, cleans up temp files that the killed worker can't clean up
    /// (SIGKILL from kill_on_drop bypasses JS cleanup handlers).
    pub async fn run_tests(
        &mut self,
        files: Vec<TranspiledTestFile>,
        timeout_ms: Option<u64>,
        force_exit: bool,
    ) -> io::Result<WorkerResponse> {
        self.ensure_alive().await?;

        self.next_id += 1;
        let id = format!("t{}", self.next_id);
        let worker_pid = self.child.id().unwrap_or(0);

        // Pre-compute temp file paths so we can clean up on timeout.
        // Must match the JS worker's naming: `.howth-test-{pid}-{id}-{name}{ext}`
        // where {name} has .test/.spec stripped to avoid node:test discovery.
        let temp_file_paths: Vec<PathBuf> = files
            .iter()
            .map(|f| {
                let p = Path::new(&f.path);
                let dir = p.parent().unwrap_or(Path::new("."));
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("test");
                let name = stem
                    .strip_suffix(".test")
                    .or_else(|| stem.strip_suffix(".spec"))
                    .unwrap_or(stem);
                let ext = if Path::new(&f.path)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("cjs"))
                    || Path::new(&f.path)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("cts"))
                {
                    ".cjs"
                } else {
                    ".mjs"
                };
                dir.join(format!(".howth-test-{worker_pid}-{id}-{name}{ext}"))
            })
            .collect();

        let request = WorkerRequest {
            id: id.clone(),
            files,
            force_exit,
        };

        // Send request as newline-delimited JSON
        let mut json = serde_json::to_string(&request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push('\n');

        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.flush().await?;

        // Read response line with timeout
        let timeout_secs = timeout_ms.unwrap_or(120_000) / 1000;
        let mut line = String::new();
        let read_result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            self.stdout.read_line(&mut line),
        )
        .await;

        match read_result {
            Ok(Ok(0)) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "worker closed stdout",
            )),
            Ok(Ok(_)) => {
                let response: WorkerResponse = serde_json::from_str(line.trim())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if response.id != id {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("response id mismatch: expected {id}, got {}", response.id),
                    ));
                }
                Ok(response)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Timeout: the worker will be killed (kill_on_drop) which sends
                // SIGKILL, bypassing JS cleanup handlers. Clean up temp files
                // from the Rust side so they don't accumulate across runs.
                warn!(
                    "test worker timed out after {timeout_secs}s, cleaning up {} temp files",
                    temp_file_paths.len()
                );
                for path in &temp_file_paths {
                    if let Err(e) = std::fs::remove_file(path) {
                        if e.kind() != io::ErrorKind::NotFound {
                            debug!("failed to clean up temp file {}: {e}", path.display());
                        }
                    }
                }
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("test worker timed out after {timeout_secs}s"),
                ))
            }
        }
    }
}

impl Drop for NodeTestWorker {
    fn drop(&mut self) {
        // kill_on_drop handles child cleanup
        let _ = std::fs::remove_file(&self.worker_script_path);
    }
}

/// Clean up stale `.howth-test-*` temp files in the directories containing the
/// given test files. Called by the daemon before each test run to remove leftovers
/// from previous runs that timed out or were killed (where neither the JS cleanup
/// handlers nor the Rust-side timeout cleanup ran successfully).
pub fn cleanup_stale_temp_files(file_paths: &[String]) {
    let mut seen_dirs = std::collections::HashSet::new();
    for file_path in file_paths {
        let p = Path::new(file_path);
        if let Some(dir) = p.parent() {
            if !seen_dirs.insert(dir.to_path_buf()) {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with(".howth-test-") {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }
}
