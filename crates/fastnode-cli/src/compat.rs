//! Backward compatibility shim for `fastnode` → `howth` rename.
//!
//! This binary prints a deprecation notice and then delegates to the main CLI.

use std::process::ExitCode;

fn main() -> ExitCode {
    // Check if we're in JSON mode (suppress notice)
    let args: Vec<String> = std::env::args().collect();
    let is_json = args.iter().any(|a| a == "--json" || a == "-j");

    if !is_json {
        eprintln!("note: `fastnode` has been renamed to `howth`");
        eprintln!("      this alias will be removed in a future release");
        eprintln!();
    }

    // Re-exec as howth with same args
    let status = std::process::Command::new("howth")
        .args(&args[1..])
        .status();

    match status {
        Ok(s) => {
            if let Some(code) = s.code() {
                ExitCode::from(code as u8)
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: failed to execute howth: {e}");
            eprintln!("hint: ensure `howth` is in your PATH");
            ExitCode::FAILURE
        }
    }
}
