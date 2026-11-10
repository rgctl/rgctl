//! Succinct Columnar Green-CST archive (`.rgctl/cst.archive.bin`, magic `RGCT` v1).

use memmap2::Mmap;
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

/// Default filename under `.rgctl/`.
pub const CST_ARCHIVE_FILE: &str = "cst.archive.bin";

const MAGIC: &[u8; 4] = b"RGCT";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 32;
const DIR_ENTRY_SIZE: usize = 32;

/// In-memory per-file Green-CST columns (writer side).
#[derive(Debug, Clone, Default)]
pub struct CstFileColumns {
    /// Repo-relative path.
    pub file_path: String,
    /// Language id byte (best-effort; 0 = unknown).
    pub lang_id: u8,
    /// bit0 = has_parse_error, bit1 = has_macro_skip.
    pub flags: u8,
    /// Balanced-parentheses bitvector words.
    pub bp_bits: Vec<u64>,
    /// Number of valid bits in [`Self::bp_bits`].
    pub bp_bit_len: u32,
    /// Tree-sitter kind ids (pre-order).
    pub kind_ids: Vec<u16>,
    /// Packed `(lead_trivia_len, byte_width)`.
    pub rel_delta_and_width: Vec<u32>,
    /// Sparse `(cst_node_idx, graph_local_idx)` anchors.
    pub graph_anchors: Vec<(u32, u32)>,
}

/// Streaming writer for `cst.archive.bin`.
#[derive(Debug, Default)]
pub struct CstArchiveWriter {
    files: Vec<CstFileColumns>,
}

impl CstArchiveWriter {
    /// Empty writer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one file's Green-CST columns.
    pub fn push_file(&mut self, mut file: CstFileColumns) {
        if file.file_path.contains('\\') {
            file.file_path = file.file_path.replace('\\', "/");
        }
        if let Some(stripped) = file.file_path.strip_prefix("./") {
            file.file_path = stripped.to_string();
        }
        self.files.push(file);
    }

    /// Write the archive to `path`.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut order: Vec<usize> = (0..self.files.len()).collect();
        order.sort_by(|&a, &b| self.files[a].file_path.cmp(&self.files[b].file_path));

        let mut bp_blob = Vec::new();
        let mut kind_blob = Vec::new();
        let mut width_blob = Vec::new();
        let mut anchor_blob = Vec::new();
        let mut strings = Vec::new();
        let mut string_offs: HashMap<String, u32> = HashMap::new();
        let mut dir = Vec::with_capacity(self.files.len() * DIR_ENTRY_SIZE);

        for &idx in &order {
            let file = &self.files[idx];
            let path_off = intern_string(&mut strings, &mut string_offs, &file.file_path);
            let bp_bit_offset = (bp_blob.len() as u64) * 8; // byte offset into bp section
            let words_needed = (file.bp_bit_len as usize)
                .div_ceil(64)
                .max(file.bp_bits.len());
            for i in 0..words_needed {
                let w = file.bp_bits.get(i).copied().unwrap_or(0);
                bp_blob.extend_from_slice(&w.to_le_bytes());
            }
            let column_start_idx = (kind_blob.len() / 2) as u32;
            for &k in &file.kind_ids {
                kind_blob.extend_from_slice(&k.to_le_bytes());
            }
            for &w in &file.rel_delta_and_width {
                width_blob.extend_from_slice(&w.to_le_bytes());
            }
            let anchor_start_idx = (anchor_blob.len() / 8) as u32;
            let anchor_count = file.graph_anchors.len().min(u16::MAX as usize) as u16;
            for &(cst_idx, graph_id) in &file.graph_anchors {
                anchor_blob.extend_from_slice(&cst_idx.to_le_bytes());
                anchor_blob.extend_from_slice(&graph_id.to_le_bytes());
            }

            let mut entry = [0u8; DIR_ENTRY_SIZE];
            entry[0..4].copy_from_slice(&path_off.to_le_bytes());
            entry[4] = file.lang_id;
            entry[5] = file.flags;
            entry[6..8].copy_from_slice(&anchor_count.to_le_bytes());
            entry[8..12].copy_from_slice(&(file.kind_ids.len() as u32).to_le_bytes());
            entry[12..20].copy_from_slice(&bp_bit_offset.to_le_bytes());
            entry[20..24].copy_from_slice(&column_start_idx.to_le_bytes());
            entry[24..28].copy_from_slice(&anchor_start_idx.to_le_bytes());
            entry[28..32].copy_from_slice(&(file.bp_bit_len).to_le_bytes());
            dir.extend_from_slice(&entry);
        }

        let file_count = self.files.len() as u32;
        let offset_dir = HEADER_SIZE as u32;
        let offset_bp = offset_dir + dir.len() as u32;
        let offset_kinds = offset_bp + bp_blob.len() as u32;
        let offset_widths = offset_kinds + kind_blob.len() as u32;
        let offset_anchors = offset_widths + width_blob.len() as u32;

        let mut header = [0u8; HEADER_SIZE];
        header[0..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&VERSION.to_le_bytes());
        header[8..12].copy_from_slice(&file_count.to_le_bytes());
        header[12..16].copy_from_slice(&offset_dir.to_le_bytes());
        header[16..20].copy_from_slice(&offset_bp.to_le_bytes());
        header[20..24].copy_from_slice(&offset_kinds.to_le_bytes());
        header[24..28].copy_from_slice(&offset_widths.to_le_bytes());
        header[28..32].copy_from_slice(&offset_anchors.to_le_bytes());

        let tmp = path.with_extension("bin.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&header)?;
            f.write_all(&dir)?;
            f.write_all(&bp_blob)?;
            f.write_all(&kind_blob)?;
            f.write_all(&width_blob)?;
            f.write_all(&anchor_blob)?;
            f.write_all(&(strings.len() as u32).to_le_bytes())?;
            f.write_all(&strings)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

fn intern_string(strings: &mut Vec<u8>, offs: &mut HashMap<String, u32>, s: &str) -> u32 {
    if let Some(&off) = offs.get(s) {
        return off;
    }
    let off = strings.len() as u32;
    let bytes = s.as_bytes();
    strings.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    strings.extend_from_slice(bytes);
    offs.insert(s.to_string(), off);
    off
}

/// Mmap-backed CST archive reader.
pub struct CstArchiveReader {
    mmap: Arc<Mmap>,
    file_count: u32,
    offset_dir: usize,
    offset_bp: usize,
    offset_kinds: usize,
    offset_widths: usize,
    #[allow(dead_code)]
    offset_anchors: usize,
    offset_strings: usize,
}

impl CstArchiveReader {
    /// Open an existing `cst.archive.bin`.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = Arc::new(unsafe { Mmap::map(&file)? });
        if mmap.len() < HEADER_SIZE {
            return Err(Error::SerdeError("cst archive truncated header".into()));
        }
        if &mmap[0..4] != MAGIC {
            return Err(Error::SerdeError("cst archive bad magic".into()));
        }
        let version = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        if version != VERSION {
            return Err(Error::SerdeError(format!(
                "cst archive unsupported version {version}"
            )));
        }
        let file_count = u32::from_le_bytes(mmap[8..12].try_into().unwrap());
        let offset_dir = u32::from_le_bytes(mmap[12..16].try_into().unwrap()) as usize;
        let offset_bp = u32::from_le_bytes(mmap[16..20].try_into().unwrap()) as usize;
        let offset_kinds = u32::from_le_bytes(mmap[20..24].try_into().unwrap()) as usize;
        let offset_widths = u32::from_le_bytes(mmap[24..28].try_into().unwrap()) as usize;
        let offset_anchors = u32::from_le_bytes(mmap[28..32].try_into().unwrap()) as usize;
        let offset_strings = if file_count == 0 {
            offset_anchors
        } else {
            let last_start = offset_dir + (file_count as usize - 1) * DIR_ENTRY_SIZE;
            if last_start + DIR_ENTRY_SIZE > mmap.len() {
                return Err(Error::SerdeError("cst dir truncated".into()));
            }
            let last = &mmap[last_start..last_start + DIR_ENTRY_SIZE];
            let anchor_count = u16::from_le_bytes([last[6], last[7]]) as usize;
            let anchor_start = u32::from_le_bytes(last[24..28].try_into().unwrap()) as usize;
            offset_anchors + (anchor_start + anchor_count) * 8
        };
        Ok(Self {
            mmap,
            file_count,
            offset_dir,
            offset_bp,
            offset_kinds,
            offset_widths,
            offset_anchors,
            offset_strings,
        })
    }

    /// Number of files in the archive.
    pub fn file_count(&self) -> u32 {
        self.file_count
    }

    /// Read directory entry `idx` (32 bytes).
    pub fn dir_entry(&self, idx: usize) -> Result<&[u8]> {
        if idx as u32 >= self.file_count {
            return Err(Error::SerdeError("cst dir index out of range".into()));
        }
        let start = self.offset_dir + idx * DIR_ENTRY_SIZE;
        let end = start + DIR_ENTRY_SIZE;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("cst dir truncated".into()));
        }
        Ok(&self.mmap[start..end])
    }

    /// Borrow the stored repo-relative file path for file `idx`.
    pub fn file_path_at(&self, idx: usize) -> Result<&str> {
        let entry = self.dir_entry(idx)?;
        let path_off = u32::from_le_bytes(entry[0..4].try_into().unwrap()) as usize;
        self.read_string(path_off)
    }

    /// Look up file index by repo-relative path (O(log N) binary search on sorted directory).
    pub fn find_file(&self, path: &str) -> Result<Option<usize>> {
        let norm: std::borrow::Cow<'_, str> = if path.contains('\\') {
            std::borrow::Cow::Owned(path.replace('\\', "/"))
        } else {
            std::borrow::Cow::Borrowed(path)
        };
        let norm = norm.strip_prefix("./").unwrap_or(norm.as_ref());

        let mut lo = 0usize;
        let mut hi = self.file_count as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let fp = self.file_path_at(mid)?;
            match fp.cmp(norm) {
                std::cmp::Ordering::Equal => return Ok(Some(mid)),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        for i in 0..self.file_count as usize {
            let fp = self.file_path_at(i)?;
            if fp == norm
                || (fp.len() > norm.len()
                    && fp.ends_with(norm)
                    && matches!(fp.as_bytes()[fp.len() - norm.len() - 1], b'/' | b'\\'))
            {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    /// Reconstruct `(kind_id, start_byte, end_byte)` for every leaf token in file `idx`
    /// using the succinct BP bitvector and Green `(lead_trivia_len, byte_width)` column.
    pub fn leaf_spans_at(&self, idx: usize) -> Result<Vec<(u16, u32, u32)>> {
        let entry = self.dir_entry(idx)?;
        let node_count = u32::from_le_bytes(entry[8..12].try_into().unwrap());
        let bp_bit_offset = u64::from_le_bytes(entry[12..20].try_into().unwrap()) as usize;
        let column_start = u32::from_le_bytes(entry[20..24].try_into().unwrap());
        let bp_bit_len = u32::from_le_bytes(entry[28..32].try_into().unwrap()) as usize;
        if node_count == 0 || bp_bit_len == 0 {
            return Ok(Vec::new());
        }
        let kinds = self.kind_ids_at(column_start, node_count)?;
        let widths = self.widths_at(column_start, node_count)?;
        let bp_bytes = self.bp_bits();
        let base_byte = bp_bit_offset / 8;

        let mut stack: Vec<(usize, bool)> = Vec::with_capacity(64);
        let mut kind_idx = 0usize;
        let mut cursor = 0u32;
        let mut leaves = Vec::new();

        for bit_i in 0..bp_bit_len {
            let byte_idx = base_byte + (bit_i / 8);
            if byte_idx >= bp_bytes.len() {
                break;
            }
            let open = ((bp_bytes[byte_idx] >> (bit_i % 8)) & 1) == 1;
            if open {
                if let Some((_, had_child)) = stack.last_mut() {
                    *had_child = true;
                }
                stack.push((kind_idx, false));
                kind_idx += 1;
            } else if let Some((n_idx, had_child)) = stack.pop()
                && !had_child
                && n_idx < widths.len()
            {
                let packed = widths[n_idx];
                let lead = (packed >> 16) as u16;
                let width = packed as u16;
                if lead != u16::MAX && width != u16::MAX {
                    let start = cursor.saturating_add(u32::from(lead));
                    let end = start.saturating_add(u32::from(width));
                    leaves.push((kinds[n_idx], start, end));
                    cursor = end;
                }
            }
        }
        Ok(leaves)
    }

    fn read_string(&self, off: usize) -> Result<&str> {
        let base = self.offset_strings + 4; // skip total strings len
        let abs = base + off;
        if abs + 4 > self.mmap.len() {
            return Err(Error::SerdeError("cst string truncated".into()));
        }
        let len = u32::from_le_bytes(self.mmap[abs..abs + 4].try_into().unwrap()) as usize;
        let start = abs + 4;
        let end = start + len;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("cst string payload truncated".into()));
        }
        std::str::from_utf8(&self.mmap[start..end])
            .map_err(|e| Error::SerdeError(format!("cst path utf8: {e}")))
    }

    /// Kind ids for a file column range.
    pub fn kind_ids_at(&self, column_start_idx: u32, node_count: u32) -> Result<Vec<u16>> {
        let start = self.offset_kinds + column_start_idx as usize * 2;
        let end = start + node_count as usize * 2;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("cst kind_ids truncated".into()));
        }
        let mut out = Vec::with_capacity(node_count as usize);
        for chunk in self.mmap[start..end].as_chunks::<2>().0 {
            out.push(u16::from_le_bytes(*chunk));
        }
        Ok(out)
    }

    /// Packed width column.
    pub fn widths_at(&self, column_start_idx: u32, node_count: u32) -> Result<Vec<u32>> {
        let start = self.offset_widths + column_start_idx as usize * 4;
        let end = start + node_count as usize * 4;
        if end > self.mmap.len() {
            return Err(Error::SerdeError("cst widths truncated".into()));
        }
        let mut out = Vec::with_capacity(node_count as usize);
        for chunk in self.mmap[start..end].as_chunks::<4>().0 {
            out.push(u32::from_le_bytes(*chunk));
        }
        Ok(out)
    }

    /// BP words for a file (from byte offset stored in dir entry).
    pub fn bp_bits(&self) -> &[u8] {
        &self.mmap[self.offset_bp..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_open_roundtrip_file_count() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CST_ARCHIVE_FILE);
        let mut w = CstArchiveWriter::new();
        w.push_file(CstFileColumns {
            file_path: "src/a.rs".into(),
            lang_id: 1,
            flags: 0,
            bp_bits: vec![0b01],
            bp_bit_len: 2,
            kind_ids: vec![10],
            rel_delta_and_width: vec![(0u32 << 16) | 12],
            graph_anchors: vec![(0, 7)],
        });
        w.write_to_path(&path).unwrap();
        let r = CstArchiveReader::open(&path).unwrap();
        assert_eq!(r.file_count(), 1);
        let entry = r.dir_entry(0).unwrap();
        let node_count = u32::from_le_bytes(entry[8..12].try_into().unwrap());
        assert_eq!(node_count, 1);
        let kinds = r.kind_ids_at(0, 1).unwrap();
        assert_eq!(kinds, vec![10]);
    }
}
