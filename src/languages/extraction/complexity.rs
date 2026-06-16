//! Shared cyclomatic/cognitive complexity calculation for tree-sitter nodes

use tree_sitter::Node;

/// Generic complexity metrics calculator for tree-sitter AST nodes.
pub struct ComplexityCalculator;

impl ComplexityCalculator {
    /// Count cyclomatic complexity from branch/decision nodes.
    pub fn cyclomatic(node: Node, branch_kinds: &[&str]) -> usize {
        let mut complexity = 1;
        Self::walk(node, branch_kinds, &mut |kind| {
            if branch_kinds.contains(&kind) {
                complexity += 1;
            }
        });
        complexity
    }

    /// Count cognitive complexity with nesting penalty.
    pub fn cognitive(node: Node, branch_kinds: &[&str]) -> usize {
        let mut cognitive = 0;
        Self::walk_nested(node, branch_kinds, &mut |kind, nesting| {
            if branch_kinds.contains(&kind) {
                cognitive += 1 + nesting;
            }
        });
        cognitive
    }

    /// Count lines of code in a node span.
    pub fn loc(node: Node) -> usize {
        (node.end_position().row - node.start_position().row + 1).max(1)
    }

    /// Maximum nesting depth within container nodes.
    pub fn nesting_depth(node: Node, container_kinds: &[&str]) -> usize {
        let mut max_depth = 0;
        Self::walk_depth(node, container_kinds, &mut max_depth, 0);
        max_depth
    }

    /// Count return statements in a function node.
    pub fn return_count(node: Node, return_kind: &str) -> usize {
        let mut count = 0;
        Self::walk(node, &[], &mut |kind| {
            if kind == return_kind {
                count += 1;
            }
        });
        count
    }

    fn walk(node: Node, _branch_kinds: &[&str], visit: &mut dyn FnMut(&str)) {
        visit(node.kind());
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::walk(child, _branch_kinds, visit);
        }
    }

    fn walk_nested(node: Node, branch_kinds: &[&str], visit: &mut dyn FnMut(&str, usize)) {
        fn inner(node: Node, branch_kinds: &[&str], visit: &mut dyn FnMut(&str, usize), nesting: usize) {
            visit(node.kind(), nesting);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let next = if branch_kinds.contains(&child.kind()) {
                    nesting + 1
                } else {
                    nesting
                };
                inner(child, branch_kinds, visit, next);
            }
        }
        inner(node, branch_kinds, visit, 0);
    }

    fn walk_depth(node: Node, container_kinds: &[&str], max: &mut usize, depth: usize) {
        *max = (*max).max(depth);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let next = if container_kinds.contains(&child.kind()) {
                depth + 1
            } else {
                depth
            };
            Self::walk_depth(child, container_kinds, max, next);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[cfg(feature = "lang-rust")]
    #[test]
    fn test_cyclomatic_rust_if() {
        let source = b"fn f(x: i32) { if x > 0 { if x < 10 {} } }";
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_rust::language())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let func = tree.root_node().named_child(0).unwrap();
        let c = ComplexityCalculator::cyclomatic(func, &["if_expression", "match_expression"]);
        assert!(c >= 3);
    }
}
