//! Configuration file discovery and parsing for `howth dev`.
//!
//! Loads `howth.config.ts`, `howth.config.js`, `vite.config.ts`, or `vite.config.js`
//! and extracts static configuration (server options, resolve aliases, define replacements).
//!
//! ## Supported config format
//!
//! ```js
//! export default {
//!   server: { port: 3000, host: 'localhost', open: true },
//!   resolve: { alias: { '@': './src' } },
//!   define: { 'process.env.NODE_ENV': '"development"' },
//!   base: '/',
//! };
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Configuration loaded from a config file.
#[derive(Debug, Clone, Default)]
pub struct HowthConfig {
    /// Server options.
    pub server: ServerConfig,
    /// Resolve options (aliases).
    pub resolve: ResolveConfig,
    /// Define replacements.
    pub define: HashMap<String, String>,
    /// Base public path.
    pub base: Option<String>,
}

/// Server configuration from config file.
#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    /// Port to listen on.
    pub port: Option<u16>,
    /// Host to bind to.
    pub host: Option<String>,
    /// Open browser automatically.
    pub open: Option<bool>,
}

/// Resolve configuration from config file.
#[derive(Debug, Clone, Default)]
pub struct ResolveConfig {
    /// Import aliases (e.g., `@` → `./src`).
    pub alias: HashMap<String, String>,
}

/// Config file names in priority order.
const CONFIG_FILES: &[&str] = &[
    "howth.config.ts",
    "howth.config.js",
    "vite.config.ts",
    "vite.config.js",
];

/// Find a config file in the given root directory.
pub fn find_config_file(root: &Path) -> Option<PathBuf> {
    for name in CONFIG_FILES {
        let path = root.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Load configuration from a config file in the given root directory.
///
/// If `config_path` is `Some`, use that specific file. Otherwise, auto-discover.
pub fn load_config(root: &Path, config_path: Option<&Path>) -> Result<Option<(PathBuf, HowthConfig)>, String> {
    let path = match config_path {
        Some(p) => {
            let abs = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };
            if !abs.exists() {
                return Err(format!("Config file not found: {}", abs.display()));
            }
            abs
        }
        None => match find_config_file(root) {
            Some(p) => p,
            None => return Ok(None),
        },
    };

    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config file {}: {}", path.display(), e))?;

    // If TypeScript, transpile to JS first
    let js_source = if path.extension().and_then(|e| e.to_str()) == Some("ts") {
        transpile_ts_config(&source, &path)?
    } else {
        source
    };

    let config = parse_config_object(&js_source)?;
    Ok(Some((path, config)))
}

/// Transpile a TypeScript config file to JavaScript using SWC.
fn transpile_ts_config(source: &str, path: &Path) -> Result<String, String> {
    use crate::compiler::{CompilerBackend, ModuleKind, SourceMapKind, SwcBackend, TranspileSpec};

    let backend = SwcBackend::new();
    let input_name = path.display().to_string();
    let mut spec = TranspileSpec::new(&input_name, &input_name);
    spec.module = ModuleKind::ESM;
    spec.sourcemaps = SourceMapKind::None;

    let output = backend
        .transpile(&spec, source)
        .map_err(|e| format!("Failed to transpile config: {}", e))?;

    Ok(output.code)
}

/// Parse the default export object from a JS config file.
///
/// Extracts the object literal after `export default` and parses it as a
/// JSON5-like structure (unquoted keys, single quotes, trailing commas).
fn parse_config_object(source: &str) -> Result<HowthConfig, String> {
    // Find "export default { ... }" — handle optional semicolons and whitespace
    let obj_str = extract_default_export_object(source)
        .ok_or_else(|| "No `export default { ... }` found in config file".to_string())?;

    // Parse the object literal into a serde_json::Value using our JSON5-like parser
    let value = parse_js_object(&obj_str)?;

    // Convert to HowthConfig
    let mut config = HowthConfig::default();

    if let Some(obj) = value.as_object() {
        // server
        if let Some(server) = obj.get("server").and_then(|v| v.as_object()) {
            if let Some(port) = server.get("port").and_then(|v| v.as_u64()) {
                config.server.port = Some(port as u16);
            }
            if let Some(host) = server.get("host").and_then(|v| v.as_str()) {
                config.server.host = Some(host.to_string());
            }
            if let Some(open) = server.get("open").and_then(|v| v.as_bool()) {
                config.server.open = Some(open);
            }
        }

        // resolve
        if let Some(resolve) = obj.get("resolve").and_then(|v| v.as_object()) {
            if let Some(alias) = resolve.get("alias").and_then(|v| v.as_object()) {
                for (key, val) in alias {
                    if let Some(s) = val.as_str() {
                        config.resolve.alias.insert(key.clone(), s.to_string());
                    }
                }
            }
        }

        // define
        if let Some(define) = obj.get("define").and_then(|v| v.as_object()) {
            for (key, val) in define {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                config.define.insert(key.clone(), val_str);
            }
        }

        // base
        if let Some(base) = obj.get("base").and_then(|v| v.as_str()) {
            config.base = Some(base.to_string());
        }
    }

    Ok(config)
}

/// Extract the object literal body from `export default { ... }` or `export default { ... };`.
///
/// Returns the object including the outer braces.
fn extract_default_export_object(source: &str) -> Option<String> {
    // Strip single-line comments (// ...) and multi-line comments (/* ... */)
    // to avoid matching inside comments. Keep line structure for offset tracking.
    let stripped = strip_comments(source);

    let marker = "export default";
    let idx = stripped.find(marker)?;
    let after = stripped[idx + marker.len()..].trim_start();

    if !after.starts_with('{') {
        return None;
    }

    // Find matching closing brace, respecting nesting and strings
    let mut depth = 0;
    let mut in_string: Option<char> = None;
    let mut prev = '\0';
    let mut end = 0;

    for (i, ch) in after.char_indices() {
        if let Some(quote) = in_string {
            if ch == quote && prev != '\\' {
                in_string = None;
            }
        } else {
            match ch {
                '"' | '\'' | '`' => in_string = Some(ch),
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        prev = ch;
    }

    if depth != 0 {
        return None;
    }

    Some(after[..end].to_string())
}

/// Strip single-line (//) and multi-line (/* */) comments from JS source.
fn strip_comments(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_string: Option<char> = None;

    while i < len {
        if let Some(quote) = in_string {
            result.push(chars[i]);
            if chars[i] == quote && (i == 0 || chars[i - 1] != '\\') {
                in_string = None;
            }
            i += 1;
        } else if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            // Single-line comment: skip to end of line
            while i < len && chars[i] != '\n' {
                i += 1;
            }
        } else if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            // Multi-line comment: skip to */
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                // Preserve newlines for line structure
                if chars[i] == '\n' {
                    result.push('\n');
                }
                i += 1;
            }
            i += 2; // skip */
        } else {
            if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
                in_string = Some(chars[i]);
            }
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Parse a JavaScript object literal into a serde_json::Value.
///
/// Handles: unquoted keys, single-quoted strings, trailing commas,
/// nested objects, arrays, numbers, booleans, null.
fn parse_js_object(input: &str) -> Result<serde_json::Value, String> {
    let mut parser = JsObjectParser::new(input);
    parser.parse_value()
}

struct JsObjectParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsObjectParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<serde_json::Value, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') | Some('\'') => self.parse_string(),
            Some(ch) if ch == '-' || ch.is_ascii_digit() => self.parse_number(),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(ch) => Err(format!("Unexpected character '{}' at position {}", ch, self.pos)),
            None => Err("Unexpected end of input".to_string()),
        }
    }

    fn parse_object(&mut self) -> Result<serde_json::Value, String> {
        self.advance(); // skip '{'
        let mut map = serde_json::Map::new();

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('}') => {
                    self.advance();
                    return Ok(serde_json::Value::Object(map));
                }
                None => return Err("Unterminated object".to_string()),
                _ => {}
            }

            // Parse key: quoted string or bare identifier
            let key = self.parse_key()?;
            self.skip_whitespace();

            // Expect ':'
            match self.advance() {
                Some(':') => {}
                other => return Err(format!("Expected ':' after key, got {:?}", other)),
            }

            // Parse value
            let value = self.parse_value()?;
            map.insert(key, value);

            // Expect ',' or '}'
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some('}') => {} // will be handled at top of loop
                None => return Err("Unterminated object".to_string()),
                Some(ch) => return Err(format!("Expected ',' or '}}' in object, got '{}'", ch)),
            }
        }
    }

    fn parse_array(&mut self) -> Result<serde_json::Value, String> {
        self.advance(); // skip '['
        let mut arr = Vec::new();

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(']') => {
                    self.advance();
                    return Ok(serde_json::Value::Array(arr));
                }
                None => return Err("Unterminated array".to_string()),
                _ => {}
            }

            let value = self.parse_value()?;
            arr.push(value);

            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(']') => {} // will be handled at top of loop
                None => return Err("Unterminated array".to_string()),
                Some(ch) => return Err(format!("Expected ',' or ']' in array, got '{}'", ch)),
            }
        }
    }

    fn parse_key(&mut self) -> Result<String, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('"') | Some('\'') => {
                if let serde_json::Value::String(s) = self.parse_string()? {
                    Ok(s)
                } else {
                    Err("Expected string key".to_string())
                }
            }
            Some(ch) if ch.is_alphabetic() || ch == '_' || ch == '$' || ch == '.' => {
                // Bare identifier key (may contain dots for keys like 'process.env.NODE_ENV')
                let mut key = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
                        key.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
                Ok(key)
            }
            other => Err(format!("Expected object key, got {:?}", other)),
        }
    }

    fn parse_string(&mut self) -> Result<serde_json::Value, String> {
        let quote = self.advance().unwrap(); // '"' or '\''
        let mut s = String::new();

        loop {
            match self.advance() {
                Some(ch) if ch == quote => {
                    return Ok(serde_json::Value::String(s));
                }
                Some('\\') => {
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some(ch) if ch == quote => s.push(ch),
                        Some(ch) => {
                            s.push('\\');
                            s.push(ch);
                        }
                        None => return Err("Unterminated string escape".to_string()),
                    }
                }
                Some(ch) => s.push(ch),
                None => return Err("Unterminated string".to_string()),
            }
        }
    }

    fn parse_number(&mut self) -> Result<serde_json::Value, String> {
        let mut num_str = String::new();
        let mut has_dot = false;

        if self.peek() == Some('-') {
            num_str.push('-');
            self.advance();
        }

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if has_dot {
            num_str
                .parse::<f64>()
                .map(|n| serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap()))
                .map_err(|e| format!("Invalid number '{}': {}", num_str, e))
        } else {
            num_str
                .parse::<i64>()
                .map(|n| serde_json::Value::Number(n.into()))
                .map_err(|e| format!("Invalid number '{}': {}", num_str, e))
        }
    }

    fn parse_bool(&mut self) -> Result<serde_json::Value, String> {
        if self.starts_with("true") {
            self.pos += 4;
            Ok(serde_json::Value::Bool(true))
        } else if self.starts_with("false") {
            self.pos += 5;
            Ok(serde_json::Value::Bool(false))
        } else {
            Err(format!("Unexpected token at position {}", self.pos))
        }
    }

    fn parse_null(&mut self) -> Result<serde_json::Value, String> {
        if self.starts_with("null") {
            self.pos += 4;
            Ok(serde_json::Value::Null)
        } else {
            Err(format!("Unexpected token at position {}", self.pos))
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        let remaining: String = self.chars[self.pos..].iter().collect();
        remaining.starts_with(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_config_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_config_file(dir.path()).is_none());

        std::fs::write(dir.path().join("vite.config.js"), "export default {}").unwrap();
        assert_eq!(
            find_config_file(dir.path()).unwrap(),
            dir.path().join("vite.config.js")
        );

        // howth.config.ts takes priority
        std::fs::write(dir.path().join("howth.config.ts"), "export default {}").unwrap();
        assert_eq!(
            find_config_file(dir.path()).unwrap(),
            dir.path().join("howth.config.ts")
        );
    }

    #[test]
    fn test_parse_simple_config() {
        let source = r#"
            export default {
                server: {
                    port: 4000,
                    host: 'localhost',
                    open: true,
                },
                resolve: {
                    alias: {
                        '@': './src',
                        '~': './src',
                    },
                },
                define: {
                    '__APP_VERSION__': '"1.0.0"',
                },
                base: '/app/',
            };
        "#;

        let config = parse_config_object(source).unwrap();
        assert_eq!(config.server.port, Some(4000));
        assert_eq!(config.server.host.as_deref(), Some("localhost"));
        assert_eq!(config.server.open, Some(true));
        assert_eq!(config.resolve.alias.get("@").map(|s| s.as_str()), Some("./src"));
        assert_eq!(config.resolve.alias.get("~").map(|s| s.as_str()), Some("./src"));
        assert_eq!(config.define.get("__APP_VERSION__").map(|s| s.as_str()), Some("\"1.0.0\""));
        assert_eq!(config.base.as_deref(), Some("/app/"));
    }

    #[test]
    fn test_parse_config_with_comments() {
        let source = r#"
            // This is a config file
            /* Multi-line
               comment */
            export default {
                server: {
                    port: 3000, // inline comment
                },
            };
        "#;

        let config = parse_config_object(source).unwrap();
        assert_eq!(config.server.port, Some(3000));
    }

    #[test]
    fn test_parse_config_double_quotes() {
        let source = r#"
            export default {
                resolve: {
                    alias: {
                        "@": "./src",
                    },
                },
            };
        "#;

        let config = parse_config_object(source).unwrap();
        assert_eq!(config.resolve.alias.get("@").map(|s| s.as_str()), Some("./src"));
    }

    #[test]
    fn test_parse_empty_config() {
        let source = "export default {};";
        let config = parse_config_object(source).unwrap();
        assert_eq!(config.server.port, None);
        assert!(config.resolve.alias.is_empty());
        assert!(config.define.is_empty());
        assert_eq!(config.base, None);
    }

    #[test]
    fn test_parse_config_with_array() {
        let source = r#"
            export default {
                server: {
                    port: 5173,
                },
            };
        "#;

        let config = parse_config_object(source).unwrap();
        assert_eq!(config.server.port, Some(5173));
    }

    #[test]
    fn test_parse_define_with_dotted_keys() {
        let source = r#"
            export default {
                define: {
                    'process.env.NODE_ENV': '"development"',
                    '__DEV__': 'true',
                },
            };
        "#;

        let config = parse_config_object(source).unwrap();
        assert_eq!(
            config.define.get("process.env.NODE_ENV").map(|s| s.as_str()),
            Some("\"development\"")
        );
        assert_eq!(config.define.get("__DEV__").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_no_default_export() {
        let source = "const config = {};";
        assert!(parse_config_object(source).is_err());
    }

    #[test]
    fn test_load_config_js_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = r#"
            export default {
                server: { port: 8080 },
                base: '/myapp/',
            };
        "#;
        std::fs::write(dir.path().join("howth.config.js"), config_content).unwrap();

        let result = load_config(dir.path(), None).unwrap();
        assert!(result.is_some());
        let (path, config) = result.unwrap();
        assert_eq!(path, dir.path().join("howth.config.js"));
        assert_eq!(config.server.port, Some(8080));
        assert_eq!(config.base.as_deref(), Some("/myapp/"));
    }

    #[test]
    fn test_load_config_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let config_content = "export default { server: { port: 9999 } };";
        std::fs::write(dir.path().join("custom.config.js"), config_content).unwrap();

        let custom_path = dir.path().join("custom.config.js");
        let result = load_config(dir.path(), Some(&custom_path)).unwrap();
        assert!(result.is_some());
        let (_, config) = result.unwrap();
        assert_eq!(config.server.port, Some(9999));
    }

    #[test]
    fn test_load_config_missing_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let custom_path = dir.path().join("nonexistent.config.js");
        let result = load_config(dir.path(), Some(&custom_path));
        assert!(result.is_err());
    }

    #[test]
    fn test_strip_comments() {
        let input = r#"
            // line comment
            hello /* block
            comment */ world
        "#;
        let result = strip_comments(input);
        assert!(!result.contains("line comment"));
        assert!(!result.contains("block"));
        assert!(result.contains("hello"));
        assert!(result.contains("world"));
    }
}
