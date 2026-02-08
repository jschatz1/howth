//! HTTP server benchmark harness for fastnode.
//!
//! Compares HTTP server throughput (requests per second) across howth, node, bun, and deno
//! by starting a "Hello World" HTTP server for each runtime and load testing it.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use crate::bench::build::MachineInfo;
use crate::bench::BenchWarning;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Schema version for HTTP benchmark reports.
pub const HTTP_BENCH_SCHEMA_VERSION: u32 = 1;

/// Default duration for each benchmark run in seconds.
pub const DEFAULT_DURATION_SECS: u32 = 10;

/// Default number of concurrent connections.
pub const DEFAULT_CONNECTIONS: u32 = 50;

/// Default number of warmup seconds.
pub const DEFAULT_WARMUP_SECS: u32 = 2;

/// Port to use for benchmarks (cycles through to avoid conflicts).
const BASE_PORT: u16 = 9100;

/// Parameters for the HTTP benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpBenchParams {
    /// Duration of the benchmark in seconds.
    pub duration_secs: u32,
    /// Number of concurrent connections.
    pub connections: u32,
    /// Warmup duration in seconds.
    pub warmup_secs: u32,
}

/// Result for a single tool's HTTP benchmark.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpToolResult {
    /// Tool name: "howth", "node", "bun", or "deno".
    pub tool: String,
    /// Requests per second (median across samples).
    pub rps: f64,
    /// Total requests completed.
    pub total_requests: u64,
    /// Average latency in microseconds.
    pub avg_latency_us: u64,
    /// p99 latency in microseconds.
    pub p99_latency_us: u64,
    /// Number of errors.
    pub errors: u64,
}

/// Comparison of howth vs another tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpComparison {
    /// The tool being compared against.
    pub tool: String,
    /// Speedup factor (e.g. 1.5 means "howth is 1.5x faster").
    pub speedup: f64,
}

/// Complete HTTP benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpBenchReport {
    /// Schema version.
    pub schema_version: u32,
    /// Machine information.
    pub machine: MachineInfo,
    /// Benchmark parameters.
    pub params: HttpBenchParams,
    /// Per-tool results.
    pub results: Vec<HttpToolResult>,
    /// Comparisons (howth vs each other tool).
    pub comparisons: Vec<HttpComparison>,
    /// Warnings encountered.
    pub warnings: Vec<BenchWarning>,
}

/// Run the HTTP benchmark.
///
/// # Panics
///
/// Panics if temp directory creation fails.
#[must_use]
pub fn run_http_bench(params: HttpBenchParams) -> HttpBenchReport {
    let mut warnings = Vec::new();

    if params.duration_secs < 5 {
        warnings.push(BenchWarning::warn(
            "SHORT_DURATION",
            format!(
                "Short benchmark duration ({}s); results may be noisy",
                params.duration_secs
            ),
        ));
    }

    // Create temp directory for server scripts
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");

    // Write server scripts for each runtime
    write_server_scripts(temp_dir.path());

    let mut results = Vec::new();
    let mut port = BASE_PORT;

    // Benchmark each tool
    // Note: "howth-static" is the pure Rust server (no JS callback) for theoretical max
    for tool in &["howth-static", "howth", "node", "bun", "deno"] {
        port += 1;
        if let Some(result) = bench_tool(tool, temp_dir.path(), port, &params, &mut warnings) {
            results.push(result);
        }
    }

    // Sort by RPS (highest first)
    results.sort_by(|a, b| {
        b.rps
            .partial_cmp(&a.rps)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute comparisons
    let comparisons = compute_comparisons(&results);

    HttpBenchReport {
        schema_version: HTTP_BENCH_SCHEMA_VERSION,
        machine: MachineInfo::detect(),
        params,
        results,
        comparisons,
        warnings,
    }
}

/// Write HTTP server scripts for each runtime.
fn write_server_scripts(dir: &Path) {
    // Howth native server (uses Howth.serveBatch)
    let howth_server = r"
const port = parseInt(process.argv[2] || '3000', 10);

Howth.serveBatch({ port, hostname: '127.0.0.1', batchSize: 64 }, (req) => {
    return { status: 200, body: 'Hello World\n' };
});

console.log(`READY:${port}`);
";
    fs::write(dir.join("server-howth.ts"), howth_server).expect("Failed to write howth server");

    // Howth static server (pure Rust, no JS callback - theoretical max performance)
    // Note: serveStatic prints READY internally
    let howth_static_server = r#"
const port = parseInt(process.argv[2] || '3000', 10);

const server = await Howth.serveStatic({ port, hostname: '127.0.0.1' }, 'Hello World\n');

// Keep the process running indefinitely using setInterval
setInterval(() => {}, 60000);
"#;
    fs::write(dir.join("server-howth-static.ts"), howth_static_server).expect("Failed to write howth static server");

    // Node.js server (uses node:http)
    let node_server = r"
import { createServer } from 'node:http';

const port = parseInt(process.argv[2] || '3000', 10);
const server = createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/plain' });
    res.end('Hello World\n');
});

server.listen(port, '127.0.0.1', () => {
    console.log(`READY:${port}`);
});
";
    fs::write(dir.join("server.mjs"), node_server).expect("Failed to write node server");

    // Bun server (uses Bun.serve)
    let bun_server = r"
const port = parseInt(Bun.argv[2] || '3000', 10);

const server = Bun.serve({
    port,
    hostname: '127.0.0.1',
    fetch(req) {
        return new Response('Hello World\n');
    },
});

console.log(`READY:${server.port}`);
";
    fs::write(dir.join("server-bun.ts"), bun_server).expect("Failed to write bun server");

    // Deno server (uses Deno.serve)
    let deno_server = r"
const port = parseInt(Deno.args[0] || '3000', 10);

Deno.serve({
    port,
    hostname: '127.0.0.1',
    onListen({ port }) {
        console.log(`READY:${port}`);
    }
}, (_req) => {
    return new Response('Hello World\n');
});
";
    fs::write(dir.join("server-deno.ts"), deno_server).expect("Failed to write deno server");
}

/// Benchmark a single tool.
fn bench_tool(
    tool: &str,
    script_dir: &Path,
    port: u16,
    params: &HttpBenchParams,
    warnings: &mut Vec<BenchWarning>,
) -> Option<HttpToolResult> {
    if !tool_available(tool) {
        warnings.push(BenchWarning::info(
            "TOOL_MISSING",
            format!("{tool} not found in PATH, skipping"),
        ));
        return None;
    }

    eprintln!("  Benchmarking {tool} HTTP server on port {port}...");

    // Start the server
    let mut server = match start_server(tool, script_dir, port) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(BenchWarning::warn(
                "SERVER_START_FAILED",
                format!("Failed to start {tool} server: {e}"),
            ));
            return None;
        }
    };

    // Wait for server to be ready
    if !wait_for_ready(&mut server, port, Duration::from_secs(10)) {
        warnings.push(BenchWarning::warn(
            "SERVER_NOT_READY",
            format!("{tool} server did not become ready"),
        ));
        let _ = server.kill();
        return None;
    }

    // Warmup
    eprintln!("    Warming up for {}s...", params.warmup_secs);
    run_load_test(port, params.connections, params.warmup_secs);

    // Actual benchmark
    eprintln!("    Running benchmark for {}s...", params.duration_secs);
    let result = run_load_test(port, params.connections, params.duration_secs);

    // Stop server
    let _ = server.kill();
    let _ = server.wait();

    // Small delay to let port be released
    thread::sleep(Duration::from_millis(100));

    Some(HttpToolResult {
        tool: tool.to_string(),
        rps: result.rps,
        total_requests: result.total_requests,
        avg_latency_us: result.avg_latency_us,
        p99_latency_us: result.p99_latency_us,
        errors: result.errors,
    })
}

/// Start an HTTP server for the given tool.
fn start_server(tool: &str, script_dir: &Path, port: u16) -> Result<Child, String> {
    let port_str = port.to_string();

    let child = match tool {
        "howth-static" => Command::new("howth")
            .args([
                "run",
                "--native",
                script_dir.join("server-howth-static.ts").to_str().unwrap(),
                "--",
                &port_str,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        "howth" => Command::new("howth")
            .args([
                "run",
                "--native",
                script_dir.join("server-howth.ts").to_str().unwrap(),
                "--",
                &port_str,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        "node" => Command::new("node")
            .args([script_dir.join("server.mjs").to_str().unwrap(), &port_str])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        "bun" => Command::new("bun")
            .args([
                "run",
                script_dir.join("server-bun.ts").to_str().unwrap(),
                &port_str,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        "deno" => Command::new("deno")
            .args([
                "run",
                "--allow-net",
                script_dir.join("server-deno.ts").to_str().unwrap(),
                &port_str,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        _ => return Err(format!("Unknown tool: {tool}")),
    };

    child.map_err(|e| e.to_string())
}

/// Wait for the server to print READY and accept connections.
fn wait_for_ready(server: &mut Child, port: u16, timeout: Duration) -> bool {
    let start = Instant::now();

    // Check stdout for READY message
    if let Some(stdout) = server.stdout.take() {
        let reader = BufReader::new(stdout);
        let handle = thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("READY:") {
                    return true;
                }
            }
            false
        });

        // Wait for the READY signal with timeout
        while start.elapsed() < timeout {
            if handle.is_finished() {
                return handle.join().unwrap_or(false);
            }
            // Also try connecting
            if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    // Fallback: just try connecting
    for _ in 0..20 {
        if TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }

    false
}

/// Result of a load test run.
struct LoadTestResult {
    rps: f64,
    total_requests: u64,
    avg_latency_us: u64,
    p99_latency_us: u64,
    errors: u64,
}

/// Run a load test against the server.
fn run_load_test(port: u16, connections: u32, duration_secs: u32) -> LoadTestResult {
    let total_requests = Arc::new(AtomicU64::new(0));
    let total_errors = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(std::sync::Mutex::new(Vec::new()));

    let duration = Duration::from_secs(u64::from(duration_secs));
    let start = Instant::now();

    // Spawn worker threads - each maintains a persistent connection
    let mut handles = Vec::new();
    for _ in 0..connections {
        let requests = Arc::clone(&total_requests);
        let errors = Arc::clone(&total_errors);
        let lats = Arc::clone(&latencies);

        let handle = thread::spawn(move || {
            let addr = format!("127.0.0.1:{port}");
            let mut local_latencies = Vec::new();
            let mut conn: Option<TcpStream> = None;

            while start.elapsed() < duration {
                let req_start = Instant::now();

                // Get or create connection
                let stream = match conn.take() {
                    Some(s) => s,
                    None => {
                        if let Ok(s) = TcpStream::connect(&addr) {
                            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                            let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
                            let _ = s.set_nodelay(true);
                            s
                        } else {
                            errors.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                    }
                };

                match make_request_keepalive(stream) {
                    Ok(s) => {
                        requests.fetch_add(1, Ordering::Relaxed);
                        local_latencies.push(req_start.elapsed().as_micros() as u64);
                        conn = Some(s); // Reuse connection
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        // Connection failed, will reconnect on next iteration
                    }
                }
            }

            // Merge local latencies
            if let Ok(mut guard) = lats.lock() {
                guard.extend(local_latencies);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    let elapsed = start.elapsed();
    let total = total_requests.load(Ordering::Relaxed);
    let errs = total_errors.load(Ordering::Relaxed);

    // Compute latency stats
    let (avg_latency_us, p99_latency_us) = if let Ok(mut lats) = latencies.lock() {
        if lats.is_empty() {
            (0, 0)
        } else {
            lats.sort_unstable();
            let avg = lats.iter().sum::<u64>() / lats.len() as u64;
            let p99_idx = (lats.len() * 99) / 100;
            let p99 = lats.get(p99_idx.min(lats.len() - 1)).copied().unwrap_or(0);
            (avg, p99)
        }
    } else {
        (0, 0)
    };

    let rps = total as f64 / elapsed.as_secs_f64();

    LoadTestResult {
        rps,
        total_requests: total,
        avg_latency_us,
        p99_latency_us,
        errors: errs,
    }
}

/// Make a single HTTP request over an existing connection (keep-alive).
/// Returns the connection back if successful for reuse.
fn make_request_keepalive(mut stream: TcpStream) -> Result<TcpStream, std::io::Error> {
    use std::io::{Read, Write};

    // Send HTTP/1.1 request with keep-alive
    stream.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")?;

    // Read response headers to find Content-Length
    let mut buf = [0u8; 4096];
    let mut total_read = 0;
    #[allow(unused_assignments)]
    let mut headers_end = None;
    #[allow(unused_assignments)]
    let mut content_length: Option<usize> = None;

    // Read until we have complete headers
    loop {
        let n = stream.read(&mut buf[total_read..])?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        total_read += n;

        // Look for end of headers
        if let Some(pos) = find_header_end(&buf[..total_read]) {
            headers_end = Some(pos);
            // Parse Content-Length from headers
            content_length = parse_content_length(&buf[..pos]);
            break;
        }

        if total_read >= buf.len() {
            // Headers too large, bail
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
    }

    let headers_end = headers_end.unwrap();
    let body_start = headers_end + 4; // Skip \r\n\r\n

    // Read remaining body if needed
    if let Some(content_len) = content_length {
        let body_read = total_read - body_start;
        let remaining = content_len.saturating_sub(body_read);
        if remaining > 0 {
            // Read remaining body bytes
            let mut discard = vec![0u8; remaining];
            stream.read_exact(&mut discard)?;
        }
    }

    Ok(stream)
}

/// Find the position of \r\n\r\n in the buffer (end of HTTP headers).
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Parse Content-Length header from HTTP headers.
fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let headers_str = std::str::from_utf8(headers).ok()?;
    for line in headers_str.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            let value = line[15..].trim();
            return value.parse().ok();
        }
    }
    None
}

/// Compute comparisons between howth and other tools.
fn compute_comparisons(results: &[HttpToolResult]) -> Vec<HttpComparison> {
    let howth_rps = results.iter().find(|r| r.tool == "howth").map(|r| r.rps);

    let Some(howth) = howth_rps else {
        return Vec::new();
    };

    if howth == 0.0 {
        return Vec::new();
    }

    results
        .iter()
        .filter(|r| r.tool != "howth")
        .map(|r| HttpComparison {
            tool: r.tool.clone(),
            speedup: howth / r.rps,
        })
        .collect()
}

/// Check if a tool is available in PATH.
fn tool_available(tool: &str) -> bool {
    // Map tool names to actual binaries (howth-static uses howth binary)
    let binary = match tool {
        "howth-static" => "howth",
        other => other,
    };
    Command::new("which")
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version() {
        assert_eq!(HTTP_BENCH_SCHEMA_VERSION, 1);
    }

    #[test]
    fn test_compute_comparisons_basic() {
        let results = vec![
            HttpToolResult {
                tool: "howth".to_string(),
                rps: 50000.0,
                total_requests: 500_000,
                avg_latency_us: 100,
                p99_latency_us: 500,
                errors: 0,
            },
            HttpToolResult {
                tool: "node".to_string(),
                rps: 25000.0,
                total_requests: 250_000,
                avg_latency_us: 200,
                p99_latency_us: 1000,
                errors: 0,
            },
        ];

        let comparisons = compute_comparisons(&results);
        assert_eq!(comparisons.len(), 1);
        assert_eq!(comparisons[0].tool, "node");
        assert!((comparisons[0].speedup - 2.0).abs() < 0.01);
    }
}
