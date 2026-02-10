//! Code splitting and chunk generation.
//!
//! Splits the module graph into chunks based on dynamic import boundaries.

use super::graph::{ModuleGraph, ModuleId};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// A chunk is a group of modules that are loaded together.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Unique chunk ID.
    pub id: ChunkId,
    /// Human-readable chunk name.
    pub name: String,
    /// Modules in this chunk (in topological order).
    pub modules: Vec<ModuleId>,
    /// Entry point for this chunk (the dynamically imported module).
    pub entry: ModuleId,
    /// Whether this is the main entry chunk.
    pub is_entry: bool,
    /// Chunks that this chunk depends on (must be loaded first).
    pub dependencies: Vec<ChunkId>,
}

/// Unique identifier for a chunk.
pub type ChunkId = usize;

/// Result of code splitting.
#[derive(Debug)]
pub struct ChunkGraph {
    /// All chunks.
    chunks: Vec<Chunk>,
    /// Module to chunk mapping.
    module_to_chunk: HashMap<ModuleId, ChunkId>,
    /// Shared modules (appear in multiple chunks' dependency trees).
    shared_modules: HashSet<ModuleId>,
}

impl ChunkGraph {
    /// Split the module graph into chunks based on dynamic imports.
    pub fn from_module_graph(graph: &ModuleGraph, entry_id: ModuleId) -> Self {
        let mut chunk_graph = ChunkGraph {
            chunks: Vec::new(),
            module_to_chunk: HashMap::default(),
            shared_modules: HashSet::default(),
        };

        // Find all dynamic import boundaries (split points)
        let split_points = find_split_points(graph);

        // Create chunks starting from entry point
        chunk_graph.create_chunks(graph, entry_id, &split_points);

        // Identify shared modules
        chunk_graph.identify_shared_modules(graph);

        chunk_graph
    }

    /// Create chunks by traversing the graph.
    fn create_chunks(
        &mut self,
        graph: &ModuleGraph,
        entry_id: ModuleId,
        split_points: &HashSet<ModuleId>,
    ) {
        // Main entry chunk
        let main_chunk_id = self.create_chunk("main".to_string(), entry_id, true);
        self.assign_modules_to_chunk(graph, entry_id, main_chunk_id, split_points);

        // Create chunks for each split point
        for &split_id in split_points {
            if let Some(module) = graph.get(split_id) {
                let chunk_name = generate_chunk_name(&module.path);
                let chunk_id = self.create_chunk(chunk_name, split_id, false);
                self.assign_modules_to_chunk(graph, split_id, chunk_id, split_points);

                // Add dependency from main chunk to this chunk
                self.chunks[main_chunk_id].dependencies.push(chunk_id);
            }
        }
    }

    /// Create a new chunk.
    fn create_chunk(&mut self, name: String, entry: ModuleId, is_entry: bool) -> ChunkId {
        let id = self.chunks.len();
        self.chunks.push(Chunk {
            id,
            name,
            modules: Vec::new(),
            entry,
            is_entry,
            dependencies: Vec::new(),
        });
        id
    }

    /// Assign modules to a chunk by following static dependencies.
    fn assign_modules_to_chunk(
        &mut self,
        graph: &ModuleGraph,
        start: ModuleId,
        chunk_id: ChunkId,
        split_points: &HashSet<ModuleId>,
    ) {
        let mut visited = HashSet::default();
        let mut stack = vec![start];

        while let Some(module_id) = stack.pop() {
            if visited.contains(&module_id) {
                continue;
            }
            visited.insert(module_id);

            // Don't cross into other chunks (split points)
            if module_id != start && split_points.contains(&module_id) {
                continue;
            }

            // Assign module to chunk
            self.module_to_chunk.insert(module_id, chunk_id);
            self.chunks[chunk_id].modules.push(module_id);

            // Follow static dependencies (not dynamic ones)
            if let Some(module) = graph.get(module_id) {
                for &dep_id in &module.dependencies {
                    if !visited.contains(&dep_id) {
                        stack.push(dep_id);
                    }
                }
            }
        }
    }

    /// Identify modules that are shared between multiple chunks.
    fn identify_shared_modules(&mut self, graph: &ModuleGraph) {
        // Count how many chunks reference each module
        let mut module_usage: HashMap<ModuleId, HashSet<ChunkId>> = HashMap::default();

        for chunk in &self.chunks {
            // Get all modules reachable from this chunk
            let reachable = get_all_dependencies(graph, chunk.entry);
            for module_id in reachable {
                module_usage.entry(module_id).or_default().insert(chunk.id);
            }
        }

        // Modules used by more than one chunk are shared
        for (module_id, chunks) in module_usage {
            if chunks.len() > 1 {
                self.shared_modules.insert(module_id);
            }
        }
    }

    /// Get all chunks.
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Get the main entry chunk.
    pub fn main_chunk(&self) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.is_entry)
    }

    /// Get async chunks (non-entry chunks).
    pub fn async_chunks(&self) -> Vec<&Chunk> {
        self.chunks.iter().filter(|c| !c.is_entry).collect()
    }

    /// Get the chunk containing a module.
    pub fn chunk_for_module(&self, module_id: ModuleId) -> Option<ChunkId> {
        self.module_to_chunk.get(&module_id).copied()
    }

    /// Check if a module is shared between chunks.
    pub fn is_shared(&self, module_id: ModuleId) -> bool {
        self.shared_modules.contains(&module_id)
    }

    /// Get shared modules.
    pub fn shared_modules(&self) -> &HashSet<ModuleId> {
        &self.shared_modules
    }

    /// Check if code splitting is needed.
    pub fn has_splits(&self) -> bool {
        self.chunks.len() > 1
    }

    /// Generate a manifest for the chunk graph.
    pub fn generate_manifest(&self, graph: &ModuleGraph) -> ChunkManifest {
        ChunkManifest {
            chunks: self
                .chunks
                .iter()
                .map(|chunk| ChunkInfo {
                    id: chunk.id,
                    name: chunk.name.clone(),
                    file: format!("{}.js", chunk.name),
                    is_entry: chunk.is_entry,
                    modules: chunk
                        .modules
                        .iter()
                        .filter_map(|&id| graph.get(id).map(|m| m.path.clone()))
                        .collect(),
                    dependencies: chunk.dependencies.clone(),
                })
                .collect(),
        }
    }
}

/// Find all split points (targets of dynamic imports).
fn find_split_points(graph: &ModuleGraph) -> HashSet<ModuleId> {
    let mut split_points = HashSet::default();

    for (_, module) in graph.iter() {
        for &dynamic_dep in &module.dynamic_dependencies {
            split_points.insert(dynamic_dep);
        }
    }

    split_points
}

/// Get all static dependencies of a module recursively.
fn get_all_dependencies(graph: &ModuleGraph, start: ModuleId) -> HashSet<ModuleId> {
    let mut deps = HashSet::default();
    let mut stack = vec![start];

    while let Some(module_id) = stack.pop() {
        if deps.contains(&module_id) {
            continue;
        }
        deps.insert(module_id);

        if let Some(module) = graph.get(module_id) {
            for &dep_id in &module.dependencies {
                if !deps.contains(&dep_id) {
                    stack.push(dep_id);
                }
            }
        }
    }

    deps
}

/// Generate a chunk name from a file path.
fn generate_chunk_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("chunk")
        .to_string()
}

/// Chunk manifest for runtime loading.
#[derive(Debug, Clone)]
pub struct ChunkManifest {
    /// Information about each chunk.
    pub chunks: Vec<ChunkInfo>,
}

/// Information about a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    /// Chunk ID.
    pub id: ChunkId,
    /// Chunk name.
    pub name: String,
    /// Output file name.
    pub file: String,
    /// Whether this is the entry chunk.
    pub is_entry: bool,
    /// Modules in this chunk.
    pub modules: Vec<String>,
    /// Chunk IDs this chunk depends on.
    pub dependencies: Vec<ChunkId>,
}

impl ChunkManifest {
    /// Serialize manifest to JSON.
    pub fn to_json(&self) -> String {
        let mut json = String::from("{\n  \"chunks\": [\n");

        for (i, chunk) in self.chunks.iter().enumerate() {
            json.push_str("    {\n");
            json.push_str(&format!("      \"id\": {},\n", chunk.id));
            json.push_str(&format!("      \"name\": \"{}\",\n", chunk.name));
            json.push_str(&format!("      \"file\": \"{}\",\n", chunk.file));
            json.push_str(&format!("      \"isEntry\": {},\n", chunk.is_entry));
            json.push_str(&format!(
                "      \"dependencies\": [{}]\n",
                chunk
                    .dependencies
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            json.push_str("    }");
            if i < self.chunks.len() - 1 {
                json.push(',');
            }
            json.push('\n');
        }

        json.push_str("  ]\n}");
        json
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundler::graph::Module;

    #[test]
    fn test_single_chunk_no_dynamic() {
        let mut graph = ModuleGraph::new();
        graph.add(Module {
            path: "/index.ts".to_string(),
            source: "".to_string(),
            imports: Vec::new(),
            dependencies: vec![],
            dynamic_dependencies: Vec::new(),
        });

        let chunks = ChunkGraph::from_module_graph(&graph, 0);
        assert_eq!(chunks.chunks().len(), 1);
        assert!(!chunks.has_splits());
    }

    #[test]
    fn test_code_split_dynamic_import() {
        let mut graph = ModuleGraph::new();

        // Entry module with dynamic import
        graph.add(Module {
            path: "/index.ts".to_string(),
            source: "".to_string(),
            imports: Vec::new(),
            dependencies: vec![],
            dynamic_dependencies: vec![1], // Dynamic import to lazy.ts
        });

        // Lazily loaded module
        graph.add(Module {
            path: "/lazy.ts".to_string(),
            source: "".to_string(),
            imports: Vec::new(),
            dependencies: vec![],
            dynamic_dependencies: Vec::new(),
        });

        let chunks = ChunkGraph::from_module_graph(&graph, 0);
        assert_eq!(chunks.chunks().len(), 2);
        assert!(chunks.has_splits());

        // Main chunk should have entry
        let main = chunks.main_chunk().unwrap();
        assert!(main.is_entry);
        assert!(main.modules.contains(&0));

        // Async chunk should have lazy module
        let async_chunks = chunks.async_chunks();
        assert_eq!(async_chunks.len(), 1);
        assert!(async_chunks[0].modules.contains(&1));
    }
}
