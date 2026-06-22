//! Cross-repository edge detection

use rbuilder_error::Result;
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{Edge, EdgeType, NodeType};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Result of cross-repo linking.
#[derive(Debug, Clone, Default)]
pub struct CrossRepoLinkReport {
    /// Number of cross-repo edges created
    pub edges_added: usize,
    /// Pairs linked (from_repo, to_repo)
    pub repo_pairs: Vec<(String, String)>,
}

/// Detect and add cross-repository dependency edges.
///
/// Heuristics:
/// - Import nodes in repo A referencing symbols that exist in repo B
/// - Shared library/function names across repos (same name, different repo)
pub fn link_cross_repo(backend: &mut MemoryBackend) -> Result<CrossRepoLinkReport> {
    let nodes = backend.all_nodes()?;
    let mut by_repo: HashMap<String, Vec<_>> = HashMap::new();
    let mut symbol_names: HashMap<String, HashSet<String>> = HashMap::new();

    for node in &nodes {
        if let Some(repo) = node.get_property("repo") {
            by_repo.entry(repo.clone()).or_default().push(node.clone());

            if matches!(
                node.node_type,
                NodeType::Function | NodeType::Class | NodeType::Module | NodeType::Import
            ) {
                symbol_names
                    .entry(node.name.clone())
                    .or_default()
                    .insert(repo.clone());
            }
        }
    }

    if by_repo.len() < 2 {
        return Ok(CrossRepoLinkReport::default());
    }

    let mut report = CrossRepoLinkReport::default();
    let mut existing: HashSet<(Uuid, Uuid)> = HashSet::new();
    for edge in backend.all_edges()? {
        existing.insert((edge.from, edge.to));
    }

    // Link imports to symbols in other repos
    for node in &nodes {
        let Some(from_repo) = node.get_property("repo") else {
            continue;
        };

        if node.node_type != NodeType::Import {
            continue;
        }

        // Import name may match a symbol in another repo
        if let Some(repos) = symbol_names.get(&node.name) {
            for to_repo in repos {
                if to_repo == from_repo {
                    continue;
                }
                if let Some(target) = find_symbol_in_repo(backend, &node.name, to_repo)? {
                    let edge = Edge::new(node.id, target, EdgeType::Uses)
                        .with_property("cross_repo".to_string(), "true".to_string())
                        .with_property("target_repo".to_string(), to_repo.clone());
                    if existing.insert((node.id, target)) {
                        backend.insert_edge(edge)?;
                        report.edges_added += 1;
                        let pair = (from_repo.clone(), to_repo.clone());
                        if !report.repo_pairs.contains(&pair) {
                            report.repo_pairs.push(pair);
                        }
                    }
                }
            }
        }
    }

    // Shared public API names across repos (functions/classes with same name)
    for (name, repos) in &symbol_names {
        if repos.len() < 2 {
            continue;
        }
        let repo_list: Vec<_> = repos.iter().cloned().collect();
        for i in 0..repo_list.len() {
            for j in (i + 1)..repo_list.len() {
                let repo_a = &repo_list[i];
                let repo_b = &repo_list[j];
                if let (Some(id_a), Some(id_b)) = (
                    find_symbol_in_repo(backend, name, repo_a)?,
                    find_symbol_in_repo(backend, name, repo_b)?,
                ) {
                    let edge = Edge::new(id_a, id_b, EdgeType::Uses)
                        .with_property("cross_repo".to_string(), "true".to_string())
                        .with_property("shared_symbol".to_string(), name.clone());
                    if existing.insert((id_a, id_b)) {
                        backend.insert_edge(edge)?;
                        report.edges_added += 1;
                    }
                }
            }
        }
    }

    Ok(report)
}

fn find_symbol_in_repo(backend: &MemoryBackend, name: &str, repo: &str) -> Result<Option<Uuid>> {
    let candidates = backend.find_nodes_by_name(name)?;
    Ok(candidates
        .into_iter()
        .find(|n| n.get_property("repo").is_some_and(|r| r == repo))
        .map(|n| n.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::Node;

    #[test]
    fn test_cross_repo_import_link() {
        let mut backend = MemoryBackend::new();

        let import = Node::new(NodeType::Import, "AuthService".into())
            .with_property("repo".into(), "frontend".into());
        let service = Node::new(NodeType::Class, "AuthService".into())
            .with_property("repo".into(), "backend".into());

        let import_id = import.id;
        let service_id = service.id;
        backend.insert_node(import).unwrap();
        backend.insert_node(service).unwrap();

        let report = link_cross_repo(&mut backend).unwrap();
        assert!(report.edges_added >= 1);
        assert!(backend.edge_count() >= 1);

        let edges = backend.all_edges().unwrap();
        assert!(edges
            .iter()
            .any(|e| e.from == import_id && e.to == service_id));
    }
}
