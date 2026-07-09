//! Mmap-friendly CFG/PDG archive for `--with-slices` and discover `--cfg`.
//!
//! Written at discover time when CFG/PDG analysis runs; loaded on blast-radius
//! slice traces to avoid rebuilding PDGs per handoff seed.

use crate::cfg::ControlFlowGraph;
use crate::pdg::ProgramDependenceGraph;
use memmap2::Mmap;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Magic bytes for CFG/PDG archive files (`RBCP`).
pub const ARCHIVE_MAGIC: [u8; 4] = *b"RBCP";
/// Current archive format version.
pub const ARCHIVE_VERSION: u32 = 1;

/// Default archive filename under `.rbuilder/analysis/`.
pub const CFG_PDG_ARCHIVE_FILE: &str = "cfg_pdg.archive.bin";

/// One function's precomputed control- and data-flow graphs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfgPdgRecord {
    /// Function node id in the code graph.
    pub function_id: Uuid,
    /// BLAKE3 of source body at index time.
    pub code_hash: String,
    /// Function symbol name at index time (survives graph re-index).
    #[serde(default)]
    pub function_name: String,
    /// Source file path at index time.
    #[serde(default)]
    pub file_path: Option<String>,
    /// Control-flow graph.
    pub cfg: ControlFlowGraph,
    /// Program dependence graph.
    pub pdg: ProgramDependenceGraph,
}

/// On-disk bundle of CFG/PDG records keyed by function id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CfgPdgArchive {
    /// Graph snapshot digest when written (optional invalidation).
    pub graph_digest: Option<String>,
    /// CFG/PDG records keyed by function UUID.
    pub records: HashMap<Uuid, CfgPdgRecord>,
}

impl CfgPdgArchive {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(".rbuilder")
            .join("analysis")
            .join(CFG_PDG_ARCHIVE_FILE)
    }

    /// Insert or replace a record.
    pub fn insert(&mut self, record: CfgPdgRecord) {
        self.records.insert(record.function_id, record);
    }

    /// Lookup PDG for a function (hot path for slice handoffs).
    pub fn get_pdg(&self, function_id: Uuid) -> Option<&ProgramDependenceGraph> {
        self.records.get(&function_id).map(|r| &r.pdg)
    }

    /// Lookup CFG for a function.
    pub fn get_cfg(&self, function_id: Uuid) -> Option<&ControlFlowGraph> {
        self.records.get(&function_id).map(|r| &r.cfg)
    }

    /// Write archive with magic header.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = bincode::serialize(self).map_err(serde_err)?;
        let mut file = File::create(path)?;
        use std::io::Write;
        file.write_all(&ARCHIVE_MAGIC)?;
        file.write_all(&ARCHIVE_VERSION.to_le_bytes())?;
        file.write_all(&(payload.len() as u64).to_le_bytes())?;
        file.write_all(&payload)?;
        Ok(())
    }

    /// Load archive from disk (mmap parse).
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        parse_payload(&mmap)
    }

    /// Open when present; `Ok(None)` if missing.
    pub fn open_if_exists(repo_root: &Path) -> Result<Option<Self>> {
        let path = Self::default_path(repo_root);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(Self::load_from_path(&path)?))
    }

    /// CFG map for [`InterproceduralCFG::from_cfg_archive`].
    pub fn function_cfgs(&self) -> HashMap<Uuid, ControlFlowGraph> {
        self.records
            .iter()
            .map(|(id, record)| (*id, record.cfg.clone()))
            .collect()
    }

    /// Build interprocedural CFG using archived CFGs and live call graph from backend.
    pub fn to_interprocedural_cfg(
        &self,
        backend: &MemoryBackend,
    ) -> Result<crate::interprocedural_cfg::InterproceduralCFG> {
        crate::interprocedural_cfg::InterproceduralCFG::from_cfg_archive(
            backend,
            self.function_cfgs(),
        )
    }
}

fn parse_payload(mmap: &[u8]) -> Result<CfgPdgArchive> {
    if mmap.len() < 16 {
        return Err(Error::SerdeError("cfg_pdg archive truncated".into()));
    }
    if mmap[0..4] != ARCHIVE_MAGIC {
        return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version != ARCHIVE_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported cfg_pdg archive version {version}"
        )));
    }
    let payload_len = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;
    if mmap.len() < 16 + payload_len {
        return Err(Error::SerdeError(
            "cfg_pdg archive payload truncated".into(),
        ));
    }
    bincode::deserialize(&mmap[16..16 + payload_len]).map_err(serde_err)
}

fn serde_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("cfg_pdg archive: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::pdg::ProgramDependenceGraph;
    use rbuilder_graph::code_index::hash_code;
    use tempfile::TempDir;

    #[test]
    fn archive_round_trip() {
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let id = Uuid::new_v4();

        let mut archive = CfgPdgArchive::default();
        archive.insert(CfgPdgRecord {
            function_id: id,
            code_hash: hash_code(code),
            function_name: "add".into(),
            file_path: None,
            cfg,
            pdg,
        });

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CFG_PDG_ARCHIVE_FILE);
        archive.write_to_path(&path).unwrap();

        let loaded = CfgPdgArchive::load_from_path(&path).unwrap();
        assert!(loaded.get_pdg(id).is_some());
        assert!(loaded.get_cfg(id).is_some());
    }
}
