//! In-memory graph backend
//!
//! Simple in-memory implementation of GraphBackend for testing and small repositories.

use crate::error::Result;
use crate::graph::backend::trait_def::GraphBackend;
use crate::graph::intern::StringInterner;
use crate::graph::schema::{Edge, EdgeType, Node, NodeType};
use crate::incremental::file_tracker::normalize_path_str;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

type PropertyIndex = HashMap<String, HashMap<String, Vec<Uuid>>>;

/// In-memory graph backend with secondary indexes for fast queries.
#[derive(Debug, Clone)]
pub struct MemoryBackend {
    nodes: Arc<RwLock<HashMap<Uuid, Node>>>,
    edges: Arc<RwLock<Vec<Edge>>>,
    node_name_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
    node_type_index: Arc<RwLock<HashMap<NodeType, Vec<Uuid>>>>,
    node_label_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
    node_property_index: Arc<RwLock<PropertyIndex>>,
    edge_type_index: Arc<RwLock<HashMap<EdgeType, Vec<usize>>>>,
    string_interner: StringInterner,
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
            string_interner: StringInterner::new(),
            query_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Result<Vec<Node>> {
        let nodes = self.nodes.read().unwrap();
        Ok(nodes.values().cloned().collect())
    }

    /// Get all edges
    pub fn all_edges(&self) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges.clone())
    }

    /// Find nodes by name (indexed)
    pub fn find_nodes_by_name(&self, name: &str) -> Result<Vec<Node>> {
        let index = self.node_name_index.read().unwrap();
        if let Some(ids) = index.get(name) {
            let nodes = self.nodes.read().unwrap();
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by type (indexed)
    pub fn find_nodes_by_type(&self, node_type: NodeType) -> Result<Vec<Node>> {
        let index = self.node_type_index.read().unwrap();
        if let Some(ids) = index.get(&node_type) {
            let nodes = self.nodes.read().unwrap();
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by label (indexed)
    pub fn find_nodes_by_label(&self, label: &str) -> Result<Vec<Node>> {
        let index = self.node_label_index.read().unwrap();
        if let Some(ids) = index.get(label) {
            let nodes = self.nodes.read().unwrap();
            Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Find nodes by property key/value (indexed).
    pub fn find_nodes_by_property(&self, key: &str, value: &str) -> Result<Vec<Node>> {
        let index = self.node_property_index.read().unwrap();
        if let Some(values) = index.get(key) {
            if let Some(ids) = values.get(value) {
                let nodes = self.nodes.read().unwrap();
                return Ok(ids.iter().filter_map(|id| nodes.get(id).cloned()).collect());
            }
        }
        Ok(Vec::new())
    }

    /// Find nodes whose names end with the given suffix (uses name index).
    pub fn find_nodes_by_name_suffix(&self, suffix: &str) -> Result<Vec<Node>> {
        let index = self.node_name_index.read().unwrap();
        let nodes = self.nodes.read().unwrap();
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

        let mut prepared = Vec::with_capacity(nodes.len());
        for mut node in nodes {
            self.intern_node(&mut node);
            prepared.push(node);
        }

        self.index_nodes(&prepared);

        let mut store = self.nodes.write().unwrap();
        for node in prepared {
            store.insert(node.id, node);
        }
        drop(store);
        self.invalidate_cache();
        Ok(())
    }

    /// Insert multiple edges with a single edges-map lock.
    pub fn insert_edges_batch(&mut self, edges: Vec<Edge>) -> Result<()> {
        if edges.is_empty() {
            return Ok(());
        }

        let mut store = self.edges.write().unwrap();
        let mut type_index = self.edge_type_index.write().unwrap();
        for edge in edges {
            let idx = store.len();
            type_index.entry(edge.edge_type).or_default().push(idx);
            store.push(edge);
        }
        drop(type_index);
        drop(store);
        self.invalidate_cache();
        Ok(())
    }

    /// Find edges by type (indexed)
    pub fn find_edges_by_type(&self, edge_type: EdgeType) -> Result<Vec<Edge>> {
        let index = self.edge_type_index.read().unwrap();
        let edges = self.edges.read().unwrap();
        if let Some(indices) = index.get(&edge_type) {
            Ok(indices.iter().filter_map(|&i| edges.get(i).cloned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get outgoing edges from a node
    pub fn get_outgoing_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges
            .iter()
            .filter(|e| e.from == node_id)
            .cloned()
            .collect())
    }

    /// Get incoming edges to a node
    pub fn get_incoming_edges(&self, node_id: Uuid) -> Result<Vec<Edge>> {
        let edges = self.edges.read().unwrap();
        Ok(edges
            .iter()
            .filter(|e| e.to == node_id)
            .cloned()
            .collect())
    }

    /// Get neighbors of a node (nodes connected by edges)
    pub fn get_neighbors(&self, node_id: Uuid) -> Result<Vec<Node>> {
        let edges = self.edges.read().unwrap();
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

        let nodes = self.nodes.read().unwrap();
        Ok(neighbor_ids
            .iter()
            .filter_map(|id| nodes.get(id).cloned())
            .collect())
    }

    /// Count nodes
    pub fn node_count(&self) -> usize {
        self.nodes.read().unwrap().len()
    }

    /// Count edges
    pub fn edge_count(&self) -> usize {
        self.edges.read().unwrap().len()
    }

    /// Remove all nodes associated with a file path (relative or absolute).
    pub fn remove_nodes_for_file(&mut self, file_path: &str) -> Result<usize> {
        let normalized = normalize_path_str(file_path);
        let ids: Vec<Uuid> = self
            .nodes
            .read()
            .unwrap()
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
        if let Some(node) = self.nodes.write().unwrap().remove(&id) {
            self.unindex_node(&node);
            self.edges.write().unwrap().retain(|e| e.from != id && e.to != id);
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
        self.edges
            .read()
            .unwrap()
            .iter()
            .any(|e| e.from == from && e.to == to && e.edge_type == edge_type)
    }

    /// Remove edges referencing deleted nodes.
    pub fn prune_orphan_edges(&mut self) {
        let node_ids: HashSet<Uuid> = self.nodes.read().unwrap().keys().copied().collect();
        {
            let mut edges = self.edges.write().unwrap();
            edges.retain(|e| node_ids.contains(&e.from) && node_ids.contains(&e.to));
        }
        self.rebuild_edge_index();
        self.invalidate_cache();
    }

    /// Execute a cached query when possible.
    pub fn cached_query(&self, query: &str) -> Result<Vec<Node>> {
        let key = query.trim().to_ascii_lowercase();
        if let Some(cached) = self.query_cache.read().unwrap().get(&key) {
            return Ok(cached.clone());
        }

        let results = crate::graph::query::execute(self, query)?;
        self.query_cache
            .write()
            .unwrap()
            .insert(key, results.clone());
        Ok(results)
    }

    /// Estimate memory usage in bytes (approximate).
    pub fn memory_estimate(&self) -> usize {
        let nodes = self.nodes.read().unwrap();
        let edges = self.edges.read().unwrap();
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
        self.nodes.write().unwrap().clear();
        self.edges.write().unwrap().clear();
        self.node_name_index.write().unwrap().clear();
        self.node_type_index.write().unwrap().clear();
        self.node_label_index.write().unwrap().clear();
        self.node_property_index.write().unwrap().clear();
        self.edge_type_index.write().unwrap().clear();
        self.query_cache.write().unwrap().clear();
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
        let mut name_index = self.node_name_index.write().unwrap();
        let mut type_index = self.node_type_index.write().unwrap();
        let mut label_index = self.node_label_index.write().unwrap();
        let mut property_index = self.node_property_index.write().unwrap();

        for node in nodes {
            name_index
                .entry(node.name.clone())
                .or_default()
                .push(node.id);
            type_index
                .entry(node.node_type)
                .or_default()
                .push(node.id);
            for label in &node.labels {
                label_index
                    .entry(label.clone())
                    .or_default()
                    .push(node.id);
            }
            for (key, value) in &node.properties {
                property_index
                    .entry(key.clone())
                    .or_default()
                    .entry(value.clone())
                    .or_default()
                    .push(node.id);
            }
        }
    }

    fn unindex_node(&self, node: &Node) {
        if let Some(ids) = self.node_name_index.write().unwrap().get_mut(&node.name) {
            ids.retain(|&x| x != node.id);
        }
        if let Some(ids) = self
            .node_type_index
            .write()
            .unwrap()
            .get_mut(&node.node_type)
        {
            ids.retain(|&x| x != node.id);
        }
        for label in &node.labels {
            if let Some(ids) = self.node_label_index.write().unwrap().get_mut(label) {
                ids.retain(|&x| x != node.id);
            }
        }
        for (key, value) in &node.properties {
            if let Some(values) = self.node_property_index.write().unwrap().get_mut(key) {
                if let Some(ids) = values.get_mut(value) {
                    ids.retain(|&x| x != node.id);
                }
            }
        }
    }

    fn rebuild_edge_index(&self) {
        let edges = self.edges.read().unwrap();
        let mut index: HashMap<EdgeType, Vec<usize>> = HashMap::new();
        for (i, edge) in edges.iter().enumerate() {
            index.entry(edge.edge_type).or_default().push(i);
        }
        *self.edge_type_index.write().unwrap() = index;
    }

    fn invalidate_cache(&self) {
        self.query_cache.write().unwrap().clear();
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
        self.nodes.write().unwrap().insert(id, node);
        self.invalidate_cache();
        Ok(())
    }

    fn get_node(&self, id: Uuid) -> Result<Option<Node>> {
        Ok(self.nodes.read().unwrap().get(&id).cloned())
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
        if let Some(node) = self.nodes.write().unwrap().remove(&id) {
            self.unindex_node(&node);
            self.edges.write().unwrap().retain(|e| e.from != id && e.to != id);
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
        assert_eq!(backend.find_edges_by_type(EdgeType::Calls).unwrap().len(), 1);
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
        backend.insert_node(Node::new(NodeType::Function, "func1".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Function, "func2".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Class, "Class1".to_string())).unwrap();

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

        backend.insert_edge(Edge::new(id1, id2, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id1, id3, EdgeType::Calls)).unwrap();

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
            .insert_node(Node::new(NodeType::File, "src/main.rs".to_string()).with_file_path("src/main.rs".to_string()))
            .unwrap();

        let removed = backend.remove_nodes_for_file("src/main.rs").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(backend.node_count(), 0);
    }

    #[test]
    fn test_clear() {
        let mut backend = MemoryBackend::new();
        backend.insert_node(Node::new(NodeType::Function, "func1".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Function, "func2".to_string())).unwrap();

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
