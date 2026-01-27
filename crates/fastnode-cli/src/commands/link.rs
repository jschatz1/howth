//! `howth link` command implementation.
//!
//! Link local packages for development, similar to `npm link` or `bun link`.
//!
//! Usage:
//! - `howth link` - Register the current package as linkable
//! - `howth link <pkg>` - Link a registered package into the current project
//! - `howth unlink` - Unregister the current package
//! - `howth unlink <pkg>` - Remove a linked package from the current project

use fastnode_core::config::Channel;
use fastnode_core::paths::data_dir;
use miette::Result;
use serde_json::Value;
use std::path::Path;

/// Get the directory where linked packages are registered.
fn links_dir(channel: Channel) -> std::path::PathBuf {
    data_dir(channel).join("links")
}

/// Run the link command.
///
/// If `package` is None, register the current package.
/// If `package` is Some, link that package into node_modules.
pub fn link(cwd: &Path, package: Option<&str>, save: bool, channel: Channel, json: bool) -> Result<()> {
    match package {
        None => register_package(cwd, channel, json),
        Some(pkg) => link_package(cwd, pkg, save, channel, json),
    }
}

/// Run the unlink command.
///
/// If `package` is None, unregister the current package.
/// If `package` is Some, remove that package from node_modules.
pub fn unlink(cwd: &Path, package: Option<&str>, channel: Channel, json: bool) -> Result<()> {
    match package {
        None => unregister_package(cwd, channel, json),
        Some(pkg) => unlink_package(cwd, pkg, json),
    }
}

/// Register the current package as linkable.
fn register_package(cwd: &Path, channel: Channel, json: bool) -> Result<()> {
    // Read package.json to get the package name
    let package_json_path = cwd.join("package.json");
    if !package_json_path.exists() {
        if json {
            println!("{}", serde_json::json!({
                "ok": false,
                "error": {
                    "code": "NO_PACKAGE_JSON",
                    "message": "No package.json found in current directory"
                }
            }));
        } else {
            eprintln!("error: No package.json found in current directory");
        }
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| miette::miette!("Failed to read package.json: {}", e))?;
    let package: Value = serde_json::from_str(&content)
        .map_err(|e| miette::miette!("Failed to parse package.json: {}", e))?;

    let name = package
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| miette::miette!("package.json must have a 'name' field"))?;

    // Create links directory
    let links = links_dir(channel);
    std::fs::create_dir_all(&links)
        .map_err(|e| miette::miette!("Failed to create links directory: {}", e))?;

    // Create symlink: links/<name> -> cwd
    let link_path = links.join(name);

    // Remove existing link if present
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        std::fs::remove_file(&link_path)
            .or_else(|_| std::fs::remove_dir(&link_path))
            .map_err(|e| miette::miette!("Failed to remove existing link: {}", e))?;
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(cwd, &link_path)
            .map_err(|e| miette::miette!("Failed to create symlink: {}", e))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(cwd, &link_path)
            .map_err(|e| miette::miette!("Failed to create symlink: {}", e))?;
    }

    if json {
        println!("{}", serde_json::json!({
            "ok": true,
            "action": "register",
            "package": name,
            "path": cwd.to_string_lossy()
        }));
    } else {
        println!("Registered {} -> {}", name, cwd.display());
        println!("\nRun `howth link {}` in another project to use it.", name);
    }

    Ok(())
}

/// Unregister the current package.
fn unregister_package(cwd: &Path, channel: Channel, json: bool) -> Result<()> {
    // Read package.json to get the package name
    let package_json_path = cwd.join("package.json");
    if !package_json_path.exists() {
        if json {
            println!("{}", serde_json::json!({
                "ok": false,
                "error": {
                    "code": "NO_PACKAGE_JSON",
                    "message": "No package.json found in current directory"
                }
            }));
        } else {
            eprintln!("error: No package.json found in current directory");
        }
        std::process::exit(1);
    }

    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| miette::miette!("Failed to read package.json: {}", e))?;
    let package: Value = serde_json::from_str(&content)
        .map_err(|e| miette::miette!("Failed to parse package.json: {}", e))?;

    let name = package
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| miette::miette!("package.json must have a 'name' field"))?;

    let link_path = links_dir(channel).join(name);

    if !link_path.exists() && link_path.symlink_metadata().is_err() {
        if json {
            println!("{}", serde_json::json!({
                "ok": false,
                "error": {
                    "code": "NOT_REGISTERED",
                    "message": format!("Package '{}' is not registered", name)
                }
            }));
        } else {
            eprintln!("error: Package '{}' is not registered", name);
        }
        std::process::exit(1);
    }

    std::fs::remove_file(&link_path)
        .or_else(|_| std::fs::remove_dir(&link_path))
        .map_err(|e| miette::miette!("Failed to remove link: {}", e))?;

    if json {
        println!("{}", serde_json::json!({
            "ok": true,
            "action": "unregister",
            "package": name
        }));
    } else {
        println!("Unregistered {}", name);
    }

    Ok(())
}

/// Link a registered package into node_modules.
fn link_package(cwd: &Path, pkg: &str, save: bool, channel: Channel, json: bool) -> Result<()> {
    let link_source = links_dir(channel).join(pkg);

    // Check if package is registered
    if !link_source.exists() && link_source.symlink_metadata().is_err() {
        if json {
            println!("{}", serde_json::json!({
                "ok": false,
                "error": {
                    "code": "NOT_REGISTERED",
                    "message": format!("Package '{}' is not registered. Run `howth link` in the package directory first.", pkg)
                }
            }));
        } else {
            eprintln!("error: Package '{}' is not registered", pkg);
            eprintln!("hint: Run `howth link` in the {} directory first", pkg);
        }
        std::process::exit(1);
    }

    // Resolve the symlink to get the actual path
    let actual_path = std::fs::read_link(&link_source)
        .map_err(|e| miette::miette!("Failed to read link: {}", e))?;

    // Create node_modules if it doesn't exist
    let node_modules = cwd.join("node_modules");
    std::fs::create_dir_all(&node_modules)
        .map_err(|e| miette::miette!("Failed to create node_modules: {}", e))?;

    // Handle scoped packages (e.g., @scope/pkg)
    let link_dest = if pkg.starts_with('@') {
        // Create scope directory
        let parts: Vec<&str> = pkg.splitn(2, '/').collect();
        if parts.len() == 2 {
            let scope_dir = node_modules.join(parts[0]);
            std::fs::create_dir_all(&scope_dir)
                .map_err(|e| miette::miette!("Failed to create scope directory: {}", e))?;
            scope_dir.join(parts[1])
        } else {
            node_modules.join(pkg)
        }
    } else {
        node_modules.join(pkg)
    };

    // Remove existing if present
    if link_dest.exists() || link_dest.symlink_metadata().is_ok() {
        std::fs::remove_file(&link_dest)
            .or_else(|_| std::fs::remove_dir_all(&link_dest))
            .map_err(|e| miette::miette!("Failed to remove existing: {}", e))?;
    }

    // Create symlink
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&actual_path, &link_dest)
            .map_err(|e| miette::miette!("Failed to create symlink: {}", e))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(&actual_path, &link_dest)
            .map_err(|e| miette::miette!("Failed to create symlink: {}", e))?;
    }

    // Optionally add to package.json
    if save {
        add_link_to_package_json(cwd, pkg)?;
    }

    if json {
        println!("{}", serde_json::json!({
            "ok": true,
            "action": "link",
            "package": pkg,
            "from": actual_path.to_string_lossy(),
            "to": link_dest.to_string_lossy()
        }));
    } else {
        println!("Linked {} -> {}", pkg, actual_path.display());
    }

    Ok(())
}

/// Remove a linked package from node_modules.
fn unlink_package(cwd: &Path, pkg: &str, json: bool) -> Result<()> {
    let node_modules = cwd.join("node_modules");

    let link_dest = if pkg.starts_with('@') {
        let parts: Vec<&str> = pkg.splitn(2, '/').collect();
        if parts.len() == 2 {
            node_modules.join(parts[0]).join(parts[1])
        } else {
            node_modules.join(pkg)
        }
    } else {
        node_modules.join(pkg)
    };

    if !link_dest.exists() && link_dest.symlink_metadata().is_err() {
        if json {
            println!("{}", serde_json::json!({
                "ok": false,
                "error": {
                    "code": "NOT_LINKED",
                    "message": format!("Package '{}' is not linked in this project", pkg)
                }
            }));
        } else {
            eprintln!("error: Package '{}' is not linked in this project", pkg);
        }
        std::process::exit(1);
    }

    std::fs::remove_file(&link_dest)
        .or_else(|_| std::fs::remove_dir_all(&link_dest))
        .map_err(|e| miette::miette!("Failed to remove link: {}", e))?;

    if json {
        println!("{}", serde_json::json!({
            "ok": true,
            "action": "unlink",
            "package": pkg
        }));
    } else {
        println!("Unlinked {}", pkg);
    }

    Ok(())
}

/// Add a link: specifier to package.json dependencies.
fn add_link_to_package_json(cwd: &Path, pkg: &str) -> Result<()> {
    let package_json_path = cwd.join("package.json");

    if !package_json_path.exists() {
        return Ok(()); // No package.json to update
    }

    let content = std::fs::read_to_string(&package_json_path)
        .map_err(|e| miette::miette!("Failed to read package.json: {}", e))?;

    let mut package: Value = serde_json::from_str(&content)
        .map_err(|e| miette::miette!("Failed to parse package.json: {}", e))?;

    // Add to dependencies
    if let Some(obj) = package.as_object_mut() {
        let deps = obj
            .entry("dependencies")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));

        if let Some(deps_obj) = deps.as_object_mut() {
            deps_obj.insert(pkg.to_string(), Value::String(format!("link:{}", pkg)));
        }
    }

    let output = serde_json::to_string_pretty(&package)
        .map_err(|e| miette::miette!("Failed to serialize package.json: {}", e))?;

    std::fs::write(&package_json_path, output + "\n")
        .map_err(|e| miette::miette!("Failed to write package.json: {}", e))?;

    Ok(())
}

/// List all registered packages.
pub fn list(channel: Channel, json: bool) -> Result<()> {
    let links = links_dir(channel);

    if !links.exists() {
        if json {
            println!("{}", serde_json::json!({
                "ok": true,
                "packages": []
            }));
        } else {
            println!("No linked packages registered.");
        }
        return Ok(());
    }

    let mut packages: Vec<serde_json::Value> = Vec::new();

    for entry in std::fs::read_dir(&links)
        .map_err(|e| miette::miette!("Failed to read links directory: {}", e))?
    {
        let entry = entry.map_err(|e| miette::miette!("Failed to read entry: {}", e))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if let Ok(target) = std::fs::read_link(entry.path()) {
            packages.push(serde_json::json!({
                "name": name,
                "path": target.to_string_lossy()
            }));
        }
    }

    if json {
        println!("{}", serde_json::json!({
            "ok": true,
            "packages": packages
        }));
    } else {
        if packages.is_empty() {
            println!("No linked packages registered.");
        } else {
            println!("Registered packages:");
            for pkg in &packages {
                println!("  {} -> {}",
                    pkg.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                    pkg.get("path").and_then(|p| p.as_str()).unwrap_or("?")
                );
            }
        }
    }

    Ok(())
}
