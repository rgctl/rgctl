//! Structural topology: typed CSR + dense UUID index maps for analysis engines.
//!
//! This is the hot graph residency for community / centrality / blast / dependency.
//! Node payloads live elsewhere (MemoryBackend or ColdMetadataDb).

use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::csr::{CodeGraphCsr, edge_type_from_u8, edge_type_to_u8};
use rgctl_graph::schema::EdgeType;
use rgctl_graph::snapshot::{PreparedGraphSnapshot, SnapshotNodeStore};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Hot structural view: CSR adjacency + UUID↔dense index maps.
#[derive(Debug, Clone)]
pub struct StructuralTopology {
    /// Typed bidirectional CSR.
    pub csr: CodeGraphCsr,
    /// Dense index → UUID.
    pub index_to_uuid: Vec<Uuid>,
    /// UUID → dense index.
    pub uuid_to_index: HashMap<Uuid, u32>,
}

impl StructuralTopology {
    /// Build from a live backend (same dense order as historical PetGraphView).
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let (csr, index_to_uuid, uuid_to_index) = CodeGraphCsr::from_backend(backend)?;
        Ok(Self {
            csr,
            index_to_uuid,
            uuid_to_index,
        })
    }

    /// Build from a prepared snapshot's topology.
    pub fn from_prepared(prepared: &PreparedGraphSnapshot) -> Result<Self> {
        let node_count = prepared.nodes.len();
        let mut index_to_uuid = Vec::with_capacity(node_count);
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        for (i, node) in prepared.nodes.iter().enumerate() {
            uuid_to_index.insert(node.id, i as u32);
            index_to_uuid.push(node.id);
        }
        let edges: Vec<_> = prepared
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.edge_type))
            .collect();
        let csr = CodeGraphCsr::from_typed_edges(node_count, &edges, &uuid_to_index);
        Ok(Self {
            csr,
            index_to_uuid,
            uuid_to_index,
        })
    }

    /// Build from a mmap snapshot store.
    pub fn from_snapshot_store(store: &SnapshotNodeStore) -> Result<Self> {
        let ids = store.all_node_ids();
        let node_count = ids.len();
        let mut index_to_uuid = Vec::with_capacity(node_count);
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        for (i, id) in ids.into_iter().enumerate() {
            uuid_to_index.insert(id, i as u32);
            index_to_uuid.push(id);
        }
        let csr = CodeGraphCsr::from_store_topology(store, &uuid_to_index)?;
        Ok(Self {
            csr,
            index_to_uuid,
            uuid_to_index,
        })
    }

    /// Node count.
    pub fn node_count(&self) -> usize {
        self.csr.node_count()
    }

    /// Directed edge count.
    pub fn edge_count(&self) -> usize {
        self.csr.edge_count()
    }

    /// Dense index for a UUID.
    pub fn get_index(&self, uuid: Uuid) -> Option<u32> {
        self.uuid_to_index.get(&uuid).copied()
    }

    /// UUID for a dense index.
    pub fn get_uuid(&self, index: u32) -> Option<Uuid> {
        self.index_to_uuid.get(index as usize).copied()
    }

    /// Iterate `(dense_index, Uuid)`.
    pub fn index_uuid_iter(&self) -> impl Iterator<Item = (u32, Uuid)> + '_ {
        self.index_to_uuid
            .iter()
            .enumerate()
            .map(|(i, uuid)| (i as u32, *uuid))
    }

    /// Outgoing neighbors filtered by allowed edge types.
    ///
    /// Uses a stack-allocated `[bool; 256]` edge-type mask instead of a heap
    /// `HashSet`, so per-node filtering is allocation-free.
    pub fn out_filtered<'a>(
        &'a self,
        u: u32,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = u32> + 'a {
        let mut mask = [false; 256];
        for &ty in allowed {
            mask[edge_type_to_u8(ty) as usize] = true;
        }
        let (targets, types) = self.csr.out_neighbors(u);
        targets
            .iter()
            .zip(types.iter())
            .filter(move |&(_, &ty)| mask[ty as usize])
            .map(|(&t, _)| t)
    }

    /// Incoming neighbors filtered by allowed edge types.
    ///
    /// Uses a stack-allocated `[bool; 256]` edge-type mask instead of a heap
    /// `HashSet`, so per-node filtering is allocation-free.
    pub fn in_filtered<'a>(
        &'a self,
        u: u32,
        allowed: &'a [EdgeType],
    ) -> impl Iterator<Item = u32> + 'a {
        let mut mask = [false; 256];
        for &ty in allowed {
            mask[edge_type_to_u8(ty) as usize] = true;
        }
        let (targets, types) = self.csr.in_neighbors(u);
        targets
            .iter()
            .zip(types.iter())
            .filter(move |&(_, &ty)| mask[ty as usize])
            .map(|(&t, _)| t)
    }

    /// Whether a typed directed edge exists.
    pub fn has_edge_type(&self, from: u32, to: u32, edge_type: EdgeType) -> bool {
        let code = edge_type_to_u8(edge_type);
        let (targets, types) = self.csr.out_neighbors(from);
        targets
            .iter()
            .zip(types.iter())
            .any(|(&t, &ty)| t == to && ty == code)
    }

    /// Undirected neighbor list (in ∪ out) filtered by types — for community detection.
    pub fn undirected_filtered_neighbors(&self, allowed: &[EdgeType]) -> Vec<Vec<usize>> {
        let n = self.node_count();
        let allowed_codes: HashSet<u8> = allowed.iter().copied().map(edge_type_to_u8).collect();
        let mut neighbors = vec![Vec::new(); n];
        for u in 0..n as u32 {
            let (out_t, out_ty) = self.csr.out_neighbors(u);
            for (i, &v) in out_t.iter().enumerate() {
                if allowed_codes.contains(&out_ty[i]) {
                    neighbors[u as usize].push(v as usize);
                    neighbors[v as usize].push(u as usize);
                }
            }
        }
        for list in &mut neighbors {
            list.sort_unstable();
            list.dedup();
        }
        neighbors
    }

    /// Kosaraju SCC restricted to edges whose type is in `allowed`.
    ///
    /// Returns components as vectors of dense node indices. Component order is
    /// finishing-time order (same convention as petgraph's `kosaraju_scc`).
    pub fn kosaraju_scc_filtered(&self, allowed: &[EdgeType]) -> Vec<Vec<u32>> {
        let allowed_codes: HashSet<u8> = allowed.iter().copied().map(edge_type_to_u8).collect();
        let n = self.node_count();
        if n == 0 {
            return Vec::new();
        }

        let mut visited = vec![false; n];
        let mut order = Vec::with_capacity(n);

        // Pass 1: DFS finishing times on reverse graph (incoming = reverse of outgoing).
        for start in 0..n as u32 {
            if visited[start as usize] {
                continue;
            }
            let mut stack = vec![(start, false)];
            while let Some((u, expanded)) = stack.pop() {
                if expanded {
                    order.push(u);
                    continue;
                }
                if visited[u as usize] {
                    continue;
                }
                visited[u as usize] = true;
                stack.push((u, true));
                let (srcs, types) = self.csr.in_neighbors(u);
                for (i, &v) in srcs.iter().enumerate() {
                    if allowed_codes.contains(&types[i]) && !visited[v as usize] {
                        stack.push((v, false));
                    }
                }
            }
        }

        // Pass 2: DFS on forward graph in reverse finishing order.
        visited.fill(false);
        let mut sccs = Vec::new();
        for &start in order.iter().rev() {
            if visited[start as usize] {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![start];
            visited[start as usize] = true;
            while let Some(u) = stack.pop() {
                component.push(u);
                let (dsts, types) = self.csr.out_neighbors(u);
                for (i, &v) in dsts.iter().enumerate() {
                    if allowed_codes.contains(&types[i]) && !visited[v as usize] {
                        visited[v as usize] = true;
                        stack.push(v);
                    }
                }
            }
            sccs.push(component);
        }
        sccs
    }

    /// Kosaraju over all edge types.
    pub fn kosaraju_scc_all(&self) -> Vec<Vec<u32>> {
        // Allow every code that appears — easier: empty filter means all.
        self.kosaraju_scc_all_impl()
    }

    fn kosaraju_scc_all_impl(&self) -> Vec<Vec<u32>> {
        let n = self.node_count();
        if n == 0 {
            return Vec::new();
        }
        let mut visited = vec![false; n];
        let mut order = Vec::with_capacity(n);

        for start in 0..n as u32 {
            if visited[start as usize] {
                continue;
            }
            let mut stack = vec![(start, false)];
            while let Some((u, expanded)) = stack.pop() {
                if expanded {
                    order.push(u);
                    continue;
                }
                if visited[u as usize] {
                    continue;
                }
                visited[u as usize] = true;
                stack.push((u, true));
                let (srcs, _) = self.csr.in_neighbors(u);
                for &v in srcs {
                    if !visited[v as usize] {
                        stack.push((v, false));
                    }
                }
            }
        }

        visited.fill(false);
        let mut sccs = Vec::new();
        for &start in order.iter().rev() {
            if visited[start as usize] {
                continue;
            }
            let mut component = Vec::new();
            let mut stack = vec![start];
            visited[start as usize] = true;
            while let Some(u) = stack.pop() {
                component.push(u);
                let (dsts, _) = self.csr.out_neighbors(u);
                for &v in dsts {
                    if !visited[v as usize] {
                        visited[v as usize] = true;
                        stack.push(v);
                    }
                }
            }
            sccs.push(component);
        }
        sccs
    }

    /// Iterate outgoing edges as `(src, dst, EdgeType)`.
    pub fn for_each_edge<F>(&self, f: F) -> Result<()>
    where
        F: FnMut(u32, u32, EdgeType),
    {
        self.csr.for_each_edge(f)
    }
}

/// Validate edge type decode round-trip used by CSR filters.
#[allow(dead_code)]
fn _assert_edge_codec(ty: EdgeType) -> Result<EdgeType> {
    edge_type_from_u8(edge_type_to_u8(ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Edge, Node, NodeType};

    #[test]
    fn topology_from_backend_and_scc() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let c = Node::new(NodeType::Function, "c");
        let a_id = a.id;
        let b_id = b.id;
        let c_id = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        // a -> b -> c -> a cycle
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(b_id, c_id, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(c_id, a_id, EdgeType::Calls))
            .unwrap();

        let topo = StructuralTopology::from_backend(&backend).unwrap();
        let sccs = topo.kosaraju_scc_filtered(&[EdgeType::Calls]);
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 3);
    }
}
