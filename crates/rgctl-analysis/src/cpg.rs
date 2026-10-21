//! Hybrid CPG query façade (L_repo ⟷ L_proc).
//!
//! Phase 0: status + CALL/PDG/slice wraps.
//! Phase 1: `cpg_mutations` over the field-write index.
//! Phase 2: `cpg_flows` (forward/backward data slice) — see `docs/design/hybrid-cpg-plan.md`.

use crate::cfg_builder::build_cfg_for_function;
use crate::cfg_pdg_archive::{CFG_PDG_ARCHIVE_FILE, CfgPdgArchive};
use crate::field_write::{FieldWriteIndex, MutationQuery};
use crate::language_profile::cfg_language_id_from_path;
use crate::pdg::ProgramDependenceGraph;
use crate::slicing::{SliceCriterion, SliceDirection, SliceOptions, compute_slice_with_options};
use rgctl_error::{Error, Result};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{EdgeType, Node, NodeType};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Snapshot of hybrid CPG readiness for a repository.
#[derive(Debug, Clone, Serialize)]
pub struct CpgStatus {
    /// Schema version for `-f json` consumers.
    pub schema_version: u32,
    /// Absolute path checked for the CFG/PDG archive.
    pub archive_path: String,
    /// Whether `.rgctl/analysis/cfg_pdg.archive.bin` exists and loads.
    pub archive_present: bool,
    /// Functions with archived CFG/PDG (0 if archive missing).
    pub function_count: usize,
    /// Graph digest recorded in the archive header, if any.
    pub graph_digest: Option<String>,
    /// Whether the field-write mutation index is present.
    pub field_write_index_present: bool,
    /// Number of indexed field writes (0 if missing).
    pub field_write_count: usize,
    /// Whether AST skeleton archive is present.
    pub ast_skeleton_present: bool,
    /// Functions with AST skeletons.
    pub ast_skeleton_count: usize,
}

/// L_repo function summary joined with L_proc availability.
#[derive(Debug, Clone, Serialize)]
pub struct CpgFunctionInfo {
    /// Schema version for `-f json` consumers.
    pub schema_version: u32,
    /// Function node id.
    pub id: String,
    /// Simple name.
    pub name: String,
    /// Qualified name when known.
    pub qualified_name: Option<String>,
    /// Source file.
    pub file_path: Option<String>,
    /// Start line.
    pub start_line: Option<usize>,
    /// Whether a CFG/PDG archive record exists for this function.
    pub has_l_proc: bool,
    /// Constructor flag from extract metadata (`is_constructor`).
    pub is_constructor: bool,
}

/// One CALL edge neighbor.
#[derive(Debug, Clone, Serialize)]
pub struct CpgCallEdge {
    /// Callee or caller id.
    pub id: String,
    /// Neighbor name.
    pub name: String,
    /// Direction relative to the queried symbol.
    pub direction: &'static str,
}

/// CALL neighborhood for a function.
#[derive(Debug, Clone, Serialize)]
pub struct CpgCallsInfo {
    /// Schema version for `-f json` consumers.
    pub schema_version: u32,
    /// Queried function id.
    pub function_id: String,
    /// Queried function name.
    pub function_name: String,
    /// Outgoing and incoming CALL edges.
    pub edges: Vec<CpgCallEdge>,
}

/// Default archive path under a repo root.
pub fn archive_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".rgctl")
        .join("analysis")
        .join(CFG_PDG_ARCHIVE_FILE)
}

/// Load CPG status (archive presence + counts). Does not require a live graph.
pub fn cpg_status(repo_root: &Path) -> Result<CpgStatus> {
    let path = archive_path(repo_root);
    let path_str = path.display().to_string();
    let (fw_present, fw_count) = match FieldWriteIndex::open_if_exists(repo_root) {
        Ok(Some(idx)) => (true, idx.writes.len()),
        _ => (false, 0),
    };
    let (ast_present, ast_count) =
        match crate::ast_skeleton::AstSkeletonArchive::open_if_exists(repo_root) {
            Ok(Some(a)) => (true, a.records.len()),
            _ => (false, 0),
        };
    if !path.is_file() {
        return Ok(CpgStatus {
            schema_version: 1,
            archive_path: path_str,
            archive_present: false,
            function_count: 0,
            graph_digest: None,
            field_write_index_present: fw_present,
            field_write_count: fw_count,
            ast_skeleton_present: ast_present,
            ast_skeleton_count: ast_count,
        });
    }
    match CfgPdgArchive::load_from_path(&path) {
        Ok(archive) => Ok(CpgStatus {
            schema_version: 1,
            archive_path: path_str,
            archive_present: true,
            function_count: archive.records.len(),
            graph_digest: archive.graph_digest,
            field_write_index_present: fw_present,
            field_write_count: fw_count,
            ast_skeleton_present: ast_present,
            ast_skeleton_count: ast_count,
        }),
        Err(e) => Err(Error::SerdeError(format!(
            "failed to load CFG/PDG archive at {path_str}: {e}"
        ))),
    }
}

fn function_matches(node: &Node, symbol: &str) -> bool {
    if node.node_type != NodeType::Function {
        return false;
    }
    node.name == symbol
        || node.qualified_name.as_deref() == Some(symbol)
        || node.qualified_name.as_deref().is_some_and(|q| {
            q.ends_with(&format!("::{symbol}")) || q.ends_with(&format!(".{symbol}"))
        })
}

/// Resolve a function by simple name, FQN, or UUID string.
pub fn find_function_nodes(backend: &MemoryBackend, symbol: &str) -> Result<Vec<Node>> {
    if let Ok(id) = Uuid::parse_str(symbol) {
        if let Some(node) = backend.get_node(id)? {
            if node.node_type == NodeType::Function {
                return Ok(vec![node]);
            }
        }
    }

    let mut matches: Vec<Node> = backend
        .find_nodes_by_name(symbol)?
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .collect();

    if matches.is_empty() {
        matches = backend
            .all_nodes()?
            .into_iter()
            .filter(|n| function_matches(n, symbol))
            .collect();
    } else {
        // Also accept FQN equality when name index returns simple-name hits only.
        let qn_extra: Vec<Node> = backend
            .all_nodes()?
            .into_iter()
            .filter(|n| {
                n.node_type == NodeType::Function
                    && n.qualified_name.as_deref() == Some(symbol)
                    && !matches.iter().any(|m| m.id == n.id)
            })
            .collect();
        matches.extend(qn_extra);
    }

    Ok(matches)
}

fn require_unique_function(backend: &MemoryBackend, symbol: &str) -> Result<Node> {
    let matches = find_function_nodes(backend, symbol)?;
    match matches.as_slice() {
        [n] => Ok(n.clone()),
        [] => Err(Error::NotFound(format!("no function matching '{symbol}'"))),
        many => Err(Error::AmbiguousSymbol {
            name: symbol.to_string(),
            count: many.len(),
            candidates: many
                .iter()
                .map(|n| rgctl_error::SymbolCandidate {
                    id: n.id.to_string(),
                    name: n.name.to_string(),
                    qualified_name: n.qualified_name.as_ref().map(|s| s.to_string()),
                    node_type: "function".into(),
                    file: n.file_path.as_ref().map(|s| s.to_string()),
                    line: n.start_line,
                })
                .collect(),
        }),
    }
}

/// Build [`CpgFunctionInfo`] for a unique symbol (errors if 0 or many matches).
pub fn cpg_function(
    backend: &MemoryBackend,
    repo_root: &Path,
    symbol: &str,
) -> Result<CpgFunctionInfo> {
    let node = require_unique_function(backend, symbol)?;
    let has_l_proc = archive_has_function(repo_root, node.id);
    let is_constructor = node
        .properties
        .get("is_constructor")
        .map(|v| v == "true")
        .unwrap_or(false)
        || node
            .qualified_name
            .as_deref()
            .is_some_and(|q| q.ends_with(".<init>") || q.contains("::<init>"));

    Ok(CpgFunctionInfo {
        schema_version: 1,
        id: node.id.to_string(),
        name: node.name.to_string(),
        qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
        file_path: node.file_path.as_ref().map(|s| s.to_string()),
        start_line: node.start_line,
        has_l_proc,
        is_constructor,
    })
}

fn archive_has_function(repo_root: &Path, function_id: Uuid) -> bool {
    let path = archive_path(repo_root);
    if !path.is_file() {
        return false;
    }
    CfgPdgArchive::load_from_path(&path)
        .map(|a| a.records.contains_key(&function_id))
        .unwrap_or(false)
}

/// CALL in/out neighborhood for a function symbol.
pub fn cpg_calls(backend: &MemoryBackend, symbol: &str) -> Result<CpgCallsInfo> {
    let node = require_unique_function(backend, symbol)?;

    let mut edges = Vec::new();
    for edge in backend.get_outgoing_edges(node.id)? {
        if edge.edge_type != EdgeType::Calls {
            continue;
        }
        if let Some(cal) = backend.get_node(edge.to)? {
            edges.push(CpgCallEdge {
                id: cal.id.to_string(),
                name: cal.name.to_string(),
                direction: "out",
            });
        }
    }
    for edge in backend.get_incoming_edges(node.id)? {
        if edge.edge_type != EdgeType::Calls {
            continue;
        }
        if let Some(caller) = backend.get_node(edge.from)? {
            edges.push(CpgCallEdge {
                id: caller.id.to_string(),
                name: caller.name.to_string(),
                direction: "in",
            });
        }
    }

    Ok(CpgCallsInfo {
        schema_version: 1,
        function_id: node.id.to_string(),
        function_name: node.name.to_string(),
        edges,
    })
}

/// Result of a `cpg mutations` query.
#[derive(Debug, Clone, Serialize)]
pub struct CpgMutationsResult {
    /// Schema version for `-f json` consumers.
    pub schema_version: u32,
    /// Queried type name.
    pub type_name: String,
    /// Whether constructors were excluded.
    pub exclude_ctors: bool,
    /// Optional member filter.
    pub member: Option<String>,
    /// Whether unresolved writes were included.
    pub include_unresolved: bool,
    /// Matching write sites.
    pub mutations: Vec<CpgMutationHit>,
}

/// One mutation hit for agents.
#[derive(Debug, Clone, Serialize)]
pub struct CpgMutationHit {
    /// Source file.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// Statement text.
    pub code: String,
    /// Field name.
    pub member: String,
    /// Enclosing function.
    pub function: String,
    /// Constructor write.
    pub is_constructor: bool,
    /// Receiver local name.
    pub receiver_local: Option<String>,
    /// Resolved receiver type.
    pub receiver_type: Option<String>,
    /// [`crate::field_write::FieldWriteKind`] debug name.
    pub kind: String,
}

/// Query the field-write index (requires `discover --with-cfg`).
pub fn cpg_mutations(repo_root: &Path, query: MutationQuery) -> Result<CpgMutationsResult> {
    let path = FieldWriteIndex::default_path(repo_root);
    if !path.is_file() {
        return Err(Error::NotFound(format!(
            "field_write index not found at {} (run `rgctl discover --with-cfg`)",
            path.display()
        )));
    }
    let index = FieldWriteIndex::load_from_path(&path)?;
    let type_name = query.type_name.clone();
    let exclude_ctors = query.exclude_ctors;
    let member = query.member.clone();
    let include_unresolved = query.include_unresolved;
    let mutations = index
        .query(&query)
        .into_iter()
        .map(|w| CpgMutationHit {
            file: w.file.clone(),
            line: w.line,
            code: w.code_snippet.clone(),
            member: w.member.clone(),
            function: w.function_name.clone(),
            is_constructor: w.is_constructor,
            receiver_local: w.receiver_local.clone(),
            receiver_type: w.receiver_type.clone(),
            kind: format!("{:?}", w.kind),
        })
        .collect();
    Ok(CpgMutationsResult {
        schema_version: 1,
        type_name,
        exclude_ctors,
        member,
        include_unresolved,
        mutations,
    })
}

/// Arguments for [`cpg_flows`].
#[derive(Debug, Clone)]
pub struct CpgFlowsArgs {
    /// Repository root (resolves relative file paths).
    pub repo_root: PathBuf,
    /// Source file (absolute or repo-relative).
    pub file: String,
    /// 1-based criterion line.
    pub line: usize,
    /// Variable / local name (also matches `var.field` defs).
    pub variable: String,
    /// Enclosing method name (required for unambiguous CFG).
    pub function: String,
    /// Language id override; inferred from extension when `None`.
    pub language: Option<String>,
    /// Forward ≈ reachableByFlows; backward ≈ classic slice.
    pub direction: SliceDirection,
    /// Expand criterion via may-alias heuristics (P3 T2).
    pub with_alias: bool,
}

/// One statement in a flow / slice.
#[derive(Debug, Clone, Serialize)]
pub struct CpgFlowStep {
    /// 1-based line.
    pub line: usize,
    /// Statement text.
    pub code: String,
}

/// Result of `cpg flows`.
#[derive(Debug, Clone, Serialize)]
pub struct CpgFlowsResult {
    /// Schema version for `-f json` consumers.
    pub schema_version: u32,
    /// Resolved source path.
    pub file: String,
    /// Function analyzed.
    pub function: String,
    /// Criterion variable.
    pub variable: String,
    /// Criterion line.
    pub line: usize,
    /// `forward` or `backward`.
    pub direction: String,
    /// Ordered flow steps (by line).
    pub steps: Vec<CpgFlowStep>,
    /// Distinct lines in the slice.
    pub lines: Vec<usize>,
    /// Reduction vs function statement count.
    pub reduction_percent: f64,
}

fn resolve_source_file(repo_root: &Path, file: &str) -> Result<PathBuf> {
    let as_given = PathBuf::from(file);
    if as_given.is_file() {
        return Ok(as_given);
    }
    if as_given.is_absolute() {
        return Err(Error::NotFound(format!(
            "source file not found: {}",
            as_given.display()
        )));
    }
    let under_repo = repo_root.join(file);
    if under_repo.is_file() {
        return Ok(under_repo);
    }
    Err(Error::NotFound(format!(
        "source file not found: {file} (also tried {})",
        under_repo.display()
    )))
}

/// Compute a forward/backward data+control slice (OrderDTO Turn 5).
pub fn cpg_flows(args: CpgFlowsArgs) -> Result<CpgFlowsResult> {
    let path = resolve_source_file(&args.repo_root, &args.file)?;
    let source = fs::read_to_string(&path)?;
    let lang = args.language.clone().unwrap_or_else(|| {
        cfg_language_id_from_path(&path)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".into())
    });
    let cfg = build_cfg_for_function(&lang, &source, &args.function)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let slice = compute_slice_with_options(
        &pdg,
        &cfg,
        SliceCriterion {
            variable: args.variable.clone(),
            line: args.line,
        },
        args.direction,
        SliceOptions {
            with_alias: args.with_alias,
        },
    )?;

    let mut steps: Vec<CpgFlowStep> = slice
        .statements
        .iter()
        .filter_map(|id| {
            let n = pdg.nodes.get(id)?;
            Some(CpgFlowStep {
                line: n.statement.line,
                code: n.statement.text.clone(),
            })
        })
        .collect();
    steps.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.code.cmp(&b.code)));
    steps.dedup_by(|a, b| a.line == b.line && a.code == b.code);

    let mut lines: Vec<usize> = slice.lines.iter().copied().collect();
    lines.sort_unstable();

    Ok(CpgFlowsResult {
        schema_version: 1,
        file: path.display().to_string(),
        function: args.function,
        variable: args.variable,
        line: args.line,
        direction: match args.direction {
            SliceDirection::Forward => "forward".into(),
            SliceDirection::Backward => "backward".into(),
        },
        steps,
        lines,
        reduction_percent: slice.reduction_percent,
    })
}

#[cfg(test)]
mod flows_tests {
    use super::*;

    #[test]
    fn cpg_flows_forward_java_order() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("OrderProcessor.java");
        let source = r#"
public class OrderProcessor {
    public OrderDTO process(OrderDTO order) {
        order.status = "PROCESSED";
        return order;
    }
}
"#;
        std::fs::write(&file, source).unwrap();
        let cfg = build_cfg_for_function("java", source, "process").unwrap();
        let write_line = cfg
            .blocks
            .values()
            .flat_map(|b| &b.statements)
            .find(|s| s.defines("order.status"))
            .map(|s| s.line)
            .expect("write");
        let result = cpg_flows(CpgFlowsArgs {
            repo_root: dir.path().to_path_buf(),
            file: file.display().to_string(),
            line: write_line,
            variable: "order".into(),
            function: "process".into(),
            language: Some("java".into()),
            direction: SliceDirection::Forward,
            with_alias: false,
        })
        .unwrap();
        assert!(result.lines.contains(&write_line));
        assert!(!result.steps.is_empty());
    }
}
