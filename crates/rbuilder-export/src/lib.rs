//! Diagram and graph export (Phase 14).

pub mod graphml;
pub mod graphviz;
pub mod html;
pub mod mermaid;
pub mod render;

pub use graphml::export_graphml;
pub use graphviz::{generate_dot, parse_layout, GraphvizOptions, Layout, RankDir};
pub use html::export_html_dashboard;
pub use mermaid::{generate_mermaid, parse_diagram_type, DiagramType, MermaidOptions};
pub use render::{check_graphviz_installed, render_dot_to_file, ImageFormat};

use rbuilder_error::Result;
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use rbuilder_graph::schema::{Edge, EdgeType, Node};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

/// Nodes and edges selected for export.
#[derive(Debug, Clone, Default)]
pub struct Subgraph {
    /// Selected nodes
    pub nodes: Vec<Node>,
    /// Edges whose endpoints are both in `nodes`
    pub edges: Vec<Edge>,
}

/// Select nodes matching `query`, optionally expanding call neighbors up to `max_depth`.
pub fn select_subgraph(
    backend: &MemoryBackend,
    query: &str,
    max_depth: Option<usize>,
) -> Result<Subgraph> {
    let seeds = rbuilder_graph::query::execute(backend, query)?;
    if seeds.is_empty() {
        return Ok(Subgraph::default());
    }

    let all_edges = backend.all_edges()?;
    let mut included: HashSet<Uuid> = seeds.iter().map(|n| n.id).collect();

    if let Some(depth) = max_depth {
        let mut queue: VecDeque<(Uuid, usize)> = seeds.iter().map(|n| (n.id, 0)).collect();
        while let Some((id, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            for edge in &all_edges {
                if !matches!(
                    edge.edge_type,
                    EdgeType::Calls | EdgeType::Contains | EdgeType::Uses
                ) {
                    continue;
                }
                if edge.from == id && included.insert(edge.to) {
                    queue.push_back((edge.to, d + 1));
                }
                if edge.to == id && included.insert(edge.from) {
                    queue.push_back((edge.from, d + 1));
                }
            }
        }
    }

    let nodes: Vec<Node> = included
        .iter()
        .filter_map(|id| backend.get_node(*id).ok().flatten())
        .collect();

    let node_ids: HashSet<Uuid> = nodes.iter().map(|n| n.id).collect();
    let edges: Vec<Edge> = all_edges
        .into_iter()
        .filter(|e| node_ids.contains(&e.from) && node_ids.contains(&e.to))
        .collect();

    Ok(Subgraph { nodes, edges })
}

/// Escape text for Mermaid / DOT labels.
pub fn escape_label(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('[', "(")
        .replace(']', ")")
}

/// Stable short id for diagram nodes.
pub fn node_diagram_id(index: usize, name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .take(20)
        .collect();
    format!("n{index}_{safe}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::{EdgeType, NodeType};

    fn chain_backend() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".into());
        let b = Node::new(NodeType::Function, "b".into());
        let c = Node::new(NodeType::Function, "c".into());
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
        backend
    }

    #[test]
    fn test_select_subgraph_max_depth() {
        let backend = chain_backend();
        let sg = select_subgraph(&backend, "name:a", Some(1)).unwrap();
        assert_eq!(sg.nodes.len(), 2);
        assert!(sg.nodes.iter().any(|n| n.name == "b"));
        assert!(!sg.nodes.iter().any(|n| n.name == "c"));
    }

    #[test]
    fn test_escape_label_quotes() {
        assert!(escape_label(r#"say "hi""#).contains("\\\""));
    }
}
