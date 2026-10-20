//! Program dependence graph construction from CFG and def-use facts.

use crate::cfg::{BlockId, ControlFlowGraph, Statement};
use crate::dataflow::{ReachingDefs, compute_reaching_definitions};
use crate::dominance::{DominatorTree, compute_post_dominators};
use rgctl_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Identifier for a PDG node.
pub type PdgNodeId = Uuid;

/// Program dependence graph combining data and control dependencies.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProgramDependenceGraph {
    /// PDG nodes keyed by id.
    pub nodes: HashMap<PdgNodeId, PdgNode>,
    /// Data dependence edges.
    pub data_deps: Vec<DataDependency>,
    /// Control dependence edges.
    pub control_deps: Vec<ControlDependency>,
    /// Map from CFG block to PDG node ids in that block.
    #[serde(default)]
    block_nodes: HashMap<BlockId, Vec<PdgNodeId>>,
    /// Outgoing data-dependence adjacency (rebuilt on load when empty).
    #[serde(default)]
    data_succ: HashMap<PdgNodeId, Vec<PdgNodeId>>,
    /// Statement line → PDG node ids (rebuilt on load when empty).
    #[serde(default)]
    line_nodes: HashMap<usize, Vec<PdgNodeId>>,
    #[serde(skip)]
    seen_data_edges: HashSet<(PdgNodeId, PdgNodeId, u32, u8)>,
    #[serde(skip)]
    var_intern: HashMap<String, u32>,
    #[serde(skip)]
    next_var_id: u32,
}

impl<'de> Deserialize<'de> for ProgramDependenceGraph {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Stored {
            nodes: HashMap<PdgNodeId, PdgNode>,
            data_deps: Vec<DataDependency>,
            control_deps: Vec<ControlDependency>,
            #[serde(default)]
            block_nodes: HashMap<BlockId, Vec<PdgNodeId>>,
            #[serde(default)]
            data_succ: HashMap<PdgNodeId, Vec<PdgNodeId>>,
            #[serde(default)]
            line_nodes: HashMap<usize, Vec<PdgNodeId>>,
        }
        let stored = Stored::deserialize(deserializer)?;
        Ok(Self::from_parts(
            stored.nodes,
            stored.data_deps,
            stored.control_deps,
            stored.block_nodes,
            stored.data_succ,
            stored.line_nodes,
        ))
    }
}

/// A PDG node representing one statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataDependency {
    /// Definition node.
    pub from: PdgNodeId,
    /// Use node.
    pub to: PdgNodeId,
    /// Variable linking the dependence.
    pub variable: String,
    /// Dependence classification.
    pub dep_type: DataDepType,
    /// True when the dependence crosses a CFG cycle (loop-carried).
    ///
    /// Populated when PDG is built with [`PdgBuildOptions::classify_loop_carried`].
    #[serde(default)]
    pub loop_carried: bool,
}

/// Options for PDG construction (hybrid CPG P3 tiers).
#[derive(Debug, Clone, Copy, Default)]
pub struct PdgBuildOptions {
    /// Tag data deps that participate in a CFG cycle (T1 / `--with-dfg-loops`).
    pub classify_loop_carried: bool,
}

/// Data dependence classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataDepType {
    /// True (flow) dependence: def then use.
    Flow,
    /// Anti dependence: use then def.
    Anti,
    /// Output dependence: def then def.
    Output,
}

/// Control dependence between statements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDependency {
    /// Controlling statement.
    pub controller: PdgNodeId,
    /// Controlled statement.
    pub dependent: PdgNodeId,
}

impl ProgramDependenceGraph {
    pub(crate) fn from_parts(
        nodes: HashMap<PdgNodeId, PdgNode>,
        data_deps: Vec<DataDependency>,
        control_deps: Vec<ControlDependency>,
        block_nodes: HashMap<BlockId, Vec<PdgNodeId>>,
        data_succ: HashMap<PdgNodeId, Vec<PdgNodeId>>,
        line_nodes: HashMap<usize, Vec<PdgNodeId>>,
    ) -> Self {
        Self {
            nodes,
            data_deps,
            control_deps,
            block_nodes,
            data_succ,
            line_nodes,
            seen_data_edges: HashSet::new(),
            var_intern: HashMap::new(),
            next_var_id: 0,
        }
    }

    fn intern_var(&mut self, variable: &str) -> u32 {
        if let Some(&id) = self.var_intern.get(variable) {
            return id;
        }
        let id = self.next_var_id;
        self.next_var_id += 1;
        self.var_intern.insert(variable.to_string(), id);
        id
    }

    /// Build a PDG from a CFG and source bytes for def-use refinement.
    pub fn build(cfg: &ControlFlowGraph, source: &[u8]) -> Result<Self> {
        Self::build_with_options(cfg, source, PdgBuildOptions::default())
    }

    /// Build a PDG reusing a precomputed dominator tree.
    pub fn build_with_dominator(
        cfg: &ControlFlowGraph,
        source: &[u8],
        dom: &DominatorTree,
    ) -> Result<Self> {
        Self::build_with_dominator_options(cfg, source, dom, PdgBuildOptions::default())
    }

    /// Build with explicit tier options (loop-carried classification, …).
    pub fn build_with_options(
        cfg: &ControlFlowGraph,
        source: &[u8],
        options: PdgBuildOptions,
    ) -> Result<Self> {
        let dom = DominatorTree::build(cfg);
        Self::build_with_dominator_options(cfg, source, &dom, options)
    }

    /// Build with a precomputed dominator tree and tier options.
    pub fn build_with_dominator_options(
        cfg: &ControlFlowGraph,
        source: &[u8],
        dom: &DominatorTree,
        options: PdgBuildOptions,
    ) -> Result<Self> {
        let mut pdg = Self::default();
        pdg.create_nodes_from_cfg(cfg);
        pdg.rebuild_line_nodes();
        let _ = source;
        let reaching = compute_reaching_definitions(cfg, &pdg);
        pdg.build_data_dependencies(cfg, &reaching);
        pdg.build_control_dependencies_dominance(cfg, dom);
        if options.classify_loop_carried {
            pdg.classify_loop_carried(cfg);
        }
        Ok(pdg)
    }

    /// Mark data dependencies that close a CFG cycle (use can reach def).
    pub fn classify_loop_carried(&mut self, cfg: &ControlFlowGraph) {
        for dep in &mut self.data_deps {
            let Some(from_node) = self.nodes.get(&dep.from) else {
                continue;
            };
            let Some(to_node) = self.nodes.get(&dep.to) else {
                continue;
            };
            dep.loop_carried = cfg_block_can_reach(cfg, to_node.block, from_node.block);
        }
    }

    /// Ensure adjacency index exists (no-op when already built).
    pub fn ensure_data_succ(&mut self) {
        if !self.data_succ.is_empty() || self.data_deps.is_empty() {
            return;
        }
        self.rebuild_data_succ();
    }

    /// Rebuild in-memory indexes after bincode/JSON deserialization.
    pub fn restore_derived_indexes(&mut self) {
        self.rebuild_block_nodes();
        self.rebuild_data_succ();
        self.rebuild_line_nodes();
        self.seen_data_edges.clear();
    }

    fn rebuild_block_nodes(&mut self) {
        self.block_nodes.clear();
        for node in self.nodes.values() {
            self.block_nodes
                .entry(node.block)
                .or_default()
                .push(node.id);
        }
    }

    fn rebuild_line_nodes(&mut self) {
        self.line_nodes.clear();
        for node in self.nodes.values() {
            self.line_nodes
                .entry(node.statement.line)
                .or_default()
                .push(node.id);
        }
    }

    /// PDG nodes whose statement is on `line`.
    pub fn nodes_at_line(&self, line: usize) -> &[PdgNodeId] {
        self.line_nodes
            .get(&line)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn rebuild_data_succ(&mut self) {
        self.data_succ.clear();
        for dep in &self.data_deps {
            self.data_succ.entry(dep.from).or_default().push(dep.to);
        }
    }

    /// Build outgoing data-dependence adjacency (for taint / traversal).
    pub fn data_succ_map(&self) -> HashMap<PdgNodeId, Vec<PdgNodeId>> {
        if !self.data_succ.is_empty() {
            return self.data_succ.clone();
        }
        let mut map: HashMap<PdgNodeId, Vec<PdgNodeId>> = HashMap::new();
        for dep in &self.data_deps {
            map.entry(dep.from).or_default().push(dep.to);
        }
        map
    }

    fn create_nodes_from_cfg(&mut self, cfg: &ControlFlowGraph) {
        for block in cfg.blocks.values() {
            for stmt in &block.statements {
                let id = Uuid::new_v4();
                let node = PdgNode {
                    id,
                    statement: stmt.clone(),
                    block: block.id,
                    defined_vars: stmt.defined_vars.iter().map(|d| d.name()).collect(),
                    used_vars: stmt.used_vars.iter().cloned().collect(),
                };
                self.block_nodes.entry(block.id).or_default().push(id);
                self.nodes.insert(id, node);
            }
        }
    }

    fn build_data_dependencies(&mut self, cfg: &ControlFlowGraph, reaching: &ReachingDefs) {
        for block in cfg.blocks.values() {
            let node_ids: Vec<PdgNodeId> =
                self.block_nodes.get(&block.id).cloned().unwrap_or_default();

            for (idx, _stmt) in block.statements.iter().enumerate() {
                let Some(use_node) = self.find_node_by_block_and_index(block.id, idx) else {
                    continue;
                };
                let use_id = use_node.id;
                let used_vars = use_node.used_vars.clone();

                for var in used_vars {
                    for def in reaching.in_definitions(block.id).filter(|d| d.variable == var) {
                        self.push_data_dep(
                            def.pdg_node,
                            use_id,
                            var.clone(),
                            DataDepType::Flow,
                        );
                    }

                    for def_idx in 0..idx {
                        if let Some(def_node) = self.find_node_by_block_and_index(block.id, def_idx)
                        {
                            if def_node.defined_vars.contains(&var) {
                                self.push_data_dep(
                                    def_node.id,
                                    use_id,
                                    var.clone(),
                                    DataDepType::Flow,
                                );
                            } else if def_node.used_vars.contains(&var) {
                                self.push_data_dep(
                                    def_node.id,
                                    use_id,
                                    var.clone(),
                                    DataDepType::Anti,
                                );
                            }
                        }
                    }
                }
            }

            for (i, &later_id) in node_ids.iter().enumerate() {
                let defined: Vec<String> = self
                    .nodes
                    .get(&later_id)
                    .map(|n| n.defined_vars.iter().cloned().collect())
                    .unwrap_or_default();
                for var in defined {
                    for &earlier_id in node_ids.iter().take(i) {
                        if self
                            .nodes
                            .get(&earlier_id)
                            .is_some_and(|n| n.defined_vars.contains(&var))
                        {
                            self.push_data_dep(
                                earlier_id,
                                later_id,
                                var.clone(),
                                DataDepType::Output,
                            );
                        }
                    }
                }
            }
        }
    }

    fn push_data_dep(
        &mut self,
        from: PdgNodeId,
        to: PdgNodeId,
        variable: String,
        dep_type: DataDepType,
    ) {
        if from == to {
            return;
        }
        let var_id = self.intern_var(&variable);
        let key = (from, to, var_id, dep_type as u8);
        if !self.seen_data_edges.insert(key) {
            return;
        }
        self.data_deps.push(DataDependency {
            from,
            to,
            variable,
            dep_type,
            loop_carried: false,
        });
        self.data_succ.entry(from).or_default().push(to);
    }

    /// Precise control dependencies via dominance frontiers (Phase 13.2).
    fn build_control_dependencies_dominance(
        &mut self,
        cfg: &ControlFlowGraph,
        dom_tree: &DominatorTree,
    ) {
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
        if !self.data_succ.is_empty() {
            return self.data_succ.get(&node_id).cloned().unwrap_or_default();
        }
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

        let has_succ = !self.data_succ.is_empty();

        let mut max_depth = 0usize;
        for seed in seeds {
            let mut queue = std::collections::VecDeque::from([(seed, 0usize)]);
            let mut visited = HashSet::new();
            while let Some((node, depth)) = queue.pop_front() {
                if !visited.insert(node) {
                    continue;
                }
                max_depth = max_depth.max(depth);
                if has_succ {
                    if let Some(next) = self.data_succ.get(&node) {
                        for &child in next {
                            queue.push_back((child, depth + 1));
                        }
                    }
                } else {
                    for dep in self.data_deps.iter().filter(|d| d.from == node) {
                        queue.push_back((dep.to, depth + 1));
                    }
                }
            }
        }
        max_depth
    }
}

/// True if `from` can reach `to` following CFG successors (including trivial `from == to`).
fn cfg_block_can_reach(cfg: &ControlFlowGraph, from: BlockId, to: BlockId) -> bool {
    if from == to {
        return true;
    }
    let mut seen = HashSet::new();
    let mut stack = vec![from];
    while let Some(b) = stack.pop() {
        if !seen.insert(b) {
            continue;
        }
        if b == to {
            return true;
        }
        for &succ in cfg.successors(b) {
            stack.push(succ);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;

    #[test]
    fn test_pdg_bincode_roundtrip() {
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let bytes = bincode::serialize(&pdg).unwrap();
        let _: ProgramDependenceGraph = bincode::deserialize(&bytes).unwrap();
    }

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

    #[test]
    fn test_data_flow_depth_uses_data_succ() {
        let code = r#"
fn chain(a: i32) -> i32 {
    let x = a + 1;
    let y = x + 1;
    let z = y + 1;
    z
}
"#;
        let cfg = build_cfg_for_function("rust", code, "chain").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        assert!(!pdg.data_succ.is_empty());
        assert!(pdg.data_flow_depth_for_symbol("a") >= 2);

        let mut reloaded: ProgramDependenceGraph =
            bincode::deserialize(&bincode::serialize(&pdg).expect("serialize pdg"))
                .expect("deserialize pdg");
        reloaded.data_succ.clear();
        assert!(pdg.data_flow_depth_for_symbol("a") >= 2);
        assert!(reloaded.data_flow_depth_for_symbol("a") >= 2);
    }

    #[test]
    fn test_loop_carried_classification() {
        let code = r#"
fn sum(n: i32) -> i32 {
    let mut acc = 0;
    let mut i = 0;
    while i < n {
        acc = acc + i;
        i = i + 1;
    }
    acc
}
"#;
        let cfg = build_cfg_for_function("rust", code, "sum").unwrap();
        let pdg = ProgramDependenceGraph::build_with_options(
            &cfg,
            code.as_bytes(),
            PdgBuildOptions {
                classify_loop_carried: true,
            },
        )
        .unwrap();
        let carried = pdg.data_deps.iter().filter(|d| d.loop_carried).count();
        assert!(
            carried > 0,
            "expected at least one loop-carried dep, deps={:?}",
            pdg.data_deps
        );
        let independent = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        assert!(independent.data_deps.iter().all(|d| !d.loop_carried));
    }

    #[test]
    fn test_guard_clause_control_deps_cover_true_and_fallthrough() {
        let code = r#"
fn guard(flag: bool, a: i32, b: i32) -> i32 {
    if flag {
        return a;
    }
    return b;
}
"#;
        let cfg = build_cfg_for_function("rust", code, "guard").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();

        let branch = pdg
            .nodes
            .values()
            .find(|n| {
                n.statement.kind == crate::cfg::StatementKind::Branch
                    && n.statement.text.contains("flag")
            })
            .expect("branch on flag");
        let ret_a = pdg
            .nodes
            .values()
            .find(|n| {
                n.statement.kind == crate::cfg::StatementKind::Return
                    && n.statement.text.contains('a')
            })
            .expect("return a");
        let ret_b = pdg
            .nodes
            .values()
            .find(|n| {
                n.statement.kind == crate::cfg::StatementKind::Return
                    && n.statement.text.contains('b')
            })
            .expect("return b");
        let from_flag: Vec<_> = pdg
            .control_deps
            .iter()
            .filter(|c| c.controller == branch.id)
            .collect();
        assert!(
            from_flag.iter().any(|c| c.dependent == ret_a.id),
            "expected control dep flag → return a, deps={from_flag:?}"
        );
        assert!(
            from_flag.iter().any(|c| c.dependent == ret_b.id),
            "expected control dep flag → fallthrough b, deps={from_flag:?}"
        );
        assert!(
            from_flag.len() >= 2,
            "expected ≥2 control deps from flag, got {}",
            from_flag.len()
        );

        let slicer = crate::slicing::BackwardSlicer::new(&pdg, &cfg);
        let slice = slicer
            .slice(crate::slicing::SliceCriterion {
                variable: "b".into(),
                line: ret_b.statement.line,
            })
            .expect("backward slice");
        let texts: Vec<_> = slice
            .statements
            .iter()
            .filter_map(|id| pdg.nodes.get(id).map(|n| n.statement.text.as_str()))
            .collect();
        assert!(
            texts.iter().any(|t| t.contains("flag")),
            "backward slice from fallthrough must include flag, got {texts:?}"
        );
    }
}
