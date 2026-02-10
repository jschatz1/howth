//! Plugin system for the bundler.
//!
//! Provides a Rollup-compatible plugin interface with hooks at various build stages.
//!
//! ## Example
//!
//! ```ignore
//! use fastnode_core::bundler::{Plugin, PluginContext, HookResult};
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &str { "my-plugin" }
//!
//!     fn transform(&self, code: &str, id: &str, _ctx: &PluginContext) -> HookResult<Option<String>> {
//!         // Transform .txt files to JS modules
//!         if id.ends_with(".txt") {
//!             return Ok(Some(format!("export default {:?};", code)));
//!         }
//!         Ok(None)

#![allow(clippy::type_complexity)]
//!     }
//! }
//! ```

#![allow(clippy::unused_self)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::unnecessary_literal_bound)]

use rustc_hash::FxHashMap as HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Result type for plugin hooks.
pub type HookResult<T> = Result<T, PluginError>;

/// Error from a plugin.
#[derive(Debug)]
pub struct PluginError {
    /// Plugin name that caused the error.
    pub plugin: String,
    /// Hook that failed.
    pub hook: &'static str,
    /// Error message.
    pub message: String,
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}: {}", self.plugin, self.hook, self.message)
    }
}

impl std::error::Error for PluginError {}

/// Context passed to plugin hooks.
#[derive(Debug, Default)]
pub struct PluginContext {
    /// Working directory.
    pub cwd: PathBuf,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Whether this is a watch/dev build.
    pub watch: bool,
    /// Build metadata for plugin communication.
    meta: HashMap<String, String>,
}

impl PluginContext {
    /// Create a new plugin context.
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            env: std::env::vars().collect(),
            watch: false,
            meta: HashMap::default(),
        }
    }

    /// Set a metadata value (for inter-plugin communication).
    pub fn set_meta(&mut self, key: &str, value: String) {
        self.meta.insert(key.to_string(), value);
    }

    /// Get a metadata value.
    pub fn get_meta(&self, key: &str) -> Option<&String> {
        self.meta.get(key)
    }
}

/// Result of resolve hook.
#[derive(Debug, Clone)]
pub struct ResolveIdResult {
    /// Resolved module ID (usually a file path).
    pub id: String,
    /// Whether this module is external (don't bundle).
    pub external: bool,
}

impl ResolveIdResult {
    /// Create a resolved module result.
    pub fn resolved(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            external: false,
        }
    }

    /// Create an external module result.
    pub fn external(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            external: true,
        }
    }
}

/// Result of load hook.
#[derive(Debug, Clone)]
pub struct LoadResult {
    /// Module source code.
    pub code: String,
    /// Optional source map.
    pub map: Option<String>,
}

impl LoadResult {
    /// Create a load result with code only.
    pub fn code(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            map: None,
        }
    }
}

/// Result of transform hook.
#[derive(Debug, Clone)]
pub struct TransformResult {
    /// Transformed code.
    pub code: String,
    /// Optional source map.
    pub map: Option<String>,
}

impl TransformResult {
    /// Create a transform result with code only.
    pub fn code(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            map: None,
        }
    }
}

/// Plugin enforcement ordering.
///
/// Controls where a plugin runs relative to others in the pipeline.
/// Mirrors Vite's `enforce` option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum PluginEnforce {
    /// Runs before normal plugins (e.g., alias resolution).
    Pre,
    /// Default ordering (no enforcement).
    #[default]
    Normal,
    /// Runs after normal plugins (e.g., minification).
    Post,
}

/// Development server configuration.
///
/// Passed to the `config` hook so plugins can modify dev server settings.
#[derive(Debug, Clone)]
pub struct DevConfig {
    /// Root directory of the project.
    pub root: PathBuf,
    /// Dev server port.
    pub port: u16,
    /// Dev server host.
    pub host: String,
    /// Base public path.
    pub base: String,
    /// Custom define replacements (like Vite's `define`).
    pub define: HashMap<String, String>,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_default(),
            port: 3000,
            host: "localhost".to_string(),
            base: "/".to_string(),
            define: HashMap::default(),
        }
    }
}

/// Context for the dev server, passed to `configure_server` hook.
///
/// Allows plugins to register middleware and custom route handlers.
pub struct ServerContext {
    /// Project root.
    pub root: PathBuf,
    /// Dev server configuration.
    pub config: DevConfig,
    /// Registered middleware (pre-handlers that run before internal handlers).
    pub middlewares: Vec<ServerMiddleware>,
}

/// A middleware function registered by a plugin.
pub struct ServerMiddleware {
    /// Name for debugging.
    pub name: String,
    /// The handler function.
    pub handler: Arc<dyn Fn(&str, &str) -> Option<MiddlewareResponse> + Send + Sync>,
}

/// Response from a middleware.
#[derive(Debug, Clone)]
pub struct MiddlewareResponse {
    /// HTTP status code.
    pub status: u16,
    /// Content-Type header.
    pub content_type: String,
    /// Response body.
    pub body: String,
}

impl ServerContext {
    /// Create a new server context.
    pub fn new(root: PathBuf, config: DevConfig) -> Self {
        Self {
            root,
            config,
            middlewares: Vec::new(),
        }
    }
}

/// Context for hot module update events.
///
/// Passed to the `handle_hot_update` hook when a file changes.
#[derive(Debug, Clone)]
pub struct HotUpdateContext {
    /// The file that changed (absolute path).
    pub file: String,
    /// Timestamp of the update.
    pub timestamp: u64,
    /// Modules affected by this change.
    pub modules: Vec<String>,
}

/// Chunk information passed to renderChunk.
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    /// Chunk name.
    pub name: String,
    /// Whether this is the entry chunk.
    pub is_entry: bool,
    /// Module IDs in this chunk.
    pub modules: Vec<String>,
}

/// The main plugin trait.
///
/// Implement this trait to create custom plugins. All methods have default
/// implementations that do nothing, so you only need to implement the hooks
/// you care about.
///
/// ## Vite-Compatible Hooks
///
/// In addition to Rollup-compatible hooks (`resolve_id`, `load`, `transform`,
/// `render_chunk`, `build_start`, `build_end`), this trait supports Vite-specific
/// hooks for dev server integration:
///
/// - `enforce` — Plugin ordering (`Pre`, `Normal`, `Post`)
/// - `config` — Modify dev config before resolution
/// - `config_resolved` — Read final resolved config
/// - `configure_server` — Add middleware/routes to dev server
/// - `transform_index_html` — Transform the index HTML page
/// - `handle_hot_update` — Custom HMR logic on file changes
pub trait Plugin: Send + Sync {
    /// Plugin name for debugging and error messages.
    fn name(&self) -> &str;

    /// Plugin ordering: `Pre`, `Normal` (default), or `Post`.
    ///
    /// `Pre` plugins run before normal plugins (useful for alias resolution).
    /// `Post` plugins run after normal plugins (useful for minification).
    fn enforce(&self) -> PluginEnforce {
        PluginEnforce::Normal
    }

    /// Called at the start of the build.
    fn build_start(&self, _ctx: &PluginContext) -> HookResult<()> {
        Ok(())
    }

    /// Resolve a module specifier to an ID.
    ///
    /// Return `Some(result)` to handle this resolution, or `None` to let
    /// the next plugin or default resolver handle it.
    fn resolve_id(
        &self,
        _specifier: &str,
        _importer: Option<&str>,
        _ctx: &PluginContext,
    ) -> HookResult<Option<ResolveIdResult>> {
        Ok(None)
    }

    /// Load a module by ID.
    ///
    /// Return `Some(result)` to provide the module source, or `None` to let
    /// the next plugin or default loader handle it.
    fn load(&self, _id: &str, _ctx: &PluginContext) -> HookResult<Option<LoadResult>> {
        Ok(None)
    }

    /// Transform module source code.
    ///
    /// Return `Some(result)` to transform the code, or `None` to pass it through.
    /// Multiple plugins can transform the same module in sequence.
    fn transform(
        &self,
        _code: &str,
        _id: &str,
        _ctx: &PluginContext,
    ) -> HookResult<Option<TransformResult>> {
        Ok(None)
    }

    /// Transform a rendered chunk.
    ///
    /// Return `Some(code)` to transform the chunk output, or `None` to pass through.
    fn render_chunk(
        &self,
        _code: &str,
        _chunk: &ChunkInfo,
        _ctx: &PluginContext,
    ) -> HookResult<Option<String>> {
        Ok(None)
    }

    /// Called at the end of the build.
    fn build_end(&self, _ctx: &PluginContext) -> HookResult<()> {
        Ok(())
    }

    // ========================================================================
    // Vite-compatible hooks (dev server only)
    // ========================================================================

    /// Modify the dev config before it is resolved.
    ///
    /// Called once at dev server startup. Plugins can mutate the config.
    fn config(&self, _config: &mut DevConfig) -> HookResult<()> {
        Ok(())
    }

    /// Called after config is resolved (read-only).
    ///
    /// Plugins can store the final config for later use.
    fn config_resolved(&self, _config: &DevConfig) -> HookResult<()> {
        Ok(())
    }

    /// Configure the dev server.
    ///
    /// Called once at dev server startup. Plugins can add middleware,
    /// custom routes, or other server-side logic.
    fn configure_server(&self, _server: &mut ServerContext) -> HookResult<()> {
        Ok(())
    }

    /// Transform the index HTML page.
    ///
    /// Called when the dev server serves the index HTML. Plugins can inject
    /// scripts, stylesheets, or modify the HTML in any way.
    ///
    /// Return `Some(html)` to replace the HTML, or `None` to pass through.
    fn transform_index_html(&self, _html: &str) -> HookResult<Option<String>> {
        Ok(None)
    }

    /// Handle a hot module update.
    ///
    /// Called when a file changes during dev. Plugins can filter or modify
    /// which modules are considered affected.
    ///
    /// Return `Some(modules)` to override the affected modules list,
    /// or `None` to use the default behavior.
    fn handle_hot_update(&self, _ctx: &HotUpdateContext) -> HookResult<Option<Vec<String>>> {
        Ok(None)
    }
}

/// A container for managing multiple plugins.
///
/// Plugins are sorted by their `enforce()` ordering: `Pre` → `Normal` → `Post`.
/// Within the same enforcement level, insertion order is preserved.
pub struct PluginContainer {
    plugins: Vec<Box<dyn Plugin>>,
    ctx: PluginContext,
    /// Whether plugins need re-sorting after insertion.
    needs_sort: bool,
}

impl PluginContainer {
    /// Create a new plugin container.
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            ctx: PluginContext::new(cwd),
            needs_sort: false,
        }
    }

    /// Add a plugin. Plugins are automatically sorted by enforce order.
    pub fn add(&mut self, plugin: Box<dyn Plugin>) {
        let enforce = plugin.enforce();
        if enforce != PluginEnforce::Normal {
            self.needs_sort = true;
        }
        self.plugins.push(plugin);
    }

    /// Sort plugins by enforce order (Pre → Normal → Post).
    /// Uses a stable sort to preserve insertion order within each level.
    fn ensure_sorted(&mut self) {
        if self.needs_sort {
            self.plugins.sort_by_key(|p| p.enforce());
            self.needs_sort = false;
        }
    }

    /// Set watch mode.
    pub fn set_watch(&mut self, watch: bool) {
        self.ctx.watch = watch;
    }

    /// Get the context (for modification).
    pub fn context_mut(&mut self) -> &mut PluginContext {
        &mut self.ctx
    }

    /// Get the context (read-only).
    pub fn context(&self) -> &PluginContext {
        &self.ctx
    }

    /// Call build_start on all plugins.
    pub fn build_start(&self) -> HookResult<()> {
        for plugin in &self.plugins {
            plugin.build_start(&self.ctx)?;
        }
        Ok(())
    }

    /// Try to resolve a module ID through plugins.
    /// Returns None if no plugin handled the resolution.
    pub fn resolve_id(
        &self,
        specifier: &str,
        importer: Option<&str>,
    ) -> HookResult<Option<ResolveIdResult>> {
        for plugin in &self.plugins {
            if let Some(result) = plugin.resolve_id(specifier, importer, &self.ctx)? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }

    /// Try to load a module through plugins.
    /// Returns None if no plugin handled the load.
    pub fn load(&self, id: &str) -> HookResult<Option<LoadResult>> {
        for plugin in &self.plugins {
            if let Some(result) = plugin.load(id, &self.ctx)? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }

    /// Transform code through all plugins.
    /// Each plugin's output is passed to the next plugin.
    pub fn transform(&self, code: &str, id: &str) -> HookResult<String> {
        let mut current = code.to_string();
        for plugin in &self.plugins {
            if let Some(result) = plugin.transform(&current, id, &self.ctx)? {
                current = result.code;
            }
        }
        Ok(current)
    }

    /// Transform a chunk through all plugins.
    pub fn render_chunk(&self, code: &str, chunk: &ChunkInfo) -> HookResult<String> {
        let mut current = code.to_string();
        for plugin in &self.plugins {
            if let Some(transformed) = plugin.render_chunk(&current, chunk, &self.ctx)? {
                current = transformed;
            }
        }
        Ok(current)
    }

    /// Call build_end on all plugins.
    pub fn build_end(&self) -> HookResult<()> {
        for plugin in &self.plugins {
            plugin.build_end(&self.ctx)?;
        }
        Ok(())
    }

    /// Check if any plugins are registered.
    pub fn has_plugins(&self) -> bool {
        !self.plugins.is_empty()
    }

    // ========================================================================
    // Vite-compatible hook dispatchers
    // ========================================================================

    /// Call `config` on all plugins, letting each mutate the config.
    pub fn call_config(&self, config: &mut DevConfig) -> HookResult<()> {
        for plugin in &self.plugins {
            plugin.config(config)?;
        }
        Ok(())
    }

    /// Call `config_resolved` on all plugins.
    pub fn call_config_resolved(&self, config: &DevConfig) -> HookResult<()> {
        for plugin in &self.plugins {
            plugin.config_resolved(config)?;
        }
        Ok(())
    }

    /// Call `configure_server` on all plugins.
    pub fn call_configure_server(&self, server: &mut ServerContext) -> HookResult<()> {
        for plugin in &self.plugins {
            plugin.configure_server(server)?;
        }
        Ok(())
    }

    /// Call `transform_index_html` on all plugins (chained).
    pub fn call_transform_index_html(&self, html: &str) -> HookResult<String> {
        let mut current = html.to_string();
        for plugin in &self.plugins {
            if let Some(transformed) = plugin.transform_index_html(&current)? {
                current = transformed;
            }
        }
        Ok(current)
    }

    /// Call `handle_hot_update` on all plugins.
    /// Returns the first non-None result, or None if no plugin handled it.
    pub fn call_handle_hot_update(
        &self,
        ctx: &HotUpdateContext,
    ) -> HookResult<Option<Vec<String>>> {
        for plugin in &self.plugins {
            if let Some(modules) = plugin.handle_hot_update(ctx)? {
                return Ok(Some(modules));
            }
        }
        Ok(None)
    }
}

impl Default for PluginContainer {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_default())
    }
}

// ============================================================================
// Built-in Plugins
// ============================================================================

/// Plugin that replaces global identifiers with values.
///
/// Useful for replacing `process.env.NODE_ENV` with `"production"`.
pub struct ReplacePlugin {
    replacements: HashMap<String, String>,
}

impl ReplacePlugin {
    /// Create a new replace plugin.
    pub fn new() -> Self {
        Self {
            replacements: HashMap::default(),
        }
    }

    /// Add a replacement.
    pub fn replace(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.replacements.insert(from.into(), to.into());
        self
    }

    /// Add environment variable replacement.
    /// Replaces `process.env.KEY` with the value.
    pub fn env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.replacements.insert(
            format!("process.env.{}", key),
            format!("\"{}\"", value.into()),
        );
        self
    }
}

impl Default for ReplacePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for ReplacePlugin {
    fn name(&self) -> &str {
        "replace"
    }

    fn transform(
        &self,
        code: &str,
        _id: &str,
        _ctx: &PluginContext,
    ) -> HookResult<Option<TransformResult>> {
        if self.replacements.is_empty() {
            return Ok(None);
        }

        let mut result = code.to_string();
        let mut changed = false;

        for (from, to) in &self.replacements {
            if result.contains(from) {
                result = result.replace(from, to);
                changed = true;
            }
        }

        if changed {
            Ok(Some(TransformResult::code(result)))
        } else {
            Ok(None)
        }
    }
}

/// Plugin that creates virtual modules.
///
/// Allows you to define modules that don't exist on disk.
pub struct VirtualPlugin {
    modules: HashMap<String, String>,
}

impl VirtualPlugin {
    /// Create a new virtual plugin.
    pub fn new() -> Self {
        Self {
            modules: HashMap::default(),
        }
    }

    /// Add a virtual module.
    pub fn module(mut self, id: impl Into<String>, code: impl Into<String>) -> Self {
        self.modules.insert(id.into(), code.into());
        self
    }
}

impl Default for VirtualPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for VirtualPlugin {
    fn name(&self) -> &str {
        "virtual"
    }

    fn resolve_id(
        &self,
        specifier: &str,
        _importer: Option<&str>,
        _ctx: &PluginContext,
    ) -> HookResult<Option<ResolveIdResult>> {
        // Handle virtual: prefix
        if let Some(id) = specifier.strip_prefix("virtual:") {
            if self.modules.contains_key(id) {
                return Ok(Some(ResolveIdResult::resolved(format!("\0virtual:{}", id))));
            }
        }
        // Also handle direct lookup
        if self.modules.contains_key(specifier) {
            return Ok(Some(ResolveIdResult::resolved(format!(
                "\0virtual:{}",
                specifier
            ))));
        }
        Ok(None)
    }

    fn load(&self, id: &str, _ctx: &PluginContext) -> HookResult<Option<LoadResult>> {
        if let Some(virtual_id) = id.strip_prefix("\0virtual:") {
            if let Some(code) = self.modules.get(virtual_id) {
                return Ok(Some(LoadResult::code(code)));
            }
        }
        Ok(None)
    }
}

/// Plugin that handles import aliases.
///
/// Maps import paths like `@/components` to `./src/components`.
pub struct AliasPlugin {
    aliases: Vec<(String, String)>,
}

impl AliasPlugin {
    /// Create a new alias plugin.
    pub fn new() -> Self {
        Self {
            aliases: Vec::new(),
        }
    }

    /// Add an alias.
    pub fn alias(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.aliases.push((from.into(), to.into()));
        self
    }
}

impl Default for AliasPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AliasPlugin {
    fn name(&self) -> &str {
        "alias"
    }

    fn resolve_id(
        &self,
        specifier: &str,
        _importer: Option<&str>,
        _ctx: &PluginContext,
    ) -> HookResult<Option<ResolveIdResult>> {
        for (from, to) in &self.aliases {
            if specifier == from {
                return Ok(Some(ResolveIdResult::resolved(to)));
            }
            if let Some(rest) = specifier.strip_prefix(from) {
                if rest.starts_with('/') {
                    return Ok(Some(ResolveIdResult::resolved(format!("{}{}", to, rest))));
                }
            }
        }
        Ok(None)
    }
}

/// Plugin that adds a banner/footer to output.
pub struct BannerPlugin {
    banner: Option<String>,
    footer: Option<String>,
}

impl BannerPlugin {
    /// Create a new banner plugin.
    pub fn new() -> Self {
        Self {
            banner: None,
            footer: None,
        }
    }

    /// Set the banner (prepended to output).
    pub fn banner(mut self, text: impl Into<String>) -> Self {
        self.banner = Some(text.into());
        self
    }

    /// Set the footer (appended to output).
    pub fn footer(mut self, text: impl Into<String>) -> Self {
        self.footer = Some(text.into());
        self
    }
}

impl Default for BannerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BannerPlugin {
    fn name(&self) -> &str {
        "banner"
    }

    fn render_chunk(
        &self,
        code: &str,
        _chunk: &ChunkInfo,
        _ctx: &PluginContext,
    ) -> HookResult<Option<String>> {
        if self.banner.is_none() && self.footer.is_none() {
            return Ok(None);
        }

        let mut result = String::new();

        if let Some(banner) = &self.banner {
            result.push_str(banner);
            result.push('\n');
        }

        result.push_str(code);

        if let Some(footer) = &self.footer {
            result.push('\n');
            result.push_str(footer);
        }

        Ok(Some(result))
    }
}

/// Plugin that handles JSON imports.
pub struct JsonPlugin;

impl Plugin for JsonPlugin {
    fn name(&self) -> &str {
        "json"
    }

    fn transform(
        &self,
        code: &str,
        id: &str,
        _ctx: &PluginContext,
    ) -> HookResult<Option<TransformResult>> {
        if !id.ends_with(".json") {
            return Ok(None);
        }

        // Convert JSON to ES module
        Ok(Some(TransformResult::code(format!(
            "export default {};",
            code.trim()
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_plugin() {
        let plugin = ReplacePlugin::new()
            .replace("__DEV__", "false")
            .env("NODE_ENV", "production");

        let code = "
            if (__DEV__) { console.log('dev'); }
            const env = process.env.NODE_ENV;
        ";

        let result = plugin
            .transform(code, "test.js", &PluginContext::default())
            .unwrap();
        let transformed = result.unwrap().code;

        assert!(transformed.contains("if (false)"));
        assert!(transformed.contains(r#"const env = "production""#));
    }

    #[test]
    fn test_virtual_plugin() {
        let plugin = VirtualPlugin::new().module("my-module", "export const x = 1;");

        let ctx = PluginContext::default();

        // Should resolve virtual module
        let result = plugin.resolve_id("virtual:my-module", None, &ctx).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "\0virtual:my-module");

        // Should load virtual module
        let result = plugin.load("\0virtual:my-module", &ctx).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().code, "export const x = 1;");
    }

    #[test]
    fn test_alias_plugin() {
        let plugin = AliasPlugin::new().alias("@", "./src").alias("~", "./");

        let ctx = PluginContext::default();

        // Exact match
        let result = plugin.resolve_id("@", None, &ctx).unwrap();
        assert_eq!(result.unwrap().id, "./src");

        // Prefix match
        let result = plugin
            .resolve_id("@/components/Button", None, &ctx)
            .unwrap();
        assert_eq!(result.unwrap().id, "./src/components/Button");

        // No match
        let result = plugin.resolve_id("lodash", None, &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_banner_plugin() {
        let plugin = BannerPlugin::new()
            .banner("/* Copyright 2024 */")
            .footer("/* End of bundle */");

        let ctx = PluginContext::default();
        let chunk = ChunkInfo {
            name: "main".to_string(),
            is_entry: true,
            modules: vec![],
        };

        let result = plugin.render_chunk("const x = 1;", &chunk, &ctx).unwrap();
        let code = result.unwrap();

        assert!(code.starts_with("/* Copyright 2024 */"));
        assert!(code.ends_with("/* End of bundle */"));
    }

    #[test]
    fn test_json_plugin() {
        let plugin = JsonPlugin;
        let ctx = PluginContext::default();

        // Should transform JSON
        let result = plugin
            .transform(r#"{"key": "value"}"#, "data.json", &ctx)
            .unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().code, r#"export default {"key": "value"};"#);

        // Should not transform JS
        let result = plugin.transform("const x = 1;", "index.js", &ctx).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_plugin_container() {
        let mut container = PluginContainer::default();

        container.add(Box::new(ReplacePlugin::new().replace("FOO", "BAR")));
        container.add(Box::new(ReplacePlugin::new().replace("BAR", "BAZ")));

        // Plugins chain: FOO -> BAR -> BAZ
        let result = container.transform("const x = FOO;", "test.js").unwrap();
        assert_eq!(result, "const x = BAZ;");
    }
}
