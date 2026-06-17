//! rBuilder CLI
//!
//! Command-line interface for the rBuilder knowledge graph system.

use clap::{Parser, Subcommand};
use rbuilder::BUILD_INFO;

#[derive(Parser)]
#[command(name = "rbuilder")]
#[command(about = "AI-powered code knowledge graph", long_about = None)]
#[command(version = BUILD_INFO)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize graph for a repository
    Init {
        /// Path to repository (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Repository namespace for multi-repo workspaces
        #[arg(long)]
        namespace: Option<String>,

        /// Languages to include (comma-separated)
        #[arg(short, long)]
        languages: Option<String>,

        /// Exclude patterns (comma-separated)
        #[arg(short, long)]
        exclude: Option<String>,
    },

    /// Update graph incrementally
    Update {
        /// Update since git commit
        #[arg(long)]
        since: Option<String>,

        /// Force full rebuild
        #[arg(long)]
        force: bool,

        /// Update only these repo-relative files (repeatable)
        #[arg(long)]
        files: Vec<String>,
    },

    /// Watch repository and re-index on file changes (Phase 13.1)
    Watch {
        /// Path to repository (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Debounce window in milliseconds
        #[arg(long)]
        debounce_ms: Option<u64>,
    },

    /// Install git hooks for pre-commit, post-commit, and post-checkout (Phase 13.2–13.3)
    InitHooks {
        /// Path to repository (default: current directory)
        #[arg(default_value = ".")]
        path: String,

        /// Overwrite existing hook scripts
        #[arg(long)]
        force: bool,
    },

    /// Analyze blast-radius risk for changed files (Phase 13.2)
    DetectChanges {
        /// Repo-relative file paths (default: git staged files)
        files: Vec<String>,

        /// Emit JSON for hook scripts
        #[arg(long)]
        json: bool,
    },

    /// Run analysis on the graph
    Analyze {
        /// Run community detection
        #[arg(long)]
        community: bool,

        /// Calculate complexity metrics
        #[arg(long)]
        complexity: bool,

        /// Compute centrality scores
        #[arg(long)]
        centrality: bool,

        /// Run all analyses
        #[arg(long)]
        all: bool,
    },

    /// Query the graph using natural language
    Ask {
        /// Natural language question
        question: String,

        /// Show the translated query
        #[arg(long)]
        explain: bool,

        /// Use dual-agent query decomposition (Phase 12.3)
        #[arg(long)]
        dual_agent: bool,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    /// Backward program slice for a variable at a source line (Phase 12.1)
    Slice {
        /// Source file path
        file: String,

        /// Line number (1-based)
        #[arg(long)]
        line: usize,

        /// Variable of interest
        #[arg(long)]
        variable: String,

        /// Function name (inferred from file when omitted)
        #[arg(long)]
        function: Option<String>,

        /// Language override (rust, python)
        #[arg(long)]
        language: Option<String>,
    },

    /// Execute graph query language (GQL) against the indexed graph (Phase 12.4)
    Gql {
        /// GQL query string
        query: String,

        /// Show execution plan
        #[arg(long)]
        explain: bool,

        /// Named query macro (e.g. all_functions, direct_calls)
        #[arg(long)]
        macro_name: Option<String>,
    },

    /// PDG-enhanced blast radius for a symbol (Phase 12.2)
    BlastRadius {
        /// Symbol name
        symbol: String,

        /// Maximum transitive caller depth
        #[arg(long, default_value = "10")]
        depth: usize,
    },

    /// Interactive conversational mode
    Chat,

    /// Apply labeling rules
    Label {
        /// Path to ruleset file
        #[arg(long)]
        ruleset: String,

        /// Dry run (show what would be labeled)
        #[arg(long)]
        dry_run: bool,
    },

    /// Generate IDL files
    Idl {
        /// IDL format (proto, thrift, openapi)
        #[arg(long)]
        format: String,

        /// Module name
        #[arg(long)]
        module: Option<String>,

        /// Output directory
        #[arg(long)]
        output_dir: Option<String>,
    },

    /// Configuration analysis
    Config {
        /// Find unused config keys
        #[arg(long)]
        unused: bool,

        /// Find missing environment variables
        #[arg(long)]
        missing_env: bool,

        /// Find hardcoded secrets
        #[arg(long)]
        secrets: bool,

        /// Compare configs for drift
        #[arg(long)]
        drift: Option<Vec<String>>,
    },

    /// Plugin management
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Export graph
    Export {
        /// Export format (json, graphml, cypher)
        #[arg(long)]
        format: String,

        /// Output file
        #[arg(long)]
        output: String,

        /// Query DSL when exporting graphml (default: all)
        #[arg(long, default_value = "all")]
        query: String,
    },

    /// Generate a diagram from a graph query (Phase 14)
    Diagram {
        /// Graph query (e.g. `type:Function`, `functions`)
        query: String,

        /// Output format: mermaid, dot, graphml, png, svg, pdf
        #[arg(long, default_value = "mermaid")]
        format: String,

        /// Mermaid diagram type: flowchart, class, call-graph
        #[arg(long, default_value = "flowchart")]
        diagram_type: String,

        /// Output file (stdout if omitted for text formats)
        #[arg(short, long)]
        output: Option<String>,

        /// Expand call neighborhood depth
        #[arg(long)]
        depth: Option<usize>,

        /// Graphviz layout: dot, neato, fdp, circo
        #[arg(long, default_value = "dot")]
        layout: String,

        /// Rank direction: TB or LR
        #[arg(long, default_value = "TB")]
        rankdir: String,
    },

    /// Start web server for graph visualization
    #[cfg(feature = "mcp-server")]
    Serve {
        /// Port number
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },

    /// Start web server for graph visualization (alias with default port 3000)
    #[cfg(feature = "mcp-server")]
    ServeWeb {
        /// Port number
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },

    /// Start MCP server for AI agent integration
    #[cfg(feature = "mcp-server")]
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// Show statistics
    Stats {
        /// Community structure report
        #[arg(long)]
        community_report: bool,

        /// Complexity distribution
        #[arg(long)]
        complexity_report: bool,

        /// Find hotspots
        #[arg(long)]
        hotspots: bool,

        /// Filter by repository namespace
        #[arg(long)]
        repo: Option<String>,
    },

    /// Multi-repository workspace management
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    /// Install external plugin
    Install { path: String },

    /// List all plugins
    List,

    /// Show plugin information
    Info { plugin_id: String },

    /// Uninstall plugin
    Uninstall { plugin_id: String },
}

#[cfg(feature = "mcp-server")]
#[derive(Subcommand)]
enum McpCommands {
    /// Start MCP server
    Serve {
        /// Transport type (stdio, http)
        #[arg(long, default_value = "stdio")]
        transport: String,

        /// Port for HTTP transport
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Watch repository and notify clients on graph updates (Phase 13.1.2)
        #[arg(long)]
        watch: bool,
    },
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    /// Initialize a multi-repo workspace
    Init,

    /// Add a repository to the workspace
    Add {
        /// Path to repository
        path: String,

        /// Namespace identifier (used in `repo:` queries)
        #[arg(long)]
        namespace: String,
    },

    /// List workspace repositories
    List,

    /// Index all repos and merge into workspace graph
    Sync,

    /// Remove a repository from the workspace
    Remove {
        /// Namespace to remove
        namespace: String,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum OutputFormat {
    Text,
    Json,
    Table,
}

fn main() -> anyhow::Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(log_level).init();

    // Route to appropriate command handler
    match cli.command {
        Commands::Init {
            path,
            namespace,
            languages,
            exclude,
        } => {
            use rbuilder::discovery::DiscoveryConfig;
            use rbuilder::languages::registry::LanguageRegistry;
            use rbuilder::multi_repo::stamp_repo_namespace;
            use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
            use std::path::Path;
            use std::sync::Arc;

            let root = Path::new(&path);
            let mut discovery = DiscoveryConfig::default();
            if let Some(langs) = languages {
                discovery.languages = Some(
                    langs
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }
            if let Some(excludes) = exclude {
                discovery.exclude_patterns = excludes
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }

            let discovery_config = discovery.clone();
            let registry = Arc::new(LanguageRegistry::new());
            let pipeline = ProcessingPipeline::with_config(
                Arc::clone(&registry),
                PipelineConfig {
                    discovery,
                    show_progress: true,
                    ..PipelineConfig::default()
                },
            );

            let (mut graph, stats) = pipeline.process_repository(root)?;
            if let Some(ns) = namespace {
                stamp_repo_namespace(&mut graph, &ns);
                println!("Tagged graph with repo namespace: {ns}");
            }

            let saved = graph.save_to_repo(root)?;

            let mut tracker = rbuilder::incremental::FileTracker::new(root);
            let discoverer =
                rbuilder::discovery::FileDiscoverer::with_config(registry, discovery_config);
            let files = discoverer.discover(root)?;
            tracker.index_files(&files, &graph)?;
            tracker.save()?;

            println!("Processed {} files", stats.files_processed);
            if stats.files_failed > 0 {
                println!("Skipped {} files due to errors", stats.files_failed);
            }
            println!("Created {} nodes", stats.nodes_created);
            println!("Created {} edges", stats.edges_created);
            println!("Time: {:.2}s", stats.duration.as_secs_f64());
            println!("Graph saved to {}", saved.display());

            let functions = graph.query("functions")?;
            println!("\nSample query (`functions`): {} result(s)", functions.len());
            Ok(())
        }

        Commands::Update { since, force, files } => {
            use rbuilder::cli::update;
            use std::path::Path;
            update::run_update(Path::new("."), since, force, files, cli.verbose)?;
            Ok(())
        }

        Commands::Watch { path, debounce_ms } => {
            use rbuilder::watch::WatchService;
            use std::path::Path;
            WatchService::run_blocking(Path::new(&path), debounce_ms)?;
            Ok(())
        }

        Commands::InitHooks { path, force } => {
            use rbuilder::hooks::install_hooks;
            use std::path::Path;
            let written = install_hooks(Path::new(&path), force)?;
            for hook in written {
                println!("Installed {}", hook.display());
            }
            Ok(())
        }

        Commands::DetectChanges { files, json } => {
            use rbuilder::changes::ChangeDetector;
            use rbuilder::config::project::RbuilderConfig;
            use rbuilder::git_util;
            use rbuilder::graph::CodeGraph;
            use std::path::Path;

            let repo = Path::new(".");
            let paths = if files.is_empty() {
                git_util::git_staged_files(repo)?
            } else {
                files
            };

            if paths.is_empty() {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "files": [],
                            "risk_level": "LOW",
                            "max_score": 0.0,
                            "details": [],
                            "summary": {
                                "files_analyzed": 0,
                                "symbols_analyzed": 0,
                                "max_score": 0.0,
                                "risk_level": "LOW"
                            }
                        }))?
                    );
                } else {
                    println!("No files to analyze");
                }
                return Ok(());
            }

            let graph = CodeGraph::load_from_repo(repo)?;
            let config = RbuilderConfig::load(repo)?;
            let result = ChangeDetector::new()
                .with_blast_radius_threshold(config.hooks.blast_radius_threshold)
                .detect(&graph, &paths)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!(
                    "Risk: {:?} (max score {:.1})",
                    result.risk_level, result.max_score
                );
                for detail in result.details.iter().take(10) {
                    println!(
                        "  {}:{} score={:.1} callers={} impact={}",
                        detail.file,
                        detail.symbol,
                        detail.blast_radius_score,
                        detail.direct_callers,
                        detail.impact_zone_size
                    );
                }
            }

            if config.hooks.block_on_risk.blocks(result.risk_level) {
                std::process::exit(1);
            }
            Ok(())
        }

        Commands::Analyze {
            community,
            complexity,
            centrality,
            all,
        } => {
            use rbuilder::analysis::{
                ComplexityAnalyzer, DependencyAnalyzer,
            };
            use rbuilder::graph::CodeGraph;
            use rbuilder::nlp::PatternMatcher;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let backend = graph.backend();
            let matcher = PatternMatcher::new();

            let run_all = all || (!community && !complexity && !centrality);

            if community || run_all {
                println!("{}", matcher.analyze_communities(backend)?);
            }
            if complexity || run_all {
                let report = ComplexityAnalyzer::analyze(backend)?;
                println!(
                    "Complexity: {} functions, avg cyclomatic {:.1}, max {}",
                    report.functions.len(),
                    report.avg_cyclomatic,
                    report.max_cyclomatic
                );
                for (level, count) in &report.by_level {
                    println!("  {:?}: {}", level, count);
                }
            }
            if centrality || run_all {
                println!("{}", matcher.analyze_centrality(backend)?);
            }
            if run_all {
                let cycles = DependencyAnalyzer::find_circular_dependencies(backend)?;
                println!("Circular dependencies: {}", cycles.len());
            }
            Ok(())
        }

        Commands::Ask {
            question,
            explain,
            dual_agent,
            format,
        } => {
            use rbuilder::graph::CodeGraph;
            use rbuilder::nlp::PatternMatcher;
            use rbuilder::nlp::QueryResult;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let matcher = PatternMatcher::from_graph(graph.backend())?;

            if dual_agent {
                use rbuilder::nlp::dual_agent::DualAgentQuerySystem;

                let dual = DualAgentQuerySystem::new().query(&question, graph.backend())?;
                let answer = dual.answer_lines.join("\n");
                if explain {
                    println!("Answer: {answer}");
                    for sq in &dual.context.sub_queries {
                        println!(
                            "  - {} => {} ({} results)",
                            sq.natural_language,
                            sq.translated_pattern.as_deref().unwrap_or("?"),
                            sq.results.len() + sq.text_results.len()
                        );
                    }
                    println!();
                } else {
                    println!("{answer}");
                }
                return Ok(());
            }

            let translated = matcher.translate_with_dual_agent(&question)?;

            if explain {
                println!("Intent: {:?}", translated.intent);
                println!("Operation: {}", translated.operation);
                println!("Internal query: {}", translated.internal_query);
                println!("Confidence: {:.2}", translated.confidence);
                println!();
            }

            let result = matcher.execute(&translated, graph.backend())?;

            match format {
                OutputFormat::Json => {
                    let json = match &result {
                        QueryResult::Count(n) => serde_json::json!({ "count": n }),
                        QueryResult::Nodes(nodes) => serde_json::json!({
                            "results": nodes.iter().map(|n| serde_json::json!({
                                "name": n.name,
                                "type": format!("{:?}", n.node_type),
                                "file": n.file_path,
                            })).collect::<Vec<_>>()
                        }),
                        QueryResult::Text(lines) => serde_json::json!({ "lines": lines }),
                    };
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                OutputFormat::Text | OutputFormat::Table => match result {
                    QueryResult::Count(n) => println!("Found {n} result(s)"),
                    QueryResult::Nodes(nodes) => {
                        if nodes.is_empty() {
                            println!("No results found.");
                        } else {
                            println!("Found {} result(s):", nodes.len());
                            for node in nodes.iter().take(50) {
                                let file = node.file_path.as_deref().unwrap_or("?");
                                println!("  - {} ({:?}) @ {}", node.name, node.node_type, file);
                            }
                            if nodes.len() > 50 {
                                println!("  ... and {} more", nodes.len() - 50);
                            }
                        }
                    }
                    QueryResult::Text(lines) => {
                        for line in lines {
                            println!("{line}");
                        }
                    }
                },
            }
            Ok(())
        }

        Commands::Chat => {
            use rbuilder::cli::chat;
            use std::path::Path;
            chat::run_chat(Path::new("."))?;
            Ok(())
        }

        Commands::Label { ruleset, dry_run } => {
            use rbuilder::graph::CodeGraph;
            use rbuilder::rules::{RuleEngine, Ruleset};
            use std::path::Path;

            let ruleset = Ruleset::from_file(Path::new(&ruleset))?;
            let mut graph = CodeGraph::load_from_repo(Path::new("."))?;
            let report = RuleEngine::apply_ruleset(graph.backend_mut(), &ruleset, dry_run)?;

            if dry_run {
                println!("Would apply {} rules:", ruleset.rules.len());
            } else {
                println!("Applied {} rules:", ruleset.rules.len());
                graph.save_to_repo(Path::new("."))?;
            }

            for (rule, count) in &report.rule_matches {
                println!("  - {rule}: {count} matches");
            }
            if !dry_run {
                println!("Modified {} nodes", report.nodes_modified);
            }
            Ok(())
        }

        Commands::Idl {
            format,
            module,
            output_dir,
        } => {
            use rbuilder::graph::CodeGraph;
            use rbuilder::semantic::{IdlFormat, IdlGenerator};
            use std::path::Path;

            let idl_format = IdlFormat::parse(&format)?;
            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let generator = IdlGenerator::new();
            let module_name = module.unwrap_or_else(|| "service".to_string());

            if let Some(dir) = output_dir {
                let path = generator.write_module(
                    graph.backend(),
                    idl_format,
                    &module_name,
                    Path::new(&dir),
                )?;
                println!("Generated IDL: {}", path.display());
            } else {
                let content = generator.generate_module(graph.backend(), idl_format, &module_name)?;
                print!("{content}");
            }
            Ok(())
        }

        Commands::Config {
            unused,
            missing_env,
            secrets,
            drift,
        } => {
            use rbuilder::config::analyzer::ConfigAnalyzer;
            use rbuilder::config::secret_detector::SecretDetector;
            use rbuilder::discovery::FileDiscoverer;
            use rbuilder::graph::CodeGraph;
            use rbuilder::languages::registry::LanguageRegistry;
            use std::path::Path;
            use std::sync::Arc;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let run_all = !unused && !missing_env && !secrets && drift.is_none();

            if unused || run_all {
                let unused_keys = ConfigAnalyzer::find_unused_keys(graph.backend())?;
                println!("Unused config keys: {}", unused_keys.len());
                for key in unused_keys.iter().take(20) {
                    println!("  - {} ({})", key.key, key.file.as_deref().unwrap_or("?"));
                }
            }
            if missing_env || run_all {
                let missing =
                    ConfigAnalyzer::find_missing_env_vars(graph.backend(), &[Path::new(".env")])?;
                println!("Missing env vars: {}", missing.len());
                for var in &missing {
                    println!("  - {}", var.var);
                }
            }
            if secrets || run_all {
                let discoverer = FileDiscoverer::new(Arc::new(LanguageRegistry::new()));
                let files = discoverer.discover(Path::new("."))?;
                let detector = SecretDetector::new();
                let mut total = 0usize;
                for file in files {
                    if let Ok(content) = std::fs::read_to_string(&file) {
                        let found = detector.scan(&content);
                        for secret in &found {
                            println!(
                                "  [{}] {}:{} - {} ({:?})",
                                file.display(),
                                secret.line,
                                secret.secret_type,
                                secret.value,
                                secret.severity
                            );
                        }
                        total += found.len();
                    }
                }
                println!("Potential secrets found: {total}");
            }
            if let Some(paths) = drift {
                use rbuilder::config::drift::{compare_configs, format_drift_report};
                use std::path::Path;

                if paths.len() < 2 {
                    anyhow::bail!("--drift requires at least two config file paths");
                }
                let left = Path::new(&paths[0]);
                let right = Path::new(&paths[1]);
                let report = compare_configs(left, right)?;
                println!("{}", format_drift_report(&report));
                if !report.is_clean() {
                    std::process::exit(1);
                }
            }
            Ok(())
        }

        Commands::Plugin { command } => {
            use rbuilder::languages::plugin_loader::{PluginLoader, PluginRegistry};
            use rbuilder::languages::registry::LanguageRegistry;
            use std::path::Path;

            match command {
                PluginCommands::Install { path } => {
                    let source = Path::new(&path);
                    let dest = PluginLoader::copy_to_plugins_dir(Path::new("."), source)?;
                    let metadata = PluginLoader::install(Path::new("."), &dest)?;
                    println!(
                        "Installed plugin '{}' v{} from {}",
                        metadata.language_id, metadata.version, dest.display()
                    );
                }
                PluginCommands::List => {
                    let registry = LanguageRegistry::new();
                    println!("Built-in plugins:");
                    for id in registry.language_plugin_ids() {
                        if let Some(plugin) = registry.get_language_plugin(&id) {
                            println!(
                                "  - {} (extensions: {})",
                                plugin.language_id(),
                                plugin.file_extensions().join(", ")
                            );
                        }
                    }
                    let external = PluginRegistry::load(Path::new("."))?;
                    if external.plugins.is_empty() {
                        println!("\nExternal plugins: none");
                    } else {
                        println!("\nExternal plugins:");
                        for plugin in &external.plugins {
                            println!("  - {} v{} at {}", plugin.language_id, plugin.version, plugin.path);
                        }
                    }
                }
                PluginCommands::Info { plugin_id } => {
                    let registry = LanguageRegistry::new();
                    if let Some(plugin) = registry.get_language_plugin(&plugin_id) {
                        println!("Built-in plugin: {}", plugin.language_id());
                        println!("Extensions: {:?}", plugin.file_extensions());
                        let caps = plugin.capabilities();
                        println!("Capabilities: functions={}, types={}", caps.extracts_functions, caps.extracts_types);
                    } else if let Some(ext) = PluginRegistry::load(Path::new("."))?.get(&plugin_id) {
                        println!("External plugin: {}", ext.language_id);
                        println!("Version: {}", ext.version);
                        println!("Path: {}", ext.path);
                        println!("Extensions: {:?}", ext.extensions);
                    } else {
                        anyhow::bail!("Plugin not found: {plugin_id}");
                    }
                }
                PluginCommands::Uninstall { plugin_id } => {
                    let mut registry = PluginRegistry::load(Path::new("."))?;
                    if registry.uninstall(&plugin_id) {
                        registry.save(Path::new("."))?;
                        println!("Uninstalled plugin: {plugin_id}");
                    } else {
                        anyhow::bail!("External plugin not found: {plugin_id}");
                    }
                }
            }
            Ok(())
        }

        Commands::Export { format, output, query } => {
            use rbuilder::export::export_graphml;
            use rbuilder::graph::CodeGraph;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let content = if format.eq_ignore_ascii_case("graphml") {
                export_graphml(graph.backend(), &query)?
            } else if format.eq_ignore_ascii_case("json") {
                graph.export_json()?
            } else {
                anyhow::bail!("Supported export formats: json, graphml");
            };
            std::fs::write(&output, content)?;
            println!(
                "Exported {} nodes and {} edges to {}",
                graph.node_count(),
                graph.edge_count(),
                output
            );
            Ok(())
        }

        Commands::Diagram {
            query,
            format,
            diagram_type,
            output,
            depth,
            layout,
            rankdir,
        } => {
            use rbuilder::cli::diagram::{run_diagram, DiagramOptions};
            use std::path::Path;

            run_diagram(
                Path::new("."),
                DiagramOptions {
                    query,
                    format,
                    diagram_type,
                    output: output.map(Into::into),
                    depth,
                    layout,
                    rankdir,
                },
            )?;
            Ok(())
        }

        #[cfg(feature = "mcp-server")]
        Commands::ServeWeb { port, open } => {
            use rbuilder::cli::serve;
            use std::path::Path;
            serve::run_serve(Path::new("."), port, open)?;
            Ok(())
        }

        #[cfg(feature = "mcp-server")]
        Commands::Serve { port, open } => {
            use rbuilder::cli::serve;
            use std::path::Path;
            serve::run_serve(Path::new("."), port, open)?;
            Ok(())
        }

        #[cfg(feature = "mcp-server")]
        Commands::Mcp { command } => {
            use rbuilder::cli::mcp;
            use std::path::Path;
            match command {
                McpCommands::Serve {
                    transport,
                    port,
                    watch,
                } => {
                    mcp::run_mcp_serve(Path::new("."), &transport, port, cli.verbose, watch)?;
                }
            }
            Ok(())
        }

        Commands::Stats {
            community_report,
            complexity_report,
            hotspots,
            repo,
        } => {
            use rbuilder::analysis::{CentralityAnalyzer, ComplexityAnalyzer};
            use rbuilder::graph::backend::GraphBackend;
            use rbuilder::graph::CodeGraph;
            use rbuilder::multi_repo::load_workspace_graph;
            use rbuilder::nlp::PatternMatcher;
            use std::path::Path;

            let graph = load_workspace_graph(Path::new("."))
                .or_else(|_| CodeGraph::load_from_repo(Path::new(".")))?;
            let backend = graph.backend();

            if let Some(ref ns) = repo {
                let nodes = graph.query(&format!("repo:{ns}"))?;
                println!("Repo '{ns}': {} nodes", nodes.len());
            }

            let run_all = !community_report && !complexity_report && !hotspots;

            if community_report || run_all {
                println!("{}", PatternMatcher::new().analyze_communities(backend)?);
            }
            if complexity_report || run_all {
                let report = ComplexityAnalyzer::analyze(backend)?;
                println!(
                    "Functions: {}, avg complexity {:.1}, max {}",
                    report.functions.len(),
                    report.avg_cyclomatic,
                    report.max_cyclomatic
                );
            }
            if hotspots || run_all {
                let report = CentralityAnalyzer::new().analyze(backend)?;
                println!("Top PageRank hotspots:");
                for (id, score) in report.top_pagerank.iter().take(10) {
                    if let Ok(Some(node)) = backend.get_node(*id) {
                        if let Some(ref ns) = repo {
                            if node.get_property("repo").is_none_or(|r| r != ns) {
                                continue;
                            }
                        }
                        println!("  - {} ({score:.4})", node.name);
                    }
                }
            }
            Ok(())
        }

        Commands::Slice {
            file,
            line,
            variable,
            function,
            language,
        } => {
            use rbuilder::analysis::{
                build_cfg_for_function, BackwardSlicer, ProgramDependenceGraph, SliceCriterion,
            };
            use std::fs;
            use std::path::Path;

            let path = Path::new(&file);
            let source = fs::read_to_string(path)?;
            let lang = language.unwrap_or_else(|| {
                match path.extension().and_then(|e| e.to_str()) {
                    Some("py") => "python".to_string(),
                    _ => "rust".to_string(),
                }
            });
            let fn_name = function.unwrap_or_else(|| {
                if lang == "python" {
                    "process".to_string()
                } else {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("main")
                        .to_string()
                }
            });

            let cfg = build_cfg_for_function(&lang, &source, &fn_name)?;
            let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
            let slice = BackwardSlicer::new(&pdg, &cfg).slice(SliceCriterion {
                variable,
                line,
            })?;

            println!(
                "Backward slice for {}:{} (variable: {})",
                path.display(),
                slice.criterion.line,
                slice.criterion.variable
            );
            println!("Reduction: {:.1}%", slice.reduction_percent);
            println!("Relevant lines:");
            let mut lines: Vec<_> = slice.lines.into_iter().collect();
            lines.sort_unstable();
            for ln in lines {
                println!("  {ln}");
            }
            Ok(())
        }

        Commands::Gql {
            query,
            explain,
            macro_name,
        } => {
            use rbuilder::gql::{execute, execute_explain, execute_macro, QueryMacroRegistry};
            use rbuilder::graph::CodeGraph;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let backend = graph.backend();
            let registry = QueryMacroRegistry::with_defaults();

            let result = if let Some(name) = macro_name {
                execute_macro(backend, &registry, &name)?
            } else if explain {
                execute_explain(backend, &query)?
            } else {
                execute(backend, &query)?
            };

            if explain {
                if let Some(plan) = result.plan {
                    for step in &plan.steps {
                        println!("{}: {}", step.operation, step.detail);
                    }
                    println!();
                }
            }

            for row in &result.rows {
                let names: Vec<_> = row
                    .values()
                    .map(|binding| binding.name.clone())
                    .collect();
                println!("{}", names.join(" -> "));
            }
            Ok(())
        }

        Commands::BlastRadius { symbol, depth } => {
            use rbuilder::analysis::BlastRadiusAnalyzer;
            use rbuilder::graph::CodeGraph;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;
            let report = BlastRadiusAnalyzer::new(graph.backend())
                .with_max_depth(depth)
                .analyze(&symbol)?;

            println!("Blast radius for '{symbol}'");
            println!("  Score: {:.1}/100", report.score);
            println!("  Direct callers: {}", report.direct_callers.len());
            println!("  Impact zone: {}", report.impact_zone.len());
            println!("  Data-flow depth: {}", report.data_flow_depth);
            if !report.direct_callers.is_empty() {
                println!("  Callers: {}", report.direct_callers.join(", "));
            }
            Ok(())
        }

        Commands::Workspace { command } => {
            use rbuilder::cli::workspace;
            use std::path::Path;
            let root = Path::new(".");
            match command {
                WorkspaceCommands::Init => workspace::run_init(root)?,
                WorkspaceCommands::Add { path, namespace } => {
                    workspace::run_add(root, Path::new(&path), &namespace)?;
                }
                WorkspaceCommands::List => workspace::run_list(root)?,
                WorkspaceCommands::Sync => workspace::run_sync(root, cli.verbose)?,
                WorkspaceCommands::Remove { namespace } => workspace::run_remove(root, &namespace)?,
            }
            Ok(())
        }
    }
}
