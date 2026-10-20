//! Field-write facts for hybrid CPG mutation queries (Phase 1).
//!
//! Extracts `obj.member = …` / `this.member = …` from CFG statement `defined_vars`
//! (compound names produced by [`crate::def_use`]), then best-effort types receivers
//! from function parameters + language-specific locals.

use crate::cfg::{ControlFlowGraph, DefVar};
use crate::cfg_pdg_archive::CfgPdgArchive;
use crate::field_write_locals::{build_file_local_types_index, language_from_path};
use rgctl_error::{Error, Result};
use rgctl_graph::schema::Node;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use rayon::prelude::*;

/// On-disk filename under `.rgctl/analysis/`.
pub const FIELD_WRITE_INDEX_FILE: &str = "field_write.index.bin";
/// Index format version.
pub const FIELD_WRITE_INDEX_VERSION: u32 = 1;

/// Classification of a field write site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldWriteKind {
    /// `obj.field =` with a resolved receiver type.
    DirectField,
    /// `this.field =` (or language equivalent) with known enclosing type.
    ThisField,
    /// Receiver type could not be resolved.
    Unresolved,
}

/// One field assignment site in L_proc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWrite {
    /// Enclosing function UUID (L_repo).
    pub function_id: Uuid,
    /// Enclosing function simple name.
    pub function_name: String,
    /// True when the enclosing function is a constructor / `<init>`.
    pub is_constructor: bool,
    /// Base local (`order`, `this`), when known.
    pub receiver_local: Option<String>,
    /// Resolved type name (`OrderDTO`), when known.
    pub receiver_type: Option<String>,
    /// Field / member name (`status`).
    pub member: String,
    /// Source file path.
    pub file: String,
    /// 1-based line.
    pub line: usize,
    /// Statement text snippet.
    pub code_snippet: String,
    /// Resolution kind.
    pub kind: FieldWriteKind,
}

/// Compact mutation index for `cpg mutations`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FieldWriteIndex {
    /// Format version.
    pub version: u32,
    /// Graph digest when built (optional invalidation hint).
    pub graph_digest: Option<String>,
    /// All write sites.
    pub writes: Vec<FieldWrite>,
}

/// Filters for [`FieldWriteIndex::query`].
#[derive(Debug, Clone, Default)]
pub struct MutationQuery {
    /// Required receiver type (simple or FQN suffix match).
    pub type_name: String,
    /// Drop constructor writes when true.
    pub exclude_ctors: bool,
    /// Optional member name filter.
    pub member: Option<String>,
    /// Include [`FieldWriteKind::Unresolved`] rows.
    pub include_unresolved: bool,
}

impl FieldWriteIndex {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(".rgctl")
            .join("analysis")
            .join(FIELD_WRITE_INDEX_FILE)
    }

    /// Build from a CFG/PDG archive and L_repo function nodes.
    ///
    /// Groups by source file and processes files in parallel (caller should install
    /// [`rgctl_pipeline::with_large_pool`] when available).
    pub fn build_from_archive(
        archive: &CfgPdgArchive,
        functions: &[Node],
        graph_digest: Option<String>,
        repo_root: Option<&Path>,
        preloaded_sources: Option<&HashMap<String, Arc<String>>>,
    ) -> Self {
        let by_id: HashMap<Uuid, &Node> = functions.iter().map(|n| (n.id, n)).collect();

        let mut by_file: HashMap<String, Vec<Uuid>> = HashMap::new();
        for record in archive.records.values() {
            let func = by_id.get(&record.function_id).copied();
            let file = record
                .file_path
                .clone()
                .or_else(|| func.and_then(|n| n.file_path.as_ref().map(|s| s.to_string())))
                .unwrap_or_default();
            by_file
                .entry(file)
                .or_default()
                .push(record.function_id);
        }

        let repo_root = repo_root.map(Path::to_path_buf);
        let file_groups: Vec<(String, Vec<Uuid>)> = by_file.into_iter().collect();

        let mut writes: Vec<FieldWrite> = file_groups
            .into_par_iter()
            .flat_map(|(file, ids)| {
                process_file_records(
                    &file,
                    &ids,
                    archive,
                    &by_id,
                    repo_root.as_deref(),
                    preloaded_sources,
                )
            })
            .collect();

        writes.sort_by(|a, b| {
            (&a.file, a.line, &a.function_id, &a.member, &a.code_snippet).cmp(&(
                &b.file,
                b.line,
                &b.function_id,
                &b.member,
                &b.code_snippet,
            ))
        });

        Self {
            version: FIELD_WRITE_INDEX_VERSION,
            graph_digest,
            writes,
        }
    }

    /// Persist with a small magic header.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = bincode::serialize(self)
            .map_err(|e| Error::SerdeError(format!("field_write index: {e}")))?;
        let mut bytes = Vec::with_capacity(8 + payload.len());
        bytes.extend_from_slice(b"RBFW");
        bytes.extend_from_slice(&FIELD_WRITE_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&payload);
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Load from disk.
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        if bytes.len() < 8 || &bytes[0..4] != b"RBFW" {
            return Err(Error::SerdeError("invalid field_write index magic".into()));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        if version != FIELD_WRITE_INDEX_VERSION {
            return Err(Error::SerdeError(format!(
                "unsupported field_write index version {version}"
            )));
        }
        bincode::deserialize(&bytes[8..])
            .map_err(|e| Error::SerdeError(format!("field_write index: {e}")))
    }

    /// Open if present.
    pub fn open_if_exists(repo_root: &Path) -> Result<Option<Self>> {
        let path = Self::default_path(repo_root);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(Self::load_from_path(&path)?))
    }

    /// Filter write sites for the OrderDTO-style mutation query.
    pub fn query(&self, q: &MutationQuery) -> Vec<&FieldWrite> {
        let want = normalize_type_name(&q.type_name);
        self.writes
            .iter()
            .filter(|w| {
                if q.exclude_ctors && w.is_constructor {
                    return false;
                }
                if let Some(m) = &q.member {
                    if w.member != *m {
                        return false;
                    }
                }
                match w.kind {
                    FieldWriteKind::Unresolved => {
                        if !q.include_unresolved {
                            return false;
                        }
                        // Unresolved rows never match a concrete type filter unless
                        // the caller only wants the unresolved bucket (empty type).
                        want.is_empty()
                    }
                    FieldWriteKind::DirectField | FieldWriteKind::ThisField => w
                        .receiver_type
                        .as_ref()
                        .map(|t| type_matches(t, &want))
                        .unwrap_or(false),
                }
            })
            .collect()
    }
}

fn type_matches(have: &str, want: &str) -> bool {
    if want.is_empty() {
        return true;
    }
    let have_n = normalize_type_name(have);
    have_n == want
        || have_n
            .strip_suffix(want)
            .is_some_and(|prefix| prefix.ends_with('.') || prefix.ends_with("::"))
}

fn normalize_type_name(name: &str) -> String {
    let bare = name.split('<').next().unwrap_or(name).trim();
    let bare = bare
        .trim_start_matches("const ")
        .trim()
        .trim_start_matches('*')
        .trim()
        .trim_start_matches("&mut ")
        .trim_start_matches('&')
        .trim();
    bare.rsplit('.')
        .next()
        .unwrap_or(bare)
        .rsplit("::")
        .next()
        .unwrap_or(bare)
        .rsplit('\\')
        .next()
        .unwrap_or(bare)
        .trim()
        .to_string()
}

fn is_constructor_node(node: &Node) -> bool {
    node.properties
        .get("is_constructor")
        .map(|v| v == "true")
        .unwrap_or(false)
        || node
            .qualified_name
            .as_deref()
            .is_some_and(|q| q.ends_with(".<init>") || q.contains("::<init>"))
}

fn function_name_looks_like_ctor(name: &str, func: Option<&Node>) -> bool {
    if let Some(node) = func {
        if let Some(enclosing) = enclosing_type_name(node) {
            return name == enclosing;
        }
    }
    false
}

fn enclosing_type_name(node: &Node) -> Option<String> {
    if let Some(qn) = &node.qualified_name {
        if qn.ends_with(".<init>") || qn.contains("::<init>") {
            if let Some(owner) = qn.strip_suffix(".<init>") {
                return Some(owner.to_string());
            }
        }
        if let Some((owner, _)) = qn.rsplit_once('#') {
            return Some(owner.to_string());
        }
        if let Some((owner, _)) = qn.rsplit_once('.') {
            return Some(owner.to_string());
        }
        if let Some((owner, _)) = qn.rsplit_once("::") {
            return Some(owner.to_string());
        }
    }
    node.properties.get("member_of").cloned()
}

fn process_file_records(
    file: &str,
    ids: &[Uuid],
    archive: &CfgPdgArchive,
    by_id: &HashMap<Uuid, &Node>,
    repo_root: Option<&Path>,
    preloaded_sources: Option<&HashMap<String, Arc<String>>>,
) -> Vec<FieldWrite> {
    let mut function_names = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(record) = archive.records.get(id) else {
            continue;
        };
        let func = by_id.get(id).copied();
        let function_name = if record.function_name.is_empty() {
            func.map(|n| n.name.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            record.function_name.clone()
        };
        function_names.push(function_name);
    }

    let source = preloaded_sources
        .and_then(|m| m.get(file).cloned())
        .or_else(|| load_source(file, repo_root).map(Arc::new));
    let file_types = source.as_ref().map(|src| {
        build_file_local_types_index(&language_from_path(file), src.as_str(), &function_names)
    });

    let mut file_writes = Vec::new();
    let mut type_env = HashMap::new();
    for (id, function_name) in ids.iter().zip(function_names.iter()) {
        let Some(record) = archive.records.get(id) else {
            continue;
        };
        let func = by_id.get(id).copied();
        let is_constructor = func.map(is_constructor_node).unwrap_or(false)
            || function_name_looks_like_ctor(function_name, func);
        type_env.clear();
        merge_type_env(func, &record.cfg, function_name, &mut type_env);
        if let Some(locals) = file_types.as_ref().and_then(|m| m.get(function_name)) {
            for (k, v) in locals {
                type_env.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        extract_writes_from_cfg(
            &record.cfg,
            *id,
            function_name,
            is_constructor,
            file,
            &type_env,
            &mut file_writes,
        );
    }
    file_writes
}

fn merge_type_env(
    func: Option<&Node>,
    cfg: &ControlFlowGraph,
    _function_name: &str,
    type_env: &mut HashMap<String, String>,
) {
    *type_env = type_env_from_node(func);
    if let Some(node) = func {
        if let Some(enclosing) = enclosing_type_name(node) {
            type_env.insert("this".into(), enclosing.clone());
            type_env.insert("self".into(), enclosing);
        }
    }
    for (k, v) in &cfg.local_types {
        type_env.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

fn type_env_from_node(func: Option<&Node>) -> HashMap<String, String> {
    let mut env = HashMap::new();
    let Some(node) = func else {
        return env;
    };
    for p in &node.parameters {
        if let Some(ty) = &p.param_type {
            env.insert(p.name.clone(), normalize_type_name(ty));
        }
    }
    env
}

fn load_source(file: &str, repo_root: Option<&Path>) -> Option<String> {
    if file.is_empty() {
        return None;
    }
    let path = Path::new(file);
    if path.is_file() {
        return fs::read_to_string(path).ok();
    }
    if let Some(root) = repo_root {
        let joined = root.join(file);
        if joined.is_file() {
            return fs::read_to_string(joined).ok();
        }
    }
    None
}

/// Extract field-write rows from a hot in-memory CFG (fused discover path).
pub fn extract_writes_from_cfg(
    cfg: &ControlFlowGraph,
    function_id: Uuid,
    function_name: &str,
    is_constructor: bool,
    file: &str,
    type_env: &HashMap<String, String>,
    out: &mut Vec<FieldWrite>,
) {
    for block in cfg.blocks.values() {
        for stmt in &block.statements {
            for def in &stmt.defined_vars {
                let DefVar::Field { receiver, member } = def else {
                    continue;
                };
                if member.is_empty() {
                    continue;
                }
                let receiver_key = receiver.trim_start_matches('$');
                let receiver_type = type_env
                    .get(receiver.as_str())
                    .or_else(|| type_env.get(receiver_key))
                    .cloned();
                let kind = if receiver == "this" || receiver == "self" {
                    if receiver_type.is_some() {
                        FieldWriteKind::ThisField
                    } else {
                        FieldWriteKind::Unresolved
                    }
                } else if receiver_type.is_some() {
                    FieldWriteKind::DirectField
                } else {
                    FieldWriteKind::Unresolved
                };
                out.push(FieldWrite {
                    function_id,
                    function_name: function_name.to_string(),
                    is_constructor,
                    receiver_local: Some(receiver.clone()),
                    receiver_type,
                    member: member.clone(),
                    file: file.to_string(),
                    line: stmt.line,
                    code_snippet: stmt.text.clone(),
                    kind,
                });
            }
        }
    }
}

/// Build index from archive on disk (or in-memory) and write beside the archive.
pub fn build_and_save_field_write_index(
    repo_root: &Path,
    archive: &CfgPdgArchive,
    functions: &[Node],
    graph_digest: Option<String>,
    preloaded_sources: Option<&HashMap<String, Arc<String>>>,
) -> Result<(PathBuf, usize)> {
    let index = FieldWriteIndex::build_from_archive(
        archive,
        functions,
        graph_digest,
        Some(repo_root),
        preloaded_sources,
    );
    let count = index.writes.len();
    let path = FieldWriteIndex::default_path(repo_root);
    index.write_to_path(&path)?;
    Ok((path, count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;
    use crate::cfg::{ControlFlowGraph, Statement, StatementKind};
    use std::collections::HashSet;
    use uuid::Uuid;

    fn cfg_with_assign(line: usize, text: &str, defined: &[&str]) -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();
        let id = cfg.entry;
        let mut defined_vars = HashSet::new();
        for d in defined {
            if let Some((receiver, member)) = d.rsplit_once('.') {
                defined_vars.insert(DefVar::Field {
                    receiver: receiver.to_string(),
                    member: member.to_string(),
                });
            } else {
                defined_vars.insert(DefVar::local(*d));
            }
        }
        if let Some(block) = cfg.blocks.get_mut(&id) {
            block.statements.push(Statement {
                kind: StatementKind::Assignment,
                line,
                text: text.to_string(),
                defined_vars: defined_vars.into_iter().collect(),
                used_vars: SmallVec::new(),
            });
            block.start_line = line;
            block.end_line = line;
        }
        cfg.exits = vec![id];
        cfg
    }

    #[test]
    fn query_excludes_ctors_and_matches_type() {
        let mut index = FieldWriteIndex {
            version: 1,
            graph_digest: None,
            writes: vec![
                FieldWrite {
                    function_id: Uuid::new_v4(),
                    function_name: "OrderDTO".into(),
                    is_constructor: true,
                    receiver_local: Some("this".into()),
                    receiver_type: Some("OrderDTO".into()),
                    member: "status".into(),
                    file: "OrderDTO.java".into(),
                    line: 10,
                    code_snippet: "this.status = status;".into(),
                    kind: FieldWriteKind::ThisField,
                },
                FieldWrite {
                    function_id: Uuid::new_v4(),
                    function_name: "process".into(),
                    is_constructor: false,
                    receiver_local: Some("order".into()),
                    receiver_type: Some("OrderDTO".into()),
                    member: "status".into(),
                    file: "OrderProcessor.java".into(),
                    line: 114,
                    code_snippet: "order.status = \"PROCESSED\";".into(),
                    kind: FieldWriteKind::DirectField,
                },
            ],
        };
        let hits = index.query(&MutationQuery {
            type_name: "OrderDTO".into(),
            exclude_ctors: true,
            member: None,
            include_unresolved: false,
        });
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 114);

        let _ = &mut index;
    }

    #[test]
    fn extract_uses_type_env() {
        let cfg = cfg_with_assign(5, "order.status = \"X\";", &["order.status"]);
        let mut writes = Vec::new();
        let mut env = HashMap::new();
        env.insert("order".into(), "OrderDTO".into());
        extract_writes_from_cfg(
            &cfg,
            Uuid::new_v4(),
            "process",
            false,
            "P.java",
            &env,
            &mut writes,
        );
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].member, "status");
        assert_eq!(writes[0].receiver_type.as_deref(), Some("OrderDTO"));
        assert_eq!(writes[0].kind, FieldWriteKind::DirectField);
    }

    #[test]
    fn java_cfg_captures_field_write_and_query() {
        use crate::cfg_builder::build_cfg_for_function;
        use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
        use crate::pdg::ProgramDependenceGraph;
        use rgctl_graph::schema::{GraphParameter, Node, NodeType};
        use std::sync::Arc;

        let source = r#"
public class OrderDTO {
    private String status;
    public OrderDTO(String status) {
        this.status = status;
    }
}
public class OrderProcessor {
    public OrderDTO process(OrderDTO order) {
        order.status = "PROCESSED";
        return order;
    }
}
"#;
        let cfg_ctor = build_cfg_for_function("java", source, "OrderDTO").expect("ctor cfg");
        let cfg_proc = build_cfg_for_function("java", source, "process").expect("process cfg");
        assert!(
            cfg_proc
                .blocks
                .values()
                .flat_map(|b| &b.statements)
                .any(|s| s.defines("order.status")),
            "process CFG should define order.status"
        );

        let mut order_fn = Node::new(NodeType::Function, "OrderDTO");
        order_fn.qualified_name = Some("OrderDTO.<init>".into());
        order_fn
            .properties
            .insert("is_constructor".into(), "true".into());
        order_fn.file_path = Some("OrderDTO.java".into());

        let mut process_fn = Node::new(NodeType::Function, "process");
        process_fn.qualified_name = Some("OrderProcessor.process".into());
        process_fn.file_path = Some("OrderProcessor.java".into());
        process_fn.parameters = vec![GraphParameter {
            name: "order".into(),
            param_type: Some("OrderDTO".into()),
            default_value: None,
        }];

        let id_ctor = order_fn.id;
        let id_proc = process_fn.id;
        let pdg_ctor = ProgramDependenceGraph::build(&cfg_ctor, source.as_bytes()).unwrap();
        let pdg_proc = ProgramDependenceGraph::build(&cfg_proc, source.as_bytes()).unwrap();

        let mut archive = CfgPdgArchive::default();
        archive.insert(CfgPdgRecord {
            function_id: id_ctor,
            code_hash: "a".into(),
            function_name: "OrderDTO".into(),
            file_path: Some("OrderDTO.java".into()),
            cfg: Arc::new(cfg_ctor),
            pdg: Arc::new(pdg_ctor),
        });
        archive.insert(CfgPdgRecord {
            function_id: id_proc,
            code_hash: "b".into(),
            function_name: "process".into(),
            file_path: Some("OrderProcessor.java".into()),
            cfg: Arc::new(cfg_proc),
            pdg: Arc::new(pdg_proc),
        });

        let index =
            FieldWriteIndex::build_from_archive(&archive, &[order_fn, process_fn], None, None, None);
        let hits = index.query(&MutationQuery {
            type_name: "OrderDTO".into(),
            exclude_ctors: true,
            member: None,
            include_unresolved: false,
        });
        assert_eq!(hits.len(), 1, "hits={hits:?}");
        assert!(hits[0].code_snippet.contains("order.status"));
        assert!(!hits[0].is_constructor);
    }

    #[test]
    fn same_file_two_functions_parse_once_resolves_locals() {
        use crate::cfg_builder::build_cfg_for_function;
        use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
        use crate::pdg::ProgramDependenceGraph;
        use rgctl_graph::schema::{Node, NodeType};
        use std::sync::Arc;
        use tempfile::TempDir;

        let source = r#"
public class Dual {
    public void first(OrderDTO a) {
        OrderDTO x = a;
        x.status = "A";
    }
    public void second(OrderDTO b) {
        OrderDTO y = b;
        y.status = "B";
    }
}
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Dual.java");
        std::fs::write(&path, source).unwrap();
        let file = path.to_string_lossy().to_string();

        let cfg_first = build_cfg_for_function("java", source, "first").expect("first cfg");
        let cfg_second = build_cfg_for_function("java", source, "second").expect("second cfg");

        let mut first_fn = Node::new(NodeType::Function, "first");
        first_fn.file_path = Some(file.clone().into());
        let mut second_fn = Node::new(NodeType::Function, "second");
        second_fn.file_path = Some(file.clone().into());

        let id_first = first_fn.id;
        let id_second = second_fn.id;
        let pdg_first = ProgramDependenceGraph::build(&cfg_first, source.as_bytes()).unwrap();
        let pdg_second = ProgramDependenceGraph::build(&cfg_second, source.as_bytes()).unwrap();

        let mut archive = CfgPdgArchive::default();
        archive.insert(CfgPdgRecord {
            function_id: id_first,
            code_hash: "1".into(),
            function_name: "first".into(),
            file_path: Some(file.clone()),
            cfg: Arc::new(cfg_first),
            pdg: Arc::new(pdg_first),
        });
        archive.insert(CfgPdgRecord {
            function_id: id_second,
            code_hash: "2".into(),
            function_name: "second".into(),
            file_path: Some(file.clone()),
            cfg: Arc::new(cfg_second),
            pdg: Arc::new(pdg_second),
        });

        let index = FieldWriteIndex::build_from_archive(
            &archive,
            &[first_fn, second_fn],
            None,
            Some(dir.path()),
            None,
        );
        let hits = index.query(&MutationQuery {
            type_name: "OrderDTO".into(),
            exclude_ctors: false,
            member: Some("status".into()),
            include_unresolved: false,
        });
        assert_eq!(hits.len(), 2, "hits={hits:?}");
        assert!(hits.iter().any(|h| h.function_name == "first"));
        assert!(hits.iter().any(|h| h.function_name == "second"));
        assert!(
            hits.iter()
                .all(|h| h.receiver_type.as_deref() == Some("OrderDTO"))
        );
        // Stable sort: same file, earlier line first
        assert!(hits[0].line <= hits[1].line);
    }

    #[test]
    fn java_locals_merge() {
        let source = r#"
public class OrderProcessor {
    public void process(OrderDTO order) {
        OrderDTO other = order;
        other.status = "X";
    }
}
"#;
        let mut env = HashMap::new();
        crate::field_write_locals::merge_local_types("java", source, "process", &mut env);
        assert_eq!(env.get("order").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env.get("other").map(String::as_str), Some("OrderDTO"));
    }

    fn mutation_hit_helper(
        language: &str,
        source: &str,
        ctor_cfg_name: &str,
        process_cfg_name: &str,
        ctor_fn: Node,
        process_fn: Node,
        type_name: &str,
        member_substr: &str,
    ) {
        use crate::cfg_builder::build_cfg_for_function;
        use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
        use crate::pdg::ProgramDependenceGraph;
        use std::sync::Arc;

        let cfg_ctor = build_cfg_for_function(language, source, ctor_cfg_name)
            .unwrap_or_else(|e| panic!("{language} ctor cfg: {e}"));
        let cfg_proc = build_cfg_for_function(language, source, process_cfg_name)
            .unwrap_or_else(|e| panic!("{language} process cfg: {e}"));

        let id_ctor = ctor_fn.id;
        let id_proc = process_fn.id;
        let pdg_ctor = ProgramDependenceGraph::build(&cfg_ctor, source.as_bytes()).unwrap();
        let pdg_proc = ProgramDependenceGraph::build(&cfg_proc, source.as_bytes()).unwrap();

        let mut archive = CfgPdgArchive::default();
        archive.insert(CfgPdgRecord {
            function_id: id_ctor,
            code_hash: "a".into(),
            function_name: ctor_fn.name.to_string(),
            file_path: ctor_fn.file_path.as_ref().map(|s| s.to_string()),
            cfg: Arc::new(cfg_ctor),
            pdg: Arc::new(pdg_ctor),
        });
        archive.insert(CfgPdgRecord {
            function_id: id_proc,
            code_hash: "b".into(),
            function_name: process_fn.name.to_string(),
            file_path: process_fn.file_path.as_ref().map(|s| s.to_string()),
            cfg: Arc::new(cfg_proc),
            pdg: Arc::new(pdg_proc),
        });

        let index =
            FieldWriteIndex::build_from_archive(&archive, &[ctor_fn, process_fn], None, None, None);
        let hits = index.query(&MutationQuery {
            type_name: type_name.into(),
            exclude_ctors: true,
            member: None,
            include_unresolved: false,
        });
        assert_eq!(
            hits.len(),
            1,
            "{language} expected 1 non-ctor mutation, writes={:?} hits={hits:?}",
            index.writes
        );
        assert!(
            hits[0].member.contains(member_substr) || hits[0].code_snippet.contains(member_substr),
            "{language} member miss: {:?}",
            hits[0]
        );
        assert!(!hits[0].is_constructor);
    }

    #[test]
    fn enclosing_type_name_ruby_method_qualified_name() {
        let n = fn_node(
            "mark_processed",
            "OrderDTO#mark_processed",
            "order.rb",
            false,
            vec![],
        );
        assert_eq!(enclosing_type_name(&n).as_deref(), Some("OrderDTO"));
    }

    fn fn_node(name: &str, qn: &str, file: &str, is_ctor: bool, params: Vec<(&str, &str)>) -> Node {
        use rgctl_graph::schema::{GraphParameter, NodeType};
        let mut n = Node::new(NodeType::Function, name);
        n.qualified_name = Some(qn.into());
        n.file_path = Some(file.into());
        if is_ctor {
            n.properties.insert("is_constructor".into(), "true".into());
        }
        n.parameters = params
            .into_iter()
            .map(|(name, ty)| GraphParameter {
                name: name.into(),
                param_type: Some(ty.into()),
                default_value: None,
            })
            .collect();
        n
    }

    #[test]
    fn csharp_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
  public string status;
  public OrderDTO(string status) { this.status = status; }
}
class OrderProcessor {
  public OrderDTO Process(OrderDTO order) {
    order.status = "PROCESSED";
    return order;
  }
}
"#;
        mutation_hit_helper(
            "csharp",
            source,
            "OrderDTO",
            "Process",
            fn_node("OrderDTO", "OrderDTO.<init>", "OrderDTO.cs", true, vec![]),
            fn_node(
                "Process",
                "OrderProcessor.Process",
                "OrderProcessor.cs",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn go_cfg_captures_field_write_and_query() {
        let source = r#"
package demo
type OrderDTO struct { Status string }
func NewOrderDTO(status string) *OrderDTO {
  return &OrderDTO{Status: status}
}
func Process(order *OrderDTO) {
  order.Status = "PROCESSED"
}
"#;
        mutation_hit_helper(
            "go",
            source,
            "NewOrderDTO",
            "Process",
            fn_node("NewOrderDTO", "OrderDTO.<init>", "order.go", true, vec![]),
            fn_node(
                "Process",
                "Process",
                "order.go",
                false,
                vec![("order", "*OrderDTO")],
            ),
            "OrderDTO",
            "Status",
        );
    }

    #[test]
    fn rust_cfg_captures_field_write_and_query() {
        let source = r#"
struct OrderDTO { status: String }
impl OrderDTO {
    fn new(status: String) -> Self { OrderDTO { status } }
}
fn process(order: &mut OrderDTO) {
    order.status = String::from("PROCESSED");
}
"#;
        mutation_hit_helper(
            "rust",
            source,
            "new",
            "process",
            fn_node("new", "OrderDTO::<init>", "order.rs", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.rs",
                false,
                vec![("order", "&mut OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn python_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO:
    def __init__(self, status: str):
        self.status = status

def process(order: OrderDTO):
    order.status = "PROCESSED"
"#;
        mutation_hit_helper(
            "python",
            source,
            "__init__",
            "process",
            fn_node("__init__", "OrderDTO.<init>", "order.py", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.py",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn typescript_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
  status: string;
  constructor(status: string) { this.status = status; }
}
function process(order: OrderDTO) {
  order.status = "PROCESSED";
}
"#;
        mutation_hit_helper(
            "typescript",
            source,
            "constructor",
            "process",
            fn_node("OrderDTO", "OrderDTO.<init>", "order.ts", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.ts",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn javascript_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
  constructor(status) { this.status = status; }
}
function process(order) {
  order.status = "PROCESSED";
}
"#;
        mutation_hit_helper(
            "javascript",
            source,
            "constructor",
            "process",
            fn_node("OrderDTO", "OrderDTO.<init>", "order.js", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.js",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn php_cfg_captures_field_write_and_query() {
        let source = r#"<?php
class OrderDTO {
    public function __construct(private string $status) {}

    public function getStatus(): string {
        return $this->status;
    }
}

function process(OrderDTO $order): void {
    $order->status = "PROCESSED";
}
"#;
        mutation_hit_helper(
            "php",
            source,
            "__construct",
            "process",
            fn_node("__construct", "OrderDTO.<init>", "order.php", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.php",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn ruby_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO
  def initialize(status)
    @status = status
  end

  def mark_processed
    @status = "PROCESSED"
  end
end
"#;
        mutation_hit_helper(
            "ruby",
            source,
            "initialize",
            "mark_processed",
            fn_node("initialize", "OrderDTO.<init>", "order.rb", true, vec![]),
            fn_node(
                "mark_processed",
                "OrderDTO#mark_processed",
                "order.rb",
                false,
                vec![("self", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn c_cfg_captures_field_write_and_query() {
        let source = r#"
typedef struct { char *status; } OrderDTO;
void order_dto_init(OrderDTO *o, char *status) { o->status = status; }
void process(OrderDTO *order) { order->status = "PROCESSED"; }
"#;
        mutation_hit_helper(
            "c",
            source,
            "order_dto_init",
            "process",
            fn_node(
                "order_dto_init",
                "OrderDTO.<init>",
                "order.c",
                true,
                vec![("o", "OrderDTO")],
            ),
            fn_node(
                "process",
                "process",
                "order.c",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn cpp_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
public:
  const char* status;
  OrderDTO(const char* status) { this->status = status; }
};
OrderDTO process(OrderDTO order) {
  order.status = "PROCESSED";
  return order;
}
"#;
        mutation_hit_helper(
            "cpp",
            source,
            "OrderDTO",
            "process",
            fn_node("OrderDTO", "OrderDTO::<init>", "order.cpp", true, vec![]),
            fn_node(
                "process",
                "process",
                "order.cpp",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn kotlin_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
  var status: String = ""
  constructor(status: String) {
    this.status = status
  }
}
class OrderProcessor {
  fun process(order: OrderDTO): OrderDTO {
    order.status = "PROCESSED"
    return order
  }
}
"#;
        mutation_hit_helper(
            "kotlin",
            source,
            "OrderDTO",
            "process",
            fn_node("OrderDTO", "OrderDTO.<init>", "OrderDTO.kt", true, vec![]),
            fn_node(
                "process",
                "OrderProcessor.process",
                "OrderProcessor.kt",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }

    #[test]
    fn groovy_cfg_captures_field_write_and_query() {
        let source = r#"
class OrderDTO {
  String status
  OrderDTO(String status) { this.status = status }
}
class OrderProcessor {
  OrderDTO process(OrderDTO order) {
    order.status = "PROCESSED"
    return order
  }
}
"#;
        mutation_hit_helper(
            "groovy",
            source,
            "OrderDTO",
            "process",
            fn_node("OrderDTO", "OrderDTO.<init>", "OrderDTO.groovy", true, vec![]),
            fn_node(
                "process",
                "OrderProcessor.process",
                "OrderProcessor.groovy",
                false,
                vec![("order", "OrderDTO")],
            ),
            "OrderDTO",
            "status",
        );
    }
}
