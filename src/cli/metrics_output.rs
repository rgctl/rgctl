//! Structured metrics CLI JSON response.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Current metrics JSON schema version.
pub const METRICS_SCHEMA_VERSION: u32 = 1;

/// PageRank section of the metrics payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsPagerankSection {
    pub top: Vec<Value>,
    pub converged: bool,
    pub iterations: usize,
    pub max_delta: f64,
}

/// Community detection section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsCommunitiesSection {
    pub count: usize,
    pub modularity: f64,
    pub assignments: usize,
}

/// Top-level metrics JSON payload (sections present based on flags).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsJsonResponse {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagerank: Option<MetricsPagerankSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betweenness: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communities: Option<MetricsCommunitiesSection>,
}

/// Wrap a legacy metrics object with schema version and normalize keys.
pub fn wrap_metrics_payload(payload: &mut Value) {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "schema_version".into(),
            json!(METRICS_SCHEMA_VERSION),
        );
    }
}

/// Fixture with all sections populated (empty arrays / zeros).
pub fn fixture_metrics_response() -> MetricsJsonResponse {
    MetricsJsonResponse {
        schema_version: METRICS_SCHEMA_VERSION,
        pagerank: Some(MetricsPagerankSection {
            top: vec![json!({ "node": "00000000-0000-0000-0000-000000000001", "pagerank": 0.1 })],
            converged: true,
            iterations: 20,
            max_delta: 1e-9,
        }),
        betweenness: Some(vec![json!({ "node": "00000000-0000-0000-0000-000000000001", "score": 0.5 })]),
        communities: Some(MetricsCommunitiesSection {
            count: 2,
            modularity: 0.42,
            assignments: 10,
        }),
    }
}

pub fn fixture_metrics_json() -> Value {
    serde_json::to_value(fixture_metrics_response()).expect("metrics fixture serializes")
}
