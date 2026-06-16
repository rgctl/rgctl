//! Graph query interface

use crate::graph::backend::GraphBackend;
use crate::error::{Error, Result};
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::{Node, NodeType};

/// Execute a simple query against the graph backend.
///
/// Supported forms:
/// - `type:Function` or `type:function` — filter by node type
/// - `name:main` — filter by exact name
/// - `functions`, `classes`, `files`, `config` — common shortcuts
/// - `all` or empty string — return all nodes
pub fn execute(backend: &MemoryBackend, query: &str) -> Result<Vec<Node>> {
    let query = query.trim();
    if query.is_empty() || query.eq_ignore_ascii_case("all") {
        return backend.all_nodes();
    }

    if let Some(type_name) = query.strip_prefix("type:") {
        let node_type = parse_node_type(type_name)?;
        return backend.find_nodes_by_type(node_type);
    }

    if let Some(name) = query.strip_prefix("name:") {
        return backend.find_nodes_by_name(name);
    }

    match query.to_ascii_lowercase().as_str() {
        "functions" | "function" => backend.find_nodes_by_type(NodeType::Function),
        "classes" | "class" => backend.find_nodes_by_type(NodeType::Class),
        "structs" | "struct" => backend.find_nodes_by_type(NodeType::Struct),
        "files" | "file" => backend.find_nodes_by_type(NodeType::File),
        "config" | "configkeys" => backend.find_nodes_by_type(NodeType::ConfigKey),
        _ => backend.find_nodes(query),
    }
}

fn parse_node_type(value: &str) -> Result<NodeType> {
    match value.to_ascii_lowercase().as_str() {
        "function" => Ok(NodeType::Function),
        "class" => Ok(NodeType::Class),
        "struct" => Ok(NodeType::Struct),
        "enum" => Ok(NodeType::Enum),
        "interface" => Ok(NodeType::Interface),
        "module" => Ok(NodeType::Module),
        "variable" => Ok(NodeType::Variable),
        "file" => Ok(NodeType::File),
        "configkey" | "config" => Ok(NodeType::ConfigKey),
        "typealias" => Ok(NodeType::TypeAlias),
        "macro" => Ok(NodeType::Macro),
        "import" => Ok(NodeType::Import),
        other => Err(Error::InvalidQuery(format!("Unknown node type: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::Node;

    #[test]
    fn test_query_by_type() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "main".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::File, "main.rs".to_string()))
            .unwrap();

        let results = execute(&backend, "type:Function").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[test]
    fn test_query_functions_shorthand() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "foo".to_string()))
            .unwrap();

        let results = execute(&backend, "functions").unwrap();
        assert_eq!(results.len(), 1);
    }
}
