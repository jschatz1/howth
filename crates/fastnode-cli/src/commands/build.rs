//! `howth build` command implementation.

use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::VERSION;
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{
    encode_frame, BuildCacheStatus, BuildRunResult, Frame, FrameResponse, Request, Response,
    BUILD_RUN_SCHEMA_VERSION,
};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::io;
use std::path::PathBuf;

/// Build command action.
#[derive(Debug, Clone)]
pub struct BuildAction {
    pub cwd: PathBuf,
    pub force: bool,
    pub dry_run: bool,
    pub max_parallel: Option<u32>,
    pub profile: bool,
    /// Show why each node was rebuilt (v2.3).
    pub why: bool,
    /// Targets to build (v2.1). Empty = use defaults.
    pub targets: Vec<String>,
}

/// Build result for JSON output (matches protocol's BuildRunResult).
#[derive(Serialize)]
struct BuildResultJson {
    schema_version: u32,
    cwd: String,
    ok: bool,
    counts: BuildCountsJson,
    summary: BuildSummaryJson,
    results: Vec<BuildNodeResultJson>,
    notes: Vec<String>,
}

#[derive(Serialize)]
struct BuildCountsJson {
    total: u32,
    succeeded: u32,
    failed: u32,
    skipped: u32,
    cache_hits: u32,
    executed: u32,
}

#[derive(Serialize)]
struct BuildSummaryJson {
    total_duration_ms: u64,
    saved_duration_ms: u64,
}

#[derive(Serialize)]
struct BuildNodeResultJson {
    id: String,
    ok: bool,
    cache: String,
    hash: String,
    duration_ms: u64,
    /// Reason for the execution status (v2.3).
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BuildErrorJson>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    stdout_truncated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    stderr_truncated: bool,
    notes: Vec<String>,
}

#[derive(Serialize)]
struct BuildErrorJson {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// Build error result for JSON output.
#[derive(Serialize)]
struct BuildErrorResult {
    schema_version: u32,
    ok: bool,
    error: BuildErrorJson,
    notes: Vec<String>,
}

/// Run the build command.
pub fn run(action: BuildAction, channel: Channel, json: bool) -> Result<()> {
    let endpoint = paths::ipc_endpoint(channel);
    let show_why = action.why;

    // Run the async client
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result = runtime.block_on(async { send_build_request(&endpoint, &action).await });

    match result {
        Ok((response, _server_version)) => handle_response(response, json, show_why),
        Err(e) => {
            if json {
                let result = BuildErrorResult {
                    schema_version: BUILD_RUN_SCHEMA_VERSION,
                    ok: false,
                    error: BuildErrorJson {
                        code: "BUILD_DAEMON_CONNECT_FAILED".to_string(),
                        message: format!("Failed to connect: {e}"),
                        detail: None,
                    },
                    notes: vec!["hint: start the daemon with `howth daemon`".to_string()],
                };
                println!("{}", serde_json::to_string(&result).unwrap());
            } else {
                eprintln!("error: daemon not running");
                eprintln!("hint: start with `howth daemon`");
            }
            std::process::exit(1);
        }
    }
}

fn handle_response(response: Response, json: bool, show_why: bool) -> Result<()> {
    match response {
        Response::BuildResult { result } => {
            let ok = result.ok;
            if json {
                let json_result = convert_to_json(result);
                println!("{}", serde_json::to_string(&json_result).unwrap());
            } else {
                print_human_output(&result, show_why);
            }

            if ok {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
        Response::Error { code, message } => {
            if json {
                let result = BuildErrorResult {
                    schema_version: BUILD_RUN_SCHEMA_VERSION,
                    ok: false,
                    error: BuildErrorJson {
                        code,
                        message,
                        detail: None,
                    },
                    notes: Vec::new(),
                };
                println!("{}", serde_json::to_string(&result).unwrap());
            } else {
                eprintln!("error: {code}: {message}");
            }
            std::process::exit(1);
        }
        _ => {
            if json {
                let result = BuildErrorResult {
                    schema_version: BUILD_RUN_SCHEMA_VERSION,
                    ok: false,
                    error: BuildErrorJson {
                        code: "BUILD_UNEXPECTED_RESPONSE".to_string(),
                        message: "Unexpected response type".to_string(),
                        detail: None,
                    },
                    notes: Vec::new(),
                };
                println!("{}", serde_json::to_string(&result).unwrap());
            } else {
                eprintln!("error: unexpected response");
            }
            std::process::exit(1);
        }
    }
}

fn print_human_output(result: &BuildRunResult, show_why: bool) {
    // v2.4: One line per node, stable ordering (already sorted by node_id from daemon)
    // Vocabulary: (cached) / (rebuilt) / (failed)

    // Collect nodes that need --why explanation (rebuilt or failed)
    let mut why_nodes: Vec<(&str, &str)> = Vec::new();

    for node_result in &result.results {
        let (symbol, status_text) = if node_result.ok {
            match node_result.cache {
                BuildCacheStatus::Hit => ("\u{2713}", "(cached)"),      // ✓
                BuildCacheStatus::Miss => ("\u{2713}", "(rebuilt)"),    // ✓
                BuildCacheStatus::Bypass => ("\u{2713}", "(rebuilt)"),  // forced = rebuilt
                BuildCacheStatus::Skipped => ("-", "(skipped)"),
            }
        } else {
            ("\u{2717}", "(failed)") // ✗
        };

        println!("{} {} {}", symbol, node_result.id, status_text);

        // Show error details immediately for failed nodes
        if !node_result.ok {
            if let Some(error) = &node_result.error {
                eprintln!("  error: {}: {}", error.code, error.message);
                if let Some(detail) = &error.detail {
                    for line in detail.lines().take(10) {
                        eprintln!("  | {}", line);
                    }
                }
            }
        }

        // Collect --why info for non-cached nodes
        if show_why && node_result.cache != BuildCacheStatus::Hit {
            if let Some(ref reason) = node_result.reason {
                why_nodes.push((&node_result.id, reason.to_human_string()));
            }
        }
    }

    // Summary line (always)
    println!();
    let rebuilt = result.counts.executed;
    let total = result.counts.total;
    let duration_ms = result.summary.total_duration_ms;

    if result.ok {
        if rebuilt == 0 {
            println!("Rebuilt 0/{} targets (cached)", total);
        } else {
            println!("Rebuilt {}/{} targets ({}ms)", rebuilt, total, duration_ms);
        }
    } else {
        println!(
            "Build failed: {}/{} targets failed",
            result.counts.failed, total
        );
    }

    // --why explanation block (v2.3, separate from node lines)
    if show_why && !why_nodes.is_empty() {
        println!();
        for (node_id, reason) in &why_nodes {
            println!("{} rebuilt because:", node_id);
            println!("  - {}", reason);
        }
    }
}

fn convert_to_json(result: BuildRunResult) -> BuildResultJson {
    BuildResultJson {
        schema_version: result.schema_version,
        cwd: result.cwd,
        ok: result.ok,
        counts: BuildCountsJson {
            total: result.counts.total,
            succeeded: result.counts.succeeded,
            failed: result.counts.failed,
            skipped: result.counts.skipped,
            cache_hits: result.counts.cache_hits,
            executed: result.counts.executed,
        },
        summary: BuildSummaryJson {
            total_duration_ms: result.summary.total_duration_ms,
            saved_duration_ms: result.summary.saved_duration_ms,
        },
        results: result
            .results
            .into_iter()
            .map(|r| BuildNodeResultJson {
                id: r.id,
                ok: r.ok,
                cache: match r.cache {
                    BuildCacheStatus::Hit => "hit".to_string(),
                    BuildCacheStatus::Miss => "miss".to_string(),
                    BuildCacheStatus::Bypass => "bypass".to_string(),
                    BuildCacheStatus::Skipped => "skipped".to_string(),
                },
                hash: r.hash,
                duration_ms: r.duration_ms,
                reason: r.reason.map(|reason| reason.to_human_string().to_string()),
                error: r.error.map(|e| BuildErrorJson {
                    code: e.code,
                    message: e.message,
                    detail: e.detail,
                }),
                stdout_truncated: r.stdout_truncated,
                stderr_truncated: r.stderr_truncated,
                notes: r.notes,
            })
            .collect(),
        notes: result.notes,
    }
}

async fn send_build_request(
    endpoint: &str,
    action: &BuildAction,
) -> io::Result<(Response, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Connect using cross-platform IpcStream
    let mut stream = IpcStream::connect(endpoint).await?;

    // Create build request
    let request = Request::Build {
        cwd: action.cwd.to_string_lossy().into_owned(),
        force: action.force,
        dry_run: action.dry_run,
        max_parallel: action.max_parallel.unwrap_or_else(default_max_parallel),
        profile: action.profile,
        targets: action.targets.clone(),
    };

    // Create and send request frame
    let frame = Frame::new(VERSION, request);
    let encoded = encode_frame(&frame)?;

    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Read response
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("response frame too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let response: FrameResponse =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((response.response, response.hello.server_version))
}

fn default_max_parallel() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
        .clamp(1, 64)
}
