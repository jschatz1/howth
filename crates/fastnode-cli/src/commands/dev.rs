//! `howth dev` command implementation.
//!
//! Development server with hot module replacement (HMR).

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use fastnode_core::bundler::{BundleFormat, BundleOptions, Bundler};
use miette::{IntoDiagnostic, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

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

/// Shared server state.
struct DevState {
    /// Current bundled code.
    bundle: RwLock<String>,
    /// Broadcast channel for HMR updates.
    hmr_tx: broadcast::Sender<HmrMessage>,
    /// Entry point path.
    entry: PathBuf,
    /// Working directory.
    cwd: PathBuf,
    /// Bundler instance.
    bundler: Bundler,
    /// Bundle options.
    options: BundleOptions,
}

/// HMR message types.
#[derive(Debug, Clone)]
enum HmrMessage {
    /// Full page reload.
    Reload,
    /// Module update (future: partial HMR).
    Update { modules: Vec<String> },
    /// Build error.
    Error { message: String },
}

impl HmrMessage {
    fn to_json(&self) -> String {
        match self {
            HmrMessage::Reload => r#"{"type":"reload"}"#.to_string(),
            HmrMessage::Update { modules } => {
                let mods = modules
                    .iter()
                    .map(|m| format!("\"{}\"", m))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(r#"{{"type":"update","modules":[{}]}}"#, mods)
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
    let bundler = Bundler::new();
    let options = BundleOptions {
        format: BundleFormat::Esm,
        treeshake: false, // Disable for faster dev builds
        ..Default::default()
    };

    // Initial bundle
    println!("  Building {}...", action.entry.display());
    let initial_bundle = match bundler.bundle(&action.entry, &action.cwd, &options) {
        Ok(result) => inject_hmr_runtime(&result.code),
        Err(e) => {
            eprintln!("  Build error: {}", e);
            format!(
                "console.error('Build error: {}');",
                e.message.replace('\'', "\\'")
            )
        }
    };

    // Create broadcast channel for HMR
    let (hmr_tx, _) = broadcast::channel::<HmrMessage>(16);

    // Create shared state
    let state = Arc::new(DevState {
        bundle: RwLock::new(initial_bundle),
        hmr_tx: hmr_tx.clone(),
        entry: action.entry.clone(),
        cwd: action.cwd.clone(),
        bundler,
        options,
    });

    // Set up file watcher with channel for rebuild events
    let (rebuild_tx, mut rebuild_rx) = mpsc::channel::<Vec<String>>(16);
    let watch_cwd = action.cwd.clone();

    std::thread::spawn(move || {
        if let Err(e) = watch_files(watch_cwd, rebuild_tx) {
            eprintln!("  File watcher error: {}", e);
        }
    });

    // Spawn rebuild handler
    let rebuild_state = state.clone();
    tokio::spawn(async move {
        while let Some(changed) = rebuild_rx.recv().await {
            rebuild(&rebuild_state, changed).await;
        }
    });

    // Create router
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/bundle.js", get(serve_bundle))
        .route("/__hmr", get(hmr_websocket))
        .with_state(state);

    // Start server - resolve hostname to IP
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

/// Serve the index HTML page.
async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// Serve the bundled JavaScript.
async fn serve_bundle(State(state): State<Arc<DevState>>) -> impl IntoResponse {
    let bundle = state.bundle.read().await;
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/javascript")
        .header("Cache-Control", "no-cache")
        .body(bundle.clone())
        .unwrap()
}

/// Handle WebSocket connections for HMR.
async fn hmr_websocket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DevState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_hmr_socket(socket, state))
}

/// Handle an HMR WebSocket connection.
async fn handle_hmr_socket(mut socket: WebSocket, state: Arc<DevState>) {
    let mut rx = state.hmr_tx.subscribe();

    // Send connected message
    let _ = socket
        .send(Message::Text(r#"{"type":"connected"}"#.to_string()))
        .await;

    // Forward HMR messages to client
    while let Ok(msg) = rx.recv().await {
        let json = msg.to_json();
        if socket.send(Message::Text(json)).await.is_err() {
            break;
        }
    }
}

/// Check if a path should be ignored by the file watcher.
fn should_ignore(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();

    // Ignore common directories
    if path_str.contains("/node_modules/")
        || path_str.contains("/target/")
        || path_str.contains("/.git/")
        || path_str.contains("/dist/")
        || path_str.contains("/.next/")
        || path_str.contains("/build/")
        || path_str.contains("/__pycache__/")
    {
        return true;
    }

    // Ignore hidden files
    if let Some(name) = path.file_name() {
        if name.to_string_lossy().starts_with('.') {
            return true;
        }
    }

    false
}

/// Watch files for changes and send rebuild events through channel.
fn watch_files(cwd: PathBuf, rebuild_tx: mpsc::Sender<Vec<String>>) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default()).into_diagnostic()?;
    watcher
        .watch(&cwd, RecursiveMode::Recursive)
        .into_diagnostic()?;

    let mut debounce_set: HashSet<PathBuf> = HashSet::new();
    let mut last_rebuild = std::time::Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                // Filter for relevant file changes
                let dominated = event.paths.iter().any(|p| {
                    // Skip ignored paths
                    if should_ignore(p) {
                        return false;
                    }

                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
                });

                if !dominated {
                    continue;
                }

                // Debounce: collect changes for 50ms
                for path in event.paths {
                    if !should_ignore(&path) {
                        debounce_set.insert(path);
                    }
                }

                let now = std::time::Instant::now();
                if now.duration_since(last_rebuild).as_millis() < 50 {
                    continue;
                }

                if debounce_set.is_empty() {
                    continue;
                }

                // Rebuild
                let changed: Vec<String> = debounce_set
                    .drain()
                    .map(|p| p.display().to_string())
                    .collect();

                last_rebuild = now;

                println!(
                    "  File changed: {}",
                    changed.first().unwrap_or(&"unknown".to_string())
                );

                // Send rebuild event through channel
                if rebuild_tx.blocking_send(changed).is_err() {
                    break; // Channel closed
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

/// Rebuild the bundle and notify clients.
async fn rebuild(state: &DevState, changed: Vec<String>) {
    let start = std::time::Instant::now();

    match state
        .bundler
        .bundle(&state.entry, &state.cwd, &state.options)
    {
        Ok(result) => {
            let code = inject_hmr_runtime(&result.code);
            *state.bundle.write().await = code;

            let duration = start.elapsed().as_millis();
            println!("  Rebuilt in {}ms", duration);

            // Send reload signal
            let _ = state.hmr_tx.send(HmrMessage::Update { modules: changed });
        }
        Err(e) => {
            eprintln!("  Build error: {}", e);
            let _ = state.hmr_tx.send(HmrMessage::Error {
                message: e.message.clone(),
            });
        }
    }
}

/// Inject HMR runtime into the bundle.
fn inject_hmr_runtime(code: &str) -> String {
    format!("{}\n\n{}", HMR_RUNTIME, code)
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

/// HMR runtime code injected into the bundle.
const HMR_RUNTIME: &str = r#"
// HMR Runtime
(function() {
  const ws = new WebSocket('ws://' + location.host + '/__hmr');
  let connected = false;

  ws.onopen = () => {
    console.log('[HMR] Connected');
    connected = true;
  };

  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    switch (msg.type) {
      case 'connected':
        console.log('[HMR] Ready');
        break;
      case 'reload':
        console.log('[HMR] Full reload');
        location.reload();
        break;
      case 'update':
        console.log('[HMR] Update:', msg.modules);
        // For now, do full reload. Future: partial HMR
        location.reload();
        break;
      case 'error':
        console.error('[HMR] Build error:', msg.message);
        showErrorOverlay(msg.message);
        break;
    }
  };

  ws.onclose = () => {
    if (connected) {
      console.log('[HMR] Disconnected. Reconnecting...');
      setTimeout(() => location.reload(), 1000);
    }
  };

  ws.onerror = (err) => {
    console.error('[HMR] WebSocket error:', err);
  };

  function showErrorOverlay(message) {
    let overlay = document.getElementById('__hmr_error_overlay');
    if (!overlay) {
      overlay = document.createElement('div');
      overlay.id = '__hmr_error_overlay';
      overlay.style.cssText = `
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        background: rgba(0,0,0,0.9);
        color: #ff5555;
        padding: 32px;
        font-family: monospace;
        font-size: 16px;
        white-space: pre-wrap;
        overflow: auto;
        z-index: 999999;
      `;
      document.body.appendChild(overlay);
    }
    overlay.textContent = 'Build Error:\n\n' + message;
    overlay.style.display = 'block';
  }

  // Hide error overlay on successful rebuild
  window.__hmr_hideError = () => {
    const overlay = document.getElementById('__hmr_error_overlay');
    if (overlay) overlay.style.display = 'none';
  };
})();
"#;

/// Default index HTML page.
const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>howth dev</title>
  <style>
    body { margin: 0; font-family: system-ui, sans-serif; }
    #root { }
  </style>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="/bundle.js"></script>
</body>
</html>
"#;
