//! Integration tests for `fastnode bench smoke --json` output.

use std::process::Command;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "-p", "fastnode-cli", "--bin", "howth", "--"]);
    cmd
}

#[test]
fn test_bench_smoke_json_is_valid_json() {
    let output = cargo_bin()
        .args([
            "--json", "bench", "smoke", "--iters", "10", "--warmup", "1", "--size", "1",
        ])
        .output()
        .expect("Failed to run bench smoke command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // stdout should begin with '{'
    assert!(
        stdout.trim().starts_with('{'),
        "stdout should begin with '{{': {stdout}"
    );

    // stderr should NOT begin with '{' (no JSON contamination)
    let stderr_content = stderr
        .lines()
        .filter(|l| !l.contains("Compiling") && !l.contains("Finished") && !l.contains("Running"))
        .collect::<Vec<_>>()
        .join("\n");
    if !stderr_content.trim().is_empty() {
        assert!(
            !stderr_content.trim().starts_with('{'),
            "stderr should not begin with '{{': {stderr_content}"
        );
    }

    // Parse stdout as JSON
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Verify schema version
    assert_eq!(
        json["bench_schema_version"].as_u64(),
        Some(1),
        "bench_schema_version should be 1"
    );

    // Verify runtime has required fields
    let runtime = &json["runtime"];
    assert!(
        runtime.get("fastnode_version").is_some(),
        "Missing runtime.fastnode_version"
    );
    assert!(runtime.get("os").is_some(), "Missing runtime.os");
    assert!(runtime.get("arch").is_some(), "Missing runtime.arch");

    // Verify params
    let params = &json["params"];
    assert_eq!(params["iters"].as_u64(), Some(10));
    assert_eq!(params["warmup"].as_u64(), Some(1));
    assert_eq!(params["size_bytes"].as_u64(), Some(1024 * 1024));

    // Verify results
    let results = json["results"].as_array().expect("results should be array");
    assert_eq!(results.len(), 3, "Should have exactly 3 benchmark results");

    // Verify the three named benchmarks exist
    let names: Vec<&str> = results
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"hash_file_blake3"),
        "Missing hash_file_blake3"
    );
    assert!(names.contains(&"atomic_write"), "Missing atomic_write");
    assert!(
        names.contains(&"project_root_walkup"),
        "Missing project_root_walkup"
    );

    // Verify each result has required numeric fields
    for result in results {
        let name = result["name"].as_str().unwrap();
        assert!(
            result["samples"].as_u64().is_some(),
            "{name} missing samples"
        );
        assert!(result["min_ns"].as_u64().is_some(), "{name} missing min_ns");
        assert!(
            result["median_ns"].as_u64().is_some(),
            "{name} missing median_ns"
        );
        assert!(result["p95_ns"].as_u64().is_some(), "{name} missing p95_ns");
        assert!(result["max_ns"].as_u64().is_some(), "{name} missing max_ns");
        assert_eq!(
            result["unit"].as_str(),
            Some("ns/op"),
            "{name} unit should be ns/op"
        );

        // Verify ordering: min <= median <= p95 <= max
        let min = result["min_ns"].as_u64().unwrap();
        let median = result["median_ns"].as_u64().unwrap();
        let p95 = result["p95_ns"].as_u64().unwrap();
        let max = result["max_ns"].as_u64().unwrap();

        assert!(
            min <= median,
            "{name}: min ({min}) should be <= median ({median})"
        );
        assert!(
            median <= p95,
            "{name}: median ({median}) should be <= p95 ({p95})"
        );
        assert!(p95 <= max, "{name}: p95 ({p95}) should be <= max ({max})");
    }

    // Verify warnings is an array (may be empty)
    assert!(
        json["warnings"].as_array().is_some(),
        "warnings should be an array"
    );
}

#[test]
fn test_bench_smoke_human_output_not_json() {
    let output = cargo_bin()
        .args([
            "bench", "smoke", "--iters", "5", "--warmup", "1", "--size", "1",
        ])
        .output()
        .expect("Failed to run bench smoke command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Human output should not be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "Human output should not be valid JSON"
    );

    // Should contain expected text
    assert!(stdout.contains("howth bench smoke"), "Missing header");
    assert!(stdout.contains("Params:"), "Missing Params line");
    assert!(
        stdout.contains("hash_file_blake3"),
        "Missing hash_file_blake3"
    );
    assert!(stdout.contains("atomic_write"), "Missing atomic_write");
    assert!(
        stdout.contains("project_root_walkup"),
        "Missing project_root_walkup"
    );
    assert!(stdout.contains("median") || stdout.contains("Time (median)"), "Missing median");
    assert!(stdout.contains("p95") || stdout.contains("p95:"), "Missing p95");
}

#[test]
fn test_bench_smoke_low_iters_warning() {
    let output = cargo_bin()
        .args([
            "--json", "bench", "smoke", "--iters", "5", "--warmup", "1", "--size", "1",
        ])
        .output()
        .expect("Failed to run bench smoke command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");

    let warnings = json["warnings"]
        .as_array()
        .expect("warnings should be array");
    assert!(
        warnings
            .iter()
            .any(|w| w["code"].as_str() == Some("LOW_ITERS")),
        "Should have LOW_ITERS warning for iters < 10"
    );
}
