//! Typed bidirectional CSR topology for cache-friendly graph algorithms.
//!
//! Each directed edge stores a dense `u32` target plus a 1-byte [`EdgeType`] code
//! (~5 bytes/edge per direction). Incoming adjacency is materialized so reverse
//! traversal (blast radius, community) stays O(degree) without scanning all edges.

use crate::backend::MemoryBackend;
use crate::schema::EdgeType;
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use uuid::Uuid;

/// Encode [`EdgeType`] as a stable `u8` (shared with columnar snapshots).
pub fn edge_type_to_u8(t: EdgeType) -> u8 {
    match t {
        EdgeType::Calls => 0,
        EdgeType::Contains => 1,
        EdgeType::Uses => 2,
        EdgeType::Implements => 3,
        EdgeType::Extends => 4,
        EdgeType::References => 5,
        EdgeType::Instantiates => 6,
        EdgeType::Modifies => 7,
        EdgeType::UsesConfig => 8,
        EdgeType::DefinedIn => 9,
        EdgeType::DependsOn => 10,
        EdgeType::IncludesRole => 11,
        EdgeType::DependsOnRole => 12,
        EdgeType::ExecutesTask => 13,
        EdgeType::NotifiesHandler => 14,
        EdgeType::IncludesPlaybook => 15,
        EdgeType::RendersTemplate => 16,
        EdgeType::DependsOnCookbook => 17,
        EdgeType::IncludesRecipe => 18,
        EdgeType::DeclaresResource => 19,
        EdgeType::UsesTemplate => 20,
        EdgeType::DefinesAttribute => 21,
        EdgeType::NotifiesResource => 22,
        EdgeType::DependsOnModule => 23,
        EdgeType::IncludesClass => 24,
        EdgeType::InheritsClass => 25,
        EdgeType::RequiresResource => 26,
        EdgeType::UsesFact => 27,
        EdgeType::AnnotatedWith => 28,
        EdgeType::Permits => 29,
        // 30 was EdgeType::Violates (Kantra); reserved retired slot — do not reuse.
        EdgeType::Overrides => 31,
        EdgeType::ExposesRoute => 32,
        EdgeType::Injects => 33,
        EdgeType::CoChangesWith => 47,
        EdgeType::Unknown => 255,
    }
}

/// Decode a stable `u8` edge-type code.
pub fn edge_type_from_u8(v: u8) -> Result<EdgeType> {
    Ok(match v {
        0 => EdgeType::Calls,
        1 => EdgeType::Contains,
        2 => EdgeType::Uses,
        3 => EdgeType::Implements,
        4 => EdgeType::Extends,
        5 => EdgeType::References,
        6 => EdgeType::Instantiates,
        7 => EdgeType::Modifies,
        8 => EdgeType::UsesConfig,
        9 => EdgeType::DefinedIn,
        10 => EdgeType::DependsOn,
        11 => EdgeType::IncludesRole,
        12 => EdgeType::DependsOnRole,
        13 => EdgeType::ExecutesTask,
        14 => EdgeType::NotifiesHandler,
        15 => EdgeType::IncludesPlaybook,
        16 => EdgeType::RendersTemplate,
        17 => EdgeType::DependsOnCookbook,
        18 => EdgeType::IncludesRecipe,
        19 => EdgeType::DeclaresResource,
        20 => EdgeType::UsesTemplate,
        21 => EdgeType::DefinesAttribute,
        22 => EdgeType::NotifiesResource,
        23 => EdgeType::DependsOnModule,
        24 => EdgeType::IncludesClass,
        25 => EdgeType::InheritsClass,
        26 => EdgeType::RequiresResource,
        27 => EdgeType::UsesFact,
        28 => EdgeType::AnnotatedWith,
        29 => EdgeType::Permits,
        // Retired Violates (Kantra) — reserved slot; Overrides stays 31.
        30 => {
            return Err(Error::SerdeError(format!(
                "retired edge type code {v} (former Violates)"
            )));
        }
        31 => EdgeType::Overrides,
        32 => EdgeType::ExposesRoute,
        33 => EdgeType::Injects,
        47 => EdgeType::CoChangesWith,
        255 => EdgeType::Unknown,
        _ => return Err(Error::SerdeError(format!("unknown edge type code {v}"))),
    })
}

/// Compressed sparse row adjacency with parallel edge-type codes (both directions).
#[derive(Debug, Clone)]
pub struct CodeGraphCsr {
    /// Outgoing row pointers — length `N + 1`.
    pub row_ptr: Vec<u32>,
    /// Outgoing target node indices.
    pub targets: Vec<u32>,
    /// Outgoing edge types parallel to [`Self::targets`].
    pub edge_types: Vec<u8>,
    /// Incoming row pointers — length `N + 1`.
    pub in_row_ptr: Vec<u32>,
    /// Incoming source node indices (edge destination → sources).
    pub in_targets: Vec<u32>,
    /// Incoming edge types parallel to [`Self::in_targets`].
    pub in_edge_types: Vec<u8>,
}

impl CodeGraphCsr {
    /// Number of nodes (`row_ptr.len() - 1`).
    pub fn node_count(&self) -> usize {
        self.row_ptr.len().saturating_sub(1)
    }

    /// Number of directed edges (outgoing).
    pub fn edge_count(&self) -> usize {
        self.targets.len()
    }

    /// Outgoing `(target, type_code)` slice for node `u`.
    pub fn out_neighbors(&self, u: u32) -> (&[u32], &[u8]) {
        let u = u as usize;
        let start = self.row_ptr[u] as usize;
        let end = self.row_ptr[u + 1] as usize;
        (&self.targets[start..end], &self.edge_types[start..end])
    }

    /// Incoming `(source, type_code)` slice for node `u`.
    pub fn in_neighbors(&self, u: u32) -> (&[u32], &[u8]) {
        let u = u as usize;
        let start = self.in_row_ptr[u] as usize;
        let end = self.in_row_ptr[u + 1] as usize;
        (
            &self.in_targets[start..end],
            &self.in_edge_types[start..end],
        )
    }

    /// Iterate all directed edges as `(src, dst, EdgeType)`.
    pub fn for_each_edge<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(u32, u32, EdgeType),
    {
        for src in 0..self.node_count() as u32 {
            let (targets, types) = self.out_neighbors(src);
            for (i, &dst) in targets.iter().enumerate() {
                f(src, dst, edge_type_from_u8(types[i])?);
            }
        }
        Ok(())
    }

    /// Build CSR from dense node order and typed UUID edges.
    ///
    /// `uuid_to_index` must cover every endpoint; unknown endpoints are skipped.
    pub fn from_typed_edges(
        node_count: usize,
        edges: &[(Uuid, Uuid, EdgeType)],
        uuid_to_index: &HashMap<Uuid, u32>,
    ) -> Self {
        let mut out_deg = vec![0u32; node_count];
        let mut in_deg = vec![0u32; node_count];
        let mut resolved: Vec<(u32, u32, u8)> = Vec::with_capacity(edges.len());

        for &(from, to, ty) in edges {
            let Some(&src) = uuid_to_index.get(&from) else {
                continue;
            };
            let Some(&dst) = uuid_to_index.get(&to) else {
                continue;
            };
            let code = edge_type_to_u8(ty);
            resolved.push((src, dst, code));
            out_deg[src as usize] += 1;
            in_deg[dst as usize] += 1;
        }

        let mut row_ptr = Vec::with_capacity(node_count + 1);
        let mut in_row_ptr = Vec::with_capacity(node_count + 1);
        row_ptr.push(0);
        in_row_ptr.push(0);
        for i in 0..node_count {
            row_ptr.push(row_ptr[i] + out_deg[i]);
            in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
        }

        let edge_count = resolved.len();
        let mut targets = vec![0u32; edge_count];
        let mut edge_types = vec![0u8; edge_count];
        let mut in_targets = vec![0u32; edge_count];
        let mut in_edge_types = vec![0u8; edge_count];
        let mut out_cursor = row_ptr[..node_count].to_vec();
        let mut in_cursor = in_row_ptr[..node_count].to_vec();

        for (src, dst, code) in resolved {
            let o = out_cursor[src as usize] as usize;
            targets[o] = dst;
            edge_types[o] = code;
            out_cursor[src as usize] += 1;

            let i = in_cursor[dst as usize] as usize;
            in_targets[i] = src;
            in_edge_types[i] = code;
            in_cursor[dst as usize] += 1;
        }

        Self {
            row_ptr,
            targets,
            edge_types,
            in_row_ptr,
            in_targets,
            in_edge_types,
        }
    }

    /// Build CSR by streaming edges from a mmap snapshot store (no full edge `Vec`).
    pub fn from_store_topology(
        store: &crate::snapshot::SnapshotNodeStore,
        uuid_to_index: &HashMap<Uuid, u32>,
    ) -> Result<Self> {
        let node_count = store.node_count();
        let mut out_deg = vec![0u32; node_count];
        let mut in_deg = vec![0u32; node_count];
        let mut resolved_count = 0usize;

        store.for_each_edge(|from, to, _ty| {
            if let (Some(&src), Some(&dst)) = (uuid_to_index.get(&from), uuid_to_index.get(&to)) {
                out_deg[src as usize] += 1;
                in_deg[dst as usize] += 1;
                resolved_count += 1;
            }
            Ok(())
        })?;

        let mut row_ptr = Vec::with_capacity(node_count + 1);
        let mut in_row_ptr = Vec::with_capacity(node_count + 1);
        row_ptr.push(0);
        in_row_ptr.push(0);
        for i in 0..node_count {
            row_ptr.push(row_ptr[i] + out_deg[i]);
            in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
        }

        let mut targets = vec![0u32; resolved_count];
        let mut edge_types = vec![0u8; resolved_count];
        let mut in_targets = vec![0u32; resolved_count];
        let mut in_edge_types = vec![0u8; resolved_count];
        let mut out_cursor = row_ptr[..node_count].to_vec();
        let mut in_cursor = in_row_ptr[..node_count].to_vec();

        store.for_each_edge(|from, to, ty| {
            let Some(&src) = uuid_to_index.get(&from) else {
                return Ok(());
            };
            let Some(&dst) = uuid_to_index.get(&to) else {
                return Ok(());
            };
            let code = edge_type_to_u8(ty);
            let o = out_cursor[src as usize] as usize;
            targets[o] = dst;
            edge_types[o] = code;
            out_cursor[src as usize] += 1;

            let i = in_cursor[dst as usize] as usize;
            in_targets[i] = src;
            in_edge_types[i] = code;
            in_cursor[dst as usize] += 1;
            Ok(())
        })?;

        Ok(Self {
            row_ptr,
            targets,
            edge_types,
            in_row_ptr,
            in_targets,
            in_edge_types,
        })
    }

    /// Build CSR from a live backend (dense index order = `all_node_ids` encounter order).
    pub fn from_backend(backend: &MemoryBackend) -> Result<(Self, Vec<Uuid>, HashMap<Uuid, u32>)> {
        let node_ids = backend.all_node_ids()?;
        let node_count = node_ids.len();
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        for (i, id) in node_ids.iter().enumerate() {
            uuid_to_index.insert(*id, i as u32);
        }
        let edges = backend.edge_topology_typed()?;
        let csr = Self::from_typed_edges(node_count, &edges, &uuid_to_index);
        Ok((csr, node_ids, uuid_to_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GraphBackend;
    use crate::schema::{Edge, Node, NodeType};

    #[test]
    fn csr_bidirectional_calls() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();

        let (csr, ids, map) = CodeGraphCsr::from_backend(&backend).unwrap();
        assert_eq!(csr.node_count(), 2);
        assert_eq!(csr.edge_count(), 1);
        let ai = map[&a_id];
        let bi = map[&b_id];
        let (out_t, out_ty) = csr.out_neighbors(ai);
        assert_eq!(out_t, &[bi]);
        assert_eq!(out_ty, &[edge_type_to_u8(EdgeType::Calls)]);
        let (in_t, _) = csr.in_neighbors(bi);
        assert_eq!(in_t, &[ai]);
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn csr_store_topology_matches_typed_edges() {
        use crate::snapshot::SnapshotNodeStore;
        use tempfile::tempdir;

        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();

        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        crate::write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let ids = store.all_node_ids();
        let mut uuid_to_index = HashMap::new();
        for (i, id) in ids.into_iter().enumerate() {
            uuid_to_index.insert(id, i as u32);
        }
        let edges = store.edge_topology_typed().unwrap();
        let legacy = CodeGraphCsr::from_typed_edges(store.node_count(), &edges, &uuid_to_index);
        let streamed = CodeGraphCsr::from_store_topology(&store, &uuid_to_index).unwrap();
        assert_eq!(legacy.node_count(), streamed.node_count());
        assert_eq!(legacy.edge_count(), streamed.edge_count());
        assert_eq!(legacy.targets, streamed.targets);
        assert_eq!(legacy.edge_types, streamed.edge_types);
    }
}
