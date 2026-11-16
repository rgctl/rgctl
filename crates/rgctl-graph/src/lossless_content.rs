//! Zstd content-addressed source store (`.rgctl/content_store.bin`, magic `RGCS` v2).
//!
//! Coexists with the legacy hash→blob [`crate::content_store::ContentStore`] used for
//! markdown out-of-line bodies. Lossless discover writes this RGCS v2 format when
//! `--lossless` is set; write-back reads it via mmap.

use memmap2::Mmap;
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

/// Default filename (same path as legacy store; distinguished by magic).
pub const LOSSLESS_CONTENT_STORE_FILE: &str = "content_store.bin";

const MAGIC: &[u8; 4] = b"RGCS";
const VERSION: u32 = 2;
const HEADER_SIZE: usize = 32;
const INDEX_ENTRY_SIZE: usize = 56;

/// Flag: frames are zstd-compressed.
pub const FLAG_ZSTD: u32 = 1;
/// Per-frame: original bytes contained CRLF.
pub const FRAME_FLAG_CRLF: u32 = 1;
/// Per-frame: UTF-8 BOM present.
pub const FRAME_FLAG_BOM: u32 = 2;
/// Per-frame: stored uncompressed (small files).
pub const FRAME_FLAG_RAW: u32 = 4;

/// One file frame prepared on a Rayon extract worker.
#[derive(Debug, Clone)]
pub struct LosslessFrame {
    /// Repo-relative path.
    pub file_path: String,
    /// Uncompressed source bytes length.
    pub uncompressed_len: u32,
    /// Zstd level-3 frame (or raw bytes when `FRAME_FLAG_RAW`).
    pub compressed: Vec<u8>,
    /// Frame flags (`FRAME_FLAG_*`).
    pub flags: u32,
    /// BLAKE3 of original uncompressed bytes (32 bytes).
    pub blake3_hash: [u8; 32],
}

impl LosslessFrame {
    /// Compress `source` at zstd level 3 on the calling (worker) thread.
    pub fn from_source(file_path: impl Into<String>, source: &[u8]) -> Result<Self> {
        let blake3_hash = *blake3::hash(source).as_bytes();
        Self::from_source_with_hash(file_path, source, blake3_hash)
    }

    /// Compress `source` at zstd level 3 reusing a precomputed BLAKE3 digest.
    pub fn from_source_with_hash(
        file_path: impl Into<String>,
        source: &[u8],
        blake3_hash: [u8; 32],
    ) -> Result<Self> {
        let raw_path = file_path.into();
        let file_path = normalize_rel_path(&raw_path);
        let mut flags = 0u32;
        if source.starts_with(&[0xEF, 0xBB, 0xBF]) {
            flags |= FRAME_FLAG_BOM;
        }
        if source.windows(2).any(|w| w == b"\r\n") {
            flags |= FRAME_FLAG_CRLF;
        }
        let compressed = if source.len() < 64 {
            flags |= FRAME_FLAG_RAW;
            source.to_vec()
        } else {
            zstd::encode_all(source, 3)
                .map_err(|e| Error::SerdeError(format!("zstd compress: {e}")))?
        };
        Ok(Self {
            file_path,
            uncompressed_len: source.len() as u32,
            compressed,
            flags,
            blake3_hash,
        })
    }
}

fn normalize_rel_path(path: &str) -> String {
    let norm = if path.contains('\\') {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    norm.strip_prefix("./").unwrap_or(&norm).to_string()
}

/// Builder that deduplicates identical BLAKE3 hashes across files.
#[derive(Debug, Default)]
pub struct LosslessContentWriter {
    frames: Vec<LosslessFrame>,
    /// blake3 → index into frames (first occurrence).
    hash_index: HashMap<[u8; 32], usize>,
}

impl LosslessContentWriter {
    /// Empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a worker-compressed frame; identical hashes share storage.
    pub fn push(&mut self, frame: LosslessFrame) {
        if self.hash_index.contains_key(&frame.blake3_hash) {
            // Still record a directory entry pointing at the shared data via hash.
            self.frames.push(frame);
            return;
        }
        let idx = self.frames.len();
        self.hash_index.insert(frame.blake3_hash, idx);
        self.frames.push(frame);
    }

    /// Write RGCS v2 to `path`.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Sort frames deterministically by file_path for O(log N) binary search on read.
        let mut order: Vec<usize> = (0..self.frames.len()).collect();
        order.sort_by(|&a, &b| self.frames[a].file_path.cmp(&self.frames[b].file_path));

        // First pass: assign compressed_offset per unique hash.
        let mut data = Vec::new();
        let mut hash_to_offset: HashMap<[u8; 32], (u64, u32)> = HashMap::new();
        for &idx in &order {
            let frame = &self.frames[idx];
            if hash_to_offset.contains_key(&frame.blake3_hash) {
                continue;
            }
            let off = data.len() as u64;
            let len = frame.compressed.len() as u32;
            data.extend_from_slice(&frame.compressed);
            hash_to_offset.insert(frame.blake3_hash, (off, len));
        }

        // String pool for paths (length-prefixed).
        let mut strings = Vec::new();
        let mut path_offs: HashMap<&str, u32> = HashMap::new();
        let mut index = Vec::with_capacity(self.frames.len() * INDEX_ENTRY_SIZE);
        for &idx in &order {
            let frame = &self.frames[idx];
            let path_off = {
                if let Some(&off) = path_offs.get(frame.file_path.as_str()) {
                    off
                } else {
                    let off = strings.len() as u32;
                    let b = frame.file_path.as_bytes();
                    strings.extend_from_slice(&(b.len() as u32).to_le_bytes());
                    strings.extend_from_slice(b);
                    path_offs.insert(frame.file_path.as_str(), off);
                    off
                }
            };
            let (comp_off, comp_len) = hash_to_offset
                .get(&frame.blake3_hash)
                .copied()
                .expect("hash offset");
            let mut entry = [0u8; INDEX_ENTRY_SIZE];
            entry[0..4].copy_from_slice(&path_off.to_le_bytes());
            entry[4..8].copy_from_slice(&frame.uncompressed_len.to_le_bytes());
            entry[8..16].copy_from_slice(&comp_off.to_le_bytes());
            entry[16..20].copy_from_slice(&comp_len.to_le_bytes());
            entry[20..24].copy_from_slice(&frame.flags.to_le_bytes());
            entry[24..56].copy_from_slice(&frame.blake3_hash);
            index.extend_from_slice(&entry);
        }

        let entry_count = self.frames.len() as u32;
        let data_offset = HEADER_SIZE as u64;
        let index_offset = data_offset + data.len() as u64;
        let strings_offset = index_offset + index.len() as u64;

        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&VERSION.to_le_bytes());
        header[8..12].copy_from_slice(&FLAG_ZSTD.to_le_bytes());
        header[12..16].copy_from_slice(&entry_count.to_le_bytes());
        header[16..24].copy_from_slice(&index_offset.to_le_bytes());
        header[24..32].copy_from_slice(&data_offset.to_le_bytes());

        let tmp = path.with_extension("bin.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&header)?;
            f.write_all(&data)?;
            f.write_all(&index)?;
            f.write_all(&(strings.len() as u32).to_le_bytes())?;
            f.write_all(&strings)?;
            let _ = strings_offset;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Mmap-backed RGCS v2 reader.
pub struct LosslessContentReader {
    mmap: Arc<Mmap>,
    entry_count: u32,
    index_offset: usize,
    data_offset: usize,
    strings_offset: usize,
}

impl LosslessContentReader {
    /// Open RGCS v2 (rejects legacy bincode blob maps).
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = Arc::new(unsafe { Mmap::map(&file)? });
        if mmap.len() < HEADER_SIZE {
            return Err(Error::SerdeError("RGCS truncated".into()));
        }
        if &mmap[0..4] != MAGIC {
            return Err(Error::SerdeError(
                "not an RGCS v2 content store (legacy blob map?)".into(),
            ));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version != VERSION {
            return Err(Error::SerdeError(format!(
                "RGCS unsupported version {version}"
            )));
        }
        let entry_count = u32::from_le_bytes(mmap[12..16].try_into().unwrap());
        let index_offset = u64::from_le_bytes(mmap[16..24].try_into().unwrap()) as usize;
        let data_offset = u64::from_le_bytes(mmap[24..32].try_into().unwrap()) as usize;
        let strings_offset = index_offset + entry_count as usize * INDEX_ENTRY_SIZE;
        Ok(Self {
            mmap,
            entry_count,
            index_offset,
            data_offset,
            strings_offset,
        })
    }

    /// Number of file entries.
    pub fn entry_count(&self) -> u32 {
        self.entry_count
    }

    /// Decompress entry `idx` to owned bytes.
    pub fn decompress_at(&self, idx: usize) -> Result<Vec<u8>> {
        if idx as u32 >= self.entry_count {
            return Err(Error::SerdeError("RGCS index out of range".into()));
        }
        let start = self.index_offset + idx * INDEX_ENTRY_SIZE;
        let entry = &self.mmap[start..start + INDEX_ENTRY_SIZE];
        let uncompressed_len = u32::from_le_bytes(entry[4..8].try_into().unwrap());
        let comp_off = u64::from_le_bytes(entry[8..16].try_into().unwrap()) as usize;
        let comp_len = u32::from_le_bytes(entry[16..20].try_into().unwrap()) as usize;
        let flags = u32::from_le_bytes(entry[20..24].try_into().unwrap());
        let hash: [u8; 32] = entry[24..56].try_into().unwrap();
        let frame_start = self.data_offset + comp_off;
        let frame_end = frame_start + comp_len;
        if frame_end > self.mmap.len() {
            return Err(Error::SerdeError("RGCS frame truncated".into()));
        }
        let frame = &self.mmap[frame_start..frame_end];
        let bytes = if flags & FRAME_FLAG_RAW != 0 {
            frame.to_vec()
        } else {
            zstd::decode_all(frame).map_err(|e| Error::SerdeError(format!("zstd decode: {e}")))?
        };
        if bytes.len() != uncompressed_len as usize {
            return Err(Error::SerdeError("RGCS length mismatch".into()));
        }
        let actual = *blake3::hash(&bytes).as_bytes();
        if actual != hash {
            return Err(Error::SerdeError("RGCS blake3 mismatch".into()));
        }
        Ok(bytes)
    }

    /// Borrow the stored repo-relative file path at entry `idx` (zero-copy from mmap).
    pub fn entry_path(&self, idx: usize) -> Result<&str> {
        if idx as u32 >= self.entry_count {
            return Err(Error::SerdeError("RGCS index out of range".into()));
        }
        let start = self.index_offset + idx * INDEX_ENTRY_SIZE;
        let path_off = u32::from_le_bytes(
            self.mmap[start..start + 4]
                .try_into()
                .map_err(|_| Error::SerdeError("RGCS path_off".into()))?,
        ) as usize;
        self.read_string(path_off)
    }

    /// Expected uncompressed byte length recorded in the index for entry `idx`.
    pub fn entry_uncompressed_len(&self, idx: usize) -> Result<u32> {
        if idx as u32 >= self.entry_count {
            return Err(Error::SerdeError("RGCS index out of range".into()));
        }
        let start = self.index_offset + idx * INDEX_ENTRY_SIZE;
        Ok(u32::from_le_bytes(
            self.mmap[start + 4..start + 8].try_into().unwrap(),
        ))
    }

    /// Look up entry index by file path (O(log N) binary search on sorted index;
    /// zero-allocation suffix fallback for un-relativized paths).
    pub fn find_path(&self, path: &str) -> Result<Option<usize>> {
        let norm: std::borrow::Cow<'_, str> = if path.contains('\\') {
            std::borrow::Cow::Owned(path.replace('\\', "/"))
        } else {
            std::borrow::Cow::Borrowed(path)
        };
        let norm = norm.strip_prefix("./").unwrap_or(norm.as_ref());

        // Fast path: O(log N) binary search over sorted entries.
        let mut lo = 0usize;
        let mut hi = self.entry_count as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let entry_path = self.entry_path(mid)?;
            match entry_path.cmp(norm) {
                std::cmp::Ordering::Equal => return Ok(Some(mid)),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }

        // Fallback: suffix match without allocating per entry.
        for i in 0..self.entry_count as usize {
            let s = self.entry_path(i)?;
            if s == norm
                || (s.len() > norm.len()
                    && s.ends_with(norm)
                    && matches!(s.as_bytes()[s.len() - norm.len() - 1], b'/' | b'\\'))
            {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    fn read_string(&self, off: usize) -> Result<&str> {
        let base = self.strings_offset + 4; // skip total strings len
        let abs = base + off;
        if abs + 4 > self.mmap.len() {
            return Err(Error::SerdeError("RGCS string truncated".into()));
        }
        let len = u32::from_le_bytes(self.mmap[abs..abs + 4].try_into().unwrap()) as usize;
        let start = abs + 4;
        let end = start + len;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("RGCS string payload truncated".into()));
        }
        std::str::from_utf8(&self.mmap[start..end])
            .map_err(|e| Error::SerdeError(format!("RGCS path utf8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn roundtrip_zstd_and_dedup() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(LOSSLESS_CONTENT_STORE_FILE);
        let src = b"fn main() { println!(\"hi\"); }\n".repeat(10);
        let mut w = LosslessContentWriter::new();
        w.push(LosslessFrame::from_source("a.rs", &src).unwrap());
        w.push(LosslessFrame::from_source("b.rs", &src).unwrap()); // same hash
        w.write_to_path(&path).unwrap();

        let r = LosslessContentReader::open(&path).unwrap();
        assert_eq!(r.entry_count(), 2);
        let a = r.decompress_at(0).unwrap();
        let b = r.decompress_at(1).unwrap();
        assert_eq!(a, src);
        assert_eq!(b, src);
        assert_eq!(r.find_path("a.rs").unwrap(), Some(0));
    }
}
