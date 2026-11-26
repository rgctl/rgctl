//! Topology utilities for analysis engines.
//!
//! [`PetGraphView`] is a compatibility façade over [`StructuralTopology`] (typed CSR).
//! Prefer [`StructuralTopology`] for new code; discover builds one topology and shares it.

use crate::structural_topology::StructuralTopology;
use petgraph::graph::NodeIndex;
use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::EdgeType;
use rgctl_graph::snapshot::{PreparedGraphSnapshot, SnapshotNodeStore};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

/// Default max traversal depth for impact/blast-radius BFS analyses.
pub const DEFAULT_TRAVERSAL_DEPTH: usize = 10;

/// Configuration for graph traversal depth limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraversalConfig {
    /// Maximum hops to traverse.
    pub max_depth: usize,
}

impl Default for TraversalConfig {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_TRAVERSAL_DEPTH,
        }
    }
}

impl TraversalConfig {
    /// Create a config with an explicit depth limit.
    pub fn new(max_depth: usize) -> Self {
        Self { max_depth }
    }

    /// Traverse without a depth cap.
    pub fn unlimited() -> Self {
        Self {
            max_depth: usize::MAX,
        }
    }
}

/// Build a set for O(1) edge-type membership checks in hot paths.
pub fn edge_type_set(allowed_types: &[EdgeType]) -> HashSet<EdgeType> {
    allowed_types.iter().copied().collect()
}

/// Typed graph topology view (CSR-backed) with UUID mapping.
///
/// Memory layout (RSS-conscious):
/// - wraps [`StructuralTopology`] directly with zero duplicate index maps or `Vec<Uuid>` clones.
pub struct PetGraphView {
    /// Underlying CSR topology.
    pub topo: StructuralTopology,
}

impl PetGraphView {
    fn from_topo(topo: StructuralTopology) -> Self {
        Self { topo }
    }

    /// Build from a memory backend.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        Ok(Self::from_topo(StructuralTopology::from_backend(backend)?))
    }

    /// Build from a prepared mmap snapshot.
    pub fn from_prepared(prepared: &PreparedGraphSnapshot) -> Result<Self> {
        Ok(Self::from_topo(StructuralTopology::from_prepared(
            prepared,
        )?))
    }

    /// Build from a mmap snapshot store.
    pub fn from_snapshot_store(store: &SnapshotNodeStore) -> Result<Self> {
        Ok(Self::from_topo(StructuralTopology::from_snapshot_store(
            store,
        )?))
    }

    /// Node count.
    pub fn node_count(&self) -> usize {
        self.topo.node_count()
    }

    /// Directed edge count.
    pub fn edge_count(&self) -> usize {
        self.topo.edge_count()
    }

    /// Incoming neighbors reachable via one of `allowed` edge types.
    pub fn incoming_filtered<'a>(
        &'a self,
        idx: NodeIndex,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = NodeIndex> + 'a {
        self.topo
            .in_filtered(idx.index() as u32, allowed)
            .map(|i| NodeIndex::new(i as usize))
    }

    /// Outgoing neighbors reachable via one of `allowed` edge types.
    pub fn outgoing_filtered<'a>(
        &'a self,
        idx: NodeIndex,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = NodeIndex> + 'a {
        self.topo
            .out_filtered(idx.index() as u32, allowed)
            .map(|i| NodeIndex::new(i as usize))
    }

    /// Whether a directed edge of the given type exists between two nodes.
    pub fn has_edge_type(&self, from: NodeIndex, to: NodeIndex, edge_type: EdgeType) -> bool {
        self.topo
            .has_edge_type(from.index() as u32, to.index() as u32, edge_type)
    }

    /// Visit every directed edge as `(src, dst, EdgeType)` dense indices.
    pub fn for_each_edge<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(NodeIndex, NodeIndex, EdgeType),
    {
        self.topo.for_each_edge(|src, dst, ty| {
            f(
                NodeIndex::new(src as usize),
                NodeIndex::new(dst as usize),
                ty,
            );
        })
    }

    /// Get petgraph NodeIndex for a UUID.
    #[inline]
    pub fn get_index(&self, uuid: Uuid) -> Option<NodeIndex> {
        self.topo
            .get_index(uuid)
            .map(|i| NodeIndex::new(i as usize))
    }

    /// Get UUID for a petgraph NodeIndex.
    #[inline]
    pub fn get_uuid(&self, index: NodeIndex) -> Option<Uuid> {
        self.topo.index_to_uuid.get(index.index()).copied()
    }

    /// Dense `index -> Uuid` slice without cloning.
    #[inline]
    pub fn index_to_uuid_slice(&self) -> &[Uuid] {
        &self.topo.index_to_uuid
    }

    /// Iterate `(NodeIndex, Uuid)` in dense index order.
    pub fn index_uuid_iter(&self) -> impl Iterator<Item = (NodeIndex, Uuid)> + '_ {
        self.topo
            .index_to_uuid
            .iter()
            .enumerate()
            .map(|(i, uuid)| (NodeIndex::new(i), *uuid))
    }
}

const CALL_EDGES: &[EdgeType] = &[EdgeType::Calls];

/// Upstream function callers within `max_depth` call hops of `target_id` (hop 1 = direct callers).
pub fn caller_ids_within_depth(
    view: &PetGraphView,
    target_id: Uuid,
    max_depth: usize,
) -> HashSet<Uuid> {
    if max_depth == 0 {
        return HashSet::new();
    }
    let Some(target_idx) = view.get_index(target_id) else {
        return HashSet::new();
    };

    let mut allowed = HashSet::new();
    let mut queue: VecDeque<(Uuid, usize)> = view
        .incoming_filtered(target_idx, CALL_EDGES)
        .filter_map(|idx| view.get_uuid(idx).map(|id| (id, 1usize)))
        .collect();

    while let Some((caller_id, depth)) = queue.pop_front() {
        if depth > max_depth || caller_id == target_id {
            continue;
        }
        if !allowed.insert(caller_id) {
            continue;
        }
        let Some(caller_idx) = view.get_index(caller_id) else {
            continue;
        };
        for pred in view.incoming_filtered(caller_idx, CALL_EDGES) {
            if let Some(uuid) = view.get_uuid(pred)
                && uuid != target_id
            {
                queue.push_back((uuid, depth + 1));
            }
        }
    }
    allowed
}

/// Restrict an impact zone to nodes reachable within `max_depth` incoming call hops.
pub fn filter_impact_by_caller_depth(
    view: &PetGraphView,
    target_id: Uuid,
    impact_zone_ids: &[Uuid],
    max_depth: usize,
) -> Vec<Uuid> {
    if max_depth == usize::MAX {
        return impact_zone_ids.to_vec();
    }
    let allowed = caller_ids_within_depth(view, target_id, max_depth);
    impact_zone_ids
        .iter()
        .copied()
        .filter(|id| allowed.contains(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, Node, NodeType};

    #[test]
    fn test_build_petgraph_view() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::Function, "helper".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend
            .insert_edge(Edge::new(id1, id2, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        assert_eq!(view.node_count(), 2);
        assert_eq!(view.edge_count(), 1);
        let idx1 = view.get_index(id1).unwrap();
        let idx2 = view.get_index(id2).unwrap();
        assert!(view.has_edge_type(idx1, idx2, EdgeType::Calls));
        assert_eq!(view.index_to_uuid_slice().len(), 2);
    }

    #[test]
    fn test_build_petgraph_view_from_prepared() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "main".to_string());
        let n2 = Node::new(NodeType::Function, "helper".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2).unwrap();
        backend
            .insert_edge(Edge::new(id1, id2, EdgeType::Calls))
            .unwrap();
        let prepared = rgctl_graph::PreparedGraphSnapshot::from_backend(&backend).unwrap();
        let view = PetGraphView::from_prepared(&prepared).unwrap();
        assert_eq!(view.node_count(), 2);
        assert_eq!(view.edge_count(), 1);
    }

    #[test]
    fn traversal_config_default_depth() {
        assert_eq!(
            TraversalConfig::default().max_depth,
            DEFAULT_TRAVERSAL_DEPTH
        );
    }

    #[test]
    fn caller_depth_limits_impact_zone_on_chain() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let c = Node::new(NodeType::Function, "c");
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        // c <- b <- a  (a calls b calls c); callers of c: b at depth 1, a at depth 2
        backend
            .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
            .unwrap();

        let view = PetGraphView::from_backend(&backend).unwrap();
        let zone = [id_a, id_b, id_c];
        let filtered = filter_impact_by_caller_depth(&view, id_c, &zone, 1);
        assert!(filtered.contains(&id_b));
        assert!(!filtered.contains(&id_a));
    }
}
