//! Integration tests for `fastnode pkg graph`.
//!
//! These tests create node_modules structures and verify the graph output.

#![allow(clippy::type_complexity)]

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
                return true;
            }
        }
        thread::sleep(Duration::from_millis(200 + i * 100));
    }
    false
}

/// Create a test project with node_modules already populated.
fn create_project_with_node_modules(
    deps: &[(&str, &str)],
    dev_deps: &[(&str, &str)],
    packages: &[(&str, &str, &[(&str, &str)])], // (name, version, deps)
) -> TempDir {
    let dir = tempfile::tempdir().unwrap();

    // Create root package.json
    let mut package_json = serde_json::json!({
        "name": "test-project",
        "version": "1.0.0"
    });

    if !deps.is_empty() {
        let deps_obj: serde_json::Map<String, serde_json::Value> = deps
            .iter()
            .map(|(name, range)| (name.to_string(), serde_json::json!(range)))
            .collect();
        package_json["dependencies"] = serde_json::Value::Object(deps_obj);
    }

    if !dev_deps.is_empty() {
        let dev_deps_obj: serde_json::Map<String, serde_json::Value> = dev_deps
            .iter()
            .map(|(name, range)| (name.to_string(), serde_json::json!(range)))
            .collect();
        package_json["devDependencies"] = serde_json::Value::Object(dev_deps_obj);
    }

    std::fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();

    // Create node_modules directory
    let node_modules = dir.path().join("node_modules");
    std::fs::create_dir_all(&node_modules).unwrap();

    // Create package directories
    for (name, version, pkg_deps) in packages {
        let pkg_dir = node_modules.join(name);
        std::fs::create_dir_all(&pkg_dir).unwrap();

        let mut pkg_json = serde_json::json!({
            "name": name,
            "version": version
        });

        if !pkg_deps.is_empty() {
            let deps_obj: serde_json::Map<String, serde_json::Value> = pkg_deps
                .iter()
                .map(|(n, r)| (n.to_string(), serde_json::json!(r)))
                .collect();
            pkg_json["dependencies"] = serde_json::Value::Object(deps_obj);
        }

        std::fs::write(
            pkg_dir.join("package.json"),
            serde_json::to_string_pretty(&pkg_json).unwrap(),
        )
        .unwrap();
    }

    dir
}

#[test]
fn test_graph_simple() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with one dependency
    let project = create_project_with_node_modules(&[("a", "^1.0.0")], &[], &[("a", "1.0.0", &[])]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

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

    let graph = &json["graph"];
    assert_eq!(graph["schema_version"].as_u64(), Some(1));

    // root should be a path string
    assert!(graph["root"].as_str().is_some(), "root should be a path");

    let nodes = graph["nodes"].as_array().expect("nodes should be array");
    assert_eq!(nodes.len(), 1, "Should have 1 package (a)");

    // Should be 'a'
    assert_eq!(nodes[0]["id"]["name"].as_str(), Some("a"));
    assert_eq!(nodes[0]["id"]["version"].as_str(), Some("1.0.0"));

    // No orphans
    let orphans = graph["orphans"]
        .as_array()
        .expect("orphans should be array");
    assert!(orphans.is_empty(), "Should have no orphans");

    // No errors
    let errors = graph["errors"].as_array().expect("errors should be array");
    assert!(errors.is_empty(), "Should have no errors");
}

#[test]
fn test_graph_with_transitive_deps() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project: root -> a -> b
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[],
        &[("a", "1.0.0", &[("b", "^2.0.0")]), ("b", "2.0.0", &[])],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

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

    let graph = &json["graph"];
    let nodes = graph["nodes"].as_array().expect("nodes should be array");
    assert_eq!(nodes.len(), 2, "Should have 2 packages (a and b)");

    // Find 'a' node and check its deps
    let a_node = nodes
        .iter()
        .find(|n| n["id"]["name"].as_str() == Some("a"))
        .unwrap();
    let a_deps = a_node["dependencies"].as_array().unwrap();
    assert_eq!(a_deps.len(), 1);
    assert_eq!(a_deps[0]["name"].as_str(), Some("b"));

    // Find 'b' node
    let b_node = nodes
        .iter()
        .find(|n| n["id"]["name"].as_str() == Some("b"))
        .unwrap();
    assert_eq!(b_node["id"]["version"].as_str(), Some("2.0.0"));
}

#[test]
fn test_graph_orphan_detection() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with an orphan package (in node_modules but not referenced)
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[],
        &[
            ("a", "1.0.0", &[]),
            ("orphan", "1.0.0", &[]), // Not referenced by anything
        ],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

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

    let graph = &json["graph"];

    // Should have orphans
    let orphans = graph["orphans"]
        .as_array()
        .expect("orphans should be array");
    assert_eq!(orphans.len(), 1, "Should have 1 orphan");
    assert_eq!(orphans[0]["name"].as_str(), Some("orphan"));
}

#[test]
fn test_graph_dev_dependencies_excluded_by_default() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with dev dependency
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[("dev-pkg", "^1.0.0")],
        &[("a", "1.0.0", &[]), ("dev-pkg", "1.0.0", &[])],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph (without --dev)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

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

    let graph = &json["graph"];
    let nodes = graph["nodes"].as_array().unwrap();

    // Should have only 'a', dev-pkg should be orphan
    assert_eq!(nodes.len(), 1, "Should have 1 package (no dev)");

    let orphans = graph["orphans"].as_array().unwrap();
    assert_eq!(orphans.len(), 1, "dev-pkg should be orphan");
    assert_eq!(orphans[0]["name"].as_str(), Some("dev-pkg"));
}

#[test]
fn test_graph_dev_dependencies_included_with_flag() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with dev dependency
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[("dev-pkg", "^1.0.0")],
        &[("a", "1.0.0", &[]), ("dev-pkg", "1.0.0", &[])],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph with --dev
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--dev",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

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

    let graph = &json["graph"];
    let nodes = graph["nodes"].as_array().unwrap();

    // Should have a + dev-pkg
    assert_eq!(nodes.len(), 2, "Should have 2 packages");

    let orphans = graph["orphans"].as_array().unwrap();
    assert!(orphans.is_empty(), "No orphans when dev deps included");
}

#[test]
fn test_graph_depth_limit() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create deep chain: root -> a -> b -> c -> d
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[],
        &[
            ("a", "1.0.0", &[("b", "^1.0.0")]),
            ("b", "1.0.0", &[("c", "^1.0.0")]),
            ("c", "1.0.0", &[("d", "^1.0.0")]),
            ("d", "1.0.0", &[]),
        ],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph with max-depth=2 (a=1, b=2, then stop)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--max-depth",
            "2",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Parse results (may have errors due to depth limit)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let graph = &json["graph"];
    let nodes = graph["nodes"].as_array().unwrap();

    // Should have a, b (depth 1, 2) - c is depth-limited
    assert_eq!(nodes.len(), 2, "Should have a + b (depth limited)");

    // d should be orphan (c was depth limited so d never visited)
    let orphans = graph["orphans"].as_array().unwrap();
    assert_eq!(orphans.len(), 1, "d should be orphan");
    assert_eq!(orphans[0]["name"].as_str(), Some("d"));

    // Should have depth limit error for c
    let errors = graph["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "Should have depth limit error");
    assert!(errors
        .iter()
        .any(|e| e["code"].as_str() == Some("PKG_GRAPH_DEPTH_LIMIT_REACHED")));
}

#[test]
fn test_graph_no_node_modules_error() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project without node_modules
    let dir = tempfile::tempdir().unwrap();
    let package_json = serde_json::json!({
        "name": "test-project",
        "version": "1.0.0",
        "dependencies": {
            "a": "^1.0.0"
        }
    });
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            dir.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Parse results - will have errors about missing node_modules
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    // ok will be false because there are errors
    assert_eq!(
        json["ok"].as_bool(),
        Some(false),
        "ok should be false with errors"
    );

    let graph = &json["graph"];
    let errors = graph["errors"].as_array().expect("errors should be array");

    // Should have error about missing node_modules
    assert!(
        !errors.is_empty(),
        "Should have errors about missing node_modules"
    );

    // Find the node_modules not found error
    let has_nm_error = errors.iter().any(|e| {
        e["code"]
            .as_str()
            .is_some_and(|c| c.contains("NODE_MODULES"))
    });
    assert!(has_nm_error, "Should have NODE_MODULES error");
}

#[test]
fn test_graph_deterministic_ordering() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with multiple deps in random order
    let project = create_project_with_node_modules(
        &[
            ("zebra", "^1.0.0"),
            ("apple", "^1.0.0"),
            ("mango", "^1.0.0"),
        ],
        &[],
        &[
            ("zebra", "1.0.0", &[]),
            ("apple", "1.0.0", &[]),
            ("mango", "1.0.0", &[]),
        ],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph twice
    let output1 = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    let output2 = cargo_bin()
        .args([
            "--json",
            "pkg",
            "graph",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Both should succeed
    assert!(output1.status.success());
    assert!(output2.status.success());

    // Parse both outputs
    let json1: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output1.stdout)).unwrap();
    let json2: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output2.stdout)).unwrap();

    // Graphs should be identical (deterministic)
    assert_eq!(
        json1["graph"], json2["graph"],
        "Graphs should be identical across runs"
    );

    // Verify ordering: nodes should be sorted by (name, version)
    let nodes = json1["graph"]["nodes"].as_array().unwrap();
    let names: Vec<&str> = nodes
        .iter()
        .map(|n| n["id"]["name"].as_str().unwrap())
        .collect();

    // Alphabetical: apple, mango, zebra
    assert_eq!(names, vec!["apple", "mango", "zebra"]);
}

#[test]
fn test_graph_human_tree_format() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with nested deps
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0")],
        &[],
        &[("a", "1.0.0", &[("b", "^2.0.0")]), ("b", "2.0.0", &[])],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph without --json (human format, default tree)
    let output = cargo_bin()
        .args(["pkg", "graph", "--cwd", project.path().to_str().unwrap()])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should succeed
    assert!(
        output.status.success(),
        "Should succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show packages
    assert!(stdout.contains("a@1.0.0"), "Should show a");
    assert!(stdout.contains("b@2.0.0"), "Should show b");
}

#[test]
fn test_graph_human_list_format() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project
    let project = create_project_with_node_modules(
        &[("a", "^1.0.0"), ("b", "^2.0.0")],
        &[],
        &[("a", "1.0.0", &[]), ("b", "2.0.0", &[])],
    );

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg graph with --format list
    let output = cargo_bin()
        .args([
            "pkg",
            "graph",
            "--format",
            "list",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg graph");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should succeed
    assert!(
        output.status.success(),
        "Should succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have flat list
    assert!(stdout.contains("a@1.0.0"), "Should show a");
    assert!(stdout.contains("b@2.0.0"), "Should show b");

    // Should NOT have tree connectors in list format
    assert!(
        !stdout.contains("└──"),
        "Should not have tree connectors in list format"
    );
    assert!(
        !stdout.contains("├──"),
        "Should not have tree connectors in list format"
    );
}
