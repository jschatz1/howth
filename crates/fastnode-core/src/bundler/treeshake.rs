//! Tree shaking (dead code elimination).
//!
//! Analyzes which exports are actually used and marks unused code for removal.
//!
//! ## How it works
//!
//! 1. Start from entry point - all its exports are "used" (public API)
//! 2. For each import, mark the corresponding export as used
//! 3. Handle re-exports by tracing through to the original source
//! 4. Respect `sideEffects` field in package.json

#![allow(dead_code)]
#![allow(clippy::manual_is_ascii_check)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::option_map_or_none)]
#![allow(clippy::needless_continue)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::cast_possible_wrap)]

use super::graph::{ModuleGraph, ModuleId};
use super::Import;
use std::collections::{HashMap, HashSet, VecDeque};

/// Tracks which exports from each module are actually used.
#[derive(Debug, Default)]
pub struct UsedExports {
    /// Module ID -> Set of used export names.
    /// If a module has `None`, all exports are used (entry point or namespace import).
    used: HashMap<ModuleId, Option<HashSet<String>>>,
    /// Modules with side effects that must be kept regardless of imports.
    side_effect_modules: HashSet<ModuleId>,
    /// Re-export mappings: (module_id, export_name) -> (source_module_id, source_export_name)
    re_exports: HashMap<(ModuleId, String), (ModuleId, String)>,
}

impl UsedExports {
    /// Create a new UsedExports tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze the module graph and determine which exports are used.
    pub fn analyze(graph: &ModuleGraph, entry_id: ModuleId) -> Self {
        let mut used = Self::new();

        // First pass: extract re-exports from all modules
        used.extract_re_exports(graph);

        // Entry module: all exports are used (it's the public API)
        used.mark_all_used(entry_id);

        // BFS traversal to mark used exports
        let mut queue: VecDeque<ModuleId> = VecDeque::new();
        let mut visited: HashSet<ModuleId> = HashSet::new();

        queue.push_back(entry_id);
        visited.insert(entry_id);

        while let Some(module_id) = queue.pop_front() {
            if let Some(module) = graph.get(module_id) {
                for import in &module.imports {
                    // Find the target module
                    if let Some(target_id) =
                        graph.resolve_specifier(&module.path, &import.specifier)
                    {
                        // Process this import
                        used.process_import(graph, target_id, import);

                        // Add to queue if not visited
                        if !visited.contains(&target_id) {
                            visited.insert(target_id);
                            queue.push_back(target_id);
                        }
                    }
                }

                // Also process dynamic dependencies
                for &dep_id in &module.dynamic_dependencies {
                    if !visited.contains(&dep_id) {
                        visited.insert(dep_id);
                        queue.push_back(dep_id);
                        // Dynamic imports use all exports (we can't statically know what's used)
                        used.mark_all_used(dep_id);
                    }
                }
            }
        }

        // Mark side-effect imports
        used.mark_side_effect_imports(graph);

        used
    }

    /// Extract re-exports from all modules.
    fn extract_re_exports(&mut self, graph: &ModuleGraph) {
        for (module_id, module) in graph.iter() {
            let re_exports = extract_re_exports(&module.source);
            for (export_name, specifier, source_name) in re_exports {
                // Resolve the specifier to find the source module
                if let Some(source_id) = graph.resolve_specifier(&module.path, &specifier) {
                    self.re_exports
                        .insert((module_id, export_name), (source_id, source_name));
                }
            }
        }
    }

    /// Mark all exports of a module as used.
    pub fn mark_all_used(&mut self, module_id: ModuleId) {
        self.used.insert(module_id, None);
    }

    /// Mark specific exports as used.
    pub fn mark_used(&mut self, module_id: ModuleId, names: &[String]) {
        let entry = self
            .used
            .entry(module_id)
            .or_insert_with(|| Some(HashSet::new()));
        if let Some(set) = entry {
            for name in names {
                set.insert(name.clone());
            }
        }
    }

    /// Process an import statement and mark the appropriate exports as used.
    fn process_import(&mut self, graph: &ModuleGraph, target_id: ModuleId, import: &Import) {
        if import.names.is_empty() {
            // Side-effect import: import './module'
            // Don't mark all exports - just mark the module as having side effects
            self.side_effect_modules.insert(target_id);
        } else {
            // Named imports - mark specific exports as used
            for name in &import.names {
                self.mark_export_used(graph, target_id, &name.imported);
            }
        }
    }

    /// Mark an export as used, following re-exports to the source.
    #[allow(clippy::only_used_in_recursion)]
    fn mark_export_used(&mut self, graph: &ModuleGraph, module_id: ModuleId, export_name: &str) {
        // Check if this is a re-export
        if let Some((source_id, source_name)) = self
            .re_exports
            .get(&(module_id, export_name.to_string()))
            .cloned()
        {
            // Recursively mark the source export as used
            self.mark_export_used(graph, source_id, &source_name);
        }

        // Also mark on this module (it re-exports, so needs to keep the re-export statement)
        let entry = self
            .used
            .entry(module_id)
            .or_insert_with(|| Some(HashSet::new()));
        if let Some(set) = entry {
            set.insert(export_name.to_string());
        }
    }

    /// Mark modules that have side-effect imports (import './module' with no bindings).
    fn mark_side_effect_imports(&mut self, graph: &ModuleGraph) {
        for (_module_id, module) in graph.iter() {
            for import in &module.imports {
                // Side-effect import: no names imported
                if import.names.is_empty() && !import.dynamic {
                    if let Some(target_id) =
                        graph.resolve_specifier(&module.path, &import.specifier)
                    {
                        // Check if target module has side effects
                        // For now, assume all side-effect imports have side effects
                        // TODO: Read sideEffects field from package.json
                        self.side_effect_modules.insert(target_id);
                    }
                }
            }
        }
    }

    /// Check if a specific export is used.
    pub fn is_used(&self, module_id: ModuleId, export_name: &str) -> bool {
        // Side-effect modules are always used
        if self.side_effect_modules.contains(&module_id) {
            return true;
        }

        match self.used.get(&module_id) {
            None => false,      // Module not in graph, not used
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

    /// Check if a module has side effects and should be kept.
    pub fn has_side_effects(&self, module_id: ModuleId) -> bool {
        self.side_effect_modules.contains(&module_id)
    }

    /// Check if a module should be included in the bundle.
    pub fn should_include(&self, module_id: ModuleId) -> bool {
        self.used.contains_key(&module_id) || self.side_effect_modules.contains(&module_id)
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
            if let Some(name) = extract_var_name(trimmed) {
                exports.push((name, false));
            }
            continue;
        }

        // export function name
        if trimmed.starts_with("export function ") {
            if let Some(name) = extract_function_name(trimmed) {
                exports.push((name, false));
            }
            continue;
        }

        // export async function name
        if trimmed.starts_with("export async function ") {
            let decl = trimmed.strip_prefix("export async function ").unwrap();
            if let Some(paren_idx) = decl.find('(') {
                let name = decl[..paren_idx].trim();
                if !name.is_empty() {
                    exports.push((name.to_string(), false));
                }
            }
            continue;
        }

        // export class Name
        if trimmed.starts_with("export class ") {
            if let Some(name) = extract_class_name(trimmed) {
                exports.push((name, false));
            }
            continue;
        }

        // export { name1, name2 }
        if trimmed.starts_with("export {") && !trimmed.contains(" from ") {
            exports.extend(extract_named_exports(trimmed));
            continue;
        }

        // export { name1, name2 } from './module' - re-exports, handled separately
    }

    exports
}

/// Extract re-exports from source code.
/// Returns a list of (export_name, specifier, source_name) tuples.
pub fn extract_re_exports(source: &str) -> Vec<(String, String, String)> {
    let mut re_exports = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // export { name1, name2 } from './module'
        if trimmed.starts_with("export {") && trimmed.contains(" from ") {
            if let Some(from_idx) = trimmed.find(" from ") {
                let names_part = &trimmed[8..from_idx];
                let spec_part = &trimmed[from_idx + 6..];
                let specifier = spec_part
                    .trim()
                    .trim_matches(|c| c == '\'' || c == '"' || c == ';');

                // Parse names
                if let Some(end) = names_part.find('}') {
                    let names = &names_part[..end];
                    for name in names.split(',') {
                        let name = name.trim();
                        if name.is_empty() {
                            continue;
                        }
                        // Handle "foo as bar" syntax
                        if name.contains(" as ") {
                            let parts: Vec<&str> = name.split(" as ").collect();
                            if parts.len() == 2 {
                                let source_name = parts[0].trim().to_string();
                                let export_name = parts[1].trim().to_string();
                                re_exports.push((
                                    export_name,
                                    specifier.to_string(),
                                    source_name,
                                ));
                            }
                        } else {
                            // Same name for import and export
                            re_exports.push((
                                name.to_string(),
                                specifier.to_string(),
                                name.to_string(),
                            ));
                        }
                    }
                }
            }
            continue;
        }

        // export * from './module' - namespace re-export
        if trimmed.starts_with("export * from ") {
            let spec_part = trimmed.strip_prefix("export * from ").unwrap();
            let specifier = spec_part
                .trim()
                .trim_matches(|c| c == '\'' || c == '"' || c == ';');
            // Mark as "*" to indicate all exports
            re_exports.push(("*".to_string(), specifier.to_string(), "*".to_string()));
            continue;
        }

        // export * as ns from './module' - namespace export
        if trimmed.starts_with("export * as ") && trimmed.contains(" from ") {
            if let Some(as_idx) = trimmed.find(" as ") {
                if let Some(from_idx) = trimmed.find(" from ") {
                    let ns_name = trimmed[as_idx + 4..from_idx].trim();
                    let spec_part = &trimmed[from_idx + 6..];
                    let specifier = spec_part
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"' || c == ';');
                    re_exports.push((ns_name.to_string(), specifier.to_string(), "*".to_string()));
                }
            }
            continue;
        }
    }

    re_exports
}

/// Extract variable name from an export const/let/var declaration.
fn extract_var_name(line: &str) -> Option<String> {
    let decl = line.strip_prefix("export ")?.strip_prefix("const ").or_else(|| {
        line.strip_prefix("export ")?
            .strip_prefix("let ")
            .or_else(|| line.strip_prefix("export ")?.strip_prefix("var "))
    })?;

    // Handle destructuring: export const { a, b } = ...
    if decl.starts_with('{') {
        // For now, skip destructuring (would need to extract multiple names)
        return None;
    }

    // Handle array destructuring: export const [a, b] = ...
    if decl.starts_with('[') {
        return None;
    }

    // Regular declaration: export const foo = ...
    let parts: Vec<&str> = decl.splitn(2, |c: char| c == '=' || c == ':' || c == ' ').collect();
    if !parts.is_empty() {
        let name = parts[0].trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    None
}

/// Extract function name from an export function declaration.
fn extract_function_name(line: &str) -> Option<String> {
    let decl = line.strip_prefix("export function ")?;
    let paren_idx = decl.find('(')?;
    let name = decl[..paren_idx].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Extract class name from an export class declaration.
fn extract_class_name(line: &str) -> Option<String> {
    let decl = line.strip_prefix("export class ")?;
    let parts: Vec<&str> = decl
        .splitn(2, |c: char| matches!(c, ' ' | '{' | '<'))
        .collect();
    if !parts.is_empty() {
        let name = parts[0].trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// Extract named exports from export { ... } statement.
fn extract_named_exports(line: &str) -> Vec<(String, bool)> {
    let mut exports = Vec::new();

    if let Some(end) = line.find('}') {
        let start = line.find('{').unwrap_or(0) + 1;
        let names = &line[start..end];
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
                if trimmed.starts_with("export function ")
                    || trimmed.starts_with("export async function ")
                    || trimmed.starts_with("export class ")
                {
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

    if line.starts_with("export const ")
        || line.starts_with("export let ")
        || line.starts_with("export var ")
    {
        return extract_var_name(line);
    }

    if line.starts_with("export function ") {
        return extract_function_name(line);
    }

    if line.starts_with("export async function ") {
        let decl = line.strip_prefix("export async function ")?;
        let paren_idx = decl.find('(')?;
        return Some(decl[..paren_idx].trim().to_string());
    }

    if line.starts_with("export class ") {
        return extract_class_name(line);
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
export async function fetchData() { }
export class User {}
export default App;
export { x, y as z };
"#;
        let exports = extract_exports(source);
        let names: Vec<&str> = exports.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"add"));
        assert!(names.contains(&"fetchData"));
        assert!(names.contains(&"User"));
        assert!(names.contains(&"default"));
        assert!(names.contains(&"x"));
        assert!(names.contains(&"z"));
    }

    #[test]
    fn test_extract_re_exports() {
        let source = r#"
export { foo, bar as baz } from './utils';
export * from './helpers';
export * as utils from './utils';
"#;
        let re_exports = extract_re_exports(source);

        // Check foo re-export
        assert!(re_exports
            .iter()
            .any(|(e, s, src)| e == "foo" && s == "./utils" && src == "foo"));

        // Check bar as baz re-export
        assert!(re_exports
            .iter()
            .any(|(e, s, src)| e == "baz" && s == "./utils" && src == "bar"));

        // Check * re-export
        assert!(re_exports
            .iter()
            .any(|(e, s, _)| e == "*" && s == "./helpers"));

        // Check * as utils re-export
        assert!(re_exports
            .iter()
            .any(|(e, s, src)| e == "utils" && s == "./utils" && src == "*"));
    }

    #[test]
    fn test_filter_unused_exports() {
        let source = "export const used = 1;
export const unused = 2;
export function usedFn() { return 1; }
export function unusedFn() { return 2; }";

        let mut used = HashSet::new();
        used.insert("used".to_string());
        used.insert("usedFn".to_string());

        let filtered = filter_unused_exports(source, Some(&used));
        assert!(filtered.contains("export const used"));
        assert!(!filtered.contains("export const unused"));
        assert!(filtered.contains("export function usedFn"));
        assert!(!filtered.contains("export function unusedFn"));
    }

    #[test]
    fn test_filter_async_function() {
        let source = "export async function used() { return 1; }
export async function unused() { return 2; }";

        let mut used = HashSet::new();
        used.insert("used".to_string());

        let filtered = filter_unused_exports(source, Some(&used));
        assert!(filtered.contains("export async function used"));
        assert!(!filtered.contains("export async function unused"));
    }
}
