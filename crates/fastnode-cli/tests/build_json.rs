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
    assert_eq!(json["ok"], false, "ok should be false when daemon not running");

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
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
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
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
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
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
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
    let _: serde_json::Value =
        serde_json::from_str(&stdout).expect("Output should be valid JSON");
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
