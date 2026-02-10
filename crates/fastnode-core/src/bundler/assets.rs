//! Asset and CSS handling for the bundler.
//!
//! Handles importing CSS files and static assets (images, fonts, etc.).

#![allow(dead_code)]

use rustc_hash::FxHashMap as HashMap;
use std::path::{Path, PathBuf};

/// Asset types that can be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    /// CSS stylesheets.
    Css,
    /// Image files (png, jpg, gif, svg, webp, ico).
    Image,
    /// Font files (woff, woff2, ttf, otf, eot).
    Font,
    /// Other static assets.
    Other,
}

impl AssetType {
    /// Determine asset type from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "css" | "scss" | "sass" => Some(AssetType::Css),
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "avif" => {
                Some(AssetType::Image)
            }
            "woff" | "woff2" | "ttf" | "otf" | "eot" => Some(AssetType::Font),
            "json" | "txt" | "xml" | "wasm" => Some(AssetType::Other),
            _ => None,
        }
    }

    /// Check if a file extension is a known asset type.
    pub fn is_asset(ext: &str) -> bool {
        Self::from_extension(ext).is_some()
    }

    /// Check if this is a CSS or Sass file.
    pub fn is_css(ext: &str) -> bool {
        matches!(ext.to_lowercase().as_str(), "css" | "scss" | "sass")
    }

    /// Check if this is a Sass/SCSS file.
    pub fn is_sass(ext: &str) -> bool {
        matches!(ext.to_lowercase().as_str(), "scss" | "sass")
    }
}

/// An imported asset.
#[derive(Debug, Clone)]
pub struct Asset {
    /// Original source path.
    pub source_path: PathBuf,
    /// Asset type.
    pub asset_type: AssetType,
    /// Content hash (for cache busting).
    pub hash: String,
    /// Output filename (with hash).
    pub output_name: String,
    /// Raw content (for CSS).
    pub content: Option<String>,
}

/// Collected assets from bundling.
#[derive(Debug, Default)]
pub struct AssetCollection {
    /// All collected assets, keyed by source path.
    assets: HashMap<String, Asset>,
    /// CSS content in import order.
    css_chunks: Vec<String>,
}

impl AssetCollection {
    /// Create a new empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a CSS file.
    pub fn add_css(&mut self, path: &Path, content: String) -> String {
        let hash = hash_content(&content);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("style");
        let output_name = format!("{}.{}.css", stem, &hash[..8]);

        let path_str = path.display().to_string();

        self.assets.insert(
            path_str.clone(),
            Asset {
                source_path: path.to_path_buf(),
                asset_type: AssetType::Css,
                hash: hash.clone(),
                output_name: output_name.clone(),
                content: Some(content.clone()),
            },
        );

        self.css_chunks.push(content);

        output_name
    }

    /// Add a static asset (image, font, etc.).
    pub fn add_asset(&mut self, path: &Path, content: &[u8]) -> String {
        let hash = hash_bytes(content);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("asset");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("bin");
        let output_name = format!("{}.{}.{}", stem, &hash[..8], ext);

        let path_str = path.display().to_string();
        let asset_type = AssetType::from_extension(ext).unwrap_or(AssetType::Other);

        self.assets.insert(
            path_str,
            Asset {
                source_path: path.to_path_buf(),
                asset_type,
                hash,
                output_name: output_name.clone(),
                content: None,
            },
        );

        output_name
    }

    /// Get the output URL for an asset.
    pub fn get_output_url(&self, path: &str) -> Option<String> {
        self.assets
            .get(path)
            .map(|a| format!("./{}", a.output_name))
    }

    /// Get all CSS concatenated.
    pub fn get_bundled_css(&self) -> String {
        self.css_chunks.join("\n\n")
    }

    /// Get all assets for copying.
    pub fn get_assets(&self) -> impl Iterator<Item = &Asset> {
        self.assets
            .values()
            .filter(|a| a.asset_type != AssetType::Css)
    }

    /// Check if there's any CSS.
    pub fn has_css(&self) -> bool {
        !self.css_chunks.is_empty()
    }

    /// Get CSS output filename (if any CSS was collected).
    pub fn get_css_output_name(&self) -> Option<String> {
        if self.css_chunks.is_empty() {
            return None;
        }

        let combined = self.get_bundled_css();
        let hash = hash_content(&combined);
        Some(format!("styles.{}.css", &hash[..8]))
    }
}

/// Hash string content using blake3.
fn hash_content(content: &str) -> String {
    hash_bytes(content.as_bytes())
}

/// Hash bytes using blake3.
fn hash_bytes(bytes: &[u8]) -> String {
    let hash = blake3::hash(bytes);
    hash.to_hex().to_string()
}

/// Process a CSS file using lightningcss.
///
/// Includes autoprefixer, CSS nesting transformation, and minification.
pub fn process_css(content: &str) -> String {
    use crate::css::{process_css as lightning_process, CssOptions};

    let options = CssOptions {
        minify: true,
        autoprefixer: true,
        css_modules: false,
        filename: None,
        targets: None,
    };

    match lightning_process(content, &options) {
        Ok(result) => result.code,
        Err(e) => {
            // Fall back to basic processing on error
            eprintln!("CSS processing warning: {}", e);
            basic_css_minify(content)
        }
    }
}

/// Process a CSS Module file, returning the processed CSS and exports.
pub fn process_css_module(
    content: &str,
    path: &Path,
) -> Result<(String, std::collections::HashMap<String, String>), String> {
    use crate::css::{process_css_file, CssError};

    process_css_file(content, path, true, true)
        .map(|r| (r.code, r.exports))
        .map_err(|e| match e {
            CssError::Parse(msg) | CssError::Transform(msg) | CssError::Print(msg) => msg,
        })
}

/// Process a Sass/SCSS file, compiling to CSS and applying lightningcss transforms.
pub fn process_sass(content: &str, path: &Path) -> Result<String, String> {
    use crate::css::sass::{compile_sass, SassOptions};

    let options = SassOptions {
        include_paths: vec![],
        minify: true,
        filename: Some(path.display().to_string()),
    };

    // Compile Sass to CSS
    let css =
        compile_sass(content, &options).map_err(|e| format!("Sass compilation error: {}", e))?;

    // Apply lightningcss processing
    Ok(process_css(&css))
}

/// Basic CSS minification fallback (comment removal, whitespace collapse).
fn basic_css_minify(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut in_comment = false;
    let mut last_char = ' ';

    for c in content.chars() {
        if in_comment {
            if last_char == '*' && c == '/' {
                in_comment = false;
            }
            last_char = c;
            continue;
        }

        if last_char == '/' && c == '*' {
            in_comment = true;
            result.pop(); // Remove the '/'
            last_char = c;
            continue;
        }

        // Collapse whitespace
        if c.is_whitespace() {
            if !last_char.is_whitespace()
                && last_char != '{'
                && last_char != ';'
                && last_char != ':'
            {
                result.push(' ');
            }
            last_char = ' ';
            continue;
        }

        // Skip space after certain chars
        if last_char == ' ' && (c == '{' || c == '}' || c == ';' || c == ':' || c == ',') {
            result.pop();
        }

        result.push(c);
        last_char = c;
    }

    result.trim().to_string()
}

/// Generate JavaScript code for CSS injection.
pub fn generate_css_injection(css_url: &str) -> String {
    format!(
        r"(function() {{
  var link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = '{}';
  document.head.appendChild(link);
}})();
",
        css_url
    )
}

/// Generate JavaScript code for asset URL export.
pub fn generate_asset_url_export(url: &str) -> String {
    format!("export default '{}';", url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_type_detection() {
        assert_eq!(AssetType::from_extension("css"), Some(AssetType::Css));
        assert_eq!(AssetType::from_extension("png"), Some(AssetType::Image));
        assert_eq!(AssetType::from_extension("woff2"), Some(AssetType::Font));
        assert_eq!(AssetType::from_extension("ts"), None);
    }

    #[test]
    fn test_css_minification() {
        let css = "
            .foo {
                color: red;
                /* comment */
                margin: 10px;
            }
        ";
        let minified = process_css(css);
        assert!(!minified.contains("comment"));
        assert!(minified.contains("color:red"));
    }

    #[test]
    fn test_asset_collection() {
        let mut collection = AssetCollection::new();

        let css_name = collection.add_css(
            Path::new("/test/style.css"),
            ".foo { color: red; }".to_string(),
        );

        assert!(css_name.starts_with("style."));
        assert!(css_name.ends_with(".css"));
        assert!(collection.has_css());
    }

    #[test]
    fn test_content_hashing() {
        let hash1 = hash_content("hello");
        let hash2 = hash_content("hello");
        let hash3 = hash_content("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
