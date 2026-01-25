//! JavaScript/TypeScript bundler.
//!
//! Bundles multiple modules into a single output file.
//!
//! ## Usage
//!
//! ```ignore
//! use fastnode_core::bundler::{Bundler, BundleOptions};
//!
//! let bundler = Bundler::new();
//! let result = bundler.bundle("src/index.ts", &options)?;
//! std::fs::write("dist/bundle.js", result.code)?;
//! ```
//!
//! ## Architecture
//!
//! 1. **Resolution** - Resolve import specifiers to file paths
//! 2. **Graph** - Build dependency graph from entry point
//! 3. **Transform** - Transpile each module (TS → JS, JSX → JS)
//! 4. **Emit** - Concatenate modules into single output

mod graph;
mod resolve;
mod emit;
mod treeshake;
mod chunks;
mod assets;
mod plugin;

pub use graph::{ModuleGraph, ModuleId, Module};
pub use resolve::{Resolver, ResolveResult, ResolveError};
pub use emit::{emit_bundle, emit_bundle_with_entry, BundleOutput, BundleFormat};
pub use treeshake::UsedExports;
pub use chunks::{ChunkGraph, Chunk, ChunkId, ChunkManifest};
pub use assets::{Asset, AssetCollection, AssetType};
pub use plugin::{
    Plugin, PluginContainer, PluginContext, PluginError, HookResult,
    ResolveIdResult, LoadResult, TransformResult, ChunkInfo,
    // Built-in plugins
    ReplacePlugin, VirtualPlugin, AliasPlugin, BannerPlugin, JsonPlugin,
};

use std::path::Path;

/// Bundle options.
#[derive(Debug, Clone)]
pub struct BundleOptions {
    /// Output format (ESM or CJS).
    pub format: BundleFormat,
    /// Minify output.
    pub minify: bool,
    /// Generate source maps.
    pub sourcemap: bool,
    /// External packages (don't bundle, keep as imports).
    pub external: Vec<String>,
    /// Target environment.
    pub target: crate::compiler::Target,
    /// Enable tree shaking (dead code elimination).
    pub treeshake: bool,
    /// Enable code splitting on dynamic imports.
    pub splitting: bool,
}

impl Default for BundleOptions {
    fn default() -> Self {
        Self {
            format: BundleFormat::Esm,
            minify: false,
            sourcemap: false,
            external: Vec::new(),
            target: crate::compiler::Target::ES2020,
            treeshake: true, // Enable by default
            splitting: false, // Disabled by default
        }
    }
}

/// Bundle result.
#[derive(Debug)]
pub struct BundleResult {
    /// Bundled code (main chunk).
    pub code: String,
    /// Source map (if enabled).
    pub map: Option<String>,
    /// Modules included in bundle.
    pub modules: Vec<String>,
    /// Warnings during bundling.
    pub warnings: Vec<String>,
    /// Additional chunks (for code splitting).
    pub chunks: Vec<ChunkOutput>,
    /// Chunk manifest (for code splitting).
    pub manifest: Option<ChunkManifest>,
    /// Bundled CSS (if any CSS was imported).
    pub css: Option<CssOutput>,
    /// Static assets to copy.
    pub assets: Vec<AssetOutput>,
}

/// CSS output.
#[derive(Debug, Clone)]
pub struct CssOutput {
    /// Output filename.
    pub name: String,
    /// CSS content.
    pub code: String,
}

/// Asset output.
#[derive(Debug, Clone)]
pub struct AssetOutput {
    /// Output filename (with hash).
    pub name: String,
    /// Source path to copy from.
    pub source: std::path::PathBuf,
}

/// Output for a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkOutput {
    /// Chunk name.
    pub name: String,
    /// Chunk code.
    pub code: String,
    /// Source map (if enabled).
    pub map: Option<String>,
}

/// Bundler error.
#[derive(Debug)]
pub struct BundleError {
    pub code: &'static str,
    pub message: String,
    pub path: Option<String>,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}: {} ({})", self.code, self.message, path)
        } else {
            write!(f, "{}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for BundleError {}

impl From<ResolveError> for BundleError {
    fn from(err: ResolveError) -> Self {
        BundleError {
            code: "BUNDLE_RESOLVE_ERROR",
            message: err.message,
            path: Some(err.from),
        }
    }
}

pub type BundleResult2 = Result<BundleResult, BundleError>;

/// The main bundler.
pub struct Bundler {
    resolver: Resolver,
    plugins: PluginContainer,
}

impl Bundler {
    /// Create a new bundler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            resolver: Resolver::new(),
            plugins: PluginContainer::default(),
        }
    }

    /// Create a bundler with the given working directory.
    pub fn with_cwd(cwd: &Path) -> Self {
        Self {
            resolver: Resolver::new(),
            plugins: PluginContainer::new(cwd.to_path_buf()),
        }
    }

    /// Add a plugin to the bundler.
    pub fn plugin(mut self, plugin: Box<dyn Plugin>) -> Self {
        self.plugins.add(plugin);
        self
    }

    /// Add multiple plugins at once.
    pub fn plugins(mut self, plugins: Vec<Box<dyn Plugin>>) -> Self {
        for plugin in plugins {
            self.plugins.add(plugin);
        }
        self
    }

    /// Get mutable access to the plugin container.
    pub fn plugins_mut(&mut self) -> &mut PluginContainer {
        &mut self.plugins
    }

    /// Bundle from an entry point.
    pub fn bundle(&self, entry: &Path, cwd: &Path, options: &BundleOptions) -> BundleResult2 {
        // 0. Call build_start hook
        self.plugins.build_start().map_err(|e| BundleError {
            code: "PLUGIN_ERROR",
            message: e.to_string(),
            path: None,
        })?;

        // 1. Build module graph starting from entry
        let mut graph = ModuleGraph::new();
        let entry_id = self.build_graph(entry, cwd, &mut graph, options)?;

        // 2. Check if code splitting is enabled and there are dynamic imports
        if options.splitting {
            let chunk_graph = ChunkGraph::from_module_graph(&graph, entry_id);

            if chunk_graph.has_splits() {
                let result = self.bundle_with_splitting(&graph, &chunk_graph, options)?;
                // Call build_end hook
                self.plugins.build_end().map_err(|e| BundleError {
                    code: "PLUGIN_ERROR",
                    message: e.to_string(),
                    path: None,
                })?;
                return Ok(result);
            }
        }

        // 3. Get modules in topological order (no splitting)
        let order = graph.toposort();

        // 4. Emit bundled output
        let output = emit_bundle(&graph, &order, options)?;

        // 5. Apply render_chunk hook if plugins are registered
        let final_code = if self.plugins.has_plugins() {
            let chunk_info = ChunkInfo {
                name: "main".to_string(),
                is_entry: true,
                modules: order.iter().filter_map(|id| graph.get(*id).map(|m| m.path.clone())).collect(),
            };
            self.plugins.render_chunk(&output.code, &chunk_info).map_err(|e| BundleError {
                code: "PLUGIN_ERROR",
                message: e.to_string(),
                path: None,
            })?
        } else {
            output.code
        };

        // 6. Collect CSS and assets
        let (css, asset_outputs) = self.collect_assets(&graph, cwd)?;

        // 7. Call build_end hook
        self.plugins.build_end().map_err(|e| BundleError {
            code: "PLUGIN_ERROR",
            message: e.to_string(),
            path: None,
        })?;

        Ok(BundleResult {
            code: final_code,
            map: output.map,
            modules: order.iter().map(|id| graph.get(*id).unwrap().path.clone()).collect(),
            warnings: Vec::new(),
            chunks: Vec::new(),
            manifest: None,
            css,
            assets: asset_outputs,
        })
    }

    /// Bundle with code splitting enabled.
    fn bundle_with_splitting(
        &self,
        graph: &ModuleGraph,
        chunk_graph: &ChunkGraph,
        options: &BundleOptions,
    ) -> BundleResult2 {
        let mut main_code = String::new();
        let mut chunk_outputs = Vec::new();
        let mut all_modules = Vec::new();

        // Generate chunk loader runtime
        main_code.push_str(&generate_chunk_loader_runtime(chunk_graph));

        // Emit main chunk with its entry point
        if let Some(main_chunk) = chunk_graph.main_chunk() {
            let output = emit_bundle_with_entry(graph, &main_chunk.modules, options, Some(main_chunk.entry))?;
            main_code.push_str(&output.code);
            all_modules.extend(
                main_chunk.modules.iter()
                    .filter_map(|id| graph.get(*id).map(|m| m.path.clone()))
            );
        }

        // Emit async chunks with their entry points
        for chunk in chunk_graph.async_chunks() {
            let output = emit_bundle_with_entry(graph, &chunk.modules, options, Some(chunk.entry))?;
            chunk_outputs.push(ChunkOutput {
                name: chunk.name.clone(),
                code: output.code,
                map: output.map,
            });
            all_modules.extend(
                chunk.modules.iter()
                    .filter_map(|id| graph.get(*id).map(|m| m.path.clone()))
            );
        }

        // Generate manifest
        let manifest = chunk_graph.generate_manifest(graph);

        Ok(BundleResult {
            code: main_code,
            map: None,
            modules: all_modules,
            warnings: Vec::new(),
            chunks: chunk_outputs,
            manifest: Some(manifest),
            css: None,    // TODO: collect CSS in splitting mode
            assets: Vec::new(),
        })
    }

    /// Collect CSS and assets from the module graph.
    fn collect_assets(&self, graph: &ModuleGraph, cwd: &Path) -> Result<(Option<CssOutput>, Vec<AssetOutput>), BundleError> {
        let mut collection = AssetCollection::new();

        for (_, module) in graph.iter() {
            for import in &module.imports {
                // Check if this is a CSS or asset import
                if let Some(resolved) = self.try_resolve_asset(&import.specifier, &module.path, cwd) {
                    let ext = resolved.extension().and_then(|e| e.to_str()).unwrap_or("");

                    if AssetType::is_css(ext) {
                        // Read and process CSS
                        if let Ok(content) = std::fs::read_to_string(&resolved) {
                            let processed = assets::process_css(&content);
                            collection.add_css(&resolved, processed);
                        }
                    } else if AssetType::is_asset(ext) {
                        // Read asset for hashing
                        if let Ok(content) = std::fs::read(&resolved) {
                            collection.add_asset(&resolved, &content);
                        }
                    }
                }
            }
        }

        // Build outputs
        let css = if collection.has_css() {
            Some(CssOutput {
                name: collection.get_css_output_name().unwrap_or_else(|| "styles.css".to_string()),
                code: collection.get_bundled_css(),
            })
        } else {
            None
        };

        let assets = collection.get_assets()
            .map(|a| AssetOutput {
                name: a.output_name.clone(),
                source: a.source_path.clone(),
            })
            .collect();

        Ok((css, assets))
    }

    /// Try to resolve an import as an asset.
    fn try_resolve_asset(&self, specifier: &str, from: &str, cwd: &Path) -> Option<std::path::PathBuf> {
        // Only handle relative imports for now
        if !specifier.starts_with('.') {
            return None;
        }

        let from_path = Path::new(from);
        let from_dir = from_path.parent()?;

        let resolved = from_dir.join(specifier);
        let resolved = if resolved.is_absolute() {
            resolved
        } else {
            cwd.join(&resolved)
        };

        // Check if file exists and is an asset type
        if resolved.exists() {
            let ext = resolved.extension().and_then(|e| e.to_str())?;
            if AssetType::is_asset(ext) || AssetType::is_css(ext) {
                return Some(resolved);
            }
        }

        None
    }

    /// Build the module graph recursively.
    fn build_graph(
        &self,
        entry: &Path,
        cwd: &Path,
        graph: &mut ModuleGraph,
        options: &BundleOptions,
    ) -> Result<ModuleId, BundleError> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let entry_path = if entry.is_absolute() {
            entry.to_path_buf()
        } else {
            cwd.join(entry)
        };

        let entry_path = entry_path.canonicalize().map_err(|e| BundleError {
            code: "BUNDLE_ENTRY_NOT_FOUND",
            message: format!("Cannot find entry point: {}", e),
            path: Some(entry.display().to_string()),
        })?;

        // Track dependency info for each module: (specifier, resolved_path, is_dynamic)
        let mut dep_info: HashMap<String, Vec<(String, String, bool)>> = HashMap::new();
        // Track external modules (resolved by plugins) - for future use in warnings/manifest
        #[allow(unused_mut)]
        let mut externals: HashSet<String> = HashSet::new();

        let mut queue: VecDeque<std::path::PathBuf> = VecDeque::new();
        queue.push_back(entry_path.clone());

        while let Some(path) = queue.pop_front() {
            let path_str = path.display().to_string();

            // Skip if already processed
            if graph.id_by_path(&path_str).is_some() {
                continue;
            }

            // Try plugin load hook first, then fall back to file system
            let source = if let Some(load_result) = self.plugins.load(&path_str).map_err(|e| BundleError {
                code: "PLUGIN_ERROR",
                message: e.to_string(),
                path: Some(path_str.clone()),
            })? {
                load_result.code
            } else {
                std::fs::read_to_string(&path).map_err(|e| BundleError {
                    code: "BUNDLE_READ_ERROR",
                    message: e.to_string(),
                    path: Some(path_str.clone()),
                })?
            };

            // Apply plugin transform hook
            let source = if self.plugins.has_plugins() {
                self.plugins.transform(&source, &path_str).map_err(|e| BundleError {
                    code: "PLUGIN_ERROR",
                    message: e.to_string(),
                    path: Some(path_str.clone()),
                })?
            } else {
                source
            };

            // Extract imports
            let imports = self.extract_imports(&source, &path)?;

            // Resolve imports to file paths
            let mut module_deps: Vec<(String, String, bool)> = Vec::new();
            for import in &imports {
                // Check if external
                if options.external.iter().any(|e| import.specifier.starts_with(e)) {
                    continue;
                }

                // Try plugin resolve hook first
                if let Some(resolved) = self.plugins.resolve_id(&import.specifier, Some(&path_str)).map_err(|e| BundleError {
                    code: "PLUGIN_ERROR",
                    message: e.to_string(),
                    path: Some(path_str.clone()),
                })? {
                    if resolved.external {
                        externals.insert(resolved.id);
                        continue;
                    }

                    // Plugin resolved to a module ID - try to use it directly or with extension resolution
                    let dep_path = std::path::PathBuf::from(&resolved.id);

                    // If the resolved path exists directly, use it
                    if dep_path.exists() {
                        let dep_str = resolved.id.clone();
                        module_deps.push((import.specifier.clone(), dep_str.clone(), import.dynamic));
                        if graph.id_by_path(&dep_str).is_none() && !queue.iter().any(|p| p.display().to_string() == dep_str) {
                            queue.push_back(dep_path);
                        }
                        continue;
                    }

                    // Otherwise, try extension resolution via the default resolver
                    // This handles cases like alias "@/utils/math" -> "/path/src/utils/math" -> "/path/src/utils/math.ts"
                    if let Ok(ResolveResult::Found(resolved_path)) = self.resolver.resolve(&resolved.id, &path, cwd) {
                        let dep_str = resolved_path.display().to_string();
                        module_deps.push((import.specifier.clone(), dep_str.clone(), import.dynamic));
                        if graph.id_by_path(&dep_str).is_none() && !queue.iter().any(|p| p.display().to_string() == dep_str) {
                            queue.push_back(resolved_path);
                        }
                        continue;
                    }

                    // Fallback: use the plugin-resolved path as-is (will fail on load if it doesn't exist)
                    let dep_str = resolved.id.clone();
                    module_deps.push((import.specifier.clone(), dep_str.clone(), import.dynamic));
                    if graph.id_by_path(&dep_str).is_none() && !queue.iter().any(|p| p.display().to_string() == dep_str) {
                        queue.push_back(dep_path);
                    }
                    continue;
                }

                // Fall back to default resolver
                let resolved = self.resolver.resolve(&import.specifier, &path, cwd)?;

                if let ResolveResult::Found(dep_path) = resolved {
                    // Skip CSS and asset files - they're collected separately
                    let ext = dep_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if AssetType::is_css(ext) || AssetType::is_asset(ext) {
                        continue;
                    }

                    let dep_str = dep_path.display().to_string();
                    module_deps.push((import.specifier.clone(), dep_str.clone(), import.dynamic));
                    if graph.id_by_path(&dep_str).is_none() && !queue.iter().any(|p| p.display().to_string() == dep_str) {
                        queue.push_back(dep_path);
                    }
                }
            }

            // Store dependency info for later resolution
            dep_info.insert(path_str.clone(), module_deps);

            // Add module to graph (dependencies will be set in second pass)
            let module = Module {
                path: path_str,
                source,
                imports,
                dependencies: Vec::new(),
                dynamic_dependencies: Vec::new(),
            };
            graph.add(module);
        }

        // Second pass: resolve dependency paths to IDs
        graph.set_dependencies(&dep_info);

        // Return entry module ID
        graph.get_by_path(&entry_path)
            .map(|m| m.0)
            .ok_or_else(|| BundleError {
                code: "BUNDLE_INTERNAL_ERROR",
                message: "Entry module not found after graph build".to_string(),
                path: None,
            })
    }

    /// Extract import statements from source.
    fn extract_imports(&self, source: &str, path: &Path) -> Result<Vec<Import>, BundleError> {
        use crate::compiler::parse_imports;

        parse_imports(source, path).map_err(|e| BundleError {
            code: "BUNDLE_PARSE_ERROR",
            message: e.to_string(),
            path: Some(path.display().to_string()),
        })
    }
}

impl Default for Bundler {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate the runtime code for loading chunks dynamically.
fn generate_chunk_loader_runtime(chunk_graph: &ChunkGraph) -> String {
    let mut runtime = String::new();

    runtime.push_str("// Chunk loading runtime\n");
    runtime.push_str("const __chunks = {};\n");
    runtime.push_str("const __chunkLoading = {};\n\n");

    // Build chunk map
    runtime.push_str("const __chunkMap = {\n");
    for chunk in chunk_graph.async_chunks() {
        runtime.push_str(&format!("  {}: \"{}.js\",\n", chunk.id, chunk.name));
    }
    runtime.push_str("};\n\n");

    // Chunk loading function
    runtime.push_str(r#"function __loadChunk(id) {
  if (__chunks[id]) return Promise.resolve(__chunks[id]);
  if (__chunkLoading[id]) return __chunkLoading[id];

  const file = __chunkMap[id];
  if (!file) return Promise.reject(new Error("Unknown chunk: " + id));

  __chunkLoading[id] = import("./" + file).then(chunk => {
    __chunks[id] = chunk;
    delete __chunkLoading[id];
    return chunk;
  });

  return __chunkLoading[id];
}

"#);

    runtime
}

/// An import statement.
#[derive(Debug, Clone)]
pub struct Import {
    /// The import specifier (e.g., "./utils", "lodash", "@scope/pkg").
    pub specifier: String,
    /// Whether this is a dynamic import().
    pub dynamic: bool,
    /// Imported names (for tree shaking later).
    pub names: Vec<ImportedName>,
}

/// An imported name.
#[derive(Debug, Clone)]
pub struct ImportedName {
    /// The exported name from the module.
    pub imported: String,
    /// The local binding name.
    pub local: String,
}
