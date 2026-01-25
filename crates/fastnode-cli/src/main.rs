#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
// Allow unnecessary_wraps for now - commands are placeholders that will return errors when implemented
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(dead_code)]
#![allow(clippy::cast_precision_loss)]

mod commands;
mod logging;

use clap::Parser;
use fastnode_core::config::Channel;
use fastnode_core::Config;
use miette::Result;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "howth")]
#[command(author, version, about = "A deterministic Node toolchain inspector", long_about = None)]
struct Cli {
    /// Increase logging verbosity (-v for DEBUG, -vv for TRACE)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Emit JSON formatted output (stable, machine-readable)
    #[arg(long, global = true)]
    json: bool,

    /// Override the working directory
    #[arg(long, global = true, value_name = "PATH")]
    cwd: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Print version information
    Version,

    /// Check system health and capabilities
    Doctor,

    /// Run micro-benchmarks
    Bench {
        #[command(subcommand)]
        bench_cmd: BenchCommands,
    },

    /// Start the daemon (foreground)
    Daemon,

    /// Ping the daemon to check if it's running
    Ping,

    /// Run a JavaScript/TypeScript file
    Run {
        /// The file to run
        entry: PathBuf,

        /// Route through the daemon instead of local execution
        #[arg(long)]
        daemon: bool,

        /// Arguments to pass to the script (after --)
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Install dependencies from lockfile
    Install {
        /// Fail if lockfile is missing or out of date
        #[arg(long)]
        frozen_lockfile: bool,

        /// Include devDependencies
        #[arg(long, default_value_t = true)]
        dev: bool,

        /// Skip devDependencies
        #[arg(long, conflicts_with = "dev")]
        no_dev: bool,

        /// Include optionalDependencies
        #[arg(long, default_value_t = true)]
        optional: bool,

        /// Skip optionalDependencies
        #[arg(long, conflicts_with = "optional")]
        no_optional: bool,
    },

    /// Build the project
    Build {
        /// Force rebuild (bypass cache)
        #[arg(long)]
        force: bool,

        /// Dry run (plan only, don't execute)
        #[arg(long)]
        dry_run: bool,

        /// Maximum parallel jobs
        #[arg(long)]
        max_parallel: Option<u32>,

        /// Include profiling information
        #[arg(long)]
        profile: bool,

        /// Show why each node was rebuilt or skipped (v2.3)
        #[arg(long)]
        why: bool,

        /// Watch for file changes and rebuild (v3.0)
        #[arg(long)]
        watch: bool,

        /// Debounce delay in milliseconds for watch mode (default 100ms)
        #[arg(long, default_value = "100")]
        debounce_ms: u32,

        /// Targets to build (comma-separated, e.g., "build,test" or "script:build")
        /// If not specified, uses default targets from the graph.
        #[arg(value_delimiter = ',')]
        targets: Vec<String>,
    },

    /// Run tests
    Test,

    /// Control the file watcher
    Watch {
        #[command(subcommand)]
        watch_cmd: WatchCommands,
    },

    /// Package management
    Pkg {
        #[command(subcommand)]
        pkg_cmd: PkgCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
enum BenchCommands {
    /// Run smoke benchmarks (internal hot-path operations)
    Smoke {
        /// Number of measured iterations
        #[arg(long, default_value_t = commands::bench::smoke::DEFAULT_ITERS)]
        iters: u32,

        /// Number of warmup iterations (not measured)
        #[arg(long, default_value_t = commands::bench::smoke::DEFAULT_WARMUP)]
        warmup: u32,

        /// Payload size in MiB for file operations (clamped to 1-256)
        #[arg(long, default_value_t = commands::bench::smoke::DEFAULT_SIZE_MIB)]
        size: u32,
    },
}

#[derive(clap::Subcommand, Debug)]
enum WatchCommands {
    /// Start watching directories
    Start {
        /// Directories to watch (defaults to current directory)
        #[arg(default_value = ".")]
        roots: Vec<PathBuf>,
    },

    /// Stop the file watcher
    Stop,

    /// Show watcher status
    Status,
}

#[derive(clap::Subcommand, Debug)]
enum PkgCommands {
    /// Add packages to the project
    Add {
        /// Package specs (e.g., "react", "lodash@^4.17.0", "@types/node")
        specs: Vec<String>,

        /// Install dependencies from package.json instead of explicit specs
        #[arg(long, conflicts_with = "specs")]
        deps: bool,

        /// Include devDependencies (only with --deps)
        #[arg(long, requires = "deps")]
        dev: bool,

        /// Include optionalDependencies (only with --deps)
        #[arg(long, requires = "deps")]
        optional: bool,
    },

    /// Show the dependency graph
    Graph {
        /// Include devDependencies from root package.json
        #[arg(long)]
        dev: bool,

        /// Exclude optionalDependencies
        #[arg(long)]
        no_optional: bool,

        /// Maximum traversal depth
        #[arg(long, default_value = "25")]
        max_depth: u32,

        /// Output format: "tree" or "list"
        #[arg(long, default_value = "tree")]
        format: String,
    },

    /// Manage the package cache
    Cache {
        #[command(subcommand)]
        cache_cmd: PkgCacheCommands,
    },

    /// Explain why a specifier resolves to a file
    Explain {
        /// The specifier to explain (e.g., "lodash", "./utils", "#internal")
        specifier: String,

        /// Resolution kind: "import", "require", or "auto" (default)
        #[arg(long, default_value = "auto")]
        kind: String,

        /// Directory of the importing file (defaults to cwd)
        #[arg(long)]
        parent: Option<PathBuf>,

        /// Show dependency chain instead of resolution path
        #[arg(long)]
        why: bool,

        /// Include devDependencies in dependency analysis (--why only)
        #[arg(long, requires = "why")]
        dev: bool,

        /// Exclude optionalDependencies (--why only)
        #[arg(long, requires = "why")]
        no_optional: bool,

        /// Maximum traversal depth (--why only)
        #[arg(long, default_value = "50", requires = "why")]
        max_depth: u32,

        /// Maximum number of chains to return (--why only, 1-50)
        #[arg(long, default_value = "5", requires = "why", value_parser = clap::value_parser!(u32).range(1..=50))]
        max_chains: u32,

        /// Output format: "tree" or "list" (--why only)
        #[arg(long, default_value = "tree", requires = "why", value_parser = ["tree", "list"])]
        format: String,

        /// Include resolver trace for subpath resolution (--why only)
        #[arg(long, requires = "why")]
        trace: bool,
    },

    /// Run package health diagnostics
    Doctor {
        /// Include devDependencies in analysis
        #[arg(long)]
        dev: bool,

        /// Exclude optionalDependencies
        #[arg(long)]
        no_optional: bool,

        /// Maximum traversal depth
        #[arg(long, default_value = "25", value_parser = clap::value_parser!(u32).range(1..=200))]
        max_depth: u32,

        /// Maximum number of findings to return
        #[arg(long, default_value = "200", value_parser = clap::value_parser!(u32).range(1..=2000))]
        max_items: u32,

        /// Minimum severity to include: "info", "warn", or "error"
        #[arg(long, default_value = "info", value_parser = ["info", "warn", "error"])]
        severity: String,

        /// Output format: "summary" or "list"
        #[arg(long, default_value = "summary", value_parser = ["summary", "list"])]
        format: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum PkgCacheCommands {
    /// List cached packages
    Ls,

    /// Remove unused cached packages
    Prune,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Determine working directory
    let cwd = cli
        .cwd
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    // Build config
    let config = Config::new(cwd.clone())
        .with_verbosity(cli.verbose)
        .with_json_logs(cli.json);

    // Commands that handle their own output (JSON to stdout, no logging)
    if matches!(cli.command, Some(Commands::Doctor)) {
        return commands::doctor::run(&cwd, Channel::Stable, cli.json);
    }

    if let Some(Commands::Bench { bench_cmd }) = &cli.command {
        return match bench_cmd {
            BenchCommands::Smoke {
                iters,
                warmup,
                size,
            } => commands::bench::smoke::run(*iters, *warmup, *size, cli.json),
        };
    }

    if matches!(cli.command, Some(Commands::Daemon)) {
        return commands::daemon::run(Channel::Stable, cli.json);
    }

    if matches!(cli.command, Some(Commands::Ping)) {
        return commands::ping::run(Channel::Stable, cli.json);
    }

    if let Some(Commands::Run {
        entry,
        daemon,
        args,
    }) = &cli.command
    {
        return commands::run::run(&cwd, entry, args, *daemon, Channel::Stable, cli.json);
    }

    if let Some(Commands::Watch { watch_cmd }) = &cli.command {
        let action = match watch_cmd {
            WatchCommands::Start { roots } => {
                // Convert relative paths to absolute
                let absolute_roots: Vec<PathBuf> = roots
                    .iter()
                    .map(|p| {
                        if p.is_absolute() {
                            p.clone()
                        } else {
                            cwd.join(p)
                        }
                    })
                    .collect();
                commands::watch::WatchAction::Start {
                    roots: absolute_roots,
                }
            }
            WatchCommands::Stop => commands::watch::WatchAction::Stop,
            WatchCommands::Status => commands::watch::WatchAction::Status,
        };
        return commands::watch::run(action, Channel::Stable, cli.json);
    }

    if let Some(Commands::Pkg { pkg_cmd }) = &cli.command {
        let action = match pkg_cmd {
            PkgCommands::Add {
                specs,
                deps,
                dev,
                optional,
            } => {
                if *deps {
                    commands::pkg::PkgAction::AddDeps {
                        cwd: cwd.clone(),
                        include_dev: *dev,
                        include_optional: *optional,
                    }
                } else if specs.is_empty() {
                    // No specs and no --deps: error
                    eprintln!("error: either provide package specs or use --deps");
                    std::process::exit(2);
                } else {
                    commands::pkg::PkgAction::Add {
                        specs: specs.clone(),
                        cwd: cwd.clone(),
                    }
                }
            }
            PkgCommands::Graph {
                dev,
                no_optional,
                max_depth,
                format,
            } => commands::pkg::PkgAction::Graph {
                cwd: cwd.clone(),
                include_dev: *dev,
                include_optional: !*no_optional,
                max_depth: *max_depth,
                format: format.clone(),
            },
            PkgCommands::Cache { cache_cmd } => match cache_cmd {
                PkgCacheCommands::Ls => commands::pkg::PkgAction::CacheList,
                PkgCacheCommands::Prune => commands::pkg::PkgAction::CachePrune,
            },
            PkgCommands::Explain {
                specifier,
                kind,
                parent,
                why,
                dev,
                no_optional,
                max_depth,
                max_chains,
                format,
                trace,
            } => {
                if *why {
                    commands::pkg::PkgAction::Why {
                        arg: specifier.clone(),
                        cwd: cwd.clone(),
                        include_dev: *dev,
                        include_optional: !*no_optional,
                        max_depth: *max_depth,
                        max_chains: *max_chains,
                        format: format.clone(),
                        include_trace: *trace,
                        trace_kind: Some(kind.clone()),
                        trace_parent: parent.clone(),
                    }
                } else {
                    commands::pkg::PkgAction::Explain {
                        specifier: specifier.clone(),
                        cwd: cwd.clone(),
                        parent: parent.clone().unwrap_or_else(|| cwd.clone()),
                        kind: kind.clone(),
                    }
                }
            }
            PkgCommands::Doctor {
                dev,
                no_optional,
                max_depth,
                max_items,
                severity,
                format,
            } => commands::pkg::PkgAction::Doctor {
                cwd: cwd.clone(),
                include_dev: *dev,
                include_optional: !*no_optional,
                max_depth: *max_depth,
                max_items: *max_items,
                min_severity: severity.clone(),
                format: format.clone(),
            },
        };
        return commands::pkg::run(action, Channel::Stable, cli.json);
    }

    if let Some(Commands::Install {
        frozen_lockfile,
        dev,
        no_dev,
        optional,
        no_optional,
    }) = &cli.command
    {
        let action = commands::pkg::PkgAction::Install {
            cwd: cwd.clone(),
            frozen: *frozen_lockfile,
            include_dev: *dev && !*no_dev,
            include_optional: *optional && !*no_optional,
        };
        return commands::pkg::run(action, Channel::Stable, cli.json);
    }

    // Handle build command early (like other daemon commands)
    if let Some(Commands::Build {
        force,
        dry_run,
        max_parallel,
        profile,
        why,
        watch,
        debounce_ms,
        targets,
    }) = &cli.command
    {
        // v3.0: --watch --json is disallowed (violates "one JSON object" contract)
        if *watch && cli.json {
            eprintln!("error: --watch and --json cannot be combined");
            eprintln!("hint: --json requires exactly one output object; watch mode streams multiple results");
            std::process::exit(2);
        }

        let action = commands::build::BuildAction {
            cwd: cwd.clone(),
            force: *force,
            dry_run: *dry_run,
            max_parallel: *max_parallel,
            profile: *profile,
            why: *why,
            watch: *watch,
            debounce_ms: *debounce_ms,
            targets: targets.clone(),
        };
        return commands::build::run(action, Channel::Stable, cli.json);
    }

    // Initialize logging for other commands
    logging::init(config.verbosity, config.json_logs);

    // Dispatch to command
    match cli.command {
        Some(Commands::Version) | None => commands::version::run(),
        Some(
            Commands::Doctor
            | Commands::Bench { .. }
            | Commands::Daemon
            | Commands::Ping
            | Commands::Run { .. }
            | Commands::Watch { .. }
            | Commands::Pkg { .. }
            | Commands::Install { .. }
            | Commands::Build { .. },
        ) => {
            unreachable!() // Handled above
        }
        Some(Commands::Test) => {
            let span = tracing::info_span!("test", cmd = "test", cwd = %cwd.display());
            let _guard = span.enter();
            commands::test::run(&config)
        }
    }
}
