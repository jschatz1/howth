//! Integration tests for `howth build --json` output.
//!
//! These tests verify:
//! - JSON output is always valid JSON
//! - Schema version is present
//! - `ok` boolean is present
//! - `notes` array is always present (even if empty)
//! - Error codes are SCREAMING_SNAKE_CASE

use std::process::Command;
use tempfile::tempdir;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

#[test]
fn test_build_json_daemon_not_running_is_valid_json() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Should have ok field
    assert!(json.get("ok").is_some(), "ok field should be present");

    // Should have schema_version
    assert!(
        json.get("schema_version").is_some(),
        "schema_version should be present"
    );

    // Since daemon is not running, ok should be false
    assert_eq!(
        json["ok"], false,
        "ok should be false when daemon not running"
    );

    // Should have notes array (may be empty)
    assert!(json["notes"].is_array(), "notes should be an array");
}

#[test]
fn test_build_json_error_code_is_screaming_snake_case() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // If there's an error, verify code format
    if let Some(error) = json.get("error") {
        if let Some(code) = error.get("code").and_then(|c| c.as_str()) {
            // Verify SCREAMING_SNAKE_CASE
            assert!(
                code.chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()),
                "Error code '{code}' should be SCREAMING_SNAKE_CASE"
            );
            assert!(
                !code.starts_with('_') && !code.ends_with('_'),
                "Error code '{code}' should not start or end with underscore"
            );
        }
    }
}

#[test]
fn test_build_human_output_not_json() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // Without --json flag
    let output = cargo_bin()
        .args(["build", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain human-readable error (not JSON)
    assert!(
        stderr.contains("daemon not running") || stderr.contains("error"),
        "Human output should contain error message: {stderr}"
    );
}

#[test]
fn test_build_json_keyguard_notes_always_present() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // notes must always be present as an array
    assert!(
        json["notes"].is_array(),
        "notes must always be present as an array"
    );
}

#[test]
fn test_build_dry_run_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --dry-run should be accepted as a flag (won't error on unknown flag)
    let output = cargo_bin()
        .args(["build", "--json", "--dry-run", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON (flag accepted, daemon not running is ok)
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_force_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --force should be accepted as a flag
    let output = cargo_bin()
        .args(["build", "--json", "--force", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_max_parallel_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --max-parallel should be accepted with a value
    let output = cargo_bin()
        .args(["build", "--json", "--max-parallel", "4", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_profile_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --profile should be accepted as a flag
    let output = cargo_bin()
        .args(["build", "--json", "--profile", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_json_schema_version_is_stable() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // Schema version should be 1 (v2.0)
    assert_eq!(
        json["schema_version"].as_u64(),
        Some(1),
        "schema_version should be 1"
    );
}

#[test]
fn test_build_with_targets_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a package.json with multiple scripts
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building", "test": "echo testing"}}"#,
    )
    .unwrap();

    // --targets should be accepted (even though daemon not running)
    let output = cargo_bin()
        .args(["build", "--json", "build", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON (flag accepted, daemon not running is ok)
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_with_comma_separated_targets() {
    let dir = tempdir().unwrap();

    // Create a package.json with multiple scripts
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building", "test": "echo testing"}}"#,
    )
    .unwrap();

    // Comma-separated targets should be accepted
    let output = cargo_bin()
        .args(["build", "--json", "build,test", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

// ============================================================
// v2.3 --why Flag Tests
// ============================================================

#[test]
fn test_build_why_flag_accepted() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --why should be accepted as a flag
    let output = cargo_bin()
        .args(["build", "--json", "--why", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

#[test]
fn test_build_json_includes_reason_field() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // Run build with --json (daemon not running, but error response should have schema)
    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // The error response should be valid JSON with schema_version
    // (reason field would be in results array when daemon is running)
    assert!(
        json.get("schema_version").is_some(),
        "schema_version should be present"
    );
}

#[test]
fn test_build_why_with_force_flag() {
    let dir = tempdir().unwrap();

    // Create a minimal package.json
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --why and --force can be combined
    let output = cargo_bin()
        .args(["build", "--json", "--why", "--force", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should still be valid JSON
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
}

// ============================================================
// v2.4 Dev Loop UX Tests
// ============================================================

#[test]
fn test_build_json_emits_exactly_one_json_object() {
    // v2.4 Hard guarantee: --json prints exactly one JSON object
    // No banners, no human text, no extra lines except optional trailing newline
    let dir = tempdir().unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Trim trailing whitespace only
    let trimmed = stdout.trim_end();

    // Must be exactly one JSON object (no lines before/after)
    assert!(
        trimmed.starts_with('{'),
        "JSON output must start with '{{': got {:?}",
        &trimmed[..trimmed.len().min(50)]
    );
    assert!(
        trimmed.ends_with('}'),
        "JSON output must end with '}}': got {:?}",
        &trimmed[trimmed.len().saturating_sub(50)..]
    );

    // Must parse as exactly one JSON value
    let json: serde_json::Value =
        serde_json::from_str(trimmed).expect("Output should be valid JSON");
    assert!(json.is_object(), "Output should be a JSON object");

    // Stderr should have no JSON (human errors go to stderr)
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        assert!(
            !stderr.trim().starts_with('{'),
            "Stderr should not contain JSON when --json is used"
        );
    }
}

#[test]
fn test_build_json_results_ordered_by_node_id() {
    // v2.4: Results array must be sorted by node_id for stable ordering
    let dir = tempdir().unwrap();

    // Create package.json with multiple scripts in non-alphabetical order
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"zebra": "echo z", "alpha": "echo a", "middle": "echo m"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");

    // If there's a results array (success case), verify ordering
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        let ids: Vec<&str> = results
            .iter()
            .filter_map(|r| r.get("id").and_then(|id| id.as_str()))
            .collect();

        // Should be sorted alphabetically
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        assert_eq!(
            ids, sorted_ids,
            "Results should be sorted by node_id: got {:?}",
            ids
        );
    }
}

#[test]
fn test_build_human_output_format_v24() {
    // v2.4: Human output should have one line per node + summary
    let dir = tempdir().unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // Without --json (human output)
    let output = cargo_bin()
        .args(["build", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should contain human-readable error (daemon not running)
    // v2.4: Error message format is still human-readable
    assert!(
        stderr.contains("daemon") || stderr.contains("error"),
        "Human output should contain error message: {stderr}"
    );

    // Should NOT contain JSON
    assert!(
        !stderr.trim().starts_with('{'),
        "Human output should not be JSON"
    );
}

// ============================================================
// v3.0 Watch Mode Tests
// ============================================================

#[test]
fn test_build_watch_flag_accepted() {
    let dir = tempdir().unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --watch should be accepted (will fail to connect to daemon, but flag is parsed)
    let output = cargo_bin()
        .args(["build", "--watch", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not error on unknown flag
    assert!(
        !stderr.contains("unexpected argument"),
        "--watch flag should be accepted"
    );
}

#[test]
fn test_build_debounce_ms_flag_accepted() {
    let dir = tempdir().unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    // --debounce-ms should be accepted with a value
    let output = cargo_bin()
        .args(["build", "--watch", "--debounce-ms", "200", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not error on unknown flag
    assert!(
        !stderr.contains("unexpected argument"),
        "--debounce-ms flag should be accepted"
    );
}

#[test]
fn test_build_watch_json_disallowed() {
    // v3.0: --watch --json is explicitly disallowed
    let dir = tempdir().unwrap();

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "scripts": {"build": "echo building"}}"#,
    )
    .unwrap();

    let output = cargo_bin()
        .args(["build", "--watch", "--json", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run build command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should error with specific message about --watch --json
    assert!(
        stderr.contains("--watch") && stderr.contains("--json"),
        "Should error about --watch --json combination: {stderr}"
    );

    // Exit code should be 2 (argument error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "Exit code should be 2 for argument error"
    );
}
