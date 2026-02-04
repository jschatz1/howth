//! Native JavaScript runtime for howth.
//!
//! Uses deno_core (V8) to execute JavaScript without Node.js subprocess.
//!
//! ## Usage
//!
//! ```ignore
//! use fastnode_runtime::Runtime;
//!
//! let mut runtime = Runtime::new()?;
//! runtime.execute_script("console.log('Hello from V8!')")?;
//! ```

#![allow(clippy::empty_line_after_doc_comments)]
#![allow(unused_doc_comments)]
#![allow(clippy::missing_safety_doc)]
#![allow(dead_code)]
#![allow(clippy::regex_creation_in_loops)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unnecessary_to_owned)]

mod module_loader;
pub mod napi;
mod ops;
mod runtime;

pub use module_loader::{HowthModuleLoader, VirtualModuleMap};
pub use runtime::{create_local_server_future, Runtime, RuntimeError, RuntimeOptions};

/// Run a JavaScript file and return the exit code.
pub async fn run_file(path: &std::path::Path) -> Result<i32, RuntimeError> {
    let code = std::fs::read_to_string(path).map_err(|e| RuntimeError::Io(e.to_string()))?;

    let mut runtime = Runtime::new(RuntimeOptions {
        main_module: Some(path.to_path_buf()),
        ..Default::default()
    })?;

    runtime.execute_script(&code).await?;
    runtime.run_event_loop().await?;

    Ok(0)
}

/// Run JavaScript code directly and return the exit code.
pub async fn run_code(code: &str) -> Result<i32, RuntimeError> {
    let mut runtime = Runtime::new(RuntimeOptions::default())?;
    runtime.execute_script(code).await?;
    runtime.run_event_loop().await?;
    Ok(0)
}
