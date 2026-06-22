//! Reaching-definitions data-flow analysis.

use crate::cfg::{BasicBlock, BlockId, ControlFlowGraph};
use crate::pdg::ProgramDependenceGraph;
use std::collections::{HashMap, HashSet, VecDeque};

/// Result of reaching-definitions analysis.
#[derive(Debug, Clone, Default)]
pub struct ReachingDefs {
    /// Definitions reaching the entry of each block.
    pub in_set: HashMap<BlockId, HashSet<Definition>>,
    /// Definitions reaching the exit of each block.
    pub out_set: HashMap<BlockId, HashSet<Definition>>,
}

/// A definition of a variable at a specific statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Definition {
    /// Variable name.
    pub variable: String,
    /// Block containing the definition.
    pub block: BlockId,
    /// Index of the defining statement within the block.
    pub statement_index: usize,
    /// PDG node id for the defining statement.
    pub pdg_node: uuid::Uuid,
}

/// Compute reaching definitions for all blocks in `cfg`.
pub fn compute_reaching_definitions(
    cfg: &ControlFlowGraph,
    pdg: &ProgramDependenceGraph,
) -> ReachingDefs {
    let mut gen = HashMap::new();
    let mut kill = HashMap::new();
    let mut in_set = HashMap::new();
    let mut out_set = HashMap::new();

    for (block_id, block) in &cfg.blocks {
        let (g, k) = compute_gen_kill(block, pdg);
        gen.insert(*block_id, g);
        kill.insert(*block_id, k);
        in_set.insert(*block_id, HashSet::new());
        out_set.insert(*block_id, HashSet::new());
    }

    let mut worklist: VecDeque<BlockId> = cfg.blocks.keys().copied().collect();

    while let Some(block_id) = worklist.pop_front() {
        let in_b: HashSet<Definition> = cfg
            .predecessors(block_id)
            .iter()
            .flat_map(|pred| out_set.get(pred).cloned().unwrap_or_default())
            .collect();

        let gen_b = gen.get(&block_id).cloned().unwrap_or_default();
        let kill_b = kill.get(&block_id).cloned().unwrap_or_default();

        let out_b: HashSet<Definition> = gen_b
            .iter()
            .cloned()
            .chain(in_b.iter().filter(|def| !kill_b.contains(def)).cloned())
            .collect();

        if out_set.get(&block_id) != Some(&out_b) {
            out_set.insert(block_id, out_b);
            in_set.insert(block_id, in_b);
            for succ in cfg.successors(block_id) {
                if !worklist.contains(&succ) {
                    worklist.push_back(succ);
                }
            }
        } else {
            in_set.insert(block_id, in_b);
        }
    }

    ReachingDefs { in_set, out_set }
}

fn compute_gen_kill(
    block: &BasicBlock,
    pdg: &ProgramDependenceGraph,
) -> (HashSet<Definition>, HashSet<Definition>) {
    let mut gen = HashSet::new();
    let mut kill = HashSet::new();
    let mut all_defs: Vec<Definition> = Vec::new();

    for (idx, stmt) in block.statements.iter().enumerate() {
        if let Some(pdg_node) = pdg.find_node_by_block_and_line(block.id, stmt.line) {
            for var in &pdg_node.defined_vars {
                let def = Definition {
                    variable: var.clone(),
                    block: block.id,
                    statement_index: idx,
                    pdg_node: pdg_node.id,
                };
                all_defs.push(def.clone());
                gen.insert(def);
            }
        }
    }

    for def in &gen {
        for other in &all_defs {
            if other.variable == def.variable && other.pdg_node != def.pdg_node {
                kill.insert(other.clone());
            }
        }
    }

    (gen, kill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::pdg::ProgramDependenceGraph;

    #[test]
    fn test_reaching_definitions_merge() {
        let code = r#"
fn example(condition: bool) {
    let mut x = 1;
    if condition {
        x = 2;
    } else {
        x = 3;
    }
    let _y = x;
}
"#;
        let cfg = build_cfg_for_function("rust", code, "example").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let reaching = compute_reaching_definitions(&cfg, &pdg);

        let use_block = cfg
            .blocks
            .values()
            .find(|b| b.statements.iter().any(|s| s.text.contains("_y")))
            .expect("use block");

        let x_defs: Vec<_> = reaching.in_set[&use_block.id]
            .iter()
            .filter(|d| d.variable == "x")
            .collect();
        assert!(x_defs.len() >= 2);
    }
}
