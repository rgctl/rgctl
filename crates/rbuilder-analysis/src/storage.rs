//! Storage and retrieval of CFG/PDG/Dominance analysis results.

use crate::cfg::ControlFlowGraph;
use crate::dominance::DominatorTree;
use crate::pdg::ProgramDependenceGraph;
use rbuilder_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Analysis results for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    /// Function node ID in the graph
    pub function_id: Uuid,
    /// Function name
    pub function_name: String,
    /// Source file path
    pub file_path: String,
    /// Control flow graph
    pub cfg: Option<ControlFlowGraph>,
    /// Program dependence graph
    pub pdg: Option<ProgramDependenceGraph>,
    /// Dominator tree
    pub dominance: Option<DominatorTree>,
}

/// Storage manager for analysis results.
pub struct AnalysisStorage {
    base_dir: PathBuf,
}

impl AnalysisStorage {
    /// Create a new storage manager rooted at the given directory.
    /// Typically .rbuilder/analysis/
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

    /// Save analysis for a function.
    pub fn save_function(&self, analysis: &FunctionAnalysis) -> Result<()> {
        self.ensure_dir()?;
        let filename = format!("{}.json", analysis.function_id);
        let path = self.base_dir.join(filename);
        let json = serde_json::to_string_pretty(analysis)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load analysis for a function by ID.
    pub fn load_function(&self, function_id: Uuid) -> Result<Option<FunctionAnalysis>> {
        let filename = format!("{}.json", function_id);
        let path = self.base_dir.join(filename);
        if !path.exists() {
            return Ok(None);
        }
        let json = fs::read_to_string(path)?;
        let analysis = serde_json::from_str(&json)?;
        Ok(Some(analysis))
    }

    /// Load all function analyses in the storage.
    pub fn load_all(&self) -> Result<Vec<FunctionAnalysis>> {
        if !self.base_dir.exists() {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let json = fs::read_to_string(&path)?;
                if let Ok(analysis) = serde_json::from_str(&json) {
                    results.push(analysis);
                }
            }
        }
        Ok(results)
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
        if !self.base_dir.exists() {
            return Ok(index);
        }

        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(uuid) = Uuid::parse_str(stem) {
                        index.insert(uuid, path);
                    }
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_function() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());

        let analysis = FunctionAnalysis {
            function_id: Uuid::new_v4(),
            function_name: "test_func".to_string(),
            file_path: "src/test.rs".to_string(),
            cfg: None,
            pdg: None,
            dominance: None,
        };

        storage.save_function(&analysis).unwrap();
        let loaded = storage.load_function(analysis.function_id).unwrap();

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().function_name, "test_func");
    }

    #[test]
    fn test_load_all() {
        let tmp = TempDir::new().unwrap();
        let storage = AnalysisStorage::new(tmp.path());

        for i in 0..3 {
            let analysis = FunctionAnalysis {
                function_id: Uuid::new_v4(),
                function_name: format!("func_{}", i),
                file_path: "src/test.rs".to_string(),
                cfg: None,
                pdg: None,
                dominance: None,
            };
            storage.save_function(&analysis).unwrap();
        }

        let all = storage.load_all().unwrap();
        assert_eq!(all.len(), 3);
    }
}
