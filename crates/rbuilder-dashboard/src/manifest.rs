//! Dashboard manifest written beside the static bundle.

use crate::metagraph::MetagraphPayload;
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
    ) -> Self {
        let mut phases = BTreeMap::new();
        phases.insert("0".into(), "complete".into());
        phases.insert("1".into(), "complete".into());
        phases.insert("2".into(), "complete".into());
        phases.insert("3".into(), "complete".into());

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
        );
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["phases"]["2"], "complete");
        assert_eq!(v["view"]["metanode_count"], 1);
    }
}
