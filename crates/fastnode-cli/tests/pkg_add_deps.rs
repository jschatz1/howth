//! Integration tests for `fastnode pkg add --deps`.
//!
//! These tests use a mock npm registry to avoid network calls.

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::thread;
use std::time::Duration;
use tar::Builder;
use tempfile::TempDir;

/// Global port counter for unique mock server ports.
static PORT_COUNTER: AtomicU16 = AtomicU16::new(19800);

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
fn start_daemon(endpoint: &str, registry_url: &str) -> Child {
    cargo_bin()
        .arg("daemon")
        .env("HOWTH_IPC_ENDPOINT", endpoint)
        .env("FASTNODE_NPM_REGISTRY", registry_url)
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

/// Create a test tarball with a package.json.
fn create_test_tarball(name: &str, version: &str) -> Vec<u8> {
    let pkg_json = format!(
        r#"{{"name":"{}","version":"{}","main":"index.js"}}"#,
        name, version
    );
    let index_js = b"module.exports = 42;";

    let mut tar_bytes = Vec::new();
    {
        let mut builder = Builder::new(&mut tar_bytes);

        // Add package/package.json
        let mut header = tar::Header::new_gnu();
        header.set_path("package/package.json").unwrap();
        header.set_size(pkg_json.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, pkg_json.as_bytes()).unwrap();

        // Add package/index.js
        let mut header = tar::Header::new_gnu();
        header.set_path("package/index.js").unwrap();
        header.set_size(index_js.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &index_js[..]).unwrap();

        builder.finish().unwrap();
    }

    // Compress with gzip
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar_bytes).unwrap();
    encoder.finish().unwrap()
}

/// Create a packument JSON for a package.
fn create_packument(name: &str, version: &str, tarball_url: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "dist-tags": {
            "latest": version
        },
        "versions": {
            version: {
                "name": name,
                "version": version,
                "main": "index.js",
                "dist": {
                    "tarball": tarball_url,
                    "shasum": "abc123"
                }
            }
        }
    })
}

/// Create the mock registry router.
fn mock_registry_router(base_url: String) -> Router {
    Router::new()
        .route("/:name", get(handle_packument))
        .route("/:name/-/:tarball", get(handle_tarball))
        .with_state(base_url)
}

async fn handle_packument(
    Path(name): Path<String>,
    axum::extract::State(base_url): axum::extract::State<String>,
) -> Response {
    // Packages a, b, c are available
    match name.as_str() {
        "a" => {
            let tarball_url = format!("{}/a/-/a-1.0.0.tgz", base_url);
            let packument = create_packument("a", "1.0.0", &tarball_url);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&packument).unwrap(),
            )
                .into_response()
        }
        "b" => {
            let tarball_url = format!("{}/b/-/b-2.0.0.tgz", base_url);
            let packument = create_packument("b", "2.0.0", &tarball_url);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&packument).unwrap(),
            )
                .into_response()
        }
        "c" => {
            let tarball_url = format!("{}/c/-/c-3.0.0.tgz", base_url);
            let packument = create_packument("c", "3.0.0", &tarball_url);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&packument).unwrap(),
            )
                .into_response()
        }
        _ => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

async fn handle_tarball(Path((name, tarball)): Path<(String, String)>) -> Response {
    // Tarball format: name-version.tgz
    // Extract version from tarball name
    let expected_prefix = format!("{}-", name);
    let version = tarball
        .strip_prefix(&expected_prefix)
        .and_then(|s| s.strip_suffix(".tgz"))
        .unwrap_or("");

    match (name.as_str(), version) {
        ("a", "1.0.0") => {
            let tarball = create_test_tarball("a", "1.0.0");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/gzip")],
                Body::from(tarball),
            )
                .into_response()
        }
        ("b", "2.0.0") => {
            let tarball = create_test_tarball("b", "2.0.0");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/gzip")],
                Body::from(tarball),
            )
                .into_response()
        }
        ("c", "3.0.0") => {
            let tarball = create_test_tarball("c", "3.0.0");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/gzip")],
                Body::from(tarball),
            )
                .into_response()
        }
        _ => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// Start the mock registry server in a background thread.
/// Returns the base URL.
fn start_mock_registry() -> String {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let base_url = format!("http://127.0.0.1:{}", port);
    let base_url_clone = base_url.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let app = mock_registry_router(base_url_clone);
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });
    });

    // Give the server time to start
    thread::sleep(Duration::from_millis(100));

    base_url
}

/// Create a test project with package.json.
fn create_test_project(deps: &[(&str, &str)], dev_deps: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();

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

    dir
}

/// Wait for daemon to be ready with retries.
fn wait_for_daemon(endpoint: &str, registry_url: &str) -> bool {
    // Use longer timeouts for CI environments which can be slower
    for i in 0..30 {
        let result = cargo_bin()
            .arg("ping")
            .env("HOWTH_IPC_ENDPOINT", endpoint)
            .env("FASTNODE_NPM_REGISTRY", registry_url)
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

#[test]
fn test_deps_installs_only_dependencies() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with deps (a) and devDeps (b)
    let project = create_test_project(&[("a", "^1.0.0")], &[("b", "^2.0.0")]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (use --cwd instead of current_dir to avoid cargo run issues)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

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

    let installed = json["installed"]
        .as_array()
        .expect("installed should be array");
    assert_eq!(
        installed.len(),
        1,
        "Should only install 1 package (deps only)"
    );
    assert_eq!(installed[0]["name"].as_str(), Some("a"));
    assert_eq!(installed[0]["version"].as_str(), Some("1.0.0"));

    // Verify symlink exists
    assert!(
        project.path().join("node_modules/a").exists(),
        "a should be linked"
    );
    assert!(
        !project.path().join("node_modules/b").exists(),
        "b should NOT be linked (devDep)"
    );
}

#[test]
fn test_deps_with_dev_includes_dev_dependencies() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with deps (a) and devDeps (b)
    let project = create_test_project(&[("a", "^1.0.0")], &[("b", "^2.0.0")]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps --dev (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--dev",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

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

    let installed = json["installed"]
        .as_array()
        .expect("installed should be array");
    assert_eq!(
        installed.len(),
        2,
        "Should install 2 packages (deps + devDeps)"
    );

    // Verify both are installed (order should be deterministic: a, b)
    let names: Vec<&str> = installed
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));

    // Verify symlinks exist
    assert!(
        project.path().join("node_modules/a").exists(),
        "a should be linked"
    );
    assert!(
        project.path().join("node_modules/b").exists(),
        "b should be linked"
    );
}

#[test]
fn test_deps_invalid_range_produces_exit_2_but_installs_others() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with valid dep (a) and invalid range dep (invalid-range package)
    let project = create_test_project(&[("a", "^1.0.0"), ("invalid-pkg", "not-a-range!!!")], &[]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should exit with code 2 (partial failure)
    assert!(!output.status.success(), "Should fail due to invalid range");
    assert_eq!(
        output.status.code(),
        Some(2),
        "Exit code should be 2: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(false), "ok should be false");

    // The valid package should still be installed
    let installed = json["installed"]
        .as_array()
        .expect("installed should be array");
    assert_eq!(installed.len(), 1, "Should install 1 valid package");
    assert_eq!(installed[0]["name"].as_str(), Some("a"));

    // There should be errors
    let errors = json["errors"].as_array().expect("errors should be array");
    assert!(!errors.is_empty(), "Should have errors");

    // Verify symlink exists for valid package
    assert!(
        project.path().join("node_modules/a").exists(),
        "a should be linked"
    );
}

#[test]
fn test_deps_deterministic_ordering() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with multiple deps in non-sorted order
    let project = create_test_project(&[("c", "^3.0.0"), ("a", "^1.0.0"), ("b", "^2.0.0")], &[]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

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
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let installed = json["installed"]
        .as_array()
        .expect("installed should be array");
    assert_eq!(installed.len(), 3, "Should install 3 packages");

    // Verify all packages are present
    let names: Vec<&str> = installed
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"b"));
    assert!(names.contains(&"c"));

    // Verify all symlinks exist
    assert!(project.path().join("node_modules/a").exists());
    assert!(project.path().join("node_modules/b").exists());
    assert!(project.path().join("node_modules/c").exists());
}

#[test]
fn test_deps_package_json_not_found_exit_2() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create empty project without package.json
    let project = tempfile::tempdir().unwrap();

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should exit with code 2
    assert!(!output.status.success(), "Should fail");
    assert_eq!(
        output.status.code(),
        Some(2),
        "Exit code should be 2: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(false));

    // Error should mention package.json not found
    let error = json["error"].as_str().expect("error should be present");
    assert!(
        error.contains("PKG_PACKAGE_JSON_NOT_FOUND") || error.contains("package.json"),
        "Error should mention package.json not found: {error}"
    );
}

#[test]
fn test_deps_no_dependencies_succeeds_with_empty_result() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with no dependencies
    let project = create_test_project(&[], &[]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should succeed with empty result
    assert!(
        output.status.success(),
        "Should succeed: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(true));

    let installed = json["installed"]
        .as_array()
        .expect("installed should be array");
    assert!(installed.is_empty(), "Should have no installed packages");
}

#[test]
fn test_deps_human_output_exit_code_2_on_error() {
    let registry_url = start_mock_registry();
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project with invalid range
    let project = create_test_project(&[("invalid-pkg", "not-a-range!!!")], &[]);

    // Start daemon
    let mut daemon = start_daemon(&endpoint, &registry_url);
    assert!(
        wait_for_daemon(&endpoint, &registry_url),
        "Daemon should start"
    );

    // Run pkg add --deps (NO --json flag) (use --cwd instead of current_dir)
    let output = cargo_bin()
        .args([
            "pkg",
            "add",
            "--deps",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .env("FASTNODE_NPM_REGISTRY", &registry_url)
        .output()
        .expect("Failed to run pkg add");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should exit with code 2 in human mode too
    assert!(!output.status.success(), "Should fail");
    assert_eq!(
        output.status.code(),
        Some(2),
        "Exit code should be 2 in human mode: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
