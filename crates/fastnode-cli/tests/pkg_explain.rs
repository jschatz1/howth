//! Integration tests for `fastnode pkg explain`.
//!
//! These tests create node_modules structures and verify the explain output.

use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

/// Generate a unique IPC endpoint for this test.
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

/// Clean up socket file if it exists.
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

/// Wait for daemon to be ready with retries.
fn wait_for_daemon(endpoint: &str) -> bool {
    // Use longer timeouts for CI environments which can be slower
    for i in 0..30 {
        let result = cargo_bin()
            .arg("ping")
            .env("HOWTH_IPC_ENDPOINT", endpoint)
            .output();

        if let Ok(output) = result {
            if output.status.success() {
                // Extra stabilization time for Windows named pipes
                #[cfg(windows)]
                thread::sleep(Duration::from_millis(500));
                return true;
            }
        }
        thread::sleep(Duration::from_millis(200 + i * 100));
    }
    false
}

/// Create a test project with node_modules and a package with exports.
fn create_project_with_exports_package() -> TempDir {
    let dir = tempfile::tempdir().unwrap();

    // Create root package.json
    let package_json = serde_json::json!({
        "name": "test-project",
        "version": "1.0.0"
    });

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();

    // Create node_modules directory
    let node_modules = dir.path().join("node_modules");
    std::fs::create_dir_all(&node_modules).unwrap();

    // Create test-pkg with exports
    let pkg_dir = node_modules.join("test-pkg");
    std::fs::create_dir_all(&pkg_dir).unwrap();

    let pkg_json = serde_json::json!({
        "name": "test-pkg",
        "version": "1.0.0",
        "exports": {
            ".": {
                "import": "./esm/index.js",
                "require": "./cjs/index.js",
                "default": "./index.js"
            },
            "./feature": "./feature.js"
        }
    });

    std::fs::write(
        pkg_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_json).unwrap(),
    )
    .unwrap();

    // Create the actual files
    std::fs::create_dir_all(pkg_dir.join("esm")).unwrap();
    std::fs::create_dir_all(pkg_dir.join("cjs")).unwrap();
    std::fs::write(pkg_dir.join("index.js"), "// default").unwrap();
    std::fs::write(pkg_dir.join("esm/index.js"), "// esm").unwrap();
    std::fs::write(pkg_dir.join("cjs/index.js"), "// cjs").unwrap();
    std::fs::write(pkg_dir.join("feature.js"), "// feature").unwrap();

    dir
}

#[test]
fn test_explain_bare_specifier_json() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_exports_package();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain with --kind import
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "test-pkg",
            "--kind",
            "import",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Check results
    assert!(
        output.status.success(),
        "Should succeed: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(true));

    let result = &json["result"];
    assert_eq!(result["schema_version"].as_u64(), Some(1));
    assert_eq!(result["specifier"].as_str(), Some("test-pkg"));
    assert_eq!(result["status"].as_str(), Some("resolved"));
    assert_eq!(result["kind"].as_str(), Some("import"));

    // Should resolve to esm/index.js for import kind
    let resolved = result["resolved"]
        .as_str()
        .expect("Should have resolved path");
    assert!(
        resolved.ends_with("esm/index.js"),
        "Should resolve to esm/index.js, got: {}",
        resolved
    );

    // Should have trace steps
    let trace = result["trace"].as_array().expect("Should have trace array");
    assert!(!trace.is_empty(), "Trace should not be empty");
}

#[test]
fn test_explain_require_kind() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_exports_package();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain with --kind require
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "test-pkg",
            "--kind",
            "require",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    assert!(output.status.success(), "Should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let result = &json["result"];
    assert_eq!(result["kind"].as_str(), Some("require"));

    // Should resolve to cjs/index.js for require kind
    let resolved = result["resolved"]
        .as_str()
        .expect("Should have resolved path");
    assert!(
        resolved.ends_with("cjs/index.js"),
        "Should resolve to cjs/index.js, got: {}",
        resolved
    );
}

#[test]
fn test_explain_unresolved_package() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_exports_package();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain for non-existent package
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "nonexistent-package",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should fail with exit code 2
    assert!(!output.status.success(), "Should fail for unresolved");
    assert_eq!(output.status.code(), Some(2), "Should exit with code 2");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(false));

    let result = &json["result"];
    assert_eq!(result["status"].as_str(), Some("unresolved"));
    assert!(
        result["resolved"].is_null(),
        "Should not have resolved path"
    );
    assert!(
        result["error_code"].as_str().is_some(),
        "Should have error code"
    );

    // Should still have trace steps
    let trace = result["trace"].as_array().expect("Should have trace array");
    assert!(!trace.is_empty(), "Trace should not be empty");
}

#[test]
fn test_explain_subpath() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_exports_package();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain for subpath
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "test-pkg/feature",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    assert!(output.status.success(), "Should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let result = &json["result"];
    assert_eq!(result["status"].as_str(), Some("resolved"));

    // Should resolve to feature.js
    let resolved = result["resolved"]
        .as_str()
        .expect("Should have resolved path");
    assert!(
        resolved.ends_with("feature.js"),
        "Should resolve to feature.js, got: {}",
        resolved
    );
}
