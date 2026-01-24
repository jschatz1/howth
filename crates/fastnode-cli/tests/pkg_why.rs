//! Integration tests for `fastnode pkg explain --why`.
//!
//! These tests create node_modules structures and verify the why output.

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
    for i in 0..15 {
        let result = cargo_bin()
            .arg("ping")
            .env("HOWTH_IPC_ENDPOINT", endpoint)
            .output();

        if let Ok(output) = result {
            if output.status.success() {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(100 + i * 50));
    }
    false
}

/// Create a test project with a dependency chain.
fn create_project_with_dep_chain() -> TempDir {
    let dir = tempfile::tempdir().unwrap();

    // Create root package.json
    let package_json = serde_json::json!({
        "name": "test-project",
        "version": "1.0.0",
        "dependencies": {
            "pkg-a": "^1.0.0"
        }
    });

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();

    // Create node_modules directory
    let node_modules = dir.path().join("node_modules");
    std::fs::create_dir_all(&node_modules).unwrap();

    // Create pkg-a which depends on pkg-b
    let pkg_a_dir = node_modules.join("pkg-a");
    std::fs::create_dir_all(&pkg_a_dir).unwrap();

    let pkg_a_json = serde_json::json!({
        "name": "pkg-a",
        "version": "1.0.0",
        "dependencies": {
            "pkg-b": "^2.0.0"
        }
    });

    std::fs::write(
        pkg_a_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_a_json).unwrap(),
    )
    .unwrap();
    std::fs::write(pkg_a_dir.join("index.js"), "// pkg-a").unwrap();

    // Create pkg-b which depends on pkg-c
    let pkg_b_dir = node_modules.join("pkg-b");
    std::fs::create_dir_all(&pkg_b_dir).unwrap();

    let pkg_b_json = serde_json::json!({
        "name": "pkg-b",
        "version": "2.0.0",
        "dependencies": {
            "pkg-c": "^3.0.0"
        }
    });

    std::fs::write(
        pkg_b_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_b_json).unwrap(),
    )
    .unwrap();
    std::fs::write(pkg_b_dir.join("index.js"), "// pkg-b").unwrap();

    // Create pkg-c (leaf)
    let pkg_c_dir = node_modules.join("pkg-c");
    std::fs::create_dir_all(&pkg_c_dir).unwrap();

    let pkg_c_json = serde_json::json!({
        "name": "pkg-c",
        "version": "3.0.0"
    });

    std::fs::write(
        pkg_c_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_c_json).unwrap(),
    )
    .unwrap();
    std::fs::write(pkg_c_dir.join("index.js"), "// pkg-c").unwrap();

    dir
}

#[test]
fn test_why_direct_dependency() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain --why for direct dependency
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "pkg-a",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

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

    let why = &json["why"];
    assert_eq!(why["schema_version"].as_u64(), Some(1));
    assert_eq!(why["found_in_node_modules"].as_bool(), Some(true));
    assert_eq!(why["is_orphan"].as_bool(), Some(false));

    // Target should be pkg-a
    assert_eq!(why["target"]["name"].as_str(), Some("pkg-a"));
    assert_eq!(why["target"]["version"].as_str(), Some("1.0.0"));

    // Chains should be empty (root-level dep) or have one chain from root
    // Since it's a direct dependency, chains might show one link from <root> -> pkg-a
}

#[test]
fn test_why_transitive_dependency() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain --why for transitive dependency
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "pkg-c",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

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

    let why = &json["why"];
    assert_eq!(why["found_in_node_modules"].as_bool(), Some(true));
    assert_eq!(why["is_orphan"].as_bool(), Some(false));

    // Target should be pkg-c
    assert_eq!(why["target"]["name"].as_str(), Some("pkg-c"));

    // Should have chains showing the path
    let chains = why["chains"].as_array().expect("Should have chains array");
    assert!(!chains.is_empty(), "Should have at least one chain");
}

#[test]
fn test_why_not_found() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg explain --why for non-existent package
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "nonexistent-package",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Per spec: exit 0 on successful daemon request (even with errors)
    assert!(
        output.status.success(),
        "Should succeed with daemon request: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(false));

    let why = &json["why"];
    assert_eq!(why["found_in_node_modules"].as_bool(), Some(false));
}

#[test]
fn test_why_human_tree_is_deterministic() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run twice
    let output1 = cargo_bin()
        .args([
            "pkg",
            "explain",
            "pkg-c",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

    let output2 = cargo_bin()
        .args([
            "pkg",
            "explain",
            "pkg-c",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Outputs should be identical
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    assert_eq!(stdout1, stdout2, "Human output should be deterministic");
}

#[test]
fn test_why_json_shape_is_locked() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "explain",
            "pkg-a",
            "--why",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    // Assert exact keys per spec: { ok, why, error? }
    assert!(json.get("ok").is_some(), "Must have 'ok' field");
    assert!(json.get("why").is_some(), "Must have 'why' field");

    // why must have specific structure
    let why = &json["why"];
    assert!(
        why.get("schema_version").is_some(),
        "why.schema_version required"
    );
    assert!(why.get("cwd").is_some(), "why.cwd required");
    assert!(why.get("target").is_some(), "why.target required");
    assert!(
        why.get("found_in_node_modules").is_some(),
        "why.found_in_node_modules required"
    );
    assert!(why.get("is_orphan").is_some(), "why.is_orphan required");

    // target must have specific structure
    let target = &why["target"];
    assert!(target.get("name").is_some(), "target.name required");
    assert!(target.get("input").is_some(), "target.input required");
}

#[test]
fn test_why_list_format() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_dep_chain();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    let output = cargo_bin()
        .args([
            "pkg",
            "explain",
            "pkg-c",
            "--why",
            "--format",
            "list",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg explain --why --format list");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    assert!(
        output.status.success(),
        "Should succeed: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // List format should have " -> " arrows showing chain
    assert!(
        stdout.contains("->"),
        "List format should contain arrows: {}",
        stdout
    );
}
