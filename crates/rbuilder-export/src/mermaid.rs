//! Mermaid diagram export (Phase 14.1).

use crate::{escape_label, node_diagram_id, select_subgraph, Subgraph};
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};

/// Mermaid diagram variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramType {
    /// General flowchart (default)
    Flowchart,
    /// UML-style class diagram
    ClassDiagram,
    /// Function call graph
    CallGraph,
}

/// Mermaid generation options.
#[derive(Debug, Clone)]
pub struct MermaidOptions {
    /// Diagram style
    pub diagram_type: DiagramType,
    /// BFS depth when selecting nodes
    pub max_depth: Option<usize>,
    /// Top-to-bottom (`true`) or left-to-right (`false`)
    pub vertical: bool,
}

impl Default for MermaidOptions {
    fn default() -> Self {
        Self {
            diagram_type: DiagramType::Flowchart,
            max_depth: None,
            vertical: true,
        }
    }
}

/// Generate a Mermaid diagram for nodes matching `query`.
pub fn generate_mermaid(
    backend: &MemoryBackend,
    query: &str,
    options: MermaidOptions,
) -> Result<String> {
    let subgraph = select_subgraph(backend, query, options.max_depth)?;
    if subgraph.nodes.is_empty() {
        return Err(Error::InvalidQuery(format!(
            "No nodes matched query: {query}"
        )));
    }

    match options.diagram_type {
        DiagramType::Flowchart => render_flowchart(&subgraph, options.vertical),
        DiagramType::ClassDiagram => render_class_diagram(&subgraph),
        DiagramType::CallGraph => render_call_graph(&subgraph, options.vertical),
    }
}

fn render_flowchart(subgraph: &Subgraph, vertical: bool) -> Result<String> {
    let dir = if vertical { "TD" } else { "LR" };
    let mut out = format!("graph {dir}\n");
    let id_map = build_id_map(&subgraph.nodes);

    for (idx, node) in subgraph.nodes.iter().enumerate() {
        let id = id_map.get(&node.id).unwrap();
        let label = escape_label(&node.name);
        let shape = match node.node_type {
            NodeType::Class | NodeType::Struct => format!("{id}[\"{label}\"]"),
            NodeType::Function => format!("{id}(\"{label}\")"),
            _ => format!("{id}[\"{label}\"]"),
        };
        out.push_str(&format!("    {shape}\n"));
        let _ = idx;
    }

    for edge in &subgraph.edges {
        if let (Some(from), Some(to)) = (id_map.get(&edge.from), id_map.get(&edge.to)) {
            out.push_str(&format!("    {from} --> {to}\n"));
        }
    }
    Ok(out)
}

fn render_class_diagram(subgraph: &Subgraph) -> Result<String> {
    let mut out = String::from("classDiagram\n");
    let class_types = [
        NodeType::Class,
        NodeType::Struct,
        NodeType::Interface,
        NodeType::Enum,
    ];

    for node in &subgraph.nodes {
        if !class_types.contains(&node.node_type) {
            continue;
        }
        let name = sanitize_class_name(&node.name);
        out.push_str(&format!("    class {name} {{\n"));
        if let Some(sig) = &node.signature {
            for line in sig.lines().take(8) {
                let t = line.trim();
                if !t.is_empty() {
                    out.push_str(&format!("        {t}\n"));
                }
            }
        } else if let Some(ret) = &node.return_type {
            out.push_str(&format!("        +{ret}\n"));
        }
        out.push_str("    }\n");
    }

    let id_map = build_id_map(&subgraph.nodes);
    for edge in &subgraph.edges {
        let (Some(from), Some(to)) = (id_map.get(&edge.from), id_map.get(&edge.to)) else {
            continue;
        };
        let from_node = subgraph
            .nodes
            .iter()
            .find(|n| id_map.get(&n.id) == Some(from));
        let to_node = subgraph
            .nodes
            .iter()
            .find(|n| id_map.get(&n.id) == Some(to));
        if let (Some(f), Some(t)) = (from_node, to_node) {
            let rel = match edge.edge_type {
                EdgeType::Extends => "<|--",
                EdgeType::Implements => "<|..",
                EdgeType::Contains => "*--",
                _ => "-->",
            };
            out.push_str(&format!(
                "    {} {} {}\n",
                sanitize_class_name(&f.name),
                rel,
                sanitize_class_name(&t.name)
            ));
        }
    }
    Ok(out)
}

fn render_call_graph(subgraph: &Subgraph, vertical: bool) -> Result<String> {
    let dir = if vertical { "TD" } else { "LR" };
    let mut out = format!("graph {dir}\n");
    let functions: Vec<&rbuilder_graph::schema::Node> = subgraph
        .nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .collect();
    let id_map = build_id_map_refs(&functions);

    for node in &functions {
        let id = id_map.get(&node.id).unwrap();
        out.push_str(&format!("    {id}[\"{}\"]\n", escape_label(&node.name)));
    }

    for edge in &subgraph.edges {
        if edge.edge_type != EdgeType::Calls {
            continue;
        }
        if let (Some(from), Some(to)) = (id_map.get(&edge.from), id_map.get(&edge.to)) {
            out.push_str(&format!("    {from} -->|call| {to}\n"));
        }
    }
    Ok(out)
}

fn build_id_map(
    nodes: &[rbuilder_graph::schema::Node],
) -> std::collections::HashMap<uuid::Uuid, String> {
    build_id_map_refs(&nodes.iter().collect::<Vec<_>>())
}

fn build_id_map_refs(
    nodes: &[&rbuilder_graph::schema::Node],
) -> std::collections::HashMap<uuid::Uuid, String> {
    nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, node_diagram_id(i, &n.name)))
        .collect()
}

fn sanitize_class_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Parse diagram type from CLI / MCP string values.
pub fn parse_diagram_type(value: &str) -> DiagramType {
    match value.to_ascii_lowercase().as_str() {
        "class" | "class-diagram" => DiagramType::ClassDiagram,
        "call" | "call-graph" | "callgraph" => DiagramType::CallGraph,
        _ => DiagramType::Flowchart,
    }
}
