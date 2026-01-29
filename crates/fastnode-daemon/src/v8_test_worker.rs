//! Native V8 test worker for `howth test`.
//!
//! Runs tests directly in the howth V8 runtime (deno_core) instead of a
//! Node.js subprocess. This eliminates ~15ms of `node:test run()` overhead
//! and subprocess pipe latency.
//!
//! The V8 runtime is `!Send` (uses `Rc<RefCell>`), so it lives on a dedicated
//! OS thread. The runtime is created once and reused across requests — the
//! 6.5ms bootstrap cost is paid only on the first test run.

use crate::test_worker::{TranspiledTestFile, WorkerResponse, WorkerTestCase};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use tracing::{debug, warn};

/// Request sent to the V8 worker thread.
struct V8Request {
    id: String,
    files: Vec<TranspiledTestFile>,
    reply: mpsc::Sender<io::Result<WorkerResponse>>,
}

/// Manages a dedicated V8 runtime thread for running tests.
pub struct V8TestWorker {
    sender: mpsc::Sender<V8Request>,
    _thread: thread::JoinHandle<()>,
    temp_dir: PathBuf,
}

impl V8TestWorker {
    /// Create a new V8 test worker with a warm runtime on a dedicated thread.
    pub fn spawn() -> io::Result<Self> {
        let (tx, rx) = mpsc::channel::<V8Request>();

        let temp_dir = std::env::temp_dir().join("howth-v8-test-worker");
        std::fs::create_dir_all(&temp_dir)?;
        let temp_dir_clone = temp_dir.clone();

        let handle = thread::Builder::new()
            .name("howth-v8-test-worker".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to create tokio runtime for V8 worker");

                rt.block_on(async {
                    v8_worker_loop(rx, &temp_dir_clone).await;
                });
            })
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("failed to spawn V8 worker thread: {e}"),
                )
            })?;

        debug!("spawned V8 test worker thread");

        Ok(Self {
            sender: tx,
            _thread: handle,
            temp_dir,
        })
    }

    /// Run tests in the V8 runtime.
    pub fn run_tests(
        &self,
        id: String,
        files: Vec<TranspiledTestFile>,
    ) -> io::Result<WorkerResponse> {
        let (reply_tx, reply_rx) = mpsc::channel();

        self.sender
            .send(V8Request {
                id,
                files,
                reply: reply_tx,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "V8 worker thread died"))?;

        reply_rx
            .recv_timeout(std::time::Duration::from_secs(120))
            .map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => io::Error::new(
                    io::ErrorKind::TimedOut,
                    "V8 test worker timed out after 120s",
                ),
                mpsc::RecvTimeoutError::Disconnected => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "V8 worker thread died")
                }
            })?
    }
}

impl Drop for V8TestWorker {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

/// The worker loop running on the dedicated V8 thread.
/// Creates the runtime once and reuses it across requests.
async fn v8_worker_loop(rx: mpsc::Receiver<V8Request>, temp_dir: &std::path::Path) {
    use fastnode_runtime::{Runtime, RuntimeOptions};

    debug!("V8 test worker loop started, initializing runtime...");

    // Create the runtime once — pays the 6.5ms bootstrap cost here.
    let mut runtime = match Runtime::new(RuntimeOptions {
        cwd: Some(temp_dir.to_path_buf()),
        ..Default::default()
    }) {
        Ok(r) => r,
        Err(e) => {
            warn!("failed to create V8 runtime: {e}");
            // Drain and error all requests
            while let Ok(req) = rx.recv() {
                let _ = req.reply.send(Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("V8 runtime init failed: {e}"),
                )));
            }
            return;
        }
    };

    debug!("V8 runtime ready, waiting for test requests");

    while let Ok(req) = rx.recv() {
        let result = run_tests_in_v8(&mut runtime, &req.id, &req.files, temp_dir).await;
        let _ = req.reply.send(result);
    }

    debug!("V8 test worker loop ended");
}

/// Execute test files in the warm V8 runtime and collect results.
async fn run_tests_in_v8(
    runtime: &mut fastnode_runtime::Runtime,
    id: &str,
    files: &[TranspiledTestFile],
    temp_dir: &std::path::Path,
) -> io::Result<WorkerResponse> {
    let start = std::time::Instant::now();

    // Write transpiled files to temp .mjs files
    let mut temp_paths = Vec::with_capacity(files.len());
    for (i, file) in files.iter().enumerate() {
        let temp_path = temp_dir.join(format!("{id}-{i}.mjs"));
        std::fs::write(&temp_path, &file.code)?;
        temp_paths.push(temp_path);
    }

    // Build a runner module that imports all test files then runs the harness.
    // Each run uses unique filenames so the module cache doesn't conflict.
    let results_path = temp_dir.join(format!("{id}-results.json"));
    let results_path_str = results_path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\'', "\\'");

    let mut runner_code = String::new();
    for temp_path in &temp_paths {
        let filename = temp_path.file_name().unwrap().to_string_lossy();
        runner_code.push_str(&format!("import './{filename}';\n"));
    }
    runner_code.push_str(&format!(
        r#"
const report = await globalThis.__howth_run_tests();
const json = JSON.stringify(report);
Deno.core.ops.op_howth_write_file('{results_path_str}', json);
"#
    ));

    let runner_path = temp_dir.join(format!("{id}-runner.mjs"));
    std::fs::write(&runner_path, &runner_code)?;
    temp_paths.push(runner_path.clone());

    // Execute as a side module (reusable runtime — no "main module" restriction)
    if let Err(e) = runtime.execute_side_module(&runner_path).await {
        cleanup_temp_files(&temp_paths);
        return Ok(WorkerResponse {
            id: id.to_string(),
            ok: false,
            total: 0,
            passed: 0,
            failed: 1,
            skipped: 0,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            tests: vec![WorkerTestCase {
                name: "test-runner".to_string(),
                file: String::new(),
                status: "fail".to_string(),
                duration_ms: 0.0,
                error: Some(format!("Failed to execute: {e}")),
            }],
            diagnostics: String::new(),
        });
    }

    // Clean up temp files
    cleanup_temp_files(&temp_paths);

    // Read results
    let json_str = match std::fs::read_to_string(&results_path) {
        Ok(s) => {
            let _ = std::fs::remove_file(&results_path);
            s
        }
        Err(e) => {
            return Ok(WorkerResponse {
                id: id.to_string(),
                ok: false,
                total: 0,
                passed: 0,
                failed: 1,
                skipped: 0,
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                tests: vec![],
                diagnostics: format!("Failed to read results file: {e}"),
            });
        }
    };

    // Parse results
    let report: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid results JSON: {e}"),
        )
    })?;

    let tests: Vec<WorkerTestCase> = report
        .get("tests")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .map(|t| WorkerTestCase {
                    name: t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                    file: String::new(),
                    status: t.get("status").and_then(|s| s.as_str()).unwrap_or("fail").to_string(),
                    duration_ms: t.get("duration_ms").and_then(|d| d.as_f64()).unwrap_or(0.0),
                    error: t.get("error").and_then(|e| {
                        if e.is_null() { None } else { e.as_str().map(String::from) }
                    }),
                })
                .collect()
        })
        .unwrap_or_default();

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

    Ok(WorkerResponse {
        id: id.to_string(),
        ok: report.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        total: report.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        passed: report.get("passed").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        failed: report.get("failed").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        skipped: report.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        duration_ms,
        tests,
        diagnostics: String::new(),
    })
}

fn cleanup_temp_files(paths: &[PathBuf]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}
