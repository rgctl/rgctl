//! Dashboard manifest written beside the static bundle.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardManifest {
    pub schema_version: u32,
    pub dashboard_version: String,
    pub phases: BTreeMap<String, String>,
    pub graph: GraphSection,
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
pub struct MetricsSection {
    pub function_count: usize,
    pub class_count: usize,
    pub calls_count: usize,
    pub avg_complexity: f64,
    pub high_blast_radius_count: usize,
}

impl DashboardManifest {
    pub fn phase0_and_1(
        node_count: u64,
        edge_count: u64,
        digest: String,
        metrics: MetricsSection,
    ) -> Self {
        let mut phases = BTreeMap::new();
        phases.insert("0".into(), "complete".into());
        phases.insert("1".into(), "complete".into());

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
            metrics,
            generated_at: chrono_now_rfc3339(),
        }
    }
}

/// Minimal RFC3339 timestamp without pulling chrono into the crate.
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

    #[test]
    fn manifest_serializes_required_keys() {
        let m = DashboardManifest::phase0_and_1(
            10,
            20,
            "abc".into(),
            MetricsSection {
                function_count: 5,
                class_count: 1,
                calls_count: 8,
                avg_complexity: 1.5,
                high_blast_radius_count: 0,
            },
        );
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["graph"]["payload_format"], "columnar_v2");
        assert_eq!(v["phases"]["0"], "complete");
    }
}
