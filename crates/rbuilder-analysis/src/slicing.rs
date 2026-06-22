//! Backward program slicing using the PDG.

use crate::cfg::ControlFlowGraph;
use crate::pdg::{PdgNodeId, ProgramDependenceGraph};
use rbuilder_error::{Error, Result};
use std::collections::{HashSet, VecDeque};

/// Criterion for a backward slice (variable at a line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceCriterion {
    /// Variable of interest.
    pub variable: String,
    /// Source line (1-based).
    pub line: usize,
}

/// Result of backward slicing.
#[derive(Debug, Clone)]
pub struct CodeSlice {
    /// Original criterion.
    pub criterion: SliceCriterion,
    /// PDG nodes in the slice.
    pub statements: HashSet<PdgNodeId>,
    /// Source lines in the slice.
    pub lines: HashSet<usize>,
    /// Percentage of function lines excluded from slice.
    pub reduction_percent: f64,
}

/// Backward slicer traversing data and control dependencies.
pub struct BackwardSlicer<'a> {
    pdg: &'a ProgramDependenceGraph,
    cfg: &'a ControlFlowGraph,
}

impl<'a> BackwardSlicer<'a> {
    /// Create a slicer over the given PDG and CFG.
    pub fn new(pdg: &'a ProgramDependenceGraph, cfg: &'a ControlFlowGraph) -> Self {
        Self { pdg, cfg }
    }

    /// Compute the backward slice for `criterion`.
    pub fn slice(&self, criterion: SliceCriterion) -> Result<CodeSlice> {
        let criterion_node = self.find_criterion_node(&criterion)?;
        let mut slice = HashSet::new();
        let mut worklist = VecDeque::from([criterion_node]);

        while let Some(node_id) = worklist.pop_front() {
            if !slice.insert(node_id) {
                continue;
            }

            for dep in self.pdg.data_deps.iter().filter(|d| d.to == node_id) {
                worklist.push_back(dep.from);
            }

            for ctrl in self
                .pdg
                .control_deps
                .iter()
                .filter(|c| c.dependent == node_id)
            {
                worklist.push_back(ctrl.controller);
            }
        }

        let lines: HashSet<usize> = slice
            .iter()
            .filter_map(|id| self.pdg.nodes.get(id).map(|n| n.statement.line))
            .collect();

        let total_lines = self.count_total_lines();
        let reduction_percent = if total_lines == 0 {
            0.0
        } else {
            100.0 * (1.0 - (lines.len() as f64 / total_lines as f64))
        };

        Ok(CodeSlice {
            criterion,
            statements: slice,
            lines,
            reduction_percent,
        })
    }

    fn find_criterion_node(&self, criterion: &SliceCriterion) -> Result<PdgNodeId> {
        self.pdg
            .nodes
            .values()
            .find(|n| {
                n.statement.line == criterion.line
                    && (n.defined_vars.contains(&criterion.variable)
                        || n.used_vars.contains(&criterion.variable))
            })
            .map(|n| n.id)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "no PDG node for {} at line {}",
                    criterion.variable, criterion.line
                ))
            })
    }

    fn count_total_lines(&self) -> usize {
        let mut lines = HashSet::new();
        for block in self.cfg.blocks.values() {
            for stmt in &block.statements {
                if stmt.line > 0 {
                    lines.insert(stmt.line);
                }
            }
        }
        lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::pdg::ProgramDependenceGraph;

    #[test]
    fn test_backward_slice_reduction() {
        let code = r#"
fn process(input: String) -> String {
    let a = 10;
    let b = 20;
    let x = input.len();
    let y = x * 2;
    format!("{}", y)
}
"#;
        let cfg = build_cfg_for_function("rust", code, "process").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let slicer = BackwardSlicer::new(&pdg, &cfg);

        let y_line = pdg
            .nodes
            .values()
            .find(|n| n.defined_vars.contains("y"))
            .unwrap()
            .statement
            .line;

        let slice = slicer
            .slice(SliceCriterion {
                variable: "y".to_string(),
                line: y_line,
            })
            .unwrap();

        assert!(!slice.lines.contains(
            &pdg.nodes
                .values()
                .find(|n| n.defined_vars.contains("a"))
                .unwrap()
                .statement
                .line
        ));
        assert!(slice.lines.contains(&y_line));
    }
}
