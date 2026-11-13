//! Weisfeiler-Lehman (1-WL) canonical graph hashing for SESE hammocks.
//!
//! Maps SESE subgraphs to a canonical 64-bit structural hash in linear O(V + E) time
//! using 2 iterations of 1-WL color refinement. Guarantees invariance under variable
//! renaming (Type-2 clones), whitespace formatting, and comment insertion.

use crate::cfg::{BasicBlock, CfgEdgeType, ControlFlowGraph, Statement, StatementKind};
use crate::sese::{SeseRegion, is_comment};
use std::collections::{HashMap, HashSet};

/// Normalized statement kinds for canonical color refinement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalStatementKind {
    /// Conditional branch statement (`if`, `match`).
    If,
    /// Loop header or loop control statement (`for`, `while`, `loop`).
    Loop,
    /// Function or method invocation.
    Call,
    /// Variable or field mutation / assignment.
    Assign,
    /// Local variable declaration (`let`, `var`).
    Decl,
    /// Return statement.
    Return,
    /// Other unclassified expression or statement.
    Other,
}

impl CanonicalStatementKind {
    /// Classify a statement in the context of its basic block and CFG.
    pub fn classify(stmt: &Statement, block: Option<&BasicBlock>, cfg: &ControlFlowGraph) -> Self {
        match stmt.kind {
            StatementKind::Return => Self::Return,
            StatementKind::Declaration => Self::Decl,
            StatementKind::Assignment => Self::Assign,
            StatementKind::FunctionCall => Self::Call,
            StatementKind::Branch => {
                let trimmed = stmt.text.trim();
                if trimmed.starts_with("while")
                    || trimmed.starts_with("for")
                    || trimmed.starts_with("loop")
                {
                    Self::Loop
                } else if let Some(blk) = block {
                    // Check if outgoing edge is a loop back-edge
                    let has_back_edge = cfg
                        .edges
                        .iter()
                        .any(|e| e.from == blk.id && e.edge_type == CfgEdgeType::Jump);
                    if has_back_edge { Self::Loop } else { Self::If }
                } else {
                    Self::If
                }
            }
            StatementKind::Jump => {
                let trimmed = stmt.text.trim();
                if trimmed.starts_with("break") || trimmed.starts_with("continue") {
                    Self::Loop
                } else {
                    Self::Other
                }
            }
            StatementKind::Expression => {
                let trimmed = stmt.text.trim();
                if trimmed.contains('(') && trimmed.contains(')') {
                    Self::Call
                } else if trimmed.contains('=') {
                    Self::Assign
                } else {
                    Self::Other
                }
            }
        }
    }

    /// Base color byte tag for initial 1-WL coloring.
    pub fn base_tag(&self) -> &'static [u8] {
        match self {
            Self::If => b"STMT:IF",
            Self::Loop => b"STMT:LOOP",
            Self::Call => b"STMT:CALL",
            Self::Assign => b"STMT:ASSIGN",
            Self::Decl => b"STMT:DECL",
            Self::Return => b"STMT:RETURN",
            Self::Other => b"STMT:OTHER",
        }
    }
}

/// Normalized edge classifications for 1-WL neighbor propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CanonicalEdgeTag {
    /// Sequential fallthrough control flow.
    Next = 1,
    /// Conditional true branch edge.
    IfTrue = 2,
    /// Conditional false branch edge.
    IfFalse = 3,
    /// Jump or loop back-edge.
    Jump = 4,
    /// Def-use data dependence edge.
    DataDefUse = 5,
}

/// 1-WL canonical subgraph hasher.
pub struct WeisfeilerLehmanHasher;

impl WeisfeilerLehmanHasher {
    /// Compute the 64-bit canonical structural hash for an SESE region.
    pub fn hash_region(cfg: &ControlFlowGraph, region: &SeseRegion) -> String {
        let n = region.statements.len();
        if n == 0 {
            return format!("{:016x}", 0u64);
        }

        // Map statements to basic blocks and build intra-block ranges
        let mut stmt_to_block = Vec::with_capacity(n);
        let mut block_stmt_ranges = HashMap::new();
        let mut offset: usize = 0;
        let mut edges: Vec<(usize, usize, CanonicalEdgeTag)> = Vec::new();

        for &block_id in &region.blocks {
            if let Some(blk) = cfg.blocks.get(&block_id) {
                let start = offset;
                for stmt in &blk.statements {
                    if is_comment(&stmt.text) {
                        continue;
                    }
                    stmt_to_block.push(blk);
                    offset += 1;
                }
                let end = offset;
                block_stmt_ranges.insert(block_id, (start, end));
                for i in start..end.saturating_sub(1) {
                    edges.push((i, i + 1, CanonicalEdgeTag::Next));
                }
            }
        }

        // 1. Initial coloring (k = 0):
        // h^(0)(v) = BLAKE3(statement_kind(v))
        let mut h_curr = Vec::with_capacity(n);
        for (i, stmt) in region.statements.iter().enumerate() {
            let blk = stmt_to_block.get(i).copied();
            let kind = CanonicalStatementKind::classify(stmt, blk, cfg);
            let hash = blake3::hash(kind.base_tag());
            let val = u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap());
            h_curr.push(val);
        }

        // 2b. Inter-block control-flow edges
        let region_block_set: HashSet<_> = region.blocks.iter().copied().collect();
        for edge in &cfg.edges {
            if region_block_set.contains(&edge.from)
                && region_block_set.contains(&edge.to)
                && let (Some(&(from_start, from_end)), Some(&(to_start, to_end))) = (
                    block_stmt_ranges.get(&edge.from),
                    block_stmt_ranges.get(&edge.to),
                )
                && from_end > from_start
                && to_end > to_start
            {
                let last_from = from_end - 1;
                let first_to = to_start;
                let tag = match edge.edge_type {
                    CfgEdgeType::Next => CanonicalEdgeTag::Next,
                    CfgEdgeType::IfTrue => CanonicalEdgeTag::IfTrue,
                    CfgEdgeType::IfFalse => CanonicalEdgeTag::IfFalse,
                    CfgEdgeType::Jump => CanonicalEdgeTag::Jump,
                    CfgEdgeType::Return => CanonicalEdgeTag::Next,
                    CfgEdgeType::Exception => CanonicalEdgeTag::Jump,
                };
                edges.push((last_from, first_to, tag));
            }
        }

        // 2c. Def-use data dependency edges
        // Map defined variables to statement indices within the hammock
        for (i, stmt_i) in region.statements.iter().enumerate() {
            for def_var in &stmt_i.defined_vars {
                let name = def_var.name();
                for (j, stmt_j) in region.statements.iter().enumerate().skip(i + 1) {
                    if stmt_j.used_vars.iter().any(|u| u == &name) {
                        edges.push((i, j, CanonicalEdgeTag::DataDefUse));
                    }
                    // If statement j redefines the same variable, kill further propagation
                    if stmt_j.defined_vars.iter().any(|d| d.name() == name) {
                        break;
                    }
                }
            }
        }

        // Build adjacency lists: outgoing and incoming
        let mut out_adj: Vec<Vec<(usize, CanonicalEdgeTag)>> = vec![Vec::new(); n];
        let mut in_adj: Vec<Vec<(usize, CanonicalEdgeTag)>> = vec![Vec::new(); n];
        for (u, v, tag) in edges {
            if u < n && v < n {
                out_adj[u].push((v, tag));
                in_adj[v].push((u, tag));
            }
        }

        // 3. Color refinement for 2 iterations (k in {1, 2})
        let mut h_next = vec![0u64; n];
        for _round in 1..=2 {
            for i in 0..n {
                // Collect neighbors into a multiset
                let mut neighbor_entries = Vec::with_capacity(out_adj[i].len() + in_adj[i].len());
                for &(target, tag) in &out_adj[i] {
                    neighbor_entries.push((tag as u8, 1u8 /* outgoing */, h_curr[target]));
                }
                for &(source, tag) in &in_adj[i] {
                    neighbor_entries.push((tag as u8, 0u8 /* incoming */, h_curr[source]));
                }
                neighbor_entries.sort_unstable();

                let mut hasher = blake3::Hasher::new();
                hasher.update(&h_curr[i].to_le_bytes());
                for (tag, dir, neighbor_hash) in neighbor_entries {
                    hasher.update(&[tag, dir]);
                    hasher.update(&neighbor_hash.to_le_bytes());
                }
                let out = hasher.finalize();
                h_next[i] = u64::from_le_bytes(out.as_bytes()[0..8].try_into().unwrap());
            }
            h_curr.copy_from_slice(&h_next);
        }

        // 4. Fragment signature:
        // H_fragment = BLAKE3( h^(2)(entry) || XOR_{v in S} h^(2)(v) )
        let entry_idx = 0;
        let mut xor_sum = 0u64;
        for &val in &h_curr {
            xor_sum ^= val;
        }

        let mut final_hasher = blake3::Hasher::new();
        final_hasher.update(&h_curr[entry_idx].to_le_bytes());
        final_hasher.update(&xor_sum.to_le_bytes());
        let final_bytes = final_hasher.finalize();
        let final_u64 = u64::from_le_bytes(final_bytes.as_bytes()[0..8].try_into().unwrap());

        format!("{final_u64:016x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::sese::extract_sese_regions;

    #[test]
    fn test_wl_hash_variable_renaming_invariance() {
        let code_a = r#"
fn sum_pos_a(items: &[i32]) -> i32 {
    let mut total = 0;
    for x in items {
        if *x > 0 {
            total += x;
        }
    }
    total
}
"#;
        let code_b = r#"
fn sum_pos_b(data: &[i32]) -> i32 {
    let mut accumulator = 0;
    for element in data {
        if *element > 0 {
            accumulator += element;
        }
    }
    accumulator
}
"#;
        let cfg_a = build_cfg_for_function("rust", code_a, "sum_pos_a").unwrap();
        let cfg_b = build_cfg_for_function("rust", code_b, "sum_pos_b").unwrap();

        let regions_a = extract_sese_regions(&cfg_a, 3, 15);
        let regions_b = extract_sese_regions(&cfg_b, 3, 15);

        assert!(!regions_a.is_empty(), "regions_a non-empty");
        assert!(!regions_b.is_empty(), "regions_b non-empty");

        let hash_a = WeisfeilerLehmanHasher::hash_region(&cfg_a, &regions_a[0]);
        let hash_b = WeisfeilerLehmanHasher::hash_region(&cfg_b, &regions_b[0]);

        assert_eq!(
            hash_a, hash_b,
            "1-WL hashes must be identical under variable renaming (Type-2)"
        );
    }

    #[test]
    fn test_wl_hash_formatting_and_comment_invariance() {
        let code_base = r#"
fn calc(items: &[i32]) -> i32 {
    let mut s = 0;
    for x in items {
        if *x > 0 {
            s += x;
        }
    }
    s
}
"#;
        let code_formatted = r#"
fn calc(items: &[i32]) -> i32 {
    // Initial accumulation variable
    let mut s = 0;

    /* Loop through items */
    for x in items {
        // Guard check
        if *x > 0 {
            s += x;
        }
    }

    s
}
"#;
        let cfg_base = build_cfg_for_function("rust", code_base, "calc").unwrap();
        let cfg_formatted = build_cfg_for_function("rust", code_formatted, "calc").unwrap();

        let regions_base = extract_sese_regions(&cfg_base, 3, 15);
        let regions_formatted = extract_sese_regions(&cfg_formatted, 3, 15);

        let hash_base = WeisfeilerLehmanHasher::hash_region(&cfg_base, &regions_base[0]);
        let hash_formatted =
            WeisfeilerLehmanHasher::hash_region(&cfg_formatted, &regions_formatted[0]);

        assert_eq!(
            hash_base, hash_formatted,
            "1-WL hashes must be identical despite comments and extra formatting"
        );
    }

    #[test]
    fn test_wl_hash_structural_difference_changes_hash() {
        let code_if = r#"
fn branch_calc(items: &[i32]) -> i32 {
    let mut s = 0;
    for x in items {
        if *x > 0 {
            s += x;
        }
    }
    s
}
"#;
        let code_no_if = r#"
fn direct_calc(items: &[i32]) -> i32 {
    let mut s = 0;
    for x in items {
        s += x;
    }
    s
}
"#;
        let cfg_if = build_cfg_for_function("rust", code_if, "branch_calc").unwrap();
        let cfg_no_if = build_cfg_for_function("rust", code_no_if, "direct_calc").unwrap();

        let regions_if = extract_sese_regions(&cfg_if, 3, 15);
        let regions_no_if = extract_sese_regions(&cfg_no_if, 3, 15);

        let hash_if = WeisfeilerLehmanHasher::hash_region(&cfg_if, &regions_if[0]);
        let hash_no_if = WeisfeilerLehmanHasher::hash_region(&cfg_no_if, &regions_no_if[0]);

        assert_ne!(
            hash_if, hash_no_if,
            "Structural difference (with if vs without if) must yield different 1-WL hashes"
        );
    }
}
