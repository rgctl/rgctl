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
/// - `signature:*pattern*` — filter by signature substring (wildcards `*` supported)
/// - `return_type:Type` — filter by return type prefix match
/// - Compound filters with `|` — e.g. `type:Function|return_type:Result`
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

    if let Some(pattern) = query.strip_prefix("signature:") {
        return filter_nodes_by_signature(backend, pattern);
    }

    if let Some(return_type) = query.strip_prefix("return_type:") {
        return filter_nodes_by_return_type(backend, return_type);
    }

    if let Some(module) = query.strip_prefix("module:") {
        return filter_nodes_by_property(backend, "module", module);
    }

    if let Some(resource_type) = query.strip_prefix("resource:") {
        return filter_nodes_by_property(backend, "resource_type", resource_type);
    }

    match query.to_ascii_lowercase().as_str() {
        "functions" | "function" => backend.find_nodes_by_type(NodeType::Function),
        "classes" | "class" => backend.find_nodes_by_type(NodeType::Class),
        "structs" | "struct" => backend.find_nodes_by_type(NodeType::Struct),
        "files" | "file" => backend.find_nodes_by_type(NodeType::File),
        "config" | "configkeys" => backend.find_nodes_by_type(NodeType::ConfigKey),
        "playbooks" | "ansibleplaybooks" => backend.find_nodes_by_type(NodeType::AnsiblePlaybook),
        "ansibleroles" | "roles" => backend.find_nodes_by_type(NodeType::AnsibleRole),
        "cookbooks" | "chefcookbooks" => backend.find_nodes_by_type(NodeType::ChefCookbook),
        "chefrecipes" | "recipes" => backend.find_nodes_by_type(NodeType::ChefRecipe),
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
        "table" => Ok(NodeType::Table),
        "dependency" => Ok(NodeType::Dependency),
        "job" => Ok(NodeType::Job),
        "buildstep" => Ok(NodeType::BuildStep),
        "ansibleplaybook" | "playbook" => Ok(NodeType::AnsiblePlaybook),
        "ansibleplay" => Ok(NodeType::AnsiblePlay),
        "ansibletask" | "task" => Ok(NodeType::AnsibleTask),
        "ansiblerole" | "role" => Ok(NodeType::AnsibleRole),
        "ansiblehandler" | "handler" => Ok(NodeType::AnsibleHandler),
        "ansiblevariable" => Ok(NodeType::AnsibleVariable),
        "ansibletemplate" => Ok(NodeType::AnsibleTemplate),
        "chefcookbook" | "cookbook" => Ok(NodeType::ChefCookbook),
        "chefrecipe" | "recipe" => Ok(NodeType::ChefRecipe),
        "chefresource" | "resource" => Ok(NodeType::ChefResource),
        "chefattribute" => Ok(NodeType::ChefAttribute),
        "cheftemplate" => Ok(NodeType::ChefTemplate),
        "chefcustomresource" => Ok(NodeType::ChefCustomResource),
        other => Err(Error::InvalidQuery(format!("Unknown node type: {other}"))),
    }
}

fn selectivity_rank(part: &str) -> usize {
    if part.starts_with("name:") {
        0
    } else if part.starts_with("signature:") {
        1
    } else if part.starts_with("module:") {
        2
    } else if part.starts_with("resource:") {
        2
    } else if part.starts_with("return_type:") {
        3
    } else if part.starts_with("repo:") {
        4
    } else if part.starts_with("type:") {
        5
    } else if part.starts_with("label:") {
        6
    } else if part.starts_with("name_suffix:") {
        7
    } else {
        8
    }
}

fn filter_nodes_by_signature(backend: &MemoryBackend, pattern: &str) -> Result<Vec<Node>> {
    Ok(backend
        .all_nodes()?
        .into_iter()
        .filter(|node| {
            node.signature_text()
                .is_some_and(|sig| signature_wildcard_match(pattern, sig))
        })
        .collect())
}

fn filter_nodes_by_return_type(backend: &MemoryBackend, prefix: &str) -> Result<Vec<Node>> {
    Ok(backend
        .all_nodes()?
        .into_iter()
        .filter(|node| {
            node.return_type_text()
                .is_some_and(|ty| ty.starts_with(prefix))
        })
        .collect())
}

fn filter_nodes_by_property(
    backend: &MemoryBackend,
    key: &str,
    value: &str,
) -> Result<Vec<Node>> {
    Ok(backend
        .all_nodes()?
        .into_iter()
        .filter(|node| {
            node.get_property(key)
                .is_some_and(|v| v.eq_ignore_ascii_case(value))
        })
        .collect())
}

fn signature_wildcard_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 1 {
            return text.contains(parts[0]);
        }
        let mut start = 0usize;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 {
                if !text.starts_with(part) {
                    return false;
                }
                start = part.len();
            } else if i == parts.len() - 1 {
                if !text[start..].ends_with(part) {
                    return false;
                }
            } else if let Some(pos) = text[start..].find(part) {
                start += pos + part.len();
            } else {
                return false;
            }
        }
        true
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
    fn test_query_signature_filter() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(
                Node::new(NodeType::Function, "run".to_string())
                    .with_signature("async fn run()"),
            )
            .unwrap();
        let results = execute(&backend, "signature:*async*").unwrap();
        assert_eq!(results.len(), 1);
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
