//! Mmap-friendly CFG/PDG archive for `--with-slices` and discover `--cfg`.
//!
//! Written at discover time when CFG/PDG analysis runs; loaded on blast-radius
//! slice traces to avoid rebuilding PDGs per handoff seed.
//!
//! Format versions:
//! - **v1**: monolithic bincode `CfgPdgArchive` payload.
//! - **v2**: header TOC then contiguous record payloads.
//! - **v3** (current): contiguous payloads + **8-byte-aligned sorted TOC footer**
//!   (`Uuid`, `u64` offset, `u32` len, `u32` flags) so open is `O(1)` and a single
//!   record is binary-search + zero-copy slice deserialize.

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
/// Current archive format version (v3 = footer TOC + lazy mmap open).
pub const ARCHIVE_VERSION: u32 = 3;
/// Legacy monolithic bincode payload.
pub const ARCHIVE_VERSION_V1: u32 = 1;
/// Legacy header-TOC format.
pub const ARCHIVE_VERSION_V2: u32 = 2;
/// Footer sentinel (`TOC3` little-endian).
pub const TOC_FOOTER_MAGIC: u32 = 0x3354_4F43;
/// Bytes per TOC entry: uuid(16) + offset(8) + len(4) + flags(4).
pub const TOC_ENTRY_SIZE: usize = 32;
/// Trailer at EOF: toc_offset(u64) + toc_count(u32) + magic(u32).
pub const TOC_TRAILER_SIZE: usize = 16;
/// Primary record keyed by graph node / synthetic UUID.
pub const TOC_FLAG_PRIMARY: u32 = 0;
/// Secondary TOC alias keyed by [`stable_record_uuid`] — same `(offset, len)` as primary.
pub const TOC_FLAG_STABLE_ALIAS: u32 = 1;

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

/// On-disk bundle of CFG/PDG records keyed by function id (eager in-memory view).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CfgPdgArchive {
    /// Graph snapshot digest when written (optional invalidation).
    pub graph_digest: Option<String>,
    /// CFG/PDG records keyed by function UUID.
    pub records: HashMap<Uuid, CfgPdgRecord>,
}

/// Packed TOC entry (32 bytes, 8-byte aligned layout).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TocEntry {
    /// Function UUID (raw bytes).
    pub uuid: [u8; 16],
    /// Absolute file offset of the record payload.
    pub offset: u64,
    /// Payload length in bytes.
    pub len: u32,
    /// `TOC_FLAG_PRIMARY` or `TOC_FLAG_STABLE_ALIAS` (alias shares primary payload).
    pub flags: u32,
}

impl TocEntry {
    fn is_primary(self) -> bool {
        self.flags == TOC_FLAG_PRIMARY
    }

    fn function_id(self) -> Uuid {
        Uuid::from_bytes(self.uuid)
    }

    fn write_to(self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.uuid);
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
    }

    fn read_from(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < TOC_ENTRY_SIZE {
            return Err(Error::SerdeError("cfg_pdg TOC entry truncated".into()));
        }
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&bytes[0..16]);
        let offset = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let flags = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        Ok(Self {
            uuid,
            offset,
            len,
            flags,
        })
    }
}

/// Lazy mmap view: `O(1)` open, `O(log F)` record lookup via footer TOC binary search.
pub struct CfgPdgArchiveMmap {
    mmap: Mmap,
    /// Archive format version (v1/v2/v3).
    #[allow(dead_code)]
    version: u32,
    /// Absolute offset of first TOC entry (v3 only).
    toc_offset: usize,
    toc_count: usize,
    graph_digest: Option<String>,
    /// Eager fallback for v1/v2 archives.
    eager: Option<CfgPdgArchive>,
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

    /// Write archive with magic header and footer TOC (v3).
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
        let mut preserved_digest: Option<String> = None;
        if path.is_file() {
            if let Ok(existing) = load_raw_record_bytes(path) {
                merged = existing;
            }
            if graph_digest.is_none()
                && let Ok(view) = CfgPdgArchiveMmap::open(path)
            {
                preserved_digest = view.graph_digest().map(str::to_string);
            }
        }
        for (id, bytes) in records {
            merged.insert(*id, bytes.clone());
        }

        let mut record_bytes: Vec<(Uuid, Vec<u8>)> = merged.into_iter().collect();
        record_bytes.sort_by_key(|(id, _)| *id);

        let digest = graph_digest.or(preserved_digest.as_deref());
        write_v3_archive(path, digest, &record_bytes)
    }

    /// Append (or replace) a single pre-encoded record in an existing archive.
    pub fn append_preencoded_record(
        path: &Path,
        graph_digest: Option<&str>,
        function_id: Uuid,
        bytes: &[u8],
    ) -> Result<()> {
        Self::write_preencoded_to_path(path, graph_digest, &[(function_id, bytes.to_vec())])
    }

    /// Load archive from disk (eager deserialize of all records).
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: read-only mapping for parse lifetime.
        let mmap = unsafe { Mmap::map(&file)? };
        parse_payload(&mmap)
    }

    /// Load a single record without deserializing the full archive.
    pub fn load_record_from_path(path: &Path, function_id: Uuid) -> Result<Option<CfgPdgRecord>> {
        let view = CfgPdgArchiveMmap::open(path)?;
        view.load_record(function_id)
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

impl CfgPdgArchiveMmap {
    /// Open an archive with `O(1)` work for v3 (footer read only).
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_mmap(mmap)
    }

    /// Open when present.
    pub fn open_if_exists(repo_root: &Path) -> Result<Option<Self>> {
        let path = CfgPdgArchive::default_path(repo_root);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(Self::open(&path)?))
    }

    fn from_mmap(mmap: Mmap) -> Result<Self> {
        if mmap.len() < 8 || mmap[0..4] != ARCHIVE_MAGIC {
            return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        match version {
            ARCHIVE_VERSION_V1 | ARCHIVE_VERSION_V2 => {
                let archive = parse_payload(&mmap)?;
                Ok(Self {
                    mmap,
                    version,
                    toc_offset: 0,
                    toc_count: 0,
                    graph_digest: archive.graph_digest.clone(),
                    eager: Some(archive),
                })
            }
            ARCHIVE_VERSION => {
                let (toc_offset, toc_count, digest) = parse_v3_footer(&mmap)?;
                Ok(Self {
                    mmap,
                    version,
                    toc_offset,
                    toc_count,
                    graph_digest: digest,
                    eager: None,
                })
            }
            other => Err(Error::SerdeError(format!(
                "unsupported cfg_pdg archive version {other}"
            ))),
        }
    }

    /// Graph digest stored in the archive header (if any).
    pub fn graph_digest(&self) -> Option<&str> {
        self.graph_digest.as_deref()
    }

    /// Number of primary records (stable-alias TOC entries excluded).
    pub fn len(&self) -> usize {
        if let Some(eager) = &self.eager {
            return eager.records.len();
        }
        (0..self.toc_count)
            .filter_map(|i| self.toc_entry_at(i).ok())
            .filter(|e| e.is_primary())
            .count()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether a function id is present (binary search on v3 TOC).
    pub fn contains(&self, function_id: Uuid) -> bool {
        if let Some(eager) = &self.eager {
            return eager.records.contains_key(&function_id);
        }
        self.toc_lookup(function_id).is_some()
    }

    /// Deserialize one record from its TOC slice.
    pub fn load_record(&self, function_id: Uuid) -> Result<Option<CfgPdgRecord>> {
        if let Some(eager) = &self.eager {
            return Ok(eager.records.get(&function_id).cloned());
        }
        let Some(entry) = self.toc_lookup(function_id) else {
            return Ok(None);
        };
        deserialize_record_slice(&self.mmap, entry)
    }

    /// Primary function ids in TOC order (stable aliases excluded).
    pub fn function_ids(&self) -> Vec<Uuid> {
        if let Some(eager) = &self.eager {
            return eager.records.keys().copied().collect();
        }
        let mut ids = Vec::with_capacity(self.toc_count);
        for i in 0..self.toc_count {
            if let Ok(entry) = self.toc_entry_at(i)
                && entry.is_primary()
            {
                ids.push(entry.function_id());
            }
        }
        ids
    }

    fn toc_lookup(&self, function_id: Uuid) -> Option<TocEntry> {
        if self.toc_count == 0 {
            return None;
        }
        let target = function_id.as_bytes();
        let mut lo = 0usize;
        let mut hi = self.toc_count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let entry = self.toc_entry_at(mid).ok()?;
            match entry.uuid.cmp(target) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(entry),
            }
        }
        None
    }

    fn toc_entry_at(&self, index: usize) -> Result<TocEntry> {
        let start = self
            .toc_offset
            .checked_add(
                index
                    .checked_mul(TOC_ENTRY_SIZE)
                    .ok_or_else(|| Error::SerdeError("cfg_pdg TOC index overflow".into()))?,
            )
            .ok_or_else(|| Error::SerdeError("cfg_pdg TOC offset overflow".into()))?;
        let end = start + TOC_ENTRY_SIZE;
        if end > self.mmap.len().saturating_sub(TOC_TRAILER_SIZE) {
            return Err(Error::SerdeError("cfg_pdg TOC entry out of bounds".into()));
        }
        TocEntry::read_from(&self.mmap[start..end])
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

/// Deterministic archive key from `(file, name, code_hash)` (first 16 bytes of BLAKE3).
pub fn stable_record_uuid(file_path: &str, function_name: &str, code_hash: &str) -> Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(file_path.as_bytes());
    hasher.update(&[0]);
    hasher.update(function_name.as_bytes());
    hasher.update(&[0]);
    hasher.update(code_hash.as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    Uuid::from_bytes(bytes)
}

/// Options for [`ensure_cfg_pdg_for_source`].
#[derive(Debug, Clone)]
pub struct EnsureCfgPdgOpts<'a> {
    /// Repository root (archive written under `.rgctl/analysis/`).
    pub repo_root: &'a Path,
    /// Language id (`rust`, `python`, …).
    pub language: &'a str,
    /// Full file source.
    pub source: &'a str,
    /// Function / method name.
    pub function_name: &'a str,
    /// Path relative to repo (used for stable cache key); optional.
    pub file_path: Option<&'a str>,
    /// Optional CST byte span for `O(log N)` seek.
    pub start_byte: Option<usize>,
    /// Exclusive end byte.
    pub end_byte: Option<usize>,
    /// Graph node id when known; otherwise derived from stable key.
    pub function_id: Option<Uuid>,
    /// When false, build but do not append to the archive.
    pub persist: bool,
}

/// Load CFG/PDG from the archive when present; otherwise build on demand (byte-range seek
/// when spans are given) and optionally append the record to `.rgctl/analysis/cfg_pdg.archive.bin`.
pub fn ensure_cfg_pdg_for_source(opts: EnsureCfgPdgOpts<'_>) -> Result<CfgPdgRecord> {
    use crate::cfg_builder::{build_cfg_for_byte_range, build_cfg_for_function};
    use rgctl_graph::code_index::hash_code;

    let file_key = opts.file_path.unwrap_or("");
    let code_hash = hash_code(opts.source);
    let java = opts.language == "java";
    let mut prepared_cfg = if java && opts.function_id.is_none() && opts.start_byte.is_none() {
        Some(build_cfg_for_function(
            opts.language,
            opts.source,
            opts.function_name,
        )?)
    } else {
        None
    };
    let function_id = opts.function_id.unwrap_or_else(|| {
        if java {
            stable_record_uuid(
                file_key,
                &format!(
                    "{}:{:?}:{:?}",
                    opts.function_name, opts.start_byte, opts.end_byte
                ),
                &code_hash,
            )
        } else {
            stable_record_uuid(file_key, opts.function_name, &code_hash)
        }
    });

    let archive_path = CfgPdgArchive::default_path(opts.repo_root);
    if archive_path.is_file()
        && let Ok(view) = CfgPdgArchiveMmap::open(&archive_path)
    {
        if let Ok(Some(record)) = view.load_record(function_id) {
            if !java || record.code_hash == code_hash {
                return Ok(record);
            }
        }
        // Discover `--with-cfg` keys by graph node Uuid; on-demand path uses a
        // stable content hash. Fall back to (file, name, hash) match.
        if !java
            && let Some(record) =
                find_record_by_stable_key(&view, file_key, opts.function_name, &code_hash)
        {
            return Ok(record);
        }
    }

    let cfg = if let Some(cfg) = prepared_cfg.take() {
        cfg
    } else {
        match (opts.start_byte, opts.end_byte) {
            (Some(start), Some(end)) => build_cfg_for_byte_range(
                opts.language,
                opts.source,
                opts.function_name,
                start,
                end,
            )?,
            _ => build_cfg_for_function(opts.language, opts.source, opts.function_name)?,
        }
    };
    let pdg = ProgramDependenceGraph::build(&cfg, opts.source.as_bytes())?;
    let record = CfgPdgRecord {
        function_id,
        code_hash,
        function_name: opts.function_name.to_string(),
        file_path: opts.file_path.map(str::to_string),
        cfg: Arc::new(cfg),
        pdg: Arc::new(pdg),
    };

    if opts.persist {
        let bytes = bincode::serialize(&record).map_err(serde_err)?;
        let _ = CfgPdgArchive::append_preencoded_record(&archive_path, None, function_id, &bytes);
    }
    Ok(record)
}

/// Peek record identity fields without restoring PDG indexes (write-path only).
fn stable_alias_uuid_from_payload(bytes: &[u8]) -> Option<Uuid> {
    let record: CfgPdgRecord = bincode::deserialize(bytes).ok()?;
    let path = record.file_path.as_deref()?;
    if path.is_empty() || record.function_name.is_empty() || record.code_hash.is_empty() {
        return None;
    }
    Some(stable_record_uuid(
        path,
        &record.function_name,
        &record.code_hash,
    ))
}

fn find_record_by_stable_key(
    view: &CfgPdgArchiveMmap,
    file_key: &str,
    function_name: &str,
    code_hash: &str,
) -> Option<CfgPdgRecord> {
    // Prefer O(log F) TOC hit when a stable alias was written (v3 + file_path).
    if !file_key.is_empty() {
        let alias = stable_record_uuid(file_key, function_name, code_hash);
        if let Ok(Some(record)) = view.load_record(alias) {
            return Some(record);
        }
    }
    // Legacy archives / path-suffix fuzzy match (linear; only for pre-alias files).
    for id in view.function_ids() {
        let Ok(Some(record)) = view.load_record(id) else {
            continue;
        };
        if record.function_name != function_name || record.code_hash != code_hash {
            continue;
        }
        if file_key.is_empty() {
            return Some(record);
        }
        let Some(path) = record.file_path.as_deref() else {
            continue;
        };
        if path == file_key || path.ends_with(file_key) || file_key.ends_with(path) {
            return Some(record);
        }
    }
    None
}

fn write_v3_archive(
    path: &Path,
    graph_digest: Option<&str>,
    record_bytes: &[(Uuid, Vec<u8>)],
) -> Result<()> {
    let digest_bytes = graph_digest.unwrap_or("").as_bytes();
    let digest_len = u32::try_from(digest_bytes.len())
        .map_err(|_| Error::SerdeError("graph digest too long".into()))?;

    // Header: magic + version + digest_len + digest
    let header_len = 4 + 4 + 4 + digest_bytes.len();
    let mut payload = Vec::new();
    // Primary + optional stable-alias TOC rows (alias shares primary payload).
    let mut toc_entries: Vec<TocEntry> = Vec::with_capacity(record_bytes.len().saturating_mul(2));
    let mut abs_offset = header_len as u64;
    for (id, bytes) in record_bytes {
        let length = u32::try_from(bytes.len())
            .map_err(|_| Error::SerdeError("cfg_pdg record exceeds u32::MAX".into()))?;
        toc_entries.push(TocEntry {
            uuid: *id.as_bytes(),
            offset: abs_offset,
            len: length,
            flags: TOC_FLAG_PRIMARY,
        });
        if let Some(alias) = stable_alias_uuid_from_payload(bytes)
            && alias != *id
        {
            toc_entries.push(TocEntry {
                uuid: *alias.as_bytes(),
                offset: abs_offset,
                len: length,
                flags: TOC_FLAG_STABLE_ALIAS,
            });
        }
        payload.extend_from_slice(bytes);
        abs_offset += u64::from(length);
    }
    toc_entries.sort_by_key(|a| a.uuid);

    let payload_end = header_len + payload.len();
    let toc_offset = (payload_end + 7) & !7; // 8-byte align
    let pad = toc_offset - payload_end;

    let count = u32::try_from(toc_entries.len())
        .map_err(|_| Error::SerdeError("cfg_pdg archive too many records".into()))?;

    let mut file = BufWriter::with_capacity(4 * 1024 * 1024, File::create(path)?);
    file.write_all(&ARCHIVE_MAGIC)?;
    file.write_all(&ARCHIVE_VERSION.to_le_bytes())?;
    file.write_all(&digest_len.to_le_bytes())?;
    file.write_all(digest_bytes)?;
    file.write_all(&payload)?;
    if pad > 0 {
        file.write_all(&vec![0u8; pad])?;
    }
    let mut toc_bytes = Vec::with_capacity(toc_entries.len() * TOC_ENTRY_SIZE);
    for entry in &toc_entries {
        entry.write_to(&mut toc_bytes);
    }
    file.write_all(&toc_bytes)?;
    file.write_all(&(toc_offset as u64).to_le_bytes())?;
    file.write_all(&count.to_le_bytes())?;
    file.write_all(&TOC_FOOTER_MAGIC.to_le_bytes())?;
    file.flush()?;
    Ok(())
}

fn parse_v3_footer(mmap: &[u8]) -> Result<(usize, usize, Option<String>)> {
    if mmap.len() < 8 + TOC_TRAILER_SIZE {
        return Err(Error::SerdeError("cfg_pdg archive truncated".into()));
    }
    let trailer_start = mmap.len() - TOC_TRAILER_SIZE;
    let toc_offset =
        u64::from_le_bytes(mmap[trailer_start..trailer_start + 8].try_into().unwrap()) as usize;
    let toc_count = u32::from_le_bytes(
        mmap[trailer_start + 8..trailer_start + 12]
            .try_into()
            .unwrap(),
    ) as usize;
    let magic = u32::from_le_bytes(
        mmap[trailer_start + 12..trailer_start + 16]
            .try_into()
            .unwrap(),
    );
    if magic != TOC_FOOTER_MAGIC {
        return Err(Error::SerdeError(
            "cfg_pdg archive missing TOC3 footer".into(),
        ));
    }
    let toc_bytes = toc_count
        .checked_mul(TOC_ENTRY_SIZE)
        .ok_or_else(|| Error::SerdeError("cfg_pdg TOC overflow".into()))?;
    if !toc_offset.is_multiple_of(8) {
        return Err(Error::SerdeError(
            "cfg_pdg TOC footer not 8-byte aligned".into(),
        ));
    }
    if toc_offset
        .checked_add(toc_bytes)
        .is_none_or(|end| end > trailer_start)
    {
        return Err(Error::SerdeError("cfg_pdg TOC out of bounds".into()));
    }
    let digest = read_header_digest(mmap);
    Ok((toc_offset, toc_count, digest))
}

fn deserialize_record_slice(mmap: &[u8], entry: TocEntry) -> Result<Option<CfgPdgRecord>> {
    let start = entry.offset as usize;
    let end = start
        .checked_add(entry.len as usize)
        .ok_or_else(|| Error::SerdeError("cfg_pdg record length overflow".into()))?;
    if end > mmap.len() {
        return Err(Error::SerdeError("cfg_pdg record truncated".into()));
    }
    let mut record: CfgPdgRecord = bincode::deserialize(&mmap[start..end]).map_err(serde_err)?;
    Arc::make_mut(&mut record.pdg).restore_derived_indexes();
    Ok(Some(record))
}

/// Load archive payloads as raw bytes (no CFG/PDG deserialize).
fn load_raw_record_bytes(path: &Path) -> Result<FxHashMap<Uuid, Vec<u8>>> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };
    if mmap.len() < 8 || mmap[0..4] != ARCHIVE_MAGIC {
        return Err(Error::SerdeError("invalid cfg_pdg archive magic".into()));
    }
    let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
    match version {
        ARCHIVE_VERSION_V1 => {
            let archive = parse_payload(&mmap)?;
            let mut out = FxHashMap::default();
            for (id, record) in &archive.records {
                out.insert(*id, bincode::serialize(record).map_err(serde_err)?);
            }
            Ok(out)
        }
        ARCHIVE_VERSION_V2 => {
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
        ARCHIVE_VERSION => {
            let (toc_offset, toc_count, _) = parse_v3_footer(&mmap)?;
            let mut out = FxHashMap::default();
            for i in 0..toc_count {
                let start = toc_offset + i * TOC_ENTRY_SIZE;
                let entry = TocEntry::read_from(&mmap[start..start + TOC_ENTRY_SIZE])?;
                // Skip stable-alias rows — payload is already keyed by primary UUID.
                if !entry.is_primary() {
                    continue;
                }
                let rec_start = entry.offset as usize;
                let rec_end = rec_start + entry.len as usize;
                if rec_end > mmap.len() {
                    return Err(Error::SerdeError("cfg_pdg record truncated".into()));
                }
                out.insert(entry.function_id(), mmap[rec_start..rec_end].to_vec());
            }
            Ok(out)
        }
        other => Err(Error::SerdeError(format!(
            "unsupported cfg_pdg archive version {other}"
        ))),
    }
}

#[derive(Debug, Clone, Copy)]
struct V2TocEntry {
    offset: u64,
    length: u32,
}

fn parse_v2_header(mmap: &[u8]) -> Result<(usize, FxHashMap<Uuid, V2TocEntry>, usize)> {
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
        toc.insert(id, V2TocEntry { offset, length });
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
    match version {
        ARCHIVE_VERSION_V1 => {
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
            Ok(archive)
        }
        ARCHIVE_VERSION_V2 => {
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
            Ok(CfgPdgArchive {
                graph_digest: read_header_digest(mmap),
                records,
            })
        }
        ARCHIVE_VERSION => {
            let (toc_offset, toc_count, digest) = parse_v3_footer(mmap)?;
            let mut records = HashMap::with_capacity(toc_count);
            for i in 0..toc_count {
                let start = toc_offset + i * TOC_ENTRY_SIZE;
                let entry = TocEntry::read_from(&mmap[start..start + TOC_ENTRY_SIZE])?;
                if !entry.is_primary() {
                    continue;
                }
                if let Some(record) = deserialize_record_slice(mmap, entry)? {
                    records.insert(entry.function_id(), record);
                }
            }
            Ok(CfgPdgArchive {
                graph_digest: digest,
                records,
            })
        }
        other => Err(Error::SerdeError(format!(
            "unsupported cfg_pdg archive version {other}"
        ))),
    }
}

fn read_header_digest(mmap: &[u8]) -> Option<String> {
    if mmap.len() < 12 {
        return None;
    }
    let digest_len = u32::from_le_bytes(mmap[8..12].try_into().unwrap()) as usize;
    if digest_len == 0 {
        return None;
    }
    let end = 12 + digest_len;
    if end > mmap.len() {
        return None;
    }
    std::str::from_utf8(&mmap[12..end]).ok().map(str::to_string)
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

    fn sample_record(id: Uuid, name: &str) -> CfgPdgRecord {
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        CfgPdgRecord {
            function_id: id,
            code_hash: hash_code(code),
            function_name: name.into(),
            file_path: None,
            cfg: Arc::new(cfg),
            pdg: Arc::new(pdg),
        }
    }

    #[test]
    fn archive_round_trip_v3() {
        let id = Uuid::new_v4();
        let mut archive = CfgPdgArchive::default();
        archive.insert(sample_record(id, "add"));

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CFG_PDG_ARCHIVE_FILE);
        archive.write_to_path(&path).unwrap();

        let file = File::open(&path).unwrap();
        let mmap = unsafe { Mmap::map(&file).unwrap() };
        assert_eq!(&mmap[0..4], &ARCHIVE_MAGIC);
        assert_eq!(
            u32::from_le_bytes(mmap[4..8].try_into().unwrap()),
            ARCHIVE_VERSION
        );
        let trailer = &mmap[mmap.len() - TOC_TRAILER_SIZE..];
        let toc_offset = u64::from_le_bytes(trailer[0..8].try_into().unwrap());
        assert_eq!(toc_offset % 8, 0);
        assert_eq!(
            u32::from_le_bytes(trailer[12..16].try_into().unwrap()),
            TOC_FOOTER_MAGIC
        );

        let loaded = CfgPdgArchive::load_from_path(&path).unwrap();
        assert!(loaded.get_pdg(id).is_some());
        assert!(loaded.get_cfg(id).is_some());

        let view = CfgPdgArchiveMmap::open(&path).unwrap();
        assert!(view.contains(id));
        assert_eq!(view.len(), 1);
        let single = view.load_record(id).unwrap().expect("selective load");
        assert_eq!(single.function_name, "add");
    }

    #[test]
    fn footer_binary_search_among_many() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CFG_PDG_ARCHIVE_FILE);
        let mut archive = CfgPdgArchive::default();
        let mut ids = Vec::new();
        for i in 0..32 {
            let id = Uuid::from_u128(i as u128 + 1);
            ids.push(id);
            archive.insert(sample_record(id, "add"));
        }
        archive.write_to_path(&path).unwrap();

        let view = CfgPdgArchiveMmap::open(&path).unwrap();
        assert_eq!(view.len(), 32);
        let target = ids[17];
        let rec = view.load_record(target).unwrap().expect("found");
        assert_eq!(rec.function_id, target);
        assert!(view.load_record(Uuid::nil()).unwrap().is_none());
    }

    #[test]
    fn append_merges_existing_records() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CFG_PDG_ARCHIVE_FILE);
        let id_a = Uuid::from_u128(1);
        let id_b = Uuid::from_u128(2);
        let mut first = CfgPdgArchive::default();
        first.graph_digest = Some("digest-keep".into());
        first.insert(sample_record(id_a, "add"));
        first.write_to_path(&path).unwrap();

        let bytes = bincode::serialize(&sample_record(id_b, "add")).unwrap();
        CfgPdgArchive::append_preencoded_record(&path, None, id_b, &bytes).unwrap();

        let view = CfgPdgArchiveMmap::open(&path).unwrap();
        assert!(view.contains(id_a));
        assert!(view.contains(id_b));
        assert_eq!(view.len(), 2);
        assert_eq!(view.graph_digest(), Some("digest-keep"));
    }

    #[test]
    fn stable_alias_toc_hits_without_linear_scan() {
        let tmp = TempDir::new().unwrap();
        let path = CfgPdgArchive::default_path(tmp.path());
        let graph_id = Uuid::from_u128(42);
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let code_hash = hash_code(code);
        let file_path = "src/math.rs";
        let mut archive = CfgPdgArchive::default();
        archive.insert(CfgPdgRecord {
            function_id: graph_id,
            code_hash: code_hash.clone(),
            function_name: "add".into(),
            file_path: Some(file_path.into()),
            cfg: Arc::new(cfg),
            pdg: Arc::new(pdg),
        });
        archive.write_to_path(&path).unwrap();

        let alias = stable_record_uuid(file_path, "add", &code_hash);
        assert_ne!(alias, graph_id, "alias must differ from graph node id");

        let view = CfgPdgArchiveMmap::open(&path).unwrap();
        assert_eq!(view.len(), 1, "aliases must not inflate primary len");
        assert_eq!(view.function_ids(), vec![graph_id]);
        assert!(view.contains(graph_id));
        assert!(view.contains(alias), "stable alias must be in TOC");
        let via_alias = view.load_record(alias).unwrap().expect("alias TOC hit");
        assert_eq!(via_alias.function_id, graph_id);
        assert_eq!(via_alias.function_name, "add");

        // ensure_cfg_pdg with function_id: None must hit alias (no rebuild).
        let loaded = ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
            repo_root: tmp.path(),
            language: "rust",
            source: code,
            function_name: "add",
            file_path: Some(file_path),
            start_byte: None,
            end_byte: None,
            function_id: None,
            persist: false,
        })
        .unwrap();
        assert_eq!(loaded.function_id, graph_id);
    }
}
