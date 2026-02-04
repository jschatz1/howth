//! Warm Node.js test worker for `howth test`.
//!
//! Keeps a long-running Node.js child process that executes tests via `node:test`.
//! Communication is newline-delimited JSON over stdin/stdout pipes.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
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
}

impl NodeTestWorker {
    /// Spawn a new Node.js test worker process.
    pub async fn spawn() -> io::Result<Self> {
        // Write the worker script to a temp file
        let worker_script_path = std::env::temp_dir().join("howth-test-worker.mjs");
        tokio::fs::write(&worker_script_path, WORKER_JS).await?;

        let (child, stdin, stdout) = Self::spawn_node(&worker_script_path)?;

        let pid: u32 = child.id().unwrap_or(0);
        debug!("spawned test worker (pid={})", pid);

        Ok(Self {
            child,
            stdin,
            stdout,
            worker_script_path,
            next_id: 0,
        })
    }

    fn spawn_node(script_path: &Path) -> io::Result<(Child, ChildStdin, BufReader<ChildStdout>)> {
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

        Ok((child, stdin, BufReader::new(stdout)))
    }

    /// Check if the worker process is still alive.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Respawn the worker if it has died.
    async fn ensure_alive(&mut self) -> io::Result<()> {
        if !self.is_alive() {
            warn!("test worker died, respawning");
            let (child, stdin, stdout) = Self::spawn_node(&self.worker_script_path)?;
            self.child = child;
            self.stdin = stdin;
            self.stdout = stdout;
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
    pub async fn run_tests(
        &mut self,
        files: Vec<TranspiledTestFile>,
    ) -> io::Result<WorkerResponse> {
        self.ensure_alive().await?;

        self.next_id += 1;
        let id = format!("t{}", self.next_id);

        let request = WorkerRequest {
            id: id.clone(),
            files,
        };

        // Send request as newline-delimited JSON
        let mut json = serde_json::to_string(&request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push('\n');

        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.flush().await?;

        // Read response line with timeout
        let mut line = String::new();
        let read_result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
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
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "test worker timed out after 120s",
            )),
        }
    }
}

impl Drop for NodeTestWorker {
    fn drop(&mut self) {
        // kill_on_drop handles child cleanup
        let _ = std::fs::remove_file(&self.worker_script_path);
    }
}
