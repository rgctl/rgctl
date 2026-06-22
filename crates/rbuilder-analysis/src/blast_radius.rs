//! Blast radius analysis (Phase 12.2).
//!
//! Computes downstream impact of changing a symbol, optionally enriched with PDG data
//! from [`FlowCache`].

use crate::flow_cache::FlowCache;
use crate::graph_utils::PetGraphView;
use petgraph::Direction;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::EdgeType;
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

/// Per-caller data-flow impact detail.
#[derive(Debug, Clone)]
pub struct DataFlowImpact {
    /// Caller node id
    pub caller: Uuid,
    /// Caller symbol name
    pub caller_name: String,
    /// Data-flow depth within the caller (0 when PDG unavailable)
    pub depth: usize,
}

/// Blast radius report for a symbol change.
#[derive(Debug, Clone)]
pub struct BlastRadiusReport {
    /// Target symbol node id
    pub symbol: Uuid,
    /// Target symbol name
    pub symbol_name: String,
    /// Impact score from 0.0 (none) to 100.0 (critical)
    pub score: f64,
    /// Direct callers of the symbol
    pub direct_callers: Vec<String>,
    /// Transitive callers (impact zone), excluding the symbol itself
    pub impact_zone: Vec<String>,
    /// Maximum data-flow depth observed across direct callers
    pub data_flow_depth: usize,
    /// Per-caller data-flow details
    pub data_flow_impact: Vec<DataFlowImpact>,
}

/// Analyzes blast radius using the in-memory graph backend.
pub struct BlastRadiusAnalyzer<'a> {
    backend: &'a MemoryBackend,
    flow_cache: Option<&'a FlowCache>,
    max_depth: usize,
}

impl<'a> BlastRadiusAnalyzer<'a> {
    /// Create an analyzer without PDG enrichment.
    pub fn new(backend: &'a MemoryBackend) -> Self {
        Self {
            backend,
            flow_cache: None,
            max_depth: 10,
        }
    }

    /// Attach an optional flow cache for PDG-backed data-flow depth.
    pub fn with_flow_cache(mut self, flow_cache: &'a FlowCache) -> Self {
        self.flow_cache = Some(flow_cache);
        self
    }

    /// Limit transitive caller traversal depth (default 10).
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Analyze blast radius by symbol name.
    pub fn analyze(&self, symbol_name: &str) -> Result<BlastRadiusReport> {
        let view = PetGraphView::from_backend(self.backend)?;
        let source = view
            .find_node_by_name(symbol_name)
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;
        self.analyze_node(&view, source.id, &source.name)
    }

    /// Analyze blast radius by node id.
    pub fn analyze_by_id(&self, symbol_id: Uuid) -> Result<BlastRadiusReport> {
        let view = PetGraphView::from_backend(self.backend)?;
        let name = view
            .nodes
            .iter()
            .find(|n| n.id == symbol_id)
            .map(|n| n.name.clone())
            .ok_or_else(|| Error::NodeNotFound(symbol_id.to_string()))?;
        self.analyze_node(&view, symbol_id, &name)
    }

    fn analyze_node(
        &self,
        view: &PetGraphView,
        symbol_id: Uuid,
        symbol_name: &str,
    ) -> Result<BlastRadiusReport> {
        let source_idx = view
            .uuid_to_directed
            .get(&symbol_id)
            .copied()
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;

        let direct_caller_ids = incoming_callers(view, source_idx);
        let direct_callers = uuid_list_to_names(view, &direct_caller_ids);

        let mut impact_ids = HashSet::new();
        let mut queue: VecDeque<(Uuid, usize)> =
            direct_caller_ids.iter().map(|id| (*id, 1usize)).collect();

        while let Some((caller_id, depth)) = queue.pop_front() {
            if depth > self.max_depth {
                continue;
            }
            if caller_id == symbol_id || !impact_ids.insert(caller_id) {
                continue;
            }
            let Some(caller_idx) = view.uuid_to_directed.get(&caller_id).copied() else {
                continue;
            };
            for pred in view
                .directed
                .neighbors_directed(caller_idx, Direction::Incoming)
            {
                if is_calls_edge(view, pred, caller_idx) {
                    if let Some(uuid) = view.directed_to_uuid.get(&pred) {
                        if *uuid != symbol_id {
                            queue.push_back((*uuid, depth + 1));
                        }
                    }
                }
            }
        }

        let mut data_flow_impact = Vec::new();
        let mut max_data_flow = 0usize;
        for caller_id in &direct_caller_ids {
            let caller_name = view
                .nodes
                .iter()
                .find(|n| n.id == *caller_id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| caller_id.to_string());
            let depth = self
                .flow_cache
                .and_then(|cache| cache.get_pdg(*caller_id))
                .map(|pdg| pdg.data_flow_depth_for_symbol(symbol_name))
                .unwrap_or(0);
            max_data_flow = max_data_flow.max(depth);
            data_flow_impact.push(DataFlowImpact {
                caller: *caller_id,
                caller_name,
                depth,
            });
        }

        let impact_zone = ids_to_names(view, &impact_ids);
        let score = calculate_score(
            direct_callers.len(),
            impact_zone.len(),
            max_data_flow,
            view,
            &impact_ids,
        );

        Ok(BlastRadiusReport {
            symbol: symbol_id,
            symbol_name: symbol_name.to_string(),
            score,
            direct_callers,
            impact_zone,
            data_flow_depth: max_data_flow,
            data_flow_impact,
        })
    }
}

fn incoming_callers(view: &PetGraphView, target_idx: petgraph::graph::NodeIndex) -> Vec<Uuid> {
    view.directed
        .neighbors_directed(target_idx, Direction::Incoming)
        .filter_map(|idx| {
            if is_calls_edge(view, idx, target_idx) {
                view.directed_to_uuid.get(&idx).copied()
            } else {
                None
            }
        })
        .collect()
}

fn is_calls_edge(
    view: &PetGraphView,
    from: petgraph::graph::NodeIndex,
    to: petgraph::graph::NodeIndex,
) -> bool {
    view.directed
        .find_edge(from, to)
        .and_then(|e| view.directed.edge_weight(e).copied())
        == Some(EdgeType::Calls)
}

fn uuid_list_to_names(view: &PetGraphView, ids: &[Uuid]) -> Vec<String> {
    let mut names: Vec<String> = ids
        .iter()
        .filter_map(|id| {
            view.nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.name.clone())
        })
        .collect();
    names.sort();
    names
}

fn ids_to_names(view: &PetGraphView, ids: &HashSet<Uuid>) -> Vec<String> {
    let mut names: Vec<String> = ids
        .iter()
        .filter_map(|id| {
            view.nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| n.name.clone())
        })
        .collect();
    names.sort();
    names
}

fn calculate_score(
    direct_count: usize,
    impact_count: usize,
    data_flow_depth: usize,
    view: &PetGraphView,
    impact_ids: &HashSet<Uuid>,
) -> f64 {
    if direct_count == 0 && impact_count == 0 {
        return 0.0;
    }

    let direct_component = (direct_count as f64 * 25.0).min(40.0);
    let transitive_component = (impact_count as f64 * 12.0).min(35.0);
    let flow_component = (data_flow_depth as f64 * 8.0).min(15.0);

    let avg_complexity = average_complexity(view, impact_ids);
    let complexity_component = (avg_complexity * 2.0).min(10.0);

    (direct_component + transitive_component + flow_component + complexity_component).min(100.0)
}

fn average_complexity(view: &PetGraphView, ids: &HashSet<Uuid>) -> f64 {
    if ids.is_empty() {
        return 0.0;
    }
    let sum: f64 = ids
        .iter()
        .filter_map(|id| {
            view.nodes.iter().find(|n| n.id == *id).and_then(|n| {
                n.get_property("complexity")
                    .or_else(|| n.get_property("cyclomatic_complexity"))
                    .and_then(|v| v.parse::<f64>().ok())
            })
        })
        .sum();
    let count = ids.len() as f64;
    if sum == 0.0 {
        1.0
    } else {
        sum / count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Statement, StatementKind};
    use crate::pdg::{DataDepType, DataDependency, PdgNode, ProgramDependenceGraph};
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node, NodeType};
    use std::collections::HashSet;

    fn build_chain() -> (MemoryBackend, Uuid, Uuid, Uuid) {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".to_string());
        let b = Node::new(NodeType::Function, "b".to_string());
        let c = Node::new(NodeType::Function, "c".to_string());
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend
            .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
            .unwrap();
        (backend, id_a, id_b, id_c)
    }

    fn test_pdg_for_caller() -> ProgramDependenceGraph {
        let block = Uuid::new_v4();
        let n1 = Uuid::new_v4();
        let n2 = Uuid::new_v4();
        let mut pdg = ProgramDependenceGraph::default();
        pdg.nodes.insert(
            n1,
            PdgNode {
                id: n1,
                statement: Statement {
                    kind: StatementKind::Expression,
                    line: 1,
                    text: "let tmp = c()".into(),
                },
                block,
                defined_vars: ["tmp"].into_iter().map(String::from).collect(),
                used_vars: ["c"].into_iter().map(String::from).collect(),
            },
        );
        pdg.nodes.insert(
            n2,
            PdgNode {
                id: n2,
                statement: Statement {
                    kind: StatementKind::Return,
                    line: 2,
                    text: "return tmp".into(),
                },
                block,
                defined_vars: HashSet::new(),
                used_vars: ["tmp"].into_iter().map(String::from).collect(),
            },
        );
        pdg.data_deps.push(DataDependency {
            from: n1,
            to: n2,
            variable: "tmp".into(),
            dep_type: DataDepType::Flow,
        });
        pdg
    }

    #[test]
    fn test_blast_radius_simple() {
        let (backend, _, _, id_c) = build_chain();
        let report = BlastRadiusAnalyzer::new(&backend).analyze("c").unwrap();
        assert_eq!(report.direct_callers, vec!["b".to_string()]);
        assert_eq!(report.impact_zone.len(), 2);
        assert!(report.impact_zone.contains(&"a".to_string()));
        assert!(report.impact_zone.contains(&"b".to_string()));
        assert!(report.score > 50.0);
        assert_eq!(report.symbol, id_c);
    }

    #[test]
    fn test_blast_radius_leaf() {
        let mut backend = MemoryBackend::new();
        let leaf = Node::new(NodeType::Function, "leaf".to_string());
        backend.insert_node(leaf).unwrap();

        let report = BlastRadiusAnalyzer::new(&backend).analyze("leaf").unwrap();
        assert!(report.direct_callers.is_empty());
        assert!(report.impact_zone.is_empty());
        assert_eq!(report.score, 0.0);
        assert_eq!(report.data_flow_depth, 0);
    }

    #[test]
    fn test_blast_radius_with_pdg() {
        let (backend, _, id_b, _) = build_chain();
        let mut cache = FlowCache::new();
        cache.insert_pdg(id_b, test_pdg_for_caller());

        let report = BlastRadiusAnalyzer::new(&backend)
            .with_flow_cache(&cache)
            .analyze("c")
            .unwrap();
        assert_eq!(report.data_flow_depth, 1);
        assert_eq!(report.data_flow_impact.len(), 1);
        assert_eq!(report.data_flow_impact[0].caller, id_b);
    }
}
