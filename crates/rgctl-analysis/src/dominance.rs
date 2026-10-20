//! Dominator tree and dominance frontiers (Phase 13.2).

use crate::cfg::{BlockId, ControlFlowGraph};
use crate::hash_maps::{FxHashMap, FxHashSet};
use rgctl_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Dominator tree with immediate dominators and dominance frontiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DominatorTree {
    /// Immediate dominator for each block.
    pub idom: HashMap<BlockId, BlockId>,
    /// Dominance frontiers per block.
    pub frontiers: HashMap<BlockId, HashSet<BlockId>>,
    /// Blocks reachable from entry (unreachable blocks excluded).
    pub reachable: HashSet<BlockId>,
    /// Reverse-postorder index per block (entry is 0).
    #[allow(dead_code)]
    block_order: HashMap<BlockId, usize>,
}

impl DominatorTree {
    /// Build dominator tree via iterative dataflow (Cooper-Harvey-Kennedy).
    ///
    /// Blocks are numbered in reverse postorder and the CHK inner loop uses dense
    /// `Vec<u32>` idoms. Public maps stay keyed by [`BlockId`] for PDG and callers.
    pub fn build(cfg: &ControlFlowGraph) -> Self {
        let reachable = cfg.reachable_blocks();
        let rpo = compute_rpo(cfg, &reachable);
        let n = rpo.len();
        let mut rpo_index: FxHashMap<BlockId, u32> =
            FxHashMap::with_capacity_and_hasher(n, Default::default());
        for (i, &id) in rpo.iter().enumerate() {
            rpo_index.insert(id, i as u32);
        }

        // idom_rpo[i] = RPO index of the immediate dominator of rpo[i]. Entry is 0.
        let mut idom_rpo = vec![0u32; n];
        let mut changed = true;
        while changed {
            changed = false;
            for i in 1..n {
                let block_id = rpo[i];
                let mut new_idom: Option<u32> = None;
                for &pred in cfg.predecessors(block_id) {
                    let Some(&pidx) = rpo_index.get(&pred) else {
                        continue;
                    };
                    new_idom = Some(match new_idom {
                        None => pidx,
                        Some(cur) => intersect_rpo(&idom_rpo, cur, pidx),
                    });
                }
                let Some(new_idom) = new_idom else {
                    continue;
                };
                if idom_rpo[i] != new_idom {
                    idom_rpo[i] = new_idom;
                    changed = true;
                }
            }
        }

        let mut idom = HashMap::with_capacity(reachable.len());
        for (i, &block_id) in rpo.iter().enumerate() {
            let idom_block = rpo[idom_rpo[i] as usize];
            idom.insert(block_id, idom_block);
        }
        for &block_id in &reachable {
            idom.entry(block_id).or_insert(cfg.entry);
        }
        idom.insert(cfg.entry, cfg.entry);

        debug_assert!(verify_idom_acyclic(&idom), "idom tree must be acyclic");

        let frontiers = compute_dominance_frontiers(cfg, &idom, &reachable);
        let block_order: HashMap<BlockId, usize> =
            rpo.iter().enumerate().map(|(i, id)| (*id, i)).collect();
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
        let mut seen: FxHashSet<BlockId> = FxHashSet::default();
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

/// Reverse postorder from `cfg.entry` (entry first). Iterative to avoid deep-CFG stack overflow.
fn compute_rpo(cfg: &ControlFlowGraph, reachable: &HashSet<BlockId>) -> Vec<BlockId> {
    if !reachable.contains(&cfg.entry) {
        return Vec::new();
    }
    let mut visited: FxHashSet<BlockId> =
        FxHashSet::with_capacity_and_hasher(reachable.len(), Default::default());
    let mut post_order = Vec::with_capacity(reachable.len());
    struct Frame {
        block: BlockId,
        succ_i: usize,
    }
    visited.insert(cfg.entry);
    let mut stack = vec![Frame {
        block: cfg.entry,
        succ_i: 0,
    }];
    while let Some(frame) = stack.last_mut() {
        let succs = cfg.successors(frame.block);
        if frame.succ_i < succs.len() {
            let succ = succs[frame.succ_i];
            frame.succ_i += 1;
            if reachable.contains(&succ) && visited.insert(succ) {
                stack.push(Frame {
                    block: succ,
                    succ_i: 0,
                });
            }
        } else {
            let Frame { block, .. } = stack.pop().expect("stack non-empty");
            post_order.push(block);
        }
    }
    post_order.reverse();
    post_order
}

fn intersect_rpo(idom: &[u32], mut b1: u32, mut b2: u32) -> u32 {
    while b1 != b2 {
        while b1 > b2 {
            b1 = idom[b1 as usize];
        }
        while b2 > b1 {
            b2 = idom[b2 as usize];
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
        let preds = cfg.predecessors(*block);
        let reachable_pred_count = preds.iter().filter(|p| reachable.contains(p)).count();
        if reachable_pred_count < 2 {
            continue;
        }
        let block_idom = idom.get(block).copied().unwrap_or(cfg.entry);
        for &pred in preds {
            if !reachable.contains(&pred) {
                continue;
            }
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

/// Post-dominator tree containing post-dominance sets for each block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostDominatorTree {
    /// Post-dominators for each block (set of all blocks that post-dominate the block).
    pub post_dominators: HashMap<BlockId, HashSet<BlockId>>,
}

impl PostDominatorTree {
    /// Build post-dominator tree from CFG.
    pub fn build(cfg: &ControlFlowGraph) -> Self {
        compute_post_dominators(cfg)
    }

    /// Returns true if `candidate` post-dominates `node`.
    pub fn post_dominates(&self, candidate: BlockId, node: BlockId) -> bool {
        self.post_dominators
            .get(&node)
            .map(|set| set.contains(&candidate))
            .unwrap_or(false)
    }

    /// Backwards compatibility / PDG helper.
    pub fn immediately_post_dominates(&self, candidate: BlockId, node: BlockId) -> bool {
        self.post_dominates(candidate, node)
    }
}

/// Compute post-dominators for a CFG via iterative dataflow.
pub fn compute_post_dominators(cfg: &ControlFlowGraph) -> PostDominatorTree {
    let all_blocks: HashSet<BlockId> = cfg.blocks.keys().copied().collect();
    let top: Arc<HashSet<BlockId>> = Arc::new(all_blocks.clone());
    let mut post_dom: HashMap<BlockId, Arc<HashSet<BlockId>>> = HashMap::new();

    let mut exit_blocks: HashSet<BlockId> = cfg.exits.iter().copied().collect();
    if exit_blocks.is_empty() {
        for &block in &all_blocks {
            if cfg.successors(block).is_empty() {
                exit_blocks.insert(block);
            }
        }
    }
    if exit_blocks.is_empty() && !all_blocks.is_empty() {
        exit_blocks.insert(cfg.entry);
    }

    for &block in &all_blocks {
        if exit_blocks.contains(&block) {
            post_dom.insert(block, Arc::new(HashSet::from([block])));
        } else {
            post_dom.insert(block, Arc::clone(&top));
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for &block in &all_blocks {
            if exit_blocks.contains(&block) {
                continue;
            }
            let succs = cfg.successors(block);
            if succs.is_empty() {
                continue;
            }
            let mut intersection = intersect_post_dom_sets(&post_dom, succs);
            intersection.insert(block);
            let next = Arc::new(intersection);
            if !post_dom_sets_equal(post_dom.get(&block), &next) {
                post_dom.insert(block, next);
                changed = true;
            }
        }
    }

    PostDominatorTree {
        post_dominators: post_dom
            .into_iter()
            .map(|(block, set)| {
                (
                    block,
                    Arc::try_unwrap(set).unwrap_or_else(|arc| (*arc).clone()),
                )
            })
            .collect(),
    }
}

fn intersect_post_dom_sets(
    post_dom: &HashMap<BlockId, Arc<HashSet<BlockId>>>,
    succs: &[BlockId],
) -> HashSet<BlockId> {
    let &smallest_succ = succs
        .iter()
        .filter(|s| post_dom.contains_key(s))
        .min_by_key(|&&s| post_dom[&s].len())
        .unwrap_or(&succs[0]);
    let Some(smallest) = post_dom.get(&smallest_succ) else {
        return HashSet::new();
    };
    if succs.len() == 1 {
        return smallest.as_ref().clone();
    }
    smallest
        .iter()
        .filter(|b| succs.iter().all(|s| post_dom.get(s).map_or(false, |set| set.contains(*b))))
        .copied()
        .collect()
}

fn post_dom_sets_equal(
    current: Option<&Arc<HashSet<BlockId>>>,
    next: &Arc<HashSet<BlockId>>,
) -> bool {
    current.map(|c| c.as_ref() == next.as_ref()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{BasicBlock, BlockId, CfgEdgeType};
    use crate::cfg_builder::build_cfg_for_function;

    fn empty_block(id: BlockId) -> BasicBlock {
        BasicBlock {
            id,
            statements: Vec::new(),
            start_line: 0,
            end_line: 0,
        }
    }

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

    #[test]
    fn rpo_starts_at_entry_and_covers_reachable() {
        let code = r#"
fn nested(n: i32) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < n {
        if i % 2 == 0 {
            s += i;
        }
        i += 1;
    }
    s
}
"#;
        let cfg = build_cfg_for_function("rust", code, "nested").unwrap();
        let reachable = cfg.reachable_blocks();
        let rpo = compute_rpo(&cfg, &reachable);
        assert_eq!(rpo.first().copied(), Some(cfg.entry));
        assert_eq!(rpo.len(), reachable.len());
        let rpo_set: HashSet<_> = rpo.into_iter().collect();
        assert_eq!(rpo_set, reachable);
    }

    #[test]
    fn diamond_join_idom_is_entry() {
        let mut cfg = ControlFlowGraph::new();
        let entry = cfg.entry;
        let left = BlockId(1);
        let right = BlockId(2);
        let join = BlockId(3);
        cfg.add_block(empty_block(left));
        cfg.add_block(empty_block(right));
        cfg.add_block(empty_block(join));
        cfg.add_edge(entry, left, CfgEdgeType::IfTrue);
        cfg.add_edge(entry, right, CfgEdgeType::IfFalse);
        cfg.add_edge(left, join, CfgEdgeType::Next);
        cfg.add_edge(right, join, CfgEdgeType::Next);

        let dom = DominatorTree::build(&cfg);
        assert_eq!(dom.idom.get(&entry), Some(&entry));
        assert_eq!(dom.idom.get(&left), Some(&entry));
        assert_eq!(dom.idom.get(&right), Some(&entry));
        assert_eq!(dom.idom.get(&join), Some(&entry));
        assert!(dom.dominates(entry, join));
        assert!(!dom.dominates(left, right));
        assert!(verify_idom_acyclic(&dom.idom));
    }
}
