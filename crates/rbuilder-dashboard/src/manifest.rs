//! Dashboard manifest written beside the static bundle.

use crate::blast_export::BlastExportSummary;
use crate::cfg_export::CfgExportSummary;
use crate::metagraph::MetagraphPayload;
use crate::slice_export::SliceExportSummary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardManifest {
    pub schema_version: u32,
    pub dashboard_version: String,
    pub phases: BTreeMap<String, String>,
    pub graph: GraphSection,
    pub view: ViewSection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisSection>,
    pub metrics: MetricsSection,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSection {
    pub payload_path: String,
    pub payload_format: String,
    pub node_count: u64,
    pub edge_count: u64,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSection {
    pub metagraph_path: String,
    pub metagraph_schema_version: u32,
    pub metanode_count: u32,
    pub metaedge_count: u32,
    pub mode: String,
    pub community_only: bool,
    pub threshold_community_only: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSection {
    pub cfg_available: bool,
    pub cfg_index_path: String,
    pub cfg_detail_dir: String,
    pub cfg_archive_path: Option<String>,
    pub cfg_function_count: usize,
    pub slice_available: bool,
    pub slice_index_path: String,
    pub slice_detail_dir: String,
    pub slice_function_count: usize,
    pub blast_available: bool,
    pub blast_index_path: String,
    pub blast_snapshot_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSection {
    pub function_count: usize,
    pub class_count: usize,
    pub calls_count: usize,
    pub avg_complexity: f64,
    pub high_blast_radius_count: usize,
}

impl DashboardManifest {
    pub fn with_phases(
        node_count: u64,
        edge_count: u64,
        digest: String,
        metrics: MetricsSection,
        meta: &MetagraphPayload,
        cfg: &CfgExportSummary,
        slice: &SliceExportSummary,
        blast: &BlastExportSummary,
    ) -> Self {
        let mut phases = BTreeMap::new();
        phases.insert("0".into(), "complete".into());
        phases.insert("1".into(), "complete".into());
        phases.insert("2".into(), "complete".into());
        phases.insert("3".into(), "complete".into());
        phases.insert(
            "4".into(),
            if cfg.available {
                "complete".into()
            } else {
                "pending".into()
            },
        );
        phases.insert(
            "5".into(),
            if slice.available {
                "complete".into()
            } else {
                "pending".into()
            },
        );
        phases.insert(
            "6".into(),
            if blast.available {
                "complete".into()
            } else {
                "pending".into()
            },
        );

        let analysis = Some(AnalysisSection {
            cfg_available: cfg.available,
            cfg_index_path: crate::cfg_export::CFG_INDEX_FILE.into(),
            cfg_detail_dir: crate::cfg_export::CFG_DETAIL_DIR.into(),
            cfg_archive_path: if cfg.archive_copied {
                Some(crate::cfg_export::CFG_ARCHIVE_BUNDLE_NAME.into())
            } else {
                None
            },
            cfg_function_count: cfg.function_count,
            slice_available: slice.available,
            slice_index_path: crate::slice_export::SLICE_INDEX_FILE.into(),
            slice_detail_dir: crate::slice_export::SLICE_DETAIL_DIR.into(),
            slice_function_count: slice.function_count,
            blast_available: blast.available,
            blast_index_path: crate::blast_export::BLAST_INDEX_FILE.into(),
            blast_snapshot_path: if blast.snapshot_copied {
                Some(crate::blast_export::BLAST_SNAPSHOT_BUNDLE_NAME.into())
            } else {
                None
            },
        });

        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            dashboard_version: env!("CARGO_PKG_VERSION").into(),
            phases,
            graph: GraphSection {
                payload_path: "graph_payload.bin".into(),
                payload_format: "columnar_v2".into(),
                node_count,
                edge_count,
                digest,
            },
            view: ViewSection {
                metagraph_path: crate::metagraph::METAGRAPH_FILE.into(),
                metagraph_schema_version: meta.schema_version,
                metanode_count: meta.nodes.len() as u32,
                metaedge_count: meta.edges.len() as u32,
                mode: meta.mode.clone(),
                community_only: meta.community_only,
                threshold_community_only: meta.threshold_community_only,
            },
            analysis,
            metrics,
            generated_at: chrono_now_rfc3339(),
        }
    }
}

fn chrono_now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metagraph::{Metanode, MetagraphPayload, COMMUNITY_ONLY_THRESHOLD};

    #[test]
    fn manifest_includes_view_section() {
        let meta = MetagraphPayload {
            schema_version: 1,
            mode: "package_metagraph".into(),
            community_only: false,
            threshold_community_only: COMMUNITY_ONLY_THRESHOLD,
            source_node_count: 100,
            nodes: vec![Metanode {
                id: 0,
                label: "com.example".into(),
                size: 10,
                functions: 8,
                classes: 2,
                avg_complexity: 1.0,
                x: 0.0,
                y: 0.0,
                member_indices: vec![0, 1],
            }],
            edges: vec![],
        };
        let m = DashboardManifest::with_phases(
            100,
            200,
            "abc".into(),
            MetricsSection {
                function_count: 8,
                class_count: 2,
                calls_count: 5,
                avg_complexity: 1.0,
                high_blast_radius_count: 0,
            },
            &meta,
            &CfgExportSummary::default(),
            &SliceExportSummary::default(),
            &BlastExportSummary::default(),
        );
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["phases"]["2"], "complete");
        assert_eq!(v["phases"]["4"], "pending");
        assert_eq!(v["phases"]["5"], "pending");
        assert_eq!(v["phases"]["6"], "pending");
        assert_eq!(v["view"]["metanode_count"], 1);
        assert_eq!(v["analysis"]["cfg_available"], false);
        assert_eq!(v["analysis"]["slice_available"], false);
        assert_eq!(v["analysis"]["blast_available"], false);
    }
}
