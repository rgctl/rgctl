//! Export community-level migration graph for the dashboard bundle.

use crate::export_context::{DashboardExportContext, resolve_analysis};
use crate::export_util::write_json_compact;
use crate::metagraph::PackageAggregation;
use rgctl_analysis::{
    MigrationGraphPayload, MigrationOrderMode, MigrationPlanPayload, MigrationWeights,
    build_migration_graph, compute_migration_plan,
};
use rgctl_graph::graph_source::GraphSource;
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
    graph: GraphSource<'_>,
    repo_root: &Path,
    out_dir: &Path,
    ctx: DashboardExportContext<'_>,
) -> Result<(MigrationExportSummary, Option<MigrationGraphPayload>), String> {
    export_migration_graph_with_aggregation(graph, repo_root, out_dir, ctx, None)
}

/// Like [`export_migration_graph`], but reuses a precomputed migration payload from
/// [`PackageAggregation`] (shared node/edge scan with metagraph) when present.
pub fn export_migration_graph_with_aggregation(
    graph: GraphSource<'_>,
    repo_root: &Path,
    out_dir: &Path,
    ctx: DashboardExportContext<'_>,
    package_agg: Option<&PackageAggregation>,
) -> Result<(MigrationExportSummary, Option<MigrationGraphPayload>), String> {
    if let Some(precomputed) = package_agg.and_then(|a| a.migration.clone()) {
        return write_migration_payload(out_dir, precomputed);
    }

    let results = match resolve_analysis(&ctx, repo_root) {
        Ok(cow) => cow,
        Err(_) => {
            write_empty_graph(out_dir)?;
            return Ok((MigrationExportSummary::default(), None));
        }
    };

    let Some(payload) = build_migration_graph(graph, &results) else {
        write_empty_graph(out_dir)?;
        return Ok((MigrationExportSummary::default(), None));
    };

    write_migration_payload(out_dir, payload)
}

fn write_migration_payload(
    out_dir: &Path,
    graph: MigrationGraphPayload,
) -> Result<(MigrationExportSummary, Option<MigrationGraphPayload>), String> {
    write_json_compact(&out_dir.join(MIGRATION_GRAPH_FILE), &graph)?;

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
    write_json_compact(path, plan)
}

pub fn write_migration_plan_from_repo(
    graph: GraphSource<'_>,
    repo_root: &Path,
    output: &Path,
    preset: &str,
    order_mode: MigrationOrderMode,
) -> Result<MigrationPlanPayload, String> {
    write_migration_plan_from_repo_with_context(
        graph,
        repo_root,
        output,
        preset,
        order_mode,
        DashboardExportContext::default(),
    )
}

pub fn write_migration_plan_from_repo_with_context(
    graph: GraphSource<'_>,
    repo_root: &Path,
    output: &Path,
    preset: &str,
    order_mode: MigrationOrderMode,
    ctx: DashboardExportContext<'_>,
) -> Result<MigrationPlanPayload, String> {
    let results = resolve_analysis(&ctx, repo_root)?;
    let mig = build_migration_graph(graph, &results)
        .ok_or_else(|| "migration graph unavailable (run discover first)".to_string())?;
    let weights = MigrationWeights::from_preset(preset);
    let plan = compute_migration_plan(&mig, preset, weights, order_mode);
    write_migration_plan(&plan, output)?;
    Ok(plan)
}

fn write_empty_graph(out_dir: &Path) -> Result<(), String> {
    let payload = MigrationGraphPayload {
        schema_version: rgctl_analysis::MIGRATION_GRAPH_SCHEMA_VERSION,
        mode: "package_macro".into(),
        modularity: 0.0,
        communities: Vec::new(),
        edges: Vec::new(),
    };
    write_json_compact(&out_dir.join(MIGRATION_GRAPH_FILE), &payload)
}
