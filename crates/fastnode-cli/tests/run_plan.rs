//! Integration tests for `fastnode run` execution plan.

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
fn test_run_local_json_success() {
    let dir = tempfile::tempdir().unwrap();
    let entry_path = dir.path().join("main.js");
    std::fs::write(&entry_path, "// test file").unwrap();

    let output = cargo_bin()
        .args(["--json", "run", "main.js", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(
        output.status.success(),
        "Command should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Check schema version
    assert_eq!(
        json["schema_version"].as_u64(),
        Some(2),
        "schema_version should be 2"
    );

    // Check resolved_entry ends with main.js
    let resolved = json["resolved_entry"]
        .as_str()
        .expect("resolved_entry should be present");
    assert!(
        resolved.ends_with("main.js"),
        "resolved_entry should end with main.js, got: {resolved}"
    );

    // Check entry_kind
    assert_eq!(
        json["entry_kind"].as_str(),
        Some("file"),
        "entry_kind should be 'file'"
    );

    // Check requested_entry
    assert_eq!(
        json["requested_entry"].as_str(),
        Some("main.js"),
        "requested_entry should be 'main.js'"
    );

    // Check channel
    assert_eq!(
        json["channel"].as_str(),
        Some("stable"),
        "channel should be 'stable'"
    );
}

#[test]
#[serial]
fn test_run_local_typescript_entry() {
    let dir = tempfile::tempdir().unwrap();
    let entry_path = dir.path().join("app.ts");
    std::fs::write(&entry_path, "// typescript test").unwrap();

    let output = cargo_bin()
        .args(["--json", "run", "app.ts", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Check notes contain TypeScript
    let notes = json["notes"].as_array().expect("notes should be array");
    assert!(!notes.is_empty(), "notes should not be empty");
    let note = notes[0].as_str().expect("note should be string");
    assert!(
        note.contains("TypeScript"),
        "note should mention TypeScript: {note}"
    );
}

#[test]
#[serial]
fn test_run_local_with_args() {
    let dir = tempfile::tempdir().unwrap();
    let entry_path = dir.path().join("main.js");
    std::fs::write(&entry_path, "// test").unwrap();

    let output = cargo_bin()
        .args([
            "--json",
            "run",
            "main.js",
            "--cwd",
            &dir.path().to_string_lossy(),
            "--",
            "--port",
            "3000",
            "--debug",
        ])
        .output()
        .expect("Failed to run command");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let args = json["args"].as_array().expect("args should be array");
    assert_eq!(args.len(), 3, "should have 3 args");
    assert_eq!(args[0].as_str(), Some("--port"));
    assert_eq!(args[1].as_str(), Some("3000"));
    assert_eq!(args[2].as_str(), Some("--debug"));
}

#[test]
#[serial]
fn test_run_local_missing_entry_exit_2() {
    let dir = tempfile::tempdir().unwrap();

    let output = cargo_bin()
        .args(["run", "nonexistent.js", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(2),
        "exit code should be 2 for validation error"
    );
}

#[test]
#[serial]
fn test_run_local_missing_entry_json() {
    let dir = tempfile::tempdir().unwrap();

    let output = cargo_bin()
        .args(["--json", "run", "nonexistent.js", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(json["ok"].as_bool(), Some(false));
    assert_eq!(
        json["error"]["code"].as_str(),
        Some("ENTRY_NOT_FOUND"),
        "error code should be ENTRY_NOT_FOUND"
    );
}

#[test]
#[serial]
fn test_run_local_directory_entry_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    let subdir = dir.path().join("subdir");
    std::fs::create_dir(&subdir).unwrap();

    let output = cargo_bin()
        .args(["run", "subdir", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(2),
        "exit code should be 2 for directory entry"
    );
}

#[test]
#[serial]
fn test_run_local_human_output() {
    let dir = tempfile::tempdir().unwrap();
    let entry_path = dir.path().join("index.js");
    std::fs::write(&entry_path, "// test").unwrap();

    let output = cargo_bin()
        .args(["run", "index.js", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run command");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human output should contain these labels
    assert!(stdout.contains("CWD:"), "should contain CWD label");
    assert!(stdout.contains("Entry:"), "should contain Entry label");
    assert!(stdout.contains("Kind:"), "should contain Kind label");
    assert!(stdout.contains("Channel:"), "should contain Channel label");
}

// Daemon tests - cross-platform
mod daemon_tests {
    use super::*;

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

    #[test]
    #[serial]
    fn test_run_daemon_json_success() {
        let endpoint = test_endpoint();
        cleanup_endpoint(&endpoint);

        // Create test file
        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        std::fs::write(&entry_path, "// test").unwrap();

        // Start daemon
        let mut daemon = start_daemon(&endpoint);

        // Give daemon time to start (longer wait for CI environments)
        thread::sleep(Duration::from_millis(800));

        // Run command via daemon with retries for startup race
        let mut output = None;
        for i in 0..10 {
            let result = cargo_bin()
                .args(["--json", "run", "main.js", "--daemon", "--cwd"])
                .arg(dir.path())
                .env("HOWTH_IPC_ENDPOINT", &endpoint)
                .output()
                .expect("Failed to run command");

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

        let output = output.expect("daemon run should succeed after retries");
        assert!(
            output.status.success(),
            "Command should succeed. stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value =
            serde_json::from_str(&stdout).expect("stdout should be valid JSON");

        // Check schema version
        assert_eq!(json["schema_version"].as_u64(), Some(2));

        // Check resolved_entry
        let resolved = json["resolved_entry"]
            .as_str()
            .expect("resolved_entry should be present");
        assert!(resolved.ends_with("main.js"));
    }

    #[test]
    #[serial]
    fn test_run_daemon_no_daemon_exit_1() {
        let endpoint = test_endpoint();
        cleanup_endpoint(&endpoint);

        let dir = tempfile::tempdir().unwrap();
        let entry_path = dir.path().join("main.js");
        std::fs::write(&entry_path, "// test").unwrap();

        let output = cargo_bin()
            .args(["run", "main.js", "--daemon", "--cwd"])
            .arg(dir.path())
            .env("HOWTH_IPC_ENDPOINT", &endpoint)
            .output()
            .expect("Failed to run command");

        assert!(!output.status.success());
        assert_eq!(
            output.status.code(),
            Some(1),
            "exit code should be 1 when daemon not running"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        // Filter cargo build output
        let stderr_lines: Vec<&str> = stderr
            .lines()
            .filter(|l| {
                !l.contains("Compiling") && !l.contains("Finished") && !l.contains("Running")
            })
            .collect();
        let stderr_content = stderr_lines.join("\n");

        assert!(
            stderr_content.contains("daemon not running")
                || stderr_content.contains("No such file")
                || stderr_content.contains("system cannot find the file") // Windows
                || stderr_content.contains("timed out"), // Windows timeout
            "should report daemon not running: {stderr_content}"
        );
    }

    #[test]
    #[serial]
    fn test_run_daemon_missing_entry() {
        let endpoint = test_endpoint();
        cleanup_endpoint(&endpoint);

        let dir = tempfile::tempdir().unwrap();

        // Start daemon
        let mut daemon = start_daemon(&endpoint);
        thread::sleep(Duration::from_millis(800));

        // Run command via daemon with missing file - with retries
        let mut output = None;
        for i in 0..10 {
            let result = cargo_bin()
                .args(["--json", "run", "nonexistent.js", "--daemon", "--cwd"])
                .arg(dir.path())
                .env("HOWTH_IPC_ENDPOINT", &endpoint)
                .output()
                .expect("Failed to run command");

            // For missing entry, we expect the command to fail with a specific error
            // Check if we got a valid JSON response (means daemon is responding)
            let stdout = String::from_utf8_lossy(&result.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if json["error"]["code"].as_str().is_some() {
                    output = Some(result);
                    break;
                }
            }
            // Exponential backoff with longer timeouts for CI
            thread::sleep(Duration::from_millis(200 + i * 100));
        }

        // Clean up daemon
        let _ = daemon.kill();
        let _ = daemon.wait();
        cleanup_endpoint(&endpoint);

        let output = output.expect("daemon should respond after retries");
        assert!(!output.status.success());
        assert_eq!(
            output.status.code(),
            Some(2),
            "exit code should be 2 for validation error"
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

        assert_eq!(json["ok"].as_bool(), Some(false));
        assert_eq!(json["error"]["code"].as_str(), Some("ENTRY_NOT_FOUND"));
    }
}
