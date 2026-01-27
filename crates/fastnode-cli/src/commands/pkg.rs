//! `fastnode pkg` command implementation.

use fastnode_core::config::Channel;
use fastnode_core::paths;
use fastnode_core::pkg::{read_package_deps, PkgDepError};
use fastnode_core::VERSION;
use fastnode_daemon::ipc::{IpcStream, MAX_FRAME_SIZE};
use fastnode_proto::{
    encode_frame, CachedPackage, DoctorFinding, Frame, FrameResponse, GraphDepEdge,
    GraphPackageNode, InstalledPackage, PackageGraph, PkgDoctorReport, PkgErrorInfo,
    PkgExplainResult, PkgInstallResult, PkgWhyChain, PkgWhyResult, Request, Response,
    UpdatedPackage,
};
use miette::{IntoDiagnostic, Result};
use serde::Serialize;
use std::io;
use std::path::PathBuf;

/// Pkg command action.
#[derive(Debug, Clone)]
pub enum PkgAction {
    Add {
        specs: Vec<String>,
        cwd: PathBuf,
        save_dev: bool,
    },
    AddDeps {
        cwd: PathBuf,
        include_dev: bool,
        include_optional: bool,
    },
    Remove {
        packages: Vec<String>,
        cwd: PathBuf,
    },
    Update {
        packages: Vec<String>,
        cwd: PathBuf,
        latest: bool,
    },
    Graph {
        cwd: PathBuf,
        include_dev: bool,
        include_optional: bool,
        max_depth: u32,
        format: String,
    },
    Explain {
        specifier: String,
        cwd: PathBuf,
        parent: PathBuf,
        kind: String,
    },
    Why {
        arg: String,
        cwd: PathBuf,
        include_dev: bool,
        include_optional: bool,
        max_depth: u32,
        max_chains: u32,
        format: String,
        include_trace: bool,
        trace_kind: Option<String>,
        trace_parent: Option<PathBuf>,
    },
    Doctor {
        cwd: PathBuf,
        include_dev: bool,
        include_optional: bool,
        max_depth: u32,
        max_items: u32,
        min_severity: String,
        format: String,
    },
    Install {
        cwd: PathBuf,
        frozen: bool,
        include_dev: bool,
        include_optional: bool,
    },
    CacheList,
    CachePrune,
}

/// Add result for JSON output.
#[derive(Serialize)]
struct PkgAddResult {
    ok: bool,
    installed: Vec<InstalledPackage>,
    errors: Vec<PkgErrorInfo>,
    reused_cache: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Remove result for JSON output.
#[derive(Serialize)]
struct PkgRemoveResult {
    ok: bool,
    removed: Vec<String>,
    errors: Vec<PkgErrorInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Update result for JSON output.
#[derive(Serialize)]
struct PkgUpdateResult {
    ok: bool,
    updated: Vec<UpdatedPackage>,
    up_to_date: Vec<String>,
    errors: Vec<PkgErrorInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Cache list result for JSON output.
#[derive(Serialize)]
struct PkgCacheListResult {
    ok: bool,
    packages: Vec<CachedPackage>,
    total_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Cache prune result for JSON output.
#[derive(Serialize)]
struct PkgCachePruneResult {
    ok: bool,
    removed_count: u32,
    freed_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Graph result for JSON output.
#[derive(Serialize)]
struct PkgGraphResult {
    ok: bool,
    graph: PackageGraph,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Explain result for JSON output.
#[derive(Serialize)]
struct PkgExplainJsonResult {
    ok: bool,
    result: Option<PkgExplainResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Why result for JSON output (locked format: { ok, why, trace? }).
#[derive(Serialize)]
struct PkgWhyJsonResult {
    ok: bool,
    why: Option<PkgWhyResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Doctor result for JSON output (locked format: { ok, doctor }).
#[derive(Serialize)]
struct PkgDoctorJsonResult {
    ok: bool,
    doctor: Option<PkgDoctorReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Install result for JSON output (locked format: { ok, install }).
#[derive(Serialize)]
struct PkgInstallJsonResult {
    ok: bool,
    install: Option<PkgInstallResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Run the pkg command.
pub fn run(action: PkgAction, channel: Channel, json: bool) -> Result<()> {
    // Handle AddDeps by converting to Add with specs from package.json
    let (effective_action, dep_errors) = match &action {
        PkgAction::AddDeps {
            cwd,
            include_dev,
            include_optional,
        } => {
            let pkg_json_path = cwd.join("package.json");
            match read_package_deps(&pkg_json_path, *include_dev, *include_optional) {
                Ok(pkg_deps) => {
                    // Convert to specs
                    let specs: Vec<String> = pkg_deps
                        .deps
                        .iter()
                        .map(|(name, range)| format!("{name}@{range}"))
                        .collect();

                    // Convert dep errors to PkgErrorInfo
                    let dep_errors: Vec<PkgErrorInfo> = pkg_deps
                        .errors
                        .iter()
                        .map(dep_error_to_pkg_error_info)
                        .collect();

                    if specs.is_empty() && dep_errors.is_empty() {
                        // No deps to install, just return success
                        if json {
                            let result = PkgAddResult {
                                ok: true,
                                installed: Vec::new(),
                                errors: Vec::new(),
                                reused_cache: 0,
                                error: None,
                            };
                            println!("{}", serde_json::to_string_pretty(&result).unwrap());
                        } else {
                            println!("No dependencies to install");
                        }
                        return Ok(());
                    }

                    if specs.is_empty() {
                        // Only errors, no valid deps
                        if json {
                            let result = PkgAddResult {
                                ok: false,
                                installed: Vec::new(),
                                errors: dep_errors,
                                reused_cache: 0,
                                error: None,
                            };
                            println!("{}", serde_json::to_string_pretty(&result).unwrap());
                        } else {
                            for err in &dep_errors {
                                eprintln!("! {}: {} {}", err.spec, err.code, err.message);
                            }
                        }
                        std::process::exit(2);
                    }

                    (
                        PkgAction::Add {
                            specs,
                            cwd: cwd.clone(),
                            save_dev: false, // --deps mode reads from existing package.json
                        },
                        dep_errors,
                    )
                }
                Err(e) => {
                    // Failed to read package.json - exit code 2 for all errors
                    let exit_code = 2;

                    if json {
                        let result = PkgAddResult {
                            ok: false,
                            installed: Vec::new(),
                            errors: Vec::new(),
                            reused_cache: 0,
                            error: Some(format!("{}: {}", e.code(), e.message())),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    } else {
                        eprintln!("error: {e}");
                    }
                    std::process::exit(exit_code);
                }
            }
        }
        other => (other.clone(), Vec::new()),
    };

    // Print header for --deps mode
    let is_deps_mode = matches!(action, PkgAction::AddDeps { .. });
    if is_deps_mode && !json {
        println!("Installing dependencies from package.json");
    }

    let endpoint = paths::ipc_endpoint(channel);

    // Run the async client
    let runtime = tokio::runtime::Runtime::new().into_diagnostic()?;
    let result =
        runtime.block_on(async { send_pkg_request(&endpoint, &effective_action, channel).await });

    match result {
        Ok((response, _server_version)) => {
            handle_response(response, &effective_action, json, dep_errors)
        }
        Err(e) => {
            if json {
                match &effective_action {
                    PkgAction::Add { .. } | PkgAction::AddDeps { .. } => {
                        let result = PkgAddResult {
                            ok: false,
                            installed: Vec::new(),
                            errors: dep_errors,
                            reused_cache: 0,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Remove { .. } => {
                        let result = PkgRemoveResult {
                            ok: false,
                            removed: Vec::new(),
                            errors: Vec::new(),
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Update { .. } => {
                        let result = PkgUpdateResult {
                            ok: false,
                            updated: Vec::new(),
                            up_to_date: Vec::new(),
                            errors: Vec::new(),
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CacheList => {
                        let result = PkgCacheListResult {
                            ok: false,
                            packages: Vec::new(),
                            total_size_bytes: 0,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CachePrune => {
                        let result = PkgCachePruneResult {
                            ok: false,
                            removed_count: 0,
                            freed_bytes: 0,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Graph { .. } => {
                        let result = PkgGraphResult {
                            ok: false,
                            graph: PackageGraph::default(),
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Explain { .. } => {
                        let result = PkgExplainJsonResult {
                            ok: false,
                            result: None,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Why { .. } => {
                        let result = PkgWhyJsonResult {
                            ok: false,
                            why: None,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Doctor { .. } => {
                        let result = PkgDoctorJsonResult {
                            ok: false,
                            doctor: None,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Install { .. } => {
                        let result = PkgInstallJsonResult {
                            ok: false,
                            install: None,
                            error: Some(format!("Failed to connect: {e}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: daemon not running");
                eprintln!("hint: start with `howth daemon`");
            }
            std::process::exit(1);
        }
    }
}

/// Convert a `PkgDepError` to `PkgErrorInfo` for protocol/output.
fn dep_error_to_pkg_error_info(err: &PkgDepError) -> PkgErrorInfo {
    PkgErrorInfo {
        spec: err.name.clone(),
        code: err.code.to_string(),
        message: err.message.clone(),
    }
}

fn handle_response(
    response: Response,
    action: &PkgAction,
    json: bool,
    dep_errors: Vec<PkgErrorInfo>,
) -> Result<()> {
    match response {
        Response::PkgAddResult {
            installed,
            errors,
            reused_cache,
        } => {
            // Merge dep_errors with daemon errors
            let mut all_errors = dep_errors;
            all_errors.extend(errors);

            let has_errors = !all_errors.is_empty();

            if json {
                let result = PkgAddResult {
                    ok: !has_errors,
                    installed,
                    errors: all_errors,
                    reused_cache,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                for pkg in &installed {
                    println!("+ {}@{}", pkg.name, pkg.version);
                }
                if reused_cache > 0 {
                    println!("({reused_cache} from cache)");
                }
                for err in &all_errors {
                    eprintln!("! {}: {} {}", err.spec, err.code, err.message);
                }
            }

            // Exit with code 2 if any errors (both JSON and human mode)
            if has_errors {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::PkgRemoveResult { removed, errors } => {
            let has_errors = !errors.is_empty();

            if json {
                let result = PkgRemoveResult {
                    ok: !has_errors && !removed.is_empty(),
                    removed,
                    errors,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                for pkg in &removed {
                    println!("- {}", pkg);
                }
                for err in &errors {
                    eprintln!("! {}: {} {}", err.spec, err.code, err.message);
                }
            }

            // Exit with code 2 if any errors
            if has_errors {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::PkgUpdateResult {
            updated,
            up_to_date,
            errors,
        } => {
            let has_errors = !errors.is_empty();

            if json {
                let result = PkgUpdateResult {
                    ok: !has_errors,
                    updated,
                    up_to_date,
                    errors,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                if updated.is_empty() && up_to_date.is_empty() && errors.is_empty() {
                    println!("No dependencies to update.");
                } else {
                    for pkg in &updated {
                        println!("~ {} {} -> {}", pkg.name, pkg.from_version, pkg.to_version);
                    }
                    if !up_to_date.is_empty() {
                        println!("({} packages already up to date)", up_to_date.len());
                    }
                    for err in &errors {
                        eprintln!("! {}: {} {}", err.spec, err.code, err.message);
                    }
                }
            }

            // Exit with code 2 if any errors
            if has_errors {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::PkgCacheListResult {
            packages,
            total_size_bytes,
        } => {
            if json {
                let result = PkgCacheListResult {
                    ok: true,
                    packages,
                    total_size_bytes,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                for pkg in &packages {
                    let size_kb = pkg.size_bytes / 1024;
                    println!("{}@{} ({} KB)", pkg.name, pkg.version, size_kb);
                }
                let total_mb = total_size_bytes as f64 / (1024.0 * 1024.0);
                println!("\nTotal: {total_mb:.2} MB");
            }
            Ok(())
        }
        Response::PkgCachePruneResult {
            removed_count,
            freed_bytes,
        } => {
            if json {
                let result = PkgCachePruneResult {
                    ok: true,
                    removed_count,
                    freed_bytes,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                let freed_mb = freed_bytes as f64 / (1024.0 * 1024.0);
                println!("Removed {removed_count} packages, freed {freed_mb:.2} MB");
            }
            Ok(())
        }
        Response::PkgGraphResult { graph } => {
            // Get format from action
            let format = match action {
                PkgAction::Graph { format, .. } => format.as_str(),
                _ => "tree",
            };

            let has_errors = !graph.errors.is_empty();

            if json {
                let result = PkgGraphResult {
                    ok: !has_errors,
                    graph,
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                print_graph_human(&graph, format);
            }

            // Exit with code 2 if any errors
            if has_errors {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::PkgExplainResult {
            result: explain_result,
        } => {
            let is_resolved = explain_result.status == "resolved";

            if json {
                let result = PkgExplainJsonResult {
                    ok: is_resolved,
                    result: Some(explain_result),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                print_explain_human(&explain_result);
            }

            // Exit with code 2 if unresolved
            if !is_resolved {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::PkgWhyResult { result: why_result } => {
            let found = why_result.found_in_node_modules;
            let has_errors = !why_result.errors.is_empty();

            // Get format from action
            let format = match action {
                PkgAction::Why { format, .. } => format.as_str(),
                _ => "tree",
            };

            if json {
                let result = PkgWhyJsonResult {
                    ok: found && !has_errors,
                    why: Some(why_result),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                print_why_human(&why_result, format);
            }

            // Exit with code 0 on successful daemon request, even with errors
            // (per spec: exit 0 on successful request, exit 1 on daemon unreachable)
            Ok(())
        }
        Response::PkgDoctorResult { report } => {
            let has_errors = report.summary.counts.error > 0;

            // Get format from action
            let format = match action {
                PkgAction::Doctor { format, .. } => format.as_str(),
                _ => "summary",
            };

            if json {
                let result = PkgDoctorJsonResult {
                    ok: true,
                    doctor: Some(report),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                print_doctor_human(&report, format);
            }

            // Exit with code 0 on successful daemon request
            // (per spec: exit 0 on successful request, exit 1 on daemon unreachable)
            let _ = has_errors; // silence warning; we don't exit non-zero for findings
            Ok(())
        }
        Response::PkgInstallResult { result } => {
            let has_errors = !result.errors.is_empty();

            if json {
                let output = PkgInstallJsonResult {
                    ok: result.ok,
                    install: Some(result),
                    error: None,
                };
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                // Print human-readable output
                println!("howth install");
                if result.summary.workspace_linked > 0 {
                    println!(
                        "  packages: {} total, {} cached, {} downloaded, {} workspace",
                        result.summary.total_packages,
                        result.summary.cached,
                        result.summary.downloaded,
                        result.summary.workspace_linked
                    );
                } else {
                    println!(
                        "  packages: {} total, {} cached, {} downloaded",
                        result.summary.total_packages,
                        result.summary.cached,
                        result.summary.downloaded
                    );
                }

                if !result.installed.is_empty() {
                    for pkg in &result.installed {
                        let source = if pkg.is_workspace {
                            "workspace"
                        } else if pkg.from_cache {
                            "cached"
                        } else {
                            "downloaded"
                        };
                        println!("  + {}@{} ({})", pkg.name, pkg.version, source);
                    }
                }

                if !result.errors.is_empty() {
                    for err in &result.errors {
                        eprintln!(
                            "  ! {}@{}: {} {}",
                            err.name, err.version, err.code, err.message
                        );
                    }
                }

                if !result.notes.is_empty() {
                    for note in &result.notes {
                        println!("  note: {note}");
                    }
                }
            }

            // Exit with code 2 if any errors
            if has_errors {
                std::process::exit(2);
            }
            Ok(())
        }
        Response::Error { code, message } => {
            if json {
                match action {
                    PkgAction::Add { .. } | PkgAction::AddDeps { .. } => {
                        let result = PkgAddResult {
                            ok: false,
                            installed: Vec::new(),
                            errors: dep_errors,
                            reused_cache: 0,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Remove { .. } => {
                        let result = PkgRemoveResult {
                            ok: false,
                            removed: Vec::new(),
                            errors: Vec::new(),
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Update { .. } => {
                        let result = PkgUpdateResult {
                            ok: false,
                            updated: Vec::new(),
                            up_to_date: Vec::new(),
                            errors: Vec::new(),
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CacheList => {
                        let result = PkgCacheListResult {
                            ok: false,
                            packages: Vec::new(),
                            total_size_bytes: 0,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CachePrune => {
                        let result = PkgCachePruneResult {
                            ok: false,
                            removed_count: 0,
                            freed_bytes: 0,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Graph { .. } => {
                        let result = PkgGraphResult {
                            ok: false,
                            graph: PackageGraph::default(),
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Explain { .. } => {
                        let result = PkgExplainJsonResult {
                            ok: false,
                            result: None,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Why { .. } => {
                        let result = PkgWhyJsonResult {
                            ok: false,
                            why: None,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Doctor { .. } => {
                        let result = PkgDoctorJsonResult {
                            ok: false,
                            doctor: None,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Install { .. } => {
                        let result = PkgInstallJsonResult {
                            ok: false,
                            install: None,
                            error: Some(format!("{code}: {message}")),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: {code}: {message}");
            }
            std::process::exit(1);
        }
        _ => {
            if json {
                match action {
                    PkgAction::Add { .. } | PkgAction::AddDeps { .. } => {
                        let result = PkgAddResult {
                            ok: false,
                            installed: Vec::new(),
                            errors: dep_errors,
                            reused_cache: 0,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Remove { .. } => {
                        let result = PkgRemoveResult {
                            ok: false,
                            removed: Vec::new(),
                            errors: Vec::new(),
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Update { .. } => {
                        let result = PkgUpdateResult {
                            ok: false,
                            updated: Vec::new(),
                            up_to_date: Vec::new(),
                            errors: Vec::new(),
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CacheList => {
                        let result = PkgCacheListResult {
                            ok: false,
                            packages: Vec::new(),
                            total_size_bytes: 0,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::CachePrune => {
                        let result = PkgCachePruneResult {
                            ok: false,
                            removed_count: 0,
                            freed_bytes: 0,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Graph { .. } => {
                        let result = PkgGraphResult {
                            ok: false,
                            graph: PackageGraph::default(),
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Explain { .. } => {
                        let result = PkgExplainJsonResult {
                            ok: false,
                            result: None,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Why { .. } => {
                        let result = PkgWhyJsonResult {
                            ok: false,
                            why: None,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Doctor { .. } => {
                        let result = PkgDoctorJsonResult {
                            ok: false,
                            doctor: None,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                    PkgAction::Install { .. } => {
                        let result = PkgInstallJsonResult {
                            ok: false,
                            install: None,
                            error: Some("Unexpected response type".to_string()),
                        };
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    }
                }
            } else {
                eprintln!("error: unexpected response");
            }
            std::process::exit(1);
        }
    }
}

/// Print the dependency graph in human-readable format.
fn print_graph_human(graph: &PackageGraph, format: &str) {
    // Print errors first
    for err in &graph.errors {
        eprintln!("! [{}] {}: {}", err.code, err.path, err.message);
    }

    if graph.nodes.is_empty() {
        println!("(no packages found)");
        return;
    }

    // Build lookup map for nodes by (name, version)
    let node_map: std::collections::HashMap<(&str, &str), &GraphPackageNode> = graph
        .nodes
        .iter()
        .map(|n| ((n.id.name.as_str(), n.id.version.as_str()), n))
        .collect();

    match format {
        "list" => print_graph_list(graph),
        _ => print_graph_tree(graph, &node_map),
    }
}

/// Print graph as flat list.
fn print_graph_list(graph: &PackageGraph) {
    for node in &graph.nodes {
        println!("{}@{}", node.id.name, node.id.version);
    }

    if !graph.orphans.is_empty() {
        println!("\nOrphans:");
        for orphan in &graph.orphans {
            println!("  {}@{}", orphan.name, orphan.version);
        }
    }
}

/// Print graph as tree.
fn print_graph_tree(
    graph: &PackageGraph,
    node_map: &std::collections::HashMap<(&str, &str), &GraphPackageNode>,
) {
    // Print all top-level nodes (nodes that are direct dependencies)
    // Since we don't have the root project in nodes, just list all packages
    for (i, node) in graph.nodes.iter().enumerate() {
        let is_last = i == graph.nodes.len() - 1 && graph.orphans.is_empty();
        let connector = if is_last { "└── " } else { "├── " };
        let next_prefix = if is_last { "    " } else { "│   " };

        println!("{connector}{}@{}", node.id.name, node.id.version);

        // Print this node's dependencies
        print_deps_tree(&node.dependencies, node_map, next_prefix, 1, 25);
    }

    if !graph.orphans.is_empty() {
        println!("\nOrphans:");
        for orphan in &graph.orphans {
            println!("  {}@{}", orphan.name, orphan.version);
        }
    }
}

/// Recursively print dependencies as tree.
fn print_deps_tree(
    deps: &[GraphDepEdge],
    node_map: &std::collections::HashMap<(&str, &str), &GraphPackageNode>,
    prefix: &str,
    depth: usize,
    max_depth: usize,
) {
    if depth >= max_depth {
        return;
    }

    let len = deps.len();
    for (i, dep) in deps.iter().enumerate() {
        let is_last = i == len - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let next_prefix = if is_last { "    " } else { "│   " };

        if let Some(ref to) = dep.to {
            print!("{prefix}{connector}{}@{}", to.name, to.version);

            // Add kind indicator
            if dep.kind != "prod" {
                print!(" ({})", dep.kind);
            }
            println!();

            // Recurse into dependencies
            if let Some(child_node) = node_map.get(&(to.name.as_str(), to.version.as_str())) {
                let new_prefix = format!("{prefix}{next_prefix}");
                print_deps_tree(
                    &child_node.dependencies,
                    node_map,
                    &new_prefix,
                    depth + 1,
                    max_depth,
                );
            }
        } else {
            // Unresolved dependency
            let req = dep.req.as_deref().unwrap_or("*");
            println!("{prefix}{connector}{} (unresolved: {})", dep.name, req);
        }
    }
}

/// Print the explain result in human-readable format.
fn print_explain_human(result: &PkgExplainResult) {
    // Print header
    println!("Specifier: {}", result.specifier);
    println!("Kind: {}", result.kind);
    println!("Parent: {}", result.parent);
    println!();

    // Print resolution status
    if let Some(ref resolved) = result.resolved {
        println!("Resolved: {resolved}");
    } else {
        println!("Status: UNRESOLVED");
        if let Some(ref code) = result.error_code {
            println!("Error: {code}");
        }
        if let Some(ref msg) = result.error_message {
            println!("Message: {msg}");
        }
    }
    println!();

    // Print trace
    println!("Resolution trace:");
    for (i, step) in result.trace.iter().enumerate() {
        let status = if step.ok { "OK" } else { "FAIL" };
        println!("  {}. [{}] {}: {}", i + 1, status, step.step, step.detail);

        // Print optional fields
        if let Some(ref path) = step.path {
            println!("      path: {path}");
        }
        if let Some(ref key) = step.key {
            println!("      key: {key}");
        }
        if let Some(ref target) = step.target {
            println!("      target: {target}");
        }
        if let Some(ref condition) = step.condition {
            println!("      condition: {condition}");
        }
        for note in &step.notes {
            println!("      note: {note}");
        }
    }

    // Print warnings
    if !result.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in &result.warnings {
            println!("  [{}] {}", warning.code, warning.message);
        }
    }

    // Print tried paths if verbose
    if !result.tried.is_empty() && result.resolved.is_none() {
        println!();
        println!("Tried paths:");
        for path in &result.tried {
            println!("  - {path}");
        }
    }
}

/// Print the why result in human-readable format.
fn print_why_human(result: &PkgWhyResult, format: &str) {
    let target = &result.target;
    let version_str = target.version.as_deref().unwrap_or("unknown");

    // 1) Header line
    println!("Why is {}@{} here?", target.name, version_str);
    if let Some(ref path) = target.path {
        println!("  at: {path}");
    }
    println!();

    // 2) Status messages
    if !result.found_in_node_modules {
        println!("NOT FOUND in node_modules");
        println!();
    } else if result.is_orphan {
        println!("ORPHAN - not reachable from any root dependency");
        println!();
    }

    // 3) Print chains based on format
    if format == "list" {
        print_why_chains_list(result);
    } else {
        print_why_chains_tree(result);
    }

    // 4) Ambiguous target info (from notes if present)
    for note in &result.notes {
        if note.starts_with("candidates:") {
            println!();
            println!("{note}");
        } else if note.starts_with("Using ") {
            println!("{note}");
        }
    }

    // 5) Print trace if present
    if let Some(ref trace) = result.trace {
        println!();
        println!("Resolver trace:");
        for (i, step) in trace.trace.iter().enumerate() {
            let status = if step.ok { "ok" } else { "fail" };
            println!("  {}. [{}] {} - {}", i + 1, status, step.step, step.detail);
            if let Some(ref path) = step.path {
                println!("       path: {path}");
            }
        }
        for warning in &trace.warnings {
            println!("  warn {}: {}", warning.code, warning.message);
        }
    }

    // 6) Print other notes (excluding candidate-related ones already printed)
    let other_notes: Vec<_> = result
        .notes
        .iter()
        .filter(|n| {
            !n.starts_with("candidates:")
                && !n.starts_with("Using ")
                && !n.starts_with("trace parent")
        })
        .collect();
    if !other_notes.is_empty() {
        println!();
        println!("Notes:");
        for note in other_notes {
            println!("  - {note}");
        }
    }

    // Print trace parent note if present
    for note in &result.notes {
        if note.starts_with("trace parent") {
            println!("  - {note}");
        }
    }

    // 7) Print errors
    if !result.errors.is_empty() {
        println!();
        println!("Errors:");
        for err in &result.errors {
            print!("  [{}] {}", err.code, err.message);
            if let Some(ref path) = err.path {
                print!(" ({path})");
            }
            println!();
        }
    }
}

/// Print chains in list format (one line per chain).
fn print_why_chains_list(result: &PkgWhyResult) {
    if result.chains.is_empty() {
        if result.found_in_node_modules && !result.is_orphan {
            println!(
                "<root> -> {}@{}",
                result.target.name,
                result.target.version.as_deref().unwrap_or("?")
            );
        }
        return;
    }

    for chain in &result.chains {
        let parts: Vec<String> = chain
            .links
            .iter()
            .map(|link| {
                if link.from == "<root>" {
                    format!(
                        "<root> -> {}@{}",
                        link.to,
                        link.resolved_version.as_deref().unwrap_or("?")
                    )
                } else {
                    format!(
                        "{}@{}",
                        link.to,
                        link.resolved_version.as_deref().unwrap_or("?")
                    )
                }
            })
            .collect();

        // Join with " -> " but first item already has the arrow
        if let Some((first, rest)) = parts.split_first() {
            print!("{first}");
            for part in rest {
                print!(" -> {part}");
            }
            println!();
        }
    }
}

/// Print chains in tree format.
fn print_why_chains_tree(result: &PkgWhyResult) {
    if result.chains.is_empty() {
        if result.found_in_node_modules && !result.is_orphan {
            println!("(This is a root-level dependency)");
        }
        return;
    }

    // Primary chain (first one, shortest and deterministic)
    if let Some(primary) = result.chains.first() {
        print_chain_tree(primary);
    }

    // Additional chains
    if result.chains.len() > 1 {
        println!();
        println!("Also reachable via:");
        for chain in result.chains.iter().skip(1) {
            println!();
            print_chain_tree(chain);
        }
    }
}

/// Print a single chain as a tree.
fn print_chain_tree(chain: &PkgWhyChain) {
    for (j, link) in chain.links.iter().enumerate() {
        let indent = "  ".repeat(j + 1);
        let connector = "└── ";

        let resolved_ver = link.resolved_version.as_deref().unwrap_or("?");
        let kind_str = if link.kind != "prod" && link.kind != "dep" {
            format!(" ({})", link.kind)
        } else {
            String::new()
        };

        if j == 0 {
            println!(
                "{}{}{} -> {}@{}{}",
                indent, connector, link.from, link.to, resolved_ver, kind_str
            );
        } else {
            println!(
                "{}{}{}@{}{}",
                indent, connector, link.to, resolved_ver, kind_str
            );
        }

        // Show requirement
        if let Some(ref req) = link.req {
            let req_indent = "  ".repeat(j + 2);
            println!("{req_indent}(requires: {req})");
        }
    }
}

/// Print the doctor report in human-readable format.
fn print_doctor_human(report: &PkgDoctorReport, format: &str) {
    // Header
    println!("Package doctor report");
    println!("=====================");
    println!();

    // Summary
    let severity_str = report.summary.severity.to_uppercase();
    println!("Overall severity: {severity_str}");
    println!(
        "Findings: {} errors, {} warnings, {} info",
        report.summary.counts.error, report.summary.counts.warn, report.summary.counts.info
    );
    println!();
    println!(
        "Packages: {} indexed ({} reachable, {} orphans)",
        report.summary.packages_indexed, report.summary.reachable_packages, report.summary.orphans
    );
    println!(
        "Issues: {} missing deps, {} invalid packages",
        report.summary.missing_edges, report.summary.invalid_packages
    );
    println!();

    if report.findings.is_empty() {
        println!("No issues found.");
        return;
    }

    // Print findings based on format
    match format {
        "list" => print_doctor_findings_list(&report.findings),
        _ => print_doctor_findings_summary(&report.findings),
    }

    // Notes
    if !report.notes.is_empty() {
        println!();
        println!("Notes:");
        for note in &report.notes {
            println!("  - {note}");
        }
    }
}

/// Print findings in summary format (grouped by severity).
fn print_doctor_findings_summary(findings: &[DoctorFinding]) {
    println!("Findings:");

    // Group by severity for display
    let errors: Vec<_> = findings.iter().filter(|f| f.severity == "error").collect();
    let warns: Vec<_> = findings.iter().filter(|f| f.severity == "warn").collect();
    let infos: Vec<_> = findings.iter().filter(|f| f.severity == "info").collect();

    if !errors.is_empty() {
        println!();
        println!("Errors:");
        for finding in errors.iter().take(10) {
            print_finding(finding, "  ");
        }
        if errors.len() > 10 {
            println!("  ... and {} more errors", errors.len() - 10);
        }
    }

    if !warns.is_empty() {
        println!();
        println!("Warnings:");
        for finding in warns.iter().take(10) {
            print_finding(finding, "  ");
        }
        if warns.len() > 10 {
            println!("  ... and {} more warnings", warns.len() - 10);
        }
    }

    if !infos.is_empty() {
        println!();
        println!("Info:");
        for finding in infos.iter().take(5) {
            print_finding(finding, "  ");
        }
        if infos.len() > 5 {
            println!("  ... and {} more info findings", infos.len() - 5);
        }
    }
}

/// Print findings in list format (one per line).
fn print_doctor_findings_list(findings: &[DoctorFinding]) {
    for finding in findings {
        let sev = finding.severity.to_uppercase();
        let pkg_str = finding.package.as_deref().unwrap_or("");
        let path_str = finding
            .path
            .as_deref()
            .map(|p| format!(" at {p}"))
            .unwrap_or_default();

        println!(
            "[{}] {} {} {}{}",
            sev, finding.code, finding.message, pkg_str, path_str
        );
    }
}

/// Print a single finding.
fn print_finding(finding: &DoctorFinding, indent: &str) {
    let sev = match finding.severity.as_str() {
        "error" => "ERROR",
        "warn" => "WARN ",
        _ => "INFO ",
    };

    print!("{}{} {}: {}", indent, sev, finding.code, finding.message);
    if let Some(ref pkg) = finding.package {
        print!(" ({pkg})");
    }
    println!();

    if let Some(ref path) = finding.path {
        println!("{indent}      at: {path}");
    }
    if let Some(ref detail) = finding.detail {
        println!("{indent}      {detail}");
    }
}

async fn send_pkg_request(
    endpoint: &str,
    action: &PkgAction,
    channel: Channel,
) -> io::Result<(Response, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Connect using cross-platform IpcStream
    let mut stream = IpcStream::connect(endpoint).await?;

    // Create request based on action
    let request = match action {
        PkgAction::Add { specs, cwd, save_dev } => Request::PkgAdd {
            specs: specs.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            save_dev: *save_dev,
        },
        PkgAction::AddDeps { .. } => {
            // AddDeps is converted to Add before reaching this function
            unreachable!("AddDeps should be converted to Add before sending request")
        }
        PkgAction::Remove { packages, cwd } => Request::PkgRemove {
            packages: packages.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
        },
        PkgAction::Update { packages, cwd, latest } => Request::PkgUpdate {
            packages: packages.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            latest: *latest,
        },
        PkgAction::CacheList => Request::PkgCacheList {
            channel: channel.as_str().to_string(),
        },
        PkgAction::CachePrune => Request::PkgCachePrune {
            channel: channel.as_str().to_string(),
        },
        PkgAction::Graph {
            cwd,
            include_dev,
            include_optional,
            max_depth,
            format,
        } => Request::PkgGraph {
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            include_dev_root: *include_dev,
            include_optional: *include_optional,
            max_depth: *max_depth,
            format: format.clone(),
        },
        PkgAction::Explain {
            specifier,
            cwd,
            parent,
            kind,
        } => Request::PkgExplain {
            specifier: specifier.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            parent: parent.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            kind: kind.clone(),
        },
        PkgAction::Why {
            arg,
            cwd,
            include_dev,
            include_optional,
            max_depth,
            max_chains,
            format,
            include_trace,
            trace_kind,
            trace_parent,
        } => Request::PkgWhy {
            arg: arg.clone(),
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            include_dev_root: *include_dev,
            include_optional: *include_optional,
            max_depth: *max_depth,
            max_chains: *max_chains,
            format: format.clone(),
            include_trace: *include_trace,
            trace_kind: trace_kind.clone(),
            trace_parent: trace_parent
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        },
        PkgAction::Doctor {
            cwd,
            include_dev,
            include_optional,
            max_depth,
            max_items,
            min_severity,
            format,
        } => Request::PkgDoctor {
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            include_dev_root: *include_dev,
            include_optional: *include_optional,
            max_depth: *max_depth,
            max_items: *max_items,
            min_severity: min_severity.clone(),
            format: format.clone(),
        },
        PkgAction::Install {
            cwd,
            frozen,
            include_dev,
            include_optional,
        } => Request::PkgInstall {
            cwd: cwd.to_string_lossy().into_owned(),
            channel: channel.as_str().to_string(),
            frozen: *frozen,
            include_dev: *include_dev,
            include_optional: *include_optional,
        },
    };

    // Create and send request frame
    let frame = Frame::new(VERSION, request);
    let encoded = encode_frame(&frame)?;

    stream.write_all(&encoded).await?;
    stream.flush().await?;

    // Read response
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("response frame too large: {len} bytes"),
        ));
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let response: FrameResponse =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((response.response, response.hello.server_version))
}
