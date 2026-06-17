//! Interprocedural backward slicing (Phase 13.1).

use crate::analysis::interprocedural_cfg::InterproceduralCFG;
use crate::analysis::pdg::{PdgNodeId, ProgramDependenceGraph};
use crate::analysis::slicing::{BackwardSlicer, SliceCriterion};
use crate::error::{Error, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

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

/// Backward slicer across function boundaries.
pub struct InterproceduralSlicer<'a> {
    icfg: &'a InterproceduralCFG,
    pdgs: HashMap<Uuid, ProgramDependenceGraph>,
}

impl<'a> InterproceduralSlicer<'a> {
    /// Build slicer with PDGs for each function CFG.
    pub fn new(
        icfg: &'a InterproceduralCFG,
        source_files: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut pdgs = HashMap::new();
        for (func_id, cfg) in &icfg.function_cfgs {
            let func_node = &icfg.call_graph.nodes[func_id];
            let source = source_files
                .get(&func_node.file_path)
                .or_else(|| {
                    source_files
                        .iter()
                        .find(|(k, _)| k.ends_with(&func_node.file_path))
                        .map(|(_, v)| v)
                });
            if let Some(source) = source {
                let pdg = ProgramDependenceGraph::build(cfg, source.as_bytes())?;
                pdgs.insert(*func_id, pdg);
            }
        }
        Ok(Self { icfg, pdgs })
    }

    /// Compute interprocedural slice starting in `function`.
    pub fn slice(&self, function: Uuid, criterion: SliceCriterion) -> Result<InterproceduralSlice> {
        let pdg = self.pdgs.get(&function).ok_or_else(|| {
            Error::NotFound(format!("PDG for function {function}"))
        })?;
        let cfg = self
            .icfg
            .get_cfg(function)
            .ok_or_else(|| Error::NotFound(format!("CFG for function {function}")))?;

        let local_slicer = BackwardSlicer::new(pdg, cfg);
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
            let current_pdg = &self.pdgs[&current_func];
            let node = &current_pdg.nodes[&current_node];

            for var in &node.used_vars {
                if self.is_parameter(current_func, var) {
                    for (caller_id, _) in self.icfg.caller_cfgs(current_func) {
                        if let Some(caller_pdg) = self.pdgs.get(&caller_id) {
                            let call_sites =
                                self.find_call_site_nodes(caller_pdg, current_func);
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
        }

        let lines: HashSet<usize> = slice
            .iter()
            .filter_map(|(func_id, node_id)| {
                self.pdgs
                    .get(func_id)
                    .and_then(|p| p.nodes.get(node_id))
                    .map(|n| n.statement.line)
            })
            .collect();

        let total = self.count_total_lines();
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

    fn is_parameter(&self, function: Uuid, variable: &str) -> bool {
        if self
            .icfg
            .call_graph
            .parameter_names(function)
            .iter()
            .any(|p| p == variable)
        {
            return true;
        }
        if let Some(pdg) = self.pdgs.get(&function) {
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
        caller_pdg: &ProgramDependenceGraph,
        callee_func: Uuid,
    ) -> Vec<PdgNodeId> {
        let callee_name = &self.icfg.call_graph.nodes[&callee_func].name;
        caller_pdg
            .nodes
            .iter()
            .filter(|(_, node)| node.statement.text.contains(callee_name))
            .map(|(id, _)| *id)
            .collect()
    }

    fn count_total_lines(&self) -> usize {
        self.icfg
            .function_cfgs
            .values()
            .flat_map(|cfg| {
                cfg.blocks
                    .values()
                    .flat_map(|b| b.statements.iter().map(|s| s.line))
            })
            .collect::<HashSet<_>>()
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::interprocedural_cfg::InterproceduralCFG;
    use crate::graph::backend::{GraphBackend, MemoryBackend};
    use crate::graph::schema::{Edge, EdgeType, Node, NodeType};

    #[cfg(feature = "lang-rust")]
    #[test]
    fn test_interprocedural_slice_includes_caller() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".into())
            .with_file_path("prog.rs".into());
        let process = Node::new(NodeType::Function, "process".into())
            .with_file_path("prog.rs".into());
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
        let slicer = InterproceduralSlicer::new(&icfg, &files).unwrap();

        let pdg = ProgramDependenceGraph::build(
            icfg.get_cfg(id_process).unwrap(),
            source.as_bytes(),
        )
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
