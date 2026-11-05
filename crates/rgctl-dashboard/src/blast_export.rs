//! Export blast-radius assets for the dashboard bundle (Phase 6).

use crate::bundle::{BundleVersion, selected_version};
use crate::export_context::{DashboardExportContext, resolve_analysis};
use crate::export_util::{link_or_copy, write_json_compact};
use rayon::prelude::*;
use rgctl_graph::graph_source::GraphSource;
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

#[allow(dead_code)] // Convenience wrapper used in unit tests; production uses `_with_graph`.
pub fn export_blast_bundle(
    repo_root: &Path,
    snapshot_path: &Path,
    out_dir: &Path,
    ctx: DashboardExportContext<'_>,
    uuid_to_index: &HashMap<Uuid, u32>,
) -> Result<BlastExportSummary, String> {
    export_blast_bundle_with_graph(None, repo_root, snapshot_path, out_dir, ctx, uuid_to_index)
}

pub fn export_blast_bundle_with_graph(
    graph: Option<GraphSource<'_>>,
    repo_root: &Path,
    snapshot_path: &Path,
    out_dir: &Path,
    ctx: DashboardExportContext<'_>,
    uuid_to_index: &HashMap<Uuid, u32>,
) -> Result<BlastExportSummary, String> {
    let engine_snapshot_path =
        rgctl_analysis::blast_engine_snapshot::BlastEngineSnapshot::default_path(repo_root);
    // V2 universe UI does not consume the engine snapshot; skip the ~27 MB copy/link.
    let mut snapshot_copied = false;
    if selected_version() == BundleVersion::V1 && engine_snapshot_path.is_file() {
        let dest = out_dir.join(BLAST_SNAPSHOT_BUNDLE_NAME);
        link_or_copy(&engine_snapshot_path, &dest)?;
        snapshot_copied = true;
    }

    let functions = export_function_scores(graph, repo_root, ctx, snapshot_path, uuid_to_index)?;

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
    write_json_compact(&out_dir.join(BLAST_INDEX_FILE), &index)?;

    Ok(BlastExportSummary {
        available: true,
        snapshot_copied,
        function_scores: functions.len(),
    })
}

fn export_function_scores(
    graph: Option<GraphSource<'_>>,
    repo_root: &Path,
    ctx: DashboardExportContext<'_>,
    snapshot_path: &Path,
    uuid_to_index: &HashMap<Uuid, u32>,
) -> Result<Vec<BlastFunctionScore>, String> {
    let columnar = graph.and_then(|g| g.as_mmap()).and_then(|s| s.columnar());
    if columnar.is_none() && !snapshot_path.is_file() {
        return Ok(Vec::new());
    }

    let results = match resolve_analysis(&ctx, repo_root) {
        Ok(cow) => cow,
        Err(_) => return Ok(Vec::new()),
    };
    let Some(blast) = results.blast_radius.as_ref() else {
        return Ok(Vec::new());
    };

    let owned_mmap = if columnar.is_none() {
        Some(mmap_snapshot(snapshot_path)?)
    } else {
        None
    };
    let node_count = results.node_count();

    let scored: Result<Vec<Option<BlastFunctionScore>>, String> = (0..node_count)
        .into_par_iter()
        .map(|compact_id| {
            let Some(uuid) = results.get_uuid(compact_id as u32) else {
                return Ok(None);
            };
            let Some(&index) = uuid_to_index.get(&uuid) else {
                return Ok(None);
            };
            let is_fn = match (columnar, owned_mmap.as_ref()) {
                (Some(col), _) => matches!(
                    col.node_type_at(index as usize)
                        .map_err(|e| e.to_string())?,
                    rgctl_graph::schema::NodeType::Function
                ),
                (None, Some(bytes)) => columnar_node_type(bytes, index)? == FUNCTION_NODE_TYPE,
                (None, None) => false,
            };
            if !is_fn {
                return Ok(None);
            }
            let score = blast.scores[compact_id];
            let direct = blast.direct_callers[compact_id];
            let zone = blast.impact_zone_size[compact_id];
            if score <= 0.0 && direct == 0 && zone == 0 {
                return Ok(None);
            }
            Ok(Some(BlastFunctionScore {
                index,
                score,
                direct,
                zone,
            }))
        })
        .collect();
    let mut scores: Vec<BlastFunctionScore> = scored?.into_iter().flatten().collect();

    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.index.cmp(&b.index))
    });
    Ok(scores)
}

/// Build UUID → row index from an already-opened [`GraphSource`] when backed by a columnar
/// mmap, or memory-map `snapshot_path` as a fallback.
pub fn load_columnar_uuid_indices_from_graph(
    graph: GraphSource<'_>,
    snapshot_path: &Path,
) -> Result<HashMap<Uuid, u32>, String> {
    if let Some(col) = graph.as_mmap().and_then(|s| s.columnar()) {
        let mut map = HashMap::with_capacity(col.node_count());
        for (idx, id) in col.node_ids_by_index() {
            map.insert(id, idx as u32);
        }
        return Ok(map);
    }
    if snapshot_path.is_file() {
        return load_columnar_uuid_indices(snapshot_path);
    }
    Ok(HashMap::new())
}

/// Memory-map columnar snapshot and build UUID → row index (shared across exporters).
pub fn load_columnar_uuid_indices(snapshot_path: &Path) -> Result<HashMap<Uuid, u32>, String> {
    let bytes = mmap_snapshot(snapshot_path)?;
    scan_columnar_uuid_indices(&bytes)
}

fn mmap_snapshot(snapshot_path: &Path) -> Result<memmap2::Mmap, String> {
    let file = fs::File::open(snapshot_path).map_err(|e| e.to_string())?;
    // SAFETY: read-only map for parsing columnar header/columns.
    unsafe { memmap2::Mmap::map(&file).map_err(|e| e.to_string()) }
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
    use crate::export_context::DashboardExportContext;
    use rgctl_graph::snapshot::MmappedGraphSnapshot;

    #[test]
    fn blast_index_written_without_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("dash");
        fs::create_dir_all(&out).unwrap();
        let summary = export_blast_bundle(
            tmp.path(),
            &tmp.path().join("missing.snapshot.bin"),
            &out,
            DashboardExportContext::default(),
            &HashMap::new(),
        )
        .unwrap();
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
    fn blast_snapshot_copy_gated_on_bundle_v1() {
        use crate::bundle::{BundleVersion, set_bundle_override};

        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let engine_dir = repo.join(".rgctl/analysis");
        fs::create_dir_all(&engine_dir).unwrap();
        let engine_snap =
            rgctl_analysis::blast_engine_snapshot::BlastEngineSnapshot::default_path(repo);
        fs::write(&engine_snap, b"fake-blast-engine").unwrap();

        let out_v2 = repo.join("dash-v2");
        fs::create_dir_all(&out_v2).unwrap();
        set_bundle_override(Some(BundleVersion::V2));
        let summary_v2 = export_blast_bundle(
            repo,
            &repo.join("missing.snapshot.bin"),
            &out_v2,
            DashboardExportContext::default(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(!summary_v2.snapshot_copied);
        assert!(!out_v2.join(BLAST_SNAPSHOT_BUNDLE_NAME).exists());

        let out_v1 = repo.join("dash-v1");
        fs::create_dir_all(&out_v1).unwrap();
        set_bundle_override(Some(BundleVersion::V1));
        let summary_v1 = export_blast_bundle(
            repo,
            &repo.join("missing.snapshot.bin"),
            &out_v1,
            DashboardExportContext::default(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(summary_v1.snapshot_copied);
        assert!(out_v1.join(BLAST_SNAPSHOT_BUNDLE_NAME).is_file());

        set_bundle_override(None);
    }

    #[test]
    fn export_blast_scores_from_gbuilder_when_present() {
        let repo = Path::new("/Users/sshaaf/git/java/gbuilder");
        if !repo.join(".rgctl/analysis_results.bin").is_file() {
            return;
        }
        let Ok(results) = resolve_analysis(&DashboardExportContext::default(), repo) else {
            return;
        };
        let Some(blast) = results.blast_radius.as_ref() else {
            return;
        };
        let snapshot_path = MmappedGraphSnapshot::default_path(repo);
        let Ok(uuid_to_index) = load_columnar_uuid_indices(&snapshot_path) else {
            return;
        };
        let Ok(expected) = export_function_scores(
            None,
            repo,
            DashboardExportContext::default(),
            &snapshot_path,
            &uuid_to_index,
        ) else {
            return;
        };
        if expected.is_empty() || blast.scores.is_empty() {
            return;
        }
        let out = tempfile::tempdir().unwrap();
        let summary = export_blast_bundle(
            repo,
            &snapshot_path,
            out.path(),
            DashboardExportContext::default(),
            &uuid_to_index,
        )
        .unwrap();
        assert!(
            summary.function_scores > 0,
            "expected blast function scores"
        );
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
        let out = repo.join(".rgctl/dashboard");
        let snapshot_path = MmappedGraphSnapshot::default_path(repo);
        let uuid_to_index = load_columnar_uuid_indices(&snapshot_path).unwrap();
        let summary = export_blast_bundle(
            repo,
            &snapshot_path,
            &out,
            DashboardExportContext::default(),
            &uuid_to_index,
        )
        .unwrap();
        eprintln!("exported {} function scores", summary.function_scores);
        assert!(summary.function_scores > 0);
    }
}
