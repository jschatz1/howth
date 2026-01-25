//! Tree shaking (dead code elimination).
//!
//! Analyzes which exports are actually used and marks unused code for removal.

use super::graph::{ModuleGraph, ModuleId};
use super::Import;
use std::collections::{HashMap, HashSet};

/// Tracks which exports from each module are actually used.
#[derive(Debug, Default)]
pub struct UsedExports {
    /// Module ID -> Set of used export names.
    /// If a module has `None`, all exports are used (entry point or namespace import).
    used: HashMap<ModuleId, Option<HashSet<String>>>,
}

impl UsedExports {
    /// Create a new UsedExports tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze the module graph and determine which exports are used.
    pub fn analyze(graph: &ModuleGraph, entry_id: ModuleId) -> Self {
        let mut used = Self::new();

        // Entry module: all exports are used (it's the public API)
        used.mark_all_used(entry_id);

        // Traverse the graph and mark used exports based on imports
        let order = graph.toposort();
        for &module_id in &order {
            if let Some(module) = graph.get(module_id) {
                for import in &module.imports {
                    // Find the target module
                    if let Some(target_id) = graph.resolve_specifier(&module.path, &import.specifier) {
                        used.process_import(target_id, import);
                    }
                }
            }
        }

        used
    }

    /// Mark all exports of a module as used.
    pub fn mark_all_used(&mut self, module_id: ModuleId) {
        self.used.insert(module_id, None);
    }

    /// Mark specific exports as used.
    pub fn mark_used(&mut self, module_id: ModuleId, names: &[String]) {
        let entry = self.used.entry(module_id).or_insert_with(|| Some(HashSet::new()));
        if let Some(set) = entry {
            for name in names {
                set.insert(name.clone());
            }
        }
    }

    /// Process an import statement and mark the appropriate exports as used.
    fn process_import(&mut self, target_id: ModuleId, import: &Import) {
        if import.names.is_empty() {
            // Side-effect import or namespace import - all exports are used
            self.mark_all_used(target_id);
        } else {
            // Named imports - only specific exports are used
            let names: Vec<String> = import.names.iter().map(|n| n.imported.clone()).collect();
            self.mark_used(target_id, &names);
        }
    }

    /// Check if a specific export is used.
    pub fn is_used(&self, module_id: ModuleId, export_name: &str) -> bool {
        match self.used.get(&module_id) {
            None => false, // Module not in graph, not used
            Some(None) => true, // All exports are used
            Some(Some(set)) => set.contains(export_name),
        }
    }

    /// Check if all exports of a module are used (entry point or namespace import).
    pub fn all_used(&self, module_id: ModuleId) -> bool {
        matches!(self.used.get(&module_id), Some(None))
    }

    /// Get the set of used export names for a module.
    /// Returns None if all exports are used.
    pub fn get_used(&self, module_id: ModuleId) -> Option<&HashSet<String>> {
        self.used.get(&module_id).and_then(|opt| opt.as_ref())
    }
}

/// Extract export names from source code.
/// Returns a list of (export_name, is_default) tuples.
pub fn extract_exports(source: &str) -> Vec<(String, bool)> {
    let mut exports = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // export default
        if trimmed.starts_with("export default ") {
            exports.push(("default".to_string(), true));
            continue;
        }

        // export const/let/var name
        if trimmed.starts_with("export const ")
            || trimmed.starts_with("export let ")
            || trimmed.starts_with("export var ")
        {
            let decl = trimmed.strip_prefix("export ").unwrap();
            let parts: Vec<&str> = decl.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let name = parts[1].trim_end_matches(|c| c == '=' || c == ':' || c == ' ');
                exports.push((name.to_string(), false));
            }
            continue;
        }

        // export function name
        if trimmed.starts_with("export function ") {
            let decl = trimmed.strip_prefix("export function ").unwrap();
            if let Some(paren_idx) = decl.find('(') {
                let name = decl[..paren_idx].trim();
                exports.push((name.to_string(), false));
            }
            continue;
        }

        // export class Name
        if trimmed.starts_with("export class ") {
            let decl = trimmed.strip_prefix("export class ").unwrap();
            let parts: Vec<&str> = decl.splitn(2, |c| c == ' ' || c == '{').collect();
            if !parts.is_empty() {
                exports.push((parts[0].trim().to_string(), false));
            }
            continue;
        }

        // export { name1, name2 }
        if trimmed.starts_with("export {") {
            if let Some(end) = trimmed.find('}') {
                let names = &trimmed[8..end];
                for name in names.split(',') {
                    let name = name.trim();
                    if name.is_empty() {
                        continue;
                    }
                    // Handle "foo as bar" syntax
                    if name.contains(" as ") {
                        let parts: Vec<&str> = name.split(" as ").collect();
                        if parts.len() == 2 {
                            exports.push((parts[1].trim().to_string(), false));
                        }
                    } else {
                        exports.push((name.to_string(), false));
                    }
                }
            }
            continue;
        }
    }

    exports
}

/// Filter source code to only include used exports.
/// This is a simple line-based filter - for production use, AST-based would be better.
pub fn filter_unused_exports(source: &str, used_exports: Option<&HashSet<String>>) -> String {
    // If all exports are used (None), return source as-is
    let used = match used_exports {
        None => return source.to_string(),
        Some(set) => set,
    };

    let mut result = Vec::new();
    let mut skip_until_brace_close = false;
    let mut brace_depth = 0;

    for line in source.lines() {
        let trimmed = line.trim();

        // Handle multi-line skipping (for functions/classes)
        if skip_until_brace_close {
            for ch in trimmed.chars() {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        skip_until_brace_close = false;
                        break;
                    }
                }
            }
            continue;
        }

        // Check if this line exports something unused
        if let Some(export_name) = get_export_name(trimmed) {
            if !used.contains(&export_name) && export_name != "default" {
                // Skip this export
                // If it's a function/class, we need to skip until closing brace
                if trimmed.starts_with("export function ") || trimmed.starts_with("export class ") {
                    if trimmed.contains('{') && !trimmed.contains('}') {
                        skip_until_brace_close = true;
                        brace_depth = trimmed.chars().filter(|&c| c == '{').count() as i32
                            - trimmed.chars().filter(|&c| c == '}').count() as i32;
                    }
                }
                continue;
            }
        }

        result.push(line);
    }

    result.join("\n")
}

/// Get the export name from a line if it's an export statement.
fn get_export_name(line: &str) -> Option<String> {
    if line.starts_with("export default ") {
        return Some("default".to_string());
    }

    if line.starts_with("export const ") || line.starts_with("export let ") || line.starts_with("export var ") {
        let decl = line.strip_prefix("export ")?;
        let parts: Vec<&str> = decl.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let name = parts[1].trim_end_matches(|c| c == '=' || c == ':' || c == ' ');
            return Some(name.to_string());
        }
    }

    if line.starts_with("export function ") {
        let decl = line.strip_prefix("export function ")?;
        if let Some(paren_idx) = decl.find('(') {
            return Some(decl[..paren_idx].trim().to_string());
        }
    }

    if line.starts_with("export class ") {
        let decl = line.strip_prefix("export class ")?;
        let parts: Vec<&str> = decl.splitn(2, |c| c == ' ' || c == '{').collect();
        if !parts.is_empty() {
            return Some(parts[0].trim().to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_exports() {
        let source = r#"
export const foo = 1;
export let bar = 2;
export function add(a, b) { return a + b; }
export class User {}
export default App;
export { x, y as z };
"#;
        let exports = extract_exports(source);
        let names: Vec<&str> = exports.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"add"));
        assert!(names.contains(&"User"));
        assert!(names.contains(&"default"));
        assert!(names.contains(&"x"));
        assert!(names.contains(&"z"));
    }

    #[test]
    fn test_filter_unused_exports() {
        let source = r#"export const used = 1;
export const unused = 2;
export function usedFn() { return 1; }
export function unusedFn() { return 2; }"#;

        let mut used = HashSet::new();
        used.insert("used".to_string());
        used.insert("usedFn".to_string());

        let filtered = filter_unused_exports(source, Some(&used));
        assert!(filtered.contains("export const used"));
        assert!(!filtered.contains("export const unused"));
        assert!(filtered.contains("export function usedFn"));
        assert!(!filtered.contains("export function unusedFn"));
    }
}
