//! `fastnode run` command implementation.

use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::{build_run_plan, runplan_codes, RunPlanInput, RunPlanOutput, VERSION};
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{encode_frame, Frame, FrameResponse, Request, Response, RunPlan};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::io;
use std::path::Path;

/// Exit code for validation errors.
const EXIT_VALIDATION_ERROR: i32 = 2;

/// Exit code for internal errors.
const EXIT_INTERNAL_ERROR: i32 = 1;

/// Run the run command.
///
/// Generates an execution plan either locally or via daemon.
pub fn run(
    cwd: &Path,
    entry: &Path,
    args: &[String],
    daemon: bool,
    channel: Channel,
    json: bool,
) -> Result<()> {
    if daemon {
        run_via_daemon(cwd, entry, args, channel, json)
    } else {
        run_local(cwd, entry, args, channel, json)
    }
}

/// Generate execution plan locally.
fn run_local(
    cwd: &Path,
    entry: &Path,
    args: &[String],
    channel: Channel,
    json: bool,
) -> Result<()> {
    let input = RunPlanInput {
        cwd: cwd.to_path_buf(),
        entry: entry.to_path_buf(),
        args: args.to_vec(),
        channel,
    };

    match build_run_plan(input) {
        Ok(plan) => {
            output_plan_local(&plan, json);
            Ok(())
        }
        Err(e) => {
            let exit_code = map_error_code_to_exit(e.code());
            if json {
                let error_json = serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": e.code(),
                        "message": e.to_string()
                    }
                });
                println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(exit_code);
        }
    }
}

/// Generate execution plan via daemon.
fn run_via_daemon(
    cwd: &Path,
    entry: &Path,
    args: &[String],
    channel: Channel,
    json: bool,
) -> Result<()> {
    let endpoint = paths::ipc_endpoint(channel);

    // Canonicalize cwd for sending to daemon
    let cwd_str = cwd.to_string_lossy().into_owned();
    let entry_str = entry.to_string_lossy().into_owned();

    // Run the async client
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result =
        runtime.block_on(async { send_run_request(&endpoint, &entry_str, args, &cwd_str).await });

    match result {
        Ok((response, _server_version)) => handle_daemon_response(response, json),
        Err(e) => {
            let exit_code = EXIT_INTERNAL_ERROR;
            if json {
                let error_json = serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "DAEMON_CONNECTION_FAILED",
                        "message": format!("Failed to connect to daemon: {e}")
                    }
                });
                println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
            } else {
                eprintln!("error: daemon not running");
                eprintln!("hint: start with `howth daemon`");
            }
            std::process::exit(exit_code);
        }
    }
}

/// Handle daemon response.
fn handle_daemon_response(response: Response, json: bool) -> Result<()> {
    match response {
        Response::RunPlan { plan } => {
            output_plan_daemon(&plan, json);
            Ok(())
        }
        Response::Error { code, message } => {
            let exit_code = map_error_code_to_exit(&code);
            if json {
                let error_json = serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": code,
                        "message": message
                    }
                });
                println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
            } else {
                eprintln!("error: {code}: {message}");
            }
            std::process::exit(exit_code);
        }
        _ => {
            if json {
                let error_json = serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "UNEXPECTED_RESPONSE",
                        "message": "Received unexpected response type from daemon"
                    }
                });
                println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
            } else {
                eprintln!("error: unexpected response from daemon");
            }
            std::process::exit(EXIT_INTERNAL_ERROR);
        }
    }
}

/// Trait for types that can be output as a run plan.
trait PlanOutput: Serialize {
    fn resolved_cwd(&self) -> &str;
    fn requested_entry(&self) -> &str;
    fn resolved_entry(&self) -> Option<&str>;
    fn entry_kind(&self) -> &str;
    fn args(&self) -> &[String];
    fn channel(&self) -> &str;
    fn notes(&self) -> &[String];
}

impl PlanOutput for RunPlanOutput {
    fn resolved_cwd(&self) -> &str {
        &self.resolved_cwd
    }
    fn requested_entry(&self) -> &str {
        &self.requested_entry
    }
    fn resolved_entry(&self) -> Option<&str> {
        self.resolved_entry.as_deref()
    }
    fn entry_kind(&self) -> &str {
        &self.entry_kind
    }
    fn args(&self) -> &[String] {
        &self.args
    }
    fn channel(&self) -> &str {
        &self.channel
    }
    fn notes(&self) -> &[String] {
        &self.notes
    }
}

impl PlanOutput for RunPlan {
    fn resolved_cwd(&self) -> &str {
        &self.resolved_cwd
    }
    fn requested_entry(&self) -> &str {
        &self.requested_entry
    }
    fn resolved_entry(&self) -> Option<&str> {
        self.resolved_entry.as_deref()
    }
    fn entry_kind(&self) -> &str {
        &self.entry_kind
    }
    fn args(&self) -> &[String] {
        &self.args
    }
    fn channel(&self) -> &str {
        &self.channel
    }
    fn notes(&self) -> &[String] {
        &self.notes
    }
}

/// Output the run plan from local execution in human or JSON format.
fn output_plan_local(plan: &RunPlanOutput, json: bool) {
    output_plan(plan, json);
}

/// Output the run plan from daemon in human or JSON format.
fn output_plan_daemon(plan: &RunPlan, json: bool) {
    output_plan(plan, json);
}

/// Implementation for outputting a plan.
fn output_plan<T: PlanOutput>(plan: &T, json: bool) {
    if json {
        // JSON: output just the plan, no wrapper
        println!("{}", serde_json::to_string_pretty(plan).unwrap());
    } else {
        // Human format
        println!("CWD: {}", plan.resolved_cwd());
        println!(
            "Entry: {} -> {}",
            plan.requested_entry(),
            plan.resolved_entry().unwrap_or("(not resolved)")
        );
        println!("Kind: {}", plan.entry_kind());
        if !plan.args().is_empty() {
            println!("Args: {}", plan.args().join(" "));
        }
        println!("Channel: {}", plan.channel());
        if !plan.notes().is_empty() {
            println!("Notes:");
            for note in plan.notes() {
                println!("  - {note}");
            }
        }
    }
}

/// Map error code to exit code.
fn map_error_code_to_exit(code: &str) -> i32 {
    // Match against both proto and core codes (they should be the same strings)
    match code {
        runplan_codes::ENTRY_NOT_FOUND
        | runplan_codes::ENTRY_IS_DIR
        | runplan_codes::ENTRY_INVALID
        | runplan_codes::CWD_INVALID => EXIT_VALIDATION_ERROR,
        _ => EXIT_INTERNAL_ERROR,
    }
}

/// Send a Run request to the daemon.
async fn send_run_request(
    endpoint: &str,
    entry: &str,
    args: &[String],
    cwd: &str,
) -> io::Result<(Response, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Connect using cross-platform IpcStream
    let mut stream = IpcStream::connect(endpoint).await?;

    // Create and send request frame
    let frame = Frame::new(
        VERSION,
        Request::Run {
            entry: entry.to_string(),
            args: args.to_vec(),
            cwd: Some(cwd.to_string()),
        },
    );
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
