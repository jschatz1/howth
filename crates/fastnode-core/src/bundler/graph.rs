//! Module dependency graph.
//!
//! Tracks modules and their dependencies for bundling.

use super::Import;
use rustc_hash::FxHashMap as HashMap;
use std::path::Path;

/// Unique identifier for a module in the graph.
pub type ModuleId = usize;

/// A module in the dependency graph.
#[derive(Debug, Clone)]
pub struct Module {
    /// Absolute path to the module.
    pub path: String,
    /// Source code.
    pub source: String,
    /// Import statements found in the module.
    pub imports: Vec<Import>,
    /// Module IDs this module depends on (static imports).
    pub dependencies: Vec<ModuleId>,
    /// Module IDs this module dynamically imports (code split points).
    pub dynamic_dependencies: Vec<ModuleId>,
}

/// The module dependency graph.
#[derive(Debug, Default)]
pub struct ModuleGraph {
    /// All modules, indexed by ID.
    modules: Vec<Module>,
    /// Path to ID mapping for deduplication.
    path_to_id: HashMap<String, ModuleId>,
    /// Specifier resolution: (from_path, specifier) -> target_module_id.
    specifier_map: HashMap<(String, String), ModuleId>,
}

impl ModuleGraph {
    /// Create a new empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a module to the graph, returning its ID.
    pub fn add(&mut self, module: Module) -> ModuleId {
        let id = self.modules.len();
        self.path_to_id.insert(module.path.clone(), id);
        self.modules.push(module);
        id
    }

    /// Get a module by ID.
    #[must_use]
    pub fn get(&self, id: ModuleId) -> Option<&Module> {
        self.modules.get(id)
    }

    /// Get a module by path.
    #[must_use]
    pub fn get_by_path(&self, path: &Path) -> Option<(ModuleId, &Module)> {
        let path_str = path.display().to_string();
        self.path_to_id
            .get(&path_str)
            .map(|&id| (id, &self.modules[id]))
    }

    /// Get module ID by path.
    #[must_use]
    pub fn id_by_path(&self, path: &str) -> Option<ModuleId> {
        self.path_to_id.get(path).copied()
    }

    /// Number of modules in the graph.
    #[must_use]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Check if graph is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Set dependencies from a map of module path -> (specifier, resolved_path, is_dynamic) tuples.
    pub fn set_dependencies(&mut self, dep_info: &HashMap<String, Vec<(String, String, bool)>>) {
        for module in &mut self.modules {
            if let Some(deps) = dep_info.get(&module.path) {
                // Static dependencies
                module.dependencies = deps
                    .iter()
                    .filter(|(_, _, is_dynamic)| !is_dynamic)
                    .filter_map(|(_, dep_path, _)| self.path_to_id.get(dep_path).copied())
                    .collect();

                // Dynamic dependencies (code split points)
                module.dynamic_dependencies = deps
                    .iter()
                    .filter(|(_, _, is_dynamic)| *is_dynamic)
                    .filter_map(|(_, dep_path, _)| self.path_to_id.get(dep_path).copied())
                    .collect();

                // Also populate the specifier map
                for (specifier, dep_path, _) in deps {
                    if let Some(&target_id) = self.path_to_id.get(dep_path) {
                        self.specifier_map
                            .insert((module.path.clone(), specifier.clone()), target_id);
                    }
                }
            }
        }
    }

    /// Look up the module ID for a specifier from a given module.
    #[must_use]
    pub fn resolve_specifier(&self, from_path: &str, specifier: &str) -> Option<ModuleId> {
        self.specifier_map
            .get(&(from_path.to_string(), specifier.to_string()))
            .copied()
    }

    /// Get modules in topological order (dependencies before dependents).
    #[must_use]
    pub fn toposort(&self) -> Vec<ModuleId> {
        let n = self.modules.len();
        if n == 0 {
            return Vec::new();
        }

        // Build adjacency list and in-degree count
        let mut in_degree = vec![0usize; n];
        let mut adj: Vec<Vec<ModuleId>> = vec![Vec::new(); n];

        for (id, module) in self.modules.iter().enumerate() {
            for &dep_id in &module.dependencies {
                adj[dep_id].push(id);
                in_degree[id] += 1;
            }
        }

        // Kahn's algorithm
        let mut queue: std::collections::VecDeque<ModuleId> = std::collections::VecDeque::new();
        for (id, &deg) in in_degree.iter().enumerate() {
            if deg == 0 {
                queue.push_back(id);
            }
        }

        let mut order = Vec::with_capacity(n);
        while let Some(id) = queue.pop_front() {
            order.push(id);
            for &next in &adj[id] {
                in_degree[next] -= 1;
                if in_degree[next] == 0 {
                    queue.push_back(next);
                }
            }
        }

        // If we didn't get all modules, there's a cycle
        // For now, include remaining modules anyway (circular deps are allowed in JS)
        if order.len() < n {
            for id in 0..n {
                if !order.contains(&id) {
                    order.push(id);
                }
            }
        }

        order
    }

    /// Iterate over all modules.
    pub fn iter(&self) -> impl Iterator<Item = (ModuleId, &Module)> {
        self.modules.iter().enumerate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = ModuleGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_add_module() {
        let mut graph = ModuleGraph::new();
        let module = Module {
            path: "/test/index.ts".to_string(),
            source: "export const x = 1;".to_string(),
            imports: Vec::new(),
            dependencies: Vec::new(),
            dynamic_dependencies: Vec::new(),
        };
        let id = graph.add(module);
        assert_eq!(id, 0);
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_toposort_linear() {
        let mut graph = ModuleGraph::new();

        // A depends on B depends on C
        graph.add(Module {
            path: "/c.ts".to_string(),
            source: String::new(),
            imports: Vec::new(),
            dependencies: Vec::new(),
            dynamic_dependencies: Vec::new(),
        });
        graph.add(Module {
            path: "/b.ts".to_string(),
            source: String::new(),
            imports: Vec::new(),
            dependencies: vec![0], // depends on C
            dynamic_dependencies: Vec::new(),
        });
        graph.add(Module {
            path: "/a.ts".to_string(),
            source: String::new(),
            imports: Vec::new(),
            dependencies: vec![1], // depends on B
            dynamic_dependencies: Vec::new(),
        });

        let order = graph.toposort();
        // C should come before B, B before A
        assert_eq!(order, vec![0, 1, 2]);
    }
}
