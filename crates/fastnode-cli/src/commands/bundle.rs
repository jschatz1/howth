//! `howth bundle` command implementation.
//!
//! Bundles JavaScript/TypeScript modules into a single output file.

use fastnode_core::bundler::{
    AliasPlugin, BannerPlugin, BundleFormat, BundleOptions, Bundler, JsonPlugin, Plugin,
    ReplacePlugin,
};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

/// Bundle command action.
#[derive(Debug, Clone)]
pub struct BundleAction {
    /// Entry point file.
    pub entry: PathBuf,
    /// Working directory.
    pub cwd: PathBuf,
    /// Output file (if None, prints to stdout).
    pub outfile: Option<PathBuf>,
    /// Output format.
    pub format: BundleFormat,
    /// Minify output.
    pub minify: bool,
    /// Mangle variable names (shorten local variables).
    pub mangle: bool,
    /// Generate source maps.
    pub sourcemap: bool,
    /// External packages (don't bundle).
    pub external: Vec<String>,
    /// Enable tree shaking (dead code elimination).
    pub treeshake: bool,
    /// Enable code splitting for dynamic imports.
    pub splitting: bool,
    /// Define replacements (e.g., __DEV__=false).
    pub define: Vec<String>,
    /// Import aliases (e.g., @=./src).
    pub alias: Vec<String>,
    /// Banner text to prepend.
    pub banner: Option<String>,
}

/// JSON output for bundle command.
#[derive(Serialize)]
struct BundleResultJson {
    ok: bool,
    entry: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    outfile: Option<String>,
    format: String,
    modules: Vec<String>,
    size_bytes: usize,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BundleErrorJson>,
}

#[derive(Serialize)]
struct BundleErrorJson {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

/// Run the bundle command.
pub fn run(action: BundleAction, json: bool) -> Result<()> {
    let start = Instant::now();

    // Build plugins from CLI options
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();

    // Add JSON plugin by default
    plugins.push(Box::new(JsonPlugin));

    // Add define/replace plugin
    if !action.define.is_empty() {
        let mut replace = ReplacePlugin::new();
        for def in &action.define {
            if let Some((key, value)) = def.split_once('=') {
                replace = replace.replace(key.trim(), value.trim());
            }
        }
        plugins.push(Box::new(replace));
    }

    // Add alias plugin
    if !action.alias.is_empty() {
        let mut alias_plugin = AliasPlugin::new();
        for a in &action.alias {
            if let Some((from, to)) = a.split_once('=') {
                alias_plugin = alias_plugin.alias(from.trim(), to.trim());
            }
        }
        plugins.push(Box::new(alias_plugin));
    }

    // Add banner plugin
    if let Some(ref banner) = action.banner {
        plugins.push(Box::new(BannerPlugin::new().banner(banner)));
    }

    // Create bundler with plugins
    let bundler = Bundler::with_cwd(&action.cwd).plugins(plugins);

    // Create options
    let options = BundleOptions {
        format: action.format,
        minify: action.minify,
        mangle: action.mangle,
        sourcemap: action.sourcemap,
        external: action.external.clone(),
        treeshake: action.treeshake,
        splitting: action.splitting,
        ..Default::default()
    };

    // Run bundler
    let result = bundler.bundle(&action.entry, &action.cwd, &options);

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(bundle_result) => {
            let code = &bundle_result.code;
            let size_bytes = code.len();
            let has_chunks = !bundle_result.chunks.is_empty();

            // Write output
            if let Some(ref outfile) = action.outfile {
                // Ensure parent directory exists
                if let Some(parent) = outfile.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).into_diagnostic()?;
                    }
                }
                std::fs::write(outfile, code).into_diagnostic()?;

                // Write sourcemap if generated
                if let Some(ref map) = bundle_result.map {
                    let map_path = outfile.with_extension("js.map");
                    std::fs::write(&map_path, map).into_diagnostic()?;
                }

                // Write additional chunks if code splitting is enabled
                if has_chunks {
                    let parent = outfile.parent().unwrap_or(std::path::Path::new("."));
                    for chunk in &bundle_result.chunks {
                        let chunk_path = parent.join(format!("{}.js", chunk.name));
                        std::fs::write(&chunk_path, &chunk.code).into_diagnostic()?;
                    }

                    // Write manifest
                    if let Some(ref manifest) = bundle_result.manifest {
                        let manifest_path = parent.join("manifest.json");
                        std::fs::write(&manifest_path, manifest.to_json()).into_diagnostic()?;
                    }
                }

                // Write CSS if any
                let parent = outfile.parent().unwrap_or(std::path::Path::new("."));
                if let Some(ref css) = bundle_result.css {
                    let css_path = parent.join(&css.name);
                    std::fs::write(&css_path, &css.code).into_diagnostic()?;
                }

                // Copy assets
                for asset in &bundle_result.assets {
                    let asset_path = parent.join(&asset.name);
                    std::fs::copy(&asset.source, &asset_path).into_diagnostic()?;
                }
            }

            if json {
                let json_result = BundleResultJson {
                    ok: true,
                    entry: action.entry.display().to_string(),
                    outfile: action.outfile.as_ref().map(|p| p.display().to_string()),
                    format: format_to_string(action.format),
                    modules: bundle_result.modules,
                    size_bytes,
                    duration_ms,
                    error: None,
                };
                println!("{}", serde_json::to_string(&json_result).unwrap());
            } else if let Some(outfile) = &action.outfile {
                // Human output - print summary
                let modules_count = bundle_result.modules.len();
                let size_kb = size_bytes as f64 / 1024.0;

                if has_chunks {
                    let chunk_count = bundle_result.chunks.len();
                    println!(
                        "  {} -> {} ({} modules, {} chunks, {:.1}KB, {}ms)",
                        action.entry.display(),
                        outfile.display(),
                        modules_count,
                        chunk_count + 1, // +1 for main chunk
                        size_kb,
                        duration_ms
                    );
                    for chunk in &bundle_result.chunks {
                        let chunk_kb = chunk.code.len() as f64 / 1024.0;
                        println!("    + {}.js ({:.1}KB)", chunk.name, chunk_kb);
                    }
                } else {
                    println!(
                        "  {} -> {} ({} modules, {:.1}KB, {}ms)",
                        action.entry.display(),
                        outfile.display(),
                        modules_count,
                        size_kb,
                        duration_ms
                    );
                }

                // Show CSS output
                if let Some(ref css) = bundle_result.css {
                    let css_kb = css.code.len() as f64 / 1024.0;
                    println!("    + {} ({:.1}KB)", css.name, css_kb);
                }

                // Show assets
                for asset in &bundle_result.assets {
                    println!("    + {}", asset.name);
                }

                // Show warnings
                for warning in &bundle_result.warnings {
                    eprintln!("  warning: {warning}");
                }
            } else {
                // No outfile, print code to stdout
                print!("{code}");
            }

            Ok(())
        }
        Err(e) => {
            if json {
                let json_result = BundleResultJson {
                    ok: false,
                    entry: action.entry.display().to_string(),
                    outfile: action.outfile.as_ref().map(|p| p.display().to_string()),
                    format: format_to_string(action.format),
                    modules: Vec::new(),
                    size_bytes: 0,
                    duration_ms,
                    error: Some(BundleErrorJson {
                        code: e.code.to_string(),
                        message: e.message.clone(),
                        path: e.path.clone(),
                    }),
                };
                println!("{}", serde_json::to_string(&json_result).unwrap());
            } else {
                eprintln!("error: {}", e);
                if let Some(path) = &e.path {
                    eprintln!("  at {path}");
                }
            }
            std::process::exit(1);
        }
    }
}

fn format_to_string(format: BundleFormat) -> String {
    match format {
        BundleFormat::Esm => "esm".to_string(),
        BundleFormat::Cjs => "cjs".to_string(),
        BundleFormat::Iife => "iife".to_string(),
    }
}

/// Parse format string to BundleFormat.
pub fn parse_format(s: &str) -> Option<BundleFormat> {
    match s.to_lowercase().as_str() {
        "esm" | "es" | "module" => Some(BundleFormat::Esm),
        "cjs" | "commonjs" => Some(BundleFormat::Cjs),
        "iife" => Some(BundleFormat::Iife),
        _ => None,
    }
}
