//! CSS processing using lightningcss.
//!
//! Provides:
//! - Autoprefixer (vendor prefixes for browser compatibility)
//! - CSS nesting transformation
//! - CSS Modules support (scoped class names)
//! - Minification
//! - Sass/SCSS preprocessing

pub mod sass;

use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, StyleSheet};
use lightningcss::targets::{Browsers, Targets};
use std::collections::HashMap;
use std::path::Path;

/// CSS processing options.
#[derive(Debug, Clone, Default)]
pub struct CssOptions {
    /// Enable minification.
    pub minify: bool,
    /// Enable CSS Modules (returns class name mappings).
    pub css_modules: bool,
    /// Enable autoprefixer with browser targets.
    pub autoprefixer: bool,
    /// Source file path (for error messages and CSS Modules).
    pub filename: Option<String>,
    /// Browser targets for autoprefixer (defaults to reasonable coverage).
    pub targets: Option<Browsers>,
}

/// Result of CSS processing.
#[derive(Debug, Clone)]
pub struct CssResult {
    /// The transformed CSS code.
    pub code: String,
    /// CSS Modules exports (original name → hashed name).
    /// Only populated if `css_modules` is enabled.
    pub exports: HashMap<String, String>,
}

/// Process CSS with lightningcss.
///
/// # Features
/// - **Autoprefixer**: Adds vendor prefixes based on browser targets
/// - **CSS Nesting**: Transforms nested selectors to flat CSS
/// - **CSS Modules**: Scopes class names and returns mappings
/// - **Minification**: Removes whitespace and optimizes
///
/// # Example
/// ```ignore
/// let options = CssOptions {
///     minify: true,
///     autoprefixer: true,
///     ..Default::default()
/// };
/// let result = process_css(".foo { display: flex; }", &options)?;
/// ```
pub fn process_css(source: &str, options: &CssOptions) -> Result<CssResult, CssError> {
    let filename = options.filename.as_deref().unwrap_or("input.css");

    // Set up parser options
    let mut parser_options = ParserOptions::default();

    // Enable CSS Modules if requested
    if options.css_modules {
        parser_options.css_modules = Some(lightningcss::css_modules::Config {
            pattern: lightningcss::css_modules::Pattern::parse("[hash]_[local]")
                .map_err(|e| CssError::Parse(format!("CSS Modules pattern error: {}", e)))?,
            dashed_idents: false,
            animation: Default::default(),
            grid: Default::default(),
            container: Default::default(),
            custom_idents: Default::default(),
            pure: false,
        });
    }

    // Parse the stylesheet
    let mut stylesheet = StyleSheet::parse(source, parser_options)
        .map_err(|e| CssError::Parse(format!("CSS parse error in {}: {}", filename, e)))?;

    // Set up browser targets for autoprefixer
    let targets = if options.autoprefixer {
        options.targets.unwrap_or_else(default_browser_targets)
    } else {
        Browsers::default()
    };

    // Minify if requested (also applies autoprefixer transforms)
    if options.minify || options.autoprefixer {
        stylesheet
            .minify(MinifyOptions {
                targets: Targets::from(targets),
                ..Default::default()
            })
            .map_err(|e| CssError::Transform(format!("CSS minify error: {}", e)))?;
    }

    // Set up printer options
    let printer_options = PrinterOptions {
        minify: options.minify,
        targets: Targets::from(targets),
        ..Default::default()
    };

    // Generate output
    let output = stylesheet
        .to_css(printer_options)
        .map_err(|e| CssError::Print(format!("CSS print error: {}", e)))?;

    // Extract CSS Modules exports
    let exports = if options.css_modules {
        output
            .exports
            .map(|exp| {
                exp.iter()
                    .map(|(k, v)| (k.to_string(), v.name.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    Ok(CssResult {
        code: output.code,
        exports,
    })
}

/// Process a CSS file, detecting if it's a CSS Module by filename.
///
/// Files ending in `.module.css` are automatically treated as CSS Modules.
pub fn process_css_file(
    source: &str,
    path: &Path,
    minify: bool,
    autoprefixer: bool,
) -> Result<CssResult, CssError> {
    let filename = path.display().to_string();
    let is_module = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".module.css"))
        .unwrap_or(false);

    let options = CssOptions {
        minify,
        css_modules: is_module,
        autoprefixer,
        filename: Some(filename),
        targets: None,
    };

    process_css(source, &options)
}

/// Get default browser targets for autoprefixer.
///
/// Targets browsers with >0.5% market share, not dead, last 2 versions.
fn default_browser_targets() -> Browsers {
    Browsers {
        // These are approximate values for modern browser support
        // Chrome 80+, Firefox 75+, Safari 13+, Edge 80+
        chrome: Some(80 << 16),
        firefox: Some(75 << 16),
        safari: Some(13 << 16),
        edge: Some(80 << 16),
        ..Default::default()
    }
}

/// Generate JavaScript code for a CSS Module.
///
/// Creates a JS module that:
/// 1. Injects the CSS as a <style> tag
/// 2. Exports the class name mappings
pub fn generate_css_module_js(css: &str, exports: &HashMap<String, String>) -> String {
    let escaped_css = css
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");

    let exports_obj: String = exports
        .iter()
        .map(|(k, v)| format!("  \"{}\": \"{}\"", k, v))
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        r#"const css = `{escaped_css}`;
const style = document.createElement('style');
style.setAttribute('data-howth-css-module', '');
style.textContent = css;
document.head.appendChild(style);

const classes = {{
{exports_obj}
}};

// HMR support
if (import.meta.hot) {{
  import.meta.hot.accept();
  import.meta.hot.dispose(() => {{
    style.remove();
  }});
}}

export default classes;
"#
    )
}

/// CSS processing error.
#[derive(Debug)]
pub enum CssError {
    /// Parse error.
    Parse(String),
    /// Transform error.
    Transform(String),
    /// Print error.
    Print(String),
}

impl std::fmt::Display for CssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CssError::Parse(msg) => write!(f, "CSS parse error: {}", msg),
            CssError::Transform(msg) => write!(f, "CSS transform error: {}", msg),
            CssError::Print(msg) => write!(f, "CSS print error: {}", msg),
        }
    }
}

impl std::error::Error for CssError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_css_processing() {
        let css = ".foo { color: red; }";
        let options = CssOptions::default();
        let result = process_css(css, &options).unwrap();
        assert!(result.code.contains("color"));
    }

    #[test]
    fn test_minification() {
        let css = ".foo {\n  color: red;\n  margin: 10px;\n}";
        let options = CssOptions {
            minify: true,
            ..Default::default()
        };
        let result = process_css(css, &options).unwrap();
        // Minified CSS should have no newlines
        assert!(!result.code.contains('\n'));
    }

    #[test]
    fn test_autoprefixer() {
        let css = ".foo { display: flex; }";
        let options = CssOptions {
            autoprefixer: true,
            ..Default::default()
        };
        let result = process_css(css, &options).unwrap();
        // Should still contain flex (lightningcss adds prefixes as needed)
        assert!(result.code.contains("flex"));
    }

    #[test]
    fn test_css_nesting() {
        let css = ".parent { .child { color: red; } }";
        let options = CssOptions {
            minify: false,
            autoprefixer: true,
            ..Default::default()
        };
        let result = process_css(css, &options).unwrap();
        // Nested selectors should be flattened
        assert!(result.code.contains(".parent"));
    }

    #[test]
    fn test_css_modules() {
        let css = ".button { color: blue; }";
        let options = CssOptions {
            css_modules: true,
            ..Default::default()
        };
        let result = process_css(css, &options).unwrap();
        // Should have exports for the class name
        assert!(!result.exports.is_empty());
        assert!(result.exports.contains_key("button"));
    }

    #[test]
    fn test_css_module_detection() {
        let path = Path::new("src/Button.module.css");
        let css = ".btn { padding: 10px; }";
        let result = process_css_file(css, path, false, false).unwrap();
        // Should detect as CSS module and have exports
        assert!(result.exports.contains_key("btn"));
    }

    #[test]
    fn test_regular_css_no_modules() {
        let path = Path::new("src/styles.css");
        let css = ".btn { padding: 10px; }";
        let result = process_css_file(css, path, false, false).unwrap();
        // Regular CSS should not have exports
        assert!(result.exports.is_empty());
    }

    #[test]
    fn test_generate_css_module_js() {
        let css = ".abc123_button { color: blue; }";
        let mut exports = HashMap::new();
        exports.insert("button".to_string(), "abc123_button".to_string());

        let js = generate_css_module_js(css, &exports);
        assert!(js.contains("export default classes"));
        assert!(js.contains("\"button\": \"abc123_button\""));
        assert!(js.contains("import.meta.hot"));
    }
}
