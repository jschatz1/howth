//! Integration tests for `fastnode doctor --json` output.

use std::process::Command;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

#[test]
fn test_doctor_json_is_valid_json() {
    let output = cargo_bin()
        .args(["--json", "doctor"])
        .output()
        .expect("Failed to run doctor command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse stdout as JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Verify required top-level fields exist
    assert!(
        json.get("report_schema_version").is_some(),
        "Missing report_schema_version"
    );
    assert!(json.get("runtime").is_some(), "Missing runtime");
    assert!(json.get("os").is_some(), "Missing os");
    assert!(json.get("hardware").is_some(), "Missing hardware");
    assert!(json.get("paths").is_some(), "Missing paths");
    assert!(json.get("project").is_some(), "Missing project");
    assert!(json.get("capabilities").is_some(), "Missing capabilities");
    assert!(json.get("warnings").is_some(), "Missing warnings");

    // Verify report_schema_version is correct
    assert_eq!(
        json["report_schema_version"].as_u64(),
        Some(1),
        "report_schema_version should be 1"
    );

    // Verify runtime has required fields
    let runtime = &json["runtime"];
    assert!(runtime.get("version").is_some(), "Missing runtime.version");
    assert!(
        runtime.get("schema_version").is_some(),
        "Missing runtime.schema_version"
    );
    assert!(runtime.get("channel").is_some(), "Missing runtime.channel");

    // Verify os has required fields
    let os = &json["os"];
    assert!(os.get("name").is_some(), "Missing os.name");
    assert!(os.get("arch").is_some(), "Missing os.arch");
    // version is optional (can be null)

    // Verify stderr doesn't contain JSON (no log contamination)
    // Stderr may contain cargo build output, but should not have JSON from our tool
    let stderr_trimmed = stderr.trim();
    if !stderr_trimmed.is_empty() {
        // If there's content in stderr, it shouldn't start with '{' (JSON)
        // unless it's cargo output like "Compiling" or "Finished"
        for line in stderr_trimmed.lines() {
            if !line.trim().is_empty()
                && !line.contains("Compiling")
                && !line.contains("Finished")
                && !line.contains("Running")
            {
                assert!(
                    !line.trim().starts_with('{'),
                    "stderr should not contain JSON: {line}"
                );
            }
        }
    }
}

#[test]
fn test_doctor_json_warnings_have_stable_codes() {
    let output = cargo_bin()
        .args(["--json", "doctor"])
        .output()
        .expect("Failed to run doctor command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    let warnings = json["warnings"]
        .as_array()
        .expect("warnings should be array");

    // All warnings should have code, severity, and message
    for warning in warnings {
        assert!(
            warning.get("code").is_some(),
            "Warning missing code: {warning}"
        );
        assert!(
            warning.get("severity").is_some(),
            "Warning missing severity: {warning}"
        );
        assert!(
            warning.get("message").is_some(),
            "Warning missing message: {warning}"
        );

        // Code should be SCREAMING_SNAKE_CASE
        let code = warning["code"].as_str().unwrap();
        assert!(
            code.chars().all(|c| c.is_uppercase() || c == '_'),
            "Warning code should be SCREAMING_SNAKE_CASE: {code}"
        );

        // Severity should be "info" or "warn"
        let severity = warning["severity"].as_str().unwrap();
        assert!(
            severity == "info" || severity == "warn",
            "Invalid severity: {severity}"
        );
    }
}

#[test]
fn test_doctor_human_output_not_json() {
    let output = cargo_bin()
        .arg("doctor")
        .output()
        .expect("Failed to run doctor command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human output should not be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "Human output should not be valid JSON"
    );

    // Should contain section headers
    assert!(stdout.contains("Runtime"), "Missing Runtime section");
    assert!(stdout.contains("OS"), "Missing OS section");
    assert!(stdout.contains("Hardware"), "Missing Hardware section");
    assert!(stdout.contains("Paths"), "Missing Paths section");
    assert!(
        stdout.contains("Capabilities"),
        "Missing Capabilities section"
    );
    assert!(stdout.contains("Warnings"), "Missing Warnings section");
}
