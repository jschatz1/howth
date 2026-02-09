//! Parse TypeScript files from directories and report compatibility results.
//!
//! Usage: cargo run --example parse_ts_dir -p howth-parser --features full -- [OPTIONS] <dir>...
//!
//! Options:
//!   --limit N     Parse at most N files total
//!   --fail-fast   Stop on first failure

use howth_parser::{Codegen, CodegenOptions, Parser, ParserOptions};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn collect_ts_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !matches!(
                name,
                "node_modules" | ".git" | "target" | "__snapshots__" | "node_modules.deno"
            ) {
                collect_ts_files(&path, files);
            }
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(ext, "ts" | "tsx" | "mts" | "cts") {
                files.push(path);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut limit: Option<usize> = None;
    let mut fail_fast = false;
    let mut codegen = false;
    let mut dirs = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" if i + 1 < args.len() => {
                limit = args[i + 1].parse().ok();
                i += 2;
            }
            "--fail-fast" => {
                fail_fast = true;
                i += 1;
            }
            "--codegen" => {
                codegen = true;
                i += 1;
            }
            _ => {
                dirs.push(args[i].clone());
                i += 1;
            }
        }
    }

    if dirs.is_empty() {
        eprintln!("Usage: parse_ts_dir [--limit N] [--fail-fast] [--codegen] <dir1> [dir2] ...");
        std::process::exit(1);
    }

    let mut files = Vec::new();
    for dir in &dirs {
        let path = Path::new(dir);
        if !path.exists() {
            eprintln!("Warning: {} does not exist, skipping", dir);
            continue;
        }
        collect_ts_files(path, &mut files);
    }
    files.sort();

    if let Some(n) = limit {
        files.truncate(n);
    }

    let total = files.len();
    if total == 0 {
        println!("No TypeScript files found.");
        return;
    }
    println!("Found {} TypeScript files to parse\n", total);

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    let start = Instant::now();

    for (i, path) in files.iter().enumerate() {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                skipped += 1;
                println!("[{:>4}/{}] SKIP {} ({})", i + 1, total, path.display(), e);
                continue;
            }
        };

        // Strip UTF-8 BOM if present
        let source = source.strip_prefix('\u{feff}').unwrap_or(&source);

        let is_tsx = path.extension().map_or(false, |e| e == "tsx");
        let opts = ParserOptions {
            module: true,
            jsx: is_tsx,
            typescript: true,
            ..Default::default()
        };

        match Parser::new(source, opts).parse() {
            Ok(ast) => {
                if codegen {
                    // Also test codegen (type stripping)
                    let _output = Codegen::new(&ast, CodegenOptions::default()).generate();
                }
                passed += 1;
                println!("[{:>4}/{}] PASS {}", i + 1, total, path.display());
            }
            Err(e) => {
                failed += 1;
                let msg = e.to_string();
                println!(
                    "[{:>4}/{}] FAIL {} -- {}",
                    i + 1,
                    total,
                    path.display(),
                    msg
                );
                failures.push((path.clone(), msg));
                if fail_fast {
                    break;
                }
            }
        }
    }

    let elapsed = start.elapsed();
    let tested = passed + failed;

    println!();
    println!("============================================================");
    println!("  TypeScript Compatibility Results");
    println!("============================================================");
    println!("  Total files:  {}", total);
    println!("  Tested:       {}", tested);
    println!(
        "  Passed:       {} ({:.1}%)",
        passed,
        if tested > 0 {
            passed as f64 / tested as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "  Failed:       {} ({:.1}%)",
        failed,
        if tested > 0 {
            failed as f64 / tested as f64 * 100.0
        } else {
            0.0
        }
    );
    println!("  Skipped:      {}", skipped);
    println!(
        "  Time:         {:.2}s ({:.0} files/sec)",
        elapsed.as_secs_f64(),
        if elapsed.as_secs_f64() > 0.0 {
            tested as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        }
    );
    println!("============================================================");

    if !failures.is_empty() {
        // Group failures by error category
        let mut error_groups: HashMap<String, Vec<&PathBuf>> = HashMap::new();
        for (path, err) in &failures {
            let key = if let Some(idx) = err.find(" at ") {
                err[..idx].to_string()
            } else {
                err.clone()
            };
            error_groups.entry(key).or_default().push(path);
        }

        let mut groups: Vec<_> = error_groups.into_iter().collect();
        groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        println!("\nErrors by category:");
        for (error, paths) in &groups {
            println!("\n  {} ({} files)", error, paths.len());
            for path in paths.iter().take(5) {
                println!("    - {}", path.display());
            }
            if paths.len() > 5 {
                println!("    ... and {} more", paths.len() - 5);
            }
        }
    }

    std::process::exit(if failed > 0 { 1 } else { 0 });
}
