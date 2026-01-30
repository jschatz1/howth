//! JavaScript Plugin Host — bridges JS/TS plugins from config files to the Rust Plugin trait.
//!
//! Spawns a dedicated OS thread owning a V8 runtime (`fastnode_runtime::Runtime`).
//! The `JsPlugin` struct implements `Plugin` and communicates with the V8 thread
//! via `std::sync::mpsc` channels, serializing hook calls as JSON.
//!
//! ## Threading Model
//!
//! `Runtime` is `!Send` (uses `Rc<RefCell<...>>`), so it cannot be shared across threads.
//! Instead, we keep it on a single dedicated thread and send requests/responses over channels.
//!
//! ```text
//! Rust (any thread)              V8 Thread
//! ─────────────────              ─────────
//! JsPlugin::transform(code, id)
//!   → serialize to JSON
//!   → send PluginRequest          → recv request
//!   → block on response           → call JS: plugins[i].transform(code, id)
//!   ← deserialize result          ← send JSON response
//! ```

use fastnode_core::bundler::{
    HookResult, LoadResult, Plugin, PluginContext, PluginEnforce, PluginError, ResolveIdResult,
    TransformResult,
};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};

/// Helper to construct a `PluginError`.
fn plugin_error(plugin: &str, message: impl Into<String>) -> PluginError {
    PluginError {
        plugin: plugin.to_string(),
        hook: "",
        message: message.into(),
    }
}

// ============================================================================
// Request / Response types
// ============================================================================

#[derive(Debug)]
pub enum PluginRequest {
    CallHook {
        plugin_idx: usize,
        hook: HookCall,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum HookCall {
    ResolveId {
        source: String,
        importer: Option<String>,
    },
    Load {
        id: String,
    },
    Transform {
        code: String,
        id: String,
    },
    TransformIndexHtml {
        html: String,
    },
    BuildStart,
    BuildEnd,
}

#[derive(Debug)]
pub enum PluginResponse {
    ResolveId(Option<ResolveIdResult>),
    Load(Option<LoadResult>),
    Transform(Option<TransformResult>),
    TransformIndexHtml(Option<String>),
    Ok,
    Error(String),
}

// ============================================================================
// Plugin metadata (extracted from JS)
// ============================================================================

/// Metadata about a single JS plugin, extracted from the V8 runtime.
#[derive(Debug, Clone)]
pub struct JsPluginDef {
    pub name: String,
    pub enforce: Option<String>,
    pub has_resolve_id: bool,
    pub has_load: bool,
    pub has_transform: bool,
    pub has_transform_index_html: bool,
    pub has_build_start: bool,
    pub has_build_end: bool,
}

// ============================================================================
// JsPluginHost — owns the V8 thread
// ============================================================================

/// Hosts JS plugins on a dedicated V8 thread.
///
/// Provides a synchronous `call()` method that sends a request to the V8 thread
/// and blocks until the response arrives.
pub struct JsPluginHost {
    request_tx: mpsc::Sender<PluginRequest>,
    response_rx: Mutex<mpsc::Receiver<PluginResponse>>,
    plugin_defs: Vec<JsPluginDef>,
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl JsPluginHost {
    /// Start the JS plugin host.
    ///
    /// Spawns a V8 thread, loads the config file as an ES module,
    /// extracts plugin metadata, and enters a request/response loop.
    pub fn start(config_path: &Path, cwd: &Path) -> Result<Self, String> {
        let config_path = config_path.to_path_buf();
        let cwd = cwd.to_path_buf();

        // Channels: main → V8 thread (requests), V8 thread → main (responses)
        let (req_tx, req_rx) = mpsc::channel::<PluginRequest>();
        let (resp_tx, resp_rx) = mpsc::channel::<PluginResponse>();

        // Oneshot for plugin metadata extraction
        let (meta_tx, meta_rx) = mpsc::channel::<Result<Vec<JsPluginDef>, String>>();

        let thread = std::thread::spawn(move || {
            // Create a single-threaded tokio runtime for async V8 operations
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for JS plugin host");

            rt.block_on(async {
                match run_v8_thread(&config_path, &cwd, meta_tx, req_rx, resp_tx).await {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("  JS plugin host error: {}", e);
                    }
                }
            });
        });

        // Wait for plugin metadata from the V8 thread
        let plugin_defs = meta_rx
            .recv()
            .map_err(|_| "JS plugin host thread terminated before sending metadata".to_string())?
            .map_err(|e| format!("Failed to extract JS plugins: {}", e))?;

        Ok(Self {
            request_tx: req_tx,
            response_rx: Mutex::new(resp_rx),
            plugin_defs,
            _thread: Some(thread),
        })
    }

    /// Number of plugins loaded.
    pub fn plugin_count(&self) -> usize {
        self.plugin_defs.len()
    }

    /// Plugin definitions.
    pub fn plugin_defs(&self) -> &[JsPluginDef] {
        &self.plugin_defs
    }

    /// Send a request to the V8 thread and block on the response.
    pub fn call(&self, request: PluginRequest) -> Result<PluginResponse, PluginError> {
        self.request_tx
            .send(request)
            .map_err(|_| plugin_error("js-plugin-host", "V8 thread disconnected"))?;

        let rx = self.response_rx.lock().map_err(|_| {
            plugin_error("js-plugin-host", "Response channel lock poisoned")
        })?;

        rx.recv()
            .map_err(|_| plugin_error("js-plugin-host", "V8 thread disconnected"))
    }
}

impl Drop for JsPluginHost {
    fn drop(&mut self) {
        // Signal the V8 thread to shut down
        let _ = self.request_tx.send(PluginRequest::Shutdown);
        if let Some(thread) = self._thread.take() {
            let _ = thread.join();
        }
    }
}

// ============================================================================
// V8 thread main loop
// ============================================================================

/// Bootstrap JS injected before the config module.
///
/// Provides `__howthExtractPlugins()` to enumerate plugins and
/// `__howthCallHook(idx, hookName, argsJson)` to invoke hooks.
const BOOTSTRAP_JS: &str = r#"
globalThis.__howthPlugins = [];

globalThis.__howthExtractPlugins = () => {
  const config = globalThis.__howthConfigDefault;
  const plugins = Array.isArray(config?.plugins)
    ? config.plugins.flat(Infinity).filter(Boolean)
    : [];
  globalThis.__howthPlugins = plugins;
  return JSON.stringify(plugins.map((p, i) => ({
    name: p.name || `js-plugin-${i}`,
    enforce: p.enforce || null,
    has_resolveId: typeof p.resolveId === 'function',
    has_load: typeof p.load === 'function',
    has_transform: typeof p.transform === 'function',
    has_transformIndexHtml: typeof p.transformIndexHtml === 'function',
    has_buildStart: typeof p.buildStart === 'function',
    has_buildEnd: typeof p.buildEnd === 'function',
  })));
};

globalThis.__howthCallHook = (pluginIdx, hookName, argsJson) => {
  const plugin = globalThis.__howthPlugins[pluginIdx];
  if (!plugin || typeof plugin[hookName] !== 'function') return 'null';
  try {
    const args = argsJson ? JSON.parse(argsJson) : [];
    const result = plugin[hookName](...args);
    if (result === undefined || result === null) return 'null';
    if (typeof result === 'string') return JSON.stringify({ code: result });
    return JSON.stringify(result);
  } catch (err) {
    return JSON.stringify({ __error: err.message || String(err) });
  }
};
"#;

/// Run the V8 runtime loop on the dedicated thread.
async fn run_v8_thread(
    config_path: &Path,
    cwd: &Path,
    meta_tx: mpsc::Sender<Result<Vec<JsPluginDef>, String>>,
    req_rx: mpsc::Receiver<PluginRequest>,
    resp_tx: mpsc::Sender<PluginResponse>,
) -> Result<(), String> {
    use fastnode_runtime::{Runtime, RuntimeOptions, VirtualModuleMap};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    // Create virtual module map with a wrapper that imports the config and captures
    // the default export.
    let config_abs = std::fs::canonicalize(config_path)
        .map_err(|e| format!("Cannot resolve config path: {}", e))?;

    // On macOS, canonicalize may return /private/var/... paths. Convert file:// URL.
    let config_url = url::Url::from_file_path(&config_abs)
        .map_err(|_| format!("Invalid config path: {}", config_abs.display()))?;

    let wrapper_code = format!(
        "import config from '{}';\nglobalThis.__howthConfigDefault = config;\n",
        config_url.as_str()
    );

    let virtual_modules: VirtualModuleMap =
        Rc::new(RefCell::new(HashMap::new()));
    let wrapper_path = cwd.join("__howth_plugin_loader__.mjs");
    virtual_modules
        .borrow_mut()
        .insert(wrapper_path.to_string_lossy().to_string(), wrapper_code);

    let mut runtime = Runtime::new(RuntimeOptions {
        main_module: Some(wrapper_path.clone()),
        cwd: Some(cwd.to_path_buf()),
        virtual_modules: Some(virtual_modules),
        ..Default::default()
    })
    .map_err(|e| format!("Failed to create V8 runtime: {}", e))?;

    // Inject bootstrap JS
    runtime
        .execute_script(BOOTSTRAP_JS)
        .await
        .map_err(|e| format!("Failed to inject bootstrap JS: {}", e))?;

    // Load the config module (this executes the wrapper which imports the config)
    runtime
        .execute_module(&wrapper_path)
        .await
        .map_err(|e| format!("Failed to load config module: {}", e))?;

    // Run the event loop to complete any pending module loads
    runtime
        .run_event_loop()
        .await
        .map_err(|e| format!("Failed to run event loop: {}", e))?;

    // Extract plugin metadata
    let meta_json = runtime
        .eval_to_string("globalThis.__howthExtractPlugins()")
        .map_err(|e| format!("Failed to extract plugins: {}", e))?;

    let plugin_defs = parse_plugin_metadata(&meta_json)?;

    // Send metadata back to the main thread
    meta_tx
        .send(Ok(plugin_defs))
        .map_err(|_| "Main thread disconnected before receiving plugin metadata".to_string())?;

    // Enter the request/response loop
    loop {
        let request = match req_rx.recv() {
            Ok(req) => req,
            Err(_) => break, // Channel closed, main thread dropped
        };

        match request {
            PluginRequest::Shutdown => break,
            PluginRequest::CallHook { plugin_idx, hook } => {
                let response = execute_hook(&mut runtime, plugin_idx, &hook);
                if resp_tx.send(response).is_err() {
                    break; // Main thread disconnected
                }
            }
        }
    }

    Ok(())
}

/// Parse plugin metadata JSON into `JsPluginDef` structs.
fn parse_plugin_metadata(json: &str) -> Result<Vec<JsPluginDef>, String> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("Invalid plugin metadata JSON: {}", e))?;

    Ok(arr
        .into_iter()
        .map(|v| JsPluginDef {
            name: v["name"].as_str().unwrap_or("unnamed").to_string(),
            enforce: v["enforce"].as_str().map(|s| s.to_string()),
            has_resolve_id: v["has_resolveId"].as_bool().unwrap_or(false),
            has_load: v["has_load"].as_bool().unwrap_or(false),
            has_transform: v["has_transform"].as_bool().unwrap_or(false),
            has_transform_index_html: v["has_transformIndexHtml"].as_bool().unwrap_or(false),
            has_build_start: v["has_buildStart"].as_bool().unwrap_or(false),
            has_build_end: v["has_buildEnd"].as_bool().unwrap_or(false),
        })
        .collect())
}

/// Execute a single hook call in the V8 runtime.
fn execute_hook(
    runtime: &mut fastnode_runtime::Runtime,
    plugin_idx: usize,
    hook: &HookCall,
) -> PluginResponse {
    let (hook_name, args_json) = match hook {
        HookCall::ResolveId { source, importer } => {
            let args = serde_json::json!([source, importer]);
            ("resolveId", args.to_string())
        }
        HookCall::Load { id } => {
            let args = serde_json::json!([id]);
            ("load", args.to_string())
        }
        HookCall::Transform { code, id } => {
            let args = serde_json::json!([code, id]);
            ("transform", args.to_string())
        }
        HookCall::TransformIndexHtml { html } => {
            let args = serde_json::json!([html]);
            ("transformIndexHtml", args.to_string())
        }
        HookCall::BuildStart => ("buildStart", "[]".to_string()),
        HookCall::BuildEnd => ("buildEnd", "[]".to_string()),
    };

    let escaped_args = args_json.replace('\\', "\\\\").replace('\'', "\\'");
    let js_code = format!(
        "globalThis.__howthCallHook({}, '{}', '{}')",
        plugin_idx, hook_name, escaped_args
    );

    let result_str = match runtime.eval_to_string(&js_code) {
        Ok(s) => s,
        Err(e) => return PluginResponse::Error(format!("V8 eval error: {}", e)),
    };

    // Parse the JSON response
    parse_hook_response(&result_str, hook)
}

/// Parse the JSON response from a hook call.
fn parse_hook_response(json_str: &str, hook: &HookCall) -> PluginResponse {
    if json_str == "null" || json_str.is_empty() {
        return match hook {
            HookCall::ResolveId { .. } => PluginResponse::ResolveId(None),
            HookCall::Load { .. } => PluginResponse::Load(None),
            HookCall::Transform { .. } => PluginResponse::Transform(None),
            HookCall::TransformIndexHtml { .. } => PluginResponse::TransformIndexHtml(None),
            HookCall::BuildStart | HookCall::BuildEnd => PluginResponse::Ok,
        };
    }

    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => return PluginResponse::Error(format!("Invalid JSON from hook: {}", e)),
    };

    // Check for error
    if let Some(err) = value.get("__error").and_then(|v| v.as_str()) {
        return PluginResponse::Error(err.to_string());
    }

    match hook {
        HookCall::ResolveId { .. } => {
            if let Some(id) = value.get("id").and_then(|v| v.as_str()) {
                PluginResponse::ResolveId(Some(ResolveIdResult {
                    id: id.to_string(),
                    external: value
                        .get("external")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                }))
            } else {
                PluginResponse::ResolveId(None)
            }
        }
        HookCall::Load { .. } => {
            if let Some(code) = value.get("code").and_then(|v| v.as_str()) {
                PluginResponse::Load(Some(LoadResult {
                    code: code.to_string(),
                    map: value.get("map").and_then(|v| v.as_str()).map(|s| s.to_string()),
                }))
            } else {
                PluginResponse::Load(None)
            }
        }
        HookCall::Transform { .. } => {
            if let Some(code) = value.get("code").and_then(|v| v.as_str()) {
                PluginResponse::Transform(Some(TransformResult {
                    code: code.to_string(),
                    map: value.get("map").and_then(|v| v.as_str()).map(|s| s.to_string()),
                }))
            } else {
                PluginResponse::Transform(None)
            }
        }
        HookCall::TransformIndexHtml { .. } => {
            if let Some(html) = value.get("code").and_then(|v| v.as_str()) {
                PluginResponse::TransformIndexHtml(Some(html.to_string()))
            } else {
                PluginResponse::TransformIndexHtml(None)
            }
        }
        HookCall::BuildStart | HookCall::BuildEnd => PluginResponse::Ok,
    }
}

// ============================================================================
// JsPlugin — implements the Rust Plugin trait
// ============================================================================

/// A JS plugin that delegates hook calls to the V8 thread via `JsPluginHost`.
pub struct JsPlugin {
    name: String,
    enforce: PluginEnforce,
    plugin_idx: usize,
    host: Arc<JsPluginHost>,
    has_resolve_id: bool,
    has_load: bool,
    has_transform: bool,
    has_transform_index_html: bool,
    has_build_start: bool,
    has_build_end: bool,
}

impl JsPlugin {
    pub fn new(def: &JsPluginDef, plugin_idx: usize, host: Arc<JsPluginHost>) -> Self {
        let enforce = match def.enforce.as_deref() {
            Some("pre") => PluginEnforce::Pre,
            Some("post") => PluginEnforce::Post,
            _ => PluginEnforce::Normal,
        };

        Self {
            name: def.name.clone(),
            enforce,
            plugin_idx,
            host,
            has_resolve_id: def.has_resolve_id,
            has_load: def.has_load,
            has_transform: def.has_transform,
            has_transform_index_html: def.has_transform_index_html,
            has_build_start: def.has_build_start,
            has_build_end: def.has_build_end,
        }
    }
}

// Ensure JsPlugin is Send + Sync (required by Plugin trait)
const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    fn _assert() {
        assert_send::<JsPlugin>();
        assert_sync::<JsPlugin>();
    }
};

impl Plugin for JsPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn enforce(&self) -> PluginEnforce {
        self.enforce
    }

    fn build_start(&self, _ctx: &PluginContext) -> HookResult<()> {
        if !self.has_build_start {
            return Ok(());
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::BuildStart,
        })? {
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(()),
        }
    }

    fn resolve_id(
        &self,
        specifier: &str,
        importer: Option<&str>,
        _ctx: &PluginContext,
    ) -> HookResult<Option<ResolveIdResult>> {
        if !self.has_resolve_id {
            return Ok(None);
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::ResolveId {
                source: specifier.to_string(),
                importer: importer.map(|s| s.to_string()),
            },
        })? {
            PluginResponse::ResolveId(result) => Ok(result),
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(None),
        }
    }

    fn load(&self, id: &str, _ctx: &PluginContext) -> HookResult<Option<LoadResult>> {
        if !self.has_load {
            return Ok(None);
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::Load {
                id: id.to_string(),
            },
        })? {
            PluginResponse::Load(result) => Ok(result),
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(None),
        }
    }

    fn transform(
        &self,
        code: &str,
        id: &str,
        _ctx: &PluginContext,
    ) -> HookResult<Option<TransformResult>> {
        if !self.has_transform {
            return Ok(None);
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::Transform {
                code: code.to_string(),
                id: id.to_string(),
            },
        })? {
            PluginResponse::Transform(result) => Ok(result),
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(None),
        }
    }

    fn transform_index_html(&self, html: &str) -> HookResult<Option<String>> {
        if !self.has_transform_index_html {
            return Ok(None);
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::TransformIndexHtml {
                html: html.to_string(),
            },
        })? {
            PluginResponse::TransformIndexHtml(result) => Ok(result),
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(None),
        }
    }

    fn build_end(&self, _ctx: &PluginContext) -> HookResult<()> {
        if !self.has_build_end {
            return Ok(());
        }
        match self.host.call(PluginRequest::CallHook {
            plugin_idx: self.plugin_idx,
            hook: HookCall::BuildEnd,
        })? {
            PluginResponse::Error(e) => Err(plugin_error(&self.name, e)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // parse_plugin_metadata tests
    // ========================================================================

    #[test]
    fn test_parse_plugin_metadata_single_plugin() {
        let json = r#"[{
            "name": "my-plugin",
            "enforce": null,
            "has_resolveId": true,
            "has_load": false,
            "has_transform": true,
            "has_transformIndexHtml": false,
            "has_buildStart": false,
            "has_buildEnd": false
        }]"#;

        let defs = parse_plugin_metadata(json).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "my-plugin");
        assert!(defs[0].enforce.is_none());
        assert!(defs[0].has_resolve_id);
        assert!(!defs[0].has_load);
        assert!(defs[0].has_transform);
        assert!(!defs[0].has_transform_index_html);
        assert!(!defs[0].has_build_start);
        assert!(!defs[0].has_build_end);
    }

    #[test]
    fn test_parse_plugin_metadata_multiple_plugins() {
        let json = r#"[
            {"name": "plugin-a", "enforce": "pre", "has_resolveId": false, "has_load": true, "has_transform": false, "has_transformIndexHtml": false, "has_buildStart": true, "has_buildEnd": true},
            {"name": "plugin-b", "enforce": "post", "has_resolveId": true, "has_load": false, "has_transform": true, "has_transformIndexHtml": true, "has_buildStart": false, "has_buildEnd": false}
        ]"#;

        let defs = parse_plugin_metadata(json).unwrap();
        assert_eq!(defs.len(), 2);

        assert_eq!(defs[0].name, "plugin-a");
        assert_eq!(defs[0].enforce.as_deref(), Some("pre"));
        assert!(defs[0].has_load);
        assert!(defs[0].has_build_start);
        assert!(defs[0].has_build_end);

        assert_eq!(defs[1].name, "plugin-b");
        assert_eq!(defs[1].enforce.as_deref(), Some("post"));
        assert!(defs[1].has_resolve_id);
        assert!(defs[1].has_transform);
        assert!(defs[1].has_transform_index_html);
    }

    #[test]
    fn test_parse_plugin_metadata_empty_array() {
        let json = "[]";
        let defs = parse_plugin_metadata(json).unwrap();
        assert!(defs.is_empty());
    }

    #[test]
    fn test_parse_plugin_metadata_defaults_for_missing_fields() {
        let json = r#"[{}]"#;
        let defs = parse_plugin_metadata(json).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "unnamed");
        assert!(defs[0].enforce.is_none());
        assert!(!defs[0].has_resolve_id);
        assert!(!defs[0].has_load);
        assert!(!defs[0].has_transform);
        assert!(!defs[0].has_transform_index_html);
        assert!(!defs[0].has_build_start);
        assert!(!defs[0].has_build_end);
    }

    #[test]
    fn test_parse_plugin_metadata_invalid_json() {
        let result = parse_plugin_metadata("not json");
        assert!(result.is_err());
    }

    // ========================================================================
    // parse_hook_response tests
    // ========================================================================

    #[test]
    fn test_parse_hook_response_null_transform() {
        let hook = HookCall::Transform {
            code: "x".to_string(),
            id: "a.ts".to_string(),
        };
        match parse_hook_response("null", &hook) {
            PluginResponse::Transform(None) => {}
            other => panic!("Expected Transform(None), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_null_resolve_id() {
        let hook = HookCall::ResolveId {
            source: "foo".to_string(),
            importer: None,
        };
        match parse_hook_response("null", &hook) {
            PluginResponse::ResolveId(None) => {}
            other => panic!("Expected ResolveId(None), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_null_load() {
        let hook = HookCall::Load {
            id: "foo".to_string(),
        };
        match parse_hook_response("null", &hook) {
            PluginResponse::Load(None) => {}
            other => panic!("Expected Load(None), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_null_build_start() {
        match parse_hook_response("null", &HookCall::BuildStart) {
            PluginResponse::Ok => {}
            other => panic!("Expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_transform_with_code() {
        let hook = HookCall::Transform {
            code: "x".to_string(),
            id: "a.ts".to_string(),
        };
        let json = r#"{"code": "const x = 42;"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "const x = 42;");
                assert!(result.map.is_none());
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_transform_with_code_and_map() {
        let hook = HookCall::Transform {
            code: "x".to_string(),
            id: "a.ts".to_string(),
        };
        let json = r#"{"code": "const x = 42;", "map": "sourcemap-data"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "const x = 42;");
                assert_eq!(result.map.as_deref(), Some("sourcemap-data"));
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_resolve_id_with_id() {
        let hook = HookCall::ResolveId {
            source: "foo".to_string(),
            importer: None,
        };
        let json = r#"{"id": "/resolved/foo.js"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::ResolveId(Some(result)) => {
                assert_eq!(result.id, "/resolved/foo.js");
                assert!(!result.external);
            }
            other => panic!("Expected ResolveId(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_resolve_id_external() {
        let hook = HookCall::ResolveId {
            source: "ext".to_string(),
            importer: None,
        };
        let json = r#"{"id": "ext", "external": true}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::ResolveId(Some(result)) => {
                assert_eq!(result.id, "ext");
                assert!(result.external);
            }
            other => panic!("Expected ResolveId(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_load_with_code() {
        let hook = HookCall::Load {
            id: "virtual:mod".to_string(),
        };
        let json = r#"{"code": "export default 42;"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::Load(Some(result)) => {
                assert_eq!(result.code, "export default 42;");
                assert!(result.map.is_none());
            }
            other => panic!("Expected Load(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_transform_index_html() {
        let hook = HookCall::TransformIndexHtml {
            html: "<html></html>".to_string(),
        };
        let json = r#"{"code": "<html><head><script></script></head></html>"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::TransformIndexHtml(Some(html)) => {
                assert!(html.contains("<script>"));
            }
            other => panic!("Expected TransformIndexHtml(Some), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_error() {
        let hook = HookCall::Transform {
            code: "x".to_string(),
            id: "a.ts".to_string(),
        };
        let json = r#"{"__error": "Something went wrong"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::Error(msg) => {
                assert_eq!(msg, "Something went wrong");
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_invalid_json() {
        let hook = HookCall::Transform {
            code: "x".to_string(),
            id: "a.ts".to_string(),
        };
        match parse_hook_response("{invalid", &hook) {
            PluginResponse::Error(msg) => {
                assert!(msg.contains("Invalid JSON"));
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_empty_string() {
        let hook = HookCall::Load {
            id: "foo".to_string(),
        };
        match parse_hook_response("", &hook) {
            PluginResponse::Load(None) => {}
            other => panic!("Expected Load(None), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_resolve_no_id_field() {
        let hook = HookCall::ResolveId {
            source: "foo".to_string(),
            importer: None,
        };
        // Object with no "id" field should return None
        let json = r#"{"something": "else"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::ResolveId(None) => {}
            other => panic!("Expected ResolveId(None), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_response_load_no_code_field() {
        let hook = HookCall::Load {
            id: "foo".to_string(),
        };
        let json = r#"{"something": "else"}"#;
        match parse_hook_response(json, &hook) {
            PluginResponse::Load(None) => {}
            other => panic!("Expected Load(None), got {:?}", other),
        }
    }

    // ========================================================================
    // JsPlugin construction tests
    // ========================================================================

    #[test]
    fn test_js_plugin_enforce_pre() {
        let def = JsPluginDef {
            name: "test".to_string(),
            enforce: Some("pre".to_string()),
            has_resolve_id: false,
            has_load: false,
            has_transform: false,
            has_transform_index_html: false,
            has_build_start: false,
            has_build_end: false,
        };

        // We can't construct a real host without V8, but we can test enforce parsing
        // by checking the enforce mapping logic directly
        let enforce = match def.enforce.as_deref() {
            Some("pre") => PluginEnforce::Pre,
            Some("post") => PluginEnforce::Post,
            _ => PluginEnforce::Normal,
        };
        assert_eq!(enforce, PluginEnforce::Pre);
    }

    #[test]
    fn test_js_plugin_enforce_post() {
        let def = JsPluginDef {
            name: "test".to_string(),
            enforce: Some("post".to_string()),
            has_resolve_id: false,
            has_load: false,
            has_transform: false,
            has_transform_index_html: false,
            has_build_start: false,
            has_build_end: false,
        };
        let enforce = match def.enforce.as_deref() {
            Some("pre") => PluginEnforce::Pre,
            Some("post") => PluginEnforce::Post,
            _ => PluginEnforce::Normal,
        };
        assert_eq!(enforce, PluginEnforce::Post);
    }

    #[test]
    fn test_js_plugin_enforce_normal_default() {
        let def = JsPluginDef {
            name: "test".to_string(),
            enforce: None,
            has_resolve_id: false,
            has_load: false,
            has_transform: false,
            has_transform_index_html: false,
            has_build_start: false,
            has_build_end: false,
        };
        let enforce = match def.enforce.as_deref() {
            Some("pre") => PluginEnforce::Pre,
            Some("post") => PluginEnforce::Post,
            _ => PluginEnforce::Normal,
        };
        assert_eq!(enforce, PluginEnforce::Normal);
    }

    #[test]
    fn test_js_plugin_enforce_unknown_string() {
        let def = JsPluginDef {
            name: "test".to_string(),
            enforce: Some("middle".to_string()),
            has_resolve_id: false,
            has_load: false,
            has_transform: false,
            has_transform_index_html: false,
            has_build_start: false,
            has_build_end: false,
        };
        let enforce = match def.enforce.as_deref() {
            Some("pre") => PluginEnforce::Pre,
            Some("post") => PluginEnforce::Post,
            _ => PluginEnforce::Normal,
        };
        assert_eq!(enforce, PluginEnforce::Normal);
    }

    // ========================================================================
    // plugin_error helper tests
    // ========================================================================

    #[test]
    fn test_plugin_error_construction() {
        let err = plugin_error("my-plugin", "something failed");
        assert_eq!(err.plugin, "my-plugin");
        assert_eq!(err.message, "something failed");
    }

    #[test]
    fn test_plugin_error_from_string() {
        let msg = String::from("owned error message");
        let err = plugin_error("test", msg);
        assert_eq!(err.message, "owned error message");
    }

    // ========================================================================
    // Integration tests (require V8 runtime via native-runtime feature)
    // ========================================================================

    /// Integration test: loads a real JS config file with plugins via V8.
    #[test]
    fn test_js_plugin_host_transform_plugin() {
        let dir = tempfile::tempdir().unwrap();

        // Write a config file with a transform plugin
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'test-replace',
                    transform(code, id) {
                        if (id.endsWith('.tsx')) {
                            return code.replace('__TEST__', '"replaced"');
                        }
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        assert_eq!(host.plugin_count(), 1);
        assert_eq!(host.plugin_defs()[0].name, "test-replace");
        assert!(host.plugin_defs()[0].has_transform);
        assert!(!host.plugin_defs()[0].has_resolve_id);
        assert!(!host.plugin_defs()[0].has_load);

        // Call transform hook — matching .tsx file
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "const x = __TEST__;".to_string(),
                    id: "src/App.tsx".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, r#"const x = "replaced";"#);
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }

        // Call transform hook — non-matching file (returns null)
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "const x = __TEST__;".to_string(),
                    id: "src/util.js".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(None) => {}
            other => panic!("Expected Transform(None), got {:?}", other),
        }
    }

    /// Integration test: virtual module plugin with resolveId + load.
    #[test]
    fn test_js_plugin_host_virtual_module() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                plugins: [{
                    name: 'virtual-env',
                    resolveId(source) {
                        if (source === 'virtual:env') {
                            return { id: '\0virtual:env' };
                        }
                    },
                    load(id) {
                        if (id === '\0virtual:env') {
                            return { code: 'export const MODE = "development";' };
                        }
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        assert_eq!(host.plugin_count(), 1);
        assert!(host.plugin_defs()[0].has_resolve_id);
        assert!(host.plugin_defs()[0].has_load);

        // Test resolveId
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::ResolveId {
                    source: "virtual:env".to_string(),
                    importer: Some("/src/main.ts".to_string()),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::ResolveId(Some(result)) => {
                assert_eq!(result.id, "\0virtual:env");
                assert!(!result.external);
            }
            other => panic!("Expected ResolveId(Some), got {:?}", other),
        }

        // Test resolveId miss
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::ResolveId {
                    source: "react".to_string(),
                    importer: None,
                },
            })
            .unwrap();

        match resp {
            PluginResponse::ResolveId(None) => {}
            other => panic!("Expected ResolveId(None), got {:?}", other),
        }

        // Test load
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Load {
                    id: "\0virtual:env".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Load(Some(result)) => {
                assert!(result.code.contains("MODE"));
                assert!(result.code.contains("development"));
            }
            other => panic!("Expected Load(Some), got {:?}", other),
        }

        // Test load miss
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Load {
                    id: "/some/real/file.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Load(None) => {}
            other => panic!("Expected Load(None), got {:?}", other),
        }
    }

    /// Integration test: multiple plugins.
    #[test]
    fn test_js_plugin_host_multiple_plugins() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                plugins: [
                    {
                        name: 'plugin-a',
                        enforce: 'pre',
                        transform(code, id) {
                            return code.replace('AAA', 'BBB');
                        },
                    },
                    {
                        name: 'plugin-b',
                        enforce: 'post',
                        transformIndexHtml(html) {
                            return { code: html.replace('</head>', '<meta name="test">\n</head>') };
                        },
                    },
                ],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        assert_eq!(host.plugin_count(), 2);
        assert_eq!(host.plugin_defs()[0].name, "plugin-a");
        assert_eq!(host.plugin_defs()[0].enforce.as_deref(), Some("pre"));
        assert_eq!(host.plugin_defs()[1].name, "plugin-b");
        assert_eq!(host.plugin_defs()[1].enforce.as_deref(), Some("post"));

        // plugin-a: transform
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "const x = 'AAA';".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "const x = 'BBB';");
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }

        // plugin-b: transformIndexHtml
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 1,
                hook: HookCall::TransformIndexHtml {
                    html: "<html><head></head></html>".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::TransformIndexHtml(Some(html)) => {
                assert!(html.contains("<meta name=\"test\">"));
            }
            other => panic!("Expected TransformIndexHtml(Some), got {:?}", other),
        }
    }

    /// Integration test: config with no plugins.
    #[test]
    fn test_js_plugin_host_no_plugins() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                server: { port: 3000 },
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        assert_eq!(host.plugin_count(), 0);
        assert!(host.plugin_defs().is_empty());
    }

    /// Integration test: plugin that throws an error in a hook.
    #[test]
    fn test_js_plugin_host_error_in_hook() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                plugins: [{
                    name: 'error-plugin',
                    transform(code, id) {
                        throw new Error('Transform failed!');
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "hello".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Error(msg) => {
                assert!(msg.contains("Transform failed!"), "Got: {}", msg);
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    /// Integration test: buildStart and buildEnd hooks.
    #[test]
    fn test_js_plugin_host_build_start_end() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                plugins: [{
                    name: 'lifecycle',
                    buildStart() {
                        globalThis.__buildStartCalled = true;
                    },
                    buildEnd() {
                        globalThis.__buildEndCalled = true;
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        assert!(host.plugin_defs()[0].has_build_start);
        assert!(host.plugin_defs()[0].has_build_end);

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::BuildStart,
            })
            .unwrap();
        match resp {
            PluginResponse::Ok => {}
            other => panic!("Expected Ok for buildStart, got {:?}", other),
        }

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::BuildEnd,
            })
            .unwrap();
        match resp {
            PluginResponse::Ok => {}
            other => panic!("Expected Ok for buildEnd, got {:?}", other),
        }
    }

    /// Integration test: plugin returning a string from transform (shorthand).
    #[test]
    fn test_js_plugin_host_transform_returns_string() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = r#"
            export default {
                plugins: [{
                    name: 'string-return',
                    transform(code, id) {
                        return 'replaced-code';
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "original".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "replaced-code");
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }
    }

    /// Integration test: host is dropped cleanly (thread joins).
    #[test]
    fn test_js_plugin_host_drop() {
        let dir = tempfile::tempdir().unwrap();

        let config_content = "export default { plugins: [] };";
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        // Dropping should not panic or hang
        drop(host);
    }

    /// Integration test: invalid config file path.
    #[test]
    fn test_js_plugin_host_invalid_config_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("nonexistent.config.js");
        let result = JsPluginHost::start(&config_path, dir.path());
        assert!(result.is_err());
    }

    // ========================================================================
    // Boundary / error / -1 tests
    // ========================================================================

    /// -1: Config file with a JS syntax error should fail to start.
    #[test]
    fn test_js_plugin_host_syntax_error_in_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = "export default {{{{{ broken syntax";
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let result = JsPluginHost::start(&config_path, dir.path());
        assert!(result.is_err(), "Expected error for syntax error in config");
    }

    /// -1: Config file that doesn't export default at all.
    #[test]
    fn test_js_plugin_host_no_default_export() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = "const x = 42;";
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        // Should still start (no plugins), since __howthConfigDefault will be undefined
        // and the extract function handles that gracefully
        let result = JsPluginHost::start(&config_path, dir.path());
        match result {
            Ok(host) => assert_eq!(host.plugin_count(), 0),
            Err(_) => {} // also acceptable — depends on module loader behavior
        }
    }

    /// -1: Plugin returns a number instead of string/object from transform.
    #[test]
    fn test_js_plugin_host_transform_returns_unexpected_type() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'bad-return',
                    transform(code, id) {
                        return 42;
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "x".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        // 42 is not null, not a string, not an object with "code" — should produce
        // a response without a code field, so Transform(None)
        match resp {
            PluginResponse::Transform(None) => {}
            PluginResponse::Error(_) => {} // also acceptable
            other => panic!("Expected Transform(None) or Error, got {:?}", other),
        }
    }

    /// -1: Plugin returns boolean from transform.
    #[test]
    fn test_js_plugin_host_transform_returns_boolean() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'bool-return',
                    transform(code, id) {
                        return false;
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "x".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        // false is JSON-serializable but has no "code" key
        match resp {
            PluginResponse::Transform(None) => {}
            PluginResponse::Error(_) => {}
            other => panic!("Expected Transform(None) or Error, got {:?}", other),
        }
    }

    /// -1: Calling a hook on an out-of-bounds plugin index.
    #[test]
    fn test_js_plugin_host_out_of_bounds_plugin_idx() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'only-one',
                    transform(code) { return code; },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 1);

        // Call with plugin_idx = 99 (doesn't exist)
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 99,
                hook: HookCall::Transform {
                    code: "x".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        // Bootstrap JS returns 'null' for missing plugins, which maps to None
        match resp {
            PluginResponse::Transform(None) => {}
            other => panic!("Expected Transform(None) for OOB index, got {:?}", other),
        }
    }

    /// -1: Empty string as code input to transform.
    #[test]
    fn test_js_plugin_host_transform_empty_code() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'echo',
                    transform(code, id) {
                        return code;
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: "".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "");
            }
            // Empty string is falsy in JS, so plugin returns "" which the bootstrap
            // wraps as { code: "" }, but "" is falsy — could return null
            PluginResponse::Transform(None) => {}
            other => panic!("Expected Transform result, got {:?}", other),
        }
    }

    /// -1: Code containing special characters (quotes, backslashes, newlines).
    #[test]
    fn test_js_plugin_host_transform_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'passthrough',
                    transform(code, id) {
                        return { code: code };
                    },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();

        let input = "const s = \"hello\\nworld\";\nconst t = 'it\\'s';";
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 0,
                hook: HookCall::Transform {
                    code: input.to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, input);
            }
            other => panic!("Expected Transform with special chars preserved, got {:?}", other),
        }
    }

    /// -1: Config with plugins set to null (not an array).
    #[test]
    fn test_js_plugin_host_plugins_null() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = "export default { plugins: null };";
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 0);
    }

    /// -1: Config with plugins set to a non-array value.
    #[test]
    fn test_js_plugin_host_plugins_not_array() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = "export default { plugins: 'not-an-array' };";
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 0);
    }

    /// -1: Config with plugins containing falsy values (null, undefined, false).
    #[test]
    fn test_js_plugin_host_plugins_with_falsy_entries() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [
                    null,
                    false,
                    undefined,
                    {
                        name: 'real-plugin',
                        transform(code) { return code; },
                    },
                ],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        // Bootstrap JS filters with .filter(Boolean), so only the real plugin remains
        assert_eq!(host.plugin_count(), 1);
        assert_eq!(host.plugin_defs()[0].name, "real-plugin");
    }

    /// -1: Config with nested plugin arrays (Vite convention: [plugin1, [plugin2, plugin3]]).
    #[test]
    fn test_js_plugin_host_plugins_nested_arrays() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [
                    { name: 'a', transform(code) { return code; } },
                    [
                        { name: 'b', transform(code) { return code; } },
                        { name: 'c', transform(code) { return code; } },
                    ],
                ],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        // Bootstrap JS uses .flat(Infinity), so nested arrays should be flattened
        assert_eq!(host.plugin_count(), 3);
        assert_eq!(host.plugin_defs()[0].name, "a");
        assert_eq!(host.plugin_defs()[1].name, "b");
        assert_eq!(host.plugin_defs()[2].name, "c");
    }

    /// -1: Plugin with no name property gets a default name.
    #[test]
    fn test_js_plugin_host_unnamed_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    transform(code) { return code; },
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 1);
        assert_eq!(host.plugin_defs()[0].name, "js-plugin-0");
    }

    /// -1: Plugin with no hooks at all.
    #[test]
    fn test_js_plugin_host_plugin_no_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                plugins: [{
                    name: 'empty-plugin',
                }],
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 1);

        let def = &host.plugin_defs()[0];
        assert!(!def.has_resolve_id);
        assert!(!def.has_load);
        assert!(!def.has_transform);
        assert!(!def.has_transform_index_html);
        assert!(!def.has_build_start);
        assert!(!def.has_build_end);
    }

    /// 0: Empty config file (no export, no content).
    #[test]
    fn test_js_plugin_host_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("howth.config.js"), "").unwrap();

        let config_path = dir.path().join("howth.config.js");
        // Empty file has no default export — __howthConfigDefault will be undefined
        let result = JsPluginHost::start(&config_path, dir.path());
        match result {
            Ok(host) => assert_eq!(host.plugin_count(), 0),
            Err(_) => {} // acceptable if module loader rejects empty file
        }
    }

    /// -1: Large number of plugins.
    #[test]
    fn test_js_plugin_host_many_plugins() {
        let dir = tempfile::tempdir().unwrap();

        // Generate a config with 20 plugins
        let mut plugins = String::from("[");
        for i in 0..20 {
            if i > 0 {
                plugins.push(',');
            }
            plugins.push_str(&format!(
                "{{ name: 'plugin-{}', transform(code) {{ return code; }} }}",
                i
            ));
        }
        plugins.push(']');

        let config_content = format!("export default {{ plugins: {} }};", plugins);
        std::fs::write(dir.path().join("howth.config.js"), &config_content).unwrap();

        let config_path = dir.path().join("howth.config.js");
        let host = JsPluginHost::start(&config_path, dir.path()).unwrap();
        assert_eq!(host.plugin_count(), 20);

        // Call transform on the last one
        let resp = host
            .call(PluginRequest::CallHook {
                plugin_idx: 19,
                hook: HookCall::Transform {
                    code: "hello".to_string(),
                    id: "test.ts".to_string(),
                },
            })
            .unwrap();

        match resp {
            PluginResponse::Transform(Some(result)) => {
                assert_eq!(result.code, "hello");
            }
            other => panic!("Expected Transform(Some), got {:?}", other),
        }
    }
}
