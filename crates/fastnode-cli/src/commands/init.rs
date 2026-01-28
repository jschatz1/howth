//! `howth init` command implementation.
//!
//! Scaffolds a new project with package.json, index.ts, and tsconfig.json.
//! Non-destructive: won't overwrite existing files.

use miette::Result;
use std::io::{self, Write};
use std::path::Path;

/// Default tsconfig.json content
const TSCONFIG_TEMPLATE: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "outDir": "dist",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  },
  "include": ["src/**/*", "*.ts"],
  "exclude": ["node_modules", "dist"]
}
"#;

/// Default index.ts content
const INDEX_TEMPLATE: &str = r#"console.log("Hello from howth!");
"#;

/// Run the init command.
pub fn run(cwd: &Path, yes: bool, json: bool) -> Result<()> {
    // Get project name from directory
    let dir_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project")
        .to_string();

    // Prompt for project name (unless -y)
    let project_name = if yes {
        dir_name.clone()
    } else {
        prompt(&format!("package name ({}): ", dir_name))?
            .filter(|s| !s.is_empty())
            .unwrap_or(dir_name.clone())
    };

    // Prompt for entry point
    let entry_point = if yes {
        detect_entry_point(cwd)
    } else {
        let default_entry = detect_entry_point(cwd);
        prompt(&format!("entry point ({}): ", default_entry))?
            .filter(|s| !s.is_empty())
            .unwrap_or(default_entry)
    };

    // Determine if TypeScript
    let is_typescript = entry_point.ends_with(".ts") || entry_point.ends_with(".tsx");

    // Track what we created
    let mut created: Vec<&str> = Vec::new();
    let mut skipped: Vec<&str> = Vec::new();

    // Create package.json
    let package_json_path = cwd.join("package.json");
    if package_json_path.exists() {
        skipped.push("package.json");
    } else {
        let package_json = create_package_json(&project_name, &entry_point);
        std::fs::write(&package_json_path, package_json)
            .map_err(|e| miette::miette!("Failed to write package.json: {}", e))?;
        created.push("package.json");
    }

    // Create entry point file
    let entry_path = cwd.join(&entry_point);
    if entry_path.exists() {
        skipped.push("entry point");
    } else {
        // Create parent directories if needed
        if let Some(parent) = entry_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| miette::miette!("Failed to create directory: {}", e))?;
            }
        }
        std::fs::write(&entry_path, INDEX_TEMPLATE)
            .map_err(|e| miette::miette!("Failed to write {}: {}", entry_point, e))?;
        created.push("entry point");
    }

    // Create tsconfig.json (only for TypeScript)
    if is_typescript {
        let tsconfig_path = cwd.join("tsconfig.json");
        if tsconfig_path.exists() {
            skipped.push("tsconfig.json");
        } else {
            std::fs::write(&tsconfig_path, TSCONFIG_TEMPLATE)
                .map_err(|e| miette::miette!("Failed to write tsconfig.json: {}", e))?;
            created.push("tsconfig.json");
        }
    }

    // Create .gitignore
    let gitignore_path = cwd.join(".gitignore");
    if gitignore_path.exists() {
        skipped.push(".gitignore");
    } else {
        let gitignore = "node_modules/\ndist/\n*.log\n.DS_Store\n";
        std::fs::write(&gitignore_path, gitignore)
            .map_err(|e| miette::miette!("Failed to write .gitignore: {}", e))?;
        created.push(".gitignore");
    }

    // Output results
    if json {
        let output = serde_json::json!({
            "ok": true,
            "project": {
                "name": project_name,
                "entry": entry_point,
                "typescript": is_typescript
            },
            "created": created,
            "skipped": skipped
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        if !created.is_empty() {
            println!("Created:");
            for file in &created {
                println!("  + {}", file);
            }
        }
        if !skipped.is_empty() {
            println!("Skipped (already exists):");
            for file in &skipped {
                println!("  - {}", file);
            }
        }
        println!("\nDone! Run `howth run {}` to start.", entry_point);
    }

    Ok(())
}

/// Detect existing entry point or return default
fn detect_entry_point(cwd: &Path) -> String {
    let candidates = [
        "index.ts",
        "index.tsx",
        "index.js",
        "index.jsx",
        "index.mts",
        "index.mjs",
        "src/index.ts",
        "src/index.tsx",
        "src/index.js",
    ];

    for candidate in candidates {
        if cwd.join(candidate).exists() {
            return candidate.to_string();
        }
    }

    "index.ts".to_string()
}

/// Create package.json content
fn create_package_json(name: &str, entry: &str) -> String {
    let main = if entry.ends_with(".ts") || entry.ends_with(".tsx") {
        entry.replace(".ts", ".js").replace(".tsx", ".js")
    } else {
        entry.to_string()
    };

    let package = serde_json::json!({
        "name": name,
        "version": "0.1.0",
        "main": main,
        "type": "module",
        "scripts": {
            "start": format!("howth run {}", entry),
            "build": "howth build",
            "test": "howth test"
        }
    });

    serde_json::to_string_pretty(&package).unwrap() + "\n"
}

/// Prompt the user for input
fn prompt(message: &str) -> Result<Option<String>> {
    print!("{}", message);
    io::stdout()
        .flush()
        .map_err(|e| miette::miette!("Failed to flush stdout: {}", e))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| miette::miette!("Failed to read input: {}", e))?;

    let trimmed = input.trim().to_string();
    Ok(if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    })
}
