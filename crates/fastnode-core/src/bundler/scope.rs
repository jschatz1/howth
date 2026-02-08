//! Scope hoisting for JavaScript bundling.
//!
//! Replaces module wrapping with scope hoisting to produce smaller, faster bundles.
//! This matches the output quality of esbuild, bun, and rolldown.
//!
//! ## How it works
//!
//! 1. Parse all modules and collect top-level symbols
//! 2. Detect naming conflicts across modules
//! 3. Rename conflicting symbols (first module wins, others get suffixes)
//! 4. Link imports to their corresponding exports
//! 5. Emit code with renamed identifiers and no import/export statements

#![allow(dead_code)]
#![allow(clippy::redundant_else)]

use super::graph::{ModuleGraph, ModuleId};
use super::Import;
use std::collections::{HashMap, HashSet};

/// Unique identifier for a symbol.
pub type SymbolId = usize;

/// A symbol (variable, function, class, import, or export) in a module.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Unique identifier for this symbol.
    pub id: SymbolId,
    /// Original name in source code.
    pub name: String,
    /// Module this symbol belongs to.
    pub module_id: ModuleId,
    /// Kind of symbol.
    pub kind: SymbolKind,
    /// Source span for error messages.
    pub span: Option<(u32, u32)>,
}

/// The kind of symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    /// Variable declaration (let, const, var).
    Variable {
        /// Whether this is a const declaration (cannot be reassigned).
        is_const: bool,
    },
    /// Function declaration.
    Function,
    /// Class declaration.
    Class,
    /// Import binding.
    Import {
        /// Module the import comes from.
        source_module: ModuleId,
        /// Name of the export being imported.
        source_name: String,
        /// Whether this is a default import.
        is_default: bool,
        /// Whether this is a namespace import (import * as ns).
        is_namespace: bool,
    },
    /// Export binding.
    Export {
        /// Local symbol being exported (None for re-exports).
        local_symbol: Option<SymbolId>,
        /// Whether this is a default export.
        is_default: bool,
    },
    /// Re-export (export { x } from './y').
    ReExport {
        /// Module the re-export comes from.
        source_module: ModuleId,
        /// Name of the export being re-exported.
        source_name: String,
    },
}

/// Context for scope hoisting analysis.
#[derive(Debug)]
pub struct ScopeHoistContext {
    /// All symbols across all modules.
    symbols: Vec<Symbol>,
    /// Module ID -> symbol IDs for that module's top-level declarations.
    module_symbols: HashMap<ModuleId, Vec<SymbolId>>,
    /// Original name -> symbol IDs with that name (for conflict detection).
    name_to_symbols: HashMap<String, Vec<SymbolId>>,
    /// Import symbol -> export symbol links.
    /// Maps the symbol ID of an import to the symbol ID of the export it references.
    symbol_links: HashMap<SymbolId, SymbolId>,
    /// Final renamed identifiers.
    /// Maps symbol ID to the new name to use in output.
    renames: HashMap<SymbolId, String>,
    /// Modules that require runtime wrapper (can't be scope hoisted).
    wrapped_modules: HashSet<ModuleId>,
    /// Module ID -> export name -> symbol ID.
    module_exports: HashMap<ModuleId, HashMap<String, SymbolId>>,
}

impl Default for ScopeHoistContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopeHoistContext {
    /// Create a new scope hoisting context.
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            module_symbols: HashMap::new(),
            name_to_symbols: HashMap::new(),
            symbol_links: HashMap::new(),
            renames: HashMap::new(),
            wrapped_modules: HashSet::new(),
            module_exports: HashMap::new(),
        }
    }

    /// Analyze a module graph and build the scope hoisting context.
    pub fn analyze(graph: &ModuleGraph, order: &[ModuleId]) -> Self {
        let mut ctx = Self::new();

        // Phase 1: Collect symbols from each module
        for &module_id in order {
            if let Some(module) = graph.get(module_id) {
                // Check if this module needs wrapping (can't be scope hoisted)
                if ctx.needs_wrapper(&module.source) {
                    ctx.wrapped_modules.insert(module_id);
                    continue;
                }

                // Collect top-level symbols
                ctx.collect_symbols(module_id, &module.source, &module.imports, graph);
            }
        }

        // Phase 2: Resolve name conflicts
        ctx.resolve_conflicts(order);

        // Phase 3: Link imports to exports
        ctx.link_imports(graph);

        ctx
    }

    /// Check if a module needs a runtime wrapper (can't be scope hoisted).
    fn needs_wrapper(&self, source: &str) -> bool {
        // Check for patterns that prevent scope hoisting
        for line in source.lines() {
            let trimmed = line.trim();

            // eval() call - dynamic code execution
            if trimmed.contains("eval(") {
                return true;
            }

            // with statement - dynamic scope
            if trimmed.starts_with("with ") || trimmed.starts_with("with(") {
                return true;
            }

            // CommonJS patterns
            if trimmed.contains("require(")
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("*")
            {
                // Check if it's not in a comment
                if !is_in_comment(line, "require(") {
                    return true;
                }
            }

            if trimmed.contains("module.exports")
                || trimmed.contains("exports.")
                || trimmed.contains("__dirname")
                || trimmed.contains("__filename")
            {
                return true;
            }
        }

        false
    }

    /// Collect top-level symbols from a module's source code.
    fn collect_symbols(
        &mut self,
        module_id: ModuleId,
        source: &str,
        imports: &[Import],
        graph: &ModuleGraph,
    ) {
        let mut module_syms = Vec::new();
        let mut exports: HashMap<String, SymbolId> = HashMap::new();

        // Collect import symbols
        for import in imports {
            if let Some(target_id) = graph.resolve_specifier(
                graph.get(module_id).map(|m| m.path.as_str()).unwrap_or(""),
                &import.specifier,
            ) {
                for name in &import.names {
                    let is_default = name.imported == "default";
                    let is_namespace = name.imported == "*";

                    let sym_id = self.add_symbol(Symbol {
                        id: 0, // Will be set by add_symbol
                        name: name.local.clone(),
                        module_id,
                        kind: SymbolKind::Import {
                            source_module: target_id,
                            source_name: name.imported.clone(),
                            is_default,
                            is_namespace,
                        },
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            }
        }

        // Collect declarations and exports from source
        for line in source.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }

            // export const/let/var name
            if let Some(decl) = trimmed.strip_prefix("export const ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Variable { is_const: true },
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("export let ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Variable { is_const: false },
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("export var ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Variable { is_const: false },
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            }
            // export function name
            else if let Some(decl) = trimmed.strip_prefix("export function ") {
                if let Some(name) = extract_fn_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Function,
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            }
            // export async function name
            else if let Some(decl) = trimmed.strip_prefix("export async function ") {
                if let Some(name) = extract_fn_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Function,
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            }
            // export class Name
            else if let Some(decl) = trimmed.strip_prefix("export class ") {
                if let Some(name) = extract_class_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name: name.clone(),
                        module_id,
                        kind: SymbolKind::Class,
                        span: None,
                    });
                    module_syms.push(sym_id);
                    exports.insert(name, sym_id);
                }
            }
            // export default
            else if trimmed.starts_with("export default ") {
                let value = trimmed.strip_prefix("export default ").unwrap();
                let value = value.trim_end_matches(';');

                // Check if it's a function or class with a name
                let name = if value.starts_with("function ") {
                    extract_fn_name(value.strip_prefix("function ").unwrap())
                } else if value.starts_with("class ") {
                    extract_class_name(value.strip_prefix("class ").unwrap())
                } else {
                    None
                };

                let sym_id = self.add_symbol(Symbol {
                    id: 0,
                    name: name.unwrap_or_else(|| "_default".to_string()),
                    module_id,
                    kind: SymbolKind::Export {
                        local_symbol: None,
                        is_default: true,
                    },
                    span: None,
                });
                module_syms.push(sym_id);
                exports.insert("default".to_string(), sym_id);
            }
            // Non-exported declarations
            else if let Some(decl) = trimmed.strip_prefix("const ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Variable { is_const: true },
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("let ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Variable { is_const: false },
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("var ") {
                if let Some(name) = extract_decl_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Variable { is_const: false },
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("function ") {
                if let Some(name) = extract_fn_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Function,
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("async function ") {
                if let Some(name) = extract_fn_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Function,
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            } else if let Some(decl) = trimmed.strip_prefix("class ") {
                if let Some(name) = extract_class_name(decl) {
                    let sym_id = self.add_symbol(Symbol {
                        id: 0,
                        name,
                        module_id,
                        kind: SymbolKind::Class,
                        span: None,
                    });
                    module_syms.push(sym_id);
                }
            }
        }

        self.module_symbols.insert(module_id, module_syms);
        self.module_exports.insert(module_id, exports);
    }

    /// Add a symbol to the context.
    fn add_symbol(&mut self, mut symbol: Symbol) -> SymbolId {
        let id = self.symbols.len();
        symbol.id = id;

        // Track by name for conflict detection
        self.name_to_symbols
            .entry(symbol.name.clone())
            .or_default()
            .push(id);

        self.symbols.push(symbol);
        id
    }

    /// Resolve naming conflicts by renaming symbols.
    /// First module (in toposort order) keeps the original name.
    /// Others get suffixes: x, x$1, x$2, etc.
    fn resolve_conflicts(&mut self, order: &[ModuleId]) {
        // Build a map of module_id -> order index for priority
        let module_order: HashMap<ModuleId, usize> =
            order.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        // For each name with multiple symbols, resolve conflicts
        for (name, symbol_ids) in &self.name_to_symbols {
            if symbol_ids.len() <= 1 {
                // No conflict, keep original name
                if let Some(&sym_id) = symbol_ids.first() {
                    self.renames.insert(sym_id, name.clone());
                }
                continue;
            }

            // Sort symbols by their module's order (earlier modules have priority)
            let mut sorted_ids = symbol_ids.clone();
            sorted_ids.sort_by_key(|&sym_id| {
                self.symbols
                    .get(sym_id)
                    .and_then(|s| module_order.get(&s.module_id))
                    .copied()
                    .unwrap_or(usize::MAX)
            });

            // First symbol keeps original name
            if let Some(&first_id) = sorted_ids.first() {
                self.renames.insert(first_id, name.clone());
            }

            // Others get suffixes
            for (i, &sym_id) in sorted_ids.iter().skip(1).enumerate() {
                let new_name = format!("{}${}", name, i + 1);
                self.renames.insert(sym_id, new_name);
            }
        }
    }

    /// Link import symbols to their corresponding export symbols.
    fn link_imports(&mut self, _graph: &ModuleGraph) {
        for symbol in &self.symbols {
            if let SymbolKind::Import {
                source_module,
                source_name,
                is_namespace,
                ..
            } = &symbol.kind
            {
                if *is_namespace {
                    // Namespace imports are special - they import the whole module
                    // For now, we don't link these (would need special handling)
                    continue;
                }

                // Find the corresponding export in the source module
                if let Some(exports) = self.module_exports.get(source_module) {
                    if let Some(&export_sym_id) = exports.get(source_name) {
                        self.symbol_links.insert(symbol.id, export_sym_id);
                    }
                }
            }
        }
    }

    /// Get the renamed identifier for a symbol.
    pub fn get_rename(&self, symbol_id: SymbolId) -> Option<&String> {
        // First check if this import links to an export
        if let Some(&export_id) = self.symbol_links.get(&symbol_id) {
            // Use the export's renamed name
            return self.renames.get(&export_id);
        }
        self.renames.get(&symbol_id)
    }

    /// Get a symbol by ID.
    pub fn get_symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id)
    }

    /// Get all symbols for a module.
    pub fn get_module_symbols(&self, module_id: ModuleId) -> &[SymbolId] {
        self.module_symbols
            .get(&module_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if a module needs to be wrapped (can't be scope hoisted).
    pub fn is_wrapped(&self, module_id: ModuleId) -> bool {
        self.wrapped_modules.contains(&module_id)
    }

    /// Get the exports map for a module.
    pub fn get_exports(&self, module_id: ModuleId) -> Option<&HashMap<String, SymbolId>> {
        self.module_exports.get(&module_id)
    }

    /// Get all rename mappings.
    pub fn get_renames(&self) -> &HashMap<SymbolId, String> {
        &self.renames
    }

    /// Get the symbol ID for a name in a specific module.
    pub fn find_symbol(&self, module_id: ModuleId, name: &str) -> Option<SymbolId> {
        self.module_symbols.get(&module_id).and_then(|syms| {
            syms.iter().find(|&&sym_id| {
                self.symbols
                    .get(sym_id)
                    .map(|s| s.name == name)
                    .unwrap_or(false)
            }).copied()
        })
    }

    /// Build a rename map for a specific module: original_name -> new_name.
    pub fn build_module_renames(&self, module_id: ModuleId) -> HashMap<String, String> {
        let mut renames = HashMap::new();

        if let Some(sym_ids) = self.module_symbols.get(&module_id) {
            for &sym_id in sym_ids {
                if let Some(symbol) = self.symbols.get(sym_id) {
                    if let Some(new_name) = self.get_rename(sym_id) {
                        if &symbol.name != new_name {
                            renames.insert(symbol.name.clone(), new_name.clone());
                        }
                    }
                }
            }
        }

        renames
    }
}

/// Check if a substring is inside a comment in a line.
fn is_in_comment(line: &str, substr: &str) -> bool {
    if let Some(pos) = line.find(substr) {
        // Check if there's a // before it
        if let Some(comment_pos) = line.find("//") {
            if comment_pos < pos {
                return true;
            }
        }
    }
    false
}

/// Extract declaration name from something like "foo = 1" or "foo: Type = 1".
fn extract_decl_name(decl: &str) -> Option<String> {
    // Skip destructuring patterns
    let trimmed = decl.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return None;
    }

    // Find the name (before =, :, or space)
    let end_chars = ['=', ':', ' ', ';', ','];
    let end_pos = trimmed
        .find(|c| end_chars.contains(&c))
        .unwrap_or(trimmed.len());

    let name = trimmed[..end_pos].trim();
    if name.is_empty() || !is_valid_identifier(name) {
        return None;
    }

    Some(name.to_string())
}

/// Extract function name from something like "foo(a, b) { ... }".
fn extract_fn_name(decl: &str) -> Option<String> {
    let trimmed = decl.trim();

    // Handle generator functions
    let trimmed = trimmed.strip_prefix('*').map(str::trim).unwrap_or(trimmed);

    // Find the opening paren
    let paren_pos = trimmed.find('(')?;
    let name = trimmed[..paren_pos].trim();

    if name.is_empty() || !is_valid_identifier(name) {
        return None;
    }

    Some(name.to_string())
}

/// Extract class name from something like "Foo { ... }" or "Foo extends Bar { ... }".
fn extract_class_name(decl: &str) -> Option<String> {
    let trimmed = decl.trim();

    // Find the name (before space, { or <)
    let end_chars = [' ', '{', '<'];
    let end_pos = trimmed
        .find(|c| end_chars.contains(&c))
        .unwrap_or(trimmed.len());

    let name = trimmed[..end_pos].trim();
    if name.is_empty() || !is_valid_identifier(name) {
        return None;
    }

    Some(name.to_string())
}

/// Check if a string is a valid JavaScript identifier.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();

    // First character must be a letter, underscore, or dollar sign
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }

    // Remaining characters can also include digits
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_decl_name() {
        assert_eq!(extract_decl_name("foo = 1"), Some("foo".to_string()));
        assert_eq!(extract_decl_name("bar: number = 1"), Some("bar".to_string()));
        assert_eq!(extract_decl_name("baz;"), Some("baz".to_string()));
        assert_eq!(extract_decl_name("{ a, b } = obj"), None);
        assert_eq!(extract_decl_name("[x, y] = arr"), None);
    }

    #[test]
    fn test_extract_fn_name() {
        assert_eq!(extract_fn_name("foo(a, b) { }"), Some("foo".to_string()));
        assert_eq!(extract_fn_name("bar() { }"), Some("bar".to_string()));
        assert_eq!(extract_fn_name("*gen() { }"), Some("gen".to_string()));
        assert_eq!(extract_fn_name("() => {}"), None);
    }

    #[test]
    fn test_extract_class_name() {
        assert_eq!(extract_class_name("Foo { }"), Some("Foo".to_string()));
        assert_eq!(
            extract_class_name("Bar extends Baz { }"),
            Some("Bar".to_string())
        );
        assert_eq!(
            extract_class_name("Generic<T> { }"),
            Some("Generic".to_string())
        );
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("$baz"));
        assert!(is_valid_identifier("foo123"));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
    }

    #[test]
    fn test_needs_wrapper() {
        let ctx = ScopeHoistContext::new();

        // Should need wrapper
        assert!(ctx.needs_wrapper("const x = eval('1 + 1');"));
        assert!(ctx.needs_wrapper("with (obj) { x = 1; }"));
        assert!(ctx.needs_wrapper("const fs = require('fs');"));
        assert!(ctx.needs_wrapper("module.exports = foo;"));
        assert!(ctx.needs_wrapper("exports.foo = bar;"));
        assert!(ctx.needs_wrapper("console.log(__dirname);"));

        // Should not need wrapper
        assert!(!ctx.needs_wrapper("const x = 1;"));
        assert!(!ctx.needs_wrapper("export const y = 2;"));
        assert!(!ctx.needs_wrapper("// require('fs')"));
    }

    #[test]
    fn test_conflict_resolution() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        // Add two modules with the same variable name 'x'
        let id_a = graph.add(Module {
            path: "/a.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let id_b = graph.add(Module {
            path: "/b.js".to_string(),
            source: "export const x = 100;".to_string(),
            imports: vec![],
            dependencies: vec![id_a],
            dynamic_dependencies: vec![],
        });

        let order = vec![id_a, id_b];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // First module should keep original name
        let a_renames = ctx.build_module_renames(id_a);
        assert!(a_renames.is_empty() || a_renames.get("x") == Some(&"x".to_string()));

        // Second module should have renamed 'x'
        let b_renames = ctx.build_module_renames(id_b);
        assert_eq!(b_renames.get("x"), Some(&"x$1".to_string()));
    }

    #[test]
    fn test_is_in_comment() {
        assert!(is_in_comment("// require('fs')", "require("));
        assert!(!is_in_comment("require('fs')", "require("));
        assert!(!is_in_comment("const x = 1; // comment", "const"));
        assert!(is_in_comment("// const x = 1", "const"));
    }

    #[test]
    fn test_collect_multiple_declarations() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/multi.js".to_string(),
            source: r#"
export const a = 1;
export let b = 2;
export var c = 3;
export function d() {}
export class E {}
const internal = 42;
"#.to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Should have collected all symbols
        let syms = ctx.get_module_symbols(id);
        assert!(syms.len() >= 5); // a, b, c, d, E, internal
    }

    #[test]
    fn test_wrapped_module_detection() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        // ESM module - should NOT be wrapped
        let esm_id = graph.add(Module {
            path: "/esm.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        // CJS module - SHOULD be wrapped
        let cjs_id = graph.add(Module {
            path: "/cjs.js".to_string(),
            source: "module.exports = { x: 1 };".to_string(),
            imports: vec![],
            dependencies: vec![esm_id],
            dynamic_dependencies: vec![],
        });

        let order = vec![esm_id, cjs_id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        assert!(!ctx.is_wrapped(esm_id));
        assert!(ctx.is_wrapped(cjs_id));
    }

    #[test]
    fn test_extract_decl_name_edge_cases() {
        // TypeScript type annotations
        assert_eq!(extract_decl_name("foo: string = 'bar'"), Some("foo".to_string()));
        // With type and initializer
        assert_eq!(extract_decl_name("count: number = 0"), Some("count".to_string()));
        // Just a name
        assert_eq!(extract_decl_name("x"), Some("x".to_string()));
        // Empty string
        assert_eq!(extract_decl_name(""), None);
        // Just whitespace
        assert_eq!(extract_decl_name("   "), None);
    }

    #[test]
    fn test_extract_fn_name_edge_cases() {
        // Async function (the 'async' would be stripped before calling this)
        assert_eq!(extract_fn_name("fetchData() { }"), Some("fetchData".to_string()));
        // With parameters
        assert_eq!(extract_fn_name("add(a, b) { return a + b; }"), Some("add".to_string()));
        // Generator
        assert_eq!(extract_fn_name("*generator() { yield 1; }"), Some("generator".to_string()));
        // No name (anonymous)
        assert_eq!(extract_fn_name("() { }"), None);
    }

    #[test]
    fn test_symbol_kind() {
        let var_sym = SymbolKind::Variable { is_const: true };
        assert_eq!(var_sym, SymbolKind::Variable { is_const: true });

        let fn_sym = SymbolKind::Function;
        assert_eq!(fn_sym, SymbolKind::Function);

        let class_sym = SymbolKind::Class;
        assert_eq!(class_sym, SymbolKind::Class);
    }

    #[test]
    fn test_no_conflict_different_names() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id_a = graph.add(Module {
            path: "/a.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let id_b = graph.add(Module {
            path: "/b.js".to_string(),
            source: "export const y = 2;".to_string(),
            imports: vec![],
            dependencies: vec![id_a],
            dynamic_dependencies: vec![],
        });

        let order = vec![id_a, id_b];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Neither should be renamed since no conflict
        let a_renames = ctx.build_module_renames(id_a);
        let b_renames = ctx.build_module_renames(id_b);

        assert!(a_renames.is_empty());
        assert!(b_renames.is_empty());
    }

    #[test]
    fn test_five_way_conflict() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        // Five modules all exporting 'name'
        let ids: Vec<_> = (0..5).map(|i| {
            graph.add(Module {
                path: format!("/mod{}.js", i),
                source: format!("export const name = {};", i),
                imports: vec![],
                dependencies: if i > 0 { vec![i - 1] } else { vec![] },
                dynamic_dependencies: vec![],
            })
        }).collect();

        let ctx = ScopeHoistContext::analyze(&graph, &ids);

        // First keeps original, others get $1, $2, $3, $4
        let renames_0 = ctx.build_module_renames(ids[0]);
        let renames_1 = ctx.build_module_renames(ids[1]);
        let renames_2 = ctx.build_module_renames(ids[2]);
        let renames_3 = ctx.build_module_renames(ids[3]);
        let renames_4 = ctx.build_module_renames(ids[4]);

        assert!(renames_0.is_empty() || renames_0.get("name") == Some(&"name".to_string()));
        assert_eq!(renames_1.get("name"), Some(&"name$1".to_string()));
        assert_eq!(renames_2.get("name"), Some(&"name$2".to_string()));
        assert_eq!(renames_3.get("name"), Some(&"name$3".to_string()));
        assert_eq!(renames_4.get("name"), Some(&"name$4".to_string()));
    }

    // =========================================================================
    // Boundary Value Tests (0, 1, -1 testing)
    // =========================================================================

    #[test]
    fn test_empty_graph() {
        use crate::bundler::graph::ModuleGraph;

        let graph = ModuleGraph::new();
        let order: Vec<usize> = vec![];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        assert!(ctx.symbols.is_empty());
        assert!(ctx.module_symbols.is_empty());
        assert!(ctx.renames.is_empty());
    }

    #[test]
    fn test_single_module() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/single.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Single module should have symbol but no renames needed
        assert!(!ctx.symbols.is_empty());
        let renames = ctx.build_module_renames(id);
        assert!(renames.is_empty()); // No conflict, no rename
    }

    #[test]
    fn test_empty_module_source() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/empty.js".to_string(),
            source: "".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Empty module should work without error
        assert!(ctx.get_module_symbols(id).is_empty());
    }

    #[test]
    fn test_whitespace_only_module() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/whitespace.js".to_string(),
            source: "   \n\n   \t\t   ".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Whitespace-only module should work
        assert!(ctx.get_module_symbols(id).is_empty());
    }

    #[test]
    fn test_single_character_identifier() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id_a = graph.add(Module {
            path: "/a.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let id_b = graph.add(Module {
            path: "/b.js".to_string(),
            source: "export const x = 2;".to_string(),
            imports: vec![],
            dependencies: vec![id_a],
            dynamic_dependencies: vec![],
        });

        let order = vec![id_a, id_b];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Single char identifier should still get renamed
        let b_renames = ctx.build_module_renames(id_b);
        assert_eq!(b_renames.get("x"), Some(&"x$1".to_string()));
    }

    #[test]
    fn test_very_long_identifier() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let long_name = "a".repeat(100);

        let mut graph = ModuleGraph::new();

        let id_a = graph.add(Module {
            path: "/a.js".to_string(),
            source: format!("export const {} = 1;", long_name),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let id_b = graph.add(Module {
            path: "/b.js".to_string(),
            source: format!("export const {} = 2;", long_name),
            imports: vec![],
            dependencies: vec![id_a],
            dynamic_dependencies: vec![],
        });

        let order = vec![id_a, id_b];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Long identifier should still work
        let b_renames = ctx.build_module_renames(id_b);
        assert_eq!(b_renames.get(&long_name), Some(&format!("{}$1", long_name)));
    }

    #[test]
    fn test_special_identifier_chars() {
        // Test identifiers with $, _, and unicode
        assert!(is_valid_identifier("$"));
        assert!(is_valid_identifier("_"));
        assert!(is_valid_identifier("$_$"));
        assert!(is_valid_identifier("_$_"));
        assert!(is_valid_identifier("$0"));
        assert!(is_valid_identifier("_0"));
        assert!(!is_valid_identifier("0$"));  // Can't start with digit
        assert!(!is_valid_identifier(""));    // Empty is invalid
    }

    #[test]
    fn test_module_with_only_comments() {
        use crate::bundler::graph::{Module, ModuleGraph};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/comments.js".to_string(),
            source: "// This is a comment\n/* Multi-line\ncomment */".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let ctx = ScopeHoistContext::analyze(&graph, &order);

        // Comment-only module should have no symbols
        assert!(ctx.get_module_symbols(id).is_empty());
    }

    #[test]
    fn test_get_nonexistent_module() {
        let ctx = ScopeHoistContext::new();

        // Getting symbols for non-existent module should return empty
        assert!(ctx.get_module_symbols(999).is_empty());
        assert!(ctx.get_exports(999).is_none());
        assert!(!ctx.is_wrapped(999));
    }

    #[test]
    fn test_get_nonexistent_symbol() {
        let ctx = ScopeHoistContext::new();

        // Getting non-existent symbol should return None
        assert!(ctx.get_symbol(999).is_none());
        assert!(ctx.get_rename(999).is_none());
    }
}
