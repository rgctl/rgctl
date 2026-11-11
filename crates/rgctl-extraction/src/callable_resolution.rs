//! Versioned source-extraction semantics and bounded unresolved-call evidence.
use crate::GraphBuilder;
use rgctl_error::{Error, Result};
use rgctl_plugin_api::Relation;
use serde::Serialize;
use std::path::Path;
pub const SEMANTICS_VERSION: u32 = 7;
pub const DIAGNOSTIC_LIMIT: usize = 1024;
const SEMANTICS_FILE: &str = "extraction_semantics.json";
pub fn semantics_current(root: &Path) -> bool {
    std::fs::read(rgctl_graph::paths::artifact_path(root, SEMANTICS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|v| v["version"] == SEMANTICS_VERSION)
}
#[derive(Debug, Serialize)]
pub struct CallableResolutionReport {
    pub schema_version: u32,
    pub unresolved_calls: usize,
    pub samples_truncated: bool,
    pub samples: Vec<Relation>,
}
pub fn save(root: &Path, builder: &mut GraphBuilder) -> Result<()> {
    let path = rgctl_graph::paths::artifact_path(root, "callable_resolution.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&builder.take_callable_resolution_report())
            .map_err(|e| Error::SerdeError(e.to_string()))?,
    )?;
    Ok(())
}
/// Mark semantics current only after the new snapshot has been persisted.
pub fn mark_current(root: &Path) -> Result<()> {
    std::fs::write(
        rgctl_graph::paths::artifact_path(root, SEMANTICS_FILE),
        serde_json::to_vec(&serde_json::json!({"version":SEMANTICS_VERSION}))
            .map_err(|e| Error::SerdeError(e.to_string()))?,
    )?;
    Ok(())
}
