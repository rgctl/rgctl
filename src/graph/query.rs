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
/// - `label:soa:service` — filter by label
/// - `repo:backend` — filter by repository namespace (multi-repo)
/// - `name_suffix:Service` — filter by name suffix (naming patterns)
/// - `functions`, `classes`, `files`, `config` — common shortcuts
/// - Compound filters with `|` — e.g. `repo:backend|type:Function`
/// - `all` or empty string — return all nodes
pub fn execute(backend: &MemoryBackend, query: &str) -> Result<Vec<Node>> {
    let query = query.trim();
    if query.is_empty() || query.eq_ignore_ascii_case("all") {
        return backend.all_nodes();
    }

    if query.contains('|') {
        let parts: Vec<&str> = query.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return backend.all_nodes();
        }
        // Intersect starting from the most selective clause (smallest result set)
        let mut ordered = parts;
        ordered.sort_by_key(|part| selectivity_rank(part));
        let mut results = execute(backend, ordered[0])?;
        for part in &ordered[1..] {
            let next = execute(backend, part)?;
            let ids: std::collections::HashSet<_> = next.iter().map(|n| n.id).collect();
            results.retain(|n| ids.contains(&n.id));
        }
        return Ok(results);
    }

    if let Some(repo) = query.strip_prefix("repo:") {
        return backend.find_nodes_by_property("repo", repo);
    }

    if let Some(type_name) = query.strip_prefix("type:") {
        let node_type = parse_node_type(type_name)?;
        return backend.find_nodes_by_type(node_type);
    }

    if let Some(name) = query.strip_prefix("name:") {
        return backend.find_nodes_by_name(name);
    }

    if let Some(label) = query.strip_prefix("label:") {
        return backend.find_nodes_by_label(label);
    }

    if let Some(suffix) = query.strip_prefix("name_suffix:") {
        return backend.find_nodes_by_name_suffix(suffix);
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

/// Return query results in fixed-size chunks for streaming large result sets.
pub fn execute_chunks(
    backend: &MemoryBackend,
    query: &str,
    chunk_size: usize,
) -> Result<Vec<Vec<Node>>> {
    if chunk_size == 0 {
        return Err(Error::InvalidQuery("chunk_size must be > 0".into()));
    }
    let results = execute(backend, query)?;
    Ok(results
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect())
}

fn selectivity_rank(part: &str) -> usize {
    if part.starts_with("name:") {
        0
    } else if part.starts_with("repo:") {
        1
    } else if part.starts_with("type:") {
        2
    } else if part.starts_with("label:") {
        3
    } else if part.starts_with("name_suffix:") {
        4
    } else {
        5
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

    #[test]
    fn test_query_by_repo() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(
                Node::new(NodeType::Function, "main".to_string())
                    .with_property("repo".to_string(), "api".to_string()),
            )
            .unwrap();
        backend
            .insert_node(
                Node::new(NodeType::Function, "other".to_string())
                    .with_property("repo".to_string(), "web".to_string()),
            )
            .unwrap();

        let results = execute(&backend, "repo:api").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[test]
    fn test_query_by_name_suffix() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Class, "UserService".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Class, "OrderService".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Class, "UserController".to_string()))
            .unwrap();

        let results = execute(&backend, "name_suffix:Service").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|n| n.name.ends_with("Service")));
    }

    #[test]
    fn test_execute_chunks() {
        let mut backend = MemoryBackend::new();
        for i in 0..5 {
            backend
                .insert_node(Node::new(NodeType::Function, format!("fn{i}")))
                .unwrap();
        }

        let chunks = execute_chunks(&backend, "functions", 2).unwrap();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.iter().map(|c| c.len()).sum::<usize>(), 5);
    }
}
