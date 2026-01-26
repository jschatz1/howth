//! Runtime implementation using deno_core.

use crate::module_loader::HowthModuleLoader;
use deno_core::{extension, op2, JsRuntime, ModuleSpecifier, RuntimeOptions as DenoRuntimeOptions};
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
        op_howth_fetch,
        op_howth_encode_utf8,
        op_howth_decode_utf8,
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

/// Fetch response from a URL.
#[derive(serde::Serialize)]
pub struct FetchResponse {
    pub ok: bool,
    pub status: u16,
    pub status_text: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Fetch request options.
#[derive(serde::Deserialize, Default)]
pub struct FetchOptions {
    pub method: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub body: Option<String>,
}

/// Fetch a URL (synchronous via blocking client, exposed as async op).
/// Uses reqwest::blocking to make HTTP requests.
#[op2(async)]
#[serde]
async fn op_howth_fetch(
    #[string] url: String,
    #[serde] options: Option<FetchOptions>,
) -> Result<FetchResponse, deno_core::error::AnyError> {
    // Use std::thread::spawn since tokio spawn_blocking doesn't work well with current_thread
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let result = (|| {
            let client = reqwest::blocking::Client::new();
            let opts = options.unwrap_or_default();

            let method = opts
                .method
                .as_deref()
                .unwrap_or("GET")
                .to_uppercase();

            let mut request = match method.as_str() {
                "GET" => client.get(&url),
                "POST" => client.post(&url),
                "PUT" => client.put(&url),
                "DELETE" => client.delete(&url),
                "PATCH" => client.patch(&url),
                "HEAD" => client.head(&url),
                _ => return Err(deno_core::error::AnyError::msg(format!("Unsupported method: {}", method))),
            };

            // Add headers
            if let Some(headers) = opts.headers {
                for (key, value) in headers {
                    request = request.header(&key, &value);
                }
            }

            // Add body
            if let Some(body) = opts.body {
                request = request.body(body);
            }

            let response = request.send()?;

            let status = response.status();
            let response_url = response.url().to_string();

            let mut headers = std::collections::HashMap::new();
            for (key, value) in response.headers().iter() {
                headers.insert(
                    key.to_string(),
                    value.to_str().unwrap_or("").to_string(),
                );
            }

            let body = response.text()?;

            Ok(FetchResponse {
                ok: status.is_success(),
                status: status.as_u16(),
                status_text: status.canonical_reason().unwrap_or("").to_string(),
                headers,
                body,
                url: response_url,
            })
        })();

        let _ = tx.send(result);
    });

    rx.await
        .map_err(|_| deno_core::error::AnyError::msg("Fetch cancelled"))?
}

/// Encode string to UTF-8 bytes.
#[op2]
#[serde]
fn op_howth_encode_utf8(#[string] text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// Decode UTF-8 bytes to string.
#[op2]
#[string]
fn op_howth_decode_utf8(#[buffer] bytes: &[u8]) -> Result<String, deno_core::error::AnyError> {
    String::from_utf8(bytes.to_vec()).map_err(|e| e.into())
}

impl Runtime {
    /// Create a new runtime.
    pub fn new(options: RuntimeOptions) -> Result<Self, RuntimeError> {
        let state = Rc::new(RefCell::new(RuntimeState::default()));

        let cwd = options.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });

        let module_loader = Rc::new(HowthModuleLoader::new(cwd.clone()));

        let mut js_runtime = JsRuntime::new(DenoRuntimeOptions {
            extensions: vec![howth_runtime::init_ops()],
            module_loader: Some(module_loader),
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

    /// Execute a script (non-module code).
    pub async fn execute_script(&mut self, code: &str) -> Result<(), RuntimeError> {
        self.js_runtime
            .execute_script("<howth>", code.to_string())
            .map_err(|e| RuntimeError::JavaScript(e.to_string()))?;
        Ok(())
    }

    /// Execute an ES module from a file path.
    pub async fn execute_module(&mut self, path: &std::path::Path) -> Result<(), RuntimeError> {
        let specifier = ModuleSpecifier::from_file_path(path)
            .map_err(|_| RuntimeError::Io(format!("Invalid path: {}", path.display())))?;

        let module_id = self
            .js_runtime
            .load_main_es_module(&specifier)
            .await
            .map_err(|e| RuntimeError::JavaScript(format!("Failed to load module: {}", e)))?;

        // mod_evaluate returns a receiver - we need to run the event loop
        // while waiting for the module to complete
        let mut receiver = self.js_runtime.mod_evaluate(module_id);

        // Poll both the event loop and the module evaluation receiver
        loop {
            tokio::select! {
                biased;

                // Check if module evaluation completed
                maybe_result = &mut receiver => {
                    match maybe_result {
                        Ok(()) => break,
                        Err(e) => return Err(RuntimeError::JavaScript(format!("Module evaluation failed: {}", e))),
                    }
                }

                // Drive the event loop
                event_loop_result = self.js_runtime.run_event_loop(Default::default()) => {
                    event_loop_result
                        .map_err(|e| RuntimeError::JavaScript(format!("Event loop error: {}", e)))?;
                }
            }
        }

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

    #[tokio::test]
    async fn test_text_encoder_decoder() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const encoder = new TextEncoder();
                const decoder = new TextDecoder();
                const encoded = encoder.encode('Hello');
                if (encoded.length !== 5) throw new Error('encode failed');
                if (encoded[0] !== 72) throw new Error('wrong byte');
                const decoded = decoder.decode(encoded);
                if (decoded !== 'Hello') throw new Error('decode failed');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_url() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const url = new URL('https://example.com:8080/path?foo=bar#hash');
                if (url.hostname !== 'example.com') throw new Error('hostname');
                if (url.port !== '8080') throw new Error('port');
                if (url.pathname !== '/path') throw new Error('pathname');
                if (url.search !== '?foo=bar') throw new Error('search');
                if (url.hash !== '#hash') throw new Error('hash');
                if (url.searchParams.get('foo') !== 'bar') throw new Error('searchParams');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_url_search_params() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const params = new URLSearchParams('a=1&b=2&a=3');
                if (params.get('a') !== '1') throw new Error('get first');
                if (params.get('b') !== '2') throw new Error('get b');
                const all = params.getAll('a');
                if (all.length !== 2) throw new Error('getAll length');
                if (all[0] !== '1' || all[1] !== '3') throw new Error('getAll values');
                params.set('c', '4');
                if (!params.has('c')) throw new Error('has');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_headers() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const headers = new Headers({'Content-Type': 'application/json'});
                if (headers.get('content-type') !== 'application/json') throw new Error('get');
                headers.set('X-Custom', 'value');
                if (!headers.has('x-custom')) throw new Error('has');
                headers.append('X-Custom', 'value2');
                if (headers.get('x-custom') !== 'value, value2') throw new Error('append');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_atob_btoa() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const original = 'Hello, World!';
                const encoded = btoa(original);
                if (encoded !== 'SGVsbG8sIFdvcmxkIQ==') throw new Error('btoa');
                const decoded = atob(encoded);
                if (decoded !== original) throw new Error('atob');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_request_response() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                const req = new Request('https://example.com', {
                    method: 'POST',
                    headers: {'Content-Type': 'application/json'},
                    body: '{"test": true}'
                });
                if (req.method !== 'POST') throw new Error('request method');
                if (req.url !== 'https://example.com') throw new Error('request url');

                const res = new Response('body', { status: 201, statusText: 'Created' });
                if (res.status !== 201) throw new Error('response status');
                if (!res.ok) throw new Error('response ok');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_fetch_mock() {
        // This test verifies fetch is callable but doesn't make real network requests
        // A full integration test would require a mock server
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                if (typeof fetch !== 'function') throw new Error('fetch not defined');
                if (typeof Request !== 'function') throw new Error('Request not defined');
                if (typeof Response !== 'function') throw new Error('Response not defined');
                if (typeof Headers !== 'function') throw new Error('Headers not defined');
                "#,
            )
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_set_timeout() {
        let mut runtime = Runtime::new(RuntimeOptions::default()).unwrap();
        runtime
            .execute_script(
                r#"
                let called = false;
                setTimeout(() => { called = true; }, 10);
                "#,
            )
            .await
            .unwrap();
        runtime.run_event_loop().await.unwrap();
    }
}
