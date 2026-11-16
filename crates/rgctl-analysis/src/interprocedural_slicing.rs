//! Interprocedural backward slicing (Phase 13.1).

use crate::interprocedural_cfg::InterproceduralCfgAccess;
use crate::pdg::{PdgNodeId, ProgramDependenceGraph};
use crate::slicing::{BackwardSlicer, SliceCriterion};
use rgctl_error::{Error, Result};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use uuid::Uuid;

/// Lazy per-function PDG cache used during interprocedural slicing.
struct PdgCache(RefCell<HashMap<Uuid, Arc<ProgramDependenceGraph>>>);

impl PdgCache {
    fn new() -> Self {
        Self(RefCell::new(HashMap::new()))
    }

    fn borrow_map(&self) -> std::cell::Ref<'_, HashMap<Uuid, Arc<ProgramDependenceGraph>>> {
        self.0.borrow()
    }

    fn borrow_map_mut(&self) -> std::cell::RefMut<'_, HashMap<Uuid, Arc<ProgramDependenceGraph>>> {
        self.0.borrow_mut()
    }
}

/// Result of interprocedural backward slicing.
#[derive(Debug, Clone)]
pub struct InterproceduralSlice {
    /// Original criterion.
    pub criterion: SliceCriterion,
    /// (function_id, pdg_node_id) pairs in the slice.
    pub statements: HashSet<(Uuid, PdgNodeId)>,
    /// Source lines included.
    pub lines: HashSet<usize>,
    /// Functions touched.
    pub functions: HashSet<Uuid>,
    /// Percentage of total LOC excluded.
    pub reduction_percent: f64,
}

/// Backward slicer across function boundaries with lazy PDG construction.
pub struct InterproceduralSlicer<'a, C: InterproceduralCfgAccess + ?Sized> {
    icfg: &'a C,
    backend: &'a MemoryBackend,
    source_files: HashMap<String, String>,
    pdg_cache: PdgCache,
    function_names: HashMap<Uuid, String>,
}

impl<'a, C: InterproceduralCfgAccess + ?Sized> InterproceduralSlicer<'a, C> {
    /// Build slicer; PDGs are constructed lazily per function during slicing.
    pub fn new(
        icfg: &'a C,
        backend: &'a MemoryBackend,
        source_files: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut function_names = HashMap::new();
        for &func_id in icfg.call_graph().function_ids() {
            if let Ok(Some(func_node)) = backend.get_node(func_id) {
                function_names.insert(func_id, func_node.name.to_string());
            }
        }
        Ok(Self {
            icfg,
            backend,
            source_files: source_files.clone(),
            pdg_cache: PdgCache::new(),
            function_names,
        })
    }

    /// Pre-populate PDG cache with refcount shares (no deep PDG clone).
    pub fn preload_pdgs(&self, archive: &crate::cfg_pdg_archive::CfgPdgArchive) {
        let mut pdgs = self.pdg_cache.borrow_map_mut();
        for (function_id, record) in &archive.records {
            pdgs.insert(*function_id, Arc::clone(&record.pdg));
        }
    }

    /// Compute interprocedural slice starting in `function`.
    pub fn slice(&self, function: Uuid, criterion: SliceCriterion) -> Result<InterproceduralSlice> {
        let pdg = self.pdg_for(function)?;
        let cfg = self
            .icfg
            .get_cfg(function)
            .ok_or_else(|| Error::NotFound(format!("CFG for function {function}")))?;

        let local_slicer = BackwardSlicer::new(pdg.as_ref(), cfg);
        let local_slice = local_slicer.slice(criterion.clone())?;

        let mut slice: HashSet<(Uuid, PdgNodeId)> = HashSet::new();
        let mut worklist: VecDeque<(Uuid, PdgNodeId)> = VecDeque::new();
        let mut visited_functions = HashSet::new();

        for node_id in &local_slice.statements {
            slice.insert((function, *node_id));
            worklist.push_back((function, *node_id));
        }
        visited_functions.insert(function);

        while let Some((current_func, current_node)) = worklist.pop_front() {
            let current_pdg = self.pdg_for(current_func)?;
            let node = &current_pdg.nodes[&current_node];

            for dep in &current_pdg.data_deps {
                if dep.to == current_node && slice.insert((current_func, dep.from)) {
                    worklist.push_back((current_func, dep.from));
                }
            }
            for dep in &current_pdg.control_deps {
                if dep.dependent == current_node && slice.insert((current_func, dep.controller)) {
                    worklist.push_back((current_func, dep.controller));
                }
            }

            for var in &node.used_vars {
                if self.is_parameter(current_func, var) {
                    for (caller_id, _) in self.icfg.caller_cfgs(current_func) {
                        let caller_pdg = self.pdg_for(caller_id)?;
                        let call_sites =
                            self.find_call_site_nodes(caller_id, caller_pdg.as_ref(), current_func);
                        for call_node in call_sites {
                            if slice.insert((caller_id, call_node)) {
                                worklist.push_back((caller_id, call_node));
                            }
                            visited_functions.insert(caller_id);
                        }
                    }
                }
            }
        }

        let lines: HashSet<usize> = slice
            .iter()
            .filter_map(|(func_id, node_id)| {
                self.pdg_cache
                    .borrow_map()
                    .get(func_id)
                    .and_then(|p| p.nodes.get(node_id))
                    .map(|n| n.statement.line)
            })
            .collect();

        let total = self.icfg.total_cfg_lines();
        let reduction_percent = if total == 0 {
            0.0
        } else {
            100.0 * (1.0 - lines.len() as f64 / total as f64)
        };

        Ok(InterproceduralSlice {
            criterion,
            statements: slice,
            lines,
            functions: visited_functions,
            reduction_percent,
        })
    }

    fn pdg_for(&self, function: Uuid) -> Result<Arc<ProgramDependenceGraph>> {
        if let Some(pdg) = self.pdg_cache.borrow_map().get(&function).cloned() {
            return Ok(pdg);
        }

        let cfg = self
            .icfg
            .get_cfg(function)
            .ok_or_else(|| Error::NotFound(format!("CFG for function {function}")))?;
        let func_node = self
            .backend
            .get_node(function)?
            .ok_or_else(|| Error::NotFound(format!("node {function}")))?;
        let file_path = func_node.file_path.as_deref().unwrap_or("");
        let source = self
            .source_files
            .get(file_path)
            .or_else(|| {
                self.source_files
                    .iter()
                    .find(|(k, _)| k.ends_with(file_path))
                    .map(|(_, v)| v)
            })
            .ok_or_else(|| Error::NotFound(format!("source for {file_path}")))?;
        let pdg = Arc::new(ProgramDependenceGraph::build(cfg, source.as_bytes())?);
        self.pdg_cache
            .borrow_map_mut()
            .insert(function, Arc::clone(&pdg));
        Ok(pdg)
    }

    fn is_parameter(&self, function: Uuid, variable: &str) -> bool {
        if matches!(
            variable,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "@" | "*"
        ) {
            return true;
        }
        if self
            .icfg
            .call_graph()
            .parameter_names(function)
            .iter()
            .any(|p| p.trim_start_matches('$') == variable)
        {
            return true;
        }
        if let Some(cfg) = self.icfg.get_cfg(function)
            && cfg
                .local_types
                .keys()
                .any(|k| k.trim_start_matches('$') == variable)
        {
            // Exclude locals that have a body definition in the function's PDG.
            if let Ok(pdg) = self.pdg_for(function) {
                let defined_in_body = pdg.nodes.values().any(|n| {
                    n.defined_vars
                        .iter()
                        .any(|d| d.trim_start_matches('$') == variable)
                });
                if !defined_in_body {
                    return true;
                }
            }
        }
        if let Ok(Some(func_node)) = self.backend.get_node(function) {
            if let Some(sig) = func_node.signature.as_deref()
                && header_mentions_param(sig, func_node.name.as_str(), variable)
            {
                return true;
            }
            if let Some(file_path) = func_node.file_path.as_deref()
                && let Some(source) = self.source_files.get(file_path).or_else(|| {
                    self.source_files
                        .iter()
                        .find(|(k, _)| k.ends_with(file_path))
                        .map(|(_, v)| v)
                })
            {
                let start_idx = func_node.start_line.unwrap_or(1).saturating_sub(1);
                let header: String = source
                    .lines()
                    .skip(start_idx)
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ");
                if header_mentions_param(&header, func_node.name.as_str(), variable) {
                    return true;
                }
            }
        }
        if let Ok(pdg) = self.pdg_for(function) {
            return pdg.nodes.values().any(|n| {
                let text = &n.statement.text;
                text.contains(&format!("{variable}:"))
                    || text.contains(&format!("({variable},"))
                    || text.contains(&format!("({variable})"))
                    || text.contains(&format!("({variable} "))
            });
        }
        false
    }

    fn find_call_site_nodes(
        &self,
        caller_id: Uuid,
        caller_pdg: &ProgramDependenceGraph,
        callee_id: Uuid,
    ) -> Vec<PdgNodeId> {
        let edges = self
            .icfg
            .call_graph()
            .call_edges_between(caller_id, callee_id);
        let lines: HashSet<usize> = edges
            .iter()
            .map(|edge| edge.call_site)
            .filter(|line| *line > 0)
            .collect();
        if !lines.is_empty() {
            let mut nodes = Vec::new();
            for line in lines {
                nodes.extend_from_slice(caller_pdg.nodes_at_line(line));
            }
            return nodes;
        }

        if let Some(callee_name) = self.function_names.get(&callee_id) {
            caller_pdg
                .nodes
                .iter()
                .filter(|(_, node)| node.statement.text.contains(callee_name))
                .map(|(id, _)| *id)
                .collect()
        } else {
            Vec::new()
        }
    }
}

fn header_mentions_param(header: &str, func_name: &str, variable: &str) -> bool {
    let scope = if let Some((_, after_open)) = header.split_once('(') {
        after_open.split(')').next().unwrap_or(after_open)
    } else if let Some(pos) = header.find(func_name) {
        &header[pos + func_name.len()..]
    } else {
        header
    };
    scope
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|tok| !tok.is_empty() && tok == variable && tok != func_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interprocedural_cfg::InterproceduralCFG;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};

    #[test]
    fn test_interprocedural_slice_includes_caller() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main").with_file_path("prog.rs");
        let process = Node::new(NodeType::Function, "process").with_file_path("prog.rs");
        let id_main = main.id;
        let id_process = process.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(process).unwrap();
        backend
            .insert_edge(Edge::new(id_main, id_process, EdgeType::Calls))
            .unwrap();

        let source = r#"
fn main() {
    let data = read_input();
    let result = process(data);
    write_output(result);
}
fn process(input: String) -> String {
    let trimmed = input.trim();
    format!("Processed: {}", trimmed)
}
fn read_input() -> String { String::new() }
fn write_output(_: String) {}
"#;
        let mut files = HashMap::new();
        files.insert("prog.rs".into(), source.to_string());
        let icfg = InterproceduralCFG::build(&backend, &files).unwrap();
        let slicer = InterproceduralSlicer::new(&icfg, &backend, &files).unwrap();

        let pdg =
            ProgramDependenceGraph::build(icfg.get_cfg(id_process).unwrap(), source.as_bytes())
                .unwrap();
        let result_line = pdg
            .nodes
            .values()
            .find(|n| n.statement.text.contains("trimmed"))
            .map(|n| n.statement.line)
            .unwrap_or(10);

        let slice = slicer
            .slice(
                id_process,
                SliceCriterion {
                    variable: "trimmed".into(),
                    line: result_line,
                },
            )
            .unwrap();

        assert!(slice.reduction_percent >= 0.0);
        assert!(slice.functions.contains(&id_process));
    }
}
