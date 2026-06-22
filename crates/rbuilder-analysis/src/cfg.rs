//! Control flow graph representation and queries.

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Identifier for a basic block in a CFG.
pub type BlockId = Uuid;

/// A control-flow graph for a single function body.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Basic blocks keyed by id.
    pub blocks: HashMap<BlockId, BasicBlock>,
    /// Directed edges between blocks.
    pub edges: Vec<CfgEdge>,
    /// Entry block id.
    pub entry: BlockId,
    /// Exit block ids (returns, implicit fall-through exits).
    pub exits: Vec<BlockId>,
}

/// A sequence of statements with no internal branches.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Block id.
    pub id: BlockId,
    /// Statements in this block.
    pub statements: Vec<Statement>,
    /// First source line (1-based).
    pub start_line: usize,
    /// Last source line (1-based).
    pub end_line: usize,
}

/// A single statement in a basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Statement classification.
    pub kind: StatementKind,
    /// Source line (1-based).
    pub line: usize,
    /// Source text.
    pub text: String,
}

/// High-level statement categories for CFG/PDG analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// General expression.
    Expression,
    /// Assignment / mutation.
    Assignment,
    /// Variable declaration (`let`, etc.).
    Declaration,
    /// Function or method call.
    FunctionCall,
    /// Return.
    Return,
    /// Branch predicate (if/match condition).
    Branch,
    /// Unstructured jump (break/continue/goto).
    Jump,
}

/// Directed edge in the CFG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgEdge {
    /// Source block.
    pub from: BlockId,
    /// Target block.
    pub to: BlockId,
    /// Edge classification.
    pub edge_type: CfgEdgeType,
}

/// Classification of control-flow edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfgEdgeType {
    /// Sequential fall-through.
    Next,
    /// Conditional true branch.
    IfTrue,
    /// Conditional false branch.
    IfFalse,
    /// Back-edge or unstructured jump.
    Jump,
    /// Return to function exit.
    Return,
    /// Exception handler edge.
    Exception,
}

impl ControlFlowGraph {
    /// Create an empty CFG with a fresh entry block.
    pub fn new() -> Self {
        let entry = Uuid::new_v4();
        let mut blocks = HashMap::new();
        blocks.insert(
            entry,
            BasicBlock {
                id: entry,
                statements: Vec::new(),
                start_line: 0,
                end_line: 0,
            },
        );
        Self {
            blocks,
            edges: Vec::new(),
            entry,
            exits: Vec::new(),
        }
    }

    /// Insert a basic block.
    pub fn add_block(&mut self, block: BasicBlock) {
        self.blocks.insert(block.id, block);
    }

    /// Add a directed edge.
    pub fn add_edge(&mut self, from: BlockId, to: BlockId, edge_type: CfgEdgeType) {
        self.edges.push(CfgEdge {
            from,
            to,
            edge_type,
        });
    }

    /// Predecessor block ids for `block_id`.
    pub fn predecessors(&self, block_id: BlockId) -> Vec<BlockId> {
        self.edges
            .iter()
            .filter(|e| e.to == block_id)
            .map(|e| e.from)
            .collect()
    }

    /// Successor block ids for `block_id`.
    pub fn successors(&self, block_id: BlockId) -> Vec<BlockId> {
        self.edges
            .iter()
            .filter(|e| e.from == block_id)
            .map(|e| e.to)
            .collect()
    }

    /// Returns true when the CFG contains a cycle reachable from entry.
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        Self::dfs_cycle(self, self.entry, &mut visited, &mut rec_stack)
    }

    fn dfs_cycle(
        cfg: &ControlFlowGraph,
        node: BlockId,
        visited: &mut HashSet<BlockId>,
        rec_stack: &mut HashSet<BlockId>,
    ) -> bool {
        visited.insert(node);
        rec_stack.insert(node);

        for succ in cfg.successors(node) {
            if !visited.contains(&succ) {
                if Self::dfs_cycle(cfg, succ, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack.contains(&succ) {
                return true;
            }
        }

        rec_stack.remove(&node);
        false
    }

    /// All simple paths from `from` to `to` (acyclic path enumeration).
    pub fn find_paths(&self, from: BlockId, to: BlockId) -> Vec<Vec<BlockId>> {
        let mut paths = Vec::new();
        let mut current_path = vec![from];
        let mut visited = HashSet::new();
        self.dfs_paths(from, to, &mut current_path, &mut visited, &mut paths);
        paths
    }

    fn dfs_paths(
        &self,
        current: BlockId,
        target: BlockId,
        path: &mut Vec<BlockId>,
        visited: &mut HashSet<BlockId>,
        paths: &mut Vec<Vec<BlockId>>,
    ) {
        if current == target {
            paths.push(path.clone());
            return;
        }

        visited.insert(current);

        for succ in self.successors(current) {
            if !visited.contains(&succ) {
                path.push(succ);
                self.dfs_paths(succ, target, path, visited, paths);
                path.pop();
            }
        }

        visited.remove(&current);
    }

    /// Export the CFG as Graphviz DOT for debugging.
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph CFG {\n");
        for (id, block) in &self.blocks {
            let label = block
                .statements
                .iter()
                .map(|s| s.text.replace('"', "\\\""))
                .collect::<Vec<_>>()
                .join("\\n");
            let label = if label.is_empty() {
                format!("block {}", &id.to_string()[..8])
            } else {
                label
            };
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\"];\n",
                id,
                label.replace('\n', "\\n")
            ));
        }
        for edge in &self.edges {
            let style = match edge.edge_type {
                CfgEdgeType::IfTrue => " [label=\"T\"]",
                CfgEdgeType::IfFalse => " [label=\"F\"]",
                CfgEdgeType::Jump => " [label=\"jump\" style=dashed]",
                CfgEdgeType::Return => " [label=\"return\" color=red]",
                CfgEdgeType::Exception => " [label=\"except\" color=orange]",
                CfgEdgeType::Next => "",
            };
            out.push_str(&format!(
                "  \"{}\" -> \"{}\"{};\n",
                edge.from, edge.to, style
            ));
        }
        out.push_str("}\n");
        out
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();
        let b1 = Uuid::new_v4();
        let b2 = Uuid::new_v4();
        let exit = Uuid::new_v4();
        cfg.add_block(BasicBlock {
            id: b1,
            statements: vec![Statement {
                kind: StatementKind::Expression,
                line: 1,
                text: "a".into(),
            }],
            start_line: 1,
            end_line: 1,
        });
        cfg.add_block(BasicBlock {
            id: b2,
            statements: vec![Statement {
                kind: StatementKind::Expression,
                line: 2,
                text: "b".into(),
            }],
            start_line: 2,
            end_line: 2,
        });
        cfg.add_edge(cfg.entry, b1, CfgEdgeType::Next);
        cfg.add_edge(b1, b2, CfgEdgeType::Next);
        cfg.add_edge(b2, exit, CfgEdgeType::Return);
        cfg.exits.push(exit);
        cfg
    }

    #[test]
    fn test_predecessors_successors() {
        let cfg = linear_cfg();
        let b2 = cfg
            .blocks
            .values()
            .find(|b| b.statements.iter().any(|s| s.text == "b"))
            .unwrap()
            .id;
        let preds = cfg.predecessors(b2);
        assert_eq!(preds.len(), 1);
        assert_eq!(cfg.successors(preds[0]).len(), 1);
    }

    #[test]
    fn test_find_paths() {
        let cfg = linear_cfg();
        let b2 = cfg
            .blocks
            .values()
            .find(|b| b.statements.iter().any(|s| s.text == "b"))
            .unwrap()
            .id;
        let exit = cfg.exits[0];
        let paths = cfg.find_paths(b2, exit);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].last(), Some(&exit));
    }

    #[test]
    fn test_has_cycle_loop() {
        let mut cfg = ControlFlowGraph::new();
        let header = Uuid::new_v4();
        let body = Uuid::new_v4();
        cfg.add_block(BasicBlock {
            id: header,
            statements: vec![],
            start_line: 1,
            end_line: 1,
        });
        cfg.add_block(BasicBlock {
            id: body,
            statements: vec![],
            start_line: 2,
            end_line: 2,
        });
        cfg.add_edge(cfg.entry, header, CfgEdgeType::Next);
        cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        cfg.add_edge(body, header, CfgEdgeType::Jump);
        assert!(cfg.has_cycle());
    }

    #[test]
    fn test_to_dot_contains_nodes() {
        let cfg = linear_cfg();
        let dot = cfg.to_dot();
        assert!(dot.contains("digraph CFG"));
        assert!(dot.contains("->"));
    }
}
