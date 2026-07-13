//! On-demand CFG previews for large repos: compact record index + data pack.
//!
//! Built during dashboard export when per-function JSON is omitted. The browser
//! fetches one record at a time via HTTP Range on `cfg_pdg.record_data.bin`.

use crate::cfg_export::CfgDetailPayload;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use uuid::Uuid;

pub const CFG_RECORD_INDEX_FILE: &str = "cfg_pdg.record_index.bin";
pub const CFG_RECORD_DATA_FILE: &str = "cfg_pdg.record_data.bin";

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
    data_file: File,
    entries: Vec<IndexEntry>,
    offset: u64,
}

impl CfgRecordPackWriter {
    pub fn create(out_dir: &Path) -> Result<Self, String> {
        let data_path = out_dir.join(CFG_RECORD_DATA_FILE);
        if data_path.exists() {
            std::fs::remove_file(&data_path).map_err(|e| e.to_string())?;
        }
        let data_file = File::create(&data_path).map_err(|e| e.to_string())?;
        Ok(Self {
            data_path,
            data_file,
            entries: Vec::new(),
            offset: 0,
        })
    }

    pub fn append(&mut self, function_id: Uuid, detail: &CfgDetailPayload) -> Result<(), String> {
        let bytes =
            serde_json::to_vec(detail).map_err(|e| format!("cfg detail json encode: {e}"))?;
        let length = u32::try_from(bytes.len())
            .map_err(|_| "cfg detail record exceeds u32::MAX".to_string())?;
        self.data_file
            .write_all(&bytes)
            .map_err(|e| e.to_string())?;
        self.entries.push(IndexEntry {
            function_id,
            offset: self.offset,
            length,
        });
        self.offset += u64::from(length);
        Ok(())
    }

    pub fn finish(mut self, out_dir: &Path) -> Result<(), String> {
        self.data_file.flush().map_err(|e| e.to_string())?;
        drop(self.data_file);

        let index_path = out_dir.join(CFG_RECORD_INDEX_FILE);
        let mut index_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&index_path)
            .map_err(|e| e.to_string())?;

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

        let _ = self.data_path;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg_export::CfgDetailPayload;
    use tempfile::TempDir;

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
}
