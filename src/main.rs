//! rBuilder CLI
//!
//! Command-line interface for the rBuilder knowledge graph system.

use clap::{Parser, Subcommand};
use rbuilder::BUILD_INFO;
use rbuilder_graph::backend::GraphBackend;

#[derive(Parser)]
#[command(name = "rbuilder")]
#[command(about = "AI-powered code knowledge graph", long_about = None)]
#[command(version = BUILD_INFO)]
struct Cli {
    /// Repository path to analyze
    #[arg(global = true)]
    path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Include only specific languages (comma-separated)
    #[arg(short, long, global = true)]
    languages: Option<String>,

    /// Exclude patterns (comma-separated)
    #[arg(short, long, global = true)]
    exclude: Option<String>,

    /// Watch for file changes and auto-update
    #[arg(short, long, global = true)]
    watch: bool,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Watch repository and re-index on file changes
    Watch {
        /// Debounce window in milliseconds
        #[arg(long)]
        debounce_ms: Option<u64>,
    },

    /// Git hooks management
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },

    /// Run specific analyses on existing graph
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

        /// Analyze dependencies (works for all languages)
        #[arg(long)]
        dependencies: bool,

        /// Run security analysis (secrets, vulnerabilities, misconfigurations)
        #[arg(long)]
        security: bool,

        /// Build control flow graphs for all functions
        #[arg(long)]
        cfg: bool,

        /// Build program dependence graphs for all functions (implies --cfg)
        #[arg(long)]
        pdg: bool,

        /// Compute dominance trees for all functions (implies --cfg)
        #[arg(long)]
        dominance: bool,

        /// Filter by language (ansible, chef, puppet, python, rust, etc.)
        #[arg(long)]
        language: Option<String>,

        /// Output format
        #[arg(long, default_value = "text")]
        format: String,

        /// Output directory for CFG/PDG/Dominance results
        #[arg(long)]
        output: Option<String>,

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

        /// Use dual-agent query decomposition
        #[arg(long)]
        dual_agent: bool,

        /// Output format
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },

    /// Backward program slice for a variable at a source line
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

    /// Execute graph query language against the indexed graph
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

    /// Impact analysis for a symbol
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
        /// Export format (json, graphml, html)
        #[arg(long)]
        format: String,

        /// Output file
        #[arg(long)]
        output: String,

        /// Query DSL when exporting graphml (default: all)
        #[arg(long, default_value = "all")]
        query: String,
    },

    /// Generate a diagram from a graph query
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

#[derive(Subcommand)]
enum HooksCommands {
    /// Install git hooks for pre-commit, post-commit, and post-checkout
    Install {
        /// Overwrite existing hook scripts
        #[arg(long)]
        force: bool,
    },

    /// Uninstall git hooks
    Uninstall,

    /// List installed hooks
    List,
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

    rbuilder::init();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(log_level).init();

    // If no subcommand, run full analysis
    if cli.command.is_none() {
        let path = match cli.path.as_deref() {
            Some(p) => p,
            None => {
                eprintln!("Error: PATH argument required for analysis");
                eprintln!();
                eprintln!("Usage: rbuilder <PATH> [OPTIONS]");
                eprintln!("       rbuilder . [OPTIONS]          # Analyze current directory");
                eprintln!("       rbuilder /path/to/repo        # Analyze specific directory");
                eprintln!();
                eprintln!("Or use a subcommand:");
                eprintln!("       rbuilder update               # Update existing graph");
                eprintln!("       rbuilder ask \"question\"       # Query the graph");
                eprintln!("       rbuilder stats                # Show statistics");
                eprintln!();
                eprintln!("Run 'rbuilder --help' for more information");
                std::process::exit(1);
            }
        };
        return run_full_analysis(path, cli.languages, cli.exclude, cli.watch, cli.verbose);
    }

    // Route to appropriate command handler
    match cli.command.unwrap() {
        Commands::Update {
            since,
            force,
            files,
        } => {
            use rbuilder::cli::update;
            use std::path::Path;
            let path = cli.path.as_deref().unwrap_or(".");
            update::run_update(Path::new(path), since, force, files, cli.verbose)?;
            Ok(())
        }

        Commands::Watch { debounce_ms } => {
            use rbuilder::watch::WatchService;
            use std::path::Path;
            let path = cli.path.as_deref().unwrap_or(".");
            WatchService::run_blocking(Path::new(path), debounce_ms)?;
            Ok(())
        }

        Commands::Hooks { command } => {
            use rbuilder::hooks::{install_hooks, list_hooks, uninstall_hooks};
            use std::path::Path;
            let path = cli.path.as_deref().unwrap_or(".");
            match command {
                HooksCommands::Install { force } => {
                    let written = install_hooks(Path::new(path), force)?;
                    for hook in written {
                        println!("Installed {}", hook.display());
                    }
                }
                HooksCommands::Uninstall => {
                    uninstall_hooks(Path::new(path))?;
                    println!("Uninstalled git hooks");
                }
                HooksCommands::List => {
                    let hooks = list_hooks(Path::new(path))?;
                    if hooks.is_empty() {
                        println!("No hooks installed");
                    } else {
                        println!("Installed hooks:");
                        for hook in hooks {
                            println!("  - {}", hook.display());
                        }
                    }
                }
            }
            Ok(())
        }

        Commands::Analyze {
            community,
            complexity,
            centrality,
            dependencies,
            security,
            cfg,
            pdg,
            dominance,
            language,
            format: _,
            output,
            all,
        } => {
            use rbuilder::analysis::{ComplexityAnalyzer, DependencyAnalyzer};
            use rbuilder::config::secret_detector::SecretDetector;
            use rbuilder::discovery::FileDiscoverer;
            use rbuilder::languages::registry::LanguageRegistry;
            use rbuilder::nlp::PatternMatcher;
            use rbuilder_graph::CodeGraph;
            use std::path::Path;

            let path = cli.path.as_deref().unwrap_or(".");
            let graph = CodeGraph::load_from_repo(Path::new(path))?;
            let backend = graph.backend();
            let matcher = PatternMatcher::new();

            let run_all = all || (!community && !complexity && !centrality && !dependencies && !security);

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
            if dependencies || run_all {
                let cycles = DependencyAnalyzer::find_circular_dependencies(backend)?;
                println!("Circular dependencies: {}", cycles.len());

                // Show dependencies filtered by language if specified
                if let Some(ref lang) = language {
                    let query = format!("type:{}", lang);
                    let nodes = graph.query(&query)?;
                    println!("\n{} dependencies for {} nodes:", lang, nodes.len());
                    for node in nodes.iter().take(10) {
                        println!("  - {}", node.name);
                    }
                }
            }
            if security || run_all {
                // Run secret detection
                let discoverer = FileDiscoverer::new(LanguageRegistry::new().into());
                let files = discoverer.discover(Path::new(path))?;
                let detector = SecretDetector::new();
                let mut total = 0usize;

                for file in files.iter().take(100) {
                    if let Ok(content) = std::fs::read_to_string(file) {
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

                // Query security findings from graph if any
                if let Ok(security_findings) = graph.query("type:SecurityFinding") {
                    if !security_findings.is_empty() {
                        println!("\nSecurity findings in graph: {}", security_findings.len());
                        for finding in security_findings.iter().take(10) {
                            println!("  - {}", finding.name);
                        }
                    }
                }
            }

            // CFG/PDG/Dominance analysis
            if cfg || pdg || dominance {
                use rbuilder::analysis::{
                    build_cfg_for_function, AnalysisStorage, DominatorTree, FunctionAnalysis,
                    ProgramDependenceGraph,
                };
                use rbuilder_graph::schema::NodeType;

                let output_dir = output
                    .as_deref()
                    .unwrap_or(".rbuilder/analysis");
                let storage = AnalysisStorage::new(output_dir);
                storage.ensure_dir()?;

                // Find all function nodes
                let functions: Vec<_> = backend
                    .all_nodes()?
                    .into_iter()
                    .filter(|n| n.node_type == NodeType::Function)
                    .collect();

                println!("\nBuilding analysis for {} functions...", functions.len());
                let mut success_count = 0;
                let mut error_count = 0;

                for func_node in &functions {
                    // Get function source
                    let file_path = match &func_node.file_path {
                        Some(p) => p,
                        None => {
                            error_count += 1;
                            continue;
                        }
                    };

                    let source = match std::fs::read_to_string(file_path) {
                        Ok(s) => s,
                        Err(_) => {
                            error_count += 1;
                            continue;
                        }
                    };

                    // Determine language from file extension
                    let lang = if file_path.ends_with(".rs") {
                        "rust"
                    } else if file_path.ends_with(".py") {
                        "python"
                    } else {
                        error_count += 1;
                        continue;
                    };

                    // Build CFG
                    let cfg_result = if cfg || pdg || dominance {
                        build_cfg_for_function(lang, &source, &func_node.name)
                    } else {
                        error_count += 1;
                        continue;
                    };

                    let cfg_data = match cfg_result {
                        Ok(c) => Some(c),
                        Err(_) => {
                            error_count += 1;
                            continue;
                        }
                    };

                    // Build PDG if requested
                    let pdg_data = if pdg && cfg_data.is_some() {
                        match ProgramDependenceGraph::build(
                            cfg_data.as_ref().unwrap(),
                            source.as_bytes(),
                        ) {
                            Ok(p) => Some(p),
                            Err(_) => None,
                        }
                    } else {
                        None
                    };

                    // Build Dominance if requested
                    let dom_data = if dominance && cfg_data.is_some() {
                        Some(DominatorTree::build(cfg_data.as_ref().unwrap()))
                    } else {
                        None
                    };

                    // Store analysis
                    let analysis = FunctionAnalysis {
                        function_id: func_node.id,
                        function_name: func_node.name.clone(),
                        file_path: file_path.clone(),
                        cfg: cfg_data,
                        pdg: pdg_data,
                        dominance: dom_data,
                    };

                    if storage.save_function(&analysis).is_ok() {
                        success_count += 1;
                    } else {
                        error_count += 1;
                    }
                }

                println!(
                    "✓ Analysis complete: {} functions analyzed, {} errors",
                    success_count, error_count
                );
                println!("Results saved to {}", output_dir);

                // Export to consolidated JSON
                let export_path = Path::new(output_dir).join("all_analyses.json");
                storage.export_all(&export_path)?;
                println!("Exported to {}", export_path.display());
            }

            Ok(())
        }

        Commands::Ask {
            question,
            explain,
            dual_agent,
            format,
        } => {
            use rbuilder::nlp::PatternMatcher;
            use rbuilder::nlp::QueryResult;
            use rbuilder_graph::CodeGraph;
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
            use rbuilder::rules::{RuleEngine, Ruleset};
            use rbuilder_graph::CodeGraph;
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
            use rbuilder::semantic::{IdlFormat, IdlGenerator};
            use rbuilder_graph::CodeGraph;
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
                let content =
                    generator.generate_module(graph.backend(), idl_format, &module_name)?;
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
            use rbuilder::languages::registry::LanguageRegistry;
            use rbuilder_graph::CodeGraph;
            use std::path::Path;

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
                let discoverer = FileDiscoverer::new(LanguageRegistry::new().into());
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
                        metadata.language_id,
                        metadata.version,
                        dest.display()
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
                            println!(
                                "  - {} v{} at {}",
                                plugin.language_id, plugin.version, plugin.path
                            );
                        }
                    }
                }
                PluginCommands::Info { plugin_id } => {
                    let registry = LanguageRegistry::new();
                    if let Some(plugin) = registry.get_language_plugin(&plugin_id) {
                        println!("Built-in plugin: {}", plugin.language_id());
                        println!("Extensions: {:?}", plugin.file_extensions());
                        let caps = plugin.capabilities();
                        println!(
                            "Capabilities: functions={}, types={}",
                            caps.extracts_functions, caps.extracts_types
                        );
                    } else if let Some(ext) = PluginRegistry::load(Path::new("."))?.get(&plugin_id)
                    {
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

        Commands::Export {
            format,
            output,
            query,
        } => {
            use rbuilder::export::{export_graphml, export_html_dashboard};
            use rbuilder_graph::CodeGraph;
            use std::path::Path;

            let graph = CodeGraph::load_from_repo(Path::new("."))?;

            if format.eq_ignore_ascii_case("html") {
                let analysis_dir = Path::new(".rbuilder/analysis");
                export_html_dashboard(
                    graph.backend(),
                    if analysis_dir.exists() { Some(analysis_dir) } else { None },
                    Path::new(&output),
                ).map_err(|e| anyhow::anyhow!(e))?;
                println!(
                    "Exported {} nodes and {} edges to HTML dashboard: {}",
                    graph.node_count(),
                    graph.edge_count(),
                    output
                );
            } else {
                let content = if format.eq_ignore_ascii_case("graphml") {
                    export_graphml(graph.backend(), &query)?
                } else if format.eq_ignore_ascii_case("json") {
                    graph.export_json()?
                } else {
                    anyhow::bail!("Supported export formats: json, graphml, html");
                };
                std::fs::write(&output, content)?;
                println!(
                    "Exported {} nodes and {} edges to {}",
                    graph.node_count(),
                    graph.edge_count(),
                    output
                );
            }
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

        Commands::Stats {
            community_report,
            complexity_report,
            hotspots,
            repo,
        } => {
            use rbuilder::analysis::{CentralityAnalyzer, ComplexityAnalyzer};
            use rbuilder::graph::backend::GraphBackend;
            use rbuilder::multi_repo::load_workspace_graph;
            use rbuilder::nlp::PatternMatcher;
            use rbuilder_graph::CodeGraph;
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
            let lang =
                language.unwrap_or_else(|| match path.extension().and_then(|e| e.to_str()) {
                    Some("py") => "python".to_string(),
                    _ => "rust".to_string(),
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
            let slice = BackwardSlicer::new(&pdg, &cfg).slice(SliceCriterion { variable, line })?;

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
            use rbuilder_graph::CodeGraph;
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
                let names: Vec<_> = row.values().map(|binding| binding.name.clone()).collect();
                println!("{}", names.join(" -> "));
            }
            Ok(())
        }

        Commands::BlastRadius { symbol, depth } => {
            use rbuilder::analysis::BlastRadiusAnalyzer;
            use rbuilder_graph::CodeGraph;
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

/// Run full analysis on a repository (default behavior when no subcommand is given)
fn run_full_analysis(
    path: &str,
    languages: Option<String>,
    exclude: Option<String>,
    watch: bool,
    verbose: bool,
) -> anyhow::Result<()> {
    use rbuilder::analysis::{CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer};
    use rbuilder::config::secret_detector::SecretDetector;
    use rbuilder::discovery::{DiscoveryConfig, FileDiscoverer};
    use rbuilder::incremental::FileTracker;
    use rbuilder::languages::registry::LanguageRegistry;
    use rbuilder::pipeline::{PipelineConfig, ProcessingPipeline};
    use rbuilder::watch::WatchService;
    use std::path::Path;
    use std::sync::Arc;

    let root = Path::new(path);
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

    println!("Analyzing repository: {}", root.display());

    let discovery_config = discovery.clone();
    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            discovery,
            show_progress: true,
            ..PipelineConfig::default()
        },
    );

    // Index the repository
    let (mut graph, stats) = pipeline.process_repository(root)?;

    println!("\n=== Indexing Complete ===");
    println!("Processed {} files", stats.files_processed);
    if stats.files_failed > 0 {
        println!("Skipped {} files due to errors", stats.files_failed);
    }
    println!("Created {} nodes", stats.nodes_created);
    println!("Created {} edges", stats.edges_created);
    println!("Time: {:.2}s", stats.duration.as_secs_f64());

    // Run analyses and persist results BEFORE saving
    println!("\n=== Running Analyses ===");

    // Community detection
    let community_result = CommunityDetector::new().detect(graph.backend_mut())?;
    // Persist community assignments
    for (node_id, community_id) in &community_result.assignments {
        if let Ok(Some(mut node)) = graph.backend().get_node(*node_id) {
            node.properties
                .insert("community".to_string(), community_id.to_string());
            graph.backend_mut().insert_node(node)?;
        }
    }
    if verbose {
        println!("\nCommunity detection:");
        println!("  Communities: {}", community_result.communities.len());
        println!("  Modularity: {:.3}", community_result.modularity);
    } else {
        println!("✓ Community detection complete ({} communities)", community_result.communities.len());
    }

    // Complexity analysis
    let complexity_report = ComplexityAnalyzer::analyze(graph.backend())?;
    // Persist complexity metrics
    for func in &complexity_report.functions {
        if let Ok(Some(mut node)) = graph.backend().get_node(func.node.id) {
            node.properties
                .insert("cyclomatic".to_string(), func.cyclomatic.to_string());
            node.properties
                .insert("cognitive".to_string(), func.cognitive.to_string());
            graph.backend_mut().insert_node(node)?;
        }
    }
    println!("\n✓ Complexity analysis:");
    println!("  Functions: {}", complexity_report.functions.len());
    println!("  Avg cyclomatic: {:.1}", complexity_report.avg_cyclomatic);
    println!("  Max cyclomatic: {}", complexity_report.max_cyclomatic);
    for (level, count) in &complexity_report.by_level {
        println!("    {:?}: {}", level, count);
    }

    // Centrality analysis
    let centrality_report = CentralityAnalyzer::new().analyze(graph.backend_mut())?;
    // Persist centrality scores
    for (node_id, scores) in &centrality_report.scores {
        if let Ok(Some(mut node)) = graph.backend().get_node(*node_id) {
            node.properties
                .insert("pagerank".to_string(), scores.pagerank.to_string());
            node.properties
                .insert("betweenness".to_string(), scores.betweenness.to_string());
            graph.backend_mut().insert_node(node)?;
        }
    }
    println!("\n✓ Centrality analysis:");
    println!("  Top hotspots by PageRank:");
    for (id, score) in centrality_report.top_pagerank.iter().take(5) {
        if let Ok(Some(node)) = graph.backend().get_node(*id) {
            println!("    - {} ({:.4})", node.name, score);
        }
    }

    // Dependency analysis
    let cycles = DependencyAnalyzer::find_circular_dependencies(graph.backend())?;
    println!("\n✓ Dependency analysis:");
    println!("  Circular dependencies: {}", cycles.len());

    // Discover files for security analysis and later tracking
    let discoverer = FileDiscoverer::with_config(Arc::clone(&registry), discovery_config);
    let files = discoverer.discover(root)?;

    // Security analysis
    println!("\n✓ Security analysis:");
    let detector = SecretDetector::new();
    let mut total_secrets = 0usize;

    for file in files.iter().take(100) {
        if let Ok(content) = std::fs::read_to_string(file) {
            let found = detector.scan(&content);
            total_secrets += found.len();

            if verbose {
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
            }
        }
    }
    println!("  Potential secrets found: {total_secrets}");

    // CFG/PDG/Dominance analysis (default)
    println!("\n✓ Control flow analysis:");
    use rbuilder::analysis::{
        build_cfg_for_function, AnalysisStorage, DominatorTree, FunctionAnalysis,
        ProgramDependenceGraph,
    };
    use rbuilder_graph::schema::NodeType;

    let output_dir = root.join(".rbuilder/analysis");
    let storage = AnalysisStorage::new(&output_dir);
    storage.ensure_dir()?;

    // Find all function nodes
    let backend = graph.backend();
    let functions: Vec<_> = backend
        .all_nodes()?
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .collect();

    let mut success_count = 0;
    let mut error_count = 0;

    for func_node in &functions {
        // Get function source
        let file_path = match &func_node.file_path {
            Some(p) => p,
            None => {
                error_count += 1;
                continue;
            }
        };

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };

        // Determine language from file extension
        let lang = if file_path.ends_with(".rs") {
            "rust"
        } else if file_path.ends_with(".py") {
            "python"
        } else if file_path.ends_with(".java") {
            "java"
        } else {
            // Skip unsupported languages for CFG
            error_count += 1;
            continue;
        };

        // Build CFG
        let cfg_result = build_cfg_for_function(lang, &source, &func_node.name);

        let cfg_data = match cfg_result {
            Ok(c) => Some(c),
            Err(_) => {
                error_count += 1;
                continue;
            }
        };

        // Build PDG
        let pdg_data = if let Some(ref cfg) = cfg_data {
            match ProgramDependenceGraph::build(cfg, source.as_bytes()) {
                Ok(p) => Some(p),
                Err(_) => None,
            }
        } else {
            None
        };

        // Build Dominance
        let dom_data = if let Some(ref cfg) = cfg_data {
            Some(DominatorTree::build(cfg))
        } else {
            None
        };

        // Store analysis
        let analysis = FunctionAnalysis {
            function_id: func_node.id,
            function_name: func_node.name.clone(),
            file_path: file_path.clone(),
            cfg: cfg_data,
            pdg: pdg_data,
            dominance: dom_data,
        };

        if storage.save_function(&analysis).is_ok() {
            success_count += 1;
        } else {
            error_count += 1;
        }
    }

    if success_count > 0 {
        println!("  CFG/PDG/Dominance: {} functions analyzed", success_count);
        if error_count > 0 {
            println!("  Skipped: {} functions (unsupported language or parse error)", error_count);
        }

        // Export consolidated file
        let export_path = output_dir.join("all_analyses.json");
        if storage.export_all(&export_path).is_ok() {
            if verbose {
                println!("  Exported to {}", export_path.display());
            }
        }
    } else if !functions.is_empty() {
        println!("  No functions analyzed (Rust/Python only)");
    }

    // Blast radius analysis (default)
    println!("\n✓ Blast radius analysis:");
    use rbuilder::analysis::{BlastRadiusAnalyzer, FlowCache};

    // Build FlowCache from stored PDG analyses for enriched blast radius
    let mut flow_cache = FlowCache::new();
    let all_analyses = storage.load_all().unwrap_or_default();
    for analysis in &all_analyses {
        if let Some(ref pdg) = analysis.pdg {
            flow_cache.insert_pdg(analysis.function_id, pdg.clone());
        }
    }

    // Compute blast radius for all functions (collect updates first)
    let mut blast_updates = Vec::new();
    let mut high_impact_count = 0;
    let mut max_impact_score = 0.0f64;
    let mut max_impact_function = String::new();

    for func_node in &functions {
        let analyzer = BlastRadiusAnalyzer::new(backend)
            .with_flow_cache(&flow_cache)
            .with_max_depth(10);

        if let Ok(report) = analyzer.analyze(&func_node.name) {
            blast_updates.push((func_node.id, report.clone()));

            if report.score > 50.0 {
                high_impact_count += 1;
            }

            if report.score > max_impact_score {
                max_impact_score = report.score;
                max_impact_function = func_node.name.clone();
            }
        }
    }

    // Apply blast radius updates to graph
    let mut analyzed_functions = 0;
    for (node_id, report) in blast_updates {
        if let Ok(Some(mut node)) = graph.backend().get_node(node_id) {
            node.properties
                .insert("blast_radius_score".to_string(), report.score.to_string());
            node.properties.insert(
                "blast_radius_direct_callers".to_string(),
                report.direct_callers.len().to_string(),
            );
            node.properties.insert(
                "blast_radius_impact_zone".to_string(),
                report.impact_zone.len().to_string(),
            );
            node.properties.insert(
                "blast_radius_data_flow_depth".to_string(),
                report.data_flow_depth.to_string(),
            );

            if graph.backend_mut().insert_node(node).is_ok() {
                analyzed_functions += 1;
            }
        }
    }

    if analyzed_functions > 0 {
        println!("  Functions: {} analyzed", analyzed_functions);
        println!("  High impact (>50): {}", high_impact_count);
        if !max_impact_function.is_empty() {
            println!(
                "  Max impact: {} (score: {:.1})",
                max_impact_function, max_impact_score
            );
        }
    } else {
        println!("  No blast radius data computed");
    }

    println!("\n=== Analysis Complete ===");

    // Save graph with analysis results
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph)?;
    tracker.save()?;

    let saved = graph.save_to_repo(root)?;
    println!("\nGraph with analysis results saved to {}", saved.display());

    // Export HTML dashboard
    use rbuilder::export::export_html_dashboard;
    let html_path = root.join(".rbuilder/dashboard.html");
    if let Err(e) = export_html_dashboard(
        graph.backend(),
        Some(&output_dir),
        &html_path,
    ) {
        eprintln!("Warning: Failed to export HTML dashboard: {}", e);
    } else {
        println!("HTML dashboard exported to {}", html_path.display());
    }

    println!("\nNext steps:");
    println!("  - Query: rbuilder ask \"your question\"");
    println!("  - GQL: rbuilder gql \"your query\"");
    println!("  - Stats: rbuilder stats");
    println!("  - Chat: rbuilder chat");
    println!("  - View: open {}", html_path.display());

    // Enter watch mode if requested
    if watch {
        println!("\nEntering watch mode...");
        WatchService::run_blocking(root, None)?;
    }

    Ok(())
}
