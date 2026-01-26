//! Runtime implementation using deno_core.

use deno_core::{extension, op2, JsRuntime, RuntimeOptions as DenoRuntimeOptions};
use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

/// Runtime error.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("JavaScript error: {0}")]
    JavaScript(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Runtime initialization failed: {0}")]
    Init(String),
}

/// Runtime configuration options.
#[derive(Debug, Clone, Default)]
pub struct RuntimeOptions {
    /// Main module path (for ES module resolution).
    pub main_module: Option<PathBuf>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
}

/// Shared state for the runtime.
#[derive(Default)]
pub struct RuntimeState {
    /// Exit code set by process.exit().
    pub exit_code: i32,
}

/// The howth JavaScript runtime.
pub struct Runtime {
    js_runtime: JsRuntime,
    state: Rc<RefCell<RuntimeState>>,
}

// Define our custom ops extension
extension!(
    howth_runtime,
    ops = [
        op_howth_print,
        op_howth_print_error,
        op_howth_read_file,
        op_howth_write_file,
        op_howth_cwd,
        op_howth_env_get,
        op_howth_exit,
        op_howth_args,
    ],
);

/// Bootstrap JavaScript code to set up globals like console, process, etc.
const BOOTSTRAP_JS: &str = include_str!("bootstrap.js");

/// Print to stdout.
#[op2(fast)]
fn op_howth_print(#[string] msg: &str) {
    print!("{}", msg);
}

/// Print to stderr.
#[op2(fast)]
fn op_howth_print_error(#[string] msg: &str) {
    eprint!("{}", msg);
}

/// Read a file as string.
#[op2]
#[string]
fn op_howth_read_file(#[string] path: &str) -> Result<String, deno_core::error::AnyError> {
    std::fs::read_to_string(path).map_err(|e| e.into())
}

/// Write string to a file.
#[op2(fast)]
fn op_howth_write_file(#[string] path: &str, #[string] contents: &str) -> Result<(), deno_core::error::AnyError> {
    std::fs::write(path, contents).map_err(|e| e.into())
}

/// Get current working directory.
#[op2]
#[string]
fn op_howth_cwd() -> Result<String, deno_core::error::AnyError> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.into())
}

/// Get environment variable.
#[op2]
#[string]
fn op_howth_env_get(#[string] key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// Exit the process.
#[op2(fast)]
fn op_howth_exit(code: i32) {
    std::process::exit(code);
}

/// Get command line arguments.
#[op2]
#[serde]
fn op_howth_args() -> Vec<String> {
    std::env::args().collect()
}

impl Runtime {
    /// Create a new runtime.
    pub fn new(options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let state = Rc::new(RefCell::new(RuntimeState::default()));

        let mut js_runtime = JsRuntime::new(DenoRuntimeOptions {
            extensions: vec![howth_runtime::init_ops()],
            ..Default::default()
        });

        // Set up cwd if provided
        if let Some(cwd) = options.cwd {
            std::env::set_current_dir(&cwd)
                .map_err(|e| RuntimeError::Init(format!("Failed to set cwd: {}", e)))?;
        }

        // Execute bootstrap code to set up globals
        js_runtime
            .execute_script("<howth:bootstrap>", BOOTSTRAP_JS.to_string())
            .map_err(|e| RuntimeError::Init(format!("Bootstrap failed: {}", e)))?;

        Ok(Self { js_runtime, state })
    }

    /// Execute a script.
    pub async fn execute_script(&mut self, code: &str) -> Result<(), RuntimeError> {
        self.js_runtime
            .execute_script("<howth>", code.to_string())
            .map_err(|e| RuntimeError::JavaScript(e.to_string()))?;
        Ok(())
    }

    /// Run the event loop until completion.
    pub async fn run_event_loop(&mut self) -> Result<(), RuntimeError> {
        self.js_runtime
            .run_event_loop(Default::default())
            .await
            .map_err(|e| RuntimeError::JavaScript(e.to_string()))?;
        Ok(())
    }

    /// Get the exit code.
    pub fn exit_code(&self) -> i32 {
        self.state.borrow().exit_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_execution() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime.execute_script("1 + 1").await.unwrap();
    }

    #[tokio::test]
    async fn test_console_log() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime.execute_script("console.log('hello')").await.unwrap();
        runtime.run_event_loop().await.unwrap();
    }

    #[tokio::test]
    async fn test_process_env() {
        std::env::set_var("HOWTH_TEST_VAR", "test_value");
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (process.env.HOWTH_TEST_VAR !== 'test_value') {
                    throw new Error('env var not found');
                }
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_process_cwd() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const cwd = process.cwd();
                if (typeof cwd !== 'string' || cwd.length === 0) {
                    throw new Error('cwd failed');
                }
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_variables() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const x = 10;
                const y = 20;
                const sum = x + y;
                if (sum !== 30) throw new Error('math failed');
                "#,
            )
            .await
            .unwrap();
    }
}
