//! SCC-based blast radius engine using dense bitsets.
//!
//! This module provides a high-performance blast radius analyzer that:
//! 1. Condenses the graph into SCCs (Strongly Connected Components)
//! 2. Builds a DAG from the condensed graph
//! 3. Precomputes reachability using topological propagation
//! 4. Provides O(1) blast radius lookups
//!
//! Performance characteristics:
//! - Build time: O(V + E) for SCC + O(V² / 64) for bitset propagation
//! - Query time: O(1) bitset read
//! - Memory: O(V² / 64) for dense bitsets (~3.4 GB for 150K nodes)

use bit_set::BitSet;
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use rbuilder_graph::schema::{EdgeType, NodeType};
use std::collections::HashMap;
use uuid::Uuid;

/// A strongly connected component in the condensed graph.
#[derive(Debug, Clone)]
pub struct SccNode {
    /// SCC identifier (index in the DAG)
    pub id: usize,
    /// Member node UUIDs in this SCC
    pub members: Vec<Uuid>,
    /// Representative node name (for display)
    pub name: String,
}

/// Blast radius analysis engine using SCC condensation + dense bitsets.
pub struct BlastRadiusEngine {
    /// SCC-condensed DAG
    dag: DiGraph<SccNode, ()>,
    /// Original node UUID → SCC index mapping
    node_to_scc: HashMap<Uuid, NodeIndex>,
    /// SCC index → original node UUIDs mapping
    scc_members: Vec<Vec<Uuid>>,
    /// Precomputed reachability bitsets (one per SCC)
    /// reachability[i] = set of all SCCs reachable FROM SCC i
    reachability: Vec<BitSet>,
    /// Total number of SCCs
    scc_count: usize,
}

impl BlastRadiusEngine {
    /// Build the engine from a memory backend.
    ///
    /// This performs:
    /// 1. SCC decomposition (Kosaraju's algorithm)
    /// 2. DAG condensation
    /// 3. Topological sort
    /// 4. Reachability propagation in reverse topo order
    pub fn build(backend: &MemoryBackend) -> Result<Self> {
        use crate::graph_utils::PetGraphView;

        let view = PetGraphView::from_backend(backend)?;
        let graph = &view.directed;

        // Step 1: Find strongly connected components
        let sccs = kosaraju_scc(graph);
        let scc_count = sccs.len();

        tracing::info!(
            scc_count,
            original_nodes = graph.node_count(),
            reduction_percent = ((graph.node_count() - scc_count) as f64 / graph.node_count() as f64 * 100.0),
            "SCC decomposition complete"
        );

        // Step 2: Build node → SCC mapping
        let mut node_to_scc_idx: HashMap<NodeIndex, usize> = HashMap::new();
        let mut scc_members: Vec<Vec<Uuid>> = vec![Vec::new(); scc_count];

        for (scc_id, component) in sccs.iter().enumerate() {
            for &node_idx in component {
                node_to_scc_idx.insert(node_idx, scc_id);
                if let Some(uuid) = view.index_to_uuid.get(&node_idx) {
                    scc_members[scc_id].push(*uuid);
                }
            }
        }

        // Step 3: Build UUID → SCC mapping
        let mut node_to_scc: HashMap<Uuid, NodeIndex> = HashMap::new();
        for (scc_id, members) in scc_members.iter().enumerate() {
            for &uuid in members {
                node_to_scc.insert(uuid, NodeIndex::new(scc_id));
            }
        }

        // Step 4: Build condensed DAG
        let mut dag: DiGraph<SccNode, ()> = DiGraph::new();
        let mut scc_node_indices: Vec<NodeIndex> = Vec::with_capacity(scc_count);

        for (scc_id, members) in scc_members.iter().enumerate() {
            // Choose representative name
            let name = if !members.is_empty() {
                // Find first function node, or use first member
                members.iter()
                    .find_map(|uuid| {
                        backend.get_node(*uuid).ok().flatten()
                            .filter(|n| n.node_type == NodeType::Function)
                            .map(|n| n.name.clone())
                    })
                    .unwrap_or_else(|| {
                        backend.get_node(members[0]).ok().flatten()
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| format!("SCC_{}", scc_id))
                    })
            } else {
                format!("SCC_{}", scc_id)
            };

            let scc_node = SccNode {
                id: scc_id,
                members: members.clone(),
                name,
            };

            let idx = dag.add_node(scc_node);
            scc_node_indices.push(idx);
        }

        // Step 5: Add edges between SCCs
        let mut added_edges: HashMap<(usize, usize), ()> = HashMap::new();

        for edge in graph.edge_references() {
            let from_scc = node_to_scc_idx[&edge.source()];
            let to_scc = node_to_scc_idx[&edge.target()];

            // Only add edges between different SCCs (skip self-loops)
            if from_scc != to_scc {
                let edge_key = (from_scc, to_scc);
                if !added_edges.contains_key(&edge_key) {
                    dag.add_edge(
                        scc_node_indices[from_scc],
                        scc_node_indices[to_scc],
                        (),
                    );
                    added_edges.insert(edge_key, ());
                }
            }
        }

        tracing::info!(
            dag_nodes = dag.node_count(),
            dag_edges = dag.edge_count(),
            "DAG condensation complete"
        );

        // Step 6: Topological sort
        let sorted = toposort(&dag, None)
            .map_err(|_| Error::GraphError("DAG contains cycles after SCC condensation".into()))?;

        // Step 7: Propagate reachability in FORWARD topological order for blast radius
        // Blast radius = who is affected if this node changes = who calls this node (transitively)
        // So we propagate BACKWARDS: each node accumulates its parents + their parents
        let mut reachability: Vec<BitSet> = vec![BitSet::new(); scc_count];

        for &scc_idx in sorted.iter() {
            let scc_id: usize = scc_idx.index();
            let mut reach = BitSet::new();

            // Node can be reached by itself
            reach.insert(scc_id);

            // Union with all parents' reachability (who can reach them, can reach us)
            for parent_idx in dag.neighbors_directed(scc_idx, petgraph::Direction::Incoming) {
                let parent_id = parent_idx.index();
                reach.union_with(&reachability[parent_id]);
            }

            reachability[scc_id] = reach;
        }

        let total_bits: usize = reachability.iter().map(|bs| bs.len()).sum();
        let avg_reachability = total_bits as f64 / scc_count as f64;

        tracing::info!(
            scc_count,
            avg_reachability,
            "Reachability propagation complete"
        );

        Ok(Self {
            dag,
            node_to_scc,
            scc_members,
            reachability,
            scc_count,
        })
    }

    /// Analyze blast radius for a function by UUID.
    ///
    /// Returns the set of all UUIDs that are reachable (upstream callers).
    pub fn analyze(&self, func_id: Uuid) -> Result<BlastRadiusResult> {
        let scc_idx = self.node_to_scc
            .get(&func_id)
            .ok_or_else(|| Error::NodeNotFound(func_id.to_string()))?;

        let scc_id = scc_idx.index();
        let reachable_sccs = &self.reachability[scc_id];

        // Expand SCCs to individual node UUIDs
        let mut impact_zone_ids = Vec::new();
        for scc in reachable_sccs.iter() {
            for &uuid in &self.scc_members[scc] {
                if uuid != func_id {  // Exclude the function itself
                    impact_zone_ids.push(uuid);
                }
            }
        }

        // Calculate direct callers (nodes in SCCs that have edges TO this SCC)
        let mut direct_caller_ids = Vec::new();
        for incoming_scc in self.dag.neighbors_directed(*scc_idx, petgraph::Direction::Incoming) {
            for &uuid in &self.scc_members[incoming_scc.index()] {
                direct_caller_ids.push(uuid);
            }
        }

        // Score based on impact size
        let impact_count = impact_zone_ids.len();
        let direct_count = direct_caller_ids.len();

        let score = calculate_impact_score(direct_count, impact_count);

        Ok(BlastRadiusResult {
            symbol_id: func_id,
            direct_caller_ids,
            impact_zone_ids,
            score,
            scc_id,
            scc_size: self.scc_members[scc_id].len(),
        })
    }

    /// Get reach centrality (blast radius size) for all functions.
    ///
    /// This is essentially free - just the cardinality of each SCC's reachability bitset.
    pub fn reach_centrality(&self) -> HashMap<Uuid, usize> {
        let mut centrality = HashMap::new();

        for (scc_id, members) in self.scc_members.iter().enumerate() {
            let reach = self.reachability[scc_id].len();
            for &uuid in members {
                centrality.insert(uuid, reach);
            }
        }

        centrality
    }

    /// Get statistics about the engine.
    pub fn stats(&self) -> EngineStats {
        let memory_bytes = self.scc_count * self.scc_count / 8;  // Dense bitset
        let avg_scc_size = self.scc_members.iter().map(|m| m.len()).sum::<usize>() as f64
            / self.scc_count as f64;

        EngineStats {
            scc_count: self.scc_count,
            dag_edges: self.dag.edge_count(),
            avg_scc_size,
            memory_mb: memory_bytes as f64 / (1024.0 * 1024.0),
        }
    }
}

/// Result of blast radius analysis.
#[derive(Debug, Clone)]
pub struct BlastRadiusResult {
    /// The analyzed function UUID
    pub symbol_id: Uuid,
    /// Direct callers (immediate predecessors)
    pub direct_caller_ids: Vec<Uuid>,
    /// Full impact zone (transitive callers, excluding self)
    pub impact_zone_ids: Vec<Uuid>,
    /// Impact score (0-100)
    pub score: f64,
    /// SCC ID this function belongs to
    pub scc_id: usize,
    /// Number of functions in the same SCC (cycle size)
    pub scc_size: usize,
}

/// Engine statistics.
#[derive(Debug, Clone)]
pub struct EngineStats {
    /// Number of SCCs
    pub scc_count: usize,
    /// Number of edges in the DAG
    pub dag_edges: usize,
    /// Average SCC size
    pub avg_scc_size: f64,
    /// Memory usage in MB
    pub memory_mb: f64,
}

fn calculate_impact_score(direct_count: usize, impact_count: usize) -> f64 {
    if direct_count == 0 && impact_count == 0 {
        return 0.0;
    }

    // Direct callers: 0-40 points (capped)
    let direct_component = (direct_count as f64 * 25.0).min(40.0);

    // Transitive impact: 0-60 points (capped)
    let transitive_component = (impact_count as f64 * 0.05).min(60.0);

    (direct_component + transitive_component).min(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node};

    fn build_chain() -> MemoryBackend {
        let mut backend = MemoryBackend::new();

        // Build: a → b → c → d
        let a = Node::new(NodeType::Function, "a".to_string());
        let b = Node::new(NodeType::Function, "b".to_string());
        let c = Node::new(NodeType::Function, "c".to_string());
        let d = Node::new(NodeType::Function, "d".to_string());

        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        let id_d = d.id;

        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend.insert_node(d).unwrap();

        backend.insert_edge(Edge::new(id_a, id_b, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_b, id_c, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_c, id_d, EdgeType::Calls)).unwrap();

        backend
    }

    fn build_with_cycle() -> MemoryBackend {
        let mut backend = build_chain();
        let nodes = backend.all_nodes().unwrap();

        // Add cycle: d → b (creates SCC {b, c, d})
        let id_b = nodes.iter().find(|n| n.name == "b").unwrap().id;
        let id_d = nodes.iter().find(|n| n.name == "d").unwrap().id;

        backend.insert_edge(Edge::new(id_d, id_b, EdgeType::Calls)).unwrap();

        backend
    }

    #[test]
    fn test_scc_chain() {
        let backend = build_chain();
        let engine = BlastRadiusEngine::build(&backend).unwrap();

        // Chain has 4 SCCs (no cycles)
        assert_eq!(engine.scc_count, 4);
        assert_eq!(engine.dag.node_count(), 4);
        assert_eq!(engine.dag.edge_count(), 3);
    }

    #[test]
    fn test_scc_with_cycle() {
        let backend = build_with_cycle();
        let engine = BlastRadiusEngine::build(&backend).unwrap();

        // Should collapse b, c, d into one SCC
        // Total: {a}, {b, c, d} = 2 SCCs
        assert_eq!(engine.scc_count, 2);

        // Find the large SCC
        let large_scc = engine.scc_members.iter()
            .find(|members| members.len() == 3)
            .expect("Should have SCC with 3 members");

        assert_eq!(large_scc.len(), 3);
    }

    #[test]
    fn test_blast_radius_lookup() {
        let backend = build_chain();
        let engine = BlastRadiusEngine::build(&backend).unwrap();

        let nodes = backend.all_nodes().unwrap();
        let id_d = nodes.iter().find(|n| n.name == "d").unwrap().id;

        let result = engine.analyze(id_d).unwrap();

        // d is called by c, and transitively by a, b
        assert_eq!(result.direct_caller_ids.len(), 1);  // c
        assert_eq!(result.impact_zone_ids.len(), 3);    // a, b, c
        assert!(result.score > 0.0);
    }

    #[test]
    fn test_reach_centrality() {
        let backend = build_chain();
        let engine = BlastRadiusEngine::build(&backend).unwrap();

        let centrality = engine.reach_centrality();

        // Each node should have a reach value
        assert_eq!(centrality.len(), 4);

        // Node 'd' (leaf) has highest reach (everyone reaches it)
        let nodes = backend.all_nodes().unwrap();
        let id_d = nodes.iter().find(|n| n.name == "d").unwrap().id;
        let reach_d = centrality[&id_d];

        assert!(reach_d >= 4);  // Reaches at least itself and its SCC
    }
}
