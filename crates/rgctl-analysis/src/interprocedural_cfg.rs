//! Interprocedural control-flow graph (Phase 13.1).

use crate::callgraph::CallGraph;
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use crate::cfg_pdg_archive::CfgPdgArchive;
use crate::language_profile::language_id_from_path;
use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// Shared CFG lookup surface for intra- and inter-procedural analysis.
pub trait InterproceduralCfgAccess {
    /// CFG for one function.
    fn get_cfg(&self, function: Uuid) -> Option<&ControlFlowGraph>;
    /// Caller CFGs for `function`.
    fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)>;
    /// Program call graph.
    fn call_graph(&self) -> &CallGraph;
    /// Count of distinct source lines referenced by all known CFGs.
    fn total_cfg_lines(&self) -> usize;
}

/// CFGs for all functions linked by a call graph.
#[derive(Debug, Clone)]
pub struct InterproceduralCFG {
    /// Per-function intraprocedural CFGs.
    pub function_cfgs: HashMap<Uuid, ControlFlowGraph>,
    /// Program call graph.
    pub call_graph: CallGraph,
}

/// Zero-copy CFG view backed by a discover-time archive.
#[derive(Debug)]
pub struct InterproceduralCfgView<'a> {
    archive: &'a CfgPdgArchive,
    call_graph: CallGraph,
}

impl InterproceduralCfgView<'_> {
    /// Create a lazy, query-local arena for resolver-selected call-site bindings.
    pub fn call_bindings(&self) -> crate::call_bindings::CallBindings<'_, Self> {
        crate::call_bindings::CallBindings::new(self)
    }

    /// Build a borrowed view over archived CFGs and a live call graph.
    pub fn from_archive<'a>(
        archive: &'a CfgPdgArchive,
        backend: &MemoryBackend,
    ) -> Result<InterproceduralCfgView<'a>> {
        Ok(InterproceduralCfgView {
            archive,
            call_graph: CallGraph::from_backend(backend)?,
        })
    }
}

impl InterproceduralCfgAccess for InterproceduralCFG {
    fn get_cfg(&self, function: Uuid) -> Option<&ControlFlowGraph> {
        self.function_cfgs.get(&function)
    }

    fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)> {
        self.call_graph
            .callers(function)
            .into_iter()
            .filter_map(|caller_id| {
                self.function_cfgs
                    .get(&caller_id)
                    .map(|cfg| (caller_id, cfg))
            })
            .collect()
    }

    fn call_graph(&self) -> &CallGraph {
        &self.call_graph
    }

    fn total_cfg_lines(&self) -> usize {
        self.function_cfgs
            .values()
            .flat_map(|cfg| {
                cfg.blocks
                    .values()
                    .flat_map(|b| b.statements.iter().map(|s| s.line))
            })
            .collect::<std::collections::HashSet<_>>()
            .len()
    }
}

impl InterproceduralCfgAccess for InterproceduralCfgView<'_> {
    fn get_cfg(&self, function: Uuid) -> Option<&ControlFlowGraph> {
        self.archive.get_cfg(function)
    }

    fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)> {
        self.call_graph
            .callers(function)
            .into_iter()
            .filter_map(|caller_id| self.archive.get_cfg(caller_id).map(|cfg| (caller_id, cfg)))
            .collect()
    }

    fn call_graph(&self) -> &CallGraph {
        &self.call_graph
    }

    fn total_cfg_lines(&self) -> usize {
        self.archive
            .records
            .values()
            .flat_map(|record| {
                record
                    .cfg
                    .blocks
                    .values()
                    .flat_map(|b| b.statements.iter().map(|s| s.line))
            })
            .collect::<std::collections::HashSet<_>>()
            .len()
    }
}

impl InterproceduralCFG {
    /// Create a lazy, query-local arena for resolver-selected call-site bindings.
    pub fn call_bindings(&self) -> crate::call_bindings::CallBindings<'_, Self> {
        crate::call_bindings::CallBindings::new(self)
    }

    /// Build from backend and source file contents keyed by path.
    pub fn build(backend: &MemoryBackend, source_files: &HashMap<String, String>) -> Result<Self> {
        use rgctl_graph::backend::GraphBackend;

        let call_graph = CallGraph::from_backend(backend)?;
        let mut function_cfgs = HashMap::new();

        for &func_id in call_graph.function_ids() {
            if let Ok(Some(func_node)) = backend.get_node(func_id) {
                let file_path = func_node.file_path.as_deref().unwrap_or("");
                let source = resolve_source(source_files, file_path);
                if let Some(source) = source {
                    let language = detect_language(file_path);
                    if let Ok(cfg) = build_cfg_for_function(language, source, &func_node.name) {
                        function_cfgs.insert(func_id, cfg);
                    }
                }
            }
        }

        Ok(Self {
            function_cfgs,
            call_graph,
        })
    }

    /// Assemble ICFG from a discover-time CFG archive + live call graph (skips CFG rebuild).
    pub fn from_cfg_archive(
        backend: &MemoryBackend,
        function_cfgs: HashMap<Uuid, ControlFlowGraph>,
    ) -> Result<Self> {
        Ok(Self {
            function_cfgs,
            call_graph: CallGraph::from_backend(backend)?,
        })
    }

    /// CFG for one function.
    pub fn get_cfg(&self, function: Uuid) -> Option<&ControlFlowGraph> {
        InterproceduralCfgAccess::get_cfg(self, function)
    }

    /// Caller CFGs for `function`.
    pub fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)> {
        InterproceduralCfgAccess::caller_cfgs(self, function)
    }
}

fn resolve_source<'a>(
    source_files: &'a HashMap<String, String>,
    file_path: &str,
) -> Option<&'a String> {
    if let Some(s) = source_files.get(file_path) {
        return Some(s);
    }
    let normalized = file_path.trim_start_matches("./");
    source_files.get(normalized).or_else(|| {
        source_files
            .iter()
            .find(|(k, _)| k.ends_with(normalized) || Path::new(k).ends_with(normalized))
            .map(|(_, v)| v)
    })
}

fn detect_language(file_path: &str) -> &'static str {
    language_id_from_path(Path::new(file_path)).unwrap_or("rust")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};

    #[test]
    fn test_interprocedural_cfg_build() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let helper = Node::new(NodeType::Function, "helper").with_file_path("main.rs");
        let id_main = main.id;
        let id_helper = helper.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(helper).unwrap();
        backend
            .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
            .unwrap();

        let source = r#"
fn main() {
    let x = helper();
}
fn helper() -> i32 {
    42
}
"#;
        let mut files = HashMap::new();
        files.insert("main.rs".into(), source.to_string());
        let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
        assert!(icfg.function_cfgs.contains_key(&id_main));
        assert!(icfg.function_cfgs.contains_key(&id_helper));
    }

    #[test]
    fn from_cfg_archive_reuses_stored_cfgs() {
        use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
        use rgctl_graph::code_index::hash_code;
        use std::sync::Arc;

        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main").with_file_path("main.rs");
        let helper = Node::new(NodeType::Function, "helper").with_file_path("main.rs");
        let id_main = main.id;
        let id_helper = helper.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(helper).unwrap();
        backend
            .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
            .unwrap();

        let source = r#"
fn main() { helper(); }
fn helper() -> i32 { 42 }
"#;
        let cfg_main = build_cfg_for_function("rust", source, "main").unwrap();
        let cfg_helper = build_cfg_for_function("rust", source, "helper").unwrap();
        let pdg = crate::pdg::ProgramDependenceGraph::build(&cfg_main, source.as_bytes()).unwrap();

        let mut archive = CfgPdgArchive::default();
        for (id, cfg) in [(id_main, cfg_main), (id_helper, cfg_helper)] {
            archive.insert(CfgPdgRecord {
                function_id: id,
                code_hash: hash_code(source),
                function_name: String::new(),
                file_path: None,
                cfg: Arc::new(cfg),
                pdg: Arc::new(pdg.clone()),
            });
        }

        let view = InterproceduralCfgView::from_archive(&archive, &backend).unwrap();
        assert!(view.get_cfg(id_main).is_some());
        assert!(view.get_cfg(id_helper).is_some());
    }
}
