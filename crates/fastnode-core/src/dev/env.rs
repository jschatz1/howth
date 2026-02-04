//! `.env` file loading for the dev server.
//!
//! Vite-compatible: loads `.env`, `.env.local`, `.env.[mode]`, `.env.[mode].local`
//! in order, with later files overriding earlier ones. System environment variables
//! already set take precedence (are not overwritten).

use std::collections::HashMap;
use std::path::Path;

/// Parse a `.env` file's contents into key-value pairs.
///
/// Supports:
/// - `KEY=value` (unquoted)
/// - `KEY="value"` (double-quoted, with escape sequences)
/// - `KEY='value'` (single-quoted, literal)
/// - Comments (`#`) and blank lines are skipped
/// - Inline comments after unquoted values
#[must_use] 
pub fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut env = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split on first '='
        let Some(eq_pos) = line.find('=') else {
            continue;
        };

        let key = line[..eq_pos].trim();
        if key.is_empty() {
            continue;
        }

        // Skip `export ` prefix (common in .env files)
        let key = key.strip_prefix("export ").unwrap_or(key).trim();

        let raw_value = line[eq_pos + 1..].trim();

        let value = if raw_value.starts_with('"') {
            // Double-quoted: parse escape sequences
            parse_double_quoted(raw_value)
        } else if raw_value.starts_with('\'') {
            // Single-quoted: literal value
            parse_single_quoted(raw_value)
        } else {
            // Unquoted: trim inline comments
            parse_unquoted(raw_value)
        };

        env.insert(key.to_string(), value);
    }

    env
}

fn parse_double_quoted(raw: &str) -> String {
    // Strip leading quote
    let inner = &raw[1..];

    let mut result = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        other => {
                            result.push('\\');
                            result.push(other);
                        }
                    }
                }
            }
            _ => result.push(c),
        }
    }

    result
}

fn parse_single_quoted(raw: &str) -> String {
    // Strip leading quote, find closing quote
    let inner = &raw[1..];
    if let Some(end) = inner.find('\'') {
        inner[..end].to_string()
    } else {
        // No closing quote — take the rest
        inner.to_string()
    }
}

fn parse_unquoted(raw: &str) -> String {
    // Strip inline comments (` #` with preceding space)
    if let Some(comment_pos) = raw.find(" #") {
        raw[..comment_pos].trim_end().to_string()
    } else {
        raw.to_string()
    }
}

/// Load `.env` files from the project root for the given mode.
///
/// Files are loaded in Vite-compatible order:
/// 1. `.env`
/// 2. `.env.local`
/// 3. `.env.[mode]`
/// 4. `.env.[mode].local`
///
/// Later files override earlier ones. System environment variables already set
/// take precedence and are not overwritten.
#[must_use] 
pub fn load_env_files(root: &Path, mode: &str) -> HashMap<String, String> {
    let files = [
        root.join(".env"),
        root.join(".env.local"),
        root.join(format!(".env.{mode}")),
        root.join(format!(".env.{mode}.local")),
    ];

    let mut env = HashMap::new();

    for file in &files {
        if let Ok(content) = std::fs::read_to_string(file) {
            let parsed = parse_env_file(&content);
            for (key, value) in parsed {
                env.insert(key, value);
            }
        }
    }

    // System env vars take precedence — remove any key already set in the process env
    env.retain(|key, _| std::env::var(key).is_err());

    env
}

/// Filter environment variables to those exposed to client code and return
/// `import.meta.env.*` replacement mappings.
///
/// Only variables prefixed with `VITE_` or `HOWTH_` are exposed.
/// Also includes built-in replacements:
/// - `import.meta.env.MODE` → `"development"` (or current mode)
/// - `import.meta.env.DEV` → `true` / `false`
/// - `import.meta.env.PROD` → `true` / `false`
/// - `import.meta.env.BASE_URL` → `"/"`
#[must_use] 
pub fn client_env_replacements(
    env: &HashMap<String, String>,
    mode: &str,
) -> HashMap<String, String> {
    let mut replacements = HashMap::new();

    // Built-in replacements
    let is_dev = mode == "development";
    replacements.insert("import.meta.env.MODE".to_string(), format!("\"{mode}\""));
    replacements.insert("import.meta.env.DEV".to_string(), is_dev.to_string());
    replacements.insert("import.meta.env.PROD".to_string(), (!is_dev).to_string());
    replacements.insert("import.meta.env.BASE_URL".to_string(), "\"/\"".to_string());

    // User-defined env vars with allowed prefixes
    for (key, value) in env {
        if key.starts_with("VITE_") || key.starts_with("HOWTH_") {
            replacements.insert(
                format!("import.meta.env.{key}"),
                format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")),
            );
        }
    }

    replacements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_key_value() {
        let content = "KEY=value\nOTHER=123";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "value");
        assert_eq!(env.get("OTHER").unwrap(), "123");
    }

    #[test]
    fn test_parse_double_quoted() {
        let content = r#"KEY="hello world""#;
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "hello world");
    }

    #[test]
    fn test_parse_double_quoted_escapes() {
        let content = r#"KEY="line1\nline2\ttab\\backslash""#;
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "line1\nline2\ttab\\backslash");
    }

    #[test]
    fn test_parse_single_quoted() {
        let content = "KEY='hello world'";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "hello world");
    }

    #[test]
    fn test_parse_single_quoted_no_escapes() {
        let content = r"KEY='hello\nworld'";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), r"hello\nworld");
    }

    #[test]
    fn test_skip_comments_and_blanks() {
        let content = "# comment\n\nKEY=value\n  # another comment\n";
        let env = parse_env_file(content);
        assert_eq!(env.len(), 1);
        assert_eq!(env.get("KEY").unwrap(), "value");
    }

    #[test]
    fn test_inline_comment() {
        let content = "KEY=value # this is a comment";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "value");
    }

    #[test]
    fn test_export_prefix() {
        let content = "export KEY=value";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "value");
    }

    #[test]
    fn test_empty_value() {
        let content = "KEY=";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "");
    }

    #[test]
    fn test_value_with_equals() {
        let content = "KEY=a=b=c";
        let env = parse_env_file(content);
        assert_eq!(env.get("KEY").unwrap(), "a=b=c");
    }

    #[test]
    fn test_client_env_replacements_builtins() {
        let env = HashMap::new();
        let replacements = client_env_replacements(&env, "development");

        assert_eq!(
            replacements.get("import.meta.env.MODE").unwrap(),
            "\"development\""
        );
        assert_eq!(replacements.get("import.meta.env.DEV").unwrap(), "true");
        assert_eq!(replacements.get("import.meta.env.PROD").unwrap(), "false");
        assert_eq!(
            replacements.get("import.meta.env.BASE_URL").unwrap(),
            "\"/\""
        );
    }

    #[test]
    fn test_client_env_replacements_production() {
        let env = HashMap::new();
        let replacements = client_env_replacements(&env, "production");

        assert_eq!(
            replacements.get("import.meta.env.MODE").unwrap(),
            "\"production\""
        );
        assert_eq!(replacements.get("import.meta.env.DEV").unwrap(), "false");
        assert_eq!(replacements.get("import.meta.env.PROD").unwrap(), "true");
    }

    #[test]
    fn test_client_env_replacements_filters_prefixes() {
        let mut env = HashMap::new();
        env.insert(
            "VITE_API_URL".to_string(),
            "http://localhost:8080".to_string(),
        );
        env.insert("HOWTH_SECRET".to_string(), "abc123".to_string());
        env.insert("DATABASE_URL".to_string(), "postgres://...".to_string());
        env.insert("SECRET_KEY".to_string(), "should_not_appear".to_string());

        let replacements = client_env_replacements(&env, "development");

        assert_eq!(
            replacements.get("import.meta.env.VITE_API_URL").unwrap(),
            "\"http://localhost:8080\""
        );
        assert_eq!(
            replacements.get("import.meta.env.HOWTH_SECRET").unwrap(),
            "\"abc123\""
        );
        assert!(replacements.get("import.meta.env.DATABASE_URL").is_none());
        assert!(replacements.get("import.meta.env.SECRET_KEY").is_none());
    }

    #[test]
    fn test_client_env_escapes_special_chars() {
        let mut env = HashMap::new();
        env.insert("VITE_MSG".to_string(), r#"say "hello""#.to_string());

        let replacements = client_env_replacements(&env, "development");
        assert_eq!(
            replacements.get("import.meta.env.VITE_MSG").unwrap(),
            r#""say \"hello\"""#
        );
    }

    #[test]
    fn test_load_env_files_merges_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // .env has base values
        std::fs::write(root.join(".env"), "VITE_A=from_env\nVITE_B=from_env").unwrap();
        // .env.development overrides VITE_A
        std::fs::write(root.join(".env.development"), "VITE_A=from_dev").unwrap();

        let env = load_env_files(root, "development");

        assert_eq!(env.get("VITE_A").unwrap(), "from_dev");
        assert_eq!(env.get("VITE_B").unwrap(), "from_env");
    }

    #[test]
    fn test_load_env_files_local_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(root.join(".env"), "VITE_X=base").unwrap();
        std::fs::write(root.join(".env.local"), "VITE_X=local").unwrap();
        std::fs::write(root.join(".env.development"), "VITE_X=dev").unwrap();
        std::fs::write(root.join(".env.development.local"), "VITE_X=dev_local").unwrap();

        let env = load_env_files(root, "development");
        assert_eq!(env.get("VITE_X").unwrap(), "dev_local");
    }

    #[test]
    fn test_load_env_files_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // No .env files at all — should return empty without error
        let env = load_env_files(root, "development");
        assert!(env.is_empty());
    }
}
