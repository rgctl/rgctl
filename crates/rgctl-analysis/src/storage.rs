//! Storage and retrieval of CFG/PDG/Dominance analysis results.
//!
//! Persists per-function analysis artifacts to disk (bincode primary, JSON legacy).
//! Not graph topology.

use crate::analysis_pack::{
    ANALYSIS_PACK_INDEX_FILE, AnalysisPackWriter, SharedPackReader, SharedPackWriter,
    pack_function_ids,
};
use crate::cfg::ControlFlowGraph;
use crate::dominance::DominatorTree;
use crate::pdg::ProgramDependenceGraph;
use crate::taint::TaintFlow;
use rgctl_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Legacy name-based key for CFG/PDG archive aliases.
///
/// Not suitable for analysis caching: different owners can have identical names and bodies.
pub fn stable_function_key(file_path: &str, function_name: &str, code_hash: &str) -> String {
    format!("{file_path}\x1f{function_name}\x1f{code_hash}")
}

/// Cache key for one exact graph callable and body revision.
///
/// The node UUID includes path, owner, callable discriminator and node kind.
/// A real identity change must rebuild analysis rather than rename its payload.
pub fn analysis_cache_key(function_id: Uuid, code_hash: &str) -> String {
    format!("function-v1\x1f{function_id}\x1f{code_hash}")
}

/// Suffix for bincode per-function analysis artifacts (`{uuid}.analysis.bin`).
pub const ANALYSIS_BIN_SUFFIX: &str = ".analysis.bin";

/// Sidecar index filename (`stable_key → metadata`) for incremental CFG without `load_all()`.
pub const ANALYSIS_INDEX_FILE: &str = "analysis_index.bin";

/// Lightweight metadata for incremental CFG reuse (no CFG/PDG payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisIndexEntry {
    /// Versioned exact-function cache key (UUID plus body hash).
    pub stable_key: String,
    /// Current graph node UUID for the function.
    pub function_id: Uuid,
    /// BLAKE3 hash of the function body used for reuse.
    pub code_hash: String,
    /// Number of taint flows recorded for this function.
    pub flow_count: usize,
    /// Number of vulnerable taint flows recorded for this function.
    pub vulnerable_count: usize,
}

/// Display metadata stored at the beginning of a function analysis payload.
///
/// Field order matches the first three fields of [`FunctionAnalysis`], allowing
/// bincode prefix reads without decoding CFG, PDG, dominance or taint data.
#[derive(Debug, Deserialize)]
pub struct FunctionAnalysisMetadata {
    /// Exact graph function UUID.
    pub function_id: Uuid,
    /// Display name of the function.
    pub function_name: String,
    /// Source path of the function.
    pub file_path: String,
}

/// Analysis results for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    /// Function node ID in the graph
    pub function_id: Uuid,
    /// Function name
    pub function_name: String,
    /// Source file path
    pub file_path: String,
    /// BLAKE3 of function body (or file fallback) for incremental reuse
    #[serde(default)]
    pub code_hash: Option<String>,
    /// Control flow graph (shared with the CFG/PDG archive via [`Arc`]).
    pub cfg: Option<Arc<ControlFlowGraph>>,
    /// Program dependence graph (shared with the CFG/PDG archive via [`Arc`]).
    pub pdg: Option<Arc<ProgramDependenceGraph>>,
    /// Dominator tree
    pub dominance: Option<DominatorTree>,
    /// Taint flows (source→sink paths)
    #[serde(default)]
    pub taint: Option<Vec<TaintFlow>>,
}

impl FunctionAnalysis {
    /// Stable cache key when body hash is known.
    pub fn stable_key(&self) -> Option<String> {
        let hash = self.code_hash.as_deref()?;
        Some(analysis_cache_key(self.function_id, hash))
    }
}

/// Storage manager for analysis results.
pub struct AnalysisStorage {
    base_dir: PathBuf,
    pack_reader: SharedPackReader,
}

/// Minimal graph function metadata for aligning analysis index UUIDs after re-index.
pub struct FunctionIdSyncEntry<'a> {
    /// Current graph node UUID for the function.
    pub function_id: Uuid,
    /// Function symbol name.
    pub function_name: &'a str,
    /// Source file path containing the function.
    pub file_path: &'a str,
    /// BLAKE3 hash of the function body.
    pub code_hash: &'a str,
}

impl AnalysisStorage {
    /// Create a new storage manager rooted at the given directory.
    /// Typically `.rgctl/analysis/`
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        let base_dir = base_dir.as_ref().to_path_buf();
        Self {
            pack_reader: SharedPackReader::new(&base_dir),
            base_dir,
        }
    }

    /// Ensure the storage directory exists.
    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        Ok(())
    }

    /// Open an append-only pack writer (replaces per-function files during discover).
    pub fn open_pack_writer(&self) -> Result<SharedPackWriter> {
        Ok(Mutex::new(Some(AnalysisPackWriter::create(
            &self.base_dir,
        )?)))
    }

    /// Replace a pack while retaining exact cache-hit payloads without decoding.
    ///
    /// Open the old mmap before `create` unlinks the data file. Its mapping stays
    /// valid while records stream into the replacement; obsolete records are omitted.
    pub fn open_pack_writer_preserving(&self, retained_ids: &[Uuid]) -> Result<SharedPackWriter> {
        let reader = self.pack_reader.get()?;
        let mut writer = AnalysisPackWriter::create(&self.base_dir)?;
        if let Some(reader) = reader {
            for id in retained_ids {
                if let Some(bytes) = reader.record_bytes(*id)? {
                    writer.append(*id, bytes)?;
                }
            }
        }
        Ok(Mutex::new(Some(writer)))
    }

    /// Finish the pack writer and write the offset index.
    pub fn finish_pack(pack: SharedPackWriter) -> Result<()> {
        if let Some(writer) = pack
            .lock()
            .map_err(|e| Error::SerdeError(e.to_string()))?
            .take()
        {
            let analysis_dir = writer
                .data_path()
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf();
            writer.finish(&analysis_dir)?;
        }
        Ok(())
    }

    /// Save without index update, appending to an open pack writer.
    pub fn save_function_no_index_packed(
        &self,
        analysis: &FunctionAnalysis,
        pack: &SharedPackWriter,
    ) -> Result<()> {
        let bytes = bincode::serialize(analysis)
            .map_err(|e| Error::SerdeError(format!("analysis bincode encode: {e}")))?;
        self.save_preencoded_no_index_packed(analysis.function_id, &bytes, pack)
    }

    /// Append worker-side pre-encoded analysis bytes to an open pack writer.
    pub fn save_preencoded_no_index_packed(
        &self,
        function_id: Uuid,
        bytes: &[u8],
        pack: &SharedPackWriter,
    ) -> Result<()> {
        let mut guard = pack.lock().map_err(|e| Error::SerdeError(e.to_string()))?;
        let writer = guard
            .as_mut()
            .ok_or_else(|| Error::SerdeError("analysis pack writer not open".into()))?;
        writer.append(function_id, bytes)
    }

    fn bin_path(&self, function_id: Uuid) -> PathBuf {
        self.base_dir
            .join(format!("{function_id}{ANALYSIS_BIN_SUFFIX}"))
    }

    fn json_path(&self, function_id: Uuid) -> PathBuf {
        self.base_dir.join(format!("{function_id}.json"))
    }

    /// Save analysis for a function (bincode; removes legacy JSON for the same id).
    pub fn save_function(&self, analysis: &FunctionAnalysis) -> Result<()> {
        self.ensure_dir()?;
        let bin_path = self.bin_path(analysis.function_id);
        let bytes = bincode::serialize(analysis)
            .map_err(|e| Error::SerdeError(format!("analysis bincode encode: {e}")))?;
        fs::write(&bin_path, bytes)?;
        let json_path = self.json_path(analysis.function_id);
        if json_path.exists() {
            let _ = fs::remove_file(json_path);
        }
        self.upsert_index_entry(analysis)?;
        Ok(())
    }

    /// Save without updating the sidecar index (caller must call [`refresh_analysis_index_from_analyses`]).
    pub fn save_function_no_index(&self, analysis: &FunctionAnalysis) -> Result<()> {
        self.ensure_dir()?;
        let bin_path = self.bin_path(analysis.function_id);
        let bytes = bincode::serialize(analysis)
            .map_err(|e| Error::SerdeError(format!("analysis bincode encode: {e}")))?;
        fs::write(&bin_path, bytes)?;
        let json_path = self.json_path(analysis.function_id);
        if json_path.exists() {
            let _ = fs::remove_file(json_path);
        }
        Ok(())
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join(ANALYSIS_INDEX_FILE)
    }

    /// Load the lightweight stable-key index (no CFG/PDG deserialization).
    pub fn load_analysis_index(&self) -> Result<HashMap<String, AnalysisIndexEntry>> {
        let path = self.index_path();
        if !path.is_file() {
            return self.rebuild_analysis_index_from_disk();
        }
        let bytes = fs::read(&path)?;
        let entries: Vec<AnalysisIndexEntry> = bincode::deserialize(&bytes)
            .map_err(|e| Error::SerdeError(format!("analysis index decode: {e}")))?;
        // Keep the existing bincode layout. Upgrade legacy name-based keys using
        // their recorded UUID, never by guessing which current callable owns them.
        // Entries already lost to a legacy collision rebuild on their next miss.
        let mut upgraded = false;
        let index = entries
            .into_iter()
            .map(|mut entry| {
                let key = analysis_cache_key(entry.function_id, &entry.code_hash);
                upgraded |= entry.stable_key != key;
                entry.stable_key = key.clone();
                (key, entry)
            })
            .collect();
        if upgraded {
            self.write_analysis_index(&index)?;
        }
        Ok(index)
    }

    fn write_analysis_index(&self, entries: &HashMap<String, AnalysisIndexEntry>) -> Result<()> {
        self.ensure_dir()?;
        let mut list: Vec<_> = entries.values().cloned().collect();
        list.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));
        let bytes = bincode::serialize(&list)
            .map_err(|e| Error::SerdeError(format!("analysis index encode: {e}")))?;
        fs::write(self.index_path(), bytes)?;
        Ok(())
    }

    fn upsert_index_entry(&self, analysis: &FunctionAnalysis) -> Result<()> {
        let Some(stable_key) = analysis.stable_key() else {
            return Ok(());
        };
        let (flow_count, vulnerable_count) = analysis
            .taint
            .as_ref()
            .map(|flows| {
                let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
                (flows.len(), vulnerable)
            })
            .unwrap_or((0, 0));
        let mut index = self.load_analysis_index().unwrap_or_default();
        index.insert(
            stable_key.clone(),
            AnalysisIndexEntry {
                stable_key,
                function_id: analysis.function_id,
                code_hash: analysis.code_hash.clone().unwrap_or_default(),
                flow_count,
                vulnerable_count,
            },
        );
        self.write_analysis_index(&index)
    }

    /// Update the sidecar index from a batch of saved analyses (single write, no per-save races).
    pub fn refresh_analysis_index_from_analyses(
        &self,
        analyses: &[FunctionAnalysis],
    ) -> Result<()> {
        let entries: Vec<AnalysisIndexEntry> = analyses
            .iter()
            .filter_map(|analysis| {
                let stable_key = analysis.stable_key()?;
                let (flow_count, vulnerable_count) = analysis
                    .taint
                    .as_ref()
                    .map(|flows| {
                        let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
                        (flows.len(), vulnerable)
                    })
                    .unwrap_or((0, 0));
                Some(AnalysisIndexEntry {
                    stable_key,
                    function_id: analysis.function_id,
                    code_hash: analysis.code_hash.clone().unwrap_or_default(),
                    flow_count,
                    vulnerable_count,
                })
            })
            .collect();
        self.refresh_analysis_index_from_entries(&entries)
    }

    /// Update the sidecar index from pre-built entries (worker-side serialize path).
    pub fn refresh_analysis_index_from_entries(
        &self,
        entries: &[AnalysisIndexEntry],
    ) -> Result<()> {
        let mut index = self.load_analysis_index().unwrap_or_default();
        for entry in entries {
            let mut entry = entry.clone();
            entry.stable_key = analysis_cache_key(entry.function_id, &entry.code_hash);
            index.insert(entry.stable_key.clone(), entry);
        }
        self.write_analysis_index(&index)
    }

    /// Rebuild index by scanning on-disk artifacts (one-time migration / repair).
    pub fn rebuild_analysis_index_from_disk(&self) -> Result<HashMap<String, AnalysisIndexEntry>> {
        let mut index = HashMap::new();
        for analysis in self.load_all()? {
            let Some(stable_key) = analysis.stable_key() else {
                continue;
            };
            let (flow_count, vulnerable_count) = analysis
                .taint
                .as_ref()
                .map(|flows| {
                    let vulnerable = flows.iter().filter(|f| f.is_vulnerable()).count();
                    (flows.len(), vulnerable)
                })
                .unwrap_or((0, 0));
            index.insert(
                stable_key.clone(),
                AnalysisIndexEntry {
                    stable_key,
                    function_id: analysis.function_id,
                    code_hash: analysis.code_hash.clone().unwrap_or_default(),
                    flow_count,
                    vulnerable_count,
                },
            );
        }
        if !index.is_empty() {
            let _ = self.write_analysis_index(&index);
        }
        Ok(index)
    }

    /// Load a single analysis by stable cache key.
    pub fn load_by_stable_key(&self, stable_key: &str) -> Result<Option<FunctionAnalysis>> {
        let index = self.load_analysis_index()?;
        let Some(entry) = index.get(stable_key) else {
            return Ok(None);
        };
        self.load_function(entry.function_id)
    }

    /// Rename on-disk artifact when graph re-index assigns a new function UUID (no deserialize).
    pub fn remap_function_artifact(&self, old_id: Uuid, new_id: Uuid) -> Result<()> {
        if old_id == new_id {
            return Ok(());
        }
        let old_bin = self.bin_path(old_id);
        let new_bin = self.bin_path(new_id);
        if new_bin.exists() {
            let _ = fs::remove_file(&new_bin);
        }
        if old_bin.exists() {
            fs::rename(&old_bin, &new_bin)?;
        }
        let old_json = self.json_path(old_id);
        let new_json = self.json_path(new_id);
        if old_json.exists() {
            if new_json.exists() {
                let _ = fs::remove_file(&new_json);
            }
            let _ = fs::rename(&old_json, &new_json);
        }
        Ok(())
    }

    /// Compatibility shim: exact-identity cache entries never remap UUIDs.
    ///
    /// Callers must rebuild analyses when graph identity changes; renaming an
    /// artifact does not update UUIDs embedded in CFG/PDG and archive payloads.
    #[deprecated(
        note = "analysis cache keys now require exact function UUIDs; rebuild on ID changes"
    )]
    pub fn sync_index_function_ids(&self, _functions: &[FunctionIdSyncEntry<'_>]) -> Result<usize> {
        Ok(0)
    }

    /// Remove artifacts whose stable keys are not in `active_keys`.
    pub fn purge_stale_by_stable_keys(&self, active_keys: &HashSet<String>) -> Result<usize> {
        let index = self.load_analysis_index()?;
        let mut removed = 0usize;
        let mut next = index.clone();
        let active_ids: HashSet<_> = index
            .iter()
            .filter(|(key, _)| active_keys.contains(*key))
            .map(|(_, entry)| entry.function_id)
            .collect();
        for (key, entry) in &index {
            if active_keys.contains(key) {
                continue;
            }
            // A body edit retains the UUID: the old hash entry must not delete
            // the newly saved payload belonging to the active hash entry.
            if !active_ids.contains(&entry.function_id) {
                let bin = self.bin_path(entry.function_id);
                let json = self.json_path(entry.function_id);
                if fs::remove_file(&bin).is_ok() {
                    removed += 1;
                }
                let _ = fs::remove_file(json);
            }
            next.remove(key);
        }
        if next.len() != index.len() {
            self.write_analysis_index(&next)?;
        }
        Ok(removed)
    }

    fn load_from_path(path: &Path) -> Result<Option<FunctionAnalysis>> {
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let json = fs::read_to_string(path)?;
            let analysis = serde_json::from_str(&json)
                .map_err(|e| Error::SerdeError(format!("analysis json decode: {e}")))?;
            return Ok(Some(analysis));
        }
        if path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name.ends_with(ANALYSIS_BIN_SUFFIX))
        {
            let bytes = fs::read(path)?;
            let mut analysis = decode_function_analysis_bincode(&bytes)?;
            if let Some(pdg) = analysis.pdg.as_mut() {
                Arc::make_mut(pdg).restore_derived_indexes();
            }
            return Ok(Some(analysis));
        }
        Ok(None)
    }
}

/// Decode per-function analysis bincode across PDG layout revisions.
fn decode_function_analysis_bincode(bytes: &[u8]) -> Result<FunctionAnalysis> {
    if let Ok(analysis) = bincode::deserialize::<FunctionAnalysis>(bytes) {
        return Ok(analysis);
    }
    if let Ok(legacy) = bincode::deserialize::<FunctionAnalysisPdgFour>(bytes) {
        return Ok(legacy.into_analysis());
    }
    if let Ok(legacy) = bincode::deserialize::<FunctionAnalysisPdgThree>(bytes) {
        return Ok(legacy.into_analysis());
    }
    bincode::deserialize(bytes)
        .map_err(|e| Error::SerdeError(format!("analysis bincode decode: {e}")))
}

#[derive(Deserialize)]
struct FunctionAnalysisPdgFour {
    function_id: Uuid,
    function_name: String,
    file_path: String,
    #[serde(default)]
    code_hash: Option<String>,
    cfg: Option<ControlFlowGraph>,
    pdg: Option<PdgStoredFour>,
    dominance: Option<DominatorTree>,
    #[serde(default)]
    taint: Option<Vec<TaintFlow>>,
}

#[derive(Deserialize)]
struct FunctionAnalysisPdgThree {
    function_id: Uuid,
    function_name: String,
    file_path: String,
    #[serde(default)]
    code_hash: Option<String>,
    cfg: Option<ControlFlowGraph>,
    pdg: Option<PdgStoredThree>,
    dominance: Option<DominatorTree>,
    #[serde(default)]
    taint: Option<Vec<TaintFlow>>,
}

#[derive(Deserialize)]
struct PdgStoredFour {
    nodes: HashMap<crate::pdg::PdgNodeId, crate::pdg::PdgNode>,
    data_deps: Vec<crate::pdg::DataDependency>,
    control_deps: Vec<crate::pdg::ControlDependency>,
    #[serde(default)]
    block_nodes: HashMap<crate::cfg::BlockId, Vec<crate::pdg::PdgNodeId>>,
}

#[derive(Deserialize)]
struct PdgStoredThree {
    nodes: HashMap<crate::pdg::PdgNodeId, crate::pdg::PdgNode>,
    data_deps: Vec<crate::pdg::DataDependency>,
    control_deps: Vec<crate::pdg::ControlDependency>,
}

impl FunctionAnalysisPdgFour {
    fn into_analysis(self) -> FunctionAnalysis {
        FunctionAnalysis {
            function_id: self.function_id,
            function_name: self.function_name,
            file_path: self.file_path,
            code_hash: self.code_hash,
            cfg: self.cfg.map(Arc::new),
            pdg: self.pdg.map(|pdg| {
                let mut graph = ProgramDependenceGraph::from_parts(
                    pdg.nodes,
                    pdg.data_deps,
                    pdg.control_deps,
                    pdg.block_nodes,
                    HashMap::new(),
                    HashMap::new(),
                );
                graph.restore_derived_indexes();
                Arc::new(graph)
            }),
            dominance: self.dominance,
            taint: self.taint,
        }
    }
}

impl FunctionAnalysisPdgThree {
    fn into_analysis(self) -> FunctionAnalysis {
        FunctionAnalysis {
            function_id: self.function_id,
            function_name: self.function_name,
            file_path: self.file_path,
            code_hash: self.code_hash,
            cfg: self.cfg.map(Arc::new),
            pdg: self.pdg.map(|pdg| {
                let mut graph = ProgramDependenceGraph::from_parts(
                    pdg.nodes,
                    pdg.data_deps,
                    pdg.control_deps,
                    HashMap::new(),
                    HashMap::new(),
                    HashMap::new(),
                );
                graph.restore_derived_indexes();
                Arc::new(graph)
            }),
            dominance: self.dominance,
            taint: self.taint,
        }
    }
}

impl AnalysisStorage {
    fn analysis_paths(&self) -> Result<Vec<PathBuf>> {
        if self.base_dir.join(ANALYSIS_PACK_INDEX_FILE).is_file() {
            if let Some(reader) = self.pack_reader.get()? {
                return Ok(reader
                    .function_ids()
                    .into_iter()
                    .map(|id| self.bin_path(id))
                    .collect());
            }
            let ids = pack_function_ids(&self.base_dir)?;
            return Ok(ids.into_iter().map(|id| self.bin_path(id)).collect());
        }
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let path = entry?.path();
            if analysis_function_id(&path).is_some() {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    /// Read display metadata without deserializing graph-analysis payloads.
    ///
    /// Cache keys are opaque identity/hash keys, not encoded names or paths.
    /// Pack and binary records support prefix reads; legacy JSON skips unknown
    /// payload fields through serde's normal struct decoding.
    pub fn load_function_metadata(
        &self,
        function_id: Uuid,
    ) -> Result<Option<FunctionAnalysisMetadata>> {
        let metadata: FunctionAnalysisMetadata = if let Some(reader) = self.pack_reader.get()?
            && let Some(bytes) = reader.record_bytes(function_id)?
        {
            bincode::deserialize_from(bytes)
                .map_err(|e| Error::SerdeError(format!("analysis metadata decode: {e}")))?
        } else if self.bin_path(function_id).is_file() {
            let file = std::io::BufReader::new(fs::File::open(self.bin_path(function_id))?);
            bincode::deserialize_from(file)
                .map_err(|e| Error::SerdeError(format!("analysis metadata decode: {e}")))?
        } else if self.json_path(function_id).is_file() {
            let file = std::io::BufReader::new(fs::File::open(self.json_path(function_id))?);
            serde_json::from_reader(file)
                .map_err(|e| Error::SerdeError(format!("analysis metadata JSON decode: {e}")))?
        } else {
            return Ok(None);
        };
        if metadata.function_id != function_id {
            return Err(Error::SerdeError(
                "analysis metadata function UUID mismatch".into(),
            ));
        }
        Ok(Some(metadata))
    }

    /// Load analysis for a function by ID (pack preferred, then per-function bincode, JSON fallback).
    pub fn load_function(&self, function_id: Uuid) -> Result<Option<FunctionAnalysis>> {
        if let Some(reader) = self.pack_reader.get()?
            && let Some(bytes) = reader.record_bytes(function_id)?
        {
            let mut analysis = decode_function_analysis_bincode(bytes)?;
            if let Some(pdg) = analysis.pdg.as_mut() {
                Arc::make_mut(pdg).restore_derived_indexes();
            }
            return Ok(Some(analysis));
        }
        let bin_path = self.bin_path(function_id);
        if bin_path.exists() {
            return Self::load_from_path(&bin_path);
        }
        let json_path = self.json_path(function_id);
        if json_path.exists() {
            return Self::load_from_path(&json_path);
        }
        Ok(None)
    }

    /// Load all function analyses (pack preferred, then per-function files).
    pub fn load_all(&self) -> Result<Vec<FunctionAnalysis>> {
        if let Some(reader) = self.pack_reader.get()?
            && !reader.is_empty()
        {
            let mut analyses = Vec::with_capacity(reader.len());
            for id in reader.function_ids() {
                if let Some(bytes) = reader.record_bytes(id)?
                    && let Ok(mut analysis) = decode_function_analysis_bincode(bytes)
                {
                    if let Some(pdg) = analysis.pdg.as_mut() {
                        Arc::make_mut(pdg).restore_derived_indexes();
                    }
                    analyses.push(analysis);
                }
            }
            return Ok(analyses);
        }
        let paths = self.analysis_paths()?;
        let mut by_id: HashMap<Uuid, (bool, FunctionAnalysis)> = HashMap::new();
        for path in paths {
            let Some(id) = analysis_function_id(&path) else {
                continue;
            };
            let Some(analysis) = Self::load_from_path(&path)? else {
                continue;
            };
            let is_bin = path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.ends_with(ANALYSIS_BIN_SUFFIX));
            match by_id.get(&id) {
                Some((true, _)) if !is_bin => continue,
                Some((false, _)) if is_bin => {
                    by_id.insert(id, (true, analysis));
                }
                None => {
                    by_id.insert(id, (is_bin, analysis));
                }
                _ => {}
            }
        }
        Ok(by_id.into_values().map(|(_, a)| a).collect())
    }

    /// Index persisted analyses by stable function key for incremental CFG reuse.
    pub fn build_stable_key_index(&self) -> Result<HashMap<String, FunctionAnalysis>> {
        let mut index = HashMap::new();
        for analysis in self.load_all()? {
            if let Some(key) = analysis.stable_key() {
                index.insert(key, analysis);
            }
        }
        Ok(index)
    }

    /// Legacy UUID-based orphan purge (prefer [`purge_stale_by_stable_keys`]).
    pub fn purge_orphans(&self, active_ids: &HashSet<Uuid>) -> Result<usize> {
        if !self.base_dir.exists() {
            return Ok(0);
        }
        let mut removed = 0usize;
        for path in self.analysis_paths()? {
            let Some(id) = analysis_function_id(&path) else {
                continue;
            };
            if !active_ids.contains(&id) && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Export all analyses to a single consolidated JSON file.
    pub fn export_all(&self, output_path: impl AsRef<Path>) -> Result<()> {
        let analyses = self.load_all()?;
        let json = serde_json::to_string_pretty(&analyses)?;
        fs::write(output_path, json)?;
        Ok(())
    }

    /// Import analyses from a consolidated JSON file.
    pub fn import_all(&self, input_path: impl AsRef<Path>) -> Result<usize> {
        let json = fs::read_to_string(input_path)?;
        let analyses: Vec<FunctionAnalysis> = serde_json::from_str(&json)?;
        let count = analyses.len();
        for analysis in analyses {
            self.save_function(&analysis)?;
        }
        Ok(count)
    }

    /// Build an index mapping function IDs to analysis file paths.
    pub fn index(&self) -> Result<HashMap<Uuid, PathBuf>> {
        let mut index = HashMap::new();
        for path in self.analysis_paths()? {
            if let Some(id) = analysis_function_id(&path) {
                index
                    .entry(id)
                    .and_modify(|existing| {
                        if path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .is_some_and(|name| name.ends_with(ANALYSIS_BIN_SUFFIX))
                        {
                            *existing = path.clone();
                        }
                    })
                    .or_insert(path);
            }
        }
        Ok(index)
    }

    /// Delete all analysis files.
    pub fn clear(&self) -> Result<()> {
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir)?;
        }
        Ok(())
    }
}

fn analysis_function_id(path: &Path) -> Option<Uuid> {
    let name = path.file_name()?.to_str()?;
    if name == "cfg_pdg.archive.bin" {
        return None;
    }
    if name.ends_with(ANALYSIS_BIN_SUFFIX) {
        let stem = name.strip_suffix(ANALYSIS_BIN_SUFFIX)?;
        return Uuid::parse_str(stem).ok();
    }
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        return Uuid::parse_str(path.file_stem()?.to_str()?).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_analysis(id: Uuid, name: &str, hash: &str) -> FunctionAnalysis {
        FunctionAnalysis {
            function_id: id,
            function_name: name.into(),
            file_path: "src/Foo.java".into(),
            code_hash: Some(hash.into()),
            cfg: None,
            pdg: None,
            dominance: None,
            taint: None,
        }
    }

    #[test]
    fn test_save_and_load_function_with_cfg_pdg() {
        use crate::cfg_builder::build_cfg_for_function;
        use crate::pdg::ProgramDependenceGraph;

        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        let id = Uuid::new_v4();
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        storage
            .save_function(&FunctionAnalysis {
                function_id: id,
                function_name: "add".into(),
                file_path: "src/lib.rs".into(),
                code_hash: Some("hash1".into()),
                cfg: Some(Arc::new(cfg.clone())),
                pdg: Some(Arc::new(pdg.clone())),
                dominance: None,
                taint: None,
            })
            .unwrap();
        let cfg_bytes = bincode::serialize(&cfg).expect("cfg serialize");
        let _: ControlFlowGraph = bincode::deserialize(&cfg_bytes).expect("cfg deserialize");
        let pdg_bytes = bincode::serialize(&pdg).expect("pdg serialize");
        let _: ProgramDependenceGraph = bincode::deserialize(&pdg_bytes).expect("pdg deserialize");
        let path = storage.bin_path(id);
        let size = fs::metadata(&path).unwrap().len();
        assert!(size > 0, "analysis file should not be empty");
        let loaded = storage
            .load_function(id)
            .unwrap()
            .expect("cfg/pdg analysis should round-trip");
        assert!(loaded.cfg.is_some());
        assert!(loaded.pdg.is_some());
    }

    #[test]
    fn test_metasfresh_analysis_load_sample() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../example/metasfresh-4.9.8b");
        let analysis_dir = repo.join(".rgctl/analysis");
        if !analysis_dir.is_dir() {
            return;
        }
        // Legacy per-file archives used UUID block ids; skip until corpus is re-discovered.
        if analysis_dir.join("analysis.pack_index.bin").is_file() {
            let storage = AnalysisStorage::new(&analysis_dir);
            let index = storage.load_analysis_index().expect("index");
            let mut ok = 0usize;
            let mut fail = 0usize;
            let mut has_cfg = 0usize;
            for entry in index.values().take(5000) {
                match storage.load_function(entry.function_id) {
                    Ok(Some(a)) => {
                        ok += 1;
                        if a.cfg.is_some() && a.pdg.is_some() {
                            has_cfg += 1;
                        }
                    }
                    _ => fail += 1,
                }
            }
            eprintln!(
                "index_total={} sampled ok={} fail={} with_cfg_pdg={}",
                index.len(),
                ok,
                fail,
                has_cfg
            );
            assert!(
                has_cfg > 4000,
                "most cached analyses should load with cfg/pdg after pack migration"
            );
        }
    }

    #[test]
    fn test_load_metasfresh_analysis_roundtrip() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../example/metasfresh-4.9.8b");
        let analysis_dir = repo.join(".rgctl/analysis");
        if !analysis_dir.is_dir() {
            return;
        }
        if !analysis_dir.join("analysis.pack_index.bin").is_file() {
            return;
        }
        let storage = AnalysisStorage::new(&analysis_dir);
        let index = storage.load_analysis_index().expect("index");
        let Some(entry) = index.values().next() else {
            return;
        };
        let loaded = storage
            .load_function(entry.function_id)
            .expect("load")
            .expect("analysis present");
        assert!(loaded.cfg.is_some(), "cfg should deserialize");
        assert!(loaded.pdg.is_some(), "pdg should deserialize");
    }

    #[test]
    fn test_save_and_load_function_bincode() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        let id = Uuid::new_v4();
        let analysis = sample_analysis(id, "foo", "abc");

        storage.save_function(&analysis).unwrap();
        assert!(storage.bin_path(id).is_file());
        let loaded = storage.load_function(id).unwrap().unwrap();
        assert_eq!(loaded.function_name, "foo");
        assert_eq!(loaded.code_hash.as_deref(), Some("abc"));
    }

    #[test]
    fn test_load_json_legacy_fallback() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        storage.ensure_dir().unwrap();
        let id = Uuid::new_v4();
        let analysis = FunctionAnalysis {
            function_id: id,
            function_name: "legacy".into(),
            file_path: "src/Legacy.java".into(),
            code_hash: None,
            cfg: None,
            pdg: None,
            dominance: None,
            taint: None,
        };
        let json = serde_json::to_string(&analysis).unwrap();
        fs::write(storage.json_path(id), json).unwrap();

        let loaded = storage.load_function(id).unwrap().unwrap();
        assert_eq!(loaded.function_name, "legacy");
        let metadata = storage.load_function_metadata(id).unwrap().unwrap();
        assert_eq!(metadata.function_name, "legacy");
        assert_eq!(metadata.file_path, "src/Legacy.java");
    }

    #[test]
    fn exact_identity_index_preserves_identical_names_and_bodies() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        let first = sample_analysis(Uuid::new_v4(), "score", "same");
        let second = sample_analysis(Uuid::new_v4(), "score", "same");
        storage.save_function(&first).unwrap();
        storage.save_function(&second).unwrap();
        let reloaded = AnalysisStorage::new(tmp.path());
        assert_eq!(reloaded.load_analysis_index().unwrap().len(), 2);
        for analysis in [&first, &second] {
            let loaded = reloaded
                .load_by_stable_key(&analysis.stable_key().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(loaded.function_id, analysis.function_id);
        }
        fs::remove_file(storage.index_path()).unwrap();
        assert_eq!(storage.load_analysis_index().unwrap().len(), 2);
    }

    #[test]
    fn legacy_index_upgrade_preserves_recorded_identity_only() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        storage.ensure_dir().unwrap();
        let id = Uuid::new_v4();
        let entry = AnalysisIndexEntry {
            stable_key: stable_function_key("Library.java", "score", "same"),
            function_id: id,
            code_hash: "same".into(),
            flow_count: 3,
            vulnerable_count: 1,
        };
        fs::write(
            storage.index_path(),
            bincode::serialize(&vec![entry]).unwrap(),
        )
        .unwrap();
        let index = storage.load_analysis_index().unwrap();
        let key = analysis_cache_key(id, "same");
        assert_eq!(index.len(), 1);
        assert_eq!(index[&key].function_id, id);
        assert_eq!(index[&key].flow_count, 3);
        assert_eq!(index[&key].vulnerable_count, 1);
        assert!(!index.contains_key(&analysis_cache_key(Uuid::new_v4(), "same")));
        let persisted: Vec<AnalysisIndexEntry> =
            bincode::deserialize(&fs::read(storage.index_path()).unwrap()).unwrap();
        assert_eq!(persisted[0].stable_key, key);
    }

    #[test]
    fn stale_hash_purge_keeps_current_payload_for_same_identity() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        let id = Uuid::new_v4();
        storage
            .save_function(&sample_analysis(id, "score", "before"))
            .unwrap();
        storage
            .save_function(&sample_analysis(id, "score", "after"))
            .unwrap();
        let active = HashSet::from([analysis_cache_key(id, "after")]);
        storage.purge_stale_by_stable_keys(&active).unwrap();
        assert_eq!(storage.load_analysis_index().unwrap().len(), 1);
        assert_eq!(
            storage
                .load_function(id)
                .unwrap()
                .unwrap()
                .code_hash
                .as_deref(),
            Some("after")
        );
    }

    #[test]
    fn display_metadata_reads_binary_and_pack_prefix_without_analysis_payload() {
        for packed in [false, true] {
            let tmp = TempDir::new().unwrap();
            let storage = AnalysisStorage::new(tmp.path());
            let id = Uuid::new_v4();
            let analysis = sample_analysis(id, "score", "hash");
            // The full record deliberately lacks its analysis tail. Display
            // metadata must not need to deserialize CFG/PDG to get its header.
            let mut bytes = bincode::serialize(&analysis).unwrap();
            let prefix_len =
                bincode::serialized_size(&(id, &analysis.function_name, &analysis.file_path))
                    .unwrap() as usize;
            bytes.truncate(prefix_len);
            if packed {
                let pack = storage.open_pack_writer().unwrap();
                storage
                    .save_preencoded_no_index_packed(id, &bytes, &pack)
                    .unwrap();
                AnalysisStorage::finish_pack(pack).unwrap();
            } else {
                storage.ensure_dir().unwrap();
                fs::write(storage.bin_path(id), &bytes).unwrap();
            }
            let metadata = storage.load_function_metadata(id).unwrap().unwrap();
            assert_eq!(metadata.function_id, id);
            assert_eq!(metadata.function_name, analysis.function_name);
            assert_eq!(metadata.file_path, analysis.file_path);
            assert!(storage.load_function(id).is_err());
        }
    }

    #[test]
    fn test_stable_key_index_and_purge_orphans() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());
        let keep = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        storage
            .save_function(&sample_analysis(keep, "keep", "h1"))
            .unwrap();
        storage
            .save_function(&sample_analysis(orphan, "orphan", "h2"))
            .unwrap();

        let index = storage.build_stable_key_index().unwrap();
        assert_eq!(index.len(), 2);

        let mut active = HashSet::new();
        active.insert(keep);
        let removed = storage.purge_orphans(&active).unwrap();
        assert_eq!(removed, 1);
        assert!(storage.load_function(orphan).unwrap().is_none());
        assert!(storage.load_function(keep).unwrap().is_some());
    }
}
