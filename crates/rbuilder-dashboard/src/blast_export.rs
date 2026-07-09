//! Export blast-radius assets for the dashboard bundle (Phase 6).

use rbuilder_analysis::AnalysisResults;
use rbuilder_graph::snapshot::MmappedGraphSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub const BLAST_INDEX_FILE: &str = "blast_index.json";
pub const BLAST_SNAPSHOT_BUNDLE_NAME: &str = "blast_engine.snapshot.bin";

const HEADER_SIZE: usize = 136;
const NODE_ROW_SIZE: usize = 64;
const FUNCTION_NODE_TYPE: u16 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastFunctionScore {
    pub index: u32,
    pub score: f32,
    pub direct: u32,
    pub zone: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastIndexPayload {
    pub schema_version: u32,
    /// WASM reverse call-graph blast is always available when graph_payload exists.
    pub available: bool,
    pub snapshot_path: Option<String>,
    pub snapshot_copied: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<BlastFunctionScore>,
}

#[derive(Debug, Default)]
pub struct BlastExportSummary {
    pub available: bool,
    pub snapshot_copied: bool,
    pub function_scores: usize,
}

pub fn export_blast_bundle(repo_root: &Path, out_dir: &Path) -> Result<BlastExportSummary, String> {
    let snapshot_path = MmappedGraphSnapshot::default_path(repo_root);
    let engine_snapshot_path = rbuilder_analysis::blast_engine_snapshot::BlastEngineSnapshot::default_path(repo_root);
    let mut snapshot_copied = false;

    if engine_snapshot_path.is_file() {
        let dest = out_dir.join(BLAST_SNAPSHOT_BUNDLE_NAME);
        fs::copy(&engine_snapshot_path, &dest).map_err(|e| e.to_string())?;
        snapshot_copied = true;
    }

    let functions = export_function_scores(repo_root, &snapshot_path)?;

    let index = BlastIndexPayload {
        schema_version: 2,
        available: true,
        snapshot_path: if snapshot_copied {
            Some(BLAST_SNAPSHOT_BUNDLE_NAME.into())
        } else {
            None
        },
        snapshot_copied,
        functions: functions.clone(),
    };
    let json = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(BLAST_INDEX_FILE), json).map_err(|e| e.to_string())?;

    Ok(BlastExportSummary {
        available: true,
        snapshot_copied,
        function_scores: functions.len(),
    })
}

fn export_function_scores(
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<Vec<BlastFunctionScore>, String> {
    let analysis_path = repo_root.join(".rbuilder/analysis_results.bin");
    if !analysis_path.is_file() || !snapshot_path.is_file() {
        return Ok(Vec::new());
    }

    let results = AnalysisResults::load(&analysis_path).map_err(|e| e.to_string())?;
    let Some(blast) = results.blast_radius.as_ref() else {
        return Ok(Vec::new());
    };

    let bytes = fs::read(snapshot_path).map_err(|e| e.to_string())?;
    let uuid_to_index = scan_columnar_uuid_indices(&bytes)?;
    let mut scores = Vec::new();

    for compact_id in 0..results.node_count() {
        let Some(uuid) = results.get_uuid(compact_id as u32) else {
            continue;
        };
        let Some(&index) = uuid_to_index.get(&uuid) else {
            continue;
        };
        if columnar_node_type(&bytes, index)? != FUNCTION_NODE_TYPE {
            continue;
        }
        let score = blast.scores[compact_id];
        let direct = blast.direct_callers[compact_id];
        let zone = blast.impact_zone_size[compact_id];
        if score <= 0.0 && direct == 0 && zone == 0 {
            continue;
        }
        scores.push(BlastFunctionScore {
            index,
            score,
            direct,
            zone,
        });
    }

    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.index.cmp(&b.index))
    });
    Ok(scores)
}

pub(crate) fn scan_columnar_uuid_indices(bytes: &[u8]) -> Result<HashMap<Uuid, u32>, String> {
    if bytes.len() < HEADER_SIZE || &bytes[0..4] != b"RBGR" {
        return Err("not columnar v2".into());
    }
    let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    let offset_nodes = u64::from_le_bytes(bytes[92..100].try_into().unwrap()) as usize;
    let end = offset_nodes + node_count * NODE_ROW_SIZE;
    if end > bytes.len() {
        return Err("node column out of range".into());
    }
    let mut map = HashMap::with_capacity(node_count);
    for idx in 0..node_count {
        let off = offset_nodes + idx * NODE_ROW_SIZE;
        let id: [u8; 16] = bytes[off..off + 16].try_into().unwrap();
        map.insert(Uuid::from_bytes(id), idx as u32);
    }
    Ok(map)
}

fn columnar_node_type(bytes: &[u8], index: u32) -> Result<u16, String> {
    let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
    let offset_nodes = u64::from_le_bytes(bytes[92..100].try_into().unwrap()) as usize;
    let idx = index as usize;
    if idx >= node_count {
        return Err(format!("node index {index} out of range"));
    }
    let off = offset_nodes + idx * NODE_ROW_SIZE + 16;
    Ok(u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap()))
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
        assert_eq!(summary.function_scores, 0);
        let index: BlastIndexPayload =
            serde_json::from_slice(&fs::read(out.join(BLAST_INDEX_FILE)).unwrap()).unwrap();
        assert!(index.available);
        assert!(index.snapshot_path.is_none());
        assert!(index.functions.is_empty());
    }

    #[test]
    fn export_blast_scores_from_gbuilder_when_present() {
        let repo = Path::new("/Users/sshaaf/git/java/gbuilder");
        if !repo.join(".rbuilder/analysis_results.bin").is_file() {
            return;
        }
        let out = tempfile::tempdir().unwrap();
        let summary = export_blast_bundle(repo, out.path()).unwrap();
        assert!(summary.function_scores > 0, "expected blast function scores");
        let index: BlastIndexPayload =
            serde_json::from_slice(&fs::read(out.path().join(BLAST_INDEX_FILE)).unwrap()).unwrap();
        assert!(!index.functions.is_empty());
        for pair in index.functions.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    #[test]
    #[ignore = "dev: refresh gbuilder blast_index.json"]
    fn refresh_gbuilder_blast_index() {
        let repo = Path::new("/Users/sshaaf/git/java/gbuilder");
        let out = repo.join(".rbuilder/dashboard");
        let summary = export_blast_bundle(repo, &out).unwrap();
        eprintln!("exported {} function scores", summary.function_scores);
        assert!(summary.function_scores > 0);
    }
}
