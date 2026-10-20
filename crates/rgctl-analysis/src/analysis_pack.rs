//! Append-only packfile for per-function analysis artifacts.
//!
//! Replaces thousands of individual `.analysis.bin` files with one data file
//! plus a compact offset index for O(1) random access.

use memmap2::Mmap;
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Pack data blob filename under `.rgctl/analysis/`.
pub const ANALYSIS_PACK_DATA_FILE: &str = "analysis.pack.bin";
/// Pack offset index filename under `.rgctl/analysis/`.
pub const ANALYSIS_PACK_INDEX_FILE: &str = "analysis.pack_index.bin";

const PACK_MAGIC: [u8; 4] = *b"RBAP";
const PACK_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
struct PackIndexEntry {
    offset: u64,
    length: u32,
}

/// Thread-safe append writer used during parallel CFG persistence.
pub struct AnalysisPackWriter {
    data_path: PathBuf,
    data_file: BufWriter<File>,
    entries: HashMap<Uuid, PackIndexEntry>,
    offset: u64,
}

impl AnalysisPackWriter {
    /// Create (truncate) a new pack in `analysis_dir`.
    pub fn create(analysis_dir: &Path) -> Result<Self> {
        fs::create_dir_all(analysis_dir)?;
        let data_path = analysis_dir.join(ANALYSIS_PACK_DATA_FILE);
        if data_path.exists() {
            fs::remove_file(&data_path)?;
        }
        let data_file = BufWriter::with_capacity(4 * 1024 * 1024, File::create(&data_path)?);
        Ok(Self {
            data_path,
            data_file,
            entries: HashMap::new(),
            offset: 0,
        })
    }

    /// Append one serialized analysis record (pre-encoded bytes welcome).
    pub fn append(&mut self, function_id: Uuid, bytes: &[u8]) -> Result<()> {
        let length = u32::try_from(bytes.len())
            .map_err(|_| Error::SerdeError("analysis pack record exceeds u32::MAX".into()))?;
        self.data_file.write_all(bytes)?;
        self.entries.insert(
            function_id,
            PackIndexEntry {
                offset: self.offset,
                length,
            },
        );
        self.offset += u64::from(length);
        Ok(())
    }

    /// Flush data and write the offset index.
    pub fn finish(mut self, analysis_dir: &Path) -> Result<()> {
        self.data_file.flush()?;
        drop(self.data_file);
        write_pack_index(analysis_dir, &self.entries)?;
        Ok(())
    }

    /// Data file path (for diagnostics).
    pub fn data_path(&self) -> &Path {
        &self.data_path
    }
}

/// Shared writer handle for parallel `save_function_no_index` calls.
pub type SharedPackWriter = Mutex<Option<AnalysisPackWriter>>;

fn write_pack_index(analysis_dir: &Path, entries: &HashMap<Uuid, PackIndexEntry>) -> Result<()> {
    let index_path = analysis_dir.join(ANALYSIS_PACK_INDEX_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&index_path)?;
    file.write_all(&PACK_MAGIC)?;
    file.write_all(&PACK_VERSION.to_le_bytes())?;
    let count = u32::try_from(entries.len())
        .map_err(|_| Error::SerdeError("analysis pack index too large".into()))?;
    file.write_all(&count.to_le_bytes())?;
    let mut list: Vec<_> = entries.iter().collect();
    list.sort_by_key(|(id, _)| *id);
    for (id, entry) in list {
        file.write_all(id.as_bytes())?;
        file.write_all(&entry.offset.to_le_bytes())?;
        file.write_all(&entry.length.to_le_bytes())?;
    }
    Ok(())
}

fn load_pack_index(analysis_dir: &Path) -> Result<Option<HashMap<Uuid, PackIndexEntry>>> {
    let index_path = analysis_dir.join(ANALYSIS_PACK_INDEX_FILE);
    if !index_path.is_file() {
        return Ok(None);
    }
    let mut file = File::open(&index_path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if magic != PACK_MAGIC {
        return Err(Error::SerdeError("invalid analysis pack index magic".into()));
    }
    let mut ver_buf = [0u8; 4];
    file.read_exact(&mut ver_buf)?;
    let version = u32::from_le_bytes(ver_buf);
    if version != PACK_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported analysis pack index version {version}"
        )));
    }
    let mut count_buf = [0u8; 4];
    file.read_exact(&mut count_buf)?;
    let count = u32::from_le_bytes(count_buf) as usize;
    let mut entries = HashMap::with_capacity(count);
    for _ in 0..count {
        let mut id_buf = [0u8; 16];
        file.read_exact(&mut id_buf)?;
        let id = Uuid::from_bytes(id_buf);
        let mut off_buf = [0u8; 8];
        file.read_exact(&mut off_buf)?;
        let offset = u64::from_le_bytes(off_buf);
        let mut len_buf = [0u8; 4];
        file.read_exact(&mut len_buf)?;
        let length = u32::from_le_bytes(len_buf);
        entries.insert(id, PackIndexEntry { offset, length });
    }
    Ok(Some(entries))
}

/// Memory-mapped pack reader: index loaded once, records served via mmap slices.
pub struct AnalysisPackReader {
    index: HashMap<Uuid, PackIndexEntry>,
    mmap: Mmap,
}

impl AnalysisPackReader {
    /// Open pack index + mmap the data blob (one-time cost per storage session).
    pub fn open(analysis_dir: &Path) -> Result<Option<Self>> {
        let Some(index) = load_pack_index(analysis_dir)? else {
            return Ok(None);
        };
        let data_path = analysis_dir.join(ANALYSIS_PACK_DATA_FILE);
        if !data_path.is_file() {
            return Ok(None);
        }
        let file = File::open(&data_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Some(Self { index, mmap }))
    }

    /// Number of records in the pack.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the pack has no records.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// All function ids present in the pack index.
    pub fn function_ids(&self) -> Vec<Uuid> {
        self.index.keys().copied().collect()
    }

    /// Borrow one record's bytes without copying.
    pub fn record_bytes(&self, function_id: Uuid) -> Result<Option<&[u8]>> {
        let Some(entry) = self.index.get(&function_id) else {
            return Ok(None);
        };
        let start = usize::try_from(entry.offset)
            .map_err(|_| Error::SerdeError("analysis pack offset out of range".into()))?;
        let end = start
            .checked_add(entry.length as usize)
            .ok_or_else(|| Error::SerdeError("analysis pack record length overflow".into()))?;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("analysis pack record extends past data file".into()));
        }
        Ok(Some(&self.mmap[start..end]))
    }
}

/// Cached reader handle shared across parallel `load_function` calls.
pub struct SharedPackReader {
    analysis_dir: PathBuf,
    state: Mutex<PackReaderState>,
}

enum PackReaderState {
    Uninit,
    Absent,
    Ready(Arc<AnalysisPackReader>),
}

impl SharedPackReader {
    /// Create a lazy reader for `analysis_dir` (no I/O until first use).
    pub fn new(analysis_dir: impl AsRef<Path>) -> Self {
        Self {
            analysis_dir: analysis_dir.as_ref().to_path_buf(),
            state: Mutex::new(PackReaderState::Uninit),
        }
    }

    /// Resolved reader, opening index + mmap on first access.
    pub fn get(&self) -> Result<Option<Arc<AnalysisPackReader>>> {
        let mut state = self
            .state
            .lock()
            .map_err(|e| Error::SerdeError(e.to_string()))?;
        if matches!(*state, PackReaderState::Uninit) {
            *state = match AnalysisPackReader::open(&self.analysis_dir)? {
                Some(reader) => PackReaderState::Ready(Arc::new(reader)),
                None => PackReaderState::Absent,
            };
        }
        Ok(match &*state {
            PackReaderState::Ready(reader) => Some(Arc::clone(reader)),
            PackReaderState::Absent | PackReaderState::Uninit => None,
        })
    }
}

/// Read one record from the pack by function id (opens index + data file every call).
pub fn read_pack_record(analysis_dir: &Path, function_id: Uuid) -> Result<Option<Vec<u8>>> {
    let Some(reader) = AnalysisPackReader::open(analysis_dir)? else {
        return Ok(None);
    };
    Ok(reader
        .record_bytes(function_id)?
        .map(|bytes| bytes.to_vec()))
}

/// Iterate all pack records as `(function_id, bytes)`.
pub fn read_all_pack_records(analysis_dir: &Path) -> Result<Vec<(Uuid, Vec<u8>)>> {
    let Some(reader) = AnalysisPackReader::open(analysis_dir)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(reader.len());
    for id in reader.function_ids() {
        if let Some(bytes) = reader.record_bytes(id)? {
            out.push((id, bytes.to_vec()));
        }
    }
    Ok(out)
}

/// All function ids present in the pack index.
pub fn pack_function_ids(analysis_dir: &Path) -> Result<Vec<Uuid>> {
    let Some(reader) = AnalysisPackReader::open(analysis_dir)? else {
        return Ok(Vec::new());
    };
    Ok(reader.function_ids())
}
