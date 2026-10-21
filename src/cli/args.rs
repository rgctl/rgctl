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

/// Agent host directories for `rgctl install --skill`.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkillHost {
    /// Claude Code, Codex, and Cursor project skill dirs.
    #[default]
    All,
    /// `<repo>/.claude/skills/rgctl/`
    Claude,
    /// `<repo>/.agents/skills/rgctl/`
    Codex,
    /// `<repo>/.cursor/skills/rgctl/`
    Cursor,
}

/// File serialization format for `rgctl export` (not the global `-f` output format).
#[derive(ValueEnum, Clone, Debug)]
pub enum ExportFormat {
    Json,
    Graphml,
    Graphviz,
    Mermaid,
    Obsidian,
    Okf,
}

/// Shared `--file` / `--class` / `--line` disambiguation for symbol commands.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct SymbolTargetArgs {
    /// Restrict matches to this source file path (substring / suffix OK)
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<String>,

    /// Restrict matches to this class / type / namespace
    #[arg(long = "class", value_name = "NAME")]
    pub class: Option<String>,

    /// Restrict matches to this 1-based definition line
    #[arg(long = "line", value_name = "N")]
    pub line: Option<usize>,
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
