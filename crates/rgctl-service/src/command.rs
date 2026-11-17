//! Typed commands dispatched by [`crate::execute`].

use crate::error::{Result, ServiceError};

/// Default row/hit cap for MCP query and search when `limit` is omitted.
pub const DEFAULT_LIMIT: usize = 20;

/// CPG multiplex operations (not `export`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpgOp {
    /// Archive / field-write / AST readiness.
    Status,
    /// Function summary.
    Function,
    /// CALL neighborhood.
    Calls,
    /// Field mutations of a type.
    Mutations,
    /// Data-flow slice via CPG flows.
    Flows,
    /// Line-level slice.
    Slice,
    /// CFG inspect.
    Inspect,
    /// PDG inspect.
    Pdg,
    /// AST skeleton.
    Ast,
}

impl CpgOp {
    /// Allowed `op` strings for errors and MCP schema.
    pub const ALL: &'static [&'static str] = &[
        "status",
        "function",
        "calls",
        "mutations",
        "flows",
        "slice",
        "inspect",
        "pdg",
        "ast",
    ];

    /// Parse a tool `op` string.
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "status" => Ok(Self::Status),
            "function" => Ok(Self::Function),
            "calls" => Ok(Self::Calls),
            "mutations" => Ok(Self::Mutations),
            "flows" => Ok(Self::Flows),
            "slice" => Ok(Self::Slice),
            "inspect" => Ok(Self::Inspect),
            "pdg" => Ok(Self::Pdg),
            "ast" => Ok(Self::Ast),
            other => Err(ServiceError::unknown_op(other, Self::ALL)),
        }
    }

    /// Canonical op name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Function => "function",
            Self::Calls => "calls",
            Self::Mutations => "mutations",
            Self::Flows => "flows",
            Self::Slice => "slice",
            Self::Inspect => "inspect",
            Self::Pdg => "pdg",
            Self::Ast => "ast",
        }
    }
}

/// Semantic index query scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    /// Function symbols.
    Function,
    /// Documentation headings / code blocks.
    Docs,
    /// Community labels.
    Community,
    /// Functions and docs.
    All,
}

impl SearchScope {
    /// Parse MCP `scope` (default function).
    pub fn parse(name: Option<&str>) -> Result<Self> {
        match name.unwrap_or("function") {
            "function" => Ok(Self::Function),
            "docs" => Ok(Self::Docs),
            "community" => Ok(Self::Community),
            "all" => Ok(Self::All),
            other => Err(ServiceError::InvalidParams(format!(
                "unknown scope '{other}'. allowed: function, docs, community, all"
            ))),
        }
    }
}

/// GQL MATCH or named macro.
#[derive(Debug, Clone)]
pub struct QueryArgs {
    /// MATCH query text.
    pub query: Option<String>,
    /// Built-in macro name.
    pub macro_name: Option<String>,
    /// Collect explain plan.
    pub explain: bool,
    /// Max rows; `None` means no truncate (CLI).
    pub limit: Option<usize>,
}

/// Natural-language semantic search.
#[derive(Debug, Clone)]
pub struct SearchArgs {
    /// Query text.
    pub text: String,
    /// Index scope.
    pub scope: SearchScope,
    /// Max hits; `None` means no truncate (CLI). MCP supplies a default.
    pub limit: Option<usize>,
}

/// Blast-radius (impact) arguments.
#[derive(Debug, Clone)]
pub struct ImpactArgs {
    /// Symbol or `Class::method`.
    pub symbol: String,
    /// Caller hop cap.
    pub depth: Option<usize>,
    /// Class / namespace filter.
    pub class: Option<String>,
    /// Source file filter.
    pub file: Option<String>,
}

/// Metrics flags (MCP requires at least one).
#[derive(Debug, Clone)]
pub struct MetricsArgs {
    /// Include PageRank.
    pub pagerank: bool,
    /// Include betweenness.
    pub betweenness: bool,
    /// Include community summary.
    pub communities: bool,
    /// Include test / intrinsic nodes in PageRank hotspots.
    pub include_tests: bool,
    /// Optional PageRank iteration cap (defaults to 20).
    pub iterations: Option<usize>,
}

/// Hybrid CPG / inspect / slice.
#[derive(Debug, Clone)]
pub struct CpgArgs {
    /// Multiplexed operation.
    pub op: CpgOp,
    /// Function or inspect symbol.
    pub symbol: Option<String>,
    /// Type name for mutations.
    pub type_name: Option<String>,
    /// Source file for flows/slice.
    pub file: Option<String>,
    /// Line number.
    pub line: Option<usize>,
    /// Variable name.
    pub variable: Option<String>,
    /// Function name for flows/slice.
    pub function: Option<String>,
    /// Exclude constructors from mutations.
    pub exclude_ctors: bool,
    /// Optional member filter for mutations.
    pub member: Option<String>,
    /// Include unresolved mutation receivers.
    pub include_unresolved: bool,
    /// `forward` or `backward`.
    pub direction: Option<String>,
    /// Expand aliases on flows.
    pub with_alias: bool,
}

/// CI policy check.
#[derive(Debug, Clone)]
pub struct CheckArgs {
    /// Path to policy JSON.
    pub policy_file: String,
    /// Git base ref for diff scoping.
    pub base_ref: Option<String>,
    /// Git head ref for diff scoping.
    pub head_ref: Option<String>,
    /// Fail when diff scope is empty or unmatched.
    pub strict: bool,
}

/// Top-level command.
#[derive(Debug, Clone)]
pub enum Command {
    /// Pipeline / artifact status.
    Status,
    /// Graph query.
    Query(QueryArgs),
    /// Semantic search.
    Search(SearchArgs),
    /// Blast-radius.
    Impact(ImpactArgs),
    /// Network metrics.
    Metrics(MetricsArgs),
    /// CPG family.
    Cpg(CpgArgs),
    /// Policy check.
    Check(CheckArgs),
}

/// Registry of multiplexed family ops (extensible without new MCP tools).
#[derive(Debug, Clone)]
pub struct CommandRegistry {
    /// Enabled CPG ops.
    pub cpg_ops: Vec<&'static str>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self {
            cpg_ops: CpgOp::ALL.to_vec(),
        }
    }
}

impl CommandRegistry {
    /// Empty registry (no CPG ops).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cpg_ops: Vec::new(),
        }
    }

    /// Whether `op` is enabled.
    #[must_use]
    pub fn cpg_enabled(&self, op: CpgOp) -> bool {
        self.cpg_ops.iter().any(|name| *name == op.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_is_unknown_cpg_op() {
        let err = CpgOp::parse("export").unwrap_err();
        match err {
            ServiceError::UnknownOp { op, allowed } => {
                assert_eq!(op, "export");
                assert!(allowed.contains("slice"), "{allowed}");
                assert!(allowed.contains("status"), "{allowed}");
                assert!(!allowed.contains("export"), "{allowed}");
            }
            other => panic!("expected UnknownOp, got {other}"),
        }
    }
}
