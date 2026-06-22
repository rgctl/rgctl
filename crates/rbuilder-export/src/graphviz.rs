//! Graphviz DOT export (Phase 14.2).

use crate::{escape_label, node_diagram_id, select_subgraph};
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use std::collections::HashMap;

/// Graphviz layout engine name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Hierarchical layout
    Dot,
    /// Spring model
    Neato,
    /// Force-directed
    Fdp,
    /// Circular
    Circo,
}

impl Layout {
    /// CLI / attribute value for this layout.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dot => "dot",
            Self::Neato => "neato",
            Self::Fdp => "fdp",
            Self::Circo => "circo",
        }
    }
}

/// Graph rank direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankDir {
    /// Left to right
    Lr,
    /// Top to bottom
    Tb,
}

/// DOT generation options.
#[derive(Debug, Clone)]
pub struct GraphvizOptions {
    /// Layout engine (used when rendering)
    pub layout: Layout,
    /// Rank direction
    pub rankdir: RankDir,
}

impl Default for GraphvizOptions {
    fn default() -> Self {
        Self {
            layout: Layout::Dot,
            rankdir: RankDir::Tb,
        }
    }
}

/// Generate a DOT digraph for nodes matching `query`.
pub fn generate_dot(
    backend: &MemoryBackend,
    query: &str,
    options: GraphvizOptions,
    max_depth: Option<usize>,
) -> Result<String> {
    let subgraph = select_subgraph(backend, query, max_depth)?;
    if subgraph.nodes.is_empty() {
        return Err(Error::InvalidQuery(format!(
            "No nodes matched query: {query}"
        )));
    }

    let rankdir = match options.rankdir {
        RankDir::Lr => "LR",
        RankDir::Tb => "TB",
    };

    let mut out = format!(
        "digraph CodeGraph {{\n  graph [rankdir={rankdir}];\n  node [fontname=\"Helvetica\"];\n"
    );

    let id_map: HashMap<_, _> = subgraph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, node_diagram_id(i, &n.name)))
        .collect();

    for node in &subgraph.nodes {
        let id = id_map.get(&node.id).unwrap();
        let (shape, color) = node_style(node.node_type);
        out.push_str(&format!(
            "  \"{id}\" [label=\"{}\" shape={shape} color={color}];\n",
            escape_label(&node.name)
        ));
    }

    for edge in &subgraph.edges {
        let (Some(from), Some(to)) = (id_map.get(&edge.from), id_map.get(&edge.to)) else {
            continue;
        };
        let (style, color, label) = edge_style(edge.edge_type);
        let label_attr = label.map(|l| format!(" label=\"{l}\"")).unwrap_or_default();
        out.push_str(&format!(
            "  \"{from}\" -> \"{to}\" [style={style} color={color}{label_attr}];\n"
        ));
    }

    out.push_str("}\n");
    Ok(out)
}

fn node_style(node_type: NodeType) -> (&'static str, &'static str) {
    match node_type {
        NodeType::Function => ("box", "blue"),
        NodeType::Class | NodeType::Struct => ("ellipse", "green"),
        NodeType::Module => ("folder", "orange"),
        NodeType::Interface => ("diamond", "purple"),
        NodeType::File => ("note", "gray"),
        _ => ("box", "black"),
    }
}

fn edge_style(edge_type: EdgeType) -> (&'static str, &'static str, Option<&'static str>) {
    match edge_type {
        EdgeType::Calls => ("solid", "black", None),
        EdgeType::Extends => ("dashed", "red", Some("extends")),
        EdgeType::Implements => ("dashed", "blue", Some("implements")),
        EdgeType::Uses => ("dotted", "gray", None),
        EdgeType::Contains => ("solid", "brown", Some("contains")),
        _ => ("solid", "black", None),
    }
}

/// Parse Graphviz layout engine from string.
pub fn parse_layout(value: &str) -> Layout {
    match value.to_ascii_lowercase().as_str() {
        "neato" => Layout::Neato,
        "fdp" => Layout::Fdp,
        "circo" => Layout::Circo,
        _ => Layout::Dot,
    }
}
