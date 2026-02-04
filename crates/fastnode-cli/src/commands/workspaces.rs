//! `howth workspaces` command implementation.
//!
//! List and manage workspace packages in a monorepo.

use fastnode_core::pkg::{detect_workspaces, find_workspace_root, link_workspace_packages};
use miette::Result;
use std::path::Path;

/// Run the workspaces command.
pub fn run(cwd: &Path, json: bool) -> Result<()> {
    // Try to find workspace root
    let root = find_workspace_root(cwd).unwrap_or_else(|| cwd.to_path_buf());

    // Detect workspaces
    let config = if let Some(c) = detect_workspaces(&root) { c } else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "workspaces": false,
                    "packages": []
                })
            );
        } else {
            println!("No workspaces configured.");
            println!("hint: Add a \"workspaces\" field to package.json");
        }
        return Ok(());
    };

    // Collect and sort packages
    let mut packages: Vec<_> = config.packages.values().collect();
    packages.sort_by(|a, b| a.name.cmp(&b.name));

    if json {
        let pkg_list: Vec<_> = packages
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "version": p.version,
                    "path": p.path.to_string_lossy()
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "workspaces": true,
                "root": root.to_string_lossy(),
                "packages": pkg_list
            })
        );
    } else {
        println!("Workspace root: {}", root.display());
        println!();
        println!("Packages ({}):", packages.len());
        for pkg in &packages {
            println!("  {} @ {}", pkg.name, pkg.version);
            println!("    {}", pkg.path.display());
        }
    }

    Ok(())
}

/// Link all workspace packages into the current project's node_modules.
pub fn link(cwd: &Path, json: bool) -> Result<()> {
    let root = find_workspace_root(cwd).unwrap_or_else(|| cwd.to_path_buf());

    let config = if let Some(c) = detect_workspaces(&root) { c } else {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": "NO_WORKSPACES",
                        "message": "No workspaces configured"
                    }
                })
            );
        } else {
            eprintln!("error: No workspaces configured");
        }
        std::process::exit(1);
    };

    match link_workspace_packages(cwd, &config) {
        Ok(linked) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": true,
                        "linked": linked
                    })
                );
            } else if linked.is_empty() {
                println!("No workspace packages to link.");
            } else {
                println!("Linked {} workspace package(s):", linked.len());
                for name in &linked {
                    println!("  + {}", name);
                }
            }
            Ok(())
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "ok": false,
                        "error": {
                            "code": "LINK_FAILED",
                            "message": e.to_string()
                        }
                    })
                );
            } else {
                eprintln!("error: {}", e);
            }
            std::process::exit(1);
        }
    }
}
