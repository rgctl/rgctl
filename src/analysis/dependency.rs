//! Dependency analysis
//!
//! Task 2.1.4: Circular dependencies and impact radius.

use petgraph::visit::EdgeRef;
use crate::analysis::graph_utils::PetGraphView;
use crate::error::{Error, Result};
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::EdgeType;
use petgraph::algo::kosaraju_scc;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// A circular dependency (strongly connected component with >1 node).
#[derive(Debug, Clone)]
pub struct CircularDependency {
    /// Node UUIDs in the cycle
    pub nodes: Vec<Uuid>,
    /// Human-readable node names
    pub names: Vec<String>,
}

/// Impact analysis result.
#[derive(Debug, Clone)]
pub struct ImpactResult {
    /// Starting node UUID
    pub source: Uuid,
    /// Source node name
    pub source_name: String,
    /// All affected node UUIDs (transitive dependents)
    pub affected_nodes: Vec<Uuid>,
    /// Affected node names
    pub affected_names: Vec<String>,
    /// Maximum traversal depth reached
    pub max_depth: usize,
}

/// Dependency analysis engine.
pub struct DependencyAnalyzer;

impl DependencyAnalyzer {
    /// Find circular dependencies using strongly connected components.
    pub fn find_circular_dependencies(backend: &MemoryBackend) -> Result<Vec<CircularDependency>> {
        let view = PetGraphView::from_backend(backend)?;
        let sccs = kosaraju_scc(&view.directed);

        let mut cycles = Vec::new();
        for component in sccs {
            if component.len() <= 1 {
                continue;
            }
            let has_call_edge = component.iter().any(|&a| {
                view.directed
                    .edges(a)
                    .any(|e| *e.weight() == EdgeType::Calls && component.contains(&e.target()))
            });
            if !has_call_edge && component.len() == 1 {
                continue;
            }

            let uuids: Vec<Uuid> = component
                .iter()
                .filter_map(|idx| view.directed_to_uuid.get(idx).copied())
                .collect();
            let names: Vec<String> = uuids
                .iter()
                .filter_map(|id| view.nodes.iter().find(|n| n.id == *id).map(|n| n.name.clone()))
                .collect();

            if uuids.len() >= 2 {
                cycles.push(CircularDependency { nodes: uuids, names });
            }
        }
        Ok(cycles)
    }

    /// Calculate impact radius: nodes that transitively depend on `symbol_name`.
    pub fn calculate_impact_radius(backend: &MemoryBackend, symbol_name: &str) -> Result<ImpactResult> {
        let view = PetGraphView::from_backend(backend)?;
        let source_node = view
            .find_node_by_name(symbol_name)
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;
        let source = source_node.id;
        let source_idx = view
            .uuid_to_directed
            .get(&source)
            .copied()
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;

        let mut affected = HashSet::new();
        let mut queue = VecDeque::new();
        let mut _depths: HashMap<Uuid, usize> = HashMap::new();
        queue.push_back((source_idx, 0usize));
        let mut max_depth = 0usize;

        while let Some((idx, depth)) = queue.pop_front() {
            if depth > 10 {
                continue;
            }
            max_depth = max_depth.max(depth);

            for neighbor in view.directed.neighbors_directed(idx, petgraph::Direction::Incoming) {
                if let Some(uuid) = view.directed_to_uuid.get(&neighbor) {
                    if *uuid != source && affected.insert(*uuid) {
                        _depths.insert(*uuid, depth + 1);
                        queue.push_back((neighbor, depth + 1));
                    }
                }
            }
        }

        let affected_names: Vec<String> = affected
            .iter()
            .filter_map(|id| view.nodes.iter().find(|n| n.id == *id).map(|n| n.name.clone()))
            .collect();

        Ok(ImpactResult {
            source,
            source_name: symbol_name.to_string(),
            affected_nodes: affected.into_iter().collect(),
            affected_names,
            max_depth,
        })
    }

    /// Find callers of a symbol (direct).
    pub fn find_callers(backend: &MemoryBackend, symbol_name: &str) -> Result<Vec<String>> {
        let view = PetGraphView::from_backend(backend)?;
        let target = view
            .find_uuid_by_name(symbol_name)
            .ok_or_else(|| Error::NodeNotFound(symbol_name.to_string()))?;
        let target_idx = view.uuid_to_directed[&target];

        let callers: Vec<String> = view
            .directed
            .neighbors_directed(target_idx, petgraph::Direction::Incoming)
            .filter_map(|idx| view.directed_to_uuid.get(&idx))
            .filter_map(|uuid| view.nodes.iter().find(|n| n.id == *uuid).map(|n| n.name.clone()))
            .collect();
        Ok(callers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, Node, NodeType};

    #[test]
    fn test_circular_dependency_detection() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".to_string());
        let b = Node::new(NodeType::Function, "b".to_string());
        let id_a = a.id;
        let id_b = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_edge(Edge::new(id_a, id_b, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_b, id_a, EdgeType::Calls)).unwrap();

        let cycles = DependencyAnalyzer::find_circular_dependencies(&backend).unwrap();
        assert!(!cycles.is_empty());
        assert!(cycles[0].nodes.len() >= 2);
    }

    #[test]
    fn test_find_callers() {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".to_string());
        let target = Node::new(NodeType::Function, "target".to_string());
        let id_main = main.id;
        let id_target = target.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(target).unwrap();
        backend.insert_edge(Edge::new(id_main, id_target, EdgeType::Calls)).unwrap();

        let callers = DependencyAnalyzer::find_callers(&backend, "target").unwrap();
        assert!(callers.contains(&"main".to_string()));
    }
}
