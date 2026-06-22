//! Call graph construction from the code knowledge graph (Phase 13.1).

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{CallType, EdgeType, GraphParameter, NodeType};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// A function node in the call graph.
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    /// Graph node id.
    pub id: Uuid,
    /// Function name.
    pub name: String,
    /// Qualified name if available.
    pub qualified_name: Option<String>,
    /// Source file path.
    pub file_path: String,
    /// Start line.
    pub start_line: usize,
    /// Formal parameter names (from graph schema when available).
    pub parameters: Vec<String>,
}

/// A call edge between functions.
#[derive(Debug, Clone)]
pub struct CallGraphEdge {
    /// Caller function id.
    pub from: Uuid,
    /// Callee function id.
    pub to: Uuid,
    /// Call site line (0 if unknown).
    pub call_site: usize,
    /// Direct vs indirect call.
    pub call_type: CallType,
}

/// Whole-program call graph.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// Function nodes.
    pub nodes: HashMap<Uuid, CallGraphNode>,
    /// Call edges.
    pub edges: Vec<CallGraphEdge>,
}

impl CallGraph {
    /// Build from in-memory graph backend.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let mut cg = Self::default();
        for node in backend.all_nodes()? {
            if node.node_type == NodeType::Function {
                cg.nodes.insert(
                    node.id,
                    CallGraphNode {
                        id: node.id,
                        name: node.name.clone(),
                        qualified_name: node.qualified_name.clone(),
                        file_path: node.file_path.clone().unwrap_or_default(),
                        start_line: node.start_line.unwrap_or(0),
                        parameters: parameter_names(&node.parameters),
                    },
                );
            }
        }
        for edge in backend.all_edges()? {
            if edge.edge_type == EdgeType::Calls {
                cg.edges.push(CallGraphEdge {
                    from: edge.from,
                    to: edge.to,
                    call_site: edge
                        .properties
                        .get("call_site")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    call_type: edge.call_type.unwrap_or(CallType::Direct),
                });
            }
        }
        Ok(cg)
    }

    /// Callees of `function`.
    pub fn callees(&self, function: Uuid) -> Vec<Uuid> {
        self.edges
            .iter()
            .filter(|e| e.from == function)
            .map(|e| e.to)
            .collect()
    }

    /// Callers of `function`.
    pub fn callers(&self, function: Uuid) -> Vec<Uuid> {
        self.edges
            .iter()
            .filter(|e| e.to == function)
            .map(|e| e.from)
            .collect()
    }

    /// Topological order from entry-like nodes (in-degree 0) to leaves.
    pub fn topological_order(&self) -> Result<Vec<Uuid>> {
        let mut in_degree: HashMap<Uuid, usize> =
            self.nodes.keys().map(|id| (*id, 0usize)).collect();
        for edge in &self.edges {
            if let Some(deg) = in_degree.get_mut(&edge.to) {
                *deg += 1;
            }
        }
        let mut queue: VecDeque<Uuid> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node);
            for callee in self.callees(node) {
                if let Some(deg) = in_degree.get_mut(&callee) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(callee);
                    }
                }
            }
        }
        if result.len() != self.nodes.len() {
            return Err(Error::InvalidQuery(
                "call graph has cycles or disconnected components".into(),
            ));
        }
        Ok(result)
    }

    /// Functions participating in recursion (SCC size > 1 or self-loop).
    pub fn recursive_functions(&self) -> HashSet<Uuid> {
        let mut graph = DiGraph::<Uuid, ()>::new();
        let mut index_map: HashMap<Uuid, NodeIndex> = HashMap::new();
        for id in self.nodes.keys() {
            index_map.insert(*id, graph.add_node(*id));
        }
        for edge in &self.edges {
            if let (Some(&from), Some(&to)) = (index_map.get(&edge.from), index_map.get(&edge.to)) {
                graph.add_edge(from, to, ());
            }
        }
        let mut recursive = HashSet::new();
        for scc in tarjan_scc(&graph) {
            if scc.len() > 1 {
                for idx in scc {
                    recursive.insert(graph[idx]);
                }
            } else if let Some(&idx) = scc.first() {
                let id = graph[idx];
                if self.edges.iter().any(|e| e.from == id && e.to == id) {
                    recursive.insert(id);
                }
            }
        }
        recursive
    }

    /// Parameter names for a function node.
    pub fn parameter_names(&self, function: Uuid) -> &[String] {
        self.nodes
            .get(&function)
            .map(|n| n.parameters.as_slice())
            .unwrap_or(&[])
    }
}

fn parameter_names(params: &[GraphParameter]) -> Vec<String> {
    params.iter().map(|p| p.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node};

    fn sample_call_graph() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        let main = Node::new(NodeType::Function, "main".into());
        let helper = Node::new(NodeType::Function, "helper".into());
        let id_main = main.id;
        let id_helper = helper.id;
        backend.insert_node(main).unwrap();
        backend.insert_node(helper).unwrap();
        backend
            .insert_edge(Edge::new(id_main, id_helper, EdgeType::Calls))
            .unwrap();
        backend
    }

    #[test]
    fn test_call_graph_from_backend() {
        let backend = sample_call_graph();
        let cg = CallGraph::from_backend(&backend).unwrap();
        assert_eq!(cg.nodes.len(), 2);
        assert_eq!(cg.edges.len(), 1);
    }

    #[test]
    fn test_callers_and_callees() {
        let backend = sample_call_graph();
        let cg = CallGraph::from_backend(&backend).unwrap();
        let main_id = cg.nodes.values().find(|n| n.name == "main").unwrap().id;
        let helper_id = cg.nodes.values().find(|n| n.name == "helper").unwrap().id;
        assert_eq!(cg.callees(main_id), vec![helper_id]);
        assert_eq!(cg.callers(helper_id), vec![main_id]);
    }
}
