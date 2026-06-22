//! GraphML XML export (Phase 14.4).

use crate::select_subgraph;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::Node;

/// Export matching nodes and their internal edges as GraphML XML.
pub fn export_graphml(backend: &MemoryBackend, query: &str) -> Result<String> {
    let subgraph = select_subgraph(backend, query, None)?;
    if subgraph.nodes.is_empty() {
        return Err(Error::InvalidQuery(format!(
            "No nodes matched query: {query}"
        )));
    }
    Ok(render_graphml(&subgraph.nodes, &subgraph.edges))
}

fn render_graphml(nodes: &[Node], edges: &[rbuilder_graph::schema::Edge]) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://graphml.graphdrawing.org/xmlns
         http://graphml.graphdrawing.org/xmlns/1.0/graphml.xsd">
  <key id="name" for="node" attr.name="name" attr.type="string"/>
  <key id="type" for="node" attr.name="type" attr.type="string"/>
  <key id="file" for="node" attr.name="file" attr.type="string"/>
  <key id="line" for="node" attr.name="line" attr.type="int"/>
  <key id="complexity" for="node" attr.name="complexity" attr.type="int"/>
  <key id="edge_type" for="edge" attr.name="type" attr.type="string"/>
  <graph id="G" edgedefault="directed">
"#,
    );

    for node in nodes {
        out.push_str(&render_node_xml(node));
    }

    for (idx, edge) in edges.iter().enumerate() {
        out.push_str(&format!(
            r#"    <edge id="e{idx}" source="{}" target="{}">
      <data key="edge_type">{:?}</data>
    </edge>
"#,
            edge.from, edge.to, edge.edge_type
        ));
    }

    out.push_str("  </graph>\n</graphml>\n");
    out
}

fn render_node_xml(node: &Node) -> String {
    let file = node.file_path.as_deref().unwrap_or("");
    let line = node.start_line.unwrap_or(0);
    let complexity = node
        .get_property("cyclomatic")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    format!(
        r#"    <node id="{}">
      <data key="name">{}</data>
      <data key="type">{:?}</data>
      <data key="file">{}</data>
      <data key="line">{}</data>
      <data key="complexity">{}</data>
    </node>
"#,
        node.id,
        xml_escape(&node.name),
        node.node_type,
        xml_escape(file),
        line,
        complexity
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::NodeType;

    #[test]
    fn test_xml_escape_ampersand() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn test_render_graphml_header() {
        let node = Node::new(NodeType::Function, "main".into());
        let xml = render_graphml(&[node], &[]);
        assert!(xml.contains("graphml"));
        assert!(xml.contains("<key id=\"name\""));
    }
}
