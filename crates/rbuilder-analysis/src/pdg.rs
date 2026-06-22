//! Program dependence graph construction from CFG and def-use facts.

use crate::cfg::{BlockId, ControlFlowGraph, Statement};
use crate::dataflow::{compute_reaching_definitions, ReachingDefs};
use crate::dominance::DominatorTree;
use rbuilder_error::Result;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Identifier for a PDG node.
pub type PdgNodeId = Uuid;

/// Program dependence graph combining data and control dependencies.
#[derive(Debug, Clone, Default)]
pub struct ProgramDependenceGraph {
    /// PDG nodes keyed by id.
    pub nodes: HashMap<PdgNodeId, PdgNode>,
    /// Data dependence edges.
    pub data_deps: Vec<DataDependency>,
    /// Control dependence edges.
    pub control_deps: Vec<ControlDependency>,
    /// Map from CFG block to PDG node ids in that block.
    block_nodes: HashMap<BlockId, Vec<PdgNodeId>>,
}

/// A PDG node representing one statement.
#[derive(Debug, Clone)]
pub struct PdgNode {
    /// Node id.
    pub id: PdgNodeId,
    /// Underlying statement.
    pub statement: Statement,
    /// CFG block containing this statement.
    pub block: BlockId,
    /// Variables defined.
    pub defined_vars: HashSet<String>,
    /// Variables used.
    pub used_vars: HashSet<String>,
}

/// Data dependence between two statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDependency {
    /// Definition node.
    pub from: PdgNodeId,
    /// Use node.
    pub to: PdgNodeId,
    /// Variable linking the dependence.
    pub variable: String,
    /// Dependence classification.
    pub dep_type: DataDepType,
}

/// Data dependence classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDepType {
    /// True (flow) dependence: def then use.
    Flow,
    /// Anti dependence: use then def.
    Anti,
    /// Output dependence: def then def.
    Output,
}

/// Control dependence between statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlDependency {
    /// Controlling statement.
    pub controller: PdgNodeId,
    /// Controlled statement.
    pub dependent: PdgNodeId,
}

impl ProgramDependenceGraph {
    /// Build a PDG from a CFG and source bytes for def-use refinement.
    pub fn build(cfg: &ControlFlowGraph, source: &[u8]) -> Result<Self> {
        let mut pdg = Self::default();
        pdg.create_nodes_from_cfg(cfg);
        pdg.enrich_def_use_from_text(source);
        let reaching = compute_reaching_definitions(cfg, &pdg);
        pdg.build_data_dependencies(cfg, &reaching);
        pdg.build_control_dependencies(cfg);
        Ok(pdg)
    }

    fn create_nodes_from_cfg(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for stmt in &block.statements {
                let id = Uuid::new_v4();
                let node = PdgNode {
                    id,
                    statement: stmt.clone(),
                    block: block.id,
                    defined_vars: HashSet::new(),
                    used_vars: HashSet::new(),
                };
                self.block_nodes.entry(block.id).or_default().push(id);
                self.nodes.insert(id, node);
            }
        }
    }

    fn enrich_def_use_from_text(&mut self, source: &[u8]) {
        for node in self.nodes.values_mut() {
            infer_def_use(
                &node.statement.text,
                &mut node.defined_vars,
                &mut node.used_vars,
            );
            let _ = source;
        }
    }

    fn build_data_dependencies(&mut self, cfg: &ControlFlowGraph, reaching: &ReachingDefs) {
        for block in cfg.blocks.values() {
            for (idx, _stmt) in block.statements.iter().enumerate() {
                let Some(use_node) = self.find_node_by_block_and_index(block.id, idx) else {
                    continue;
                };
                let use_id = use_node.id;
                let used_vars = use_node.used_vars.clone();

                for var in used_vars {
                    if reaching.in_set.contains_key(&block.id) {
                        for def in reaching.in_set[&block.id]
                            .iter()
                            .filter(|d| d.variable == var)
                        {
                            self.data_deps.push(DataDependency {
                                from: def.pdg_node,
                                to: use_id,
                                variable: var.clone(),
                                dep_type: DataDepType::Flow,
                            });
                        }
                    }

                    for def_idx in 0..idx {
                        if let Some(def_node) = self.find_node_by_block_and_index(block.id, def_idx)
                        {
                            if def_node.defined_vars.contains(&var) {
                                self.data_deps.push(DataDependency {
                                    from: def_node.id,
                                    to: use_id,
                                    variable: var.clone(),
                                    dep_type: DataDepType::Flow,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn build_control_dependencies(&mut self, cfg: &ControlFlowGraph) {
        self.build_control_dependencies_dominance(cfg);
    }

    /// Precise control dependencies via dominance frontiers (Phase 13.2).
    fn build_control_dependencies_dominance(&mut self, cfg: &ControlFlowGraph) {
        let dom_tree = DominatorTree::build(cfg);

        for block_id in cfg.blocks.keys() {
            for &frontier_block in dom_tree.frontier(*block_id).iter() {
                let Some(controller_nodes) = self.block_nodes.get(block_id).cloned() else {
                    continue;
                };
                let Some(dependent_nodes) = self.block_nodes.get(&frontier_block).cloned() else {
                    continue;
                };
                let controller = controller_nodes
                    .iter()
                    .copied()
                    .find(|id| {
                        self.nodes
                            .get(id)
                            .map(|n| n.statement.kind == crate::cfg::StatementKind::Branch)
                            .unwrap_or(false)
                    })
                    .or_else(|| controller_nodes.last().copied());

                if let Some(controller) = controller {
                    for dependent in dependent_nodes {
                        if dependent != controller {
                            self.control_deps.push(ControlDependency {
                                controller,
                                dependent,
                            });
                        }
                    }
                }
            }
        }

        if self.control_deps.is_empty() {
            self.build_control_dependencies_postdom(cfg);
        }
    }

    /// Fallback post-dominator control dependencies (Phase 12).
    fn build_control_dependencies_postdom(&mut self, cfg: &ControlFlowGraph) {
        let post_dom = compute_post_dominators(cfg);

        for edge in &cfg.edges {
            if post_dom.immediately_post_dominates(edge.to, edge.from) {
                continue;
            }
            if let Some(controller) = self.primary_node_for_block(edge.from) {
                if let Some(ids) = self.block_nodes.get(&edge.to) {
                    for &dependent in ids {
                        if dependent != controller {
                            self.control_deps.push(ControlDependency {
                                controller,
                                dependent,
                            });
                        }
                    }
                }
            }
        }
    }

    fn primary_node_for_block(&self, block: BlockId) -> Option<PdgNodeId> {
        self.block_nodes
            .get(&block)
            .and_then(|ids| ids.first().copied())
    }

    /// Find a PDG node by block and statement line.
    pub fn find_node_by_block_and_line(&self, block: BlockId, line: usize) -> Option<&PdgNode> {
        self.block_nodes.get(&block).and_then(|ids| {
            ids.iter().find_map(|id| {
                let node = &self.nodes[id];
                if node.statement.line == line {
                    Some(node)
                } else {
                    None
                }
            })
        })
    }

    fn find_node_by_block_and_index(&self, block: BlockId, index: usize) -> Option<&PdgNode> {
        self.block_nodes
            .get(&block)
            .and_then(|ids| ids.get(index).map(|id| &self.nodes[id]))
    }

    /// Upstream definition nodes for a variable.
    pub fn get_dependencies(&self, var: &str) -> Vec<PdgNodeId> {
        self.data_deps
            .iter()
            .filter(|dep| dep.variable == var)
            .map(|dep| dep.from)
            .collect()
    }

    /// Downstream nodes depending on `node_id`.
    pub fn get_dependents(&self, node_id: PdgNodeId) -> Vec<PdgNodeId> {
        self.data_deps
            .iter()
            .filter(|dep| dep.from == node_id)
            .map(|dep| dep.to)
            .collect()
    }

    /// Maximum forward data-flow depth from statements that use `symbol_name`.
    pub fn data_flow_depth_for_symbol(&self, symbol_name: &str) -> usize {
        let seeds: Vec<PdgNodeId> = self
            .nodes
            .values()
            .filter(|n| n.used_vars.contains(symbol_name))
            .map(|n| n.id)
            .collect();
        if seeds.is_empty() {
            return 0;
        }

        let mut adjacency: HashMap<PdgNodeId, Vec<PdgNodeId>> = HashMap::new();
        for dep in &self.data_deps {
            adjacency.entry(dep.from).or_default().push(dep.to);
        }

        let mut max_depth = 0usize;
        for seed in seeds {
            let mut queue = std::collections::VecDeque::from([(seed, 0usize)]);
            let mut visited = HashSet::new();
            while let Some((node, depth)) = queue.pop_front() {
                if !visited.insert(node) {
                    continue;
                }
                max_depth = max_depth.max(depth);
                if let Some(next) = adjacency.get(&node) {
                    for &child in next {
                        queue.push_back((child, depth + 1));
                    }
                }
            }
        }
        max_depth
    }
}

fn infer_def_use(text: &str, defined: &mut HashSet<String>, used: &mut HashSet<String>) {
    let trimmed = text.trim();
    if trimmed.starts_with("let ") || trimmed.starts_with("let\t") {
        if let Some(rest) = trimmed.strip_prefix("let") {
            let rest = rest.trim().trim_start_matches("mut").trim();
            if let Some(name) = rest.split('=').next() {
                let name = name.trim();
                if is_ident(name) {
                    defined.insert(name.to_string());
                }
            }
        }
    } else if trimmed.contains('=') && !trimmed.contains("==") && !trimmed.contains("!=") {
        if let Some(left) = trimmed.split('=').next() {
            let name = left.trim();
            if is_ident(name) {
                defined.insert(name.to_string());
            }
        }
    }

    for token in trimmed.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token.is_empty() || is_keyword(token) {
            continue;
        }
        if !defined.contains(token) {
            used.insert(token.to_string());
        }
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "fn" | "let"
            | "mut"
            | "if"
            | "else"
            | "return"
            | "for"
            | "while"
            | "loop"
            | "match"
            | "def"
            | "class"
            | "import"
            | "from"
            | "in"
            | "true"
            | "false"
            | "None"
            | "Some"
            | "Ok"
            | "Err"
            | "i32"
            | "i64"
            | "usize"
    )
}

/// Post-dominator tree for control dependence.
#[derive(Debug, Clone)]
struct PostDominatorTree {
    ipdom: HashMap<BlockId, HashSet<BlockId>>,
}

impl PostDominatorTree {
    fn immediately_post_dominates(&self, candidate: BlockId, node: BlockId) -> bool {
        self.ipdom
            .get(&node)
            .map(|set| set.contains(&candidate))
            .unwrap_or(false)
    }
}

fn compute_post_dominators(cfg: &ControlFlowGraph) -> PostDominatorTree {
    let all_blocks: HashSet<BlockId> = cfg.blocks.keys().copied().collect();
    let mut post_dom: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

    for &block in &all_blocks {
        post_dom.insert(block, all_blocks.clone());
    }

    for exit in &cfg.exits {
        post_dom.insert(*exit, HashSet::from([*exit]));
    }

    let mut changed = true;
    while changed {
        changed = false;
        for &block in &all_blocks {
            if cfg.exits.contains(&block) {
                continue;
            }
            let succs = cfg.successors(block);
            if succs.is_empty() {
                continue;
            }
            let mut intersection = post_dom[&succs[0]].clone();
            for succ in &succs[1..] {
                intersection.retain(|b| post_dom[succ].contains(b));
            }
            intersection.insert(block);
            if post_dom.get(&block) != Some(&intersection) {
                post_dom.insert(block, intersection);
                changed = true;
            }
        }
    }

    PostDominatorTree { ipdom: post_dom }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;

    #[test]
    fn test_pdg_data_dependency() {
        let code = r#"
fn example(a: i32) -> i32 {
    let x = a + 1;
    let y = x * 2;
    y
}
"#;
        let cfg = build_cfg_for_function("rust", code, "example").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();

        let y_node = pdg
            .nodes
            .values()
            .find(|n| n.defined_vars.contains("y"))
            .expect("y node");

        let dep = pdg
            .data_deps
            .iter()
            .find(|d| d.to == y_node.id && d.variable == "x");
        assert!(
            dep.is_some(),
            "expected flow dep on x, deps: {:?}",
            pdg.data_deps
        );
    }

    #[test]
    fn test_get_dependencies() {
        let code = "fn f(a: i32) { let x = a; let y = x; }";
        let cfg = build_cfg_for_function("rust", code, "f").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let deps = pdg.get_dependents(
            pdg.nodes
                .values()
                .find(|n| n.defined_vars.contains("x"))
                .unwrap()
                .id,
        );
        assert!(!deps.is_empty());
    }
}
