//! On-demand CFG previews for large repos: compact record index + data pack.
//!
//! Built during dashboard export when per-function JSON is omitted. The browser
//! fetches one record at a time via HTTP Range on `cfg_pdg.record_data.bin`.

use crate::cfg_export::CfgDetailPayload;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use uuid::Uuid;

pub const CFG_RECORD_INDEX_FILE: &str = "cfg_pdg.record_index.bin";
pub const CFG_RECORD_DATA_FILE: &str = "cfg_pdg.record_data.bin";

/// Slice PDG payloads share the same `RBCI` record-pack layout as CFG details.
pub const SLICE_RECORD_INDEX_FILE: &str = "slice.record_index.bin";
pub const SLICE_RECORD_DATA_FILE: &str = "slice.record_data.bin";

const INDEX_MAGIC: [u8; 4] = *b"RBCI";
const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
struct IndexEntry {
    function_id: Uuid,
    offset: u64,
    length: u32,
}

/// Append CFG detail records during streamed export.
pub struct CfgRecordPackWriter {
    data_path: std::path::PathBuf,
    data_file: BufWriter<File>,
    entries: Vec<IndexEntry>,
    offset: u64,
}

impl CfgRecordPackWriter {
    pub fn create(out_dir: &Path) -> Result<Self, String> {
        Self::create_named(out_dir, CFG_RECORD_DATA_FILE)
    }

    /// Create a record pack writing to a custom data filename (e.g. slice pack).
    pub fn create_named(out_dir: &Path, data_file_name: &str) -> Result<Self, String> {
        let data_path = out_dir.join(data_file_name);
        if data_path.exists() {
            std::fs::remove_file(&data_path).map_err(|e| e.to_string())?;
        }
        let data_file = File::create(&data_path).map_err(|e| e.to_string())?;
        Ok(Self {
            data_path,
            data_file: BufWriter::with_capacity(8 * 1024 * 1024, data_file),
            entries: Vec::new(),
            offset: 0,
        })
    }

    #[allow(dead_code)] // Used by unit tests; production path uses `append_bytes`.
    pub fn append(&mut self, function_id: Uuid, detail: &CfgDetailPayload) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(detail).map_err(|e| format!("cfg detail json encode: {e}"))?;
        self.append_bytes(function_id, &bytes)
    }

    /// Append a pre-serialized JSON payload (CFG or slice).
    pub fn append_bytes(&mut self, function_id: Uuid, bytes: &[u8]) -> Result<(), String> {
        let length =
            u32::try_from(bytes.len()).map_err(|_| "record exceeds u32::MAX".to_string())?;
        self.data_file.write_all(bytes).map_err(|e| e.to_string())?;
        self.entries.push(IndexEntry {
            function_id,
            offset: self.offset,
            length,
        });
        self.offset += u64::from(length);
        Ok(())
    }

    pub fn finish(self, out_dir: &Path) -> Result<(), String> {
        self.finish_named(out_dir, CFG_RECORD_INDEX_FILE)
    }

    pub fn finish_named(mut self, out_dir: &Path, index_file_name: &str) -> Result<(), String> {
        self.data_file.flush().map_err(|e| e.to_string())?;
        drop(self.data_file);

        let index_path = out_dir.join(index_file_name);
        let index_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&index_path)
            .map_err(|e| e.to_string())?;
        let mut index_file = BufWriter::with_capacity(1024 * 1024, index_file);

        index_file
            .write_all(&INDEX_MAGIC)
            .map_err(|e| e.to_string())?;
        index_file
            .write_all(&INDEX_VERSION.to_le_bytes())
            .map_err(|e| e.to_string())?;
        index_file
            .write_all(&(self.entries.len() as u64).to_le_bytes())
            .map_err(|e| e.to_string())?;

        for entry in &self.entries {
            index_file
                .write_all(entry.function_id.as_bytes())
                .map_err(|e| e.to_string())?;
            index_file
                .write_all(&entry.offset.to_le_bytes())
                .map_err(|e| e.to_string())?;
            index_file
                .write_all(&entry.length.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        index_file.flush().map_err(|e| e.to_string())?;

        let _ = self.data_path;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_export::CfgDetailPayload;
    use tempfile::TempDir;

    fn lookup_record(index: &[u8], data: &[u8], want: Uuid) -> Option<Vec<u8>> {
        if index.len() < 16 || &index[0..4] != INDEX_MAGIC {
            return None;
        }
        let count = u64::from_le_bytes(index[8..16].try_into().ok()?) as usize;
        let mut off = 16usize;
        for _ in 0..count {
            if off + 28 > index.len() {
                return None;
            }
            let id = Uuid::from_bytes(index[off..off + 16].try_into().ok()?);
            let offset = u64::from_le_bytes(index[off + 16..off + 24].try_into().ok()?);
            let length = u32::from_le_bytes(index[off + 24..off + 28].try_into().ok()?);
            off += 28;
            if id == want {
                let start = offset as usize;
                let end = start + length as usize;
                return data.get(start..end).map(|s| s.to_vec());
            }
        }
        None
    }

    #[test]
    fn record_pack_round_trip() {
        let tmp = TempDir::new().unwrap();
        let id = Uuid::new_v4();
        let detail = CfgDetailPayload {
            schema_version: 2,
            function_id: id.to_string(),
            name: "foo".into(),
            file_path: Some("a.rs".into()),
            entry: 0,
            exits: vec![1],
            blocks: vec![],
            edges: vec![],
            idom: None,
            dominance_frontiers: None,
        };

        let mut writer = CfgRecordPackWriter::create(tmp.path()).unwrap();
        writer.append(id, &detail).unwrap();
        writer.finish(tmp.path()).unwrap();

        assert!(tmp.path().join(CFG_RECORD_INDEX_FILE).is_file());
        assert!(tmp.path().join(CFG_RECORD_DATA_FILE).is_file());

        let index = std::fs::read(tmp.path().join(CFG_RECORD_INDEX_FILE)).unwrap();
        assert_eq!(&index[0..4], &INDEX_MAGIC);
        let count = u64::from_le_bytes(index[8..16].try_into().unwrap());
        assert_eq!(count, 1);

        let data = std::fs::read(tmp.path().join(CFG_RECORD_DATA_FILE)).unwrap();
        assert!(!data.is_empty(), "record data file should not be empty");
        let decoded: CfgDetailPayload = serde_json::from_slice(&data).unwrap();
        assert_eq!(decoded.name, "foo");
    }

    #[test]
    fn slice_record_pack_round_trip() {
        let tmp = TempDir::new().unwrap();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let payload_a = br#"{"schema_version":2,"function_id":"a","name":"slice_a"}"#;
        let payload_b = br#"{"schema_version":2,"function_id":"b","name":"slice_b"}"#;

        let mut writer =
            CfgRecordPackWriter::create_named(tmp.path(), SLICE_RECORD_DATA_FILE).unwrap();
        writer.append_bytes(id_a, payload_a).unwrap();
        writer.append_bytes(id_b, payload_b).unwrap();
        writer
            .finish_named(tmp.path(), SLICE_RECORD_INDEX_FILE)
            .unwrap();

        let index = std::fs::read(tmp.path().join(SLICE_RECORD_INDEX_FILE)).unwrap();
        let data = std::fs::read(tmp.path().join(SLICE_RECORD_DATA_FILE)).unwrap();
        assert_eq!(&index[0..4], &INDEX_MAGIC);
        let count = u64::from_le_bytes(index[8..16].try_into().unwrap());
        assert_eq!(count, 2);

        let got_a = lookup_record(&index, &data, id_a).expect("record a");
        let got_b = lookup_record(&index, &data, id_b).expect("record b");
        assert_eq!(got_a, payload_a);
        assert_eq!(got_b, payload_b);
        assert!(lookup_record(&index, &data, Uuid::new_v4()).is_none());
    }
}
