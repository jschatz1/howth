//! `howth dev` command implementation.
//!
//! Vite-compatible development server with unbundled ES module serving and HMR.
//!
//! ## Architecture
//!
//! Instead of bundling everything into a single `bundle.js`, the dev server
//! serves individual ES modules on demand:
//!
//! ```text
//! Browser requests GET /src/App.tsx
//!   → resolve (plugin hooks + file system)
//!   → load (plugin hooks + file system)
//!   → transpile (SWC: TSX → JS)
//!   → transform (plugin hooks, e.g., React Refresh)
//!   → rewrite imports (bare → /@modules/, relative → absolute)
//!   → serve as application/javascript
//! ```
//!
//! Dependencies from `node_modules` are pre-bundled on startup into `.howth/deps/`
//! and served at `/@modules/{pkg}` URLs.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, RawQuery, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use fastnode_core::bundler::{
    plugins::ReactRefreshPlugin, AliasPlugin, BundleFormat, BundleOptions, Bundler, DevConfig,
    PluginContainer, ReplacePlugin,
};
use fastnode_core::dev::{
    client_env_replacements, extract_import_urls, is_self_accepting_module, load_config,
    load_env_files, HmrEngine, ModuleTransformer, PreBundler,
};
use miette::{IntoDiagnostic, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

/// Dev server action.
#[derive(Debug, Clone)]
pub struct DevAction {
    /// Entry point file.
    pub entry: PathBuf,
    /// Working directory.
    pub cwd: PathBuf,
    /// Port to listen on.
    pub port: u16,
    /// Host to bind to.
    pub host: String,
    /// Open browser automatically.
    pub open: bool,
    /// Explicit config file path (overrides auto-discovery).
    pub config: Option<PathBuf>,
    /// Mode (e.g. "development", "production").
    pub mode: String,
}

/// Shared server state for Vite-compatible unbundled serving.
struct DevState {
    /// Broadcast channel for HMR updates.
    hmr_tx: broadcast::Sender<HmrMessage>,
    /// Entry point path (relative to cwd).
    entry: PathBuf,
    /// Working directory (project root).
    cwd: PathBuf,
    /// Port (for HMR client).
    port: u16,
    /// Module transformer (per-request resolve → load → transpile → transform → rewrite).
    transformer: ModuleTransformer,
    /// Pre-bundled dependencies (/@modules/ serving).
    prebundler: PreBundler,
    /// Plugin container (shared across requests).
    plugins: PluginContainer,
    /// HMR engine (module graph + boundary detection).
    hmr_engine: HmrEngine,
    /// Fallback: bundled code for legacy mode.
    bundler: Bundler,
    /// Bundle options for fallback.
    bundle_options: BundleOptions,
}

/// HMR message types.
#[derive(Debug, Clone)]
enum HmrMessage {
    /// Full page reload.
    Reload,
    /// Partial module update (Vite-compatible).
    Update { updates: Vec<HmrModuleUpdate> },
    /// Build error.
    Error { message: String },
    /// Connected confirmation.
    Connected,
}

/// A single module update in an HMR message.
#[derive(Debug, Clone)]
struct HmrModuleUpdate {
    /// Module URL path.
    module: String,
    /// Update timestamp.
    timestamp: u64,
}

impl HmrMessage {
    fn to_json(&self) -> String {
        match self {
            HmrMessage::Connected => r#"{"type":"connected"}"#.to_string(),
            HmrMessage::Reload => r#"{"type":"reload"}"#.to_string(),
            HmrMessage::Update { updates } => {
                let update_json: Vec<String> = updates
                    .iter()
                    .map(|u| {
                        format!(
                            r#"{{"module":"{}","timestamp":{}}}"#,
                            u.module.replace('"', "\\\""),
                            u.timestamp
                        )
                    })
                    .collect();
                format!(
                    r#"{{"type":"update","updates":[{}]}}"#,
                    update_json.join(",")
                )
            }
            HmrMessage::Error { message } => {
                format!(
                    r#"{{"type":"error","message":"{}"}}"#,
                    message.replace('"', "\\\"")
                )
            }
        }
    }
}

/// Run the dev server.
pub async fn run(action: DevAction) -> Result<()> {
    let cwd = action.cwd.canonicalize().into_diagnostic()?;

    // Load config file (howth.config.ts, vite.config.ts, etc.)
    #[allow(unused_variables)]
    let (howth_config, config_file_path) = match load_config(&cwd, action.config.as_deref()) {
        Ok(Some((config_path, config))) => {
            let rel_path = config_path.strip_prefix(&cwd).unwrap_or(&config_path);
            println!("  Loaded config from {}", rel_path.display());
            (Some(config), Some(config_path))
        }
        Ok(None) => (None, None),
        Err(e) => {
            eprintln!("  Warning: Failed to load config: {}", e);
            (None, None)
        }
    };

    // Determine effective settings (CLI flags override config file)
    let effective_port = if action.port != 3000 {
        // CLI explicitly set (non-default) — CLI wins
        action.port
    } else if let Some(ref cfg) = howth_config {
        cfg.server.port.unwrap_or(action.port)
    } else {
        action.port
    };

    let effective_host = if action.host != "localhost" {
        action.host.clone()
    } else if let Some(ref cfg) = howth_config {
        cfg.server
            .host
            .clone()
            .unwrap_or_else(|| action.host.clone())
    } else {
        action.host.clone()
    };

    let effective_open = if action.open {
        true
    } else if let Some(ref cfg) = howth_config {
        cfg.server.open.unwrap_or(false)
    } else {
        action.open
    };

    // Load .env files
    let mode = &action.mode;
    let dot_env = load_env_files(&cwd, mode);
    let env_replacements = client_env_replacements(&dot_env, mode);
    let env_var_count = dot_env
        .iter()
        .filter(|(k, _)| k.starts_with("VITE_") || k.starts_with("HOWTH_"))
        .count();
    if !dot_env.is_empty() {
        println!(
            "  Loaded {} env var{} ({} exposed to client)",
            dot_env.len(),
            if dot_env.len() == 1 { "" } else { "s" },
            env_var_count,
        );
    }

    // Initialize plugin system
    let mut plugins = PluginContainer::new(cwd.clone());
    plugins.set_watch(true);

    // Add React Refresh plugin by default
    plugins.add(Box::new(ReactRefreshPlugin::new()));

    // Build alias map: tsconfig.json paths (lower priority) + config file aliases (higher priority)
    {
        let mut all_aliases: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Load tsconfig.json / jsconfig.json paths first (lower priority)
        if let Some(tsconfig_aliases) = fastnode_core::dev::config::load_tsconfig_paths(&cwd) {
            for (key, value) in tsconfig_aliases {
                all_aliases.insert(key, value);
            }
            println!("  Loaded tsconfig.json path aliases");
        }

        // Config file aliases override tsconfig paths
        if let Some(ref cfg) = howth_config {
            for (from, to) in &cfg.resolve.alias {
                all_aliases.insert(from.clone(), to.clone());
            }
        }

        if !all_aliases.is_empty() {
            let mut alias_plugin = AliasPlugin::new();
            for (from, to) in &all_aliases {
                alias_plugin = alias_plugin.alias(from, to);
            }
            plugins.add(Box::new(alias_plugin));
        }
    }

    // Merge .env replacements and config define into a single ReplacePlugin
    {
        let mut replace_plugin = ReplacePlugin::new();

        // .env replacements first (config define can override)
        for (from, to) in &env_replacements {
            replace_plugin = replace_plugin.replace(from, to);
        }

        // Config define replacements override .env
        if let Some(ref cfg) = howth_config {
            for (from, to) in &cfg.define {
                replace_plugin = replace_plugin.replace(from, to);
            }
        }

        if !env_replacements.is_empty()
            || howth_config.as_ref().is_some_and(|c| !c.define.is_empty())
        {
            plugins.add(Box::new(replace_plugin));
        }
    }

    // Load JS plugins from config (requires native-runtime feature)
    #[cfg(feature = "native-runtime")]
    let _js_plugin_host = if howth_config.as_ref().map_or(false, |c| c.has_js_plugins) {
        if let Some(ref cfg_path) = config_file_path {
            match super::js_plugin::JsPluginHost::start(cfg_path, &cwd) {
                Ok(host) => {
                    let count = host.plugin_count();
                    let host = Arc::new(host);
                    for (idx, def) in host.plugin_defs().iter().enumerate() {
                        plugins.add(Box::new(super::js_plugin::JsPlugin::new(
                            def,
                            idx,
                            Arc::clone(&host),
                        )));
                    }
                    println!("  Loaded {} JS plugin(s)", count);
                    Some(host)
                }
                Err(e) => {
                    eprintln!("  Warning: Failed to load JS plugins: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Run config hooks
    let mut dev_config = DevConfig {
        root: cwd.clone(),
        port: effective_port,
        host: effective_host.clone(),
        base: howth_config
            .as_ref()
            .and_then(|c| c.base.clone())
            .unwrap_or_else(|| "/".to_string()),
        ..Default::default()
    };

    // Merge define from config into DevConfig
    if let Some(ref cfg) = howth_config {
        for (key, value) in &cfg.define {
            dev_config.define.insert(key.clone(), value.clone());
        }
    }

    let _ = plugins.call_config(&mut dev_config);
    let _ = plugins.call_config_resolved(&dev_config);

    // Initialize module transformer
    let transformer = ModuleTransformer::new(cwd.clone());

    // Pre-bundle dependencies
    println!("  Scanning dependencies...");
    let mut prebundler = PreBundler::new(cwd.clone());
    let entry_path = if action.entry.is_absolute() {
        action.entry.clone()
    } else {
        cwd.join(&action.entry)
    };

    let bare_imports = prebundler.scan_file_recursive(&entry_path);
    if !bare_imports.is_empty() {
        println!(
            "  Pre-bundling {} dependencies: {}",
            bare_imports.len(),
            bare_imports
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Err(e) = prebundler.bundle_deps(&bare_imports) {
            eprintln!("  Warning: Pre-bundling failed: {}", e);
        }
    }

    // Initialize HMR engine
    let hmr_engine = HmrEngine::new();

    // Initialize bundler (fallback for legacy mode)
    let bundler = Bundler::new();
    let bundle_options = BundleOptions {
        format: BundleFormat::Esm,
        treeshake: false,
        ..Default::default()
    };

    // Sort plugins by enforce order
    plugins
        .context_mut()
        .set_meta("mode", "development".to_string());

    // Create broadcast channel for HMR
    let (hmr_tx, _) = broadcast::channel::<HmrMessage>(16);

    // Compute entry URL path (relative to cwd)
    let entry_url = if let Ok(rel) = entry_path.strip_prefix(&cwd) {
        format!("/{}", rel.display())
    } else {
        format!("/{}", action.entry.display())
    };

    // Create shared state
    let state = Arc::new(DevState {
        hmr_tx: hmr_tx.clone(),
        entry: action.entry.clone(),
        cwd: cwd.clone(),
        port: effective_port,
        transformer,
        prebundler,
        plugins,
        hmr_engine,
        bundler,
        bundle_options,
    });

    // Set up file watcher
    let (file_change_tx, mut file_change_rx) = mpsc::channel::<Vec<String>>(16);
    let watch_cwd = cwd.clone();

    std::thread::spawn(move || {
        if let Err(e) = watch_files(watch_cwd, file_change_tx) {
            eprintln!("  File watcher error: {}", e);
        }
    });

    // Spawn file change handler
    let change_state = state.clone();
    tokio::spawn(async move {
        while let Some(changed) = file_change_rx.recv().await {
            handle_file_change(&change_state, changed).await;
        }
    });

    // Load index.html: prefer user's file, fall back to generated template
    let user_index_path = cwd.join("index.html");
    let index_html = if user_index_path.exists() {
        let mut html = std::fs::read_to_string(&user_index_path)
            .unwrap_or_else(|_| generate_index_html(&entry_url, action.port));
        // Inject HMR client script before </head> or </body>
        let hmr_script = r#"<script type="module" src="/@hmr-client"></script>"#;
        if !html.contains("/@hmr-client") {
            if let Some(pos) = html.find("</head>") {
                html.insert_str(pos, &format!("  {}\n  ", hmr_script));
            } else if let Some(pos) = html.find("</body>") {
                html.insert_str(pos, &format!("  {}\n  ", hmr_script));
            } else {
                html.push_str(&format!("\n{}", hmr_script));
            }
        }
        html
    } else {
        generate_index_html(&entry_url, action.port)
    };

    // Apply transform_index_html plugin hook
    let index_html = match state.plugins.call_transform_index_html(&index_html) {
        Ok(html) => html,
        Err(_) => index_html,
    };

    let index_html: &'static str = Box::leak(index_html.into_boxed_str());

    // Create router
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/__hmr", get(hmr_websocket))
        .route("/@hmr-client", get(serve_hmr_client))
        .route("/@react-refresh", get(serve_react_refresh))
        .route("/@modules/*pkg", get(serve_prebundled_dep))
        .route("/@style/*path", get(serve_css_module))
        .route("/*path", get(serve_module))
        .with_state((state, index_html));

    // Start server
    let host_ip = if effective_host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        effective_host.clone()
    };

    let addr: SocketAddr = format!("{}:{}", host_ip, effective_port)
        .parse()
        .into_diagnostic()?;

    println!();
    println!(
        "  Dev server running at http://localhost:{}",
        effective_port
    );
    println!("  Vite-compatible unbundled serving enabled");
    println!("  Hot Module Replacement enabled");
    println!();
    println!("  Press Ctrl+C to stop");
    println!();

    // Open browser if requested
    if effective_open {
        let url = format!("http://{}:{}", effective_host, effective_port);
        let _ = open_browser(&url);
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .into_diagnostic()?;
    axum::serve(listener, app).await.into_diagnostic()?;

    Ok(())
}

// ============================================================================
// Route Handlers
// ============================================================================

type AppState = (Arc<DevState>, &'static str);

/// Serve the index HTML page.
async fn serve_index(State((_state, index_html)): State<AppState>) -> Html<&'static str> {
    Html(index_html)
}

/// Serve the HMR client runtime at `/@hmr-client`.
async fn serve_hmr_client(State((state, _)): State<AppState>) -> impl IntoResponse {
    let runtime = HmrEngine::client_runtime(state.port);
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/javascript")
        .header("Cache-Control", "no-cache")
        .body(runtime)
        .unwrap()
}

/// Serve the React Refresh runtime at `/@react-refresh`.
async fn serve_react_refresh(State((state, _)): State<AppState>) -> impl IntoResponse {
    match state.plugins.load("\0react-refresh") {
        Ok(Some(result)) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript")
            .header("Cache-Control", "no-cache")
            .body(result.code)
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body("// React Refresh runtime not available".to_string())
            .unwrap(),
    }
}

/// Serve a pre-bundled dependency at `/@modules/{pkg}`.
async fn serve_prebundled_dep(
    State((state, _)): State<AppState>,
    AxumPath(pkg): AxumPath<String>,
) -> impl IntoResponse {
    if let Some(dep) = state.prebundler.get(&pkg) {
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/javascript")
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(dep.code.clone())
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(format!("// Module not found: {}", pkg))
            .unwrap()
    }
}

/// Serve a CSS file as a JS module at `/@style/{path}`.
async fn serve_css_module(
    State((state, _)): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> impl IntoResponse {
    let url_path = format!("/@style/{}", path);
    match state
        .transformer
        .transform_module(&url_path, &state.plugins)
    {
        Ok(module) => {
            // Register CSS module in HMR graph (CSS modules are self-accepting)
            let file_path = state.cwd.join(&path).display().to_string();
            state
                .hmr_engine
                .module_graph
                .ensure_module(&url_path, &file_path);
            state.hmr_engine.module_graph.mark_self_accepting(&url_path);

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", module.content_type)
                .header("Cache-Control", "no-cache")
                .body(module.code)
                .unwrap()
        }
        Err(e) => {
            // Return 404 for missing files, 500 for other errors
            let status =
                if e.message.contains("not found") || e.message.contains("Module not found") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
            Response::builder()
                .status(status)
                .header("Content-Type", "application/javascript")
                .body(format!(
                    "console.error('CSS load error: {}');",
                    e.message.replace('\'', "\\'")
                ))
                .unwrap()
        }
    }
}

/// Serve an individual module on demand.
///
/// This is the core of the unbundled dev server: each request triggers
/// the full transform pipeline (resolve → load → transpile → transform → rewrite).
///
/// Also handles SPA fallback: non-file routes (no extension) return index.html
/// so client-side routing (React Router, Vue Router, etc.) works on refresh.
async fn serve_module(
    State((state, index_html)): State<AppState>,
    AxumPath(path): AxumPath<String>,
    RawQuery(query): RawQuery,
) -> impl IntoResponse {
    let url_path = format!("/{}", path);

    // Check for ?import query (asset imports from JS)
    // Note: AxumPath does NOT include query parameters, so we use RawQuery
    let is_asset_import = query.as_deref().is_some_and(|q| q.contains("import"));

    // Strip query parameters from path (e.g., ?t=1234 for cache busting)
    let url_path = url_path.split('?').next().unwrap_or(&url_path);

    // Check if this is a JS/TS module request
    let ext = url_path.rsplit('.').next().unwrap_or("");

    match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "json" => {
            match state.transformer.transform_module(url_path, &state.plugins) {
                Ok(module) => {
                    // Register in HMR module graph
                    state
                        .hmr_engine
                        .module_graph
                        .ensure_module(url_path, &module.file_path);

                    // Extract import URLs from the transformed code and populate graph edges
                    let import_urls = extract_import_urls(&module.code);
                    // Ensure imported modules exist in the graph before updating edges
                    for import_url in &import_urls {
                        // Resolve the import to a file path for the graph
                        let import_file = if import_url.starts_with("/@style/") {
                            let rel = import_url.strip_prefix("/@style").unwrap_or(import_url);
                            state
                                .cwd
                                .join(rel.strip_prefix('/').unwrap_or(rel))
                                .display()
                                .to_string()
                        } else {
                            state
                                .cwd
                                .join(import_url.strip_prefix('/').unwrap_or(import_url))
                                .display()
                                .to_string()
                        };
                        state
                            .hmr_engine
                            .module_graph
                            .ensure_module(import_url, &import_file);
                    }
                    state
                        .hmr_engine
                        .module_graph
                        .update_module_imports(url_path, &import_urls);

                    // Detect self-accepting modules (import.meta.hot.accept())
                    if is_self_accepting_module(&module.code) {
                        state.hmr_engine.module_graph.mark_self_accepting(url_path);
                    }

                    // Inject HMR preamble for JS modules
                    let code = if ext == "json" {
                        module.code
                    } else {
                        let preamble = HmrEngine::module_preamble(url_path);
                        format!("{}\n{}", preamble, module.code)
                    };

                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", module.content_type)
                        .header("Cache-Control", "no-cache")
                        .body(code)
                        .unwrap()
                }
                Err(e) => {
                    // Return 404 for missing files, 500 for other errors
                    let status = if e.message.contains("not found")
                        || e.message.contains("Module not found")
                    {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    };
                    Response::builder()
                        .status(status)
                        .header("Content-Type", "application/javascript")
                        .body(format!(
                            "console.error('Transform error: {}');",
                            e.message.replace('\'', "\\'")
                        ))
                        .unwrap()
                }
            }
        }
        "css" => {
            // Serve raw CSS for <link> tags
            let file_path = state.cwd.join(&path);
            match std::fs::read_to_string(&file_path) {
                Ok(css) => Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/css")
                    .header("Cache-Control", "no-cache")
                    .body(css)
                    .unwrap(),
                Err(_) => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(format!("/* CSS not found: {} */", path))
                    .unwrap(),
            }
        }
        _ if is_asset_import => {
            // Asset import from JS: return a JS module that exports the URL
            let asset_url = url_path.to_string();
            let js_module = format!("export default \"{}\";\n", asset_url);
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/javascript")
                .header("Cache-Control", "no-cache")
                .body(js_module)
                .unwrap()
        }
        _ => {
            // Static file serving
            let file_path = state.cwd.join(&path);
            if file_path.exists() {
                let content_type = match ext {
                    "html" => "text/html",
                    "svg" => "image/svg+xml",
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "ico" => "image/x-icon",
                    "woff" => "font/woff",
                    "woff2" => "font/woff2",
                    "ttf" => "font/ttf",
                    "wasm" => "application/wasm",
                    _ => "application/octet-stream",
                };
                match std::fs::read(&file_path) {
                    Ok(bytes) => Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", content_type)
                        .body(String::from_utf8_lossy(&bytes).to_string())
                        .unwrap(),
                    Err(_) => Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(format!("Not found: {}", path))
                        .unwrap(),
                }
            } else if ext.is_empty() {
                // SPA fallback: no file extension means this is likely a client-side
                // route (e.g., /about, /users/123). Return index.html so the app's
                // router can handle it.
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/html")
                    .body(index_html.to_string())
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(format!("Not found: {}", path))
                    .unwrap()
            }
        }
    }
}

// ============================================================================
// WebSocket HMR
// ============================================================================

/// Handle WebSocket connections for HMR.
async fn hmr_websocket(
    ws: WebSocketUpgrade,
    State((state, _)): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_hmr_socket(socket, state))
}

/// Handle an HMR WebSocket connection.
async fn handle_hmr_socket(mut socket: WebSocket, state: Arc<DevState>) {
    let mut rx = state.hmr_tx.subscribe();

    // Send connected message
    let _ = socket
        .send(Message::Text(HmrMessage::Connected.to_json()))
        .await;

    // Bidirectional: forward server→client HMR messages, handle client→server messages
    loop {
        tokio::select! {
            // Server → Client: forward HMR updates
            Ok(msg) = rx.recv() => {
                let json = msg.to_json();
                if socket.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            // Client → Server: handle hotAccept, invalidate, etc.
            Some(Ok(msg)) = socket.recv() => {
                if let Message::Text(text) = msg {
                    handle_client_hmr_message(&state, &text);
                }
            }
            else => break,
        }
    }
}

/// Handle an incoming HMR message from the client.
fn handle_client_hmr_message(state: &DevState, text: &str) {
    // Parse JSON manually (avoid serde dependency for a few message types)
    if let Some(path) = extract_json_string(text, "path") {
        if text.contains("\"hotAccept\"") {
            // Client confirmed this module is self-accepting
            state.hmr_engine.module_graph.mark_self_accepting(&path);
        }
        // "invalidate" is handled client-side (reload), no server action needed
    }
}

/// Extract a string value for a key from a simple JSON object.
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    // Skip `:` and whitespace
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Extract string value
    let quote = after_colon.chars().next()?;
    if quote != '"' {
        return None;
    }
    let inner = &after_colon[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

// ============================================================================
// File Watching
// ============================================================================

/// Check if a path should be ignored by the file watcher.
fn should_ignore(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();

    if path_str.contains("/node_modules/")
        || path_str.contains("/target/")
        || path_str.contains("/.git/")
        || path_str.contains("/dist/")
        || path_str.contains("/.next/")
        || path_str.contains("/build/")
        || path_str.contains("/.howth/")
        || path_str.contains("/__pycache__/")
    {
        return true;
    }

    if let Some(name) = path.file_name() {
        if name.to_string_lossy().starts_with('.') {
            return true;
        }
    }

    false
}

/// Watch files for changes.
fn watch_files(cwd: PathBuf, file_change_tx: mpsc::Sender<Vec<String>>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default()).into_diagnostic()?;
    watcher
        .watch(&cwd, RecursiveMode::Recursive)
        .into_diagnostic()?;

    let mut debounce_set: HashSet<PathBuf> = HashSet::new();
    let mut last_change = std::time::Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                let relevant = event.paths.iter().any(|p| {
                    if should_ignore(p) {
                        return false;
                    }
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    matches!(
                        ext,
                        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "css" | "json" | "html"
                    )
                });

                if !relevant {
                    continue;
                }

                for path in event.paths {
                    if !should_ignore(&path) {
                        debounce_set.insert(path);
                    }
                }

                let now = std::time::Instant::now();
                if now.duration_since(last_change).as_millis() < 50 {
                    continue;
                }

                if debounce_set.is_empty() {
                    continue;
                }

                let changed: Vec<String> = debounce_set
                    .drain()
                    .map(|p| p.display().to_string())
                    .collect();

                last_change = now;

                if file_change_tx.blocking_send(changed).is_err() {
                    break;
                }
            }
            Ok(Err(e)) => {
                eprintln!("  Watch error: {}", e);
            }
            Err(_) => break,
        }
    }

    Ok(())
}

/// Handle file changes: invalidate cache and send HMR updates.
async fn handle_file_change(state: &DevState, changed: Vec<String>) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    for file_path in &changed {
        println!(
            "  File changed: {}",
            file_path
                .strip_prefix(&state.cwd.display().to_string())
                .unwrap_or(file_path)
        );

        // Invalidate transform cache
        state.transformer.invalidate(file_path);
    }

    // Determine HMR updates
    let mut updates = Vec::new();
    let mut needs_full_reload = false;

    for file_path in &changed {
        // Check plugin handle_hot_update hook
        let hot_ctx = fastnode_core::bundler::HotUpdateContext {
            file: file_path.clone(),
            timestamp,
            modules: vec![],
        };

        if let Ok(Some(_modules)) = state.plugins.call_handle_hot_update(&hot_ctx) {
            // Plugin handled the update
            continue;
        }

        // Use HMR engine to find boundaries
        match state.hmr_engine.on_file_change(file_path) {
            fastnode_core::dev::hmr::HmrUpdateResult::Updates(hmr_updates) => {
                for update in hmr_updates {
                    updates.push(HmrModuleUpdate {
                        module: update.module_url,
                        timestamp: update.timestamp,
                    });
                }
            }
            fastnode_core::dev::hmr::HmrUpdateResult::FullReload => {
                needs_full_reload = true;
            }
        }
    }

    // Send HMR message
    if needs_full_reload || updates.is_empty() {
        let _ = state.hmr_tx.send(HmrMessage::Reload);
    } else {
        let _ = state.hmr_tx.send(HmrMessage::Update { updates });
    }
}

// ============================================================================
// Utilities
// ============================================================================

/// Generate a fallback index HTML when the project has no index.html.
fn generate_index_html(entry_url: &str, _port: u16) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>howth dev</title>
  <script type="module" src="/@hmr-client"></script>
  <style>
    body {{ margin: 0; font-family: system-ui, sans-serif; }}
    #root {{ }}
  </style>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="{entry_url}"></script>
</body>
</html>"#,
        entry_url = entry_url
    )
}

/// Open a URL in the default browser.
fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()?;
    }
    Ok(())
}
