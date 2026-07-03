//! Structured check CLI JSON response.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current check JSON schema version.
pub const CHECK_SCHEMA_VERSION: u32 = 1;

/// One policy violation or engine error entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckViolationEntry {
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub violation: Option<String>,
}

/// Top-level check JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckJsonResponse {
    pub schema_version: u32,
    pub policy: String,
    pub violations: Vec<CheckViolationEntry>,
    pub passed: bool,
}

/// Build the check response object.
pub fn build_check_response(
    policy_file: &str,
    violations: Vec<CheckViolationEntry>,
) -> CheckJsonResponse {
    CheckJsonResponse {
        schema_version: CHECK_SCHEMA_VERSION,
        policy: policy_file.to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

pub fn check_response_to_json(response: &CheckJsonResponse) -> Value {
    serde_json::to_value(response).expect("CheckJsonResponse serializes")
}

/// Passing fixture (empty violations array).
pub fn fixture_check_response() -> CheckJsonResponse {
    CheckJsonResponse {
        schema_version: CHECK_SCHEMA_VERSION,
        policy: "policy.json".into(),
        violations: Vec::new(),
        passed: true,
    }
}

pub fn fixture_check_json() -> Value {
    check_response_to_json(&fixture_check_response())
}

/// Convert legacy json violation rows into typed entries.
pub fn violations_from_json_values(rows: &[Value]) -> Vec<CheckViolationEntry> {
    rows.iter()
        .filter_map(|v| {
            let obj = v.as_object()?;
            Some(CheckViolationEntry {
                symbol: obj.get("symbol")?.as_str()?.to_string(),
                error: obj
                    .get("error")
                    .and_then(|e| e.as_str())
                    .map(str::to_string),
                violation: obj
                    .get("violation")
                    .and_then(|e| e.as_str())
                    .map(str::to_string),
            })
        })
        .collect()
}
