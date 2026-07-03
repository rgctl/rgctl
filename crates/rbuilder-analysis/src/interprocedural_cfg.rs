//! Interprocedural control-flow graph (Phase 13.1).

use crate::callgraph::CallGraph;
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

/// CFGs for all functions linked by a call graph.
#[derive(Debug, Clone)]
pub struct InterproceduralCFG {
    /// Per-function intraprocedural CFGs.
    pub function_cfgs: HashMap<Uuid, ControlFlowGraph>,
    /// Program call graph.
    pub call_graph: CallGraph,
}

impl InterproceduralCFG {
    /// Build from backend and source file contents keyed by path.
    pub fn build(backend: &MemoryBackend, source_files: &HashMap<String, String>) -> Result<Self> {
        use rbuilder_graph::backend::GraphBackend;

        let call_graph = CallGraph::from_backend(backend)?;
        let mut function_cfgs = HashMap::new();

        // Iterate over function IDs and fetch metadata from backend
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
        self.function_cfgs.get(&function)
    }

    /// Caller CFGs for `function`.
    pub fn caller_cfgs(&self, function: Uuid) -> Vec<(Uuid, &ControlFlowGraph)> {
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

fn detect_language(file_path: &str) -> &str {
    match Path::new(file_path).extension().and_then(|e| e.to_str()) {
        Some("py") => "python",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        Some("ts") | Some("tsx") => "typescript",
        Some("rb") => "ruby",
        _ => "rust",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, EdgeType, Node, NodeType};

    #[test]
    fn test_interprocedural_cfg_build() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".into()).with_file_path("main.rs".into());
        let helper =
            Node::new(NodeType::Function, "helper".into()).with_file_path("main.rs".into());
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
        use rbuilder_graph::code_index::hash_code;

        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".into()).with_file_path("main.rs".into());
        let helper =
            Node::new(NodeType::Function, "helper".into()).with_file_path("main.rs".into());
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
                cfg,
                pdg: pdg.clone(),
            });
        }

        let icfg = archive.to_interprocedural_cfg(&backend).unwrap();
        assert_eq!(icfg.function_cfgs.len(), 2);
        assert!(icfg.get_cfg(id_main).is_some());
    }
}
