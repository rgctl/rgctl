//! Mmap-friendly CFG/PDG archive for `--with-slices` and discover `--cfg`.
//!
//! Written at discover time when CFG/PDG analysis runs; loaded on blast-radius
//! slice traces to avoid rebuilding PDGs per handoff seed.
//!
//! Version 2 stores a table of contents for selective per-record deserialization.

use crate::cfg::ControlFlowGraph;
use crate::hash_maps::FxHashMap;
use crate::pdg::ProgramDependenceGraph;
use crate::storage::stable_function_key;
use memmap2::Mmap;
use rgctl_error::{Error, Result};
use rgctl_graph::backend::MemoryBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Magic bytes for CFG/PDG archive files (`RBCP`).
pub const ARCHIVE_MAGIC: [u8; 4] = *b"RBCP";
/// Current archive format version (v2 = TOC + per-record payloads).
pub const ARCHIVE_VERSION: u32 = 2;
/// Legacy archive format (monolithic bincode payload).
pub const ARCHIVE_VERSION_V1: u32 = 1;

/// Default archive filename under `.rgctl/analysis/`.
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
    /// Control-flow graph (shared with in-memory [`crate::storage::FunctionAnalysis`]).
    #[serde(
        serialize_with = "serialize_arc_cfg",
        deserialize_with = "deserialize_arc_cfg"
    )]
    pub cfg: Arc<ControlFlowGraph>,
    /// Program dependence graph (shared across slice handoffs).
    #[serde(
        serialize_with = "serialize_arc_pdg",
        deserialize_with = "deserialize_arc_pdg"
    )]
    pub pdg: Arc<ProgramDependenceGraph>,
}

fn serialize_arc_cfg<S>(
    cfg: &Arc<ControlFlowGraph>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    cfg.as_ref().serialize(serializer)
}

fn deserialize_arc_cfg<'de, D>(
    deserializer: D,
) -> std::result::Result<Arc<ControlFlowGraph>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    ControlFlowGraph::deserialize(deserializer).map(Arc::new)
}

fn serialize_arc_pdg<S>(
    pdg: &Arc<ProgramDependenceGraph>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    pdg.as_ref().serialize(serializer)
}

fn deserialize_arc_pdg<'de, D>(
    deserializer: D,
) -> std::result::Result<Arc<ProgramDependenceGraph>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    ProgramDependenceGraph::deserialize(deserializer).map(Arc::new)
}

/// On-disk bundle of CFG/PDG records keyed by function id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CfgPdgArchive {
    /// Graph snapshot digest when written (optional invalidation).
    pub graph_digest: Option<String>,
    /// CFG/PDG records keyed by function UUID.
    pub records: HashMap<Uuid, CfgPdgRecord>,
}

#[derive(Debug, Clone, Copy)]
struct TocEntry {
    offset: u64,
    length: u32,
}

impl CfgPdgArchive {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(".rgctl")
            .join("analysis")
            .join(CFG_PDG_ARCHIVE_FILE)
    }

    /// Insert or replace a record.
    pub fn insert(&mut self, record: CfgPdgRecord) {
        self.records.insert(record.function_id, record);
    }

    /// Lookup PDG for a function (hot path for slice handoffs).
    pub fn get_pdg(&self, function_id: Uuid) -> Option<&ProgramDependenceGraph> {
        self.records.get(&function_id).map(|r| r.pdg.as_ref())
    }

    /// Lookup CFG for a function.
    pub fn get_cfg(&self, function_id: Uuid) -> Option<&ControlFlowGraph> {
        self.records.get(&function_id).map(|r| r.cfg.as_ref())
    }

    /// Write archive with magic header and per-record TOC (v2).
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        let mut record_bytes: Vec<(Uuid, Vec<u8>)> = Vec::with_capacity(self.records.len());
        for (id, record) in &self.records {
            let bytes = bincode::serialize(record).map_err(serde_err)?;
            record_bytes.push((*id, bytes));
        }
        Self::write_preencoded_to_path(path, self.graph_digest.as_deref(), &record_bytes)
    }

    /// Write an archive from pre-serialized `(function_id, bincode)` pairs.
    ///
    /// When `path` already exists and contains records not present in `records`, those
    /// untouched records are merged in (incremental CFG cache hits) without re-serializing.
    pub fn write_preencoded_to_path(
        path: &Path,
        graph_digest: Option<&str>,
        records: &[(Uuid, Vec<u8>)],
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut merged: FxHashMap<Uuid, Vec<u8>> = FxHashMap::default();
        if path.is_file() {
            if let Ok(existing) = load_raw_record_bytes(path) {
                merged = existing;
            }
        }
        for (id, bytes) in records {
            merged.insert(*id, bytes.clone());
        }

        let mut record_bytes: Vec<(Uuid, Vec<u8>)> = merged.into_iter().collect();
        record_bytes.sort_by_key(|(id, _)| *id);

        let digest_bytes = graph_digest.unwrap_or("").as_bytes();
        let mut payload = Vec::new();
        let mut toc: Vec<(Uuid, TocEntry)> = Vec::with_capacity(record_bytes.len());
        let mut offset = 0u64;
        for (id, bytes) in &record_bytes {
            let length = u32::try_from(bytes.len())
                .map_err(|_| Error::SerdeError("cfg_pdg record exceeds u32::MAX".into()))?;
            toc.push((
                *id,
                TocEntry {
                    offset,
                    length,
                },
            ));
            payload.extend_from_slice(bytes);
            offset += u64::from(length);
        }

        let mut file = BufWriter::with_capacity(4 * 1024 * 1024, File::create(path)?);
        file.write_all(&ARCHIVE_MAGIC)?;
        file.write_all(&ARCHIVE_VERSION.to_le_bytes())?;
        let digest_len = u32::try_from(digest_bytes.len())
            .map_err(|_| Error::SerdeError("graph digest too long".into()))?;
        file.write_all(&digest_len.to_le_bytes())?;
        file.write_all(digest_bytes)?;
        let count = u32::try_from(toc.len())
            .map_err(|_| Error::SerdeError("cfg_pdg archive too many records".into()))?;
        file.write_all(&count.to_le_bytes())?;
        for (id, entry) in &toc {
            file.write_all(id.as_bytes())?;
            file.write_all(&entry.offset.to_le_bytes())?;
            file.write_all(&entry.length.to_le_bytes())?;
        }
        file.write_all(&payload)?;
        file.flush()?;
        Ok(())
    }

    /// Load archive from disk (mmap parse).
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: read-only mapping for parse lifetime.
        let mmap = unsafe { Mmap::map(&file)? };
        parse_payload(&mmap)
    }

    /// Load a single record without deserializing the full archive.
    pub fn load_record_from_path(path: &Path, function_id: Uuid) -> Result<Option<CfgPdgRecord>> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        if mmap.len() < 16 || mmap[0..4] != ARCHIVE_MAGIC {
            return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version == ARCHIVE_VERSION_V1 {
            let archive = parse_payload(&mmap)?;
            return Ok(archive.records.get(&function_id).cloned());
        }
        if version != ARCHIVE_VERSION {
            return Err(Error::SerdeError(format!(
                "unsupported cfg_pdg archive version {version}"
            )));
        }
        let (digest_len, toc, payload_start) = parse_v2_header(&mmap)?;
        let _ = digest_len;
        let Some(entry) = toc.get(&function_id) else {
            return Ok(None);
        };
        let start = payload_start + entry.offset as usize;
        let end = start + entry.length as usize;
        if end > mmap.len() {
            return Err(Error::SerdeError("cfg_pdg record truncated".into()));
        }
        let mut record: CfgPdgRecord = bincode::deserialize(&mmap[start..end]).map_err(serde_err)?;
        Arc::make_mut(&mut record.pdg).restore_derived_indexes();
        Ok(Some(record))
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
            .map(|(id, record)| (*id, (*record.cfg).clone()))
            .collect()
    }

    /// Zero-copy interprocedural CFG view (avoids cloning every archived CFG).
    pub fn interprocedural_cfg_view(
        &self,
        backend: &MemoryBackend,
    ) -> Result<crate::interprocedural_cfg::InterproceduralCfgView<'_>> {
        crate::interprocedural_cfg::InterproceduralCfgView::from_archive(self, backend)
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

    /// Index records by stable `(file, name, code_hash)` for incremental CFG reuse.
    pub fn stable_key_index(&self) -> HashMap<String, CfgPdgRecord> {
        let mut index = HashMap::new();
        for record in self.records.values() {
            if let Some(key) = record.stable_key() {
                index.insert(key, record.clone());
            }
        }
        index
    }
}

impl CfgPdgRecord {
    /// Stable cache key when path and hash are present.
    pub fn stable_key(&self) -> Option<String> {
        let path = self.file_path.as_deref()?;
        if self.code_hash.is_empty() || self.function_name.is_empty() {
            return None;
        }
        Some(stable_function_key(
            path,
            &self.function_name,
            &self.code_hash,
        ))
    }
}

/// Load v2 archive payloads as raw bytes (no CFG/PDG deserialize). Falls back to
/// re-serializing records for legacy v1 archives.
fn load_raw_record_bytes(path: &Path) -> Result<FxHashMap<Uuid, Vec<u8>>> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    if mmap.len() < 8 || mmap[0..4] != ARCHIVE_MAGIC {
        return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version == ARCHIVE_VERSION_V1 {
        let archive = parse_payload(&mmap)?;
        let mut out = FxHashMap::default();
        for (id, record) in &archive.records {
            out.insert(*id, bincode::serialize(record).map_err(serde_err)?);
        }
        return Ok(out);
    }
    if version != ARCHIVE_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported cfg_pdg archive version {version}"
        )));
    }
    let (_digest_len, toc, payload_start) = parse_v2_header(&mmap)?;
    let mut out = FxHashMap::default();
    for (id, entry) in toc {
        let start = payload_start + entry.offset as usize;
        let end = start + entry.length as usize;
        if end > mmap.len() {
            return Err(Error::SerdeError("cfg_pdg record truncated".into()));
        }
        out.insert(id, mmap[start..end].to_vec());
    }
    Ok(out)
}

fn parse_v2_header(mmap: &[u8]) -> Result<(usize, FxHashMap<Uuid, TocEntry>, usize)> {
    if mmap.len() < 12 {
        return Err(Error::SerdeError("cfg_pdg archive truncated".into()));
    }
    let digest_len = u32::from_le_bytes(mmap[8..12].try_into().unwrap()) as usize;
    let header_end = 12 + digest_len;
    if mmap.len() < header_end + 4 {
        return Err(Error::SerdeError("cfg_pdg archive TOC truncated".into()));
    }
    let count = u32::from_le_bytes(mmap[header_end..header_end + 4].try_into().unwrap()) as usize;
    let toc_bytes = count
        .checked_mul(16 + 8 + 4)
        .ok_or_else(|| Error::SerdeError("cfg_pdg TOC overflow".into()))?;
    let toc_start = header_end + 4;
    let payload_start = toc_start + toc_bytes;
    if mmap.len() < payload_start {
        return Err(Error::SerdeError("cfg_pdg archive TOC truncated".into()));
    }
    let mut toc = FxHashMap::default();
    let mut pos = toc_start;
    for _ in 0..count {
        let id = Uuid::from_bytes(mmap[pos..pos + 16].try_into().unwrap());
        pos += 16;
        let offset = u64::from_le_bytes(mmap[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let length = u32::from_le_bytes(mmap[pos..pos + 4].try_into().unwrap());
        pos += 4;
        toc.insert(id, TocEntry { offset, length });
    }
    Ok((digest_len, toc, payload_start))
}

fn parse_payload(mmap: &[u8]) -> Result<CfgPdgArchive> {
    if mmap.len() < 8 {
        return Err(Error::SerdeError("cfg_pdg archive truncated".into()));
    }
    if mmap[0..4] != ARCHIVE_MAGIC {
        return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    if version == ARCHIVE_VERSION_V1 {
        if mmap.len() < 16 {
            return Err(Error::SerdeError("cfg_pdg archive truncated".into()));
        }
        let payload_len = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;
        if mmap.len() < 16 + payload_len {
            return Err(Error::SerdeError(
                "cfg_pdg archive payload truncated".into(),
            ));
        }
        let mut archive: CfgPdgArchive =
            bincode::deserialize(&mmap[16..16 + payload_len]).map_err(serde_err)?;
        for record in archive.records.values_mut() {
            Arc::make_mut(&mut record.pdg).restore_derived_indexes();
        }
        return Ok(archive);
    }
    if version != ARCHIVE_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported cfg_pdg archive version {version}"
        )));
    }
    let (_digest_len, toc, payload_start) = parse_v2_header(mmap)?;
    let mut records = HashMap::with_capacity(toc.len());
    for (id, entry) in toc {
        let start = payload_start + entry.offset as usize;
        let end = start + entry.length as usize;
        if end > mmap.len() {
            return Err(Error::SerdeError("cfg_pdg record truncated".into()));
        }
        let mut record: CfgPdgRecord =
            bincode::deserialize(&mmap[start..end]).map_err(serde_err)?;
        Arc::make_mut(&mut record.pdg).restore_derived_indexes();
        records.insert(id, record);
    }
    let digest = read_v2_digest(mmap);
    Ok(CfgPdgArchive {
        graph_digest: digest,
        records,
    })
}

fn read_v2_digest(mmap: &[u8]) -> Option<String> {
    if mmap.len() < 12 {
        return None;
    }
    let digest_len = u32::from_le_bytes(mmap[8..12].try_into().unwrap()) as usize;
    if digest_len == 0 {
        return None;
    }
    let bytes = &mmap[12..12 + digest_len];
    std::str::from_utf8(bytes).ok().map(str::to_string)
}

fn serde_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("cfg_pdg archive: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_builder::build_cfg_for_function;
    use crate::pdg::ProgramDependenceGraph;
    use rgctl_graph::code_index::hash_code;
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
            cfg: Arc::new(cfg),
            pdg: Arc::new(pdg),
        });

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CFG_PDG_ARCHIVE_FILE);
        archive.write_to_path(&path).unwrap();

        let loaded = CfgPdgArchive::load_from_path(&path).unwrap();
        assert!(loaded.get_pdg(id).is_some());
        assert!(loaded.get_cfg(id).is_some());

        let single = CfgPdgArchive::load_record_from_path(&path, id)
            .unwrap()
            .expect("selective load");
        assert_eq!(single.function_name, "add");
    }
}
