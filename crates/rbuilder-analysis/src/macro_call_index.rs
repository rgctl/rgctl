//! Pre-computed blast-radius call graph index persisted at discover time.
//!
//! Avoids rebuilding the SCC engine and dense reachability bitsets on every
//! `blast-radius` CLI invocation.

use crate::blast_radius_scc::{BlastRadiusEngine, BlastRadiusResult};
use rbuilder_error::Result;
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Fingerprint of the persisted graph file for cheap cache validation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphFingerprint {
    /// Byte length of `graph.db` at index build time.
    pub file_size: u64,
    /// Node count at index build time.
    pub node_count: usize,
    /// Edge count at index build time.
    pub edge_count: usize,
    /// BLAKE3 digest of binary graph snapshot when available.
    #[serde(default)]
    pub graph_digest: Option<String>,
}

impl GraphFingerprint {
    /// Capture fingerprint from the on-disk graph and in-memory backend.
    pub fn capture(graph_db_path: &Path, backend: &MemoryBackend) -> Result<Self> {
        Self::capture_with_digest(graph_db_path, backend, None)
    }

    /// Capture fingerprint with optional binary snapshot digest.
    pub fn capture_with_digest(
        graph_db_path: &Path,
        backend: &MemoryBackend,
        graph_digest: Option<String>,
    ) -> Result<Self> {
        let meta = std::fs::metadata(graph_db_path)?;
        Ok(Self {
            file_size: meta.len(),
            node_count: backend.node_count(),
            edge_count: backend.edge_count(),
            graph_digest,
        })
    }

    /// Returns true when the graph file still matches this fingerprint.
    pub fn matches_graph_file(&self, graph_db_path: &Path) -> Result<bool> {
        let meta = std::fs::metadata(graph_db_path)?;
        Ok(meta.len() == self.file_size)
    }

    /// Returns true when the repository graph matches this fingerprint.
    pub fn matches_repo(&self, repo_root: &Path) -> Result<bool> {
        let snapshot_path = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE);
        if let Some(ref digest) = self.graph_digest {
            if snapshot_path.exists() {
                let mmap = rbuilder_graph::snapshot::MmappedGraphSnapshot::open(&snapshot_path)?;
                return Ok(mmap.content_digest() == digest);
            }
        }
        let graph_db = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(rbuilder_graph::code_graph::GRAPH_FILE);
        if !graph_db.exists() {
            return Ok(false);
        }
        if !self.matches_graph_file(&graph_db)? {
            return Ok(false);
        }
        Ok(true)
    }
}

/// Per-node symbol context for FQN disambiguation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolContext {
    /// Simple class or namespace name.
    pub class_name: Option<String>,
    /// Source file path.
    pub file_path: String,
}

/// Cached blast-radius metrics for a single function node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCallIndexEntry {
    /// Impact score (0–100).
    pub score: f64,
    /// Immediate caller node IDs.
    pub direct_caller_ids: Vec<Uuid>,
    /// Transitive caller node IDs (excluding self).
    pub impact_zone_ids: Vec<Uuid>,
    /// Resolved direct caller names (for instant CLI output).
    #[serde(default)]
    pub direct_caller_names: Vec<String>,
    /// Resolved impact-zone function names (for instant CLI output).
    #[serde(default)]
    pub impact_function_names: Vec<String>,
    /// SCC identifier.
    pub scc_id: usize,
    /// Number of nodes in the same SCC.
    pub scc_size: usize,
}

/// Minimized macro-call index keyed by function UUID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCallIndex {
    /// Graph file fingerprint for cache invalidation without loading the graph.
    #[serde(default)]
    pub graph_fingerprint: GraphFingerprint,
    /// Node count fingerprint for cache invalidation.
    pub node_count: usize,
    /// Edge count fingerprint for cache invalidation.
    pub edge_count: usize,
    /// Function name → UUID(s) for symbol resolution without loading the graph.
    #[serde(default)]
    pub name_index: HashMap<String, Vec<Uuid>>,
    /// Pre-computed blast-radius results keyed by function UUID.
    pub entries: HashMap<Uuid, MacroCallIndexEntry>,
    /// Class/file context keyed by function UUID.
    #[serde(default)]
    pub symbol_context: HashMap<Uuid, SymbolContext>,
}

impl MacroCallIndex {
    /// Default persistence path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".rbuilder/macro_call_index.bin")
    }

    fn symbol_context_from_node(node: &rbuilder_graph::schema::Node) -> SymbolContext {
        SymbolContext {
            class_name: crate::macro_call_lookup::class_name_from_node(node),
            file_path: node.file_path.clone().unwrap_or_default(),
        }
    }

    fn node_name(backend: &MemoryBackend, id: Uuid) -> Option<String> {
        backend.get_node(id).ok().flatten().map(|n| n.name.clone())
    }

    fn entry_from_result(backend: &MemoryBackend, result: &BlastRadiusResult) -> MacroCallIndexEntry {
        let direct_caller_names: Vec<String> = result
            .direct_caller_ids
            .iter()
            .filter_map(|id| Self::node_name(backend, *id))
            .collect();

        let impact_ids =
            BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids)
                .unwrap_or_default();
        let mut impact_function_names: Vec<String> = impact_ids
            .iter()
            .filter_map(|id| Self::node_name(backend, *id))
            .collect();
        impact_function_names.sort();

        MacroCallIndexEntry {
            score: result.score,
            direct_caller_ids: result.direct_caller_ids.clone(),
            impact_zone_ids: result.impact_zone_ids.clone(),
            direct_caller_names,
            impact_function_names,
            scc_id: result.scc_id,
            scc_size: result.scc_size,
        }
    }

    /// Build an index from discover-time blast-radius results.
    pub fn from_results(
        graph_db_path: &Path,
        backend: &MemoryBackend,
        results: &[(Uuid, BlastRadiusResult)],
        graph_digest: Option<String>,
    ) -> Result<Self> {
        let mut entries = HashMap::with_capacity(results.len());
        let mut name_index: HashMap<String, Vec<Uuid>> = HashMap::new();
        let mut symbol_context = HashMap::with_capacity(results.len());

        for (id, result) in results {
            entries.insert(*id, Self::entry_from_result(backend, result));
            if let Some(node) = backend.get_node(*id).ok().flatten() {
                symbol_context.insert(*id, Self::symbol_context_from_node(&node));
                name_index.entry(node.name.clone()).or_default().push(*id);
            }
        }

        Ok(Self {
            graph_fingerprint: GraphFingerprint::capture_with_digest(
                graph_db_path,
                backend,
                graph_digest,
            )?,
            node_count: backend.node_count(),
            edge_count: backend.edge_count(),
            name_index,
            entries,
            symbol_context,
        })
    }

    /// Returns true when the index matches the current in-memory graph topology.
    pub fn is_valid_for(&self, backend: &MemoryBackend) -> bool {
        self.node_count == backend.node_count() && self.edge_count == backend.edge_count()
    }

    /// Returns true when the on-disk graph matches the index fingerprint.
    pub fn is_valid_for_graph_file(&self, graph_db_path: &Path) -> Result<bool> {
        if !self.graph_fingerprint.matches_graph_file(graph_db_path)? {
            return Ok(false);
        }
        Ok(true)
    }

    /// Returns true when the repository graph matches the index fingerprint.
    pub fn is_valid_for_repo(&self, repo_root: &Path) -> Result<bool> {
        self.graph_fingerprint.matches_repo(repo_root)
    }

    /// Resolve a symbol name to a unique cached entry without loading the graph.
    pub fn lookup_by_name(&self, symbol: &str) -> Result<Option<(&Uuid, &MacroCallIndexEntry)>> {
        let Some(ids) = self.name_index.get(symbol) else {
            return Ok(None);
        };
        match ids.len() {
            0 => Ok(None),
            1 => Ok(Some((&ids[0], self.entries.get(&ids[0]).expect("entry")))),
            count => Err(rbuilder_error::Error::AmbiguousSymbol {
                name: symbol.to_string(),
                count,
            }),
        }
    }

    /// Build candidate records for FQN disambiguation.
    pub fn get_candidates(&self, target_name: &str) -> Vec<crate::macro_call_lookup::MacroIndexEntry> {
        let Some(ids) = self.name_index.get(target_name) else {
            return vec![];
        };
        ids.iter()
            .filter_map(|id| {
                let entry = self.entries.get(id)?;
                let ctx = self.symbol_context.get(id)?;
                Some(crate::macro_call_lookup::MacroIndexEntry {
                    id: *id,
                    symbol_name: target_name.to_string(),
                    class_name: ctx.class_name.clone(),
                    file_path: ctx.file_path.clone(),
                    score: entry.score,
                    direct_callers: entry.direct_caller_names.clone(),
                    impact_zone: entry.impact_function_names.clone(),
                })
            })
            .collect()
    }

    /// Build all candidate rows for SQLite persistence.
    pub fn all_candidate_rows(&self) -> Vec<crate::macro_call_lookup::MacroIndexEntry> {
        self.name_index
            .iter()
            .flat_map(|(name, ids)| {
                ids.iter().filter_map(move |id| {
                    let entry = self.entries.get(id)?;
                    let ctx = self.symbol_context.get(id)?;
                    Some(crate::macro_call_lookup::MacroIndexEntry {
                        id: *id,
                        symbol_name: name.clone(),
                        class_name: ctx.class_name.clone(),
                        file_path: ctx.file_path.clone(),
                        score: entry.score,
                        direct_callers: entry.direct_caller_names.clone(),
                        impact_zone: entry.impact_function_names.clone(),
                    })
                })
            })
            .collect()
    }

    /// Lookup a cached entry by function UUID.
    pub fn get(&self, func_id: Uuid) -> Option<&MacroCallIndexEntry> {
        self.entries.get(&func_id)
    }

    /// Reconstruct a [`BlastRadiusResult`] from a cached entry.
    pub fn to_blast_result(entry: &MacroCallIndexEntry, func_id: Uuid) -> BlastRadiusResult {
        BlastRadiusResult {
            symbol_id: func_id,
            direct_caller_ids: entry.direct_caller_ids.clone(),
            impact_zone_ids: entry.impact_zone_ids.clone(),
            score: entry.score,
            scc_id: entry.scc_id,
            scc_size: entry.scc_size,
        }
    }

    /// Build SQLite lookup rows for uniquely-named symbols only.
    pub fn unique_lookup_rows(&self) -> Vec<crate::macro_call_lookup::MacroCallLookupRow> {
        self.name_index
            .iter()
            .filter(|(_, ids)| ids.len() == 1)
            .filter_map(|(name, ids)| {
                self.entries.get(&ids[0]).map(|entry| {
                    crate::macro_call_lookup::MacroCallLookupRow {
                        symbol_name: name.clone(),
                        score: entry.score,
                        direct_callers: entry.direct_caller_names.clone(),
                        impact_zone: entry.impact_function_names.clone(),
                    }
                })
            })
            .collect()
    }

    /// Persist the index to disk (bincode).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        bincode::serialize_into(file, self).map_err(|e| {
            rbuilder_error::Error::SerdeError(format!("Failed to serialize macro_call_index: {e}"))
        })?;
        Ok(())
    }

    /// Load an index from disk. Returns `Ok(None)` when the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let file = std::fs::File::open(path)?;
        let index = bincode::deserialize_from(file).map_err(|e| {
            rbuilder_error::Error::SerdeError(format!("Failed to deserialize macro_call_index: {e}"))
        })?;
        Ok(Some(index))
    }

    /// Rebuild and persist the index from a loaded graph (e.g. after upgrading rBuilder).
    pub fn rebuild_and_save(
        repo_root: &Path,
        graph_db_path: &Path,
        backend: &MemoryBackend,
        function_ids: &[Uuid],
    ) -> Result<Self> {
        let engine = BlastRadiusEngine::build(backend)?;
        let mut results = Vec::with_capacity(function_ids.len());
        for &func_id in function_ids {
            if let Ok(result) = engine.analyze(func_id) {
                results.push((func_id, result));
            }
        }
        let digest = rbuilder_graph::snapshot::MmappedGraphSnapshot::default_path(repo_root)
            .exists()
            .then(|| {
                rbuilder_graph::snapshot::MmappedGraphSnapshot::open(
                    &rbuilder_graph::snapshot::MmappedGraphSnapshot::default_path(repo_root),
                )
                .ok()
                .map(|m| m.content_digest().to_string())
            })
            .flatten()
            .or_else(|| {
                rbuilder_graph::PreparedGraphSnapshot::from_backend(backend)
                    .ok()
                    .map(|p| p.content_digest)
            });
        let index = Self::from_results(graph_db_path, backend, &results, digest)?;
        index.save(&Self::default_path(repo_root))?;
        let lookup_path = crate::macro_call_lookup::MacroCallLookupDb::default_path(repo_root);
        let rows = index.unique_lookup_rows();
        crate::macro_call_lookup::MacroCallLookupDb::replace_all(&lookup_path, &rows)?;
        crate::macro_call_lookup::MacroCallLookupDb::replace_candidates(
            &lookup_path,
            &index.all_candidate_rows(),
        )?;
        crate::macro_call_lookup::MacroCallLookupDb::write_meta_with_digest(
            &lookup_path,
            std::fs::metadata(graph_db_path).map(|m| m.len()).unwrap_or(0),
            backend.node_count(),
            backend.edge_count(),
            index.graph_fingerprint.graph_digest.as_deref(),
        )?;
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trip_save_load() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("macro_call_index.bin");
        let id = Uuid::new_v4();
        let mut entries = HashMap::new();
        entries.insert(
            id,
            MacroCallIndexEntry {
                score: 42.0,
                direct_caller_ids: vec![Uuid::new_v4()],
                impact_zone_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
                direct_caller_names: vec!["caller".into()],
                impact_function_names: vec!["impact".into()],
                scc_id: 1,
                scc_size: 2,
            },
        );
        let mut name_index = HashMap::new();
        name_index.insert("target".into(), vec![id]);
        let index = MacroCallIndex {
            graph_fingerprint: GraphFingerprint {
                file_size: 100,
                node_count: 100,
                edge_count: 200,
                graph_digest: None,
            },
            node_count: 100,
            edge_count: 200,
            name_index,
            entries,
            symbol_context: HashMap::new(),
        };
        index.save(&path).unwrap();
        let loaded = MacroCallIndex::load(&path).unwrap().unwrap();
        assert_eq!(loaded.node_count, 100);
        assert_eq!(loaded.edge_count, 200);
        assert_eq!(loaded.entries.get(&id).unwrap().score, 42.0);
        assert_eq!(loaded.lookup_by_name("target").unwrap().unwrap().1.score, 42.0);
    }
}
