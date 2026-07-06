//! Dominator tree and dominance frontiers (Phase 13.2).

use crate::cfg::{BlockId, ControlFlowGraph};
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Dominator tree with immediate dominators and dominance frontiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DominatorTree {
    /// Immediate dominator for each block.
    pub idom: HashMap<BlockId, BlockId>,
    /// Dominance frontiers per block.
    pub frontiers: HashMap<BlockId, HashSet<BlockId>>,
    /// Blocks reachable from entry (unreachable blocks excluded).
    pub reachable: HashSet<BlockId>,
    /// DFS block order for intersect algorithm (used by [`intersect`]).
    #[allow(dead_code)]
    block_order: HashMap<BlockId, usize>,
}

impl DominatorTree {
    /// Build dominator tree via iterative dataflow (Cooper-Harvey-Kennedy style).
    pub fn build(cfg: &ControlFlowGraph) -> Self {
        let reachable = cfg.reachable_blocks();
        let block_order = compute_block_order(cfg, &reachable);
        let mut idom = HashMap::new();
        for block_id in reachable.iter() {
            idom.insert(*block_id, cfg.entry);
        }
        idom.insert(cfg.entry, cfg.entry);

        let mut changed = true;
        while changed {
            changed = false;
            for &block_id in reachable.iter() {
                if block_id == cfg.entry {
                    continue;
                }
                let preds: Vec<BlockId> = cfg
                    .predecessors(block_id)
                    .into_iter()
                    .filter(|p| reachable.contains(p))
                    .collect();
                if preds.is_empty() {
                    continue;
                }
                let mut new_idom = preds[0];
                for pred in &preds[1..] {
                    new_idom = intersect(&idom, &block_order, new_idom, *pred);
                }
                if idom.get(&block_id) != Some(&new_idom) {
                    idom.insert(block_id, new_idom);
                    changed = true;
                }
            }
        }

        debug_assert!(verify_idom_acyclic(&idom), "idom tree must be acyclic");

        let frontiers = compute_dominance_frontiers(cfg, &idom, &reachable);
        Self {
            idom,
            frontiers,
            reachable,
            block_order,
        }
    }

    /// Build and validate dominator tree, returning error if idom is cyclic.
    pub fn build_verified(cfg: &ControlFlowGraph) -> Result<Self> {
        let tree = Self::build(cfg);
        if !verify_idom_acyclic(&tree.idom) {
            return Err(Error::InvalidQuery(
                "dominator tree contains a cycle".into(),
            ));
        }
        Ok(tree)
    }

    /// Returns true if `dominator` dominates `node`.
    pub fn dominates(&self, dominator: BlockId, node: BlockId) -> bool {
        if !self.reachable.contains(&node) || !self.reachable.contains(&dominator) {
            return false;
        }
        if dominator == node {
            return true;
        }
        let mut current = node;
        let mut steps = 0usize;
        while let Some(&parent) = self.idom.get(&current) {
            if parent == current {
                break;
            }
            if parent == dominator {
                return true;
            }
            current = parent;
            steps += 1;
            if steps > self.reachable.len() {
                return false;
            }
        }
        false
    }

    /// Dominance frontier of `block`.
    pub fn frontier(&self, block: BlockId) -> &HashSet<BlockId> {
        static EMPTY: std::sync::OnceLock<HashSet<BlockId>> = std::sync::OnceLock::new();
        self.frontiers
            .get(&block)
            .unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
    }
}

/// Verify that the immediate-dominator relation has no cycles.
pub fn verify_idom_acyclic(idom: &HashMap<BlockId, BlockId>) -> bool {
    for &node in idom.keys() {
        let mut seen = HashSet::new();
        let mut current = node;
        while let Some(&parent) = idom.get(&current) {
            if parent == current {
                break;
            }
            if !seen.insert(parent) {
                return false;
            }
            current = parent;
        }
    }
    true
}

fn compute_block_order(
    cfg: &ControlFlowGraph,
    reachable: &HashSet<BlockId>,
) -> HashMap<BlockId, usize> {
    let mut order = HashMap::new();
    let mut stack = vec![cfg.entry];
    let mut visited = HashSet::new();
    let mut idx = 0usize;
    while let Some(block) = stack.pop() {
        if !reachable.contains(&block) || !visited.insert(block) {
            continue;
        }
        order.insert(block, idx);
        idx += 1;
        for succ in cfg.successors(block) {
            if reachable.contains(&succ) && !visited.contains(&succ) {
                stack.push(succ);
            }
        }
    }
    order
}

fn intersect(
    idom: &HashMap<BlockId, BlockId>,
    order: &HashMap<BlockId, usize>,
    mut b1: BlockId,
    mut b2: BlockId,
) -> BlockId {
    while b1 != b2 {
        while order.get(&b1).unwrap_or(&0) > order.get(&b2).unwrap_or(&0) {
            b1 = *idom.get(&b1).unwrap_or(&b1);
            if b1 == *idom.get(&b1).unwrap_or(&b1) {
                break;
            }
        }
        while order.get(&b2).unwrap_or(&0) > order.get(&b1).unwrap_or(&0) {
            b2 = *idom.get(&b2).unwrap_or(&b2);
            if b2 == *idom.get(&b2).unwrap_or(&b2) {
                break;
            }
        }
    }
    b1
}

fn compute_dominance_frontiers(
    cfg: &ControlFlowGraph,
    idom: &HashMap<BlockId, BlockId>,
    reachable: &HashSet<BlockId>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let mut frontiers: HashMap<BlockId, HashSet<BlockId>> =
        reachable.iter().map(|id| (*id, HashSet::new())).collect();

    for block in reachable {
        let preds: Vec<BlockId> = cfg
            .predecessors(*block)
            .into_iter()
            .filter(|p| reachable.contains(p))
            .collect();
        if preds.len() < 2 {
            continue;
        }
        let block_idom = idom.get(block).copied().unwrap_or(cfg.entry);
        for pred in preds {
            let mut runner = pred;
            while runner != block_idom {
                frontiers.entry(runner).or_default().insert(*block);
                runner = idom.get(&runner).copied().unwrap_or(cfg.entry);
                if runner == idom.get(&runner).copied().unwrap_or(runner) && runner != cfg.entry {
                    break;
                }
            }
        }
    }
    frontiers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;

    #[test]
    fn test_dominance_entry_dominates_all() {
        let code = r#"
fn test(x: i32) -> i32 {
    if x > 0 {
        return x * 2;
    }
    0
}
"#;
        let cfg = build_cfg_for_function("rust", code, "test").unwrap();
        let dom = DominatorTree::build(&cfg);
        for block in dom.reachable.iter() {
            assert!(dom.dominates(cfg.entry, *block));
        }
        assert!(verify_idom_acyclic(&dom.idom));
    }

    #[test]
    fn test_dominance_frontiers_non_empty_on_branch() {
        let code = r#"
fn branch(x: i32) {
    if x > 0 {
        let y = 1;
    }
}
"#;
        let cfg = build_cfg_for_function("rust", code, "branch").unwrap();
        let dom = DominatorTree::build(&cfg);
        let has_frontier = dom.frontiers.values().any(|f| !f.is_empty());
        assert!(has_frontier || cfg.blocks.len() <= 2);
    }

    #[test]
    fn test_idom_acyclic_on_loop() {
        let code = r#"
fn sum(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < n {
        s += i;
        i += 1;
    }
    s
}
"#;
        let cfg = build_cfg_for_function("rust", code, "sum").unwrap();
        let dom = DominatorTree::build(&cfg);
        assert!(verify_idom_acyclic(&dom.idom));
    }
}
