//! Integration tests for `fastnode daemon` and `fastnode ping`.

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
///
/// On Unix, returns a socket path.
/// On Windows, returns a named pipe name (short form, normalized by the daemon).
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
        // Short name - will be normalized to \\.\pipe\fastnode-test-xxx by the daemon
        format!("fastnode-test-{unique_id}")
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

/// Clean up socket file if it exists (Unix only).
fn cleanup_endpoint(endpoint: &str) {
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(endpoint);
    }

    #[cfg(windows)]
    {
        // Named pipes are cleaned up automatically by the OS
        let _ = endpoint;
    }
}

#[test]
#[serial]
fn test_ping_without_daemon_fails() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let output = cargo_bin()
        .arg("ping")
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run ping command");

    // Should exit with error
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Filter out cargo build output
    let stderr_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.contains("Compiling") && !l.contains("Finished") && !l.contains("Running"))
        .collect();

    // Check for helpful error message
    let stderr_content = stderr_lines.join("\n");
    assert!(
        stderr_content.contains("daemon not running")
            || stderr_content.contains("No such file or directory")
            || stderr_content.contains("system cannot find the file") // Windows
            || stderr_content.contains("timed out"), // Windows timeout
        "Should report daemon not running: {stderr_content}"
    );
}

#[test]
#[serial]
fn test_ping_without_daemon_json_output() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let output = cargo_bin()
        .args(["--json", "ping"])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run ping command");

    // Should exit with error
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(false));
    assert!(json["error"].as_str().is_some());
}

#[test]
#[serial]
fn test_daemon_and_ping() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Start daemon
    let mut daemon = start_daemon(&endpoint);

    // Give daemon time to start (longer wait for CI environments)
    thread::sleep(Duration::from_millis(800));

    // Try ping (human output) with retries for startup race
    let mut output = None;
    for i in 0..10 {
        let result = cargo_bin()
            .arg("ping")
            .env("HOWTH_IPC_ENDPOINT", &endpoint)
            .output()
            .expect("Failed to run ping command");

        if result.status.success() {
            output = Some(result);
            break;
        }
        // Exponential backoff with longer timeouts for CI
        thread::sleep(Duration::from_millis(200 + i * 100));
    }

    // Clean up daemon
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Check results
    let output = output.expect("ping should succeed after retries");
    assert!(
        output.status.success(),
        "ping should succeed when daemon running"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pong"), "Should print pong");
}

#[test]
#[serial]
fn test_daemon_and_ping_json() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Start daemon
    let mut daemon = start_daemon(&endpoint);

    // Give daemon time to start (longer wait for CI environments)
    thread::sleep(Duration::from_millis(800));

    // Try JSON ping with retries for startup race
    let mut output = None;
    for i in 0..20 {
        let result = cargo_bin()
            .args(["--json", "ping"])
            .env("HOWTH_IPC_ENDPOINT", &endpoint)
            .output()
            .expect("Failed to run ping command");

        if result.status.success() {
            output = Some(result);
            break;
        }
        // Exponential backoff with longer timeouts for CI
        thread::sleep(Duration::from_millis(200 + i * 100));
    }

    // Clean up daemon
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Check results
    let output = output.expect("ping should succeed after retries");
    assert!(
        output.status.success(),
        "ping should succeed when daemon running"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(true), "ok should be true");
    assert!(json["nonce"].as_u64().is_some(), "nonce should be present");
    assert!(
        json["server_version"].as_str().is_some(),
        "server_version should be present"
    );
    assert_eq!(
        json["server_version"].as_str(),
        Some("0.1.0"),
        "server_version should match"
    );
}
