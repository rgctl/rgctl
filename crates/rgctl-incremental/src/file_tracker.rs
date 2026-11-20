//! File change tracking
//!
//! Task 5.1.1: Track file hashes to detect changes

use rayon::prelude::*;
use rgctl_error::{Error, Result};
use rgctl_graph::code_graph::{CodeGraph, GRAPH_DIR};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Filename for persisted file hash metadata.
pub const FILE_HASHES_FILE: &str = "file_hashes.json";

/// Persisted hash metadata for incremental indexing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileHashMetadata {
    /// rgctl version that created this metadata
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

    /// Load persisted hash metadata from `.rgctl/file_hashes.json`.
    pub fn load(repo_root: impl AsRef<Path>) -> Result<Self> {
        let repo_root = repo_root.as_ref().to_path_buf();
        let path = repo_root.join(GRAPH_DIR).join(FILE_HASHES_FILE);
        if !path.exists() {
            return Ok(Self::new(repo_root));
        }

        let bytes = std::fs::read(path)?;
        let metadata: FileHashMetadata =
            serde_json::from_slice(&bytes).map_err(|e| Error::SerdeError(e.to_string()))?;

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
        let mapping = build_node_mapping(graph);
        self.index_files_with_mapping(files, mapping, None)
    }

    /// Index files using a precomputed node→file mapping (cold discover path).
    ///
    /// When `precomputed_hashes` is provided, keys are absolute path strings (as from
    /// extract workers). Missing entries fall back to parallel BLAKE3 hashing.
    pub fn index_files_with_mapping(
        &mut self,
        files: &[PathBuf],
        node_mapping: HashMap<String, Vec<Uuid>>,
        precomputed_hashes: Option<&HashMap<String, String>>,
    ) -> Result<()> {
        self.metadata.files.clear();
        self.metadata.node_mapping = node_mapping;

        let pairs: Vec<Result<(String, String)>> = files
            .par_iter()
            .map(|file| {
                let rel = relative_path(&self.repo_root, file)?;
                let hash =
                    match precomputed_hashes.and_then(|m| m.get(file.to_string_lossy().as_ref())) {
                        Some(existing) => existing.clone(),
                        None => Self::hash_file(file)?,
                    };
                Ok((rel, hash))
            })
            .collect();

        self.metadata.files.reserve(pairs.len());
        for pair in pairs {
            let (rel, hash) = pair?;
            self.metadata.files.insert(rel, hash);
        }

        self.metadata.indexed_at = chrono_lite_now();
        self.metadata.last_commit = current_git_commit(&self.repo_root);
        self.metadata.version = env!("CARGO_PKG_VERSION").to_string();
        Ok(())
    }

    /// Patch only changed/added/deleted entries in `O(|delta|)` without re-hashing
    /// untouched repository files or scanning `all_nodes()`.
    pub fn apply_delta_update(
        &mut self,
        deleted_paths: &[String],
        extract_paths: &[String],
        new_nodes: &[rgctl_graph::schema::Node],
        worker_hashes: &HashMap<String, String>,
        precomputed_rel_hashes: &FxHashMap<String, String>,
    ) -> Result<()> {
        for rel in deleted_paths.iter().chain(extract_paths.iter()) {
            let norm_rel = normalize_path_str(rel);
            let abs = resolve_path(&self.repo_root, &norm_rel);
            let norm_abs = normalize_path_str(&abs.to_string_lossy());
            self.metadata.files.remove(&norm_rel);
            self.metadata.node_mapping.remove(&norm_rel);
            self.metadata.node_mapping.remove(&norm_abs);
        }

        let updated_hashes: Vec<Result<Option<(String, String)>>> = extract_paths
            .par_iter()
            .map(|rel| {
                let norm_rel = normalize_path_str(rel);
                if let Some(hash) = precomputed_rel_hashes.get(&norm_rel) {
                    return Ok(Some((norm_rel, hash.clone())));
                }
                let abs = resolve_path(&self.repo_root, &norm_rel);
                if let Some(hash) = worker_hashes.get(abs.to_string_lossy().as_ref()) {
                    return Ok(Some((norm_rel, hash.clone())));
                }
                if !abs.is_file() {
                    return Ok(None);
                }
                Ok(Some((norm_rel, Self::hash_file(&abs)?)))
            })
            .collect();

        for item in updated_hashes {
            if let Some((rel, hash)) = item? {
                self.metadata.files.insert(rel, hash);
            }
        }

        let mut pairs: Vec<(String, Uuid)> = Vec::with_capacity(new_nodes.len());
        for node in new_nodes {
            if let Some(path) = node_file_path(node) {
                pairs.push((normalize_path_str(path), node.id));
            }
        }
        pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (path, ids) in group_sorted_node_paths(pairs) {
            self.metadata.node_mapping.insert(path, ids);
        }

        self.metadata.indexed_at = chrono_lite_now();
        self.metadata.last_commit = current_git_commit(&self.repo_root);
        self.metadata.version = env!("CARGO_PKG_VERSION").to_string();
        Ok(())
    }

    /// Compare current file hashes against stored metadata.
    pub fn detect_changes(&self, files: &[PathBuf]) -> Result<ChangeSet> {
        let (changes, _) = self.detect_changes_with_hashes(files)?;
        Ok(changes)
    }

    /// Compare current file hashes in parallel and return precomputed hashes for
    /// added/changed files so incremental updates never read+hash them twice.
    pub fn detect_changes_with_hashes(
        &self,
        files: &[PathBuf],
    ) -> Result<(ChangeSet, FxHashMap<String, String>)> {
        // Cold / empty tracker: treat every file as added without hashing.
        // Returning a non-empty ChangeSet prevents false snapshot reuse when
        // `file_hashes.json` is missing but a stale snapshot file remains.
        if self.metadata.files.is_empty() {
            let added: Vec<String> = files
                .iter()
                .filter_map(|path| relative_path(&self.repo_root, path).ok())
                .collect();
            return Ok((
                ChangeSet {
                    added,
                    changed: Vec::new(),
                    deleted: Vec::new(),
                    renamed: Vec::new(),
                },
                FxHashMap::default(),
            ));
        }

        let current: Vec<(String, String)> = files
            .par_iter()
            .filter_map(|path| {
                let rel = relative_path(&self.repo_root, path).ok()?;
                let hash = Self::hash_file(path).unwrap_or_default();
                Some((rel, hash))
            })
            .collect();

        let mut current_keys: FxHashSet<&str> =
            FxHashSet::with_capacity_and_hasher(current.len(), Default::default());
        let mut added = Vec::new();
        let mut changed = Vec::new();
        let mut dirty_hashes: FxHashMap<String, String> = FxHashMap::default();

        for (rel, hash) in &current {
            current_keys.insert(rel.as_str());
            match self.metadata.files.get(rel.as_str()) {
                None => {
                    added.push(rel.clone());
                    dirty_hashes.insert(rel.clone(), hash.clone());
                }
                Some(prev) if prev != hash => {
                    changed.push(rel.clone());
                    dirty_hashes.insert(rel.clone(), hash.clone());
                }
                _ => {}
            }
        }

        let mut deleted = Vec::new();
        for prev_rel in self.metadata.files.keys() {
            if !current_keys.contains(prev_rel.as_str()) {
                deleted.push(prev_rel.clone());
            }
        }

        Ok((
            ChangeSet {
                added,
                changed,
                deleted,
                renamed: Vec::new(),
            },
            dirty_hashes,
        ))
    }

    /// Return node IDs associated with a relative file path.
    pub fn nodes_for_file(&self, rel_path: &str) -> &[Uuid] {
        self.metadata
            .node_mapping
            .get(rel_path)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Persist hash metadata to disk atomically.
    pub fn save(&self) -> Result<PathBuf> {
        let dir = self.repo_root.join(GRAPH_DIR);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(FILE_HASHES_FILE);
        let tmp = dir.join(format!("{FILE_HASHES_FILE}.tmp"));
        let json =
            serde_json::to_vec(&self.metadata).map_err(|e| Error::SerdeError(e.to_string()))?;
        std::fs::write(&tmp, json)?;
        if cfg!(windows) && path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        std::fs::rename(&tmp, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })?;
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

/// Set of file changes detected by hash comparison or git name-status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// Newly discovered files (`A` in git name-status).
    pub added: Vec<String>,
    /// Files whose hash or content changed (`M`).
    pub changed: Vec<String>,
    /// Files removed since last index (`D`).
    pub deleted: Vec<String>,
    /// Rename or copy pairs (`R*` / `C*`): `(old_path, new_path)`.
    pub renamed: Vec<(String, String)>,
}

impl ChangeSet {
    /// All paths that require graph updates (added + changed + deleted + rename sides).
    pub fn affected(&self) -> Vec<String> {
        let mut paths = self.scoped_paths();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Repo-relative paths in the PR / watch scope (includes deleted and rename sides).
    pub fn scoped_paths(&self) -> Vec<String> {
        let mut paths = self.added.clone();
        paths.extend(self.changed.clone());
        paths.extend(self.deleted.clone());
        for (old, new) in &self.renamed {
            paths.push(old.clone());
            paths.push(new.clone());
        }
        paths
    }

    /// Paths whose base-graph nodes must be dropped before compacting the delta.
    pub fn invalidation_paths(&self) -> Vec<String> {
        let mut paths = self.changed.clone();
        paths.extend(self.deleted.clone());
        for (old, _) in &self.renamed {
            paths.push(old.clone());
        }
        paths.sort();
        paths.dedup();
        paths
    }

    /// Repo-relative paths that need (re-)extraction after invalidation.
    pub fn extract_paths(&self) -> Vec<String> {
        let mut paths = self.added.clone();
        paths.extend(self.changed.clone());
        for (_, new) in &self.renamed {
            paths.push(new.clone());
        }
        paths.sort();
        paths.dedup();
        paths
    }

    /// Whether any changes were detected.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.deleted.is_empty()
            && self.renamed.is_empty()
    }

    /// Total number of changed files (added + modified + deleted + renames).
    pub fn len(&self) -> usize {
        self.added.len() + self.changed.len() + self.deleted.len() + self.renamed.len()
    }
}

/// Merge two change sets, preferring explicit git status when paths overlap.
pub fn merge_change_sets(primary: ChangeSet, supplemental: ChangeSet) -> ChangeSet {
    let mut merged = primary;
    for path in supplemental.added {
        push_unique(&mut merged.added, path);
    }
    for path in supplemental.changed {
        push_unique(&mut merged.changed, path);
    }
    for path in supplemental.deleted {
        push_unique(&mut merged.deleted, path);
    }
    for pair in supplemental.renamed {
        if !merged.renamed.contains(&pair) {
            merged.renamed.push(pair);
        }
    }
    merged
}

fn push_unique(vec: &mut Vec<String>, path: String) {
    if !vec.contains(&path) {
        vec.push(path);
    }
}

/// Build node-to-file mapping from graph nodes.
pub fn build_node_mapping(graph: &CodeGraph) -> HashMap<String, Vec<Uuid>> {
    let mut pairs = Vec::new();
    if let Ok(nodes) = graph.backend().all_nodes() {
        pairs.reserve(nodes.len());
        for node in nodes {
            if let Some(path) = node_file_path(&node) {
                pairs.push((normalize_path_str(path), node.id));
            }
        }
    }
    pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    group_sorted_node_paths(pairs)
}

/// Group `(normalized_path, node_id)` rows that are already sorted by path.
pub fn group_sorted_node_paths(pairs: Vec<(String, Uuid)>) -> HashMap<String, Vec<Uuid>> {
    if pairs.is_empty() {
        return HashMap::new();
    }
    debug_assert!(pairs.windows(2).all(|w| w[0].0 <= w[1].0));

    let mut mapping = HashMap::new();
    let mut start = 0usize;
    for i in 1..=pairs.len() {
        if i == pairs.len() || pairs[i].0 != pairs[start].0 {
            let path = pairs[start].0.clone();
            let ids: Vec<Uuid> = pairs[start..i].iter().map(|(_, id)| *id).collect();
            mapping.insert(path, ids);
            start = i;
        }
    }
    mapping
}

fn node_file_path(node: &rgctl_graph::schema::Node) -> Option<&str> {
    if let Some(path) = node.file_path.as_deref() {
        Some(path)
    } else if matches!(node.node_type, rgctl_graph::schema::NodeType::File) {
        Some(node.name.as_str())
    } else {
        None
    }
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
        renamed: Vec::new(),
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
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::{Node, NodeType};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_changes_skips_hash_when_tracker_empty() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("main.rs");
        fs::write(&file, "fn main() {}\n").unwrap();

        let tracker = FileTracker::new(temp.path());
        let changes = tracker.detect_changes(&[file]).unwrap();
        assert_eq!(changes.added.len(), 1);
        assert!(changes.changed.is_empty());
        assert!(changes.deleted.is_empty());
    }

    #[test]
    fn test_index_files_with_precomputed_hashes() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("lib.rs");
        fs::write(&file, "fn hello() {}\n").unwrap();
        let expected = FileTracker::hash_file(&file).unwrap();

        let mut precomputed = HashMap::new();
        precomputed.insert(file.to_string_lossy().into_owned(), expected.clone());

        let mut tracker = FileTracker::new(temp.path());
        tracker
            .index_files_with_mapping(&[file.clone()], HashMap::new(), Some(&precomputed))
            .unwrap();

        assert_eq!(tracker.file_hashes().get("lib.rs"), Some(&expected));
    }

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
    fn test_group_sorted_node_paths() {
        let pairs = vec![
            ("a.rs".into(), Uuid::new_v4()),
            ("b.rs".into(), Uuid::new_v4()),
            ("b.rs".into(), Uuid::new_v4()),
            ("c.rs".into(), Uuid::new_v4()),
        ];
        let mapping = group_sorted_node_paths(pairs);
        assert_eq!(mapping.len(), 3);
        assert_eq!(mapping["a.rs"].len(), 1);
        assert_eq!(mapping["b.rs"].len(), 2);
        assert_eq!(mapping["c.rs"].len(), 1);
    }

    #[test]
    fn test_node_mapping() {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let mut node = Node::new(NodeType::Function, "hello");
        node.file_path = Some("src/lib.rs".into());
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
