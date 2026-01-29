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
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
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

    // Create a shared virtual module map for in-memory module loading
    let virtual_modules: Rc<RefCell<HashMap<String, String>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // Create the runtime once — pays the 6.5ms bootstrap cost here.
    let mut runtime = match Runtime::new(RuntimeOptions {
        cwd: Some(temp_dir.to_path_buf()),
        virtual_modules: Some(virtual_modules.clone()),
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
        let result =
            run_tests_in_v8(&mut runtime, &req.id, &req.files, temp_dir, &virtual_modules).await;
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
    virtual_modules: &Rc<RefCell<HashMap<String, String>>>,
) -> io::Result<WorkerResponse> {
    let start = std::time::Instant::now();

    // Populate virtual module map with transpiled code (no disk I/O)
    let mut virtual_paths = Vec::with_capacity(files.len());
    {
        let mut vm = virtual_modules.borrow_mut();
        for (i, file) in files.iter().enumerate() {
            let temp_path = temp_dir.join(format!("{id}-{i}.mjs"));
            let path_str = temp_path.to_string_lossy().to_string();
            vm.insert(path_str, file.code.clone());
            virtual_paths.push(temp_path);
        }
    }

    // Build a runner module that imports all test files then runs the harness.
    let mut runner_code = String::new();
    for vpath in &virtual_paths {
        let filename = vpath.file_name().unwrap().to_string_lossy();
        runner_code.push_str(&format!("import './{filename}';\n"));
    }
    runner_code.push_str(
        r#"
const report = await globalThis.__howth_run_tests();
globalThis.__howth_test_result_json = JSON.stringify(report);
"#,
    );

    let runner_path = temp_dir.join(format!("{id}-runner.mjs"));
    {
        let mut vm = virtual_modules.borrow_mut();
        vm.insert(
            runner_path.to_string_lossy().to_string(),
            runner_code,
        );
    }

    // Execute as a side module (reusable runtime — no "main module" restriction)
    if let Err(e) = runtime.execute_side_module(&runner_path).await {
        cleanup_virtual_modules(virtual_modules, &virtual_paths, &runner_path);
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

    // Clean up virtual modules
    cleanup_virtual_modules(virtual_modules, &virtual_paths, &runner_path);

    // Extract results from globalThis (stays in V8 memory, no disk I/O)
    let json_str = match runtime.eval_to_string("globalThis.__howth_test_result_json") {
        Ok(s) => s,
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
                diagnostics: format!("Failed to read test results from V8: {e}"),
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

fn cleanup_virtual_modules(
    virtual_modules: &Rc<RefCell<HashMap<String, String>>>,
    file_paths: &[PathBuf],
    runner_path: &PathBuf,
) {
    let mut vm = virtual_modules.borrow_mut();
    for p in file_paths {
        vm.remove(&p.to_string_lossy().to_string());
    }
    vm.remove(&runner_path.to_string_lossy().to_string());
}
