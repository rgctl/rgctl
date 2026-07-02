//! Call graph construction from the code knowledge graph (Phase 13.1).
//!
//! ## Ultra-Lean Design
//!
//! Uses contiguous adjacency lists with u32 indices for cache-friendly traversal.
//! Column-oriented metadata (names, parameters, call-site lines) avoids cloning nodes.

use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use rbuilder_graph::schema::{CallType, EdgeType, GraphParameter, NodeType};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// A function node in the call graph (kept for backward compatibility).
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

/// A call edge between functions (kept for backward compatibility).
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
    /// Argument variable names at the call site (when known).
    pub argument_names: Vec<String>,
}

/// Ultra-lean, contiguous Call Graph built for speed.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// Maps our fast internal u32 index back to the global Uuid
    pub index_to_id: Vec<Uuid>,
    /// Maps the global Uuid to our fast internal u32 index
    pub id_to_index: HashMap<Uuid, u32>,

    /// Outgoing edges: index -> list of target u32s
    pub success_list: Vec<Vec<u32>>,
    /// Incoming edges: index -> list of source u32s
    pub precursor_list: Vec<Vec<u32>>,

    /// Column-oriented metadata parallel to `index_to_id`
    pub names: Vec<String>,
    /// Source file path for each node (empty when unknown).
    pub file_paths: Vec<String>,
    /// Definition line number for each node.
    pub line_numbers: Vec<usize>,
    /// Formal parameter names for each function node.
    pub parameters: Vec<Vec<String>>,

    /// Call-site metadata for each outgoing edge instance
    call_edge_meta: Vec<Vec<CallEdgeMeta>>,

    /// Backward compatibility: lazily populated on first access
    nodes_cache: Option<HashMap<Uuid, CallGraphNode>>,
    edges_cache: Option<Vec<CallGraphEdge>>,
}

/// Metadata for one call edge instance (caller idx -> callee idx).
#[derive(Debug, Clone)]
struct CallEdgeMeta {
    call_site: usize,
    call_type: CallType,
    argument_names: Vec<String>,
}

impl Default for CallEdgeMeta {
    fn default() -> Self {
        Self {
            call_site: 0,
            call_type: CallType::Direct,
            argument_names: Vec::new(),
        }
    }
}

impl CallGraph {
    /// Build from in-memory graph backend using zero-clone construction.
    pub fn from_backend(backend: &MemoryBackend) -> Result<Self> {
        let function_ids = backend.find_node_ids_by_type(NodeType::Function)?;
        let node_count = function_ids.len();

        let mut id_to_index = HashMap::with_capacity(node_count);
        let mut index_to_id = Vec::with_capacity(node_count);
        let mut success_list = vec![Vec::new(); node_count];
        let mut precursor_list = vec![Vec::new(); node_count];
        let mut call_edge_meta = vec![Vec::new(); node_count];
        let mut names = Vec::with_capacity(node_count);
        let mut file_paths = Vec::with_capacity(node_count);
        let mut line_numbers = vec![0; node_count];
        let mut parameters = Vec::with_capacity(node_count);

        for (index, &func_id) in function_ids.iter().enumerate() {
            id_to_index.insert(func_id, index as u32);
            index_to_id.push(func_id);

            if let Ok(Some(node)) = backend.get_node(func_id) {
                names.push(node.name.clone());
                file_paths.push(node.file_path.clone().unwrap_or_default());
                line_numbers[index] = node.start_line.unwrap_or(0);
                parameters.push(parameter_names_from_node(&node.parameters));
            } else {
                names.push(String::new());
                file_paths.push(String::new());
                parameters.push(Vec::new());
            }
        }

        backend.for_each_edge(|edge| {
            if edge.edge_type != EdgeType::Calls {
                return;
            }
            let (Some(&from_idx), Some(&to_idx)) =
                (id_to_index.get(&edge.from), id_to_index.get(&edge.to))
            else {
                return;
            };
            let from = from_idx as usize;
            let to = to_idx as usize;
            success_list[from].push(to_idx);
            precursor_list[to].push(from_idx);
            let call_site = edge
                .properties
                .get("call_site_line")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let argument_names = parse_argument_names(&edge.properties);
            call_edge_meta[from].push(CallEdgeMeta {
                call_site,
                call_type: edge.call_type.unwrap_or(CallType::Direct),
                argument_names,
            });
        })?;

        Ok(Self {
            index_to_id,
            id_to_index,
            success_list,
            precursor_list,
            names,
            file_paths,
            line_numbers,
            parameters,
            call_edge_meta,
            nodes_cache: None,
            edges_cache: None,
        })
    }

    /// Number of functions in the call graph.
    pub fn function_count(&self) -> usize {
        self.index_to_id.len()
    }

    /// Number of call edges.
    pub fn call_edge_count(&self) -> usize {
        self.success_list.iter().map(|v| v.len()).sum()
    }

    /// Resolve a function id by name (first match).
    pub fn id_by_name(&self, name: &str) -> Option<Uuid> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|idx| self.index_to_id[idx])
    }

    /// Function name for an id.
    pub fn name(&self, function: Uuid) -> Option<&str> {
        self.id_to_index
            .get(&function)
            .and_then(|idx| self.names.get(*idx as usize))
            .map(String::as_str)
    }

    /// O(1) lookup: Get callees of a function by UUID.
    pub fn callees(&self, function: Uuid) -> Vec<Uuid> {
        if let Some(&idx) = self.id_to_index.get(&function) {
            self.success_list[idx as usize]
                .iter()
                .map(|&callee_idx| self.index_to_id[callee_idx as usize])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// O(1) lookup: Get callers of a function by UUID.
    pub fn callers(&self, function: Uuid) -> Vec<Uuid> {
        if let Some(&idx) = self.id_to_index.get(&function) {
            self.precursor_list[idx as usize]
                .iter()
                .map(|&caller_idx| self.index_to_id[caller_idx as usize])
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Formal parameter names for a function.
    pub fn parameter_names(&self, function: Uuid) -> &[String] {
        static EMPTY: &[String] = &[];
        self.id_to_index
            .get(&function)
            .and_then(|idx| self.parameters.get(*idx as usize))
            .map(|p| p.as_slice())
            .unwrap_or(EMPTY)
    }

    /// Parameter name at index for a callee function.
    pub fn parameter_at(&self, function: Uuid, index: usize) -> Option<&str> {
        self.parameter_names(function)
            .get(index)
            .map(String::as_str)
    }

    /// Call edges from `caller` to `callee` with site metadata.
    pub fn call_edges_between(&self, caller: Uuid, callee: Uuid) -> Vec<CallGraphEdge> {
        let Some(&from_idx) = self.id_to_index.get(&caller) else {
            return Vec::new();
        };
        let from = from_idx as usize;
        let mut edges = Vec::new();
        for (pos, &to_idx) in self.success_list[from].iter().enumerate() {
            let to_id = self.index_to_id[to_idx as usize];
            if to_id != callee {
                continue;
            }
            let meta = self.call_edge_meta[from]
                .get(pos)
                .cloned()
                .unwrap_or_default();
            edges.push(CallGraphEdge {
                from: caller,
                to: callee,
                call_site: meta.call_site,
                call_type: meta.call_type,
                argument_names: meta.argument_names,
            });
        }
        edges
    }

    /// Argument name passed at call site for callee parameter index.
    pub fn argument_at_call(
        &self,
        caller: Uuid,
        callee: Uuid,
        param_index: usize,
    ) -> Option<String> {
        self.call_edges_between(caller, callee)
            .into_iter()
            .find_map(|edge| edge.argument_names.get(param_index).cloned())
    }

    /// Get all function IDs.
    pub fn function_ids(&self) -> &[Uuid] {
        &self.index_to_id
    }

    /// Backward compatibility: Access nodes (builds cache on first call).
    pub fn nodes(&mut self) -> &HashMap<Uuid, CallGraphNode> {
        if self.nodes_cache.is_none() {
            let mut nodes = HashMap::new();
            for (idx, &id) in self.index_to_id.iter().enumerate() {
                nodes.insert(
                    id,
                    CallGraphNode {
                        id,
                        name: self.names.get(idx).cloned().unwrap_or_default(),
                        qualified_name: None,
                        file_path: self.file_paths.get(idx).cloned().unwrap_or_default(),
                        start_line: self.line_numbers[idx],
                        parameters: self.parameters.get(idx).cloned().unwrap_or_default(),
                    },
                );
            }
            self.nodes_cache = Some(nodes);
        }
        self.nodes_cache.as_ref().unwrap()
    }

    /// Backward compatibility: Access edges (builds cache on first call).
    pub fn edges(&mut self) -> &Vec<CallGraphEdge> {
        if self.edges_cache.is_none() {
            let mut edges = Vec::new();
            for (from_idx, callees) in self.success_list.iter().enumerate() {
                let from_id = self.index_to_id[from_idx];
                for (pos, &to_idx) in callees.iter().enumerate() {
                    let to_id = self.index_to_id[to_idx as usize];
                    let meta = self
                        .call_edge_meta
                        .get(from_idx)
                        .and_then(|m| m.get(pos))
                        .cloned()
                        .unwrap_or_default();
                    edges.push(CallGraphEdge {
                        from: from_id,
                        to: to_id,
                        call_site: meta.call_site,
                        call_type: meta.call_type,
                        argument_names: meta.argument_names,
                    });
                }
            }
            self.edges_cache = Some(edges);
        }
        self.edges_cache.as_ref().unwrap()
    }

    /// Topological order from entry-like nodes (in-degree 0) to leaves.
    pub fn topological_order(&self) -> Result<Vec<Uuid>> {
        let node_count = self.index_to_id.len();
        let mut in_degree: HashMap<Uuid, usize> =
            self.index_to_id.iter().map(|id| (*id, 0usize)).collect();

        for callees in &self.success_list {
            for &callee_idx in callees {
                let callee_id = self.index_to_id[callee_idx as usize];
                if let Some(deg) = in_degree.get_mut(&callee_id) {
                    *deg += 1;
                }
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
        if result.len() != node_count {
            return Err(Error::InvalidQuery(
                "call graph has cycles or disconnected components".into(),
            ));
        }
        Ok(result)
    }

    /// Functions participating in recursion (SCC size > 1 or self-loop).
    pub fn recursive_functions(&self) -> HashSet<Uuid> {
        use petgraph::algo::tarjan_scc;
        use petgraph::graph::{DiGraph, NodeIndex};

        let mut graph = DiGraph::<Uuid, ()>::new();
        let mut index_map: HashMap<Uuid, NodeIndex> = HashMap::new();

        for id in &self.index_to_id {
            index_map.insert(*id, graph.add_node(*id));
        }

        for (from_idx, callees) in self.success_list.iter().enumerate() {
            let from_id = self.index_to_id[from_idx];
            for &to_idx in callees {
                let to_id = self.index_to_id[to_idx as usize];
                if let (Some(&from), Some(&to)) = (index_map.get(&from_id), index_map.get(&to_id)) {
                    graph.add_edge(from, to, ());
                }
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
                if let Some(&self_idx) = self.id_to_index.get(&id) {
                    if self.success_list[self_idx as usize].contains(&self_idx) {
                        recursive.insert(id);
                    }
                }
            }
        }
        recursive
    }
}

fn parameter_names_from_node(params: &[GraphParameter]) -> Vec<String> {
    params.iter().map(|p| p.name.clone()).collect()
}

fn parse_argument_names(properties: &HashMap<String, String>) -> Vec<String> {
    let mut args: Vec<(usize, String)> = properties
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("arg_")
                .and_then(|idx| idx.parse::<usize>().ok())
                .map(|idx| (idx, v.clone()))
        })
        .collect();
    args.sort_by_key(|(idx, _)| *idx);
    args.into_iter().map(|(_, name)| name).collect()
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
        assert_eq!(cg.function_count(), 2);
        assert_eq!(cg.call_edge_count(), 1);
    }

    #[test]
    fn test_callers_and_callees() {
        let backend = sample_call_graph();
        let cg = CallGraph::from_backend(&backend).unwrap();
        let main_id = cg.id_by_name("main").unwrap();
        let helper_id = cg.id_by_name("helper").unwrap();
        assert_eq!(cg.callees(main_id), vec![helper_id]);
        assert_eq!(cg.callers(helper_id), vec![main_id]);
    }
}
