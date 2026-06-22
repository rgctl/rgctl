//! Shared application state for MCP and REST API servers.

use rbuilder_error::Result;
use rbuilder_graph::CodeGraph;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Thread-safe context shared by MCP and HTTP servers.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<RwLock<AppStateInner>>,
}

struct AppStateInner {
    repo_root: PathBuf,
    graph: CodeGraph,
}

impl AppState {
    /// Load graph from a repository root.
    pub fn from_repo(repo_root: impl AsRef<Path>) -> Result<Self> {
        let repo_root = repo_root.as_ref().to_path_buf();
        let graph = CodeGraph::load_from_repo(&repo_root)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(AppStateInner { repo_root, graph })),
        })
    }

    /// Repository root path.
    pub fn repo_root(&self) -> PathBuf {
        self.inner.read().unwrap().repo_root.clone()
    }

    /// Read-only access to the graph.
    pub fn with_graph<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&CodeGraph) -> Result<T>,
    {
        let inner = self.inner.read().unwrap();
        f(&inner.graph)
    }

    /// Mutable access to the graph (e.g. after incremental update).
    pub fn with_graph_mut<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut CodeGraph) -> Result<T>,
    {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner.graph)
    }

    /// Reload graph from disk.
    pub fn reload(&self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();
        inner.graph = CodeGraph::load_from_repo(&inner.repo_root)?;
        Ok(())
    }

    /// Clone handle for background watch threads.
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }
}
