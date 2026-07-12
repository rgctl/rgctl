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
        Ok(())
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
            let analysis = bincode::deserialize(&bytes)
                .map_err(|e| Error::SerdeError(format!("analysis bincode decode: {e}")))?;
            return Ok(Some(analysis));
        }
        Ok(None)
    }

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

    /// Remove analysis artifacts whose function id is not in `active_ids`.
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
