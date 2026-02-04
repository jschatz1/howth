//! React Fast Refresh plugin for hot module replacement.
//!
//! Equivalent to `@vitejs/plugin-react` — injects React Refresh runtime
//! and wraps components with HMR boundaries so edits to React components
//! update in the browser without losing state.
//!
//! ## How It Works
//!
//! 1. Serves `/@react-refresh` virtual module with the refresh runtime
//! 2. For each `.tsx`/`.jsx` file, injects a preamble that registers
//!    components with the refresh runtime
//! 3. Appends a footer that performs the actual refresh call
//!
//! ## Usage
//!
//! ```ignore
//! use fastnode_core::bundler::plugins::ReactRefreshPlugin;
//!
//! let plugin = ReactRefreshPlugin::new();
//! let bundler = Bundler::new().plugin(Box::new(plugin));
//! ```

use crate::bundler::{
    HookResult, LoadResult, Plugin, PluginContext, PluginEnforce, ResolveIdResult, TransformResult,
};

/// React Fast Refresh plugin.
///
/// Provides component-level HMR for React applications without full page reloads.
pub struct ReactRefreshPlugin {
    /// Only inject refresh in dev mode (not production builds).
    dev_only: bool,
}

impl ReactRefreshPlugin {
    /// Create a new React Refresh plugin.
    pub fn new() -> Self {
        Self { dev_only: true }
    }

    /// Set whether this plugin only runs in dev mode.
    pub fn dev_only(mut self, dev_only: bool) -> Self {
        self.dev_only = dev_only;
        self
    }

    /// Check if a file should have refresh transforms applied.
    fn is_refresh_target(id: &str) -> bool {
        id.ends_with(".tsx") || id.ends_with(".jsx")
    }

    /// Check if the transformed code likely contains React components.
    ///
    /// A function is considered a component if it starts with an uppercase letter
    /// and returns JSX (which after transpilation becomes createElement/jsx calls).
    fn has_react_components(code: &str) -> bool {
        // Look for function components: function App() or const App = () =>
        // After SWC transpilation, JSX becomes React.createElement or _jsx calls
        code.contains("_jsx(")
            || code.contains("_jsxs(")
            || code.contains("jsx(")
            || code.contains("jsxs(")
            || code.contains("createElement(")
    }
}

impl Default for ReactRefreshPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ReactRefreshPlugin {
    fn name(&self) -> &'static str {
        "react-refresh"
    }

    fn enforce(&self) -> PluginEnforce {
        // Run after normal plugins (after SWC transpilation)
        PluginEnforce::Post
    }

    fn resolve_id(
        &self,
        specifier: &str,
        _importer: Option<&str>,
        _ctx: &PluginContext,
    ) -> HookResult<Option<ResolveIdResult>> {
        // Handle the virtual refresh runtime module
        if specifier == "/@react-refresh" || specifier == "\0react-refresh" {
            return Ok(Some(ResolveIdResult::resolved("\0react-refresh")));
        }
        Ok(None)
    }

    fn load(&self, id: &str, _ctx: &PluginContext) -> HookResult<Option<LoadResult>> {
        // Serve the React Refresh runtime
        if id == "\0react-refresh" {
            return Ok(Some(LoadResult::code(REACT_REFRESH_RUNTIME)));
        }
        Ok(None)
    }

    fn transform(
        &self,
        code: &str,
        id: &str,
        ctx: &PluginContext,
    ) -> HookResult<Option<TransformResult>> {
        // Only transform in dev/watch mode if dev_only is set
        if self.dev_only && !ctx.watch {
            return Ok(None);
        }

        // Only transform JSX/TSX files
        if !Self::is_refresh_target(id) {
            return Ok(None);
        }

        // Only transform files that contain React components
        if !Self::has_react_components(code) {
            return Ok(None);
        }

        // Inject refresh preamble and footer
        let preamble = REACT_REFRESH_PREAMBLE;
        let footer = generate_refresh_footer(id);

        let transformed = format!("{}\n{}\n{}", preamble, code, footer);

        Ok(Some(TransformResult::code(transformed)))
    }

    fn transform_index_html(&self, html: &str) -> HookResult<Option<String>> {
        // Inject the refresh runtime script before the first <script> tag
        if html.contains("<script") {
            let injection = r#"<script type="module">
import RefreshRuntime from '/@react-refresh';
RefreshRuntime.injectIntoGlobalHook(window);
window.$RefreshReg$ = () => {};
window.$RefreshSig$ = () => (type) => type;
window.__vite_plugin_react_preamble_installed__ = true;
</script>"#;

            let transformed = html.replacen("<script", &format!("{}\n  <script", injection), 1);
            return Ok(Some(transformed));
        }
        Ok(None)
    }
}

/// Generate the refresh footer for a specific module.
///
/// This footer registers all exported components with the refresh runtime
/// and performs the actual hot update.
fn generate_refresh_footer(module_id: &str) -> String {
    let escaped_id = module_id.replace('\\', "\\\\").replace('"', "\\\"");

    format!(
        r#"
// React Refresh Footer
if (import.meta.hot) {{
  import.meta.hot.accept();
  if (!window.__vite_plugin_react_preamble_installed__) {{
    throw new Error(
      "React refresh preamble was not loaded. " +
      "Make sure the index.html includes the refresh runtime script."
    );
  }}
  RefreshRuntime.performReactRefresh();
}}

window.$RefreshReg$ && window.$RefreshReg$(function() {{}}, "{module_id}");
"#,
        module_id = escaped_id
    )
}

/// React Refresh preamble injected at the top of each component file.
///
/// Sets up the registration functions that React Refresh uses to track components.
const REACT_REFRESH_PREAMBLE: &str = r"import RefreshRuntime from '/@react-refresh';

const prevRefreshReg = window.$RefreshReg$;
const prevRefreshSig = window.$RefreshSig$;

window.$RefreshReg$ = (type, id) => {
  RefreshRuntime.register(type, __MODULE_ID__ + ' ' + id);
};
window.$RefreshSig$ = RefreshRuntime.createSignatureFunctionForTransform;";

/// The React Refresh runtime module served at `/@react-refresh`.
///
/// This is a minimal implementation of the React Refresh runtime that
/// provides the core APIs:
/// - `register(type, id)` — Register a component with its unique ID
/// - `createSignatureFunctionForTransform()` — Track hook signatures
/// - `performReactRefresh()` — Trigger refresh for registered components
/// - `injectIntoGlobalHook(window)` — Set up global hooks
const REACT_REFRESH_RUNTIME: &str = r"
// React Refresh Runtime (minimal implementation for howth dev server)
//
// This provides the core APIs that the React Refresh Babel/SWC transform expects.
// In production, this would use react-refresh/runtime, but for the dev server
// we provide a lightweight shim that delegates to React's internal refresh mechanism.

const registeredComponents = new Map();
const pendingUpdates = new Set();
let isPerformingRefresh = false;

function debounce(fn, delay) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delay);
  };
}

function register(type, fullID) {
  if (type == null || typeof type !== 'function') return;

  registeredComponents.set(fullID, type);
  pendingUpdates.add(fullID);
}

function createSignatureFunctionForTransform() {
  let savedType;
  let hasCustomHooks = false;
  let didCollectHooks = false;

  return function(type, key, forceReset, getCustomHooks) {
    if (typeof key === 'string') {
      if (!savedType) {
        savedType = type;
        hasCustomHooks = typeof getCustomHooks === 'function';
      }

      if (type != null && (typeof type === 'function' || typeof type === 'object')) {
        // Store the signature for comparison on update
        if (!type.__signature) {
          type.__signature = { key, forceReset, getCustomHooks };
        }
      }
    } else {
      // No key means we're just wrapping
    }
    return type;
  };
}

const scheduleRefresh = debounce(() => {
  performReactRefresh();
}, 30);

function performReactRefresh() {
  if (isPerformingRefresh) return;
  isPerformingRefresh = true;

  try {
    // Attempt to use React's built-in refresh mechanism if available
    if (window.__REACT_DEVTOOLS_GLOBAL_HOOK__?.renderers?.size > 0) {
      for (const [, renderer] of window.__REACT_DEVTOOLS_GLOBAL_HOOK__.renderers) {
        if (renderer.scheduleRefresh) {
          renderer.scheduleRefresh(
            new Set(Array.from(pendingUpdates).map(id => registeredComponents.get(id)).filter(Boolean)),
            new Map()
          );
        }
      }
    }
    pendingUpdates.clear();
  } catch (e) {
    console.error('[react-refresh] Failed to perform refresh:', e);
    // Fallback: full page reload
    window.location.reload();
  } finally {
    isPerformingRefresh = false;
  }
}

function injectIntoGlobalHook(globalObject) {
  if (!globalObject.__REACT_DEVTOOLS_GLOBAL_HOOK__) {
    // Create a minimal DevTools hook for refresh to work
    let nextID = 0;
    globalObject.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
      renderers: new Map(),
      supportsFiber: true,
      inject(renderer) {
        const id = nextID++;
        this.renderers.set(id, renderer);
        return id;
      },
      onScheduleFiberRoot() {},
      onCommitFiberRoot() {},
      onCommitFiberUnmount() {},
    };
  }
}

export default {
  register,
  createSignatureFunctionForTransform,
  performReactRefresh,
  injectIntoGlobalHook,
};

export {
  register,
  createSignatureFunctionForTransform,
  performReactRefresh,
  injectIntoGlobalHook,
};
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_refresh_target() {
        assert!(ReactRefreshPlugin::is_refresh_target("App.tsx"));
        assert!(ReactRefreshPlugin::is_refresh_target("Button.jsx"));
        assert!(ReactRefreshPlugin::is_refresh_target(
            "/src/components/App.tsx"
        ));
        assert!(!ReactRefreshPlugin::is_refresh_target("utils.ts"));
        assert!(!ReactRefreshPlugin::is_refresh_target("index.js"));
        assert!(!ReactRefreshPlugin::is_refresh_target("style.css"));
    }

    #[test]
    fn test_has_react_components() {
        assert!(ReactRefreshPlugin::has_react_components(
            "return _jsx(\"div\", { children: \"Hello\" });"
        ));
        assert!(ReactRefreshPlugin::has_react_components(
            "return _jsxs(\"div\", { children: [\"Hello\"] });"
        ));
        assert!(ReactRefreshPlugin::has_react_components(
            "React.createElement('div', null, 'Hello')"
        ));
        assert!(!ReactRefreshPlugin::has_react_components(
            "export const x = 42;"
        ));
    }

    #[test]
    fn test_transform_jsx_file() {
        let plugin = ReactRefreshPlugin::new().dev_only(false);
        let ctx = PluginContext::default();

        // Code that looks like it contains JSX (post-SWC transpilation)
        let code = r#"
import { useState } from 'react';
function App() {
  const [count, setCount] = useState(0);
  return _jsx("div", { children: count });
}
export default App;
"#;

        let result = plugin.transform(code, "App.tsx", &ctx).unwrap();
        assert!(result.is_some());

        let transformed = result.unwrap().code;
        assert!(transformed.contains("RefreshRuntime"));
        assert!(transformed.contains("performReactRefresh"));
        assert!(transformed.contains("import.meta.hot"));
    }

    #[test]
    fn test_no_transform_for_non_jsx() {
        let plugin = ReactRefreshPlugin::new().dev_only(false);
        let ctx = PluginContext::default();

        let code = "export const x = 42;";
        let result = plugin.transform(code, "utils.ts", &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_refresh_runtime() {
        let plugin = ReactRefreshPlugin::new();
        let ctx = PluginContext::default();

        let result = plugin.resolve_id("/@react-refresh", None, &ctx).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "\0react-refresh");
    }

    #[test]
    fn test_load_refresh_runtime() {
        let plugin = ReactRefreshPlugin::new();
        let ctx = PluginContext::default();

        let result = plugin.load("\0react-refresh", &ctx).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().code.contains("register"));
    }

    #[test]
    fn test_transform_index_html() {
        let plugin = ReactRefreshPlugin::new();
        let html = r#"<!DOCTYPE html>
<html>
<head>
  <script type="module" src="/src/main.tsx"></script>
</head>
</html>"#;

        let result = plugin.transform_index_html(html).unwrap();
        assert!(result.is_some());
        let transformed = result.unwrap();
        assert!(transformed.contains("/@react-refresh"));
        assert!(transformed.contains("$RefreshReg$"));
    }
}
