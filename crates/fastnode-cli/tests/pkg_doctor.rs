//! Integration tests for `howth pkg doctor`.
//!
//! These tests create node_modules structures and verify the doctor output.

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

/// Create a test project with orphans and missing deps.
fn create_project_with_issues() -> TempDir {
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

    // Create pkg-a which depends on missing pkg-b
    let pkg_a_dir = node_modules.join("pkg-a");
    std::fs::create_dir_all(&pkg_a_dir).unwrap();

    let pkg_a_json = serde_json::json!({
        "name": "pkg-a",
        "version": "1.0.0",
        "dependencies": {
            "pkg-b": "^2.0.0"  // This dep is missing
        }
    });

    std::fs::write(
        pkg_a_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_a_json).unwrap(),
    )
    .unwrap();
    std::fs::write(pkg_a_dir.join("index.js"), "// pkg-a").unwrap();

    // Create orphan package (not referenced by anyone)
    let orphan_dir = node_modules.join("orphan-pkg");
    std::fs::create_dir_all(&orphan_dir).unwrap();

    let orphan_json = serde_json::json!({
        "name": "orphan-pkg",
        "version": "1.0.0"
    });

    std::fs::write(
        orphan_dir.join("package.json"),
        serde_json::to_string_pretty(&orphan_json).unwrap(),
    )
    .unwrap();
    std::fs::write(orphan_dir.join("index.js"), "// orphan").unwrap();

    dir
}

/// Create a healthy project with no issues.
fn create_healthy_project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();

    // Create root package.json
    let package_json = serde_json::json!({
        "name": "healthy-project",
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

    // Create pkg-a with no deps
    let pkg_a_dir = node_modules.join("pkg-a");
    std::fs::create_dir_all(&pkg_a_dir).unwrap();

    let pkg_a_json = serde_json::json!({
        "name": "pkg-a",
        "version": "1.0.0"
    });

    std::fs::write(
        pkg_a_dir.join("package.json"),
        serde_json::to_string_pretty(&pkg_a_json).unwrap(),
    )
    .unwrap();
    std::fs::write(pkg_a_dir.join("index.js"), "// pkg-a").unwrap();

    dir
}

#[test]
fn test_doctor_json_shape_locked() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor with JSON output
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

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

    // Assert locked JSON shape
    assert!(json.get("ok").is_some(), "Must have 'ok' field");
    assert!(json.get("doctor").is_some(), "Must have 'doctor' field");

    let doctor = &json["doctor"];
    assert_eq!(
        doctor["schema_version"].as_u64(),
        Some(1),
        "Schema version must be 1"
    );
    assert!(doctor.get("cwd").is_some(), "doctor.cwd required");
    assert!(doctor.get("summary").is_some(), "doctor.summary required");
    assert!(doctor.get("findings").is_some(), "doctor.findings required");

    // Summary structure
    let summary = &doctor["summary"];
    assert!(
        summary.get("severity").is_some(),
        "summary.severity required"
    );
    assert!(summary.get("counts").is_some(), "summary.counts required");
    assert!(
        summary.get("packages_indexed").is_some(),
        "summary.packages_indexed required"
    );
    assert!(
        summary.get("reachable_packages").is_some(),
        "summary.reachable_packages required"
    );
    assert!(summary.get("orphans").is_some(), "summary.orphans required");
    assert!(
        summary.get("missing_edges").is_some(),
        "summary.missing_edges required"
    );
}

#[test]
fn test_doctor_detects_orphans_and_missing_edges() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor with JSON output
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let doctor = &json["doctor"];
    let summary = &doctor["summary"];

    // Should detect orphan and missing edge
    assert_eq!(summary["orphans"].as_u64(), Some(1), "Should have 1 orphan");
    assert_eq!(
        summary["missing_edges"].as_u64(),
        Some(1),
        "Should have 1 missing edge"
    );

    // Verify findings
    let findings = doctor["findings"]
        .as_array()
        .expect("findings should be array");

    let orphan_finding = findings
        .iter()
        .find(|f| f["code"].as_str() == Some("PKG_DOCTOR_ORPHAN_PACKAGE"));
    assert!(orphan_finding.is_some(), "Should have orphan finding");

    let missing_finding = findings
        .iter()
        .find(|f| f["code"].as_str() == Some("PKG_DOCTOR_MISSING_EDGE_TARGET"));
    assert!(
        missing_finding.is_some(),
        "Should have missing edge finding"
    );
}

#[test]
fn test_doctor_human_summary_stable() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run twice
    let output1 = cargo_bin()
        .args(["pkg", "doctor", "--cwd", project.path().to_str().unwrap()])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    let output2 = cargo_bin()
        .args(["pkg", "doctor", "--cwd", project.path().to_str().unwrap()])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Outputs should be identical (deterministic)
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    assert_eq!(stdout1, stdout2, "Human output should be deterministic");
}

#[test]
fn test_doctor_severity_filtering() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run with --severity warn (should filter out info findings)
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--severity",
            "warn",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let findings = json["doctor"]["findings"]
        .as_array()
        .expect("findings should be array");

    // All findings should be warn or error (no info)
    for finding in findings {
        let severity = finding["severity"].as_str().unwrap();
        assert!(
            severity == "warn" || severity == "error",
            "Filtered findings should be warn or error, got: {}",
            severity
        );
    }
}

#[test]
fn test_doctor_no_node_modules_is_report_not_failure() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    // Create project without node_modules
    let dir = tempfile::tempdir().unwrap();
    let package_json = serde_json::json!({
        "name": "empty-project",
        "version": "1.0.0"
    });
    std::fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&package_json).unwrap(),
    )
    .unwrap();
    // Note: no node_modules directory

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            dir.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // Should succeed with exit 0 (successful daemon request)
    assert!(
        output.status.success(),
        "Should succeed even with no node_modules: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    assert_eq!(json["ok"].as_bool(), Some(true));

    // Should have NODE_MODULES_NOT_FOUND finding
    let findings = json["doctor"]["findings"]
        .as_array()
        .expect("findings should be array");
    let nm_finding = findings
        .iter()
        .find(|f| f["code"].as_str() == Some("PKG_DOCTOR_NODE_MODULES_NOT_FOUND"));
    assert!(
        nm_finding.is_some(),
        "Should have NODE_MODULES_NOT_FOUND finding"
    );
}

#[test]
fn test_doctor_healthy_project() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_healthy_project();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    let summary = &json["doctor"]["summary"];

    // Healthy project should have no issues
    assert_eq!(summary["orphans"].as_u64(), Some(0), "No orphans");
    assert_eq!(
        summary["missing_edges"].as_u64(),
        Some(0),
        "No missing edges"
    );
    assert_eq!(
        summary["severity"].as_str(),
        Some("info"),
        "Overall severity should be info"
    );
}

#[test]
fn test_doctor_list_format() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run with --format list
    let output = cargo_bin()
        .args([
            "pkg",
            "doctor",
            "--format",
            "list",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // List format should have findings with severity in brackets
    assert!(
        stdout.contains("[WARN]") || stdout.contains("[ERROR]") || stdout.contains("[INFO]"),
        "List format should contain severity markers: {}",
        stdout
    );
}

/// **LOCKED v1.7.1+**: notes field is always present in JSON output.
#[test]
fn test_doctor_v171_notes_always_present() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_healthy_project();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor with JSON output
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    // LOCKED v1.7.1+: notes field must always be present (even when empty)
    assert!(
        json["doctor"].get("notes").is_some(),
        "LOCKED v1.7.1+: notes field must always be present in JSON output"
    );
    assert!(json["doctor"]["notes"].is_array(), "notes must be an array");
}

/// **LOCKED v1.7.1+**: JSON key-set guard to prevent serde drift.
/// This test ensures the exact keys are present at each level using set equality.
/// No key ordering assumptions (serde JSON maps aren't ordered).
#[test]
fn test_doctor_json_keyguard() {
    use std::collections::HashSet;

    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor with JSON output
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Should be valid JSON");

    // Helper to get keys as HashSet
    fn keys_set(obj: &serde_json::Map<String, serde_json::Value>) -> HashSet<&str> {
        obj.keys().map(|s| s.as_str()).collect()
    }

    // Top-level keys (LOCKED): exactly {ok, doctor} or {ok, doctor, error}
    let top_keys = keys_set(json.as_object().unwrap());
    let top_required: HashSet<&str> = ["ok", "doctor"].into_iter().collect();
    let top_optional: HashSet<&str> = ["error"].into_iter().collect();
    assert!(
        top_required.is_subset(&top_keys),
        "top-level missing required keys: {:?}",
        top_required.difference(&top_keys).collect::<Vec<_>>()
    );
    let top_allowed: HashSet<&str> = top_required.union(&top_optional).copied().collect();
    let top_extra: HashSet<&str> = top_keys.difference(&top_allowed).copied().collect();
    assert!(
        top_extra.is_empty(),
        "top-level has unexpected keys: {:?}",
        top_extra
    );

    // doctor object keys (LOCKED): exactly {schema_version, cwd, summary, findings, notes}
    let doctor = json["doctor"].as_object().expect("doctor must be object");
    let doctor_keys = keys_set(doctor);
    let doctor_required: HashSet<&str> = ["schema_version", "cwd", "summary", "findings", "notes"]
        .into_iter()
        .collect();
    assert_eq!(
        doctor_keys,
        doctor_required,
        "doctor keys mismatch.\n  expected: {:?}\n  got: {:?}\n  missing: {:?}\n  extra: {:?}",
        doctor_required,
        doctor_keys,
        doctor_required.difference(&doctor_keys).collect::<Vec<_>>(),
        doctor_keys.difference(&doctor_required).collect::<Vec<_>>()
    );

    // notes must be present and be an array (LOCKED v1.7.1+)
    assert!(doctor["notes"].is_array(), "notes must be an array");

    // summary object keys (LOCKED): exact set
    let summary = doctor["summary"]
        .as_object()
        .expect("summary must be object");
    let summary_keys = keys_set(summary);
    let summary_required: HashSet<&str> = [
        "severity",
        "counts",
        "packages_indexed",
        "reachable_packages",
        "orphans",
        "missing_edges",
        "invalid_packages",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        summary_keys, summary_required,
        "summary keys mismatch.\n  expected: {:?}\n  got: {:?}",
        summary_required, summary_keys
    );

    // counts object keys (LOCKED): exactly {info, warn, error}
    let counts = summary["counts"]
        .as_object()
        .expect("counts must be object");
    let counts_keys = keys_set(counts);
    let counts_required: HashSet<&str> = ["info", "warn", "error"].into_iter().collect();
    assert_eq!(
        counts_keys, counts_required,
        "counts keys mismatch.\n  expected: {:?}\n  got: {:?}",
        counts_required, counts_keys
    );

    // finding object keys (LOCKED): check ALL findings
    let findings = doctor["findings"]
        .as_array()
        .expect("findings must be array");
    assert!(!findings.is_empty(), "should have at least one finding");

    let finding_required: HashSet<&str> = ["code", "severity", "message"].into_iter().collect();
    let finding_optional: HashSet<&str> = ["package", "path", "detail", "related"]
        .into_iter()
        .collect();
    let finding_allowed: HashSet<&str> =
        finding_required.union(&finding_optional).copied().collect();

    for (i, finding_val) in findings.iter().enumerate() {
        let finding = finding_val.as_object().expect("finding must be object");
        let finding_keys = keys_set(finding);

        // Required keys must exist
        assert!(
            finding_required.is_subset(&finding_keys),
            "finding[{}] missing required keys: {:?}",
            i,
            finding_required
                .difference(&finding_keys)
                .collect::<Vec<_>>()
        );

        // No unexpected keys allowed
        let extra_keys: HashSet<&str> =
            finding_keys.difference(&finding_allowed).copied().collect();
        assert!(
            extra_keys.is_empty(),
            "finding[{}] has unexpected keys: {:?}. Allowed: {:?}",
            i,
            extra_keys,
            finding_allowed
        );
    }
}

/// **LOCKED**: In --json mode, stdout must be valid JSON only (no extra output).
/// Errors and warnings go to stderr only.
#[test]
fn test_doctor_json_no_extra_stdout() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor with JSON output
    let output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // stdout must be ONLY valid JSON - no extra lines, no debug output
    let trimmed = stdout.trim();
    assert!(
        trimmed.starts_with('{') && trimmed.ends_with('}'),
        "stdout in --json mode must be a single JSON object, got: {}",
        &trimmed[..trimmed.len().min(200)]
    );

    // Must parse as valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(trimmed);
    assert!(
        parsed.is_ok(),
        "stdout must be valid JSON, parse error: {:?}",
        parsed.err()
    );

    // No extra lines (JSON should be single logical unit, possibly pretty-printed)
    // Count braces to detect stray output or malformed JSON
    let brace_depth: i32 = trimmed.chars().fold(0, |acc, c| match c {
        '{' | '[' => acc + 1,
        '}' | ']' => acc - 1,
        _ => acc,
    });
    assert_eq!(
        brace_depth, 0,
        "JSON must have balanced braces, depth was: {}",
        brace_depth
    );

    // Verify we can round-trip parse it
    let json: serde_json::Value = parsed.unwrap();
    assert!(json.get("ok").is_some(), "parsed JSON must have 'ok' field");
}

/// Get path to the howth binary built by cargo.
/// Builds in debug mode and returns the path.
fn get_howth_bin_path() -> String {
    // Build howth first to ensure it exists
    let build_output = Command::new(env!("CARGO"))
        .args(["build", "-p", "fastnode-cli", "--bin", "howth"])
        .output()
        .expect("Failed to build howth");

    assert!(
        build_output.status.success(),
        "Failed to build howth: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // Get the target directory
    let cargo_metadata = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .expect("Failed to get cargo metadata");

    let metadata: serde_json::Value =
        serde_json::from_slice(&cargo_metadata.stdout).expect("Failed to parse cargo metadata");

    let target_dir = metadata["target_directory"]
        .as_str()
        .expect("No target_directory in metadata");

    format!("{}/debug/howth", target_dir)
}

/// Helper to run the `fastnode` compat shim binary with HOWTH_BIN set.
fn cargo_bin_fastnode_with_howth(howth_bin: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "fastnode", "--"]);
    cmd.env("HOWTH_BIN", howth_bin);
    cmd
}

/// **LOCKED v1.8.0+**: The `fastnode` compat shim forwards to `howth` with deprecation warning.
#[test]
fn test_fastnode_shim_forwards() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Build howth and get its path
    let howth_bin = get_howth_bin_path();

    // Start daemon (using howth)
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run `fastnode pkg doctor --json` (the compat shim)
    let fastnode_output = cargo_bin_fastnode_with_howth(&howth_bin)
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run fastnode shim");

    // Run `howth pkg doctor --json` (direct)
    let howth_output = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run howth");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    // 1. Exit codes must match
    assert_eq!(
        fastnode_output.status.code(),
        howth_output.status.code(),
        "fastnode shim exit code must match howth"
    );

    // 2. Stdout JSON must be valid and contain { ok, doctor }
    let fastnode_stdout = String::from_utf8_lossy(&fastnode_output.stdout);
    let fastnode_json: serde_json::Value =
        serde_json::from_str(&fastnode_stdout).expect("fastnode shim stdout should be valid JSON");

    assert!(
        fastnode_json.get("ok").is_some(),
        "fastnode shim JSON must have 'ok' field"
    );
    assert!(
        fastnode_json.get("doctor").is_some(),
        "fastnode shim JSON must have 'doctor' field"
    );

    // 3. In --json mode, deprecation warning is suppressed (by design)
    //    Verify no warning in stderr (filtered for cargo messages)
    let fastnode_stderr = String::from_utf8_lossy(&fastnode_output.stderr);
    let stderr_lines: Vec<&str> = fastnode_stderr
        .lines()
        .filter(|l| {
            !l.contains("Compiling")
                && !l.contains("Finished")
                && !l.contains("Running")
                && !l.contains("Blocking waiting")
        })
        .collect();
    let stderr_content = stderr_lines.join("\n");

    // In --json mode, the deprecation warning should NOT appear
    assert!(
        !stderr_content.contains("renamed"),
        "In --json mode, deprecation warning should be suppressed. Got: {}",
        stderr_content
    );

    // 4. Stdout should be identical between fastnode and howth
    let howth_stdout = String::from_utf8_lossy(&howth_output.stdout);
    let howth_json: serde_json::Value =
        serde_json::from_str(&howth_stdout).expect("howth stdout should be valid JSON");

    // Compare the doctor objects (they should be identical)
    assert_eq!(
        fastnode_json["doctor"], howth_json["doctor"],
        "fastnode shim doctor output must match howth"
    );
}

/// **LOCKED v1.8.0+**: The `fastnode` shim shows deprecation warning in non-JSON mode.
#[test]
fn test_fastnode_shim_deprecation_warning() {
    // Build howth and get its path
    let howth_bin = get_howth_bin_path();

    // Run `fastnode --version` (non-JSON mode, should show deprecation)
    let fastnode_output = cargo_bin_fastnode_with_howth(&howth_bin)
        .args(["--version"])
        .output()
        .expect("Failed to run fastnode shim");

    // Filter stderr for deprecation warning
    let fastnode_stderr = String::from_utf8_lossy(&fastnode_output.stderr);
    let stderr_lines: Vec<&str> = fastnode_stderr
        .lines()
        .filter(|l| {
            !l.contains("Compiling")
                && !l.contains("Finished")
                && !l.contains("Running")
                && !l.contains("Blocking waiting")
        })
        .collect();
    let stderr_content = stderr_lines.join("\n");

    // In non-JSON mode, the deprecation warning MUST appear
    assert!(
        stderr_content.contains("fastnode") && stderr_content.contains("renamed"),
        "In non-JSON mode, stderr must contain deprecation warning. Got: {}",
        stderr_content
    );

    // Should exit successfully
    assert!(
        fastnode_output.status.success(),
        "fastnode --version should succeed"
    );
}

/// **LOCKED v1.7.1+**: Deterministic sort order is severity_rank desc, code asc, package asc, path asc.
#[test]
fn test_doctor_v171_deterministic_sort_order() {
    let endpoint = test_endpoint();
    cleanup_endpoint(&endpoint);

    let project = create_project_with_issues();

    // Start daemon
    let mut daemon = start_daemon(&endpoint);
    assert!(wait_for_daemon(&endpoint), "Daemon should start");

    // Run pkg doctor twice with JSON output
    let output1 = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    let output2 = cargo_bin()
        .args([
            "--json",
            "pkg",
            "doctor",
            "--cwd",
            project.path().to_str().unwrap(),
        ])
        .env("HOWTH_IPC_ENDPOINT", &endpoint)
        .output()
        .expect("Failed to run pkg doctor");

    // Cleanup
    let _ = daemon.kill();
    let _ = daemon.wait();
    cleanup_endpoint(&endpoint);

    let json1: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output1.stdout)).unwrap();
    let json2: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output2.stdout)).unwrap();

    // LOCKED v1.7.1+: findings order must be deterministic
    let findings1 = &json1["doctor"]["findings"];
    let findings2 = &json2["doctor"]["findings"];

    assert_eq!(
        findings1, findings2,
        "LOCKED v1.7.1+: findings order must be deterministic"
    );

    // Verify sort order: errors before warns before info
    let findings = findings1.as_array().unwrap();
    let mut last_severity_rank = 4u8; // Start with value higher than any valid rank

    for (i, finding) in findings.iter().enumerate() {
        let severity = finding["severity"].as_str().unwrap();
        let rank = match severity {
            "error" => 3,
            "warn" => 2,
            "info" => 1,
            _ => 0,
        };

        assert!(
            rank <= last_severity_rank,
            "LOCKED v1.7.1+: finding at index {} with severity '{}' should not come after higher severity. \
             Sort order: error (3) > warn (2) > info (1)",
            i,
            severity
        );
        last_severity_rank = rank;
    }
}
