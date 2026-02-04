//! Integration tests for file watcher cache invalidation.
//!
//! Tests that file changes trigger cache invalidation, causing subsequent
//! `fastnode run` commands to re-resolve imports instead of using cached results.

use serial_test::serial;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

/// Generate a unique endpoint for this test.
fn test_endpoint() -> String {
    let unique_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    #[cfg(unix)]
    {
        format!("/tmp/fastnode-test-{unique_id}.sock")
    }

    #[cfg(windows)]
    {
        format!("fastnode-test-{unique_id}")
    }
}

/// Clean up socket file if it exists (Unix only).
fn cleanup_endpoint(endpoint: &str) {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(endpoint);
    }

    #[cfg(windows)]
    {
        let _ = endpoint;
    }
}

/// Start the daemon as a background process.
fn start_daemon(endpoint: &str) -> Child {
    cargo_bin()
        .arg("daemon")
        .env("HOWTH_IPC_ENDPOINT", endpoint)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start daemon")
}

/// Get watcher status and return last_event_unix_ms.
fn get_watcher_status(endpoint: &str) -> Option<u64> {
    let output = cargo_bin()
        .args(["--json", "watch", "status"])
        .env("HOWTH_IPC_ENDPOINT", endpoint)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).ok()?;

    json["last_event_unix_ms"].as_u64()
}

/// Poll watcher status until last_event_unix_ms changes from baseline.
/// Returns true if an update was detected within timeout.
fn wait_for_watcher_event(endpoint: &str, baseline_ts: Option<u64>, timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        if start.elapsed() > timeout {
            return false;
        }

        let current_ts = get_watcher_status(endpoint);

        // Check if timestamp changed
        match (baseline_ts, current_ts) {
            (None, Some(_)) => return true,
            (Some(b), Some(c)) if c > b => return true,
            _ => {}
        }

        thread::sleep(Duration::from_millis(50));
    }
}

/// Run fastnode run and return the parsed JSON response.
fn run_and_get_plan(
    endpoint: &str,
    cwd: &std::path::Path,
    entry: &str,
) -> Option<serde_json::Value> {
    let output = cargo_bin()
        .args(["--json", "run", entry, "--dry-run", "--daemon", "--cwd"])
        .arg(cwd)
        .env("HOWTH_IPC_ENDPOINT", endpoint)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).ok()
}

#[test]
#[serial]
#[cfg_attr(windows, ignore = "Flaky on Windows CI due to file watcher timing")]
fn test_watcher_invalidates_cache() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // 1. Create temp project with main.js importing dep.js
    let dir = tempfile::tempdir().unwrap();
    let main_path = dir.path().join("main.js");
    let dep_path = dir.path().join("dep.js");

    std::fs::write(&main_path, r#"import "./dep.js";"#).unwrap();
    std::fs::write(&dep_path, "export const x = 1;").unwrap();

    // 2. Start daemon
    let mut daemon = start_daemon(&endpoint);
    thread::sleep(Duration::from_millis(800));

    // 3a. First run - should NOT be from cache
    let mut plan1 = None;
    for i in 0..20 {
        if let Some(p) = run_and_get_plan(&endpoint, dir.path(), "main.js") {
            plan1 = Some(p);
            break;
        }
        // Longer timeouts for CI environments
        thread::sleep(Duration::from_millis(200 + i * 100));
    }

    let plan1 = plan1.expect("First run should succeed");
    let resolved1 = plan1["resolved_imports"]
        .as_array()
        .expect("resolved_imports should be array");

    assert!(!resolved1.is_empty(), "should have resolved imports");
    assert_eq!(
        resolved1[0]["from_cache"].as_bool(),
        Some(false),
        "First run should NOT be from cache"
    );
    assert_eq!(
        resolved1[0]["status"].as_str(),
        Some("resolved"),
        "Import should be resolved"
    );

    // 3b. Second run - should be from cache
    let plan2 =
        run_and_get_plan(&endpoint, dir.path(), "main.js").expect("Second run should succeed");
    let resolved2 = plan2["resolved_imports"]
        .as_array()
        .expect("resolved_imports should be array");

    assert!(!resolved2.is_empty(), "should have resolved imports");
    assert_eq!(
        resolved2[0]["from_cache"].as_bool(),
        Some(true),
        "Second run should be from cache"
    );

    // 4. Start watcher on the project directory
    let start_output = cargo_bin()
        .args(["--json", "watch", "start"])
        .arg(dir.path())
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to start watcher");

    assert!(
        start_output.status.success(),
        "Watch start should succeed. stderr: {}",
        String::from_utf8_lossy(&start_output.stderr)
    );

    // Get baseline timestamp before modification
    let baseline_ts = get_watcher_status(&endpoint);

    // 5. Modify dep.js
    thread::sleep(Duration::from_millis(100)); // Ensure file timestamp changes
    std::fs::write(&dep_path, "export const x = 2; // modified").unwrap();

    // 6. Wait for watcher to process the event (deterministic polling)
    // Use longer timeout for Windows CI where file watchers can be slower
    let event_detected = wait_for_watcher_event(&endpoint, baseline_ts, 15000);
    assert!(
        event_detected,
        "Watcher should detect file modification within timeout"
    );

    // Small delay to ensure cache invalidation is applied
    thread::sleep(Duration::from_millis(100));

    // 7. Third run - should NOT be from cache (invalidated)
    let plan3 =
        run_and_get_plan(&endpoint, dir.path(), "main.js").expect("Third run should succeed");
    let resolved3 = plan3["resolved_imports"]
        .as_array()
        .expect("resolved_imports should be array");

    assert!(!resolved3.is_empty(), "should have resolved imports");
    assert_eq!(
        resolved3[0]["from_cache"].as_bool(),
        Some(false),
        "Third run should NOT be from cache after invalidation"
    );

    // Clean up
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);
}

#[test]
#[serial]
#[cfg_attr(windows, ignore = "Flaky on Windows CI due to daemon IPC timing")]
fn test_watcher_status_reports_running() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let dir = tempfile::tempdir().unwrap();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    thread::sleep(Duration::from_millis(800));

    // Check status before starting - should not be running
    let mut status_ok = false;
    for i in 0..10 {
        let output = cargo_bin()
            .args(["--json", "watch", "status"])
            .env("HOWTH_IPC_ENDPOINT", &endpoint)
            .output()
            .expect("Failed to get status");

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
            assert_eq!(
                json["running"].as_bool(),
                Some(false),
                "Watcher should not be running initially"
            );
            status_ok = true;
            break;
        }
        // Longer timeouts for CI environments
        thread::sleep(Duration::from_millis(200 + i * 100));
    }
    assert!(status_ok, "Should get watcher status");

    // Start watcher
    let output = cargo_bin()
        .args(["--json", "watch", "start"])
        .arg(dir.path())
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to start watcher");

    assert!(output.status.success(), "Watch start should succeed");

    // Check status after starting - should be running
    let output = cargo_bin()
        .args(["--json", "watch", "status"])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to get status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["running"].as_bool(),
        Some(true),
        "Watcher should be running after start"
    );

    // Stop watcher
    let output = cargo_bin()
        .args(["--json", "watch", "stop"])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to stop watcher");

    assert!(output.status.success(), "Watch stop should succeed");

    // Check status after stopping - should not be running
    let output = cargo_bin()
        .args(["--json", "watch", "status"])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to get status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["running"].as_bool(),
        Some(false),
        "Watcher should not be running after stop"
    );

    // Clean up
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);
}
