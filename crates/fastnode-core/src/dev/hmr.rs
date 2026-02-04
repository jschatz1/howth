//! HMR (Hot Module Replacement) engine for Vite-compatible dev serving.
//!
//! Provides:
//! - Module graph tracking for HMR boundary detection
//! - `import.meta.hot` client-side API
//! - Vite-compatible WebSocket protocol
//! - HMR preamble injection into served modules

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// A node in the HMR module graph.
#[derive(Debug, Clone)]
pub struct HmrModuleNode {
    /// The module URL path (e.g., `/src/App.tsx`).
    pub url: String,
    /// The file path on disk.
    pub file: String,
    /// Modules that import this module.
    pub importers: HashSet<String>,
    /// Modules that this module imports.
    pub imported_modules: HashSet<String>,
    /// Whether this module accepts self-updates (has `import.meta.hot.accept()`).
    pub is_self_accepting: bool,
    /// Whether this module accepts updates for specific deps.
    pub accepted_deps: HashSet<String>,
    /// Last update timestamp.
    pub last_invalidation_timestamp: u64,
}

impl HmrModuleNode {
    /// Create a new HMR module node.
    #[must_use] 
    pub fn new(url: String, file: String) -> Self {
        Self {
            url,
            file,
            importers: HashSet::new(),
            imported_modules: HashSet::new(),
            is_self_accepting: false,
            accepted_deps: HashSet::new(),
            last_invalidation_timestamp: 0,
        }
    }
}

/// The HMR module graph tracks import relationships for boundary detection.
pub struct HmrModuleGraph {
    /// URL → `HmrModuleNode` mapping.
    modules: RwLock<HashMap<String, HmrModuleNode>>,
    /// File path → URL mapping.
    file_to_url: RwLock<HashMap<String, String>>,
}

impl HmrModuleGraph {
    /// Create a new empty module graph.
    #[must_use] 
    pub fn new() -> Self {
        Self {
            modules: RwLock::new(HashMap::new()),
            file_to_url: RwLock::new(HashMap::new()),
        }
    }

    /// Register a module in the graph.
    pub fn ensure_module(&self, url: &str, file: &str) {
        let mut modules = self.modules.write().unwrap();
        if !modules.contains_key(url) {
            modules.insert(
                url.to_string(),
                HmrModuleNode::new(url.to_string(), file.to_string()),
            );
            self.file_to_url
                .write()
                .unwrap()
                .insert(file.to_string(), url.to_string());
        }
    }

    /// Update the import relationships for a module.
    pub fn update_module_imports(&self, url: &str, imports: &[String]) {
        let mut modules = self.modules.write().unwrap();

        // Remove old importer references
        if let Some(module) = modules.get(url) {
            let old_imports: Vec<String> = module.imported_modules.iter().cloned().collect();
            for old_import in &old_imports {
                if let Some(imported_mod) = modules.get_mut(old_import) {
                    imported_mod.importers.remove(url);
                }
            }
        }

        // Set new imports
        if let Some(module) = modules.get_mut(url) {
            module.imported_modules = imports.iter().cloned().collect();
        }

        // Add importer references
        let url_str = url.to_string();
        for import in imports {
            if let Some(imported_mod) = modules.get_mut(import) {
                imported_mod.importers.insert(url_str.clone());
            }
        }
    }

    /// Mark a module as self-accepting (has `import.meta.hot.accept()` without deps).
    pub fn mark_self_accepting(&self, url: &str) {
        if let Some(module) = self.modules.write().unwrap().get_mut(url) {
            module.is_self_accepting = true;
        }
    }

    /// Get the URL for a file path.
    pub fn get_url_by_file(&self, file: &str) -> Option<String> {
        self.file_to_url.read().unwrap().get(file).cloned()
    }

    /// Determine which modules need updating when a file changes.
    ///
    /// Walks up the importer chain until it finds an HMR boundary
    /// (a self-accepting module or a module that accepts the changed dep).
    ///
    /// Returns the list of modules to update, or None if a full page reload
    /// is needed (no HMR boundary found).
    pub fn get_hmr_boundaries(&self, file: &str) -> HmrUpdateResult {
        let modules = self.modules.read().unwrap();
        let file_to_url = self.file_to_url.read().unwrap();

        let url = match file_to_url.get(file) {
            Some(u) => u.clone(),
            None => return HmrUpdateResult::FullReload,
        };

        let module = match modules.get(&url) {
            Some(m) => m,
            None => return HmrUpdateResult::FullReload,
        };

        // If the module itself is self-accepting, it's the boundary
        if module.is_self_accepting {
            return HmrUpdateResult::Updates(vec![HmrUpdate {
                module_url: url.clone(),
                changed_file: file.to_string(),
                timestamp: now_ms(),
            }]);
        }

        // Walk up importers to find boundaries
        let mut updates = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: Vec<String> = module.importers.iter().cloned().collect();

        while let Some(importer_url) = queue.pop() {
            if !visited.insert(importer_url.clone()) {
                continue;
            }

            if let Some(importer) = modules.get(&importer_url) {
                // Check if the importer accepts updates for this dep
                if importer.accepted_deps.contains(&url) || importer.is_self_accepting {
                    updates.push(HmrUpdate {
                        module_url: importer_url,
                        changed_file: file.to_string(),
                        timestamp: now_ms(),
                    });
                } else if importer.importers.is_empty() {
                    // Reached a root with no HMR boundary → full reload
                    return HmrUpdateResult::FullReload;
                } else {
                    // Keep walking up
                    queue.extend(importer.importers.iter().cloned());
                }
            } else {
                return HmrUpdateResult::FullReload;
            }
        }

        if updates.is_empty() {
            HmrUpdateResult::FullReload
        } else {
            HmrUpdateResult::Updates(updates)
        }
    }
}

impl Default for HmrModuleGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of computing HMR updates for a file change.
#[derive(Debug)]
pub enum HmrUpdateResult {
    /// Partial update: only these modules need to re-execute.
    Updates(Vec<HmrUpdate>),
    /// No HMR boundary found: full page reload needed.
    FullReload,
}

/// A single HMR update for a module.
#[derive(Debug, Clone)]
pub struct HmrUpdate {
    /// URL of the module to update.
    pub module_url: String,
    /// File that changed.
    pub changed_file: String,
    /// Timestamp of the update.
    pub timestamp: u64,
}

/// The HMR engine manages the update lifecycle.
pub struct HmrEngine {
    /// Module graph for boundary detection.
    pub module_graph: HmrModuleGraph,
}

impl HmrEngine {
    /// Create a new HMR engine.
    #[must_use] 
    pub fn new() -> Self {
        Self {
            module_graph: HmrModuleGraph::new(),
        }
    }

    /// Process a file change and determine what to update.
    pub fn on_file_change(&self, file: &str) -> HmrUpdateResult {
        self.module_graph.get_hmr_boundaries(file)
    }

    /// Generate the HMR client runtime JavaScript.
    ///
    /// This is served at `/@hmr-client` and provides the `import.meta.hot` API.
    #[must_use] 
    pub fn client_runtime(port: u16) -> String {
        HMR_CLIENT_RUNTIME.replace("__HMR_PORT__", &port.to_string())
    }

    /// Generate the HMR preamble to inject at the top of each served module.
    ///
    /// Creates the `import.meta.hot` object for the module.
    #[must_use] 
    pub fn module_preamble(module_url: &str) -> String {
        format!(
            r#"import {{ createHotContext as __vite__createHotContext }} from "/@hmr-client";
import.meta.hot = __vite__createHotContext("{module_url}");
"#
        )
    }
}

impl Default for HmrEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The HMR client runtime JavaScript.
///
/// Provides the `import.meta.hot` API (Vite-compatible):
/// - `hot.accept()` — Self-accepting module
/// - `hot.accept(deps, cb)` — Accept specific dep updates
/// - `hot.dispose(cb)` — Cleanup before module replacement
/// - `hot.invalidate()` — Force propagation to importers
/// - `hot.data` — Persist data across updates
/// - `hot.on(event, cb)` / `hot.send(event, data)` — Custom events
const HMR_CLIENT_RUNTIME: &str = r"
// Howth HMR Client Runtime (Vite-compatible)
const hmrPort = __HMR_PORT__;
const hotModulesMap = new Map();
const disposeMap = new Map();
const dataMap = new Map();
const customListeners = new Map();

let ws;
let isConnected = false;

function setupWebSocket() {
  ws = new WebSocket(`ws://${location.hostname}:${hmrPort}/__hmr`);

  ws.onopen = () => {
    console.log('[howth] connected.');
    isConnected = true;
  };

  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    handleMessage(msg);
  };

  ws.onclose = () => {
    if (isConnected) {
      console.log('[howth] server connection lost. Polling for restart...');
      isConnected = false;
      setTimeout(() => location.reload(), 1000);
    }
  };

  ws.onerror = (err) => {
    console.error('[howth] websocket error:', err);
  };
}

function handleMessage(msg) {
  switch (msg.type) {
    case 'connected':
      console.log('[howth] ready.');
      break;

    case 'update':
      if (msg.updates) {
        for (const update of msg.updates) {
          handleUpdate(update);
        }
      } else {
        // Legacy: full reload
        location.reload();
      }
      break;

    case 'reload':
      console.log('[howth] full reload');
      location.reload();
      break;

    case 'error':
      console.error('[howth] build error:', msg.message);
      showErrorOverlay(msg.message);
      break;

    case 'custom':
      const listeners = customListeners.get(msg.event);
      if (listeners) {
        listeners.forEach(cb => cb(msg.data));
      }
      break;
  }
}

async function handleUpdate(update) {
  const { module: moduleUrl, timestamp } = update;

  const hotModule = hotModulesMap.get(moduleUrl);
  if (!hotModule) {
    // No HMR handler registered, full reload
    location.reload();
    return;
  }

  // Run dispose callbacks
  const disposeCb = disposeMap.get(moduleUrl);
  if (disposeCb) {
    disposeCb(dataMap.get(moduleUrl) || {});
  }

  // Re-import the updated module
  try {
    hideErrorOverlay();
    const newModule = await import(moduleUrl + '?t=' + timestamp);

    // Run accept callbacks
    if (hotModule.selfAccepted) {
      if (hotModule.selfAcceptCb) {
        hotModule.selfAcceptCb(newModule);
      }
    }

    if (hotModule.depCallbacks) {
      for (const [deps, cb] of hotModule.depCallbacks) {
        if (deps.includes(moduleUrl)) {
          cb(deps.map(d => d === moduleUrl ? newModule : undefined));
        }
      }
    }

    console.log(`[howth] hot updated: ${moduleUrl}`);
  } catch (err) {
    console.error(`[howth] HMR update failed for ${moduleUrl}:`, err);
    location.reload();
  }
}

function showErrorOverlay(message) {
  let overlay = document.getElementById('__howth_error_overlay');
  if (!overlay) {
    overlay = document.createElement('div');
    overlay.id = '__howth_error_overlay';
    overlay.style.cssText = `
      position: fixed; top: 0; left: 0; right: 0; bottom: 0;
      background: rgba(0,0,0,0.9); color: #ff5555;
      padding: 32px; font-family: monospace; font-size: 16px;
      white-space: pre-wrap; overflow: auto; z-index: 999999;
    `;
    document.body.appendChild(overlay);
  }
  overlay.textContent = 'Build Error:\n\n' + message;
  overlay.style.display = 'block';
}

function hideErrorOverlay() {
  const overlay = document.getElementById('__howth_error_overlay');
  if (overlay) overlay.style.display = 'none';
}

export function createHotContext(ownerPath) {
  if (!dataMap.has(ownerPath)) {
    dataMap.set(ownerPath, {});
  }

  const hot = {
    get data() {
      return dataMap.get(ownerPath);
    },

    accept(deps, cb) {
      if (typeof deps === 'function' || !deps) {
        // Self-accepting: hot.accept() or hot.accept(cb)
        const entry = hotModulesMap.get(ownerPath) || {
          selfAccepted: false,
          depCallbacks: [],
        };
        entry.selfAccepted = true;
        entry.selfAcceptCb = typeof deps === 'function' ? deps : cb;
        hotModulesMap.set(ownerPath, entry);
        // Notify server that this module is self-accepting
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'hotAccept', path: ownerPath }));
        }
      } else if (typeof deps === 'string') {
        // Accept single dep: hot.accept('./dep', cb)
        const entry = hotModulesMap.get(ownerPath) || {
          selfAccepted: false,
          depCallbacks: [],
        };
        entry.depCallbacks.push([[deps], cb]);
        hotModulesMap.set(ownerPath, entry);
      } else if (Array.isArray(deps)) {
        // Accept multiple deps: hot.accept(['./a', './b'], cb)
        const entry = hotModulesMap.get(ownerPath) || {
          selfAccepted: false,
          depCallbacks: [],
        };
        entry.depCallbacks.push([deps, cb]);
        hotModulesMap.set(ownerPath, entry);
      }
    },

    dispose(cb) {
      disposeMap.set(ownerPath, cb);
    },

    invalidate() {
      // Tell the server this module can't self-update
      ws.send(JSON.stringify({ type: 'invalidate', path: ownerPath }));
      location.reload();
    },

    on(event, cb) {
      if (!customListeners.has(event)) {
        customListeners.set(event, []);
      }
      customListeners.get(event).push(cb);
    },

    send(event, data) {
      ws.send(JSON.stringify({ type: 'custom', event, data }));
    },
  };

  return hot;
}

// Initialize
setupWebSocket();
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmr_module_graph_basic() {
        let graph = HmrModuleGraph::new();

        graph.ensure_module("/src/App.tsx", "/project/src/App.tsx");
        graph.ensure_module("/src/main.tsx", "/project/src/main.tsx");

        graph.update_module_imports("/src/main.tsx", &["/src/App.tsx".to_string()]);

        let url = graph.get_url_by_file("/project/src/App.tsx");
        assert_eq!(url, Some("/src/App.tsx".to_string()));
    }

    #[test]
    fn test_hmr_self_accepting_boundary() {
        let graph = HmrModuleGraph::new();

        graph.ensure_module("/src/App.tsx", "/project/src/App.tsx");
        graph.mark_self_accepting("/src/App.tsx");

        let result = graph.get_hmr_boundaries("/project/src/App.tsx");
        match result {
            HmrUpdateResult::Updates(updates) => {
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].module_url, "/src/App.tsx");
            }
            HmrUpdateResult::FullReload => panic!("Expected partial update"),
        }
    }

    #[test]
    fn test_hmr_no_boundary_full_reload() {
        let graph = HmrModuleGraph::new();

        graph.ensure_module("/src/utils.ts", "/project/src/utils.ts");

        // No self-accepting, no importers → full reload
        let result = graph.get_hmr_boundaries("/project/src/utils.ts");
        match result {
            HmrUpdateResult::FullReload => {} // expected
            HmrUpdateResult::Updates(_) => panic!("Expected full reload"),
        }
    }

    #[test]
    fn test_hmr_engine_client_runtime() {
        let runtime = HmrEngine::client_runtime(3000);
        assert!(runtime.contains("3000"));
        assert!(runtime.contains("createHotContext"));
        assert!(runtime.contains("__hmr"));
    }

    #[test]
    fn test_hmr_module_preamble() {
        let preamble = HmrEngine::module_preamble("/src/App.tsx");
        assert!(preamble.contains("createHotContext"));
        assert!(preamble.contains("/src/App.tsx"));
    }
}
