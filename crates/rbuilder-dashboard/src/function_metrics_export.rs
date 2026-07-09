//! Export per-node analysis scores for the Functions dashboard tab.

use crate::blast_export::scan_columnar_uuid_indices;
use rbuilder_analysis::AnalysisResults;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FUNCTION_METRICS_FILE: &str = "function_metrics.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetricRow {
    pub index: u32,
    pub pagerank: f32,
    pub betweenness: f32,
    pub harmonic: f32,
    pub blast: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionMetricsPayload {
    pub schema_version: u32,
    pub rows: Vec<FunctionMetricRow>,
}

pub fn export_function_metrics(
    repo_root: &Path,
    snapshot_path: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let analysis_path = repo_root.join(".rbuilder/analysis_results.bin");
    if !analysis_path.is_file() || !snapshot_path.is_file() {
        write_empty(out_dir)?;
        return Ok(());
    }

    let results = AnalysisResults::load(&analysis_path).map_err(|e| e.to_string())?;
    let bytes = fs::read(snapshot_path).map_err(|e| e.to_string())?;
    let uuid_to_index = scan_columnar_uuid_indices(&bytes)?;

    let centrality = results.centrality.as_ref();
    let blast = results.blast_radius.as_ref();

    let mut rows = Vec::new();
    for compact_id in 0..results.node_count() {
        let Some(uuid) = results.get_uuid(compact_id as u32) else {
            continue;
        };
        let Some(&index) = uuid_to_index.get(&uuid) else {
            continue;
        };

        let pagerank = centrality
            .and_then(|c| c.pagerank.get(compact_id).copied())
            .unwrap_or(0.0);
        let betweenness = centrality
            .and_then(|c| c.betweenness.get(compact_id).copied())
            .unwrap_or(0.0);
        let harmonic = centrality
            .and_then(|c| c.harmonic.get(compact_id).copied())
            .unwrap_or(0.0);
        let blast_score = blast
            .and_then(|b| b.scores.get(compact_id).copied())
            .unwrap_or(0.0);
        let community_id = results
            .community
            .as_ref()
            .and_then(|c| c.get(compact_id as u32))
            .map(|id| id as u32);

        if pagerank <= 0.0
            && betweenness <= 0.0
            && harmonic <= 0.0
            && blast_score <= 0.0
            && community_id.is_none()
        {
            continue;
        }

        rows.push(FunctionMetricRow {
            index,
            pagerank,
            betweenness,
            harmonic,
            blast: blast_score,
            community_id,
        });
    }

    rows.sort_by_key(|a| a.index);

    let payload = FunctionMetricsPayload {
        schema_version: 2,
        rows: rows.clone(),
    };
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(FUNCTION_METRICS_FILE), json).map_err(|e| e.to_string())?;

    Ok(())
}

fn write_empty(out_dir: &Path) -> Result<(), String> {
    let payload = FunctionMetricsPayload {
        schema_version: 2,
        rows: Vec::new(),
    };
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(FUNCTION_METRICS_FILE), json).map_err(|e| e.to_string())
}
