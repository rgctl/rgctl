//! File change tracking
//!
//! Task 5.1.1: Track file hashes to detect changes

use rbuilder_error::{Error, Result};
use rbuilder_graph::code_graph::{CodeGraph, GRAPH_DIR};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Filename for persisted file hash metadata.
pub const FILE_HASHES_FILE: &str = "file_hashes.json";

/// Persisted hash metadata for incremental indexing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileHashMetadata {
    /// rBuilder version that created this metadata
    pub version: String,
    /// ISO-8601 timestamp of last index
    pub indexed_at: String,
    /// Git commit at last index (if available)
    pub last_commit: Option<String>,
    /// Relative file path -> blake3 hex hash
    pub files: HashMap<String, String>,
    /// Relative file path -> node IDs defined in that file
    pub node_mapping: HashMap<String, Vec<Uuid>>,
}

impl Default for FileHashMetadata {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            indexed_at: String::new(),
            last_commit: None,
            files: HashMap::new(),
            node_mapping: HashMap::new(),
        }
    }
}

/// Tracks file content hashes to detect changes since the last index.
#[derive(Debug, Clone)]
pub struct FileTracker {
    repo_root: PathBuf,
    metadata: FileHashMetadata,
}

impl FileTracker {
    /// Create a tracker for a repository root.
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            metadata: FileHashMetadata::default(),
        }
    }

    /// Load persisted hash metadata from `.rbuilder/file_hashes.json`.
    pub fn load(repo_root: impl AsRef<Path>) -> Result<Self> {
        let repo_root = repo_root.as_ref().to_path_buf();
        let path = repo_root.join(GRAPH_DIR).join(FILE_HASHES_FILE);
        if !path.exists() {
            return Ok(Self::new(repo_root));
        }

        let json = std::fs::read_to_string(path)?;
        let metadata: FileHashMetadata =
            serde_json::from_str(&json).map_err(|e| Error::SerdeError(e.to_string()))?;

        Ok(Self {
            repo_root,
            metadata,
        })
    }

    /// Compute blake3 hash of a file's contents.
    pub fn hash_file(path: &Path) -> Result<String> {
        let bytes = std::fs::read(path)?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    /// Record hashes for all discovered files and build node-to-file mapping from the graph.
    pub fn index_files(&mut self, files: &[PathBuf], graph: &CodeGraph) -> Result<()> {
        self.metadata.files.clear();
        self.metadata.node_mapping = build_node_mapping(graph);

        for file in files {
            let rel = relative_path(&self.repo_root, file)?;
            self.metadata.files.insert(rel, Self::hash_file(file)?);
        }

        self.metadata.indexed_at = chrono_lite_now();
        self.metadata.last_commit = current_git_commit(&self.repo_root);
        self.metadata.version = env!("CARGO_PKG_VERSION").to_string();
        Ok(())
    }

    /// Compare current file hashes against stored metadata.
    pub fn detect_changes(&self, files: &[PathBuf]) -> Result<ChangeSet> {
        let current: HashMap<String, String> = files
            .iter()
            .filter_map(|path| {
                relative_path(&self.repo_root, path)
                    .ok()
                    .map(|rel| (rel, Self::hash_file(path).unwrap_or_default()))
            })
            .collect();

        let previous: HashSet<&str> = self.metadata.files.keys().map(String::as_str).collect();
        let current_keys: HashSet<&str> = current.keys().map(String::as_str).collect();

        let mut added = Vec::new();
        let mut changed = Vec::new();
        let mut deleted = Vec::new();

        for (rel, hash) in &current {
            match self.metadata.files.get(rel.as_str()) {
                None => added.push(rel.clone()),
                Some(prev) if prev != hash => changed.push(rel.clone()),
                _ => {}
            }
        }

        for rel in previous.difference(&current_keys) {
            deleted.push((*rel).to_string());
        }

        Ok(ChangeSet {
            added,
            changed,
            deleted,
        })
    }

    /// Return node IDs associated with a relative file path.
    pub fn nodes_for_file(&self, rel_path: &str) -> &[Uuid] {
        self.metadata
            .node_mapping
            .get(rel_path)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Persist hash metadata to disk.
    pub fn save(&self) -> Result<PathBuf> {
        let dir = self.repo_root.join(GRAPH_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(FILE_HASHES_FILE);
        let json = serde_json::to_string_pretty(&self.metadata)
            .map_err(|e| Error::SerdeError(e.to_string()))?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Git commit recorded at last index.
    pub fn last_commit(&self) -> Option<&str> {
        self.metadata.last_commit.as_deref()
    }

    /// Access stored file hashes.
    pub fn file_hashes(&self) -> &HashMap<String, String> {
        &self.metadata.files
    }

    /// Access node-to-file mapping.
    pub fn node_mapping(&self) -> &HashMap<String, Vec<Uuid>> {
        &self.metadata.node_mapping
    }

    /// Update node mapping from the current graph state.
    pub fn refresh_node_mapping(&mut self, graph: &CodeGraph) {
        self.metadata.node_mapping = build_node_mapping(graph);
    }
}

/// Set of file changes detected by hash comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// Newly discovered files
    pub added: Vec<String>,
    /// Files whose hash changed
    pub changed: Vec<String>,
    /// Files removed since last index
    pub deleted: Vec<String>,
}

impl ChangeSet {
    /// All paths that require graph updates (added + changed + deleted).
    pub fn affected(&self) -> Vec<String> {
        let mut paths = self.added.clone();
        paths.extend(self.changed.clone());
        paths.extend(self.deleted.clone());
        paths.sort();
        paths.dedup();
        paths
    }

    /// Whether any changes were detected.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.deleted.is_empty()
    }

    /// Total number of changed files (added + modified + deleted).
    pub fn len(&self) -> usize {
        self.added.len() + self.changed.len() + self.deleted.len()
    }
}

/// Build node-to-file mapping from graph nodes.
pub fn build_node_mapping(graph: &CodeGraph) -> HashMap<String, Vec<Uuid>> {
    let mut mapping: HashMap<String, Vec<Uuid>> = HashMap::new();
    if let Ok(nodes) = graph.backend().all_nodes() {
        for node in nodes {
            let file = if let Some(path) = node.file_path.as_deref() {
                Some(path)
            } else if matches!(node.node_type, rbuilder_graph::schema::NodeType::File) {
                Some(node.name.as_str())
            } else {
                None
            };
            if let Some(file) = file {
                mapping
                    .entry(normalize_path_str(file))
                    .or_default()
                    .push(node.id);
            }
        }
    }
    mapping
}

/// Convert an absolute or relative path to a repo-relative string.
pub fn relative_path(repo_root: &Path, path: &Path) -> Result<String> {
    let normalized = path
        .strip_prefix(repo_root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| path.to_path_buf());
    Ok(normalize_path_str(&normalized.to_string_lossy()))
}

/// Normalize path separators for consistent comparison.
pub fn normalize_path_str(path: &str) -> String {
    path.replace('\\', "/")
}

/// Resolve a relative path to an absolute path under the repo root.
pub fn resolve_path(repo_root: &Path, rel: &str) -> PathBuf {
    repo_root.join(rel)
}

/// Build a change set for explicit repo-relative paths (watch / `update --files`).
pub fn changes_for_paths(repo_root: &Path, relative_paths: &[String]) -> Result<ChangeSet> {
    let tracker = FileTracker::load(repo_root).unwrap_or_else(|_| FileTracker::new(repo_root));
    let known = tracker.file_hashes();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();

    for rel in relative_paths {
        let normalized = normalize_path_str(rel);
        let path = resolve_path(repo_root, &normalized);
        if path.exists() {
            if known.contains_key(&normalized) {
                changed.push(normalized);
            } else {
                added.push(normalized);
            }
        } else if known.contains_key(&normalized) {
            deleted.push(normalized);
        }
    }

    Ok(ChangeSet {
        added,
        changed,
        deleted,
    })
}

/// Get changed files from git since a commit ref.
pub fn git_changed_files(repo_root: &Path, since: &str) -> Result<Vec<PathBuf>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", since, "HEAD"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| Error::Other(format!("Failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!("git diff failed: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| repo_root.join(line))
        .collect();

    Ok(files)
}

fn current_git_commit(repo_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if commit.is_empty() {
        None
    } else {
        Some(commit)
    }
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_change_detection() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("src/main.rs");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "fn main() {}\n").unwrap();

        let mut tracker = FileTracker::new(temp.path());
        let files = vec![file.clone()];
        let graph = CodeGraph::new();
        tracker.index_files(&files, &graph).unwrap();

        fs::write(&file, "fn main() { println!(\"hi\"); }\n").unwrap();
        let changes = tracker.detect_changes(&files).unwrap();

        assert_eq!(changes.changed.len(), 1);
        assert!(changes.changed[0].ends_with("src/main.rs"));
    }

    #[test]
    fn test_detect_added_and_deleted_files() {
        let temp = TempDir::new().unwrap();
        let main = temp.path().join("main.rs");
        let extra = temp.path().join("extra.rs");
        fs::write(&main, "fn main() {}\n").unwrap();

        let mut tracker = FileTracker::new(temp.path());
        tracker
            .index_files(std::slice::from_ref(&main), &CodeGraph::new())
            .unwrap();

        fs::write(&extra, "fn extra() {}\n").unwrap();
        fs::remove_file(&main).unwrap();

        let changes = tracker
            .detect_changes(std::slice::from_ref(&extra))
            .unwrap();
        assert_eq!(changes.added.len(), 1);
        assert_eq!(changes.deleted.len(), 1);
    }

    #[test]
    fn test_node_mapping() {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let mut node = Node::new(NodeType::Function, "hello".to_string());
        node.file_path = Some("src/lib.rs".to_string());
        backend.insert_node(node).unwrap();

        let mapping = build_node_mapping(&graph);
        assert!(mapping.contains_key("src/lib.rs"));
        assert_eq!(mapping["src/lib.rs"].len(), 1);
    }

    #[test]
    fn test_save_and_load() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("lib.rs");
        fs::write(&file, "fn hello() {}\n").unwrap();

        let mut tracker = FileTracker::new(temp.path());
        tracker.index_files(&[file], &CodeGraph::new()).unwrap();
        tracker.save().unwrap();

        let loaded = FileTracker::load(temp.path()).unwrap();
        assert_eq!(loaded.file_hashes().len(), 1);
    }
}
