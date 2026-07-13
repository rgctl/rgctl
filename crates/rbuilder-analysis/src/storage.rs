//! Storage and retrieval of CFG/PDG/Dominance analysis results.
//!
//! Persists per-function analysis artifacts to disk (bincode primary, JSON legacy).
//! Not graph topology.

use crate::cfg::ControlFlowGraph;
use crate::dominance::DominatorTree;
use crate::pdg::ProgramDependenceGraph;
use crate::taint::TaintFlow;
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Stable cache key across graph re-indexes (UUIDs may change).
pub fn stable_function_key(file_path: &str, function_name: &str, code_hash: &str) -> String {
    format!("{file_path}\x1f{function_name}\x1f{code_hash}")
}

/// Suffix for bincode per-function analysis artifacts (`{uuid}.analysis.bin`).
pub const ANALYSIS_BIN_SUFFIX: &str = ".analysis.bin";

/// Sidecar index filename (`stable_key → metadata`) for incremental CFG without `load_all()`.
pub const ANALYSIS_INDEX_FILE: &str = "analysis_index.bin";

/// Lightweight metadata for incremental CFG reuse (no CFG/PDG payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisIndexEntry {
    pub stable_key: String,
    pub function_id: Uuid,
    pub code_hash: String,
    pub flow_count: usize,
    pub vulnerable_count: usize,
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
    /// Control flow graph
    pub cfg: Option<ControlFlowGraph>,
    /// Program dependence graph
    pub pdg: Option<ProgramDependenceGraph>,
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
        Some(stable_function_key(&self.file_path, &self.function_name, hash))
    }
}

/// Storage manager for analysis results.
pub struct AnalysisStorage {
    base_dir: PathBuf,
}

/// Minimal graph function metadata for aligning analysis index UUIDs after re-index.
pub struct FunctionIdSyncEntry<'a> {
    pub function_id: Uuid,
    pub function_name: &'a str,
    pub file_path: &'a str,
    pub code_hash: &'a str,
}

impl AnalysisStorage {
    /// Create a new storage manager rooted at the given directory.
    /// Typically `.rbuilder/analysis/`
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    /// Ensure the storage directory exists.
    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.base_dir)?;
        Ok(())
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
        Ok(entries
            .into_iter()
            .map(|e| (e.stable_key.clone(), e))
            .collect())
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
        if let Some(prev) = index.get(&stable_key) {
            if prev.function_id != analysis.function_id {
                let old_bin = self.bin_path(prev.function_id);
                let old_json = self.json_path(prev.function_id);
                let _ = fs::remove_file(old_bin);
                let _ = fs::remove_file(old_json);
            }
        }
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
        let mut index = self.load_analysis_index().unwrap_or_default();
        for analysis in analyses {
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
            if let Some(prev) = index.get(&stable_key) {
                if prev.function_id != analysis.function_id {
                    let old_bin = self.bin_path(prev.function_id);
                    let old_json = self.json_path(prev.function_id);
                    let _ = fs::remove_file(old_bin);
                    let _ = fs::remove_file(old_json);
                }
            }
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

    /// Align index entries (and artifact filenames) with current graph function UUIDs.
    pub fn sync_index_function_ids(
        &self,
        functions: &[FunctionIdSyncEntry<'_>],
    ) -> Result<usize> {
        let mut index = self.load_analysis_index()?;
        let mut remapped = 0usize;
        for func in functions {
            let key = stable_function_key(
                func.file_path,
                func.function_name,
                func.code_hash,
            );
            let Some(entry) = index.get_mut(&key) else {
                continue;
            };
            if entry.code_hash != func.code_hash {
                continue;
            }
            if entry.function_id == func.function_id {
                continue;
            }
            self.remap_function_artifact(entry.function_id, func.function_id)?;
            entry.function_id = func.function_id;
            remapped += 1;
        }
        if remapped > 0 {
            self.write_analysis_index(&index)?;
        }
        Ok(remapped)
    }

    /// Remove artifacts whose stable keys are not in `active_keys`.
    pub fn purge_stale_by_stable_keys(&self, active_keys: &HashSet<String>) -> Result<usize> {
        let index = self.load_analysis_index()?;
        let mut removed = 0usize;
        let mut next = index.clone();
        for (key, entry) in &index {
            if active_keys.contains(key) {
                continue;
            }
            let bin = self.bin_path(entry.function_id);
            let json = self.json_path(entry.function_id);
            if fs::remove_file(&bin).is_ok() {
                removed += 1;
            }
            let _ = fs::remove_file(json);
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
                pdg.restore_derived_indexes();
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
    bincode::deserialize(bytes).map_err(|e| {
        Error::SerdeError(format!("analysis bincode decode: {e}"))
    })
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
            cfg: self.cfg,
            pdg: self.pdg.map(|pdg| {
                let mut graph = ProgramDependenceGraph::from_parts(
                    pdg.nodes,
                    pdg.data_deps,
                    pdg.control_deps,
                    pdg.block_nodes,
                    HashMap::new(),
                );
                graph.restore_derived_indexes();
                graph
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
            cfg: self.cfg,
            pdg: self.pdg.map(|pdg| {
                let mut graph = ProgramDependenceGraph::from_parts(
                    pdg.nodes,
                    pdg.data_deps,
                    pdg.control_deps,
                    HashMap::new(),
                    HashMap::new(),
                );
                graph.restore_derived_indexes();
                graph
            }),
            dominance: self.dominance,
            taint: self.taint,
        }
    }
}

impl AnalysisStorage {
    fn analysis_paths(&self) -> Result<Vec<PathBuf>> {
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

    /// Load analysis for a function by ID (bincode preferred, JSON fallback).
    pub fn load_function(&self, function_id: Uuid) -> Result<Option<FunctionAnalysis>> {
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

    /// Load all function analyses (deduped by function id; bincode wins over JSON).
    pub fn load_all(&self) -> Result<Vec<FunctionAnalysis>> {
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
            if !active_ids.contains(&id) {
                if fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
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
                cfg: Some(cfg.clone()),
                pdg: Some(pdg.clone()),
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
        let analysis_dir = repo.join(".rbuilder/analysis");
        if !analysis_dir.is_dir() {
            return;
        }
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
            "most cached analyses should load with cfg/pdg after serde fix"
        );
    }

    #[test]
    fn test_load_metasfresh_analysis_roundtrip() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../example/metasfresh-4.9.8b");
        let analysis_dir = repo.join(".rbuilder/analysis");
        if !analysis_dir.is_dir() {
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
