//! Configuration analysis
//!
//! Tasks 2.2.1 and 2.2.2: Unused config keys and missing env vars.

use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use std::collections::HashSet;
use std::path::Path;

/// An unused configuration key.
#[derive(Debug, Clone, PartialEq)]
pub struct UnusedConfigKey {
    /// Config key path
    pub key: String,
    /// Source file
    pub file: Option<String>,
    /// Confidence that key is truly unused
    pub confidence: f64,
}

/// A missing environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEnvVar {
    /// Variable name
    pub var: String,
    /// Files referencing this variable
    pub referenced_in: Vec<String>,
}

/// Configuration analyzer.
pub struct ConfigAnalyzer;

impl ConfigAnalyzer {
    /// Find config keys with no incoming UsesConfig edges.
    pub fn find_unused_keys(backend: &MemoryBackend) -> Result<Vec<UnusedConfigKey>> {
        let config_keys = backend.find_nodes_by_type(NodeType::ConfigKey)?;
        let edges = backend.all_edges()?;
        let used: HashSet<_> = edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::UsesConfig)
            .map(|e| e.to)
            .collect();

        Ok(config_keys
            .into_iter()
            .filter(|k| !used.contains(&k.id))
            .map(|k| UnusedConfigKey {
                key: k.name.clone(),
                file: k.file_path.clone(),
                confidence: if k.name.contains("legacy") || k.name.contains("old") {
                    0.9
                } else {
                    0.7
                },
            })
            .collect())
    }

    /// Find env vars referenced in code but not defined in env files.
    pub fn find_missing_env_vars(
        backend: &MemoryBackend,
        env_files: &[&Path],
    ) -> Result<Vec<MissingEnvVar>> {
        let mut defined = HashSet::new();
        for path in env_files {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        if let Some((key, _)) = trimmed.split_once('=') {
                            defined.insert(key.trim().to_string());
                        }
                    }
                }
            }
        }

        let env_nodes = backend.find_nodes_by_type(NodeType::Variable)?;
        let mut referenced: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for node in env_nodes {
            if node.has_label("env") {
                referenced
                    .entry(node.name.clone())
                    .or_default()
                    .push(node.file_path.clone().unwrap_or_default());
            }
        }

        Ok(referenced
            .into_iter()
            .filter(|(var, _)| !defined.contains(var))
            .map(|(var, files)| MissingEnvVar {
                var,
                referenced_in: files,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, Node};

    #[test]
    fn test_unused_config_detection() {
        let mut backend = MemoryBackend::new();
        let used_key = Node::new(NodeType::ConfigKey, "database.host".to_string());
        let unused_key = Node::new(NodeType::ConfigKey, "legacy.old_feature".to_string());
        let func = Node::new(NodeType::Function, "connect".to_string());
        let used_id = used_key.id;
        let func_id = func.id;
        backend.insert_node(used_key).unwrap();
        backend.insert_node(unused_key).unwrap();
        backend.insert_node(func).unwrap();
        backend
            .insert_edge(Edge::new(func_id, used_id, EdgeType::UsesConfig))
            .unwrap();

        let unused = ConfigAnalyzer::find_unused_keys(&backend).unwrap();
        assert!(unused.iter().any(|k| k.key == "legacy.old_feature"));
        assert!(!unused.iter().any(|k| k.key == "database.host"));
    }
}
