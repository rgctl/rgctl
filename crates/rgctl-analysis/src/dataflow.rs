//! Reaching-definitions data-flow analysis.
//!
//! **Algorithm:** iterative gen/kill dataflow on the CFG worklist.
//! **Complexity:** O(b · d) blocks × definition-set size; gen/kill built in O(total defs).

use crate::cfg::{BasicBlock, BlockId, ControlFlowGraph};
use crate::hash_maps::{FxHashMap, FxHashSet};
use crate::pdg::ProgramDependenceGraph;
use bit_set::BitSet;
use std::collections::VecDeque;

/// Result of reaching-definitions analysis.
#[derive(Debug, Clone, Default)]
pub struct ReachingDefs {
    /// Definitions reaching the entry of each block (indices into [`Self::catalog`]).
    pub in_set: FxHashMap<BlockId, BitSet>,
    /// Definitions reaching the exit of each block (indices into [`Self::catalog`]).
    pub out_set: FxHashMap<BlockId, BitSet>,
    /// Definition catalog indexed by bit positions in the sets above.
    pub catalog: DefCatalog,
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

/// Catalog of definitions with variable index for kill sets.
#[derive(Debug, Clone, Default)]
pub struct DefCatalog {
    /// All definitions in stable index order.
    pub defs: Vec<Definition>,
    by_var: FxHashMap<String, Vec<usize>>,
}

impl DefCatalog {
    fn push(&mut self, def: Definition) -> usize {
        let idx = self.defs.len();
        self.by_var
            .entry(def.variable.clone())
            .or_default()
            .push(idx);
        self.defs.push(def);
        idx
    }
}

impl ReachingDefs {
    /// Iterate definitions reaching the entry of `block`.
    pub fn in_definitions<'a>(
        &'a self,
        block: BlockId,
    ) -> impl Iterator<Item = &'a Definition> + 'a {
        self.in_set
            .get(&block)
            .into_iter()
            .flat_map(|bits| bits.iter().map(|idx| &self.catalog.defs[idx]))
    }

    /// Iterate definitions reaching the exit of `block`.
    pub fn out_definitions<'a>(
        &'a self,
        block: BlockId,
    ) -> impl Iterator<Item = &'a Definition> + 'a {
        self.out_set
            .get(&block)
            .into_iter()
            .flat_map(|bits| bits.iter().map(|idx| &self.catalog.defs[idx]))
    }
}

/// Compute reaching definitions for all blocks in `cfg`.
pub fn compute_reaching_definitions(
    cfg: &ControlFlowGraph,
    pdg: &ProgramDependenceGraph,
) -> ReachingDefs {
    let mut catalog = DefCatalog::default();
    let mut block_local_defs: FxHashMap<BlockId, Vec<usize>> =
        FxHashMap::with_capacity_and_hasher(cfg.blocks.len(), Default::default());

    for (&block_id, block) in &cfg.blocks {
        let mut local = Vec::new();
        for def in collect_block_definitions(block, pdg) {
            let idx = catalog.push(def);
            local.push(idx);
        }
        block_local_defs.insert(block_id, local);
    }

    let mut gen_sets: FxHashMap<BlockId, BitSet> =
        FxHashMap::with_capacity_and_hasher(cfg.blocks.len(), Default::default());
    let mut kill_sets: FxHashMap<BlockId, BitSet> =
        FxHashMap::with_capacity_and_hasher(cfg.blocks.len(), Default::default());
    let mut in_set_bits: FxHashMap<BlockId, BitSet> =
        FxHashMap::with_capacity_and_hasher(cfg.blocks.len(), Default::default());
    let mut out_set_bits: FxHashMap<BlockId, BitSet> =
        FxHashMap::with_capacity_and_hasher(cfg.blocks.len(), Default::default());

    for &block_id in cfg.blocks.keys() {
        let local_defs = &block_local_defs[&block_id];

        let mut gen_b = BitSet::new();
        let mut defined_vars: FxHashSet<String> = FxHashSet::default();
        for &def_idx in local_defs.iter().rev() {
            let var = &catalog.defs[def_idx].variable;
            if defined_vars.insert(var.clone()) {
                gen_b.insert(def_idx);
            }
        }

        let mut kill_b = BitSet::new();
        for var in &defined_vars {
            if let Some(all_defs_of_var) = catalog.by_var.get(var) {
                for &def_idx in all_defs_of_var {
                    if !gen_b.contains(def_idx) {
                        kill_b.insert(def_idx);
                    }
                }
            }
        }

        gen_sets.insert(block_id, gen_b);
        kill_sets.insert(block_id, kill_b);
        in_set_bits.insert(block_id, BitSet::new());
        out_set_bits.insert(block_id, BitSet::new());
    }

    let mut worklist: VecDeque<BlockId> = cfg.blocks.keys().copied().collect();
    let mut on_worklist: FxHashSet<BlockId> = worklist.iter().copied().collect();

    while let Some(block_id) = worklist.pop_front() {
        on_worklist.remove(&block_id);

        let mut in_b = BitSet::new();
        for &pred in cfg.predecessors(block_id) {
            if let Some(pred_out) = out_set_bits.get(&pred) {
                in_b.union_with(pred_out);
            }
        }

        let gen_b = &gen_sets[&block_id];
        let kill_b = &kill_sets[&block_id];

        let mut out_b = gen_b.clone();
        for def_idx in in_b.iter() {
            if !kill_b.contains(def_idx) {
                out_b.insert(def_idx);
            }
        }

        if out_set_bits.get(&block_id) != Some(&out_b) {
            out_set_bits.insert(block_id, out_b);
            in_set_bits.insert(block_id, in_b);
            for &succ in cfg.successors(block_id) {
                if on_worklist.insert(succ) {
                    worklist.push_back(succ);
                }
            }
        } else {
            in_set_bits.insert(block_id, in_b);
        }
    }

    ReachingDefs {
        in_set: in_set_bits,
        out_set: out_set_bits,
        catalog,
    }
}

fn collect_block_definitions(block: &BasicBlock, pdg: &ProgramDependenceGraph) -> Vec<Definition> {
    let mut defs = Vec::new();
    for (idx, _stmt) in block.statements.iter().enumerate() {
        if let Some(pdg_node) = pdg.find_node_by_block_and_index(block.id, idx) {
            for var in &pdg_node.defined_vars {
                defs.push(Definition {
                    variable: var.clone(),
                    block: block.id,
                    statement_index: idx,
                    pdg_node: pdg_node.id,
                });
            }
        }
    }
    defs
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

        let x_defs: Vec<_> = reaching
            .in_definitions(use_block.id)
            .filter(|d| d.variable == "x")
            .collect();
        assert!(x_defs.len() >= 2);
    }

    #[test]
    fn test_reaching_definitions_diamond_merge() {
        let code = r#"
fn diamond(cond: bool) {
    let mut x = 1;
    if cond {
        x = 2;
    } else {
        x = 3;
    }
    let y = x;
    let _z = y;
}
"#;
        let cfg = build_cfg_for_function("rust", code, "diamond").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let reaching = compute_reaching_definitions(&cfg, &pdg);

        let merge_block = cfg
            .blocks
            .values()
            .find(|b| b.statements.iter().any(|s| s.text.contains("_z")))
            .expect("merge block");

        let x_defs: Vec<_> = reaching
            .in_definitions(merge_block.id)
            .filter(|d| d.variable == "x")
            .collect();
        assert!(
            x_defs.len() >= 2,
            "diamond merge should see multiple reaching definitions for x"
        );
    }

    #[test]
    fn test_same_block_redefinition_masks_earlier_def() {
        let code = r#"
fn shadow() {
    let mut x = 1;
    x = 2;
    let _y = x;
}
"#;
        let cfg = build_cfg_for_function("rust", code, "shadow").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let reaching = compute_reaching_definitions(&cfg, &pdg);

        let mut saw_double_def_block = false;
        for (block_id, block) in &cfg.blocks {
            let local_x: Vec<_> = collect_block_definitions(block, &pdg)
                .into_iter()
                .filter(|d| d.variable == "x")
                .collect();
            if local_x.len() < 2 {
                continue;
            }
            saw_double_def_block = true;
            let out_x: Vec<_> = reaching
                .out_definitions(*block_id)
                .filter(|d| d.variable == "x")
                .collect();
            assert_eq!(
                out_x.len(),
                1,
                "block with multiple assignments to x should gen only the last definition"
            );
        }
        assert!(
            saw_double_def_block,
            "expected cfg to place at least two x assignments in one basic block"
        );
    }

    #[test]
    fn test_same_line_statements_get_distinct_defs() {
        use crate::cfg::{
            BasicBlock, BlockId, CfgEdgeType, ControlFlowGraph, DefVar, Statement, StatementKind,
        };
        use smallvec::SmallVec;

        let mut cfg = ControlFlowGraph::new();
        let b0 = cfg.entry;
        cfg.blocks.get_mut(&b0).unwrap().statements = vec![
            Statement {
                kind: StatementKind::Declaration,
                line: 1,
                text: "let a = 1;".into(),
                defined_vars: SmallVec::from_vec(vec![DefVar::local("a")]),
                used_vars: SmallVec::new(),
            },
            Statement {
                kind: StatementKind::Declaration,
                line: 1, // same source line — index lookup must not collide
                text: "let b = a + 1;".into(),
                defined_vars: SmallVec::from_vec(vec![DefVar::local("b")]),
                used_vars: SmallVec::from_vec(vec!["a".into()]),
            },
        ];
        cfg.blocks.get_mut(&b0).unwrap().start_line = 1;
        cfg.blocks.get_mut(&b0).unwrap().end_line = 1;
        let exit = BlockId(1);
        cfg.add_block(BasicBlock {
            id: exit,
            statements: vec![],
            start_line: 0,
            end_line: 0,
        });
        cfg.add_edge(b0, exit, CfgEdgeType::Next);
        cfg.exits.push(exit);

        let pdg = ProgramDependenceGraph::build(&cfg, b"").unwrap();
        let a_node = pdg.find_node_by_block_and_index(b0, 0).expect("a");
        let b_node = pdg.find_node_by_block_and_index(b0, 1).expect("b");
        assert_ne!(a_node.id, b_node.id);
        assert!(a_node.defined_vars.contains("a"));
        assert!(b_node.defined_vars.contains("b"));

        let defs = collect_block_definitions(cfg.blocks.get(&b0).unwrap(), &pdg);
        assert_eq!(defs.len(), 2);
        assert!(defs.iter().any(|d| d.variable == "a" && d.statement_index == 0));
        assert!(defs.iter().any(|d| d.variable == "b" && d.statement_index == 1));

        let flow = pdg
            .data_deps
            .iter()
            .any(|d| d.from == a_node.id && d.to == b_node.id && d.variable == "a");
        assert!(flow, "expected flow dep a → b on same line, deps={:?}", pdg.data_deps);
    }
}
