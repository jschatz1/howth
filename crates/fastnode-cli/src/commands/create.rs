//! `howth create` command implementation.
//!
//! Scaffolds a new project from a template.

use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Command;

/// Known templates with their GitHub repositories.
const KNOWN_TEMPLATES: &[(&str, &str)] = &[
    ("react", "facebook/create-react-app"),
    ("next", "vercel/next.js/tree/canary/examples/basic"),
    ("vite", "vitejs/vite/tree/main/packages/create-vite"),
    ("astro", "withastro/astro/tree/main/examples/basics"),
    ("remix", "remix-run/remix/tree/main/templates/remix"),
    ("svelte", "sveltejs/kit/tree/main/packages/create-svelte"),
    ("solid", "solidjs/templates/tree/main/ts"),
];

#[derive(Serialize)]
struct CreateResult {
    ok: bool,
    template: String,
    project_name: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Run the create command.
pub fn run(cwd: &Path, template: &str, name: Option<&str>, json: bool) -> Result<()> {
    // Determine project name
    let project_name = name.unwrap_or(template.split('/').last().unwrap_or("my-app"));
    let project_path = cwd.join(project_name);

    // Check if directory already exists
    if project_path.exists() {
        let error = format!("Directory '{}' already exists", project_name);
        if json {
            let result = CreateResult {
                ok: false,
                template: template.to_string(),
                project_name: project_name.to_string(),
                path: project_path.to_string_lossy().to_string(),
                error: Some(error.clone()),
            };
            println!("{}", serde_json::to_string_pretty(&result).into_diagnostic()?);
        } else {
            eprintln!("error: {}", error);
        }
        std::process::exit(1);
    }

    // Resolve template to a GitHub URL or degit-compatible path
    let template_source = resolve_template(template);

    if !json {
        println!("Creating project from template: {}", template);
        println!("Project name: {}", project_name);
    }

    // Try degit first (preferred for templates), fall back to git clone
    let result = try_degit(&template_source, &project_path)
        .or_else(|_| try_git_clone(&template_source, &project_path));

    match result {
        Ok(()) => {
            if json {
                let result = CreateResult {
                    ok: true,
                    template: template.to_string(),
                    project_name: project_name.to_string(),
                    path: project_path.to_string_lossy().to_string(),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).into_diagnostic()?);
            } else {
                println!();
                println!("Created project at: {}", project_path.display());
                println!();
                println!("Next steps:");
                println!("  cd {}", project_name);
                println!("  howth install");
                println!("  howth run dev");
            }
            Ok(())
        }
        Err(e) => {
            if json {
                let result = CreateResult {
                    ok: false,
                    template: template.to_string(),
                    project_name: project_name.to_string(),
                    path: project_path.to_string_lossy().to_string(),
                    error: Some(e.to_string()),
                };
                println!("{}", serde_json::to_string_pretty(&result).into_diagnostic()?);
            } else {
                eprintln!("error: {}", e);
            }
            std::process::exit(1);
        }
    }
}

/// Resolve a template name to a source path.
fn resolve_template(template: &str) -> String {
    // Check if it's a known template alias
    for (alias, repo) in KNOWN_TEMPLATES {
        if template.eq_ignore_ascii_case(alias) {
            return format!("github:{}", repo);
        }
    }

    // Check if it's already a full URL or path
    if template.starts_with("http://")
        || template.starts_with("https://")
        || template.starts_with("github:")
        || template.starts_with("gitlab:")
        || template.starts_with("bitbucket:")
    {
        return template.to_string();
    }

    // Check if it looks like a GitHub user/repo pattern
    if template.contains('/') && !template.contains(':') {
        return format!("github:{}", template);
    }

    // Assume it's a local path or unknown template
    template.to_string()
}

/// Try to use degit for scaffolding (faster, no .git directory).
fn try_degit(source: &str, dest: &Path) -> Result<(), String> {
    // Strip the "github:" prefix for degit
    let degit_source = source.strip_prefix("github:").unwrap_or(source);

    let output = Command::new("npx")
        .arg("degit")
        .arg(degit_source)
        .arg(dest)
        .output()
        .map_err(|e| format!("Failed to run degit: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("degit failed: {}", stderr))
    }
}

/// Fall back to git clone.
fn try_git_clone(source: &str, dest: &Path) -> Result<(), String> {
    // Convert source to a git URL
    let git_url = if source.starts_with("github:") {
        let repo = source.strip_prefix("github:").unwrap();
        // Handle tree/branch paths
        if repo.contains("/tree/") {
            // Extract repo and path
            let parts: Vec<&str> = repo.splitn(2, "/tree/").collect();
            if parts.len() == 2 {
                // Clone the whole repo, we'll handle subdirs later
                format!("https://github.com/{}.git", parts[0])
            } else {
                format!("https://github.com/{}.git", repo)
            }
        } else {
            format!("https://github.com/{}.git", repo)
        }
    } else if source.starts_with("gitlab:") {
        let repo = source.strip_prefix("gitlab:").unwrap();
        format!("https://gitlab.com/{}.git", repo)
    } else if source.starts_with("http://") || source.starts_with("https://") {
        source.to_string()
    } else {
        return Err(format!("Unsupported source format: {}", source));
    };

    let output = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&git_url)
        .arg(dest)
        .output()
        .map_err(|e| format!("Failed to run git clone: {}", e))?;

    if output.status.success() {
        // Remove .git directory to make it a fresh project
        let git_dir = dest.join(".git");
        if git_dir.exists() {
            let _ = std::fs::remove_dir_all(git_dir);
        }
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git clone failed: {}", stderr))
    }
}
