//! Export community-level migration graph for the dashboard bundle.

use rbuilder_analysis::{
    build_migration_graph, compute_migration_plan, AnalysisResults, MigrationGraphPayload,
    MigrationOrderMode, MigrationPlanPayload, MigrationWeights,
};
use rbuilder_graph::backend::MemoryBackend;
use std::fs;
use std::path::Path;

pub const MIGRATION_GRAPH_FILE: &str = "migration_graph.json";
pub const MIGRATION_PLAN_FILE: &str = "migration_plan.json";

#[derive(Debug, Clone, Default)]
pub struct MigrationExportSummary {
    pub available: bool,
    pub community_count: usize,
    pub edge_count: usize,
}

pub fn export_migration_graph(
    backend: &MemoryBackend,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<(MigrationExportSummary, Option<MigrationGraphPayload>), String> {
    let analysis_path = repo_root.join(".rbuilder/analysis_results.bin");
    if !analysis_path.is_file() {
        write_empty_graph(out_dir)?;
        return Ok((MigrationExportSummary::default(), None));
    }

    let results = AnalysisResults::load(&analysis_path).map_err(|e| e.to_string())?;
    let Some(graph) = build_migration_graph(backend, &results) else {
        write_empty_graph(out_dir)?;
        return Ok((MigrationExportSummary::default(), None));
    };

    let json = serde_json::to_string_pretty(&graph).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(MIGRATION_GRAPH_FILE), json).map_err(|e| e.to_string())?;

    let summary = MigrationExportSummary {
        available: true,
        community_count: graph.communities.len(),
        edge_count: graph.edges.len(),
    };
    Ok((summary, Some(graph)))
}

pub fn export_default_migration_plan(
    graph: &MigrationGraphPayload,
    out_dir: &Path,
) -> Result<(), String> {
    let plan = compute_migration_plan(
        graph,
        "hybrid_default",
        MigrationWeights::hybrid_default(),
        MigrationOrderMode::Scheduled,
    );
    write_migration_plan(&plan, &out_dir.join(MIGRATION_PLAN_FILE))
}

pub fn write_migration_plan(plan: &MigrationPlanPayload, path: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(plan).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn write_migration_plan_from_repo(
    backend: &MemoryBackend,
    repo_root: &Path,
    output: &Path,
    preset: &str,
    order_mode: MigrationOrderMode,
) -> Result<MigrationPlanPayload, String> {
    let analysis_path = repo_root.join(".rbuilder/analysis_results.bin");
    let results = AnalysisResults::load(&analysis_path).map_err(|e| e.to_string())?;
    let graph = build_migration_graph(backend, &results)
        .ok_or_else(|| "migration graph unavailable (run discover first)".to_string())?;
    let weights = MigrationWeights::from_preset(preset);
    let plan = compute_migration_plan(&graph, preset, weights, order_mode);
    write_migration_plan(&plan, output)?;
    Ok(plan)
}

fn write_empty_graph(out_dir: &Path) -> Result<(), String> {
    let payload = MigrationGraphPayload {
        schema_version: rbuilder_analysis::MIGRATION_GRAPH_SCHEMA_VERSION,
        modularity: 0.0,
        communities: Vec::new(),
        edges: Vec::new(),
    };
    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    fs::write(out_dir.join(MIGRATION_GRAPH_FILE), json).map_err(|e| e.to_string())
}
