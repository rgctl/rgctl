//! Control flow graph representation and queries.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

/// Dense per-function identifier for a basic block (0..num_blocks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

impl BlockId {
    /// Construct from a dense index.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl Serialize for BlockId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(u32::deserialize(deserializer)?))
    }
}

/// A variable definition site on a CFG statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DefVar {
    /// Simple local / parameter name.
    Local(String),
    /// Field assignment target `receiver.member`.
    Field {
        /// Base local (`order`, `this`, …).
        receiver: String,
        /// Member name (`status`).
        member: String,
    },
}

impl DefVar {
    /// Construct a local definition.
    pub fn local(name: impl Into<String>) -> Self {
        Self::Local(name.into())
    }

    /// Canonical name for data-flow / PDG (`obj.field` or local name).
    pub fn name(&self) -> String {
        match self {
            Self::Local(s) => s.clone(),
            Self::Field { receiver, member } => format!("{receiver}.{member}"),
        }
    }

    /// True when this definition matches `name` (exact `name()` equality).
    pub fn defines_name(&self, name: &str) -> bool {
        match self {
            Self::Local(s) => s == name,
            Self::Field { receiver, member } => name
                .strip_prefix(receiver)
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|m| m == member),
        }
    }

    /// Receiver local for [`Self::Field`], if any.
    pub fn field_receiver(&self) -> Option<&str> {
        match self {
            Self::Field { receiver, .. } => Some(receiver.as_str()),
            _ => None,
        }
    }
}

fn def_var_from_wire(s: String) -> DefVar {
    if let Some((receiver, member)) = s.rsplit_once('.') {
        if !receiver.is_empty()
            && !member.is_empty()
            && !receiver.contains('(')
            && !member.contains('(')
        {
            return DefVar::Field {
                receiver: receiver.to_string(),
                member: member.to_string(),
            };
        }
    }
    DefVar::Local(s)
}

impl Serialize for DefVar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for DefVar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(def_var_from_wire(s))
    }
}

/// A control-flow graph for a single function body.
#[derive(Debug, Clone, Serialize)]
pub struct ControlFlowGraph {
    /// Basic blocks keyed by id.
    pub blocks: HashMap<BlockId, BasicBlock>,
    /// Directed edges between blocks.
    pub edges: Vec<CfgEdge>,
    /// Entry block id.
    pub entry: BlockId,
    /// Exit block ids (returns, implicit fall-through exits).
    pub exits: Vec<BlockId>,
    /// Typed locals + formals (name → bare type), populated at CFG build (not persisted).
    #[serde(skip)]
    pub local_types: HashMap<String, String>,
    /// Successor lists keyed by block id (derived from [`Self::edges`], not serialized).
    #[serde(skip)]
    succ: HashMap<BlockId, Vec<BlockId>>,
    /// Predecessor lists keyed by block id (derived from [`Self::edges`], not serialized).
    #[serde(skip)]
    pred: HashMap<BlockId, Vec<BlockId>>,
}

/// A sequence of statements with no internal branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    /// Statement classification.
    pub kind: StatementKind,
    /// Source line (1-based).
    pub line: usize,
    /// Source text.
    pub text: String,
    /// Variables defined by this statement (tree-sitter extraction).
    #[serde(default)]
    pub defined_vars: SmallVec<[DefVar; 2]>,
    /// Variables used by this statement (tree-sitter extraction).
    #[serde(default)]
    pub used_vars: SmallVec<[String; 3]>,
}

impl Statement {
    /// True when this statement defines `name` (local or `obj.field`).
    pub fn defines(&self, name: &str) -> bool {
        self.defined_vars.iter().any(|d| d.defines_name(name))
    }
}

/// High-level statement categories for CFG/PDG analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgEdge {
    /// Source block.
    pub from: BlockId,
    /// Target block.
    pub to: BlockId,
    /// Edge classification.
    pub edge_type: CfgEdgeType,
}

/// Classification of control-flow edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        let entry = BlockId(0);
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
            local_types: HashMap::new(),
            succ: HashMap::new(),
            pred: HashMap::new(),
        }
    }

    /// Rebuild cached adjacency lists from [`Self::edges`].
    pub fn rebuild_adjacency(&mut self) {
        self.succ.clear();
        self.pred.clear();
        for edge in &self.edges {
            self.succ.entry(edge.from).or_default().push(edge.to);
            self.pred.entry(edge.to).or_default().push(edge.from);
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
        self.succ.entry(from).or_default().push(to);
        self.pred.entry(to).or_default().push(from);
    }

    /// Predecessor block ids for `block_id`.
    pub fn predecessors(&self, block_id: BlockId) -> &[BlockId] {
        static EMPTY: &[BlockId] = &[];
        self.pred
            .get(&block_id)
            .map(|v| v.as_slice())
            .unwrap_or(EMPTY)
    }

    /// Successor block ids for `block_id`.
    pub fn successors(&self, block_id: BlockId) -> &[BlockId] {
        static EMPTY: &[BlockId] = &[];
        self.succ
            .get(&block_id)
            .map(|v| v.as_slice())
            .unwrap_or(EMPTY)
    }

    /// Blocks reachable from the entry block.
    pub fn reachable_blocks(&self) -> HashSet<BlockId> {
        let mut reachable = HashSet::new();
        let mut stack = vec![self.entry];
        while let Some(block) = stack.pop() {
            if !reachable.insert(block) {
                continue;
            }
            for &succ in self.successors(block) {
                if !reachable.contains(&succ) {
                    stack.push(succ);
                }
            }
        }
        reachable
    }

    /// Remove blocks not reachable from entry (dead code after return, etc.).
    pub fn prune_unreachable_blocks(&mut self) {
        let reachable = self.reachable_blocks();
        self.blocks.retain(|id, _| reachable.contains(id));
        self.edges
            .retain(|e| reachable.contains(&e.from) && reachable.contains(&e.to));
        self.exits.retain(|id| reachable.contains(id));
        self.rebuild_adjacency();
    }

    /// Contract non-exit empty pass-through blocks (`b --Next--> s`) into their successor.
    ///
    /// A block `b` is contracted when it has zero statements, is not an exit, and all of its
    /// outgoing edges are `CfgEdgeType::Next` to a single successor `s != b`. Incoming edges
    /// keep their [`CfgEdgeType`]. Exit blocks and empty loop back-edges are preserved.
    pub fn contract_empty_blocks(&mut self) {
        loop {
            let candidate = self.find_contractable_empty_block();
            let Some((b, s)) = candidate else {
                break;
            };

            for edge in &mut self.edges {
                if edge.to == b {
                    edge.to = s;
                }
            }
            self.edges.retain(|e| e.from != b);

            let mut seen = HashSet::with_capacity(self.edges.len());
            self.edges
                .retain(|e| seen.insert((e.from, e.to, e.edge_type)));

            if self.entry == b {
                self.entry = s;
            }
            self.blocks.remove(&b);
            self.rebuild_adjacency();
        }
    }

    fn find_contractable_empty_block(&self) -> Option<(BlockId, BlockId)> {
        for &b in self.blocks.keys() {
            if self.exits.contains(&b) {
                continue;
            }
            let Some(block) = self.blocks.get(&b) else {
                continue;
            };
            if !block.statements.is_empty() {
                continue;
            }

            let outs: Vec<&CfgEdge> = self.edges.iter().filter(|e| e.from == b).collect();
            if outs.is_empty() {
                continue;
            }
            let s = outs[0].to;
            if s == b {
                continue;
            }
            if !outs
                .iter()
                .all(|e| e.edge_type == CfgEdgeType::Next && e.to == s)
            {
                continue;
            }
            // Do not contract a loop-header entry that still has predecessors.
            if b == self.entry && !self.predecessors(b).is_empty() {
                continue;
            }
            if !self.blocks.contains_key(&s) {
                continue;
            }
            return Some((b, s));
        }
        None
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

        for &succ in cfg.successors(node) {
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

        for &succ in self.successors(current) {
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
                format!("block {}", id.0)
            } else {
                label
            };
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\"];\n",
                id.0,
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
                edge.from.0, edge.to.0, style
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

impl<'de> Deserialize<'de> for ControlFlowGraph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ControlFlowGraphData {
            blocks: HashMap<BlockId, BasicBlock>,
            edges: Vec<CfgEdge>,
            entry: BlockId,
            exits: Vec<BlockId>,
        }

        let data = ControlFlowGraphData::deserialize(deserializer)?;
        let mut cfg = Self {
            blocks: data.blocks,
            edges: data.edges,
            entry: data.entry,
            exits: data.exits,
            local_types: HashMap::new(),
            succ: HashMap::new(),
            pred: HashMap::new(),
        };
        cfg.rebuild_adjacency();
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_cfg() -> ControlFlowGraph {
        let mut cfg = ControlFlowGraph::new();
        let b1 = BlockId(1);
        let b2 = BlockId(2);
        let exit = BlockId(3);
        cfg.add_block(BasicBlock {
            id: b1,
            statements: vec![Statement {
                kind: StatementKind::Expression,
                line: 1,
                text: "a".into(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
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
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
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
        let header = BlockId(1);
        let body = BlockId(2);
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
    fn test_defines_name_field_no_alloc() {
        let def = DefVar::Field {
            receiver: "order".into(),
            member: "status".into(),
        };
        assert!(def.defines_name("order.status"));
        assert!(!def.defines_name("order.id"));
    }

    #[test]
    fn test_to_dot_contains_nodes() {
        let cfg = linear_cfg();
        let dot = cfg.to_dot();
        assert!(dot.contains("digraph CFG"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_contract_empty_blocks_preserves_exits_and_edge_kinds() {
        let mut cfg = ControlFlowGraph::new();
        let entry = cfg.entry;
        let trampoline = BlockId(1);
        let cond = BlockId(2);
        let then_b = BlockId(3);
        let exit = BlockId(4);
        cfg.add_block(BasicBlock {
            id: trampoline,
            statements: vec![],
            start_line: 0,
            end_line: 0,
        });
        cfg.add_block(BasicBlock {
            id: cond,
            statements: vec![Statement {
                kind: StatementKind::Branch,
                line: 1,
                text: "x".into(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            }],
            start_line: 1,
            end_line: 1,
        });
        cfg.add_block(BasicBlock {
            id: then_b,
            statements: vec![Statement {
                kind: StatementKind::Return,
                line: 2,
                text: "return".into(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            }],
            start_line: 2,
            end_line: 2,
        });
        cfg.add_block(BasicBlock {
            id: exit,
            statements: vec![],
            start_line: 0,
            end_line: 0,
        });
        cfg.add_edge(entry, trampoline, CfgEdgeType::Next);
        cfg.add_edge(trampoline, cond, CfgEdgeType::Next);
        cfg.add_edge(cond, then_b, CfgEdgeType::IfTrue);
        cfg.add_edge(cond, exit, CfgEdgeType::IfFalse);
        cfg.add_edge(then_b, exit, CfgEdgeType::Return);
        cfg.exits.push(exit);

        cfg.contract_empty_blocks();

        assert!(!cfg.blocks.contains_key(&trampoline));
        assert_eq!(cfg.entry, cond);
        assert!(cfg.exits.contains(&exit));
        assert!(cfg.blocks.contains_key(&exit));
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.from == cond && e.to == exit && e.edge_type == CfgEdgeType::IfFalse)
        );
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.from == then_b && e.to == exit && e.edge_type == CfgEdgeType::Return)
        );
    }
}
