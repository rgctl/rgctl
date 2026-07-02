//! Slice direction for line-level analysis.
use clap::ValueEnum;

/// Output serialization format.
#[derive(ValueEnum, Clone, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Graphviz,
    Mermaid,
    /// Interactive HTML dashboard (graph + analysis overlays)
    #[value(name = "html-dashboard")]
    HtmlDashboard,
}

/// Slice traversal direction.
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum SliceDirection {
    #[default]
    Backward,
    Forward,
}

/// Slice result presentation.
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum SliceView {
    #[default]
    Text,
    Cfg,
    Pdg,
}

/// PDG edge layer filter for inspect.
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum PdgEdgeLayer {
    #[default]
    All,
    Data,
    Control,
}

/// File serialization format for `rbuilder export` (not the global `-f` output format).
#[derive(ValueEnum, Clone, Debug)]
pub enum ExportFormat {
    Json,
    Graphml,
    Graphviz,
    Mermaid,
}

/// Inspect layer subcommand.
#[derive(clap::Subcommand, Clone, Debug)]
pub enum InspectLayer {
    /// Control-flow graph
    Cfg {
        /// Remove unreachable blocks before display
        #[arg(long)]
        prune: bool,
    },
    /// Program dependence graph
    Pdg {
        #[arg(long, value_enum, default_value = "all")]
        edge_layer: PdgEdgeLayer,
        /// Include def-use variable lists on nodes
        #[arg(long)]
        def_use: bool,
    },
    /// Dominator tree
    Dom {
        /// Print dominance frontiers (DF)
        #[arg(long)]
        frontiers: bool,
    },
}
