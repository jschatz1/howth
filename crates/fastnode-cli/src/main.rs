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
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::doc_markdown)]

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

    /// Initialize a new project
    Init {
        /// Accept all defaults without prompting
        #[arg(short, long)]
        yes: bool,
    },

    /// Create a new project from a template
    Create {
        /// Template to use (e.g., "react", "next", "vite", or a GitHub repo like "user/repo")
        template: String,

        /// Project name / directory name
        name: Option<String>,
    },

    /// Register or link a local package
    Link {
        /// Package name to link (omit to register current package)
        package: Option<String>,

        /// Add to package.json dependencies with link: specifier
        #[arg(long)]
        save: bool,

        /// List all registered packages
        #[arg(long)]
        list: bool,
    },

    /// Unregister or unlink a local package
    Unlink {
        /// Package name to unlink (omit to unregister current package)
        package: Option<String>,
    },

    /// Run micro-benchmarks
    Bench {
        #[command(subcommand)]
        bench_cmd: BenchCommands,
    },

    /// Start the daemon (foreground)
    Daemon,

    /// Stop the running daemon
    Stop,

    /// Ping the daemon to check if it's running
    Ping,

    /// Run a JavaScript/TypeScript file or package.json script
    Run {
        /// The file or package.json script to run
        entry: String,

        /// Route through the daemon instead of local execution
        #[arg(long)]
        daemon: bool,

        /// Only show the execution plan without running
        #[arg(long)]
        dry_run: bool,

        /// Use native V8 runtime (default when native-runtime feature is enabled)
        #[arg(long)]
        native: bool,

        /// Force Node.js subprocess instead of native runtime
        #[arg(long)]
        node: bool,

        /// Arguments to pass to the script (after --)
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Execute a binary from node_modules/.bin or PATH
    Exec {
        /// Binary name to execute (e.g., "jest", "eslint", "tsc")
        binary: String,

        /// Arguments to pass to the binary (after --)
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

    /// Bundle JavaScript/TypeScript modules
    Bundle {
        /// Entry point file
        entry: PathBuf,

        /// Output file (if not specified, prints to stdout)
        #[arg(long, short = 'o')]
        outfile: Option<PathBuf>,

        /// Output format: esm, cjs, or iife
        #[arg(long, default_value = "esm")]
        format: String,

        /// Minify output
        #[arg(long)]
        minify: bool,

        /// Generate source maps
        #[arg(long)]
        sourcemap: bool,

        /// External packages (don't bundle, keep as imports)
        #[arg(long, value_delimiter = ',')]
        external: Vec<String>,

        /// Enable tree shaking (dead code elimination) - enabled by default
        #[arg(long, default_value_t = true)]
        treeshake: bool,

        /// Disable tree shaking
        #[arg(long, conflicts_with = "treeshake")]
        no_treeshake: bool,

        /// Enable code splitting for dynamic imports
        #[arg(long)]
        splitting: bool,

        /// Define global replacements (e.g., --define __DEV__=false)
        #[arg(long, value_delimiter = ',')]
        define: Vec<String>,

        /// Import path aliases (e.g., --alias @=./src)
        #[arg(long = "alias", value_delimiter = ',')]
        aliases: Vec<String>,

        /// Banner text to prepend to output
        #[arg(long)]
        banner: Option<String>,
    },

    /// Start development server with HMR, or run the "dev" script from package.json
    Dev {
        /// Entry point file (if omitted, runs the "dev" script from package.json)
        entry: Option<PathBuf>,

        /// Port to listen on
        #[arg(long, short = 'p', default_value = "3000")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "localhost")]
        host: String,

        /// Open browser automatically
        #[arg(long)]
        open: bool,

        /// Path to config file (overrides auto-discovery)
        #[arg(long, short = 'c', value_name = "FILE")]
        config: Option<PathBuf>,

        /// Mode (e.g. "development", "production") — controls which .env files are loaded
        #[arg(long, short = 'm', default_value = "development")]
        mode: String,
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
        ///
        /// In watch mode, defaults to transpile-only for fast feedback.
        /// Add targets to include more (e.g., `--watch typecheck`).
        #[arg(long)]
        watch: bool,

        /// Debounce delay in milliseconds for watch mode (default 100ms)
        #[arg(long, default_value = "100")]
        debounce_ms: u32,

        /// Targets to build (e.g., "typecheck" or "transpile,typecheck")
        ///
        /// Without --watch: empty means all targets.
        /// With --watch: empty means transpile-only; add targets to include more.
        #[arg(value_delimiter = ',')]
        targets: Vec<String>,
    },

    /// Run tests
    Test {
        /// Paths to test files or directories (default: discover in cwd)
        paths: Vec<String>,
    },

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

    /// List and manage workspace packages
    Workspaces {
        /// Link all workspace packages into node_modules
        #[arg(long)]
        link: bool,
    },

    /// Run a package.json script directly (e.g., `howth test` instead of `howth run test`)
    #[command(external_subcommand)]
    Script(Vec<String>),
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

    /// Benchmark transpile performance
    Transpile {
        /// Number of measured iterations
        #[arg(long, default_value_t = commands::bench::build::DEFAULT_ITERS)]
        iters: u32,

        /// Number of warmup iterations (not measured)
        #[arg(long, default_value_t = commands::bench::build::DEFAULT_WARMUP)]
        warmup: u32,

        /// Project directory to benchmark (uses temp project if not specified)
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Benchmark full dev loop performance
    Devloop {
        /// Number of measured iterations
        #[arg(long, default_value_t = commands::bench::build::DEFAULT_ITERS)]
        iters: u32,

        /// Number of warmup iterations (not measured)
        #[arg(long, default_value_t = commands::bench::build::DEFAULT_WARMUP)]
        warmup: u32,

        /// Project directory to benchmark (uses temp project if not specified)
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Benchmark package install speed (howth vs npm vs bun)
    Install {
        /// Number of measured iterations
        #[arg(long, default_value_t = commands::bench::install::DEFAULT_ITERS)]
        iters: u32,

        /// Number of warmup iterations
        #[arg(long, default_value_t = commands::bench::install::DEFAULT_WARMUP)]
        warmup: u32,

        /// Project directory to benchmark (uses temp project if not specified)
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Benchmark test execution speed (howth vs node vs bun)
    #[command(name = "test")]
    TestRun {
        /// Number of measured iterations
        #[arg(long, default_value_t = commands::bench::test::DEFAULT_ITERS)]
        iters: u32,

        /// Number of warmup iterations
        #[arg(long, default_value_t = commands::bench::test::DEFAULT_WARMUP)]
        warmup: u32,
    },

    /// Benchmark HTTP server throughput (howth vs node vs bun vs deno)
    Http {
        /// Duration of the benchmark in seconds
        #[arg(long, default_value_t = commands::bench::http::DEFAULT_DURATION_SECS)]
        duration: u32,

        /// Number of concurrent connections
        #[arg(long, default_value_t = commands::bench::http::DEFAULT_CONNECTIONS)]
        connections: u32,

        /// Warmup duration in seconds
        #[arg(long, default_value_t = commands::bench::http::DEFAULT_WARMUP_SECS)]
        warmup: u32,
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

        /// Save as devDependency (-D is shorthand for --save-dev)
        #[arg(short = 'D', long = "save-dev", conflicts_with = "deps")]
        save_dev: bool,
    },

    /// Remove packages from the project
    Remove {
        /// Package names to remove (e.g., "react", "lodash")
        packages: Vec<String>,
    },

    /// Update packages to latest versions
    Update {
        /// Specific packages to update (empty = all dependencies)
        packages: Vec<String>,

        /// Update to latest version, ignoring semver ranges
        #[arg(long)]
        latest: bool,
    },

    /// Show outdated packages
    Outdated,

    /// Publish package to npm registry
    Publish {
        /// Dry run (don't actually publish)
        #[arg(long)]
        dry_run: bool,

        /// npm tag (defaults to "latest")
        #[arg(long, default_value = "latest")]
        tag: String,

        /// Access level for scoped packages: "public" or "restricted"
        #[arg(long)]
        access: Option<String>,

        /// Custom registry URL
        #[arg(long)]
        registry: Option<String>,
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

    if let Some(Commands::Init { yes }) = &cli.command {
        return commands::init::run(&cwd, *yes, cli.json);
    }

    if let Some(Commands::Create { template, name }) = &cli.command {
        return commands::create::run(&cwd, template, name.as_deref(), cli.json);
    }

    if let Some(Commands::Link {
        package,
        save,
        list,
    }) = &cli.command
    {
        if *list {
            return commands::link::list(Channel::Stable, cli.json);
        }
        return commands::link::link(&cwd, package.as_deref(), *save, Channel::Stable, cli.json);
    }

    if let Some(Commands::Unlink { package }) = &cli.command {
        return commands::link::unlink(&cwd, package.as_deref(), Channel::Stable, cli.json);
    }

    if let Some(Commands::Workspaces { link }) = &cli.command {
        if *link {
            return commands::workspaces::link(&cwd, cli.json);
        }
        return commands::workspaces::run(&cwd, cli.json);
    }

    // Handle script shortcuts (e.g., `howth test` instead of `howth run test`)
    if let Some(Commands::Script(args)) = &cli.command {
        if let Some(script_name) = args.first() {
            // Pass remaining args to the script
            let script_args: Vec<String> = args.iter().skip(1).cloned().collect();
            return commands::run::run(
                &cwd,
                script_name,
                &script_args,
                false, // daemon
                false, // dry_run
                false, // native
                false, // node
                Channel::Stable,
                cli.json,
            );
        }
    }

    if let Some(Commands::Bench { bench_cmd }) = &cli.command {
        return match bench_cmd {
            BenchCommands::Smoke {
                iters,
                warmup,
                size,
            } => commands::bench::smoke::run(*iters, *warmup, *size, cli.json),
            BenchCommands::Transpile {
                iters,
                warmup,
                project,
            } => commands::bench::build::run_transpile(*iters, *warmup, project.clone(), cli.json),
            BenchCommands::Devloop {
                iters,
                warmup,
                project,
            } => commands::bench::build::run_devloop(*iters, *warmup, project.clone(), cli.json),
            BenchCommands::Install {
                iters,
                warmup,
                project,
            } => commands::bench::install::run(*iters, *warmup, project.clone(), cli.json),
            BenchCommands::TestRun {
                iters,
                warmup,
            } => commands::bench::test::run(*iters, *warmup, cli.json),
            BenchCommands::Http {
                duration,
                connections,
                warmup,
            } => commands::bench::http::run(*duration, *connections, *warmup, cli.json),
        };
    }

    if matches!(cli.command, Some(Commands::Daemon)) {
        return commands::daemon::run(Channel::Stable, cli.json);
    }

    if matches!(cli.command, Some(Commands::Stop)) {
        return commands::stop::run(Channel::Stable, cli.json);
    }

    if matches!(cli.command, Some(Commands::Ping)) {
        return commands::ping::run(Channel::Stable, cli.json);
    }

    if let Some(Commands::Run {
        entry,
        daemon,
        dry_run,
        native,
        node,
        args,
    }) = &cli.command
    {
        return commands::run::run(
            &cwd,
            entry,
            args,
            *daemon,
            *dry_run,
            *native,
            *node,
            Channel::Stable,
            cli.json,
        );
    }

    if let Some(Commands::Exec { binary, args }) = &cli.command {
        return commands::exec::run(&cwd, binary, args, cli.json);
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
                save_dev,
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
                        save_dev: *save_dev,
                    }
                }
            }
            PkgCommands::Remove { packages } => {
                if packages.is_empty() {
                    eprintln!("error: specify at least one package to remove");
                    std::process::exit(2);
                }
                commands::pkg::PkgAction::Remove {
                    packages: packages.clone(),
                    cwd: cwd.clone(),
                }
            }
            PkgCommands::Update { packages, latest } => {
                commands::pkg::PkgAction::Update {
                    packages: packages.clone(),
                    cwd: cwd.clone(),
                    latest: *latest,
                }
            }
            PkgCommands::Outdated => commands::pkg::PkgAction::Outdated { cwd: cwd.clone() },
            PkgCommands::Publish {
                dry_run,
                tag,
                access,
                registry,
            } => commands::pkg::PkgAction::Publish {
                cwd: cwd.clone(),
                dry_run: *dry_run,
                tag: tag.clone(),
                access: access.clone(),
                registry: registry.clone(),
            },
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

    // Handle bundle command
    if let Some(Commands::Bundle {
        entry,
        outfile,
        format,
        minify,
        sourcemap,
        external,
        treeshake,
        no_treeshake,
        splitting,
        define,
        aliases,
        banner,
    }) = &cli.command
    {
        let bundle_format = commands::bundle::parse_format(format).unwrap_or_else(|| {
            eprintln!("error: invalid format '{}'. Use: esm, cjs, or iife", format);
            std::process::exit(2);
        });

        let action = commands::bundle::BundleAction {
            entry: entry.clone(),
            cwd: cwd.clone(),
            outfile: outfile.clone(),
            format: bundle_format,
            minify: *minify,
            sourcemap: *sourcemap,
            external: external.clone(),
            treeshake: *treeshake && !*no_treeshake,
            splitting: *splitting,
            define: define.clone(),
            alias: aliases.clone(),
            banner: banner.clone(),
        };
        return commands::bundle::run(action, cli.json);
    }

    // Handle dev command
    if let Some(Commands::Dev {
        entry,
        port,
        host,
        open,
        config,
        mode,
    }) = &cli.command
    {
        match entry {
            Some(entry) => {
                // Explicit entry file: start the built-in dev server
                // Use the global --cwd (or current directory) as project root,
                // not the entry file's parent, so config discovery works correctly.
                let action = commands::dev::DevAction {
                    entry: entry.clone(),
                    cwd: cwd.clone(),
                    port: *port,
                    host: host.clone(),
                    open: *open,
                    config: config.clone(),
                    mode: mode.clone(),
                };

                let rt = tokio::runtime::Runtime::new().unwrap();
                return rt.block_on(commands::dev::run(action));
            }
            None => {
                // No entry file: run the "dev" script from package.json (like pnpm dev)
                return commands::run::run(
                    &cwd,
                    "dev",
                    &[],
                    false, // daemon
                    false, // dry_run
                    false, // native
                    false, // node
                    Channel::Stable,
                    cli.json,
                );
            }
        }
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

        // v3.4: Watch mode defaults to transpile-only for fast feedback (Bun parity)
        // - `howth build --watch` → transpile only
        // - `howth build --watch typecheck` → transpile + typecheck
        // Non-watch mode uses all targets by default (empty = all)
        let effective_targets = if *watch {
            if targets.is_empty() {
                // Watch mode default: transpile only for fast feedback
                vec!["transpile".to_string()]
            } else {
                // Watch mode with explicit targets: always include transpile + specified targets
                let mut t = vec!["transpile".to_string()];
                for target in targets {
                    if target != "transpile" && !t.contains(target) {
                        t.push(target.clone());
                    }
                }
                t
            }
        } else {
            targets.clone()
        };

        let action = commands::build::BuildAction {
            cwd: cwd.clone(),
            force: *force,
            dry_run: *dry_run,
            max_parallel: *max_parallel,
            profile: *profile,
            why: *why,
            watch: *watch,
            debounce_ms: *debounce_ms,
            targets: effective_targets,
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
            | Commands::Bundle { .. }
            | Commands::Create { .. }
            | Commands::Daemon
            | Commands::Stop
            | Commands::Dev { .. }
            | Commands::Init { .. }
            | Commands::Link { .. }
            | Commands::Unlink { .. }
            | Commands::Ping
            | Commands::Run { .. }
            | Commands::Exec { .. }
            | Commands::Script(_)
            | Commands::Watch { .. }
            | Commands::Workspaces { .. }
            | Commands::Pkg { .. }
            | Commands::Install { .. }
            | Commands::Build { .. },
        ) => {
            unreachable!() // Handled above
        }
        Some(Commands::Test { paths }) => {
            let span = tracing::info_span!("test", cmd = "test", cwd = %cwd.display());
            let _guard = span.enter();
            commands::test::run(&config, &paths)
        }
    }
}
