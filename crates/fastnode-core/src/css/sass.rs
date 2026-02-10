//! Sass/SCSS preprocessing using grass.
//!
//! Compiles .scss and .sass files to CSS.

use std::path::Path;

/// Sass compilation options.
#[derive(Debug, Clone, Default)]
pub struct SassOptions {
    /// Include paths for @import/@use resolution.
    pub include_paths: Vec<String>,
    /// Output style (expanded or compressed).
    pub minify: bool,
    /// Source file path (for error messages and imports).
    pub filename: Option<String>,
}

/// Compile Sass/SCSS to CSS.
///
/// Supports both `.scss` (Sassy CSS) and `.sass` (indented syntax).
pub fn compile_sass(source: &str, options: &SassOptions) -> Result<String, SassError> {
    let filename = options.filename.as_deref().unwrap_or("input.scss");
    let is_indented = std::path::Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sass"));

    // Set up grass options
    let mut grass_options = grass::Options::default();

    // Set output style
    if options.minify {
        grass_options = grass_options.style(grass::OutputStyle::Compressed);
    } else {
        grass_options = grass_options.style(grass::OutputStyle::Expanded);
    }

    // Add include paths for @import/@use resolution
    for path in &options.include_paths {
        grass_options = grass_options.load_path(path);
    }

    // If we have a filename, add its directory as an include path
    if let Some(ref fname) = options.filename {
        if let Some(parent) = Path::new(fname).parent() {
            grass_options = grass_options.load_path(parent);
        }
    }

    // Compile
    let result = if is_indented {
        // .sass files use indented syntax
        grass::from_string(source.to_string(), &grass_options)
    } else {
        // .scss files use CSS-like syntax
        grass::from_string(source.to_string(), &grass_options)
    };

    result.map_err(|e| SassError::Compile(format!("{e}")))
}

/// Compile a Sass/SCSS file to CSS.
pub fn compile_sass_file(path: &Path, minify: bool) -> Result<String, SassError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| SassError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

    let options = SassOptions {
        include_paths: vec![],
        minify,
        filename: Some(path.display().to_string()),
    };

    compile_sass(&source, &options)
}

/// Check if a file is a Sass/SCSS file.
#[must_use]
pub fn is_sass_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "scss" || e == "sass")
}

/// Sass compilation error.
#[derive(Debug)]
pub enum SassError {
    /// IO error (file not found, etc.).
    Io(String),
    /// Compilation error.
    Compile(String),
}

impl std::fmt::Display for SassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SassError::Io(msg) => write!(f, "Sass IO error: {msg}"),
            SassError::Compile(msg) => write!(f, "Sass compile error: {msg}"),
        }
    }
}

impl std::error::Error for SassError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scss() {
        let scss = r"
            $primary: blue;
            .button {
                color: $primary;
            }
        ";
        let options = SassOptions::default();
        let result = compile_sass(scss, &options).unwrap();
        assert!(result.contains("color: blue"));
    }

    #[test]
    fn test_scss_nesting() {
        let scss = r"
            .parent {
                .child {
                    color: red;
                }
            }
        ";
        let options = SassOptions::default();
        let result = compile_sass(scss, &options).unwrap();
        assert!(result.contains(".parent .child"));
    }

    #[test]
    fn test_scss_mixins() {
        let scss = r"
            @mixin flex-center {
                display: flex;
                align-items: center;
                justify-content: center;
            }
            .container {
                @include flex-center;
            }
        ";
        let options = SassOptions::default();
        let result = compile_sass(scss, &options).unwrap();
        assert!(result.contains("display: flex"));
        assert!(result.contains("align-items: center"));
    }

    #[test]
    fn test_scss_minification() {
        let scss = ".foo { color: red; }";
        let options = SassOptions {
            minify: true,
            ..Default::default()
        };
        let result = compile_sass(scss, &options).unwrap();
        // Compressed output should have no newlines in the middle
        assert!(!result.trim().contains("\n\n"));
    }

    #[test]
    fn test_scss_variables() {
        let scss = r"
            $font-size: 16px;
            $line-height: 1.5;
            body {
                font-size: $font-size;
                line-height: $line-height;
            }
        ";
        let options = SassOptions::default();
        let result = compile_sass(scss, &options).unwrap();
        assert!(result.contains("font-size: 16px"));
        assert!(result.contains("line-height: 1.5"));
    }

    #[test]
    fn test_scss_functions() {
        let scss = r"
            .box {
                width: 100px + 50px;
                opacity: 0.5 * 2;
            }
        ";
        let options = SassOptions::default();
        let result = compile_sass(scss, &options).unwrap();
        assert!(result.contains("width: 150px"));
        assert!(result.contains("opacity: 1"));
    }

    #[test]
    fn test_is_sass_file() {
        assert!(is_sass_file(Path::new("styles.scss")));
        assert!(is_sass_file(Path::new("theme.sass")));
        assert!(!is_sass_file(Path::new("styles.css")));
        assert!(!is_sass_file(Path::new("app.js")));
    }
}
