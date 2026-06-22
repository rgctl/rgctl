//! Multi-repo workspace manifest and persistence

use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::code_graph::{CodeGraph, GRAPH_DIR, GRAPH_FILE};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Workspace manifest filename.
pub const WORKSPACE_FILE: &str = "workspace.json";

/// A repository entry in a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoEntry {
    /// Namespace identifier for queries (`repo:namespace`)
    pub namespace: String,
    /// Path to repository root (relative to workspace or absolute)
    pub path: PathBuf,
}

/// Multi-repo workspace manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    /// Manifest format version
    pub version: String,
    /// Registered repositories
    pub repos: Vec<RepoEntry>,
}

impl Default for WorkspaceManifest {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            repos: Vec::new(),
        }
    }
}

impl WorkspaceManifest {
    /// Load workspace manifest from a directory.
    pub fn load(workspace_root: &Path) -> Result<Self> {
        let path = workspace_root.join(GRAPH_DIR).join(WORKSPACE_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| Error::SerdeError(e.to_string()))
    }

    /// Save manifest to disk.
    pub fn save(&self, workspace_root: &Path) -> Result<PathBuf> {
        let dir = workspace_root.join(GRAPH_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(WORKSPACE_FILE);
        let json =
            serde_json::to_string_pretty(self).map_err(|e| Error::SerdeError(e.to_string()))?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Add or update a repository entry.
    pub fn add_repo(&mut self, namespace: impl Into<String>, path: PathBuf) -> Result<()> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        if self.repos.iter().any(|r| r.namespace == namespace) {
            return Err(Error::InvalidQuery(format!(
                "Namespace already registered: {namespace}"
            )));
        }
        self.repos.push(RepoEntry { namespace, path });
        Ok(())
    }

    /// Remove a repository by namespace.
    pub fn remove_repo(&mut self, namespace: &str) -> bool {
        let before = self.repos.len();
        self.repos.retain(|r| r.namespace != namespace);
        self.repos.len() < before
    }

    /// Resolve repo path relative to workspace root.
    pub fn resolve_path(&self, entry: &RepoEntry, workspace_root: &Path) -> PathBuf {
        if entry.path.is_absolute() {
            entry.path.clone()
        } else {
            workspace_root.join(&entry.path)
        }
    }
}

fn validate_namespace(namespace: &str) -> Result<()> {
    if namespace.is_empty() {
        return Err(Error::InvalidQuery("Namespace cannot be empty".into()));
    }
    if !namespace
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::InvalidQuery(format!(
            "Invalid namespace '{namespace}'. Use alphanumeric, '-', or '_'."
        )));
    }
    Ok(())
}

/// Stamp every node in a graph with a repository namespace property.
pub fn stamp_repo_namespace(graph: &mut CodeGraph, namespace: &str) {
    let nodes = graph.backend().all_nodes().unwrap_or_default();
    let ids: Vec<_> = nodes.iter().map(|n| n.id).collect();
    for id in ids {
        if let Ok(Some(mut node)) = graph.backend().get_node(id) {
            node.properties
                .insert("repo".to_string(), namespace.to_string());
            let _ = graph.backend_mut().insert_node(node);
        }
    }
}

/// Load merged workspace graph if it exists.
pub fn load_workspace_graph(workspace_root: &Path) -> Result<CodeGraph> {
    let path = workspace_root.join(GRAPH_DIR).join(GRAPH_FILE);
    if !path.exists() {
        return Err(Error::NotFound(format!(
            "Workspace graph not found at {}. Run `rbuilder workspace sync`.",
            path.display()
        )));
    }
    let json = std::fs::read_to_string(path)?;
    CodeGraph::import_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_workspace_manifest_roundtrip() {
        let temp = TempDir::new().unwrap();
        let mut manifest = WorkspaceManifest::default();
        manifest
            .add_repo("backend", PathBuf::from("../backend"))
            .unwrap();
        manifest.save(temp.path()).unwrap();
        let loaded = WorkspaceManifest::load(temp.path()).unwrap();
        assert_eq!(loaded.repos.len(), 1);
        assert_eq!(loaded.repos[0].namespace, "backend");
    }
}
