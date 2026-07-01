//! In-memory graph backend
//!
//! Simple in-memory implementation of GraphBackend for testing and small repositories.

use crate::backend::trait_def::GraphBackend;
use crate::intern::StringInterner;
use crate::normalize_path_str;
use crate::schema::{Edge, EdgeType, Node, NodeType};
use rbuilder_error::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::trace;
use uuid::Uuid;
use petgraph::graph::{DiGraph, NodeIndex};

type PropertyIndex = HashMap<Arc<str>, HashMap<Arc<str>, Vec<Uuid>>>;

fn read_lock<T>(lock: &RwLock<T>) -> Result<std::sync::RwLockReadGuard<'_, T>> {
    lock.read()
        .map_err(|e| Error::GraphError(format!("Lock poisoned: {e}")))
}

fn write_lock<T>(lock: &RwLock<T>) -> Result<std::sync::RwLockWriteGuard<'_, T>> {
    lock.write()
        .map_err(|e| Error::GraphError(format!("Lock poisoned: {e}")))
}

fn expect_read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    read_lock(lock).expect("graph backend lock poisoned")
}

fn expect_write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    write_lock(lock).expect("graph backend lock poisoned")
}

/// In-memory graph backend with secondary indexes for fast queries.
#[derive(Debug, Clone)]
pub struct MemoryBackend {
    nodes: Arc<RwLock<HashMap<Uuid, Node>>>,
    edges: Arc<RwLock<Vec<Edge>>>,
    node_name_index: Arc<RwLock<HashMap<Arc<str>, Vec<Uuid>>>>,
    node_type_index: Arc<RwLock<HashMap<NodeType, Vec<Uuid>>>>,
    node_label_index: Arc<RwLock<HashMap<Arc<str>, Vec<Uuid>>>>,
    node_property_index: Arc<RwLock<PropertyIndex>>,
    edge_type_index: Arc<RwLock<HashMap<EdgeType, Vec<usize>>>>,
    string_interner: Arc<StringInterner>,  // CRITICAL: Must be Arc-wrapped to share pool across clones
    query_cache: Arc<RwLock<HashMap<String, Vec<Node>>>>,
}

impl MemoryBackend {
    /// Create a new in-memory backend
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            edges: Arc::new(RwLock::new(Vec::new())),
            node_name_index: Arc::new(RwLock::new(HashMap::new())),
            node_type_index: Arc::new(RwLock::new(HashMap::new())),
            node_label_index: Arc::new(RwLock::new(HashMap::new())),
            node_property_index: Arc::new(RwLock::new(HashMap::new())),
            edge_type_index: Arc::new(RwLock::new(HashMap::new())),
            string_interner: Arc::new(StringInterner::new()),  // Wrap in Arc
            query_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Result<Vec<Node>> {
        let nodes = read_lock(&self.nodes)?;
        Ok(nodes.values().cloned().collect())
    }

    /// Get all edges
    pub fn all_edges(&self) -> Result<Vec<Edge>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges.clone())
    }

    /// Get all node UUIDs (zero-copy except for the Vec of UUIDs)
    pub fn all_node_ids(&self) -> Result<Vec<Uuid>> {
        let nodes = read_lock(&self.nodes)?;
        Ok(nodes.keys().copied().collect())
    }

    /// Get edge topology as (from, to) pairs (zero-copy except for the Vec of tuples)
    pub fn edge_topology(&self) -> Result<Vec<(Uuid, Uuid)>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges.iter().map(|e| (e.from, e.to)).collect())
    }

    // ========== ZERO-CLONE API (use these for performance) ==========

    /// Zero-allocation node iteration. Passes read-only references to a closure.
    /// Use this instead of all_nodes() to avoid cloning 187K nodes.
    pub fn for_each_node<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(&Node),
    {
        let nodes = read_lock(&self.nodes)?;
        for node in nodes.values() {
            f(node);
        }
        Ok(())
    }

    /// Zero-allocation edge iteration.
    pub fn for_each_edge<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(&Edge),
    {
        let edges = read_lock(&self.edges)?;
        for edge in edges.iter() {
            f(edge);
        }
        Ok(())
    }

    /// Scoped read-only access to a single node. Returns result of closure.
    pub fn with_node<F, R>(&self, id: Uuid, f: F) -> Result<Option<R>>
    where
        F: FnOnce(&Node) -> R,
    {
        let nodes = read_lock(&self.nodes)?;
        Ok(nodes.get(&id).map(f))
    }

    /// Find node IDs by name (returns UUIDs, not cloned nodes).
    pub fn find_node_ids_by_name(&self, name: &str) -> Result<Vec<Uuid>> {
        let name_arc = self.string_interner.intern(name);
        let index = read_lock(&self.node_name_index)?;
        Ok(index.get(name_arc.as_ref()).cloned().unwrap_or_default())
    }

    /// Find node IDs by type (returns UUIDs, not cloned nodes).
    pub fn find_node_ids_by_type(&self, node_type: NodeType) -> Result<Vec<Uuid>> {
        let index = read_lock(&self.node_type_index)?;
        Ok(index.get(&node_type).cloned().unwrap_or_default())
    }

    /// Get outgoing edge target IDs (returns UUIDs, not cloned edges).
    pub fn get_outgoing_edge_targets(&self, node_id: Uuid) -> Result<Vec<Uuid>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges
            .iter()
            .filter(|e| e.from == node_id)
            .map(|e| e.to)
            .collect())
    }

    /// Get incoming edge source IDs (returns UUIDs, not cloned edges).
    pub fn get_incoming_edge_sources(&self, node_id: Uuid) -> Result<Vec<Uuid>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges
            .iter()
            .filter(|e| e.to == node_id)
            .map(|e| e.from)
            .collect())
    }

    // ========== LEGACY API (clones nodes - avoid in hot paths) ==========

    /// Find nodes by name (indexed)
    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        let name_arc = self.string_interner.intern(name);
        let index = read_lock(&self.node_name_index)?;
        if let Some(ids) = index.get(name_arc.as_ref()) {
            let nodes = read_lock(&self.nodes)?;
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by type (indexed)
    pub fn find_nodes_by_type(&self, node_type: NodeType) -> Result<Vec<Node>> {
        let index = read_lock(&self.node_type_index)?;
        if let Some(ids) = index.get(&node_type) {
            let nodes = read_lock(&self.nodes)?;
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by label (indexed)
    pub fn find_nodes_by_label(&self, label: &str) -> Result<Vec<Node>> {
        let label_arc = self.string_interner.intern(label);
        let index = read_lock(&self.node_label_index)?;
        if let Some(ids) = index.get(label_arc.as_ref()) {
            let nodes = read_lock(&self.nodes)?;
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by property key/value (indexed).
    pub fn find_nodes_by_property(&self, key: &str, value: &str) -> Result<Vec<Node>> {
        let key_arc = self.string_interner.intern(key);
        let value_arc = self.string_interner.intern(value);
        let index = read_lock(&self.node_property_index)?;
        if let Some(values) = index.get(key_arc.as_ref()) {
            if let Some(ids) = values.get(value_arc.as_ref()) {
                let nodes = read_lock(&self.nodes)?;
                return Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect());
            }
        }
        Ok(Vec::new())
    }

    /// Find nodes whose names end with the given suffix (uses name index).
    pub fn find_nodes_by_name_suffix(&self, suffix: &str) -> Result<Vec<Node>> {
        let index = read_lock(&self.node_name_index)?;
        let nodes = read_lock(&self.nodes)?;
        let mut results = Vec::new();
        for (name, ids) in index.iter() {
            if name.ends_with(suffix) {
                for id in ids {
                    if let Some(node) = nodes.get(id) {
                        results.push(node.clone());
                    }
                }
            }
        }
        Ok(results)
    }

    /// Insert multiple nodes with a single nodes-map lock.
    pub fn insert_nodes_batch(&mut self, nodes: Vec<Node>) -> Result<()> {
        if nodes.is_empty() {
            return Ok(());
        }

        let start = std::time::Instant::now();
        let node_count = nodes.len();

        trace!(node_count, "insert_nodes_batch starting");

        let intern_start = std::time::Instant::now();
        let mut prepared = Vec::with_capacity(nodes.len());
        for mut node in nodes {
            self.intern_node(&mut node);
            prepared.push(node);
        }
        trace!(elapsed = ?intern_start.elapsed(), "insert_nodes_batch: string interning complete");

        let index_start = std::time::Instant::now();
        self.index_nodes(&prepared);
        trace!(elapsed = ?index_start.elapsed(), "insert_nodes_batch: indexing complete");

        let store_start = std::time::Instant::now();
        let mut store = write_lock(&self.nodes)?;
        for node in prepared {
            store.insert(node.id, node);
        }
        drop(store);
        trace!(elapsed = ?store_start.elapsed(), "insert_nodes_batch: storing complete");

        self.invalidate_cache();
        trace!(elapsed = ?start.elapsed(), "insert_nodes_batch complete");
        Ok(())
    }

    /// Insert multiple edges with a single edges-map lock.
    pub fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()> {
        if edges.is_empty() {
            return Ok(());
        }

        let start = std::time::Instant::now();
        let edge_count = edges.len();
        trace!(edge_count, "insert_edges_batch starting");

        let mut store = write_lock(&self.edges)?;
        let mut type_index = write_lock(&self.edge_type_index)?;
        for edge in edges {
            let idx = store.len();
            type_index.entry(edge.edge_type).or_default().push(idx);
            store.push(edge);
        }
        drop(type_index);
        drop(store);
        self.invalidate_cache();

        trace!(elapsed = ?start.elapsed(), "insert_edges_batch complete");
        Ok(())
    }

    /// Batch update node properties with minimal lock contention.
    ///
    /// This method updates properties for multiple nodes in a single transaction,
    /// acquiring write locks only once. This is dramatically faster than calling
    /// `insert_node()` repeatedly when updating many nodes.
    ///
    /// # Performance
    /// - Single nodes lock (vs N locks for N nodes)
    /// - Single property index lock (vs N locks)
    /// - ~1000x faster for large batches (187K nodes: 5min → < 1s)
    ///
    /// # Arguments
    /// * `updates` - Map of node_id -> properties to add/update
    pub fn batch_update_node_properties(
        &mut self,
        updates: HashMap<Uuid, HashMap<String, String>>,
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let start = std::time::Instant::now();
        let update_count = updates.len();
        trace!(update_count, "batch_update_node_properties starting");

        // Step 1: Get nodes lock once
        let mut nodes = write_lock(&self.nodes)?;

        // Step 2: Collect old and new properties for property index update
        let mut old_properties: Vec<(Uuid, HashMap<String, String>)> = Vec::with_capacity(updates.len());
        let mut new_properties: Vec<(Uuid, HashMap<String, String>)> = Vec::with_capacity(updates.len());

        // Step 3: Update nodes and track property changes
        for (node_id, props_to_add) in updates {
            if let Some(node) = nodes.get_mut(&node_id) {
                // Save old properties for this node (only keys we're updating)
                let mut old_props = HashMap::new();
                for key in props_to_add.keys() {
                    if let Some(old_value) = node.properties.get(key) {
                        old_props.insert(key.clone(), old_value.clone());
                    }
                }
                if !old_props.is_empty() {
                    old_properties.push((node_id, old_props));
                }

                // Intern new property strings
                let mut interned_props = HashMap::new();
                for (key, value) in &props_to_add {
                    let key_interned = self.string_interner.intern(key);
                    let value_interned = self.string_interner.intern(value);
                    interned_props.insert(key_interned.to_string(), value_interned.to_string());
                }

                // Update node properties
                node.properties.extend(interned_props.clone());
                new_properties.push((node_id, interned_props));
            }
        }

        // Step 4: Release nodes lock before updating indexes
        drop(nodes);

        // Step 5: Update property index once
        let mut property_index = write_lock(&self.node_property_index)?;

        // Remove old property index entries
        for (node_id, old_props) in old_properties {
            for (key, value) in old_props {
                let key_arc = self.string_interner.intern(&key);
                let value_arc = self.string_interner.intern(&value);
                if let Some(values) = property_index.get_mut(key_arc.as_ref()) {
                    if let Some(ids) = values.get_mut(value_arc.as_ref()) {
                        ids.retain(|&id| id != node_id);
                    }
                }
            }
        }

        // Add new property index entries
        for (node_id, new_props) in new_properties {
            for (key, value) in new_props {
                let key_arc = self.string_interner.intern(&key);
                let value_arc = self.string_interner.intern(&value);
                property_index
                    .entry(key_arc)
                    .or_default()
                    .entry(value_arc)
                    .or_default()
                    .push(node_id);
            }
        }

        drop(property_index);

        self.invalidate_cache();

        trace!(elapsed = ?start.elapsed(), update_count, "batch_update_node_properties complete");
        Ok(())
    }

    /// Find edges by type (indexed)
    pub fn find_edges_by_type(&self, edge_type: EdgeType) -> Result<Vec<Edge>> {
        let index = read_lock(&self.edge_type_index)?;
        let edges = read_lock(&self.edges)?;
        if let Some(indices) = index.get(&edge_type) {
            Ok(indices
                .iter()
                .filter_map(|&i| edges.get(i).cloned())
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get outgoing edges from a node
    pub fn get_outgoing_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges
            .iter()
            .filter(|e| e.from == node_id)
            .cloned()
            .collect())
    }

    /// Get incoming edges to a node
    pub fn get_incoming_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = read_lock(&self.edges)?;
        Ok(edges.iter().filter(|e| e.to == node_id).cloned().collect())
    }

    /// Get neighbors of a node (nodes connected by edges)
    pub fn get_neighbors(&self, node_id: Uuid) -> Result<Vec<Node>> {
        let edges = read_lock(&self.edges)?;
        let mut neighbor_ids: Vec<Uuid> = edges
            .iter()
            .filter_map(|e| {
                if e.from == node_id {
                    Some(e.to)
                } else if e.to == node_id {
                    Some(e.from)
                } else {
                    None
                }
            })
            .collect();
        neighbor_ids.dedup();

        let nodes = read_lock(&self.nodes)?;
        Ok(neighbor_ids
            .iter()
            .filter_map(|id| nodes.get(id).cloned())
            .collect())
    }

    /// Count nodes
    pub fn node_count(&self) -> usize {
        expect_read(&self.nodes).len()
    }

    /// Count edges
    pub fn edge_count(&self) -> usize {
        expect_read(&self.edges).len()
    }

    /// Remove all nodes associated with a file path (relative or absolute).
    pub fn remove_nodes_for_file(&mut self, file_path: &str) -> Result<usize> {
        let normalized = normalize_path_str(file_path);
        let ids: Vec<Uuid> = read_lock(&self.nodes)?
            .values()
            .filter(|n| node_matches_file(n, &normalized))
            .map(|n| n.id)
            .collect();

        let count = ids.len();
        for id in &ids {
            self.delete_node_without_reindex(*id)?;
        }
        if count > 0 {
            self.rebuild_edge_index();
            self.invalidate_cache();
        }
        Ok(count)
    }

    fn delete_node_without_reindex(&mut self, id: Uuid) -> Result<()> {
        if let Some(node) = write_lock(&self.nodes)?.remove(&id) {
            self.unindex_node(&node);
            write_lock(&self.edges)?.retain(|e| e.from != id && e.to != id);
        }
        Ok(())
    }

    /// Build a symbol lookup index (`file::name` -> node ID).
    pub fn build_symbol_index(&self) -> HashMap<String, Uuid> {
        let mut index = HashMap::new();
        if let Ok(nodes) = self.all_nodes() {
            for node in nodes {
                if matches!(
                    node.node_type,
                    NodeType::Function
                        | NodeType::Class
                        | NodeType::Struct
                        | NodeType::Enum
                        | NodeType::Interface
                        | NodeType::Variable
                        | NodeType::TypeAlias
                ) {
                    if let Some(file) = &node.file_path {
                        let key = format!(
                            "{}::{}",
                            normalize_path_str(file),
                            node.qualified_name.as_deref().unwrap_or(&node.name)
                        );
                        index.insert(key, node.id);
                    }
                }
            }
        }
        index
    }

    /// Check whether an edge already exists.
    pub fn has_edge(&self, from: Uuid, to: Uuid, edge_type: EdgeType) -> bool {
        expect_read(&self.edges)
            .iter()
            .any(|e| e.from == from && e.to == to && e.edge_type == edge_type)
    }

    /// Remove edges referencing deleted nodes.
    pub fn prune_orphan_edges(&mut self) {
        let node_ids: HashSet<Uuid> = expect_read(&self.nodes).keys().copied().collect();
        {
            let mut edges = expect_write(&self.edges);
            edges.retain(|e| node_ids.contains(&e.from) && node_ids.contains(&e.to));
        }
        self.rebuild_edge_index();
        self.invalidate_cache();
    }

    /// Execute a cached query when possible.
    pub fn cached_query(&self, query: &str) -> Result<Vec<Node>> {
        let key = query.trim().to_ascii_lowercase();
        if let Some(cached) = read_lock(&self.query_cache)?.get(&key) {
            return Ok(cached.clone());
        }

        let results = crate::query::execute(self, query)?;
        write_lock(&self.query_cache)?.insert(key, results.clone());
        Ok(results)
    }

    /// Estimate memory usage in bytes (approximate).
    pub fn memory_estimate(&self) -> usize {
        let nodes = expect_read(&self.nodes);
        let edges = expect_read(&self.edges);
        let mut bytes = 0usize;
        for node in nodes.values() {
            bytes += node.name.len();
            bytes += node.qualified_name.as_ref().map(|s| s.len()).unwrap_or(0);
            bytes += node.file_path.as_ref().map(|s| s.len()).unwrap_or(0);
            bytes += node.labels.iter().map(|l| l.len()).sum::<usize>();
            bytes += node
                .properties
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>();
            bytes += std::mem::size_of::<Node>();
        }
        bytes += edges.len() * std::mem::size_of::<Edge>();
        bytes
    }

    /// Clear all data
    pub fn clear(&self) -> Result<()> {
        write_lock(&self.nodes)?.clear();
        write_lock(&self.edges)?.clear();
        write_lock(&self.node_name_index)?.clear();
        write_lock(&self.node_type_index)?.clear();
        write_lock(&self.node_label_index)?.clear();
        write_lock(&self.node_property_index)?.clear();
        write_lock(&self.edge_type_index)?.clear();
        write_lock(&self.query_cache)?.clear();
        Ok(())
    }

    fn intern_node(&self, node: &mut Node) {
        self.string_interner.intern_string(&mut node.name);
        if let Some(qn) = &mut node.qualified_name {
            self.string_interner.intern_string(qn);
        }
        if let Some(fp) = &mut node.file_path {
            self.string_interner.intern_string(fp);
        }
        for label in &mut node.labels {
            self.string_interner.intern_string(label);
        }
        let props: Vec<(String, String)> = node.properties.drain().collect();
        for (mut k, mut v) in props {
            self.string_interner.intern_string(&mut k);
            self.string_interner.intern_string(&mut v);
            node.properties.insert(k, v);
        }
    }

    fn index_node(&self, node: &Node) {
        self.index_nodes(std::slice::from_ref(node));
    }

    fn index_nodes(&self, nodes: &[Node]) {
        let start = std::time::Instant::now();

        let lock_start = std::time::Instant::now();
        let mut name_index = expect_write(&self.node_name_index);
        let mut type_index = expect_write(&self.node_type_index);
        let mut label_index = expect_write(&self.node_label_index);
        let mut property_index = expect_write(&self.node_property_index);
        trace!(elapsed = ?lock_start.elapsed(), "index_nodes: lock acquisition complete");

        let index_start = std::time::Instant::now();
        for node in nodes {
            // Intern strings once, share Arc across indexes (no clones!)
            let name_arc = self.string_interner.intern(&node.name);
            name_index
                .entry(name_arc)  // Arc::clone is cheap (just pointer + refcount)
                .or_default()
                .push(node.id);

            type_index.entry(node.node_type).or_default().push(node.id);

            for label in &node.labels {
                let label_arc = self.string_interner.intern(label);
                label_index.entry(label_arc).or_default().push(node.id);
            }

            for (key, value) in &node.properties {
                let key_arc = self.string_interner.intern(key);
                let value_arc = self.string_interner.intern(value);
                property_index
                    .entry(key_arc)
                    .or_default()
                    .entry(value_arc)
                    .or_default()
                    .push(node.id);
            }
        }
        trace!(elapsed = ?index_start.elapsed(), "index_nodes: index population complete");
        trace!(elapsed = ?start.elapsed(), "index_nodes complete");
    }

    fn unindex_node(&self, node: &Node) {
        let name_arc = self.string_interner.intern(&node.name);
        if let Some(ids) = expect_write(&self.node_name_index).get_mut(name_arc.as_ref()) {
            ids.retain(|&x| x != node.id);
        }
        if let Some(ids) = expect_write(&self.node_type_index).get_mut(&node.node_type) {
            ids.retain(|&x| x != node.id);
        }
        for label in &node.labels {
            let label_arc = self.string_interner.intern(label);
            if let Some(ids) = expect_write(&self.node_label_index).get_mut(label_arc.as_ref()) {
                ids.retain(|&x| x != node.id);
            }
        }
        for (key, value) in &node.properties {
            let key_arc = self.string_interner.intern(key);
            let value_arc = self.string_interner.intern(value);
            if let Some(values) = expect_write(&self.node_property_index).get_mut(key_arc.as_ref()) {
                if let Some(ids) = values.get_mut(value_arc.as_ref()) {
                    ids.retain(|&x| x != node.id);
                }
            }
        }
    }

    fn rebuild_edge_index(&self) {
        let edges = expect_read(&self.edges);
        let mut index: HashMap<EdgeType, Vec<usize>> = HashMap::new();
        for (i, edge) in edges.iter().enumerate() {
            index.entry(edge.edge_type).or_default().push(i);
        }
        *expect_write(&self.edge_type_index) = index;
    }

    fn invalidate_cache(&self) {
        expect_write(&self.query_cache).clear();
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphBackend for MemoryBackend {
    fn insert_node(&mut self, node: Node) -> Result<()> {
        let mut node = node;
        self.intern_node(&mut node);
        let id = node.id;
        self.index_node(&node);
        write_lock(&self.nodes)?.insert(id, node);
        self.invalidate_cache();
        Ok(())
    }

    fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        Ok(read_lock(&self.nodes)?.get(&id).cloned())
    }

    fn insert_edge(&mut self, edge: Edge) -> Result<()> {
        self.insert_edges_batch(vec![edge])
    }

    fn insert_nodes_batch(&mut self, nodes: Vec<Node>) -> Result<()> {
        MemoryBackend::insert_nodes_batch(self, nodes)
    }

    fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()> {
        MemoryBackend::insert_edges_batch(self, edges)
    }

    fn delete_node(&mut self, id: Uuid) -> Result<()> {
        if let Some(node) = write_lock(&self.nodes)?.remove(&id) {
            self.unindex_node(&node);
            write_lock(&self.edges)?.retain(|e| e.from != id && e.to != id);
            self.rebuild_edge_index();
            self.invalidate_cache();
        }
        Ok(())
    }

    fn find_nodes(&self, filter: &str) -> Result<Vec<Node>> {
        self.find_nodes_by_name(filter)
    }

    fn query(&self, query: &str) -> Result<Vec<Node>> {
        self.cached_query(query)
    }
}

impl MemoryBackend {
    /// Stream nodes in batches to avoid loading all nodes into memory at once.
    ///
    /// Returns an iterator that yields batches of nodes. This is memory-efficient
    /// for large graphs as it only holds one batch in memory at a time.
    pub fn stream_nodes(&self, batch_size: usize) -> Result<NodeBatchIterator> {
        let ids: Vec<Uuid> = {
            let nodes = read_lock(&self.nodes)?;
            nodes.keys().copied().collect()
        };

        Ok(NodeBatchIterator {
            nodes: Arc::clone(&self.nodes),
            ids,
            batch_size,
            position: 0,
        })
    }

    /// Stream edges in batches to avoid loading all edges into memory at once.
    pub fn stream_edges(&self, batch_size: usize) -> Result<EdgeBatchIterator> {
        let total_edges = {
            let edges = read_lock(&self.edges)?;
            edges.len()
        };

        Ok(EdgeBatchIterator {
            edges: Arc::clone(&self.edges),
            batch_size,
            position: 0,
            total: total_edges,
        })
    }

    /// Stream nodes of a specific type in batches.
    pub fn stream_nodes_by_type(&self, node_type: NodeType, batch_size: usize) -> Result<NodeBatchIterator> {
        let ids: Vec<Uuid> = {
            let index = read_lock(&self.node_type_index)?;
            index.get(&node_type).cloned().unwrap_or_default()
        };

        Ok(NodeBatchIterator {
            nodes: Arc::clone(&self.nodes),
            ids,
            batch_size,
            position: 0,
        })
    }
}

/// Petgraph integration for graph algorithms
impl MemoryBackend {
    /// Convert the graph to a Petgraph DiGraph for running graph algorithms.
    ///
    /// Returns the graph and a mapping from UUID to NodeIndex for result lookup.
    pub fn to_petgraph(&self) -> Result<(DiGraph<Uuid, EdgeType>, HashMap<Uuid, NodeIndex>)> {
        let mut graph = DiGraph::new();
        let mut id_map = HashMap::new();

        // Add all nodes to the graph
        {
            let nodes = read_lock(&self.nodes)?;
            for uuid in nodes.keys() {
                let idx = graph.add_node(*uuid);
                id_map.insert(*uuid, idx);
            }
        }

        // Add all edges
        {
            let edges = read_lock(&self.edges)?;
            for edge in edges.iter() {
                if let (Some(&from_idx), Some(&to_idx)) =
                    (id_map.get(&edge.from), id_map.get(&edge.to)) {
                    graph.add_edge(from_idx, to_idx, edge.edge_type);
                }
            }
        }

        Ok((graph, id_map))
    }

    /// Calculate PageRank scores using Petgraph's optimized algorithm.
    ///
    /// Much faster than custom implementation (< 1 second vs 17+ minutes on large graphs).
    pub fn calculate_pagerank(&self, damping: f64, iterations: usize) -> Result<HashMap<Uuid, f64>> {
        use petgraph::algo::page_rank;

        let (graph, id_map) = self.to_petgraph()?;

        // Run Petgraph's optimized PageRank
        let scores = page_rank(&graph, damping, iterations);

        // Reverse map: NodeIndex -> Uuid -> score
        let reverse_map: HashMap<NodeIndex, Uuid> =
            id_map.iter().map(|(uuid, idx)| (*idx, *uuid)).collect();

        Ok(scores.iter()
            .enumerate()
            .filter_map(|(idx_raw, &score)| {
                let idx = NodeIndex::new(idx_raw);
                reverse_map.get(&idx).map(|uuid| (*uuid, score))
            })
            .collect())
    }
}

/// Iterator that yields batches of nodes
pub struct NodeBatchIterator {
    nodes: Arc<RwLock<HashMap<Uuid, Node>>>,
    ids: Vec<Uuid>,
    batch_size: usize,
    position: usize,
}

impl Iterator for NodeBatchIterator {
    type Item = Result<Vec<Node>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.ids.len() {
            return None;
        }

        let end = (self.position + self.batch_size).min(self.ids.len());
        let batch_ids = &self.ids[self.position..end];
        self.position = end;

        let nodes_lock = match read_lock(&self.nodes) {
            Ok(lock) => lock,
            Err(e) => return Some(Err(e)),
        };

        let batch: Vec<Node> = batch_ids
            .iter()
            .filter_map(|id| nodes_lock.get(id).cloned())
            .collect();

        Some(Ok(batch))
    }
}

/// Iterator that yields batches of edges
pub struct EdgeBatchIterator {
    edges: Arc<RwLock<Vec<Edge>>>,
    batch_size: usize,
    position: usize,
    total: usize,
}

impl Iterator for EdgeBatchIterator {
    type Item = Result<Vec<Edge>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.total {
            return None;
        }

        let edges_lock = match read_lock(&self.edges) {
            Ok(lock) => lock,
            Err(e) => return Some(Err(e)),
        };

        let end = (self.position + self.batch_size).min(self.total);
        let batch = edges_lock[self.position..end].to_vec();
        self.position = end;

        Some(Ok(batch))
    }
}

fn node_matches_file(node: &Node, file_path: &str) -> bool {
    let target = normalize_path_str(file_path);
    let matches_path = |path: &str| {
        let norm = normalize_path_str(path);
        norm == target || norm.ends_with(&format!("/{target}"))
    };

    if let Some(fp) = &node.file_path {
        return matches_path(fp);
    }
    if node.node_type == NodeType::File {
        return matches_path(&node.name);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_backend() {
        let backend = MemoryBackend::new();
        assert_eq!(backend.node_count(), 0);
        assert_eq!(backend.edge_count(), 0);
    }

    #[test]
    fn test_insert_and_get_node() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "test".to_string());
        let id = node.id;

        backend.insert_node(node.clone()).unwrap();
        assert_eq!(backend.node_count(), 1);

        let retrieved = backend.get_node(id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test");
    }

    #[test]
    fn test_insert_edge() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "func1".to_string());
        let node2 = Node::new(NodeType::Function, "func2".to_string());
        let id1 = node1.id;
        let id2 = node2.id;

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();

        let edge = Edge::new(id1, id2, EdgeType::Calls);
        backend.insert_edge(edge).unwrap();

        assert_eq!(backend.edge_count(), 1);
        assert_eq!(
            backend.find_edges_by_type(EdgeType::Calls).unwrap().len(),
            1
        );
    }

    #[test]
    fn test_find_nodes_by_name() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "test".to_string());
        let node2 = Node::new(NodeType::Function, "test".to_string());
        let node3 = Node::new(NodeType::Function, "other".to_string());

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();
        backend.insert_node(node3).unwrap();

        let results = backend.find_nodes_by_name("test").unwrap();
        assert_eq!(results.len(), 2);

        let results = backend.find_nodes_by_name("other").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_find_nodes_by_type() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "func1".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Function, "func2".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Class, "Class1".to_string()))
            .unwrap();

        let functions = backend.find_nodes_by_type(NodeType::Function).unwrap();
        assert_eq!(functions.len(), 2);

        let classes = backend.find_nodes_by_type(NodeType::Class).unwrap();
        assert_eq!(classes.len(), 1);
    }

    #[test]
    fn test_find_nodes_by_label() {
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Class, "UserService".to_string());
        node.labels.push("soa:service".to_string());
        backend.insert_node(node).unwrap();

        let results = backend.find_nodes_by_label("soa:service").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_by_label_performance_index() {
        let mut backend = MemoryBackend::new();
        for i in 0..1000 {
            let mut node = Node::new(NodeType::Class, format!("Service{i}"));
            node.labels.push("react:component".to_string());
            backend.insert_node(node).unwrap();
        }
        for i in 0..1000 {
            backend
                .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
                .unwrap();
        }

        let start = std::time::Instant::now();
        let results = backend.find_nodes_by_label("react:component").unwrap();
        let duration = start.elapsed();
        assert_eq!(results.len(), 1000);
        assert!(duration < std::time::Duration::from_millis(50));
    }

    #[test]
    fn test_get_neighbors() {
        let mut backend = MemoryBackend::new();
        let node1 = Node::new(NodeType::Function, "func1".to_string());
        let node2 = Node::new(NodeType::Function, "func2".to_string());
        let node3 = Node::new(NodeType::Function, "func3".to_string());
        let id1 = node1.id;
        let id2 = node2.id;
        let id3 = node3.id;

        backend.insert_node(node1).unwrap();
        backend.insert_node(node2).unwrap();
        backend.insert_node(node3).unwrap();

        backend
            .insert_edge(Edge::new(id1, id2, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id1, id3, EdgeType::Calls))
            .unwrap();

        let neighbors = backend.get_neighbors(id1).unwrap();
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_delete_node() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "test".to_string());
        let id = node.id;

        backend.insert_node(node).unwrap();
        assert_eq!(backend.node_count(), 1);

        backend.delete_node(id).unwrap();
        assert_eq!(backend.node_count(), 0);
    }

    #[test]
    fn test_remove_nodes_for_file() {
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Function, "main".to_string());
        node.file_path = Some("src/main.rs".to_string());
        backend.insert_node(node).unwrap();
        backend
            .insert_node(
                Node::new(NodeType::File, "src/main.rs".to_string())
                    .with_file_path("src/main.rs".to_string()),
            )
            .unwrap();

        let removed = backend.remove_nodes_for_file("src/main.rs").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(backend.node_count(), 0);
    }

    #[test]
    fn test_clear() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "func1".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Function, "func2".to_string()))
            .unwrap();

        assert_eq!(backend.node_count(), 2);

        backend.clear().unwrap();
        assert_eq!(backend.node_count(), 0);
        assert_eq!(backend.edge_count(), 0);
    }

    #[test]
    fn test_insert_nodes_batch() {
        let mut backend = MemoryBackend::new();
        let nodes: Vec<_> = (0..100)
            .map(|i| Node::new(NodeType::Function, format!("fn{i}")))
            .collect();
        backend.insert_nodes_batch(nodes).unwrap();
        assert_eq!(backend.node_count(), 100);
    }

    #[test]
    fn test_insert_edges_batch() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "a".to_string());
        let n2 = Node::new(NodeType::Function, "b".to_string());
        let id1 = n1.id;
        let id2 = n2.id;
        backend.insert_nodes_batch(vec![n1, n2]).unwrap();

        backend
            .insert_edges_batch(vec![
                Edge::new(id1, id2, EdgeType::Calls),
                Edge::new(id2, id1, EdgeType::Calls),
            ])
            .unwrap();
        assert_eq!(backend.edge_count(), 2);
    }

    #[test]
    fn test_find_nodes_by_property() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(
                Node::new(NodeType::Function, "main".to_string())
                    .with_property("repo".into(), "api".into()),
            )
            .unwrap();
        backend
            .insert_node(
                Node::new(NodeType::Function, "other".to_string())
                    .with_property("repo".into(), "web".into()),
            )
            .unwrap();

        let api_nodes = backend.find_nodes_by_property("repo", "api").unwrap();
        assert_eq!(api_nodes.len(), 1);
        assert_eq!(api_nodes[0].name, "main");
    }
}
