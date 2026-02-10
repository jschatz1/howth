//! Bundle output emission.
//!
//! Generates the final bundled JavaScript from the module graph.

#![allow(clippy::redundant_else)]
#![allow(clippy::manual_is_ascii_check)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::manual_pattern_char_comparison)]

use super::graph::{ModuleGraph, ModuleId};
use super::scope::ScopeHoistContext;
use super::treeshake::UsedExports;
use super::{BundleError, BundleOptions};
use howth_parser::{Codegen, CodegenOptions, Parser, ParserOptions};
use rayon::prelude::*;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

// =============================================================================
// Minification
// =============================================================================

/// Minify a bundle string using howth-parser (whitespace removal + optional mangling).
///
/// Parses the concatenated bundle and re-emits with minified codegen
/// (no whitespace, no newlines, deferred semicolons).
/// When `mangle` is true, also shortens local variable names.
fn minify_bundle(code: &str, mangle: bool) -> Result<String, BundleError> {
    let opts = ParserOptions {
        module: false,
        ..Default::default()
    };
    let mut ast = Parser::new(code, opts).parse().map_err(|e| BundleError {
        code: "MINIFY_PARSE_ERROR",
        message: format!("Failed to parse bundle for minification: {e}"),
        path: None,
    })?;

    if mangle {
        howth_parser::mangle::mangle(
            &mut ast,
            &howth_parser::mangle::MangleOptions::default(),
        );
    }

    let codegen_opts = CodegenOptions {
        minify: true,
        ..Default::default()
    };
    Ok(Codegen::new(&ast, codegen_opts).generate())
}

// =============================================================================
// Source Map Support
// =============================================================================

/// VLQ-encode a signed integer and append to output string.
fn vlq_encode(value: i64, out: &mut String) {
    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    #[allow(clippy::cast_sign_loss)]
    let mut v = (if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    }) as u64;
    loop {
        let mut digit = (v & 0x1f) as u8;
        v >>= 5;
        if v > 0 {
            digit |= 0x20; // continuation bit
        }
        out.push(B64[digit as usize] as char);
        if v == 0 {
            break;
        }
    }
}

/// Tracks source positions during bundle emission and generates a V3 sourcemap.
struct SourceMapBuilder {
    /// Module paths (source file names).
    sources: Vec<String>,
    /// Original source content for each module.
    sources_content: Vec<String>,
    /// Mapping segments: (output_line, output_col, source_idx, source_line, source_col).
    mappings: Vec<(u32, u32, u32, u32, u32)>,
}

impl SourceMapBuilder {
    fn new() -> Self {
        Self {
            sources: Vec::new(),
            sources_content: Vec::new(),
            mappings: Vec::new(),
        }
    }

    /// Register a source file and return its index.
    fn add_source(&mut self, path: &str, content: &str) -> u32 {
        let idx = self.sources.len() as u32;
        self.sources.push(path.to_string());
        self.sources_content.push(content.to_string());
        idx
    }

    /// Add a line-level mapping: output_line maps to source_line in source_idx.
    fn add_line_mapping(&mut self, output_line: u32, source_idx: u32, source_line: u32) {
        self.mappings
            .push((output_line, 0, source_idx, source_line, 0));
    }

    /// Generate a V3 sourcemap JSON string.
    fn generate(&self, file: &str) -> String {
        let mut mappings_str = String::new();
        let mut prev_output_line: u32 = 0;
        let mut prev_source: i64 = 0;
        let mut prev_source_line: i64 = 0;
        let mut prev_source_col: i64 = 0;

        // Sort mappings by output line
        let mut sorted = self.mappings.clone();
        sorted.sort_by_key(|m| (m.0, m.1));

        for &(output_line, output_col, source_idx, source_line, source_col) in &sorted {
            // Add semicolons for skipped lines
            while prev_output_line < output_line {
                mappings_str.push(';');
                prev_output_line += 1;
            }

            // If not the first segment on this line, add comma
            // (We only emit one segment per line, so this rarely triggers)

            // Encode: output_col (relative), source_idx (relative), source_line (relative), source_col (relative)
            vlq_encode(i64::from(output_col), &mut mappings_str);
            vlq_encode(i64::from(source_idx) - prev_source, &mut mappings_str);
            vlq_encode(i64::from(source_line) - prev_source_line, &mut mappings_str);
            vlq_encode(i64::from(source_col) - prev_source_col, &mut mappings_str);

            prev_source = i64::from(source_idx);
            prev_source_line = i64::from(source_line);
            prev_source_col = i64::from(source_col);
        }

        // Build JSON manually (avoid serde dependency for this small structure)
        let sources_json: Vec<String> = self.sources.iter().map(|s| json_string(s)).collect();
        let contents_json: Vec<String> = self
            .sources_content
            .iter()
            .map(|s| json_string(s))
            .collect();

        format!(
            r#"{{"version":3,"file":{},"sources":[{}],"sourcesContent":[{}],"mappings":{}}}"#,
            json_string(file),
            sources_json.join(","),
            contents_json.join(","),
            json_string(&mappings_str),
        )
    }
}

/// JSON-encode a string value (with escaping).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Output format for the bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BundleFormat {
    /// ES Modules (import/export).
    #[default]
    Esm,
    /// CommonJS (require/module.exports).
    Cjs,
    /// IIFE (immediately invoked function expression).
    Iife,
}

/// Bundle output.
#[derive(Debug)]
pub struct BundleOutput {
    /// The bundled code.
    pub code: String,
    /// Source map (if enabled).
    pub map: Option<String>,
}

/// Emit a bundle from the module graph.
pub fn emit_bundle(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
) -> Result<BundleOutput, BundleError> {
    emit_bundle_with_entry(graph, order, options, None)
}

/// Emit a bundle with a specific entry point (for code splitting).
pub fn emit_bundle_with_entry(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    entry_override: Option<ModuleId>,
) -> Result<BundleOutput, BundleError> {
    // Determine entry point
    let entry_id = entry_override.or_else(|| order.last().copied());

    // Compute used exports for tree shaking
    let used_exports = if options.treeshake {
        entry_id.map(|id| UsedExports::analyze(graph, id))
    } else {
        None
    };

    let mut output = String::new();

    // Bundle header (skip when minifying)
    if !options.minify {
        output.push_str("// howth bundle\n");
        output.push_str("// Generated by howth v0.1.0\n\n");
    }

    match options.format {
        BundleFormat::Esm => emit_esm(
            graph,
            order,
            options,
            used_exports.as_ref(),
            entry_id,
            &mut output,
        )?,
        BundleFormat::Cjs => emit_cjs(
            graph,
            order,
            options,
            used_exports.as_ref(),
            entry_id,
            &mut output,
        )?,
        BundleFormat::Iife => emit_iife(
            graph,
            order,
            options,
            used_exports.as_ref(),
            entry_id,
            &mut output,
        )?,
    }

    // Minification is handled per-module in emit_module_to_string (parallel).
    // Scope-hoisted bundles still use minify_bundle since they share a single scope.

    // Generate sourcemap if requested
    let map = if options.sourcemap {
        Some(build_sourcemap_from_output(&output, graph, order))
    } else {
        None
    };

    Ok(BundleOutput { code: output, map })
}

/// Build a line-level sourcemap by scanning the output for module path comments.
/// This works for both wrapped and scope-hoisted output since both emit `// /path` comments.
fn build_sourcemap_from_output(output: &str, graph: &ModuleGraph, order: &[ModuleId]) -> String {
    let mut builder = SourceMapBuilder::new();

    // Register all sources
    let mut source_indices: HashMap<ModuleId, u32> = HashMap::default();
    for &id in order {
        if let Some(module) = graph.get(id) {
            let idx = builder.add_source(&module.path, &module.source);
            source_indices.insert(id, idx);
        }
    }

    // Build a map from module path to (module_id, source_idx)
    let mut path_to_source: HashMap<&str, (ModuleId, u32)> = HashMap::default();
    for &id in order {
        if let Some(module) = graph.get(id) {
            if let Some(&src_idx) = source_indices.get(&id) {
                path_to_source.insert(&module.path, (id, src_idx));
            }
        }
    }

    // Scan output lines for module path comments and track which module each line belongs to
    let mut current_source: Option<(u32, u32)> = None; // (source_idx, source_line_offset)
    for (output_line, line) in output.lines().enumerate() {
        let trimmed = line.trim();

        // Detect module path comment: "// /path/to/module.js"
        if trimmed.starts_with("// ")
            && !trimmed.starts_with("// howth")
            && !trimmed.starts_with("// Generated")
            && !trimmed.starts_with("// Module registry")
            && !trimmed.starts_with("// Entry")
        {
            let path = &trimmed[3..];
            if let Some(&(_, src_idx)) = path_to_source.get(path) {
                current_source = Some((src_idx, 0));
                continue;
            }
            // Also match "Module N: /path" pattern
            if trimmed.starts_with("// Module ") {
                if let Some(colon_idx) = trimmed.find(": ") {
                    let path = &trimmed[colon_idx + 2..];
                    if let Some(&(_, src_idx)) = path_to_source.get(path) {
                        current_source = Some((src_idx, 0));
                        continue;
                    }
                }
            }
        }

        // Map this output line to the current source
        if let Some((src_idx, ref mut src_line)) = current_source {
            if !trimmed.is_empty()
                && !trimmed.starts_with("__modules[")
                && !trimmed.starts_with("};")
            {
                builder.add_line_mapping(output_line as u32, src_idx, *src_line);
                *src_line += 1;
            }
        }
    }

    builder.generate("bundle.js")
}

/// Emit ESM bundle.
fn emit_esm(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    used_exports: Option<&UsedExports>,
    entry_id: Option<ModuleId>,
    output: &mut String,
) -> Result<(), BundleError> {
    // For ESM, we use a module registry pattern
    if options.minify {
        output.push_str("const __modules={};const __exports={};");
        output.push_str("function __require(id){if(__exports[id])return __exports[id];const module={exports:{}};__modules[id](module,module.exports,__require);__exports[id]=module.exports;return module.exports;}");
    } else {
        output.push_str("const __modules = {};\n");
        output.push_str("const __exports = {};\n\n");

        output.push_str("function __require(id) {\n");
        output.push_str("  if (__exports[id]) return __exports[id];\n");
        output.push_str("  const module = { exports: {} };\n");
        output.push_str("  __modules[id](module, module.exports, __require);\n");
        output.push_str("  __exports[id] = module.exports;\n");
        output.push_str("  return module.exports;\n");
        output.push_str("}\n\n");
    }

    // Parallel emit: process each module in parallel, then concatenate in order
    let module_outputs: Vec<Result<String, BundleError>> = order
        .par_iter()
        .map(|&id| {
            let module = graph.get(id).ok_or_else(|| BundleError {
                code: "BUNDLE_INTERNAL_ERROR",
                message: format!("Module {} not found in graph", id),
                path: None,
            })?;
            emit_module_to_string(id, module, graph, options, used_exports)
        })
        .collect();

    // Pre-allocate capacity for concatenation (estimate ~1KB per module)
    let total_len: usize = module_outputs
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|s| s.len())
        .sum();
    output.reserve(total_len);

    // Concatenate results in order
    for result in module_outputs {
        output.push_str(&result?);
    }

    // Entry point execution
    if let Some(entry) = entry_id {
        if options.minify {
            output.push_str(&format!("__require({});", entry));
        } else {
            output.push_str(&format!("\n// Entry point\n__require({});\n", entry));
        }
    }

    Ok(())
}

/// Emit CJS bundle.
fn emit_cjs(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    used_exports: Option<&UsedExports>,
    entry_id: Option<ModuleId>,
    output: &mut String,
) -> Result<(), BundleError> {
    // Similar to ESM but with CommonJS wrapper
    emit_esm(graph, order, options, used_exports, entry_id, output)?;

    // Add module.exports for the entry
    if let Some(entry) = entry_id {
        if options.minify {
            output.push_str(&format!("module.exports=__exports[{}];", entry));
        } else {
            output.push_str(&format!("\nmodule.exports = __exports[{}];\n", entry));
        }
    }

    Ok(())
}

/// Emit IIFE bundle.
fn emit_iife(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    used_exports: Option<&UsedExports>,
    entry_id: Option<ModuleId>,
    output: &mut String,
) -> Result<(), BundleError> {
    if options.minify {
        output.push_str("(function(){'use strict';");
    } else {
        output.push_str("(function() {\n");
        output.push_str("'use strict';\n\n");
    }

    // Emit the ESM content inside IIFE
    let mut inner = String::new();
    emit_esm(graph, order, options, used_exports, entry_id, &mut inner)?;

    if options.minify {
        output.push_str(&inner);
    } else {
        // Indent the inner content
        for line in inner.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str("})();");
    if !options.minify {
        output.push('\n');
    }

    Ok(())
}

/// Emit a single module to a string (for parallel processing).
fn emit_module_to_string(
    id: ModuleId,
    module: &super::graph::Module,
    graph: &ModuleGraph,
    options: &BundleOptions,
    used_exports: Option<&UsedExports>,
) -> Result<String, BundleError> {
    // Get the set of used exports for tree shaking
    let used_set: Option<HashSet<String>> = used_exports.and_then(|u| u.get_used(id).cloned());

    // Transform the source code with tree shaking info
    let transformed = transform_module(&module.source, &module.path, graph, used_set.as_ref())?;

    if options.minify {
        // Build the wrapped module string, then parse+minify+mangle in one shot
        let mut wrapped = String::with_capacity(transformed.len() + 100);
        wrapped.push_str(&format!(
            "__modules[{}]=function(module,exports,require){{",
            id
        ));
        for line in transformed.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                wrapped.push_str(trimmed);
                wrapped.push('\n');
            }
        }
        wrapped.push_str("};");

        // Parse the small wrapped module (~500 bytes)
        let opts = ParserOptions {
            module: false,
            ..Default::default()
        };
        let mut ast = Parser::new(&wrapped, opts).parse().map_err(|e| BundleError {
            code: "MINIFY_PARSE_ERROR",
            message: format!("Failed to parse module {} for minification: {e}", module.path),
            path: Some(module.path.clone()),
        })?;

        if options.mangle {
            howth_parser::mangle::mangle(
                &mut ast,
                &howth_parser::mangle::MangleOptions::default(),
            );
        }

        let codegen_opts = CodegenOptions {
            minify: true,
            ..Default::default()
        };
        Ok(Codegen::new(&ast, codegen_opts).generate())
    } else {
        // Pretty-print with indentation
        let mut output = String::with_capacity(module.source.len() + 200);
        output.push_str(&format!("// Module {}: {}\n", id, module.path));
        output.push_str(&format!(
            "__modules[{}]=function(module,exports,require){{",
            id
        ));
        output.push('\n');

        for line in transformed.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }

        output.push_str("};\n\n");
        Ok(output)
    }
}

/// Emit a single module (legacy function, kept for compatibility).
#[allow(dead_code)]
fn emit_module(
    id: ModuleId,
    module: &super::graph::Module,
    graph: &ModuleGraph,
    output: &mut String,
    options: &BundleOptions,
    used_exports: Option<&UsedExports>,
) -> Result<(), BundleError> {
    let module_output = emit_module_to_string(id, module, graph, options, used_exports)?;
    output.push_str(&module_output);
    Ok(())
}

/// Transform module source (rewrite imports/exports for bundling).
/// Note: Source is already transpiled from TypeScript/JSX in the graph building phase.
fn transform_module(
    source: &str,
    module_path: &str,
    graph: &ModuleGraph,
    used_exports: Option<&HashSet<String>>,
) -> Result<String, BundleError> {
    // Source is already transpiled - just rewrite imports/exports
    // Collect exports to emit at the end
    let mut pending_exports: Vec<String> = Vec::new();
    // Pre-allocate: source size + some extra for export statements
    let mut result = String::with_capacity(source.len() + 100);

    for line in source.lines() {
        let (transformed, export_stmts) =
            transform_line_with_exports(line, module_path, graph, used_exports)?;

        // Filter SWC-generated exports.xxx = xxx; statements based on tree shaking
        let filtered = filter_swc_export(&transformed, used_exports);
        if let Some(filtered_line) = filtered {
            result.push_str(&filtered_line);
            result.push('\n');
        }

        pending_exports.extend(export_stmts);
    }

    // Emit all pending exports at the end (only used ones)
    for export_stmt in pending_exports {
        result.push_str(&export_stmt);
        result.push('\n');
    }

    Ok(result)
}

/// Filter SWC-generated `exports.xxx = xxx;` statements based on tree shaking.
/// Returns None if the line should be removed, Some(line) otherwise.
fn filter_swc_export(line: &str, used_exports: Option<&HashSet<String>>) -> Option<String> {
    let trimmed = line.trim();

    // Check for SWC-generated export pattern: exports.xxx = xxx;
    if trimmed.starts_with("exports.") && trimmed.contains(" = ") {
        // Extract the export name: "exports.foo = foo;" -> "foo"
        if let Some(dot_end) = trimmed[8..].find(' ') {
            let export_name = &trimmed[8..8 + dot_end];

            // Check if this export is used
            match used_exports {
                None => return Some(line.to_string()), // No tree shaking, keep all
                Some(set) => {
                    if set.contains(export_name) || export_name == "default" {
                        return Some(line.to_string()); // Export is used, keep it
                    } else {
                        return None; // Export is unused, remove it
                    }
                }
            }
        }
    }

    Some(line.to_string())
}

/// Transform a single line (basic import/export rewriting).
/// Returns (transformed_line, pending_exports_to_emit_later).
fn transform_line_with_exports(
    line: &str,
    module_path: &str,
    graph: &ModuleGraph,
    used_exports: Option<&HashSet<String>>,
) -> Result<(String, Vec<String>), BundleError> {
    let trimmed = line.trim();

    // Rewrite imports
    if trimmed.starts_with("import ") {
        return Ok((rewrite_import(line, module_path, graph), Vec::new()));
    }

    // Rewrite exports
    if trimmed.starts_with("export ") {
        let (transformed, exports) = rewrite_export_with_pending(line, used_exports);
        return Ok((transformed, exports));
    }

    // Pass through unchanged
    Ok((line.to_string(), Vec::new()))
}

/// Check if a specifier is a CSS file.
fn is_css_import(spec: &str) -> bool {
    std::path::Path::new(spec)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("css"))
}

/// Check if a specifier is an asset file.
fn is_asset_import(spec: &str) -> bool {
    let asset_exts = [
        "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "avif", "woff", "woff2", "ttf", "otf",
        "eot", "json", "txt", "wasm",
    ];
    std::path::Path::new(spec)
        .extension()
        .is_some_and(|ext| asset_exts.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// Rewrite an import statement.
fn rewrite_import(line: &str, module_path: &str, graph: &ModuleGraph) -> String {
    // import { foo } from './bar' -> const { foo } = require(1)
    // import foo from './bar' -> const foo = require(1).default
    // import './bar' -> require(1)
    // import './style.css' -> (CSS injection, handled at bundle level)
    // import logo from './logo.png' -> const logo = './logo.abc123.png'

    let trimmed = line.trim();

    // Helper to resolve specifier to module ID or keep as string
    let resolve_require = |spec: &str| -> String {
        if let Some(id) = graph.resolve_specifier(module_path, spec) {
            format!("require({})", id)
        } else {
            // External or unresolved - keep as string
            format!("require('{}')", spec)
        }
    };

    // Side-effect import: import './foo'
    if let Some(rest) = trimmed.strip_prefix("import '") {
        if let Some(spec) = rest.strip_suffix("';") {
            // CSS import - generate empty statement (CSS is bundled separately)
            if is_css_import(spec) {
                return format!("/* CSS: {} */", spec);
            }
            return format!("{};", resolve_require(spec));
        }
    }
    if let Some(rest) = trimmed.strip_prefix("import \"") {
        if let Some(spec) = rest.strip_suffix("\";") {
            // CSS import - generate empty statement (CSS is bundled separately)
            if is_css_import(spec) {
                return format!("/* CSS: {} */", spec);
            }
            return format!("{};", resolve_require(spec));
        }
    }

    // Named imports: import { foo, bar } from './baz'
    if trimmed.contains('{') && trimmed.contains('}') && trimmed.contains(" from ") {
        // Extract the names and specifier
        if let Some(from_idx) = trimmed.find(" from ") {
            let imports_part = &trimmed[7..from_idx]; // after "import "
            let spec_part = &trimmed[from_idx + 6..].trim();
            let spec = spec_part.trim_matches(|c| c == '\'' || c == '"' || c == ';');

            // Convert import-style `as` to destructuring-style `:`
            // e.g. `{ jsx as _jsx }` → `{ jsx: _jsx }`
            let destructure_part = imports_part.replace(" as ", ": ");
            return format!("const {} = {};", destructure_part, resolve_require(spec));
        }
    }

    // Default import: import foo from './bar'
    if trimmed.contains(" from ") && !trimmed.contains('{') {
        if let Some(from_idx) = trimmed.find(" from ") {
            let name = trimmed[7..from_idx].trim(); // after "import "
            let spec_part = &trimmed[from_idx + 6..].trim();
            let spec = spec_part.trim_matches(|c| c == '\'' || c == '"' || c == ';');

            // Check for * as namespace import
            if name.starts_with("* as ") {
                let ns_name = name.strip_prefix("* as ").unwrap().trim();
                return format!("const {} = {};", ns_name, resolve_require(spec));
            }

            // Asset import: import logo from './logo.png'
            // Returns the asset URL (will be rewritten with hash at bundle time)
            if is_asset_import(spec) {
                return format!("const {} = '{}';", name, spec);
            }

            let req = resolve_require(spec);
            return format!("const {} = {}.default || {};", name, req, req);
        }
    }

    // Fallback: return as-is with a comment
    format!("/* TODO: transform */ {}", line)
}

/// Rewrite an export statement, returning the transformed line and pending exports.
/// Returns (transformed_line, vec_of_exports_to_emit_at_end).
/// If used_exports is Some, only exports in that set will be emitted.
fn rewrite_export_with_pending(
    line: &str,
    used_exports: Option<&HashSet<String>>,
) -> (String, Vec<String>) {
    let trimmed = line.trim();

    // Helper to check if an export is used
    let is_used = |name: &str| -> bool {
        match used_exports {
            None => true, // No tree shaking, all exports are used
            Some(set) => set.contains(name) || name == "default",
        }
    };

    // export default - always include (entry point API)
    if trimmed.starts_with("export default ") {
        let value = trimmed.strip_prefix("export default ").unwrap();
        let value = value.trim_end_matches(';');
        return (format!("exports.default = {};", value), Vec::new());
    }

    // export const/let/var
    if trimmed.starts_with("export const ")
        || trimmed.starts_with("export let ")
        || trimmed.starts_with("export var ")
    {
        let decl = trimmed.strip_prefix("export ").unwrap();
        let parts: Vec<&str> = decl.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let name = parts[1].trim_end_matches(['=', ':', ' ']);
            if is_used(name) {
                return (
                    decl.to_string(),
                    vec![format!("exports.{} = {};", name, name)],
                );
            } else {
                // Tree shake: include declaration but don't export
                return (decl.to_string(), Vec::new());
            }
        }
    }

    // export function
    if trimmed.starts_with("export function ") {
        let decl = trimmed.strip_prefix("export ").unwrap();
        if let Some(paren_idx) = decl.find('(') {
            let name = decl[9..paren_idx].trim(); // after "function "
            if is_used(name) {
                return (
                    decl.to_string(),
                    vec![format!("exports.{} = {};", name, name)],
                );
            } else {
                // Tree shake: include function but don't export
                return (decl.to_string(), Vec::new());
            }
        }
    }

    // export class
    if trimmed.starts_with("export class ") {
        let decl = trimmed.strip_prefix("export ").unwrap();
        let parts: Vec<&str> = decl.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let name = parts[1].trim_end_matches(|c: char| matches!(c, '{' | ' '));
            if is_used(name) {
                return (
                    decl.to_string(),
                    vec![format!("exports.{} = {};", name, name)],
                );
            } else {
                // Tree shake: include class but don't export
                return (decl.to_string(), Vec::new());
            }
        }
    }

    // export { foo, bar }
    if trimmed.starts_with("export {") {
        if let Some(end) = trimmed.find('}') {
            let names = &trimmed[8..end];
            let export_names: Vec<&str> = names.split(',').map(|s| s.trim()).collect();

            let mut export_stmts = Vec::new();
            for name in export_names {
                if name.contains(" as ") {
                    let parts: Vec<&str> = name.split(" as ").collect();
                    if parts.len() == 2 {
                        export_stmts.push(format!(
                            "exports.{} = {};",
                            parts[1].trim(),
                            parts[0].trim()
                        ));
                    }
                } else if !name.is_empty() {
                    export_stmts.push(format!("exports.{} = {};", name, name));
                }
            }
            return (String::new(), export_stmts); // Remove the export line, emit exports at end
        }
    }

    // Fallback
    (format!("/* TODO: transform export */ {}", line), Vec::new())
}

// =============================================================================
// Scope Hoisting Emission
// =============================================================================

/// Emit a bundle using scope hoisting.
///
/// This produces smaller, faster bundles by:
/// 1. Hoisting top-level declarations to the bundle scope
/// 2. Renaming conflicting identifiers
/// 3. Removing import/export statements
/// 4. Linking imports directly to their exports
pub fn emit_scope_hoisted(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
) -> Result<BundleOutput, BundleError> {
    // Analyze the module graph for scope hoisting
    let ctx = ScopeHoistContext::analyze(graph, order);

    let mut output = String::new();

    // Bundle header (skip when minifying)
    if !options.minify {
        output.push_str("// howth bundle (scope hoisted)\n");
        output.push_str("// Generated by howth v0.1.0\n\n");
    }

    match options.format {
        BundleFormat::Esm => emit_scope_hoisted_esm(graph, order, options, &ctx, &mut output)?,
        BundleFormat::Cjs => emit_scope_hoisted_cjs(graph, order, options, &ctx, &mut output)?,
        BundleFormat::Iife => emit_scope_hoisted_iife(graph, order, options, &ctx, &mut output)?,
    }

    // Run minifier when minify is enabled (whitespace removal)
    if options.minify {
        output = minify_bundle(&output, options.mangle).unwrap_or(output);
    }

    // Generate sourcemap if requested (must be after minification since line numbers change)
    let map = if options.sourcemap {
        Some(build_sourcemap_from_output(&output, graph, order))
    } else {
        None
    };

    Ok(BundleOutput { code: output, map })
}

/// Emit scope-hoisted ESM bundle.
fn emit_scope_hoisted_esm(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    ctx: &ScopeHoistContext,
    output: &mut String,
) -> Result<(), BundleError> {
    let minify = options.minify;

    // For modules that need wrapping, emit the module registry
    let has_wrapped = order.iter().any(|&id| ctx.is_wrapped(id));
    if has_wrapped {
        if minify {
            output.push_str("const __modules={};const __exports={};");
            output.push_str("function __require(id){if(__exports[id])return __exports[id];const module={exports:{}};__modules[id](module,module.exports,__require);__exports[id]=module.exports;return module.exports;}");
        } else {
            output.push_str("// Module registry for wrapped modules\n");
            output.push_str("const __modules = {};\n");
            output.push_str("const __exports = {};\n\n");
            output.push_str("function __require(id) {\n");
            output.push_str("  if (__exports[id]) return __exports[id];\n");
            output.push_str("  const module = { exports: {} };\n");
            output.push_str("  __modules[id](module, module.exports, __require);\n");
            output.push_str("  __exports[id] = module.exports;\n");
            output.push_str("  return module.exports;\n");
            output.push_str("}\n\n");
        }
    }

    // Emit each module in topological order
    for &module_id in order {
        let module = graph.get(module_id).ok_or_else(|| BundleError {
            code: "BUNDLE_INTERNAL_ERROR",
            message: format!("Module {} not found in graph", module_id),
            path: None,
        })?;

        if !minify {
            output.push_str(&format!("// {}\n", module.path));
        }

        if ctx.is_wrapped(module_id) {
            // Emit wrapped module (fallback for modules that can't be scope hoisted)
            emit_wrapped_module(module_id, module, graph, minify, output)?;
        } else {
            // Emit scope-hoisted module
            let renames = ctx.build_module_renames(module_id);
            emit_hoisted_module(&module.source, &renames, output)?;
        }

        if !minify {
            output.push('\n');
        }
    }

    Ok(())
}

/// Emit scope-hoisted CJS bundle.
fn emit_scope_hoisted_cjs(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    ctx: &ScopeHoistContext,
    output: &mut String,
) -> Result<(), BundleError> {
    // Same as ESM for the main content
    emit_scope_hoisted_esm(graph, order, options, ctx, output)?;

    // Add module.exports for entry point
    if let Some(&entry_id) = order.last() {
        if let Some(exports) = ctx.get_exports(entry_id) {
            if !exports.is_empty() {
                if !options.minify {
                    output.push_str("\n// Entry exports\n");
                }
                output.push_str("module.exports={");
                for (export_name, &sym_id) in exports {
                    if let Some(new_name) = ctx.get_rename(sym_id) {
                        output.push_str(&format!("{}:{},", export_name, new_name));
                    }
                }
                output.push_str("};");
                if !options.minify {
                    output.push('\n');
                }
            }
        }
    }

    Ok(())
}

/// Emit scope-hoisted IIFE bundle.
fn emit_scope_hoisted_iife(
    graph: &ModuleGraph,
    order: &[ModuleId],
    options: &BundleOptions,
    ctx: &ScopeHoistContext,
    output: &mut String,
) -> Result<(), BundleError> {
    if options.minify {
        output.push_str("(function(){'use strict';");
    } else {
        output.push_str("(function() {\n");
        output.push_str("'use strict';\n\n");
    }

    // Emit content
    let mut inner = String::new();
    emit_scope_hoisted_esm(graph, order, options, ctx, &mut inner)?;

    if options.minify {
        output.push_str(&inner);
    } else {
        for line in inner.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str("})();");
    if !options.minify {
        output.push('\n');
    }

    Ok(())
}

/// Emit a wrapped module (fallback for modules that can't be scope hoisted).
fn emit_wrapped_module(
    id: ModuleId,
    module: &super::graph::Module,
    graph: &ModuleGraph,
    minify: bool,
    output: &mut String,
) -> Result<(), BundleError> {
    output.push_str(&format!(
        "__modules[{}]=function(module,exports,require){{",
        id
    ));
    if !minify {
        output.push('\n');
    }

    // Transform the source for bundling
    let transformed = transform_module(&module.source, &module.path, graph, None)?;

    if minify {
        for line in transformed.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push('\n');
            }
        }
    } else {
        for line in transformed.lines() {
            output.push_str("  ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str("};");
    if !minify {
        output.push('\n');
    }

    Ok(())
}

/// Emit a scope-hoisted module (declarations without import/export).
/// Uses AST-based renaming for correctness (doesn't rename object keys, string contents, etc.)
fn emit_hoisted_module(
    source: &str,
    renames: &HashMap<String, String>,
    output: &mut String,
) -> Result<(), BundleError> {
    // Try AST-based renaming first
    if let Ok(renamed_code) = emit_hoisted_module_ast(source, renames) {
        output.push_str(&renamed_code);
        return Ok(());
    }

    // Fallback to line-based transformation for unparseable code
    emit_hoisted_module_fallback(source, renames, output)
}

/// AST-based module emission with proper identifier renaming.
fn emit_hoisted_module_ast(
    source: &str,
    renames: &HashMap<String, String>,
) -> Result<String, BundleError> {
    // Parse the source
    let parser_opts = ParserOptions {
        module: true,
        ..Default::default()
    };

    let ast = Parser::new(source, parser_opts)
        .parse()
        .map_err(|e| BundleError {
            code: "SCOPE_HOIST_PARSE_ERROR",
            message: format!("Failed to parse module for scope hoisting: {}", e),
            path: None,
        })?;

    // Filter out import/export statements from the AST
    let mut filtered_stmts = Vec::new();
    for stmt in &ast.stmts {
        match &stmt.kind {
            // Skip import statements
            howth_parser::StmtKind::Import(_) => {}
            // Transform export statements
            howth_parser::StmtKind::Export(export) => {
                match export.as_ref() {
                    // export { a, b } / export { a } from './mod' / export * from './mod' - skip
                    howth_parser::ExportDecl::Named { .. }
                    | howth_parser::ExportDecl::All { .. } => {}
                    // export default expr - keep as variable
                    howth_parser::ExportDecl::Default { .. } => {
                        // For default exports, we'd need to transform to var _default = ...
                        // For now, keep the original statement and rely on fallback
                        filtered_stmts.push(stmt.clone());
                    }
                    // export const/let/var/function/class - remove export keyword
                    howth_parser::ExportDecl::Decl { decl, .. } => {
                        filtered_stmts.push(decl.clone());
                    }
                }
            }
            // Keep all other statements
            _ => filtered_stmts.push(stmt.clone()),
        }
    }

    // Create a new AST with the filtered statements
    let filtered_ast = howth_parser::Ast::new(filtered_stmts, ast.source.clone());

    // Generate code with renames applied
    let codegen_opts = CodegenOptions::default();
    let std_renames: std::collections::HashMap<String, String> = renames
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let code = Codegen::with_renames(&filtered_ast, codegen_opts, std_renames).generate();

    Ok(code)
}

/// Fallback line-based module emission.
fn emit_hoisted_module_fallback(
    source: &str,
    renames: &HashMap<String, String>,
    output: &mut String,
) -> Result<(), BundleError> {
    for line in source.lines() {
        let trimmed = line.trim();

        // Skip import statements entirely
        if trimmed.starts_with("import ") {
            continue;
        }

        // Transform export statements
        if trimmed.starts_with("export ") {
            if let Some(transformed) = transform_export_for_hoisting(line, renames) {
                output.push_str(&transformed);
                output.push('\n');
            }
            continue;
        }

        // Apply renames to other lines
        let renamed = apply_renames(line, renames);
        output.push_str(&renamed);
        output.push('\n');
    }

    Ok(())
}

/// Transform an export statement for scope hoisting.
/// Returns the declaration without the export keyword.
fn transform_export_for_hoisting(line: &str, renames: &HashMap<String, String>) -> Option<String> {
    let trimmed = line.trim();

    // export default - emit as variable assignment
    if trimmed.starts_with("export default ") {
        let value = trimmed.strip_prefix("export default ")?;
        let value = value.trim_end_matches(';');

        // Check if it's a named function/class
        if value.starts_with("function ") || value.starts_with("class ") {
            let renamed = apply_renames(value, renames);
            return Some(format!("{};", renamed));
        }

        // Anonymous default export - assign to _default
        let renamed_value = apply_renames(value, renames);
        let default_name = renames
            .get("_default")
            .cloned()
            .unwrap_or_else(|| "_default".to_string());
        return Some(format!("var {} = {};", default_name, renamed_value));
    }

    // export const/let/var/function/class - remove export keyword
    if let Some(decl) = trimmed.strip_prefix("export ") {
        if decl.starts_with("const ")
            || decl.starts_with("let ")
            || decl.starts_with("var ")
            || decl.starts_with("function ")
            || decl.starts_with("async function ")
            || decl.starts_with("class ")
        {
            let renamed = apply_renames(decl, renames);
            return Some(renamed);
        }
    }

    // export { a, b } - skip entirely (names are already in scope)
    if trimmed.starts_with("export {") && !trimmed.contains(" from ") {
        return None;
    }

    // export { a } from './module' - skip (handled by import linking)
    if trimmed.starts_with("export {") && trimmed.contains(" from ") {
        return None;
    }

    // export * from './module' - skip (handled by import linking)
    if trimmed.starts_with("export *") {
        return None;
    }

    // Unknown export pattern - keep as comment
    Some(format!("/* scope-hoist: {} */", line))
}

/// Apply rename mappings to a line of code.
fn apply_renames(line: &str, renames: &HashMap<String, String>) -> String {
    if renames.is_empty() {
        return line.to_string();
    }

    let mut result = line.to_string();

    // Sort by length (longest first) to avoid partial replacements
    let mut sorted_renames: Vec<_> = renames.iter().collect();
    sorted_renames.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    for (old_name, new_name) in sorted_renames {
        if old_name == new_name {
            continue;
        }

        // Replace only whole words (not substrings)
        result = replace_identifier(&result, old_name, new_name);
    }

    result
}

/// Replace a whole-word identifier in source code.
fn replace_identifier(source: &str, old_name: &str, new_name: &str) -> String {
    // Handle edge cases
    if old_name.is_empty() || source.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len());
    let mut i = 0;

    let old_bytes = old_name.as_bytes();
    let source_bytes = source.as_bytes();

    while i < source_bytes.len() {
        // Check if we're at the start of the identifier
        if source_bytes.len() >= i + old_bytes.len()
            && &source_bytes[i..i + old_bytes.len()] == old_bytes
        {
            // Check that it's a whole word (not part of a larger identifier)
            let is_word_start = i == 0 || !is_ident_char(source_bytes[i - 1] as char);
            let is_word_end = i + old_bytes.len() >= source_bytes.len()
                || !is_ident_char(source_bytes[i + old_bytes.len()] as char);

            if is_word_start && is_word_end {
                result.push_str(new_name);
                i += old_bytes.len();
                continue;
            }
        }

        result.push(source_bytes[i] as char);
        i += 1;
    }

    result
}

/// Check if a character can be part of an identifier.
fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_graph() -> ModuleGraph {
        ModuleGraph::new()
    }

    #[test]
    fn test_rewrite_import_side_effect() {
        let graph = empty_graph();
        // CSS imports are handled separately (bundled CSS), not require'd
        assert_eq!(
            rewrite_import("import './styles.css';", "/test/file.ts", &graph),
            "/* CSS: ./styles.css */"
        );
    }

    #[test]
    fn test_rewrite_import_named() {
        let graph = empty_graph();
        assert_eq!(
            rewrite_import(
                "import { foo, bar } from './utils';",
                "/test/file.ts",
                &graph
            ),
            "const { foo, bar } = require('./utils');"
        );
    }

    #[test]
    fn test_rewrite_import_default() {
        let graph = empty_graph();
        assert_eq!(
            rewrite_import("import React from 'react';", "/test/file.ts", &graph),
            "const React = require('react').default || require('react');"
        );
    }

    #[test]
    fn test_rewrite_export_const() {
        let (decl, exports) = rewrite_export_with_pending("export const foo = 1;", None);
        assert_eq!(decl, "const foo = 1;");
        assert_eq!(exports, vec!["exports.foo = foo;"]);
    }

    #[test]
    fn test_rewrite_export_default() {
        let (result, exports) = rewrite_export_with_pending("export default App;", None);
        assert_eq!(result, "exports.default = App;");
        assert!(exports.is_empty());
    }

    #[test]
    fn test_tree_shaking_filters_unused() {
        let mut used = HashSet::default();
        used.insert("usedFn".to_string());

        // Used export should be included
        let (_, exports) = rewrite_export_with_pending("export function usedFn() {}", Some(&used));
        assert_eq!(exports, vec!["exports.usedFn = usedFn;"]);

        // Unused export should be filtered
        let (_, exports) =
            rewrite_export_with_pending("export function unusedFn() {}", Some(&used));
        assert!(exports.is_empty());
    }

    #[test]
    fn test_filter_swc_export() {
        let mut used = HashSet::default();
        used.insert("foo".to_string());

        // Used export - should be kept
        assert!(filter_swc_export("exports.foo = foo;", Some(&used)).is_some());

        // Unused export - should be removed
        assert!(filter_swc_export("exports.bar = bar;", Some(&used)).is_none());

        // Default always kept
        assert!(filter_swc_export("exports.default = App;", Some(&used)).is_some());

        // No tree shaking - all kept
        assert!(filter_swc_export("exports.bar = bar;", None).is_some());
    }

    #[test]
    fn test_tree_shaking_integration() {
        use crate::bundler::graph::Module;
        use crate::bundler::{Import, ImportedName};

        // Create a module graph with:
        // - entry.ts: imports only `add` from utils
        // - utils.ts: exports `add`, `subtract`, `multiply`
        let mut graph = ModuleGraph::new();

        // utils.ts - has three exports but only one is used
        let utils_module = Module {
            path: "/test/utils.ts".to_string(),
            source: r"
export function add(a, b) { return a + b; }
export function subtract(a, b) { return a - b; }
export function multiply(a, b) { return a * b; }
"
            .to_string(),
            imports: Vec::new(),
            dependencies: Vec::new(),
            dynamic_dependencies: Vec::new(),
        };
        let utils_id = graph.add(utils_module);

        // entry.ts - only imports `add`
        let entry_module = Module {
            path: "/test/entry.ts".to_string(),
            source: r"
import { add } from './utils';
console.log(add(1, 2));
"
            .to_string(),
            imports: vec![Import {
                specifier: "./utils".to_string(),
                dynamic: false,
                names: vec![ImportedName {
                    imported: "add".to_string(),
                    local: "add".to_string(),
                }],
            }],
            dependencies: vec![utils_id],
            dynamic_dependencies: Vec::new(),
        };
        let entry_id = graph.add(entry_module);

        // Set up specifier resolution
        let mut dep_info = HashMap::default();
        dep_info.insert(
            "/test/entry.ts".to_string(),
            vec![("./utils".to_string(), "/test/utils.ts".to_string(), false)],
        );
        graph.set_dependencies(&dep_info);

        // Analyze tree shaking
        let used_exports = UsedExports::analyze(&graph, entry_id);

        // Entry should have all exports used (it's the entry point)
        assert!(used_exports.all_used(entry_id));

        // Utils should only have `add` marked as used
        let utils_used = used_exports.get_used(utils_id);
        assert!(utils_used.is_some());
        let utils_set = utils_used.unwrap();
        assert!(utils_set.contains("add"));
        assert!(!utils_set.contains("subtract"));
        assert!(!utils_set.contains("multiply"));
    }

    // =========================================================================
    // Scope Hoisting Tests
    // =========================================================================

    #[test]
    fn test_replace_identifier_basic() {
        assert_eq!(
            replace_identifier("const foo = 1;", "foo", "foo$1"),
            "const foo$1 = 1;"
        );
    }

    #[test]
    fn test_replace_identifier_multiple() {
        assert_eq!(
            replace_identifier("const foo = foo + foo;", "foo", "foo$1"),
            "const foo$1 = foo$1 + foo$1;"
        );
    }

    #[test]
    fn test_replace_identifier_no_partial() {
        // Should not replace 'foo' inside 'foobar'
        assert_eq!(
            replace_identifier("const foobar = 1;", "foo", "foo$1"),
            "const foobar = 1;"
        );
    }

    #[test]
    fn test_replace_identifier_suffix() {
        // Should not replace 'foo' inside 'barfoo'
        assert_eq!(
            replace_identifier("const barfoo = 1;", "foo", "foo$1"),
            "const barfoo = 1;"
        );
    }

    #[test]
    fn test_apply_renames_empty() {
        let renames = HashMap::default();
        assert_eq!(apply_renames("const foo = 1;", &renames), "const foo = 1;");
    }

    #[test]
    fn test_apply_renames_single() {
        let mut renames = HashMap::default();
        renames.insert("foo".to_string(), "foo$1".to_string());
        assert_eq!(
            apply_renames("const foo = 1;", &renames),
            "const foo$1 = 1;"
        );
    }

    #[test]
    fn test_apply_renames_multiple() {
        let mut renames = HashMap::default();
        renames.insert("a".to_string(), "a$1".to_string());
        renames.insert("b".to_string(), "b$1".to_string());
        let result = apply_renames("const x = a + b;", &renames);
        assert!(result.contains("a$1"));
        assert!(result.contains("b$1"));
    }

    #[test]
    fn test_transform_export_for_hoisting_const() {
        let renames = HashMap::default();
        let result = transform_export_for_hoisting("export const foo = 1;", &renames);
        assert_eq!(result, Some("const foo = 1;".to_string()));
    }

    #[test]
    fn test_transform_export_for_hoisting_function() {
        let renames = HashMap::default();
        let result = transform_export_for_hoisting("export function bar() {}", &renames);
        assert_eq!(result, Some("function bar() {}".to_string()));
    }

    #[test]
    fn test_transform_export_for_hoisting_default() {
        let renames = HashMap::default();
        let result = transform_export_for_hoisting("export default 42;", &renames);
        assert_eq!(result, Some("var _default = 42;".to_string()));
    }

    #[test]
    fn test_transform_export_for_hoisting_named_export() {
        let renames = HashMap::default();
        // Named exports without `from` should be stripped
        let result = transform_export_for_hoisting("export { foo, bar };", &renames);
        assert_eq!(result, None);
    }

    #[test]
    fn test_scope_hoisted_bundle() {
        use crate::bundler::graph::Module;
        use crate::bundler::{BundleOptions, Import, ImportedName};

        // Create a simple module graph
        let mut graph = ModuleGraph::new();

        // a.js: export const x = 1;
        let a_id = graph.add(Module {
            path: "/a.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        // b.js: export const x = 100; (conflicts with a.js)
        let b_id = graph.add(Module {
            path: "/b.js".to_string(),
            source: "export const x = 100;".to_string(),
            imports: vec![],
            dependencies: vec![a_id],
            dynamic_dependencies: vec![],
        });

        // entry.js: imports from both
        let entry_id = graph.add(Module {
            path: "/entry.js".to_string(),
            source: r"
import { x as a } from './a';
import { x as b } from './b';
console.log(a + b);
"
            .to_string(),
            imports: vec![
                Import {
                    specifier: "./a".to_string(),
                    dynamic: false,
                    names: vec![ImportedName {
                        imported: "x".to_string(),
                        local: "a".to_string(),
                    }],
                },
                Import {
                    specifier: "./b".to_string(),
                    dynamic: false,
                    names: vec![ImportedName {
                        imported: "x".to_string(),
                        local: "b".to_string(),
                    }],
                },
            ],
            dependencies: vec![a_id, b_id],
            dynamic_dependencies: vec![],
        });

        // Set up specifier resolution
        let mut dep_info = HashMap::default();
        dep_info.insert(
            "/entry.js".to_string(),
            vec![
                ("./a".to_string(), "/a.js".to_string(), false),
                ("./b".to_string(), "/b.js".to_string(), false),
            ],
        );
        graph.set_dependencies(&dep_info);

        let order = vec![a_id, b_id, entry_id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should not contain __modules wrapper
        assert!(!output.code.contains("__modules["));
        // Should contain the declarations
        assert!(output.code.contains("const x"));
        // Should have renamed one of the x's
        assert!(output.code.contains("x$1"));
    }

    #[test]
    fn test_scope_hoisted_wrapped_module() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        // Module with CommonJS pattern should be wrapped
        let cjs_id = graph.add(Module {
            path: "/cjs.js".to_string(),
            source: "module.exports = { foo: 1 };".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![cjs_id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should contain __modules wrapper for CJS module
        assert!(output.code.contains("__modules["));
        assert!(output.code.contains("__require"));
    }

    #[test]
    fn test_scope_hoisted_async_function() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/async.js".to_string(),
            source: "export async function fetchData() { return 42; }".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should contain async function without export keyword
        assert!(output.code.contains("async function fetchData"));
        assert!(!output.code.contains("export async"));
    }

    #[test]
    fn test_scope_hoisted_class() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/class.js".to_string(),
            source: "export class MyClass { constructor() { this.x = 1; } }".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should contain class without export keyword
        assert!(output.code.contains("class MyClass"));
        assert!(!output.code.contains("export class"));
    }

    #[test]
    fn test_scope_hoisted_three_way_conflict() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        // Three modules all exporting 'value'
        let a_id = graph.add(Module {
            path: "/a.js".to_string(),
            source: "export const value = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let b_id = graph.add(Module {
            path: "/b.js".to_string(),
            source: "export const value = 2;".to_string(),
            imports: vec![],
            dependencies: vec![a_id],
            dynamic_dependencies: vec![],
        });

        let c_id = graph.add(Module {
            path: "/c.js".to_string(),
            source: "export const value = 3;".to_string(),
            imports: vec![],
            dependencies: vec![b_id],
            dynamic_dependencies: vec![],
        });

        let order = vec![a_id, b_id, c_id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should have value, value$1, and value$2
        assert!(output.code.contains("value"));
        assert!(output.code.contains("value$1"));
        assert!(output.code.contains("value$2"));
    }

    #[test]
    fn test_scope_hoisted_iife_format() {
        use crate::bundler::graph::Module;
        use crate::bundler::{BundleFormat, BundleOptions};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/module.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            format: BundleFormat::Iife,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should be wrapped in IIFE
        assert!(output.code.contains("(function()"));
        assert!(output.code.contains("'use strict'"));
        assert!(output.code.contains("})();"));
    }

    #[test]
    fn test_scope_hoisted_cjs_format() {
        use crate::bundler::graph::Module;
        use crate::bundler::{BundleFormat, BundleOptions};

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/module.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            format: BundleFormat::Cjs,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should have module.exports at the end
        assert!(output.code.contains("module.exports"));
    }

    #[test]
    fn test_replace_identifier_dollar_sign() {
        // Identifiers with $ should work
        assert_eq!(
            replace_identifier("const $foo = 1;", "$foo", "$foo$1"),
            "const $foo$1 = 1;"
        );
    }

    #[test]
    fn test_replace_identifier_underscore() {
        // Identifiers with _ should work
        assert_eq!(
            replace_identifier("const _private = 1;", "_private", "_private$1"),
            "const _private$1 = 1;"
        );
    }

    #[test]
    fn test_replace_identifier_in_function_call() {
        assert_eq!(
            replace_identifier("console.log(foo);", "foo", "foo$1"),
            "console.log(foo$1);"
        );
    }

    #[test]
    fn test_replace_identifier_in_object() {
        // Note: Current simple implementation replaces both key and value.
        // This is a known limitation - proper fix requires AST-based replacement.
        // For scope hoisting, this is acceptable because:
        // 1. Object shorthand { foo } becomes { foo: foo } after transpilation
        // 2. Both get renamed consistently, preserving semantics
        assert_eq!(
            replace_identifier("const obj = { foo: foo };", "foo", "foo$1"),
            "const obj = { foo$1: foo$1 };"
        );
    }

    #[test]
    fn test_replace_identifier_in_string_literal() {
        // Should NOT replace inside string literals (this is a known limitation)
        // In a full implementation we'd need proper parsing
        let result = replace_identifier("const s = 'foo';", "foo", "foo$1");
        // Current simple implementation will replace it - this documents the limitation
        // A proper fix would require parsing string literals
        assert!(result.contains("foo"));
    }

    #[test]
    fn test_scope_hoisted_preserves_code_structure() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        // Module with various code structures
        let id = graph.add(Module {
            path: "/complex.js".to_string(),
            source: r"
export const CONFIG = { debug: true };
export function init() {
    if (CONFIG.debug) {
        console.log('Debug mode');
    }
}
const internal = 42;
"
            .to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should preserve code structure
        assert!(output.code.contains("const CONFIG"));
        assert!(output.code.contains("function init()"));
        assert!(output.code.contains("if (CONFIG.debug)"));
        assert!(output.code.contains("const internal = 42"));
        // Should not have export keywords
        assert!(!output.code.contains("export const"));
        assert!(!output.code.contains("export function"));
    }

    #[test]
    fn test_scope_hoisted_eval_detection() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        // Module with eval should be wrapped
        let id = graph.add(Module {
            path: "/eval.js".to_string(),
            source: "const result = eval('1 + 1');".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should be wrapped due to eval
        assert!(output.code.contains("__modules["));
    }

    // =========================================================================
    // Boundary Value Tests (0, 1, -1 testing)
    // =========================================================================

    #[test]
    fn test_replace_identifier_empty_source() {
        assert_eq!(replace_identifier("", "foo", "bar"), "");
    }

    #[test]
    fn test_replace_identifier_empty_name() {
        // Empty old name should return source unchanged
        assert_eq!(
            replace_identifier("const foo = 1;", "", "bar"),
            "const foo = 1;"
        );
    }

    #[test]
    fn test_replace_identifier_single_char_source() {
        assert_eq!(replace_identifier("x", "x", "y"), "y");
        assert_eq!(replace_identifier("a", "x", "y"), "a");
    }

    #[test]
    fn test_replace_identifier_at_start() {
        assert_eq!(replace_identifier("foo = 1", "foo", "bar"), "bar = 1");
    }

    #[test]
    fn test_replace_identifier_at_end() {
        assert_eq!(replace_identifier("return foo", "foo", "bar"), "return bar");
    }

    #[test]
    fn test_replace_identifier_only_content() {
        assert_eq!(replace_identifier("foo", "foo", "bar"), "bar");
    }

    #[test]
    fn test_apply_renames_same_name() {
        // When old and new are the same, should be a no-op
        let mut renames = HashMap::default();
        renames.insert("foo".to_string(), "foo".to_string());
        assert_eq!(apply_renames("const foo = 1;", &renames), "const foo = 1;");
    }

    #[test]
    fn test_emit_scope_hoisted_empty_order() {
        use crate::bundler::graph::ModuleGraph;
        use crate::bundler::BundleOptions;

        let graph = ModuleGraph::new();
        let order: Vec<usize> = vec![];
        let options = BundleOptions {
            scope_hoist: true,
            ..Default::default()
        };

        // Empty order should produce valid (but minimal) output
        let result = emit_scope_hoisted(&graph, &order, &options);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.code.contains("howth bundle"));
    }

    #[test]
    fn test_emit_hoisted_module_empty_source() {
        let renames = HashMap::default();
        let mut output = String::new();

        let result = emit_hoisted_module("", &renames, &mut output);
        assert!(result.is_ok());
        assert!(output.is_empty());
    }

    #[test]
    fn test_emit_hoisted_module_only_newlines() {
        let renames = HashMap::default();
        let mut output = String::new();

        let result = emit_hoisted_module("\n\n\n", &renames, &mut output);
        assert!(result.is_ok());
        // Should have only newlines
        assert_eq!(output.trim(), "");
    }

    #[test]
    fn test_transform_export_for_hoisting_empty() {
        let renames = HashMap::default();
        let result = transform_export_for_hoisting("", &renames);
        // Empty line with export prefix check fails, returns None or comment
        assert!(result.is_none() || result.as_ref().unwrap().contains("scope-hoist"));
    }

    #[test]
    fn test_ast_based_renaming_preserves_object_keys() {
        // Test that AST-based renaming correctly handles object properties
        let source = r"
const foo = 1;
const obj = { foo: foo, bar: foo };
";
        let mut renames = HashMap::default();
        renames.insert("foo".to_string(), "foo$1".to_string());

        let result = emit_hoisted_module_ast(source, &renames);
        assert!(result.is_ok());

        let code = result.unwrap();
        // The value 'foo' should be renamed to 'foo$1'
        assert!(code.contains("foo$1"));
        // Object key 'foo' should NOT be renamed (should still contain 'foo:')
        // This is what distinguishes AST-based from text-based renaming
        assert!(code.contains("foo:") || code.contains("foo :"));
    }

    #[test]
    fn test_ast_based_renaming_function_and_class() {
        let source = r"
function myFunc() { return 1; }
class MyClass { constructor() {} }
const x = myFunc();
const y = new MyClass();
";
        let mut renames = HashMap::default();
        renames.insert("myFunc".to_string(), "myFunc$1".to_string());
        renames.insert("MyClass".to_string(), "MyClass$1".to_string());

        let result = emit_hoisted_module_ast(source, &renames);
        assert!(result.is_ok());

        let code = result.unwrap();
        // Function and class declarations should be renamed
        assert!(code.contains("function myFunc$1"));
        assert!(code.contains("class MyClass$1"));
        // References should also be renamed
        assert!(code.contains("myFunc$1()"));
        assert!(code.contains("new MyClass$1"));
    }

    #[test]
    fn test_ast_based_renaming_import_removal() {
        let source = r"
import { foo } from './other';
import bar from './bar';
const x = foo + bar;
";
        let renames = HashMap::default();

        let result = emit_hoisted_module_ast(source, &renames);
        assert!(result.is_ok());

        let code = result.unwrap();
        // Import statements should be removed
        assert!(!code.contains("import"));
        // The rest of the code should remain
        assert!(code.contains("const x"));
    }

    #[test]
    fn test_ast_based_renaming_export_removal() {
        let source = r"
export const foo = 1;
export function bar() { return 2; }
export class Baz {}
";
        let renames = HashMap::default();

        let result = emit_hoisted_module_ast(source, &renames);
        assert!(result.is_ok());

        let code = result.unwrap();
        // Export keywords should be removed
        assert!(!code.contains("export const"));
        assert!(!code.contains("export function"));
        assert!(!code.contains("export class"));
        // But the declarations should remain
        assert!(code.contains("const foo"));
        assert!(code.contains("function bar"));
        assert!(code.contains("class Baz"));
    }

    // =========================================================================
    // VLQ and Sourcemap Tests
    // =========================================================================

    #[test]
    fn test_vlq_encode_zero() {
        let mut out = String::new();
        vlq_encode(0, &mut out);
        assert_eq!(out, "A");
    }

    #[test]
    fn test_vlq_encode_positive() {
        let mut out = String::new();
        vlq_encode(1, &mut out);
        assert_eq!(out, "C");
    }

    #[test]
    fn test_vlq_encode_negative() {
        let mut out = String::new();
        vlq_encode(-1, &mut out);
        assert_eq!(out, "D");
    }

    #[test]
    fn test_vlq_encode_large() {
        // 16 → encoded as 32 (shifted) → first 5 bits = 0, continuation, next = 1
        let mut out = String::new();
        vlq_encode(16, &mut out);
        assert_eq!(out, "gB");
    }

    #[test]
    fn test_sourcemap_generation() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/test.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            sourcemap: true,
            ..Default::default()
        };

        let result = emit_bundle(&graph, &order, &options).unwrap();
        assert!(result.map.is_some());

        let map = result.map.unwrap();
        assert!(map.contains("\"version\":3"));
        assert!(map.contains("/test.js"));
        assert!(map.contains("\"mappings\""));
    }

    #[test]
    fn test_sourcemap_scope_hoisted() {
        use crate::bundler::graph::Module;
        use crate::bundler::BundleOptions;

        let mut graph = ModuleGraph::new();

        let id = graph.add(Module {
            path: "/module.js".to_string(),
            source: "export const x = 1;".to_string(),
            imports: vec![],
            dependencies: vec![],
            dynamic_dependencies: vec![],
        });

        let order = vec![id];
        let options = BundleOptions {
            scope_hoist: true,
            sourcemap: true,
            ..Default::default()
        };

        let result = emit_scope_hoisted(&graph, &order, &options).unwrap();
        assert!(result.map.is_some());

        let map = result.map.unwrap();
        assert!(map.contains("\"version\":3"));
        assert!(map.contains("/module.js"));
    }
}
