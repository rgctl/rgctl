//! Export blast-radius assets for the dashboard bundle (Phase 6).

use rbuilder_analysis::blast_engine_snapshot::BlastEngineSnapshot;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const BLAST_INDEX_FILE: &str = "blast_index.json";
pub const BLAST_SNAPSHOT_BUNDLE_NAME: &str = "blast_engine.snapshot.bin";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastIndexPayload {
    pub schema_version: u32,
    /// WASM reverse call-graph blast is always available when graph_payload exists.
    pub available: bool,
    pub snapshot_path: Option<String>,
    pub snapshot_copied: bool,
}

#[derive(Debug, Default)]
pub struct BlastExportSummary {
    pub available: bool,
    pub snapshot_copied: bool,
}

pub fn export_blast_bundle(repo_root: &Path, out_dir: &Path) -> Result<BlastExportSummary, String> {
    let snapshot_path = BlastEngineSnapshot::default_path(repo_root);
    let mut snapshot_copied = false;

    if snapshot_path.is_file() {
        let dest = out_dir.join(BLAST_SNAPSHOT_BUNDLE_NAME);
        fs::copy(&snapshot_path, &dest).map_err(|e| e.to_string())?;
        snapshot_copied = true;
    }

    let index = BlastIndexPayload {
        schema_version: 1,
        available: true,
        snapshot_path: if snapshot_copied {
            Some(BLAST_SNAPSHOT_BUNDLE_NAME.into())
        } else {
            None
        },
        snapshot_copied,
    };
    let json = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(BLAST_INDEX_FILE), json).map_err(|e| e.to_string())?;

    Ok(BlastExportSummary {
        available: true,
        snapshot_copied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blast_index_written_without_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("dash");
        fs::create_dir_all(&out).unwrap();
        let summary = export_blast_bundle(tmp.path(), &out).unwrap();
        assert!(summary.available);
        assert!(!summary.snapshot_copied);
        let index: BlastIndexPayload =
            serde_json::from_slice(&fs::read(out.join(BLAST_INDEX_FILE)).unwrap()).unwrap();
        assert!(index.available);
        assert!(index.snapshot_path.is_none());
    }
}
