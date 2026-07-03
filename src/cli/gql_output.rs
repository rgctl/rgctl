//! Structured GQL CLI JSON response.

use rbuilder_gql::QueryResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Current GQL JSON schema version.
pub const GQL_SCHEMA_VERSION: u32 = 1;

/// One bound variable in a result row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GqlRowBinding {
    /// Variable name from the query pattern.
    pub binding: String,
    /// Matched node name.
    pub node: String,
    /// Node type label.
    #[serde(rename = "type")]
    pub node_type: String,
    /// Source file path when present.
    pub file: Option<String>,
}

/// Top-level GQL JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GqlJsonResponse {
    pub schema_version: u32,
    pub rows: Vec<Vec<GqlRowBinding>>,
    pub count: usize,
    pub explain: bool,
}

/// Serialize a [`QueryResult`] to the CLI JSON shape.
pub fn gql_result_to_json(result: &QueryResult, explain: bool) -> Value {
    let response = gql_response_from_result(result, explain);
    serde_json::to_value(&response).expect("GqlJsonResponse serializes")
}

/// Build a typed response from executor output.
pub fn gql_response_from_result(result: &QueryResult, explain: bool) -> GqlJsonResponse {
    let rows: Vec<Vec<GqlRowBinding>> = result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(name, node)| GqlRowBinding {
                    binding: name.clone(),
                    node: node.name.clone(),
                    node_type: format!("{:?}", node.node_type),
                    file: node.file_path.clone(),
                })
                .collect()
        })
        .collect();
    let count = rows.len();
    GqlJsonResponse {
        schema_version: GQL_SCHEMA_VERSION,
        rows,
        count,
        explain,
    }
}

/// Minimal fixture for schema sanity tests.
pub fn fixture_gql_response() -> GqlJsonResponse {
    GqlJsonResponse {
        schema_version: GQL_SCHEMA_VERSION,
        rows: vec![vec![GqlRowBinding {
            binding: "f".into(),
            node: "main".into(),
            node_type: "Function".into(),
            file: Some("src/main.rs".into()),
        }]],
        count: 1,
        explain: false,
    }
}

pub fn fixture_gql_json() -> Value {
    json!(fixture_gql_response())
}
