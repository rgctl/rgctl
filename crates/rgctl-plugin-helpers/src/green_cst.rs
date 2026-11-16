//! Succinct pre-order Green-CST builder for Tier-1 lossless archives (`RGCT`).
//!
//! Built on Rayon extract workers when `--lossless` is active. Zero pointer-heavy
//! AST copies: only flat `bp_bits` / `kind_ids` / relative width columns.

/// Packed `(lead_trivia_len: u16, byte_width: u16)` — overflow widths (≥ 65535) use
/// `u16::MAX` and should be recorded separately by the caller if needed.
#[inline]
pub fn pack_rel_delta_and_width(lead_trivia_len: u16, byte_width: u16) -> u32 {
    (u32::from(lead_trivia_len) << 16) | u32::from(byte_width)
}

/// Unpack [`pack_rel_delta_and_width`].
#[inline]
pub fn unpack_rel_delta_and_width(packed: u32) -> (u16, u16) {
    ((packed >> 16) as u16, packed as u16)
}

/// Pre-order Balanced-Parentheses Green-CST accumulator for one source file.
#[derive(Debug, Default, Clone)]
pub struct GreenCstFileBuilder {
    /// Bit-packed open/close topology (`1` = open, `0` = close), little-endian words.
    pub bp_bits: Vec<u64>,
    /// Number of bits written into [`Self::bp_bits`].
    pub bp_bit_len: u32,
    /// Tree-sitter `kind_id` per CST node (pre-order, one entry per open).
    pub kind_ids: Vec<u16>,
    /// Packed `(lead_trivia_len, byte_width)` per CST node (pre-order).
    pub rel_delta_and_width: Vec<u32>,
    /// Sparse `(cst_node_idx, graph_local_idx)` anchors (declaration / call sites).
    pub graph_anchors: Vec<(u32, u32)>,
    /// Running end-byte of the previous visited node (for lead trivia gaps).
    cursor_byte: u32,
    /// Stack of open CST node indices awaiting close bits.
    open_stack: Vec<u32>,
}

impl GreenCstFileBuilder {
    /// Create an empty builder with capacity hint for `approx_nodes` CST nodes.
    pub fn with_capacity(approx_nodes: usize) -> Self {
        Self {
            bp_bits: Vec::with_capacity(approx_nodes.div_ceil(32)),
            bp_bit_len: 0,
            kind_ids: Vec::with_capacity(approx_nodes),
            rel_delta_and_width: Vec::with_capacity(approx_nodes),
            graph_anchors: Vec::new(),
            cursor_byte: 0,
            open_stack: Vec::with_capacity(64),
        }
    }

    /// Push an open bit and columnar row for `node` (pre-order enter).
    ///
    /// Returns the CST node index (0-based, counting opens only).
    pub fn enter(&mut self, node: tree_sitter::Node) -> u32 {
        let start = node.start_byte() as u32;
        let end = node.end_byte() as u32;
        let lead = start.saturating_sub(self.cursor_byte);
        let width = end.saturating_sub(start);
        let lead_u16 = u16::try_from(lead).unwrap_or(u16::MAX);
        let width_u16 = u16::try_from(width).unwrap_or(u16::MAX);
        let idx = self.kind_ids.len() as u32;
        self.push_bit(true);
        self.kind_ids.push(node.kind_id());
        self.rel_delta_and_width
            .push(pack_rel_delta_and_width(lead_u16, width_u16));
        self.open_stack.push(idx);
        // Leaf nodes advance the cursor on enter; parents advance on exit after children.
        if node.child_count() == 0 {
            self.cursor_byte = end;
        }
        idx
    }

    /// Push a close bit for the most recent open (pre-order exit).
    pub fn exit(&mut self, _node: tree_sitter::Node) {
        let _ = self.open_stack.pop();
        self.push_bit(false);
    }

    /// Record a sparse CST→graph anchor (`graph_local_idx` is the symbol/relation local id).
    pub fn anchor(&mut self, cst_node_idx: u32, graph_local_idx: u32) {
        self.graph_anchors.push((cst_node_idx, graph_local_idx));
    }

    /// Walk `root` in iterative pre-order DFS, invoking `on_enter` after each enter.
    ///
    /// `on_enter` receives `(cst_idx, node)` and may call [`Self::anchor`].
    pub fn walk_tree<F>(&mut self, root: tree_sitter::Node, mut on_enter: F)
    where
        F: FnMut(&mut Self, u32, tree_sitter::Node),
    {
        // Explicit stack: (node, visiting_children)
        let mut stack: Vec<(tree_sitter::Node, usize, bool)> = Vec::with_capacity(64);
        let idx = self.enter(root);
        on_enter(self, idx, root);
        stack.push((root, 0, false));

        while let Some((node, child_i, closed)) = stack.pop() {
            if closed {
                self.exit(node);
                continue;
            }
            if child_i < node.child_count() {
                stack.push((node, child_i + 1, false));
                if let Some(child) = node.child(child_i) {
                    let cidx = self.enter(child);
                    on_enter(self, cidx, child);
                    stack.push((child, 0, false));
                }
            } else {
                self.exit(node);
            }
        }
    }

    fn push_bit(&mut self, open: bool) {
        let word = (self.bp_bit_len / 64) as usize;
        let bit = self.bp_bit_len % 64;
        if word >= self.bp_bits.len() {
            self.bp_bits.push(0);
        }
        if open {
            self.bp_bits[word] |= 1u64 << bit;
        }
        self.bp_bit_len += 1;
    }

    /// Number of CST nodes (opens).
    pub fn node_count(&self) -> u32 {
        self.kind_ids.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn pack_roundtrip() {
        let p = pack_rel_delta_and_width(3, 40);
        assert_eq!(unpack_rel_delta_and_width(p), (3, 40));
    }

    #[test]
    fn builder_balances_parens_on_rust_fn() {
        let src = "fn hello() {}\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mut b = GreenCstFileBuilder::with_capacity(32);
        b.walk_tree(tree.root_node(), |_, _, _| {});
        assert!(b.node_count() > 0);
        assert_eq!(b.bp_bit_len % 2, 0, "open/close bits must balance");
        assert_eq!(b.kind_ids.len(), b.rel_delta_and_width.len());
        // Sum of open bits should equal node_count
        let mut opens = 0u32;
        for i in 0..b.bp_bit_len {
            let word = b.bp_bits[(i / 64) as usize];
            if (word >> (i % 64)) & 1 == 1 {
                opens += 1;
            }
        }
        assert_eq!(opens, b.node_count());
    }
}
