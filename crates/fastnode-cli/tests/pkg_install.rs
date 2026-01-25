//! Integration tests for `howth install` command.
//!
//! These tests verify the install command's handling of lockfiles and error cases.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

/// Helper to create a minimal package.json.
fn create_package_json(dir: &std::path::Path, name: &str, version: &str) {
    let content = format!(
        r#"{{"name": "{name}", "version": "{version}", "dependencies": {{}}}}"#
    );
    fs::write(dir.join("package.json"), content).unwrap();
}

/// Helper to create a lockfile with no packages.
fn create_empty_lockfile(dir: &std::path::Path, name: &str, version: &str) {
    let content = format!(
        r#"{{
  "lockfile_version": 1,
  "root": {{
    "name": "{name}",
    "version": "{version}"
  }},
  "packages": {{}}
}}"#
    );
    fs::write(dir.join("howth.lock"), content).unwrap();
}

/// Test that `howth install --frozen-lockfile` fails when no lockfile exists.
#[test]
fn test_install_frozen_fails_without_lock() {
    let dir = tempdir().unwrap();
    create_package_json(dir.path(), "test-project", "1.0.0");

    let output = cargo_bin()
        .args(["install", "--frozen-lockfile", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run howth install");

    // Should fail with exit code 1 (daemon not running) or non-zero
    // Note: Without daemon, we get "daemon not running" error
    // But the frozen lockfile check happens in the daemon handler
    // So we expect either error about lockfile or daemon not running
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either daemon isn't running or we got the lockfile error
    assert!(
        !output.status.success(),
        "install --frozen-lockfile should fail without lockfile. stderr: {stderr}"
    );
}

/// Test that `howth install --frozen-lockfile --json` produces valid JSON even on error.
#[test]
fn test_install_frozen_json_output_on_error() {
    let dir = tempdir().unwrap();
    create_package_json(dir.path(), "test-project", "1.0.0");

    let output = cargo_bin()
        .args(["--json", "install", "--frozen-lockfile", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run howth install");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should be valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout should be valid JSON: {stdout}"));

    // Should have 'ok' field
    assert!(
        json.get("ok").is_some(),
        "JSON output should have 'ok' field"
    );

    // Should not be ok (either daemon error or lockfile error)
    assert_eq!(
        json["ok"].as_bool(),
        Some(false),
        "install --frozen without lockfile should return ok: false"
    );
}

/// Test that `howth install --json` produces no extra stdout besides JSON.
#[test]
fn test_install_json_no_extra_stdout() {
    let dir = tempdir().unwrap();
    create_package_json(dir.path(), "test-project", "1.0.0");
    create_empty_lockfile(dir.path(), "test-project", "1.0.0");

    let output = cargo_bin()
        .args(["--json", "install", "--cwd"])
        .arg(dir.path())
        .output()
        .expect("Failed to run howth install");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Stdout should be valid JSON and nothing else
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout should be valid JSON (no extra content): {stdout}"));

    // Verify it has the expected structure
    assert!(json.get("ok").is_some(), "Should have 'ok' field");

    // The JSON should either have 'install' or 'error' field
    assert!(
        json.get("install").is_some() || json.get("error").is_some(),
        "Should have either 'install' or 'error' field"
    );
}

/// Test the help output for install command.
#[test]
fn test_install_help_shows_options() {
    let output = cargo_bin()
        .args(["install", "--help"])
        .output()
        .expect("Failed to run howth install --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show the available options
    assert!(
        stdout.contains("--frozen-lockfile"),
        "Help should show --frozen-lockfile option"
    );
    assert!(
        stdout.contains("--no-dev"),
        "Help should show --no-dev option"
    );
    assert!(
        stdout.contains("--no-optional"),
        "Help should show --no-optional option"
    );
}

/// Test that install command exists and is recognized.
#[test]
fn test_install_command_exists() {
    let output = cargo_bin()
        .args(["help", "install"])
        .output()
        .expect("Failed to run howth help install");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show install command help
    assert!(
        stdout.contains("Install") || stdout.contains("install"),
        "help install should show install command info"
    );
}

/// Test that --frozen-lockfile and --no-dev can be combined.
#[test]
fn test_install_options_combination() {
    let dir = tempdir().unwrap();
    create_package_json(dir.path(), "test-project", "1.0.0");

    // This should not error on parsing (may error on execution due to no daemon)
    let output = cargo_bin()
        .args([
            "--json",
            "install",
            "--frozen-lockfile",
            "--no-dev",
            "--no-optional",
            "--cwd",
        ])
        .arg(dir.path())
        .output()
        .expect("Failed to run howth install with options");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should produce valid JSON (command was parsed correctly)
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("stdout should be valid JSON: {stdout}"));

    assert!(json.get("ok").is_some(), "Should have 'ok' field");
}

/// Verify the lockfile schema version constant is stable.
#[test]
fn test_lockfile_schema_version_stable() {
    use fastnode_core::pkg::PKG_LOCK_SCHEMA_VERSION;
    assert_eq!(PKG_LOCK_SCHEMA_VERSION, 1, "Lockfile schema version should be 1");
}

/// Verify the install protocol schema version constant is stable.
#[test]
fn test_install_protocol_schema_version_stable() {
    use fastnode_proto::PKG_INSTALL_SCHEMA_VERSION;
    assert_eq!(
        PKG_INSTALL_SCHEMA_VERSION, 1,
        "Install protocol schema version should be 1"
    );
}

/// Test lockfile error codes are valid.
#[test]
fn test_lockfile_error_codes_are_uppercase() {
    use fastnode_core::pkg::lockfile_codes;

    let codes = [
        lockfile_codes::PKG_LOCK_NOT_FOUND,
        lockfile_codes::PKG_LOCK_INVALID_JSON,
        lockfile_codes::PKG_LOCK_VERSION_MISMATCH,
        lockfile_codes::PKG_LOCK_INTEGRITY_MISMATCH,
        lockfile_codes::PKG_LOCK_PACKAGE_MISSING,
        lockfile_codes::PKG_LOCK_WRITE_FAILED,
        lockfile_codes::PKG_LOCK_STALE,
        lockfile_codes::PKG_LOCK_CONFLICT,
    ];

    for code in codes {
        assert!(
            code.chars().all(|c| c.is_uppercase() || c == '_'),
            "Lockfile error code '{code}' should be SCREAMING_SNAKE_CASE"
        );
    }
}
