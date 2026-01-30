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
        Path as AxumPath, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use fastnode_core::bundler::{
    BundleFormat, BundleOptions, Bundler, DevConfig, PluginContainer,
    plugins::ReactRefreshPlugin,
};
use fastnode_core::dev::{HmrEngine, ModuleTransformer, PreBundler};
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
    Update {
        updates: Vec<HmrModuleUpdate>,
    },
    /// Build error.
    Error {
        message: String,
    },
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

    // Initialize plugin system
    let mut plugins = PluginContainer::new(cwd.clone());
    plugins.set_watch(true);

    // Add React Refresh plugin by default
    plugins.add(Box::new(ReactRefreshPlugin::new()));

    // Run config hooks
    let mut dev_config = DevConfig {
        root: cwd.clone(),
        port: action.port,
        host: action.host.clone(),
        ..Default::default()
    };
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
    plugins.context_mut().set_meta("mode", "development".to_string());

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
        port: action.port,
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

    // Build entry URL into index HTML
    let index_html = generate_index_html(&entry_url, action.port);

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
        .route("/@modules/{*pkg}", get(serve_prebundled_dep))
        .route("/@style/{*path}", get(serve_css_module))
        .route("/{*path}", get(serve_module))
        .with_state((state, index_html));

    // Start server
    let host_ip = if action.host == "localhost" {
        "127.0.0.1".to_string()
    } else {
        action.host.clone()
    };

    let addr: SocketAddr = format!("{}:{}", host_ip, action.port)
        .parse()
        .into_diagnostic()?;

    println!();
    println!("  Dev server running at http://localhost:{}", action.port);
    println!("  Vite-compatible unbundled serving enabled");
    println!("  Hot Module Replacement enabled");
    println!();
    println!("  Press Ctrl+C to stop");
    println!();

    // Open browser if requested
    if action.open {
        let url = format!("http://{}:{}", action.host, action.port);
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
    match state.transformer.transform_module(&url_path, &state.plugins) {
        Ok(module) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", module.content_type)
            .header("Cache-Control", "no-cache")
            .body(module.code)
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("Content-Type", "application/javascript")
            .body(format!(
                "console.error('CSS load error: {}');",
                e.message.replace('\'', "\\'")
            ))
            .unwrap(),
    }
}

/// Serve an individual module on demand.
///
/// This is the core of the unbundled dev server: each request triggers
/// the full transform pipeline (resolve → load → transpile → transform → rewrite).
async fn serve_module(
    State((state, _)): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> impl IntoResponse {
    let url_path = format!("/{}", path);

    // Strip query parameters (e.g., ?t=1234 for cache busting)
    let url_path = url_path.split('?').next().unwrap_or(&url_path);

    // Check if this is a JS/TS module request
    let ext = url_path
        .rsplit('.')
        .next()
        .unwrap_or("");

    match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "json" => {
            match state.transformer.transform_module(url_path, &state.plugins) {
                Ok(module) => {
                    // Register in HMR module graph
                    state
                        .hmr_engine
                        .module_graph
                        .ensure_module(url_path, &module.file_path);

                    // Inject HMR preamble for JS modules
                    let code = if ext != "json" {
                        let preamble = HmrEngine::module_preamble(url_path);
                        format!("{}\n{}", preamble, module.code)
                    } else {
                        module.code
                    };

                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", module.content_type)
                        .header("Cache-Control", "no-cache")
                        .body(code)
                        .unwrap()
                }
                Err(e) => Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header("Content-Type", "application/javascript")
                    .body(format!(
                        "console.error('Transform error: {}');",
                        e.message.replace('\'', "\\'")
                    ))
                    .unwrap(),
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

    // Forward HMR messages to client
    while let Ok(msg) = rx.recv().await {
        let json = msg.to_json();
        if socket.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
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

/// Generate the index HTML for unbundled module serving.
fn generate_index_html(entry_url: &str, _port: u16) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>howth dev</title>
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
