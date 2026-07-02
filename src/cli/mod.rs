//! rBuilder CLI command definitions and dispatch.

mod args;
mod blast_radius;
mod check;
mod context;
mod discover;
mod discover_impl;
mod export;
mod gql;
mod inspect;
mod metrics;
mod policy_file;
mod slice;

pub use args::OutputFormat;

use args::{ExportFormat, InspectLayer, SliceDirection, SliceView};
use clap::{Parser, Subcommand};
use context::CliContext;
use crate::BUILD_INFO;

#[derive(Parser)]
#[command(name = "rbuilder")]
#[command(about = "AI-powered code knowledge graph", version = BUILD_INFO)]
pub struct Cli {
    /// Path to the graph cache database
    #[arg(short = 'd', long = "db", global = true)]
    pub db: Option<std::path::PathBuf>,

    /// Target repository root
    #[arg(short = 'r', long = "repo", global = true)]
    pub repo: Option<std::path::PathBuf>,

    /// Output format
    #[arg(short = 'f', long = "format", value_enum, global = true)]
    pub format: Option<OutputFormat>,

    /// Write output to file instead of stdout
    #[arg(short = 'o', long = "output", global = true)]
    pub output: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Index and analyze a codebase
    Discover {
        /// Repository path (defaults to --repo or cwd)
        #[arg(value_name = "PATH")]
        path: Option<String>,

        #[arg(short = 'l', long = "languages")]
        languages: Option<String>,

        #[arg(short = 'e', long = "exclude")]
        exclude: Option<String>,

        #[arg(short = 'v', long = "verbose")]
        verbose: bool,

        #[arg(long = "security")]
        security: bool,

        #[arg(long = "cfg")]
        cfg: bool,

        #[arg(long = "all")]
        all: bool,
    },

    /// Execute graph query language
    Gql {
        query: String,

        #[arg(long)]
        explain: bool,

        #[arg(long)]
        macro_name: Option<String>,
    },

    /// Line-level program slice or taint trace
    Slice {
        file: String,

        #[arg(long)]
        line: usize,

        #[arg(long)]
        variable: String,

        #[arg(long)]
        function: Option<String>,

        #[arg(long)]
        language: Option<String>,

        #[arg(long, value_enum, default_value = "backward")]
        direction: SliceDirection,

        #[arg(long)]
        taint: bool,

        #[arg(long, value_enum, default_value = "text")]
        view: SliceView,
    },

    /// Macro impact / blast radius for a symbol
    BlastRadius {
        symbol: String,

        #[arg(long)]
        depth: Option<usize>,

        #[arg(long)]
        policy_file: Option<String>,

        #[arg(long)]
        no_policy: bool,
    },

    /// Inspect raw CFG / PDG / dominance for a function symbol
    Inspect {
        symbol: String,
        #[command(subcommand)]
        layer: InspectLayer,
    },

    /// Network analytics (PageRank, betweenness, communities)
    Metrics {
        #[arg(long)]
        pagerank: bool,

        #[arg(long)]
        betweenness: bool,

        #[arg(long)]
        communities: bool,

        #[arg(long)]
        iterations: Option<usize>,
    },

    /// CI policy gateway
    Check {
        #[arg(long)]
        policy_file: String,
    },

    /// Export graph or projections (use global `-f html-dashboard` for the HTML dashboard)
    Export {
        #[arg(long = "export-format", value_enum)]
        export_format: ExportFormat,

        #[arg(long = "export-output", value_name = "FILE")]
        export_output: String,

        #[arg(long, default_value = "all")]
        query: String,
    },
}

impl Cli {
    pub fn run(self) -> anyhow::Result<()> {
        let verbose = matches!(self.command, Commands::Discover { verbose: true, .. });
        init_logging(verbose);

        let ctx = CliContext::new(
            self.repo,
            self.db,
            self.format.unwrap_or_default(),
            self.output,
            verbose,
        );

        let result = match self.command {
            Commands::Discover {
                path,
                languages,
                exclude,
                verbose: _,
                security,
                cfg,
                all,
            } => discover::run(
                &ctx,
                discover::DiscoverArgs {
                    path,
                    languages,
                    exclude,
                    security,
                    cfg,
                    all,
                },
            ),
            Commands::Gql {
                query,
                explain,
                macro_name,
            } => gql::run(
                &ctx,
                gql::GqlArgs {
                    query,
                    explain,
                    macro_name,
                },
            ),
            Commands::Slice {
                file,
                line,
                variable,
                function,
                language,
                direction,
                taint,
                view,
            } => slice::run(
                &ctx,
                slice::SliceArgs {
                    file,
                    line,
                    variable,
                    function,
                    language,
                    direction,
                    taint,
                    view,
                },
            ),
            Commands::BlastRadius {
                symbol,
                depth,
                policy_file,
                no_policy,
            } => blast_radius::run(
                &ctx,
                blast_radius::BlastRadiusArgs {
                    symbol,
                    depth,
                    policy_file,
                    no_policy,
                },
            ),
            Commands::Inspect { symbol, layer } => {
                inspect::run(&ctx, inspect::InspectArgs { symbol, layer })
            }
            Commands::Metrics {
                pagerank,
                betweenness,
                communities,
                iterations,
            } => metrics::run(
                &ctx,
                metrics::MetricsArgs {
                    pagerank,
                    betweenness,
                    communities,
                    iterations,
                },
            ),
            Commands::Check { policy_file } => {
                check::run(&ctx, check::CheckArgs { policy_file })
            }
            Commands::Export {
                export_format,
                export_output,
                query,
            } => export::run(
                &ctx,
                export::ExportArgs {
                    export_format,
                    export_output,
                    query,
                },
            ),
        };

        if ctx.is_html_dashboard() {
            ctx.emit_html_dashboard()?;
        }

        result
    }
}

fn init_logging(verbose: bool) {
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::EnvFilter;

    if verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info,rbuilder=debug")),
            )
            .with_target(true)
            .with_level(true)
            .with_span_events(FmtSpan::CLOSE)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("warn,rbuilder=info,rbuilder_extraction=warn,rbuilder_analysis=warn")
            }))
            .with_target(false)
            .with_level(false)
            .with_ansi(true)
            .without_time()
            .init();
    }
}
