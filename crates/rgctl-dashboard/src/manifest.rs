//! Dashboard manifest written beside the static bundle.

use crate::blast_export::BlastExportSummary;
use crate::cfg_export::CfgExportSummary;
use crate::dataflow_export::DataflowExportSummary;
use crate::metagraph::MetagraphExport;
use crate::migration_export::MigrationExportSummary;
use crate::mutations_export::MutationsExportSummary;
use crate::slice_export::SliceExportSummary;
use crate::taint_export::TaintExportSummary;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSection {
    pub available: bool,
    pub functions_indexed: usize,
    pub model_id: String,
    pub dimensions: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraSection {
    pub available: bool,
    pub violation_count: usize,
    pub evaluated_rules: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub by_category: std::collections::HashMap<String, usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruleset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_filter: Option<String>,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub rule_count: usize,
}

/// Composed workflow documents copied into the bundle beside `manifest.json`.
///
/// These are not analysis outputs of *this* graph. A review artifact is composed from a **pair** of
/// revisions (a base and a head), so it describes a different revision pair than the bundle's graph —
/// which is why it is a section of its own rather than a field under `analysis`, and why
/// `review_head_digest` is recorded: a consumer can then tell whether the artifact describes the graph
/// in this bundle or another one, instead of presenting foreign numbers as current.
///
/// `available: false` is recorded explicitly rather than by omitting the section, so a consumer can
/// distinguish "there is no artifact here" from "this manifest predates the field".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowsSection {
    pub available: bool,
    /// Bundle-relative path, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_path: Option<String>,
    /// BLAKE3 of the copied artifact, so re-export can notice that only the artifact changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_digest: Option<String>,
    /// The artifact's own `generated_at`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_generated_at: Option<String>,
    /// The artifact's `provenance.graph.head_digest`, for comparison against `graph.digest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_head_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardManifest {
    pub schema_version: u32,
    pub dashboard_version: String,
    pub phases: BTreeMap<String, String>,
    pub graph: GraphSection,
    pub view: ViewSection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SemanticSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kantra: Option<KantraSection>,
    /// Composed workflow documents carried in the bundle (`workflows/review.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflows: Option<WorkflowsSection>,
    pub metrics: MetricsSection,
    pub generated_at: String,
    /// Stable fingerprint for incremental dashboard export (semantic, not volatile UUID digest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_fingerprint: Option<String>,
    /// Which dashboard bundle these assets came from: `"v1"` or `"v2"`.
    ///
    /// The export is skipped when the fingerprint matches, and that fingerprint is derived from the
    /// graph — so it says nothing about which bundle was extracted. Selecting a different bundle
    /// therefore matched and returned early, leaving the previous bundle in `.rgctl/dashboard/`: the
    /// operator believed they were looking at v2 while the extracted files were still v1.
    ///
    /// `Option` with a default so a manifest written before this field existed still deserialises —
    /// and compares unequal, which forces one re-export and is the correct outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_version: Option<String>,
    /// What this bundle *is*, for anything that serves it: `"universe"` for the v2 dashboard,
    /// `"dashboard"` for the pre-universe one served through the legacy hatch.
    ///
    /// Distinct from `bundle_version`, which says which files were extracted. A server that opens
    /// whatever the manifest declares needs the mode, and inferring it from a version string would put
    /// that knowledge in two places.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communities_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communities_schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_count: Option<u32>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_metrics_path: Option<String>,
    pub blast_snapshot_path: Option<String>,
    pub dataflow_available: bool,
    pub dataflow_index_path: String,
    pub dataflow_detail_dir: String,
    pub dataflow_function_count: usize,
    /// Hybrid CPG field mutations (`mutations_index.json` from `field_write.index.bin`).
    #[serde(default)]
    pub mutations_available: bool,
    #[serde(default = "default_mutations_index_path")]
    pub mutations_index_path: String,
    #[serde(default)]
    pub mutations_write_count: usize,
    #[serde(default)]
    pub mutations_type_count: usize,
    pub taint_available: bool,
    pub taint_index_path: String,
    pub taint_detail_dir: String,
    pub taint_function_count: usize,
    pub taint_flow_count: usize,
    pub taint_vulnerable_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_graph_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_plan_path: Option<String>,
    pub migration_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_community_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSection {
    pub function_count: usize,
    pub class_count: usize,
    pub calls_count: usize,
    pub avg_complexity: f64,
    pub high_blast_radius_count: usize,
}

fn default_mutations_index_path() -> String {
    crate::mutations_export::MUTATIONS_INDEX_FILE.into()
}

impl DashboardManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn with_phases(
        node_count: u64,
        edge_count: u64,
        digest: String,
        export_fingerprint: String,
        metrics: MetricsSection,
        export: &MetagraphExport,
        cfg: &CfgExportSummary,
        slice: &SliceExportSummary,
        blast: &BlastExportSummary,
        dataflow: &DataflowExportSummary,
        mutations: &MutationsExportSummary,
        taint: &TaintExportSummary,
        migration: &MigrationExportSummary,
        semantic: Option<SemanticSection>,
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
        phases.insert(
            "7".into(),
            if dataflow.available {
                "complete".into()
            } else {
                "pending".into()
            },
        );
        phases.insert(
            "8".into(),
            if taint.available {
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
            function_metrics_path: Some(
                crate::function_metrics_export::FUNCTION_METRICS_FILE.into(),
            ),
            blast_snapshot_path: if blast.snapshot_copied {
                Some(crate::blast_export::BLAST_SNAPSHOT_BUNDLE_NAME.into())
            } else {
                None
            },
            dataflow_available: dataflow.available,
            dataflow_index_path: crate::dataflow_export::DATAFLOW_INDEX_FILE.into(),
            dataflow_detail_dir: crate::slice_export::SLICE_DETAIL_DIR.into(),
            dataflow_function_count: dataflow.function_count,
            mutations_available: mutations.available,
            mutations_index_path: crate::mutations_export::MUTATIONS_INDEX_FILE.into(),
            mutations_write_count: mutations.write_count,
            mutations_type_count: mutations.type_count,
            taint_available: taint.available,
            taint_index_path: crate::taint_export::TAINT_INDEX_FILE.into(),
            taint_detail_dir: crate::taint_export::TAINT_DETAIL_DIR.into(),
            taint_function_count: taint.function_count,
            taint_flow_count: taint.total_flows,
            taint_vulnerable_count: taint.vulnerable_flows,
            migration_graph_path: if migration.available {
                Some(crate::migration_export::MIGRATION_GRAPH_FILE.into())
            } else {
                None
            },
            migration_plan_path: if migration.available {
                Some(crate::migration_export::MIGRATION_PLAN_FILE.into())
            } else {
                None
            },
            migration_available: migration.available,
            migration_community_count: if migration.available {
                Some(migration.community_count as u32)
            } else {
                None
            },
        });

        let meta = &export.meta;
        let communities = &export.communities;

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
                communities_path: if communities.communities.is_empty() {
                    None
                } else {
                    Some(crate::communities::COMMUNITIES_FILE.into())
                },
                communities_schema_version: if communities.communities.is_empty() {
                    None
                } else {
                    Some(communities.schema_version)
                },
                community_count: if communities.communities.is_empty() {
                    None
                } else {
                    Some(communities.communities.len() as u32)
                },
            },
            analysis,
            semantic,
            kantra: None,
            workflows: None,
            metrics,
            generated_at: chrono_now_rfc3339(),
            export_fingerprint: Some(export_fingerprint),
            mode: Some(crate::bundle::selected_mode().to_string()),
            bundle_version: Some(crate::bundle::selected_version_tag().to_string()),
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
    use crate::communities::CommunitiesPayload;
    use crate::metagraph::{COMMUNITY_ONLY_THRESHOLD, MetagraphExport, MetagraphPayload, Metanode};
    use crate::migration_export::MigrationExportSummary;

    #[test]
    fn manifest_includes_view_section() {
        let meta = MetagraphPayload {
            schema_version: 3,
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
                community_id: Some(0),
            }],
            edges: vec![],
        };
        let export = MetagraphExport {
            meta,
            communities: CommunitiesPayload {
                schema_version: 1,
                modularity: 0.5,
                communities: vec![crate::communities::CommunitySummary {
                    id: 0,
                    label: "Community 0".into(),
                    color: "#5a8fd4".into(),
                    member_count: 10,
                    package_count: 1,
                }],
            },
        };
        let m = DashboardManifest::with_phases(
            100,
            200,
            "abc".into(),
            "fp".into(),
            MetricsSection {
                function_count: 8,
                class_count: 2,
                calls_count: 5,
                avg_complexity: 1.0,
                high_blast_radius_count: 0,
            },
            &export,
            &CfgExportSummary::default(),
            &SliceExportSummary::default(),
            &BlastExportSummary::default(),
            &DataflowExportSummary::default(),
            &MutationsExportSummary::default(),
            &TaintExportSummary::default(),
            &MigrationExportSummary::default(),
            None,
        );
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["phases"]["2"], "complete");
        assert_eq!(v["phases"]["4"], "pending");
        assert_eq!(v["phases"]["5"], "pending");
        assert_eq!(v["phases"]["6"], "pending");
        assert_eq!(v["phases"]["7"], "pending");
        assert_eq!(v["phases"]["8"], "pending");
        assert_eq!(v["view"]["metanode_count"], 1);
        assert_eq!(v["view"]["communities_path"], "communities.json");
        assert_eq!(v["view"]["community_count"], 1);
        assert_eq!(v["analysis"]["cfg_available"], false);
        assert_eq!(v["analysis"]["slice_available"], false);
        assert_eq!(v["analysis"]["blast_available"], false);
        assert_eq!(v["analysis"]["dataflow_available"], false);
        assert_eq!(v["analysis"]["taint_available"], false);
        // `with_phases` reads no filesystem, so it leaves the section unset. The *export* always sets it —
        // to `available: false` when the repository has no artifact — which is what lets a consumer tell
        // "there is none" from "this manifest predates the field". Asserted end-to-end in
        // `tests/dashboard_bundle.rs`; here the constructor's behaviour is what is pinned.
        assert!(v["workflows"].is_null());
    }

    #[test]
    fn workflows_section_records_the_artifact_identity() {
        let section = WorkflowsSection {
            available: true,
            review_path: Some("workflows/review.json".into()),
            review_digest: Some("deadbeef".into()),
            review_generated_at: Some("1792080101".into()),
            review_head_digest: Some("0710".into()),
        };
        let v = serde_json::to_value(&section).unwrap();
        assert_eq!(v["available"], true);
        assert_eq!(v["review_path"], "workflows/review.json");
        assert_eq!(v["review_head_digest"], "0710");

        // An absent artifact must not serialise empty-string placeholders that read as values.
        let none = WorkflowsSection {
            available: false,
            review_path: None,
            review_digest: None,
            review_generated_at: None,
            review_head_digest: None,
        };
        let v = serde_json::to_value(&none).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 1);
    }
}
