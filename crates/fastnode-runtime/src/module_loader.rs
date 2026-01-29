//! ES Module loader for howth runtime.
//!
//! Handles resolving and loading ES modules from the filesystem.

use deno_core::error::AnyError;
use deno_core::futures::FutureExt;
use deno_core::{
    ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType,
    RequestedModuleType, ResolutionKind,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Minimal package.json structure for module resolution.
#[derive(Debug, Deserialize, Default)]
struct PackageJson {
    /// Main entry point (CommonJS or fallback)
    main: Option<String>,
    /// ES module entry point
    module: Option<String>,
    /// Modern exports field
    exports: Option<serde_json::Value>,
    /// Package type (module or commonjs)
    #[serde(rename = "type")]
    pkg_type: Option<String>,
}

/// A map of virtual module paths to their source code.
/// Modules in this map are served from memory without disk I/O.
pub type VirtualModuleMap = Rc<RefCell<HashMap<String, String>>>;

/// Howth's custom module loader.
pub struct HowthModuleLoader {
    /// Base directory for resolving relative imports.
    cwd: PathBuf,
    /// Virtual modules that live in memory (no disk I/O needed).
    virtual_modules: Option<VirtualModuleMap>,
}

impl HowthModuleLoader {
    /// Create a new module loader with the given working directory.
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            virtual_modules: None,
        }
    }

    /// Create a new module loader with virtual modules for in-memory loading.
    pub fn new_with_virtual_modules(cwd: PathBuf, virtual_modules: VirtualModuleMap) -> Self {
        Self {
            cwd,
            virtual_modules: Some(virtual_modules),
        }
    }

    /// Resolve a module specifier to a file path.
    fn resolve_path(
        &self,
        specifier: &str,
        referrer: &ModuleSpecifier,
    ) -> Result<PathBuf, AnyError> {
        // Handle relative imports
        if specifier.starts_with("./") || specifier.starts_with("../") {
            let referrer_path = referrer
                .to_file_path()
                .map_err(|_| AnyError::msg(format!("Invalid referrer path: {}", referrer)))?;
            let referrer_dir = referrer_path.parent().unwrap_or(Path::new("."));
            let resolved = referrer_dir.join(specifier);
            return self.resolve_with_extensions(&resolved);
        }

        // Handle absolute imports
        if specifier.starts_with('/') {
            let resolved = PathBuf::from(specifier);
            return self.resolve_with_extensions(&resolved);
        }

        // Handle file:// URLs
        if specifier.starts_with("file://") {
            let url = ModuleSpecifier::parse(specifier)?;
            let path = url
                .to_file_path()
                .map_err(|_| AnyError::msg(format!("Invalid file URL: {}", specifier)))?;
            // Try resolving the file URL path first
            if let Ok(resolved) = self.resolve_with_extensions(&path) {
                return Ok(resolved);
            }
            // If the file doesn't exist, try extracting a package name and resolving via node_modules
            // This handles cases like pathToFileURL('@next/swc-wasm-nodejs') which produces
            // file:///cwd/@next/swc-wasm-nodejs but the actual package is in node_modules
            let path_str = path.to_string_lossy();
            // Check if the path looks like it could be a package in cwd (not in node_modules already)
            if !path_str.contains("node_modules") {
                // Try to extract the package name from the end of the path
                // For scoped packages like @next/swc-wasm-nodejs
                let filename = path.file_name().map(|f| f.to_string_lossy().to_string());
                let parent_name = path.parent().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string());

                let bare_specifier = if let Some(ref parent) = parent_name {
                    if parent.starts_with('@') {
                        // Scoped package: parent is @scope, filename is package name
                        if let Some(ref name) = filename {
                            Some(format!("{}/{}", parent, name))
                        } else {
                            None
                        }
                    } else {
                        filename.clone()
                    }
                } else {
                    filename.clone()
                };

                if let Some(specifier_name) = bare_specifier {
                    if let Ok(resolved) = self.resolve_bare_specifier(&specifier_name, referrer) {
                        return Ok(resolved);
                    }
                }
            }
            return Err(AnyError::msg(format!("Cannot find module: '{}'", path.display())));
        }

        // Bare specifiers - resolve from node_modules
        self.resolve_bare_specifier(specifier, referrer)
    }

    /// Normalize a path by removing `.` and resolving `..` components.
    fn normalize_path(path: &Path) -> PathBuf {
        use std::path::Component;
        let mut result = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                other => result.push(other),
            }
        }
        result
    }

    /// Check if a path exists in the virtual module map.
    fn is_virtual_module(&self, path: &Path) -> bool {
        if let Some(ref vm) = self.virtual_modules {
            let normalized = Self::normalize_path(path);
            vm.borrow()
                .contains_key(&normalized.to_string_lossy().to_string())
        } else {
            false
        }
    }

    /// Try to resolve a path with various extensions.
    fn resolve_with_extensions(&self, path: &Path) -> Result<PathBuf, AnyError> {
        // Check virtual module map first (no disk I/O needed)
        if self.is_virtual_module(path) {
            return Ok(path.to_path_buf());
        }

        // If the path already has an extension and exists, use it
        if path.exists() && path.is_file() {
            return Ok(path.to_path_buf());
        }

        // Try common extensions
        let extensions = [".ts", ".tsx", ".js", ".jsx", ".mts", ".mjs"];
        for ext in extensions {
            let with_ext = path.with_extension(ext.trim_start_matches('.'));
            if with_ext.exists() && with_ext.is_file() {
                return Ok(with_ext);
            }

            // Also try appending extension (for paths without extension)
            let mut appended = path.as_os_str().to_owned();
            appended.push(ext);
            let appended_path = PathBuf::from(appended);
            if appended_path.exists() && appended_path.is_file() {
                return Ok(appended_path);
            }
        }

        // Try index files in directory
        if path.is_dir() {
            for ext in extensions {
                let index = path.join(format!("index{}", ext));
                if index.exists() && index.is_file() {
                    return Ok(index);
                }
            }
        }

        Err(AnyError::msg(format!(
            "Cannot find module: '{}'",
            path.display()
        )))
    }

    /// Resolve a bare specifier (e.g., 'lodash' or 'lodash/fp') from node_modules.
    fn resolve_bare_specifier(
        &self,
        specifier: &str,
        referrer: &ModuleSpecifier,
    ) -> Result<PathBuf, AnyError> {
        // Parse the specifier into package name and subpath
        let (package_name, subpath) = self.parse_bare_specifier(specifier);

        // Find the package in node_modules
        let package_dir = self.find_package(&package_name, referrer)?;

        // If there's a subpath, resolve it directly
        if let Some(subpath) = subpath {
            let subpath_resolved = package_dir.join(&subpath);
            return self.resolve_with_extensions(&subpath_resolved);
        }

        // Otherwise, resolve the package entry point
        self.resolve_package_entry(&package_dir, &package_name)
    }

    /// Parse a bare specifier into package name and optional subpath.
    /// - 'lodash' -> ('lodash', None)
    /// - 'lodash/fp' -> ('lodash', Some('fp'))
    /// - '@scope/pkg' -> ('@scope/pkg', None)
    /// - '@scope/pkg/sub' -> ('@scope/pkg', Some('sub'))
    fn parse_bare_specifier<'a>(&self, specifier: &'a str) -> (String, Option<String>) {
        if specifier.starts_with('@') {
            // Scoped package: @scope/package or @scope/package/subpath
            let parts: Vec<&str> = specifier.splitn(3, '/').collect();
            if parts.len() >= 2 {
                let package_name = format!("{}/{}", parts[0], parts[1]);
                let subpath = if parts.len() > 2 {
                    Some(parts[2].to_string())
                } else {
                    None
                };
                return (package_name, subpath);
            }
        }

        // Regular package: package or package/subpath
        if let Some(slash_pos) = specifier.find('/') {
            let package_name = specifier[..slash_pos].to_string();
            let subpath = specifier[slash_pos + 1..].to_string();
            (package_name, Some(subpath))
        } else {
            (specifier.to_string(), None)
        }
    }

    /// Find a package in node_modules, walking up from the referrer.
    fn find_package(
        &self,
        package_name: &str,
        referrer: &ModuleSpecifier,
    ) -> Result<PathBuf, AnyError> {
        // Start from the referrer's directory
        let start_dir = if let Ok(path) = referrer.to_file_path() {
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.cwd.clone())
        } else {
            self.cwd.clone()
        };

        // Walk up the directory tree looking for node_modules
        let mut current = start_dir.as_path();
        loop {
            let node_modules = current.join("node_modules");
            let package_dir = node_modules.join(package_name);

            if package_dir.is_dir() {
                return Ok(package_dir);
            }

            // Move up to parent directory
            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }

        // Also try the current working directory (for scripts that chdir into a project)
        if let Ok(actual_cwd) = std::env::current_dir() {
            if actual_cwd != start_dir {
                let mut current = actual_cwd.as_path();
                loop {
                    let node_modules = current.join("node_modules");
                    let package_dir = node_modules.join(package_name);

                    if package_dir.is_dir() {
                        return Ok(package_dir);
                    }

                    // Move up to parent directory
                    match current.parent() {
                        Some(parent) => current = parent,
                        None => break,
                    }
                }
            }
        }

        Err(AnyError::msg(format!(
            "Cannot find package '{}' in node_modules",
            package_name
        )))
    }

    /// Resolve the entry point of a package using package.json.
    fn resolve_package_entry(
        &self,
        package_dir: &Path,
        package_name: &str,
    ) -> Result<PathBuf, AnyError> {
        let package_json_path = package_dir.join("package.json");

        // Read and parse package.json
        let pkg: PackageJson = if package_json_path.exists() {
            let content = std::fs::read_to_string(&package_json_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PackageJson::default()
        };

        // Try exports field first (modern resolution)
        if let Some(exports) = &pkg.exports {
            if let Some(entry) = self.resolve_exports(exports, ".") {
                let resolved = package_dir.join(&entry);
                if resolved.exists() {
                    return Ok(resolved);
                }
                // Try with extensions
                if let Ok(path) = self.resolve_with_extensions(&resolved) {
                    return Ok(path);
                }
            }
        }

        // Try module field (ES modules)
        if let Some(module) = &pkg.module {
            let resolved = package_dir.join(module);
            if resolved.exists() {
                return Ok(resolved);
            }
            if let Ok(path) = self.resolve_with_extensions(&resolved) {
                return Ok(path);
            }
        }

        // Try main field (CommonJS or fallback)
        if let Some(main) = &pkg.main {
            let resolved = package_dir.join(main);
            if resolved.exists() {
                return Ok(resolved);
            }
            if let Ok(path) = self.resolve_with_extensions(&resolved) {
                return Ok(path);
            }
        }

        // Default to index.js
        self.resolve_with_extensions(&package_dir.join("index"))
            .map_err(|_| {
                AnyError::msg(format!(
                    "Cannot find entry point for package '{}'",
                    package_name
                ))
            })
    }

    /// Resolve the exports field of package.json.
    /// Handles both string exports and conditional exports.
    fn resolve_exports(&self, exports: &serde_json::Value, subpath: &str) -> Option<String> {
        match exports {
            // Simple string export: "exports": "./dist/index.js"
            serde_json::Value::String(s) if subpath == "." => Some(s.clone()),

            // Object exports
            serde_json::Value::Object(map) => {
                // Check for subpath match first
                if let Some(value) = map.get(subpath) {
                    return self.resolve_export_value(value);
                }

                // Check for "." entry (main export)
                if subpath == "." {
                    if let Some(value) = map.get(".") {
                        return self.resolve_export_value(value);
                    }

                    // If no "." entry, this might be conditional exports at the top level
                    // Try import/default/node conditions
                    return self.resolve_export_value(exports);
                }

                None
            }

            _ => None,
        }
    }

    /// Resolve a single export value, handling conditional exports.
    fn resolve_export_value(&self, value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(s) => Some(s.clone()),

            serde_json::Value::Object(conditions) => {
                // Priority: import > module > default > node > require
                let priority = ["import", "module", "default", "node", "require"];
                for condition in priority {
                    if let Some(v) = conditions.get(condition) {
                        if let Some(resolved) = self.resolve_export_value(v) {
                            return Some(resolved);
                        }
                    }
                }
                None
            }

            _ => None,
        }
    }

    /// Load and optionally transpile a module.
    fn load_module(&self, path: &Path) -> Result<(String, ModuleType), AnyError> {
        let source = std::fs::read_to_string(path)?;

        // Determine if transpilation is needed
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let needs_transpile = matches!(ext.to_lowercase().as_str(), "ts" | "tsx" | "jsx" | "mts");

        let code = if needs_transpile {
            self.transpile(&source, path)?
        } else {
            source
        };

        // Detect if this is a CommonJS module and wrap it for ESM compatibility
        // CommonJS indicators: module.exports, exports., require(
        let is_commonjs = code.contains("module.exports")
            || code.contains("exports.")
            || (code.contains("require(") && !code.contains("import "));

        if std::env::var("DEBUG_MODULES").is_ok() {
            eprintln!("[DEBUG] Loading module: {} (is_commonjs={})", path.display(), is_commonjs);
        }

        let wrapped_code = if is_commonjs {
            let result = self.wrap_commonjs(&code, path)?;
            if std::env::var("DEBUG_CJS_WRAPPER").is_ok() {
                eprintln!("[DEBUG] CJS wrapper length: {}", result.len());
                // Print line by line with line numbers
                for (i, line) in result.lines().enumerate() {
                    eprintln!("[DEBUG] {:4}: {}", i + 1, if line.len() > 100 { &line[..100] } else { line });
                    if i > 35 { break; } // Stop after line 35
                }
            }
            result
        } else {
            code
        };

        Ok((wrapped_code, ModuleType::JavaScript))
    }

    /// Wrap a CommonJS module to work as ESM.
    fn wrap_commonjs(&self, source: &str, path: &Path) -> Result<String, AnyError> {
        let path_str = path.display().to_string().replace('\\', "/");
        let dir_str = path
            .parent()
            .map(|p| p.display().to_string().replace('\\', "/"))
            .unwrap_or_else(|| ".".to_string());

        // Use JSON serialization to safely escape the source code
        let escaped_source = serde_json::to_string(source)
            .map_err(|e| AnyError::msg(format!("Failed to escape source: {}", e)))?;

        // Scan for named exports in the CJS source
        let named_exports = self.scan_cjs_exports(source);
        let export_declarations = self.generate_export_declarations(&named_exports);

        // Wrap the CommonJS module and execute it with proper CJS environment
        Ok(format!(
            r#"
// CommonJS module wrapper
const __howth_cjs_source__ = {};
const __howth_cjs_filename__ = "{}";
const __howth_cjs_dirname__ = "{}";

// Set up CommonJS environment
const __howth_cjs_module__ = {{ exports: {{}}, id: __howth_cjs_filename__, filename: __howth_cjs_filename__, loaded: false, path: __howth_cjs_dirname__ }};
const __howth_cjs_exports__ = __howth_cjs_module__.exports;

// Create a require function for this module
const __howth_cjs_require__ = globalThis.__howth_modules?.["module"]?.createRequire
    ? globalThis.__howth_modules["module"].createRequire(__howth_cjs_filename__)
    : globalThis.require;

// Execute the CommonJS module in a function scope
(function(exports, require, module, __filename, __dirname) {{
    eval(__howth_cjs_source__);
}}).call(__howth_cjs_exports__, __howth_cjs_exports__, __howth_cjs_require__, __howth_cjs_module__, __howth_cjs_filename__, __howth_cjs_dirname__);

__howth_cjs_module__.loaded = true;

// The module.exports is the default export
const __howth_result__ = __howth_cjs_module__.exports;

// Export as ESM default
export default __howth_result__;

// Named exports extracted from CJS
{}
"#,
            escaped_source, path_str, dir_str, export_declarations
        ))
    }

    /// Scan CommonJS source code for exported names.
    fn scan_cjs_exports(&self, source: &str) -> Vec<String> {
        use std::collections::HashSet;
        let mut exports = HashSet::new();

        // Pattern 1: Object.defineProperty(exports, "name", ...)
        let define_prop_re = regex::Regex::new(r#"Object\.defineProperty\s*\(\s*exports\s*,\s*["'](\w+)["']"#).unwrap();
        for cap in define_prop_re.captures_iter(source) {
            if let Some(name) = cap.get(1) {
                exports.insert(name.as_str().to_string());
            }
        }

        // Pattern 2: exports.name = ...
        let exports_dot_re = regex::Regex::new(r#"exports\.(\w+)\s*="#).unwrap();
        for cap in exports_dot_re.captures_iter(source) {
            if let Some(name) = cap.get(1) {
                exports.insert(name.as_str().to_string());
            }
        }

        // Pattern 3: module.exports = { name1, name2 } or module.exports = { name1: ..., name2: ... }
        // This is harder to parse statically, so we look for simple patterns
        let module_exports_re = regex::Regex::new(r#"module\.exports\s*=\s*\{([^}]+)\}"#).unwrap();
        for cap in module_exports_re.captures_iter(source) {
            if let Some(body) = cap.get(1) {
                // Parse the object body for property names
                let prop_re = regex::Regex::new(r#"(\w+)\s*[,:]"#).unwrap();
                for prop_cap in prop_re.captures_iter(body.as_str()) {
                    if let Some(name) = prop_cap.get(1) {
                        exports.insert(name.as_str().to_string());
                    }
                }
            }
        }

        exports.into_iter().collect()
    }

    /// Generate ESM export declarations for the scanned CJS exports.
    fn generate_export_declarations(&self, exports: &[String]) -> String {
        if exports.is_empty() {
            return String::new();
        }

        // JavaScript reserved keywords that cannot be used as export names
        const RESERVED_KEYWORDS: &[&str] = &[
            "break", "case", "catch", "continue", "debugger", "default", "delete",
            "do", "else", "export", "extends", "finally", "for", "function", "if",
            "import", "in", "instanceof", "new", "return", "super", "switch", "this",
            "throw", "try", "typeof", "var", "void", "while", "with", "yield",
            "class", "const", "enum", "let", "static", "implements", "interface",
            "package", "private", "protected", "public", "await", "null", "true",
            "false", "undefined", "__esModule",
        ];

        let mut declarations = String::new();
        for name in exports {
            // Skip reserved keywords and __esModule
            if RESERVED_KEYWORDS.contains(&name.as_str()) {
                continue;
            }
            // Generate a live getter export
            declarations.push_str(&format!(
                "export const {} = __howth_result__.{};\n",
                name, name
            ));
        }
        declarations
    }

    /// Transpile TypeScript/JSX to JavaScript using SWC.
    fn transpile(&self, source: &str, path: &Path) -> Result<String, AnyError> {
        use fastnode_core::compiler::{CompilerBackend, SwcBackend, TranspileSpec};

        let backend = SwcBackend::new();
        let spec = TranspileSpec::new(path, "");

        let output = backend
            .transpile(&spec, source)
            .map_err(|e| AnyError::msg(format!("Transpilation failed: {}", e.message)))?;

        Ok(output.code)
    }
}

impl ModuleLoader for HowthModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, AnyError> {
        // Handle node: built-in modules
        if specifier.starts_with("node:") {
            return ModuleSpecifier::parse(&format!("howth-builtin:///{}", specifier))
                .map_err(|e| AnyError::msg(format!("Invalid builtin module: {}", e)));
        }

        // Parse referrer as URL
        let referrer_url = if referrer == "." || referrer.is_empty() {
            // Entry point - use cwd
            ModuleSpecifier::from_file_path(&self.cwd.join("__entry__"))
                .map_err(|_| AnyError::msg("Invalid cwd"))?
        } else {
            // Try to parse the referrer as a URL
            match ModuleSpecifier::parse(referrer) {
                Ok(url) => url,
                Err(_) => {
                    // Fall back to cwd for unknown referrers (like eval'd code)
                    let actual_cwd = std::env::current_dir().unwrap_or_else(|_| self.cwd.clone());
                    ModuleSpecifier::from_file_path(&actual_cwd.join("__eval__"))
                        .map_err(|_| AnyError::msg("Invalid cwd"))?
                }
            }
        };

        // Resolve the specifier to a path
        let resolved_path = self.resolve_path(specifier, &referrer_url)?;

        // Convert back to file:// URL
        ModuleSpecifier::from_file_path(&resolved_path)
            .map_err(|_| AnyError::msg(format!("Invalid path: {}", resolved_path.display())))
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: RequestedModuleType,
    ) -> ModuleLoadResponse {
        let specifier = module_specifier.clone();
        let cwd = self.cwd.clone();
        let virtual_modules = self.virtual_modules.clone();

        ModuleLoadResponse::Async(
            async move {
                // Handle built-in modules
                if specifier.scheme() == "howth-builtin" {
                    let module_name = specifier.path().trim_start_matches('/');
                    let code = Self::generate_builtin_module(module_name)?;
                    return Ok(ModuleSource::new(
                        ModuleType::JavaScript,
                        ModuleSourceCode::String(code.into()),
                        &specifier,
                        None,
                    ));
                }

                let path = specifier.to_file_path().map_err(|_| {
                    AnyError::msg(format!("Invalid module specifier: {}", specifier))
                })?;

                // Check virtual module map before hitting disk
                if let Some(ref vm) = virtual_modules {
                    let normalized = Self::normalize_path(&path);
                    let normalized_str = normalized.to_string_lossy();
                    if let Some(code) = vm.borrow().get(normalized_str.as_ref()) {
                        return Ok(ModuleSource::new(
                            ModuleType::JavaScript,
                            ModuleSourceCode::String(code.clone().into()),
                            &specifier,
                            None,
                        ));
                    }
                }

                let loader = HowthModuleLoader::new(cwd);

                let (code, module_type) = loader.load_module(&path)?;

                Ok(ModuleSource::new(
                    module_type,
                    ModuleSourceCode::String(code.into()),
                    &specifier,
                    None,
                ))
            }
            .boxed_local(),
        )
    }
}

impl HowthModuleLoader {
    /// Generate synthetic module code for built-in modules.
    fn generate_builtin_module(module_name: &str) -> Result<String, AnyError> {
        // Check if the module exists in __howth_modules
        let code = format!(
            r#"
            const mod = globalThis.__howth_modules?.["{}"];
            if (!mod) {{
                throw new Error("Built-in module '{}' is not implemented");
            }}
            export default mod;
            export const {{ {} }} = mod;
            "#,
            module_name,
            module_name,
            Self::get_builtin_exports(module_name)
        );
        Ok(code)
    }

    /// Get the named exports for a built-in module.
    fn get_builtin_exports(module_name: &str) -> &'static str {
        match module_name {
            "node:path" | "path" => {
                "join, resolve, dirname, basename, extname, normalize, isAbsolute, relative, parse, format, sep, delimiter, posix, win32, toNamespacedPath"
            }
            "node:fs" | "fs" => {
                "readFileSync, writeFileSync, appendFileSync, existsSync, mkdirSync, rmdirSync, rmSync, unlinkSync, renameSync, copyFileSync, readdirSync, statSync, lstatSync, realpathSync, chmodSync, accessSync, promises, constants, Stats, Dirent, readFile, writeFile, appendFile, mkdir, rmdir, rm, unlink, rename, copyFile, readdir, stat, lstat, realpath, chmod, access, exists, F_OK, R_OK, W_OK, X_OK"
            }
            "node:fs/promises" | "fs/promises" => {
                "readFile, writeFile, appendFile, mkdir, rmdir, rm, unlink, rename, copyFile, readdir, stat, lstat, realpath, chmod, access"
            }
            "node:test" | "test" => {
                "test, describe, it, before, after, beforeEach, afterEach, mock"
            }
            "node:assert" | "assert" => {
                "ok, equal, notEqual, strictEqual, notStrictEqual, deepEqual, notDeepEqual, deepStrictEqual, notDeepStrictEqual, throws, doesNotThrow, rejects, doesNotReject, fail, ifError, match, doesNotMatch, strict, AssertionError"
            }
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_node_modules(temp: &TempDir) {
        // Create node_modules/simple-pkg/index.js
        let simple_pkg = temp.path().join("node_modules/simple-pkg");
        fs::create_dir_all(&simple_pkg).unwrap();
        fs::write(simple_pkg.join("index.js"), "export const x = 1;").unwrap();
        fs::write(simple_pkg.join("package.json"), r#"{"name": "simple-pkg"}"#).unwrap();

        // Create node_modules/with-main with main field
        let with_main = temp.path().join("node_modules/with-main");
        fs::create_dir_all(&with_main).unwrap();
        fs::write(with_main.join("lib.js"), "export const y = 2;").unwrap();
        fs::write(
            with_main.join("package.json"),
            r#"{"name": "with-main", "main": "lib.js"}"#,
        )
        .unwrap();

        // Create node_modules/with-exports with exports field
        let with_exports = temp.path().join("node_modules/with-exports");
        fs::create_dir_all(with_exports.join("dist")).unwrap();
        fs::write(with_exports.join("dist/index.mjs"), "export const z = 3;").unwrap();
        fs::write(
            with_exports.join("package.json"),
            r#"{
            "name": "with-exports",
            "exports": {
                ".": {
                    "import": "./dist/index.mjs",
                    "require": "./dist/index.cjs"
                }
            }
        }"#,
        )
        .unwrap();

        // Create node_modules/with-subpath with subpath exports
        let with_subpath = temp.path().join("node_modules/with-subpath");
        fs::create_dir_all(with_subpath.join("utils")).unwrap();
        fs::write(with_subpath.join("index.js"), "export const a = 1;").unwrap();
        fs::write(with_subpath.join("utils/helper.js"), "export const b = 2;").unwrap();
        fs::write(
            with_subpath.join("package.json"),
            r#"{"name": "with-subpath"}"#,
        )
        .unwrap();

        // Create node_modules/@scope/pkg (scoped package)
        let scoped = temp.path().join("node_modules/@scope/pkg");
        fs::create_dir_all(&scoped).unwrap();
        fs::write(scoped.join("index.js"), "export const scoped = true;").unwrap();
        fs::write(scoped.join("package.json"), r#"{"name": "@scope/pkg"}"#).unwrap();
    }

    #[test]
    fn test_parse_bare_specifier() {
        let loader = HowthModuleLoader::new(PathBuf::from("/tmp"));

        // Simple package
        let (name, subpath) = loader.parse_bare_specifier("lodash");
        assert_eq!(name, "lodash");
        assert_eq!(subpath, None);

        // Package with subpath
        let (name, subpath) = loader.parse_bare_specifier("lodash/fp");
        assert_eq!(name, "lodash");
        assert_eq!(subpath, Some("fp".to_string()));

        // Scoped package
        let (name, subpath) = loader.parse_bare_specifier("@scope/pkg");
        assert_eq!(name, "@scope/pkg");
        assert_eq!(subpath, None);

        // Scoped package with subpath
        let (name, subpath) = loader.parse_bare_specifier("@scope/pkg/utils");
        assert_eq!(name, "@scope/pkg");
        assert_eq!(subpath, Some("utils".to_string()));
    }

    #[test]
    fn test_resolve_simple_package() {
        let temp = TempDir::new().unwrap();
        setup_node_modules(&temp);

        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let resolved = loader
            .resolve_bare_specifier("simple-pkg", &referrer)
            .unwrap();
        assert!(resolved.ends_with("simple-pkg/index.js"));
    }

    #[test]
    fn test_resolve_package_with_main() {
        let temp = TempDir::new().unwrap();
        setup_node_modules(&temp);

        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let resolved = loader
            .resolve_bare_specifier("with-main", &referrer)
            .unwrap();
        assert!(resolved.ends_with("with-main/lib.js"));
    }

    #[test]
    fn test_resolve_package_with_exports() {
        let temp = TempDir::new().unwrap();
        setup_node_modules(&temp);

        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let resolved = loader
            .resolve_bare_specifier("with-exports", &referrer)
            .unwrap();
        assert!(resolved.ends_with("with-exports/dist/index.mjs"));
    }

    #[test]
    fn test_resolve_subpath_import() {
        let temp = TempDir::new().unwrap();
        setup_node_modules(&temp);

        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let resolved = loader
            .resolve_bare_specifier("with-subpath/utils/helper", &referrer)
            .unwrap();
        assert!(resolved.ends_with("with-subpath/utils/helper.js"));
    }

    #[test]
    fn test_resolve_scoped_package() {
        let temp = TempDir::new().unwrap();
        setup_node_modules(&temp);

        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let resolved = loader
            .resolve_bare_specifier("@scope/pkg", &referrer)
            .unwrap();
        assert!(resolved.ends_with("@scope/pkg/index.js"));
    }

    #[test]
    fn test_package_not_found() {
        let temp = TempDir::new().unwrap();
        let loader = HowthModuleLoader::new(temp.path().to_path_buf());
        let referrer = ModuleSpecifier::from_file_path(temp.path().join("index.js")).unwrap();

        let result = loader.resolve_bare_specifier("nonexistent-pkg", &referrer);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot find package"));
    }
}
