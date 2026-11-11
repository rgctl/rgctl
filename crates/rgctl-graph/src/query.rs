//! Graph query interface
//!
//! **Complexity:** indexed clauses are O(k) for k matching IDs; compound `|` queries intersect
//! ID sets before materializing nodes; full scans (signature/return_type) are O(N).

use crate::backend::GraphBackend;
use crate::backend::MemoryBackend;
use crate::schema::{Node, NodeType};
use rgctl_error::{Error, Result};
use std::collections::HashSet;
use uuid::Uuid;

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
        let parts: Vec<&str> = query
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            return backend.all_nodes();
        }
        let mut ordered = parts;
        ordered.sort_by_key(|part| selectivity_rank(part));
        let mut intersection = execute_node_ids(backend, ordered[0])?;
        for part in &ordered[1..] {
            let next = execute_node_ids(backend, part)?;
            intersection.retain(|id| next.contains(id));
            if intersection.is_empty() {
                break;
            }
        }
        return backend.get_nodes_by_ids(&intersection);
    }

    let ids = execute_node_ids(backend, query)?;
    backend.get_nodes_by_ids(&ids)
}

/// Stream query results one node at a time without materializing the full result set.
pub struct QueryStream<'a> {
    backend: &'a MemoryBackend,
    ids: Vec<Uuid>,
    pos: usize,
}

impl<'a> Iterator for QueryStream<'a> {
    type Item = Result<Node>;

    fn next(&mut self) -> Option<Self::Item> {
        let id = *self.ids.get(self.pos)?;
        self.pos += 1;
        match self.backend.get_node(id) {
            Ok(Some(node)) => Some(Ok(node)),
            Ok(None) => self.next(),
            Err(err) => Some(Err(err)),
        }
    }
}

/// Lazily stream nodes matching `query` (IDs resolved first, nodes loaded on demand).
pub fn stream_query<'a>(backend: &'a MemoryBackend, query: &str) -> Result<QueryStream<'a>> {
    let ids: Vec<Uuid> = execute_node_ids(backend, query)?.into_iter().collect();
    Ok(QueryStream {
        backend,
        ids,
        pos: 0,
    })
}

/// Return query results in fixed-size chunks without cloning the full result set twice.
pub fn execute_chunks(
    backend: &MemoryBackend,
    query: &str,
    chunk_size: usize,
) -> Result<Vec<Vec<Node>>> {
    if chunk_size == 0 {
        return Err(Error::InvalidQuery("chunk_size must be > 0".into()));
    }
    let mut chunks = Vec::new();
    let mut current = Vec::with_capacity(chunk_size);
    for node in stream_query(backend, query)? {
        current.push(node?);
        if current.len() == chunk_size {
            chunks.push(current);
            current = Vec::with_capacity(chunk_size);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    Ok(chunks)
}

fn execute_node_ids(backend: &MemoryBackend, query: &str) -> Result<HashSet<Uuid>> {
    let query = query.trim();
    if query.is_empty() || query.eq_ignore_ascii_case("all") {
        return Ok(backend.all_node_ids()?.into_iter().collect());
    }

    if let Some(repo) = query.strip_prefix("repo:") {
        return Ok(backend
            .find_node_ids_by_property("repo", repo)?
            .into_iter()
            .collect());
    }

    if let Some(type_name) = query.strip_prefix("type:") {
        let node_type = parse_node_type(type_name)?;
        return Ok(backend
            .find_node_ids_by_type(node_type)?
            .into_iter()
            .collect());
    }

    if let Some(name) = query.strip_prefix("name:") {
        return Ok(backend.find_node_ids_by_name(name)?.into_iter().collect());
    }

    if let Some(label) = query.strip_prefix("label:") {
        return Ok(backend.find_node_ids_by_label(label)?.into_iter().collect());
    }

    if let Some(suffix) = query.strip_prefix("name_suffix:") {
        return filter_node_ids_by_name_suffix(backend, suffix);
    }

    if let Some(pattern) = query.strip_prefix("signature:") {
        return filter_node_ids_by_signature(backend, pattern);
    }

    if let Some(return_type) = query.strip_prefix("return_type:") {
        return filter_node_ids_by_return_type(backend, return_type);
    }

    if let Some(module) = query.strip_prefix("module:") {
        return Ok(backend
            .find_node_ids_by_property("module", module)?
            .into_iter()
            .collect());
    }

    if let Some(resource_type) = query.strip_prefix("resource:") {
        return Ok(backend
            .find_node_ids_by_property("resource_type", resource_type)?
            .into_iter()
            .collect());
    }

    match query.to_ascii_lowercase().as_str() {
        "functions" | "function" => Ok(backend
            .find_node_ids_by_type(NodeType::Function)?
            .into_iter()
            .collect()),
        "classes" | "class" => Ok(backend
            .find_node_ids_by_type(NodeType::Class)?
            .into_iter()
            .collect()),
        "structs" | "struct" => Ok(backend
            .find_node_ids_by_type(NodeType::Struct)?
            .into_iter()
            .collect()),
        "files" | "file" => Ok(backend
            .find_node_ids_by_type(NodeType::File)?
            .into_iter()
            .collect()),
        "config" | "configkeys" => Ok(backend
            .find_node_ids_by_type(NodeType::ConfigKey)?
            .into_iter()
            .collect()),
        "playbooks" | "ansibleplaybooks" => Ok(backend
            .find_node_ids_by_type(NodeType::AnsiblePlaybook)?
            .into_iter()
            .collect()),
        "ansibleroles" | "roles" => Ok(backend
            .find_node_ids_by_type(NodeType::AnsibleRole)?
            .into_iter()
            .collect()),
        "cookbooks" | "chefcookbooks" => Ok(backend
            .find_node_ids_by_type(NodeType::ChefCookbook)?
            .into_iter()
            .collect()),
        "chefrecipes" | "recipes" => Ok(backend
            .find_node_ids_by_type(NodeType::ChefRecipe)?
            .into_iter()
            .collect()),
        "puppetmodules" | "modules" => Ok(backend
            .find_node_ids_by_type(NodeType::PuppetModule)?
            .into_iter()
            .collect()),
        "puppetclasses" => Ok(backend
            .find_node_ids_by_type(NodeType::PuppetClass)?
            .into_iter()
            .collect()),
        _ => Ok(backend
            .find_nodes(query)?
            .into_iter()
            .map(|n| n.id)
            .collect()),
    }
}

fn filter_node_ids_by_signature(backend: &MemoryBackend, pattern: &str) -> Result<HashSet<Uuid>> {
    let mut matching_ids = HashSet::new();
    backend.for_each_node(|node| {
        if node
            .signature_text()
            .is_some_and(|sig| signature_wildcard_match(pattern, sig))
        {
            matching_ids.insert(node.id);
        }
    })?;
    Ok(matching_ids)
}

fn filter_node_ids_by_return_type(backend: &MemoryBackend, prefix: &str) -> Result<HashSet<Uuid>> {
    let mut matching_ids = HashSet::new();
    backend.for_each_node(|node| {
        if node
            .return_type_text()
            .is_some_and(|ty| ty.starts_with(prefix))
        {
            matching_ids.insert(node.id);
        }
    })?;
    Ok(matching_ids)
}

fn filter_node_ids_by_name_suffix(backend: &MemoryBackend, suffix: &str) -> Result<HashSet<Uuid>> {
    let mut matching_ids = HashSet::new();
    backend.for_each_node(|node| {
        if node.name.ends_with(suffix) {
            matching_ids.insert(node.id);
        }
    })?;
    Ok(matching_ids)
}

fn parse_node_type(value: &str) -> Result<NodeType> {
    match value.to_ascii_lowercase().as_str() {
        "function" => Ok(NodeType::Function),
        "class" => Ok(NodeType::Class),
        "struct" => Ok(NodeType::Struct),
        "enum" => Ok(NodeType::Enum),
        "interface" => Ok(NodeType::Interface),
        "annotation" => Ok(NodeType::Annotation),
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
        "ansibleplaybook" => Ok(NodeType::AnsiblePlaybook),
        "ansibleplay" => Ok(NodeType::AnsiblePlay),
        "ansibletask" => Ok(NodeType::AnsibleTask),
        "ansiblerole" => Ok(NodeType::AnsibleRole),
        "ansiblehandler" => Ok(NodeType::AnsibleHandler),
        "ansiblevariable" => Ok(NodeType::AnsibleVariable),
        "ansibletemplate" => Ok(NodeType::AnsibleTemplate),
        "chefcookbook" => Ok(NodeType::ChefCookbook),
        "chefrecipe" => Ok(NodeType::ChefRecipe),
        "chefresource" => Ok(NodeType::ChefResource),
        "chefattribute" => Ok(NodeType::ChefAttribute),
        "cheftemplate" => Ok(NodeType::ChefTemplate),
        "chefcustomresource" => Ok(NodeType::ChefCustomResource),
        "puppetmodule" => Ok(NodeType::PuppetModule),
        "puppetclass" => Ok(NodeType::PuppetClass),
        "puppetdefinedtype" => Ok(NodeType::PuppetDefinedType),
        "puppetresource" => Ok(NodeType::PuppetResource),
        "puppetvariable" => Ok(NodeType::PuppetVariable),
        "puppetfact" => Ok(NodeType::PuppetFact),
        "puppetnode" => Ok(NodeType::PuppetNode),
        other => Err(Error::InvalidQuery(format!("unknown node type: {other}"))),
    }
}

fn selectivity_rank(clause: &str) -> usize {
    if clause.starts_with("name:") {
        0
    } else if clause.starts_with("type:") {
        1
    } else if clause.starts_with("label:") {
        2
    } else if clause.starts_with("repo:") {
        3
    } else if clause.starts_with("module:") || clause.starts_with("resource:") {
        4
    } else if clause.starts_with("signature:") || clause.starts_with("return_type:") {
        5
    } else {
        6
    }
}

fn signature_wildcard_match(pattern: &str, signature: &str) -> bool {
    if !pattern.contains('*') {
        return signature.contains(pattern);
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return true;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !signature.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if let Some(found) = signature[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }
    parts
        .last()
        .is_none_or(|last| last.is_empty() || signature.ends_with(last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Node;

    #[test]
    fn execute_delegates_to_node_ids() {
        let mut backend = MemoryBackend::new();
        let node = Node::new(NodeType::Function, "main");
        let id = node.id;
        backend.insert_node(node).unwrap();
        let results = execute(&backend, "name:main").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
    }

    #[test]
    fn signature_filter_uses_for_each_node() {
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Function, "foo");
        node.signature = Some(crate::schema::SharedStr::from("fn foo() -> i32"));
        backend.insert_node(node).unwrap();
        let ids = filter_node_ids_by_signature(&backend, "*i32").unwrap();
        assert_eq!(ids.len(), 1);
    }
}
