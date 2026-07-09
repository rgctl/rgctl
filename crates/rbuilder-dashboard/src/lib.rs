//! Export `.rbuilder/dashboard/` static bundle after discover.

mod blast_export;
mod bundle;
mod cfg_export;
mod communities;
mod dataflow_export;
mod function_meta;
mod function_metrics_export;
mod manifest;
mod metagraph;
mod migration_export;
mod slice_export;
mod taint_export;

pub use bundle::{default_dashboard_path, dist_embedded, DASHBOARD_DIR_NAME};
pub use communities::{CommunitiesPayload, COMMUNITIES_FILE, COMMUNITIES_SCHEMA_VERSION};
pub use dataflow_export::{DataflowExportSummary, DATAFLOW_INDEX_FILE};
pub use manifest::{
    AnalysisSection, DashboardManifest, MetricsSection, ViewSection, MANIFEST_SCHEMA_VERSION,
};
pub use metagraph::{
    MetagraphExport, MetagraphPayload, COMMUNITY_ONLY_THRESHOLD, METAGRAPH_FILE,
};
pub use migration_export::{
    export_default_migration_plan, export_migration_graph, write_migration_plan,
    write_migration_plan_from_repo, MigrationExportSummary, MIGRATION_GRAPH_FILE,
    MIGRATION_PLAN_FILE,
};
pub use slice_export::{SliceExportSummary, SLICE_INDEX_FILE};
pub use taint_export::{TaintExportSummary, TAINT_INDEX_FILE};

use blast_export::export_blast_bundle;
use function_metrics_export::export_function_metrics;
use bundle::{extract_static_assets, inject_manifest_bootstrap};
use cfg_export::export_cfg_bundle;
use dataflow_export::export_dataflow_index;
use manifest::DashboardManifest as Manifest;
use metagraph::write_metagraph;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use slice_export::export_slice_bundle;
use std::fs;
use std::path::Path;
use taint_export::export_taint_bundle;

/// Write dashboard bundle: static UI, manifest, graph payload copy.
pub fn export_dashboard_bundle(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<(), String> {
    let out_dir = bundle::default_dashboard_path(repo_root);
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    extract_static_assets(&out_dir)?;

    let (node_count, edge_count, digest) = payload_stats(snapshot_path, backend)?;
    let metrics = collect_metrics(backend);

    let export = write_metagraph(backend, snapshot_path, &out_dir, node_count)?;
    let cfg_summary = export_cfg_bundle(backend, repo_root, &out_dir)?;
    let slice_summary = export_slice_bundle(backend, repo_root, &out_dir)?;
    let dataflow_summary = export_dataflow_index(&slice_summary, &out_dir)?;
    let taint_summary = export_taint_bundle(repo_root, &out_dir)?;
    let blast_summary = export_blast_bundle(repo_root, &out_dir)?;
    export_function_metrics(repo_root, snapshot_path, &out_dir)?;
    let (migration_summary, migration_graph) =
        migration_export::export_migration_graph(backend, repo_root, &out_dir)?;
    if let Some(ref graph) = migration_graph {
        migration_export::export_default_migration_plan(graph, &out_dir)?;
    }
    let manifest = Manifest::with_phases(
        node_count,
        edge_count,
        digest,
        metrics,
        &export,
        &cfg_summary,
        &slice_summary,
        &blast_summary,
        &dataflow_summary,
        &taint_summary,
        &migration_summary,
    );
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("manifest.json"), &manifest_json).map_err(|e| e.to_string())?;
    inject_manifest_bootstrap(&out_dir, &manifest_json)?;

    copy_graph_payload(snapshot_path, &out_dir)?;

    Ok(())
}

fn copy_graph_payload(snapshot_path: &Path, out_dir: &Path) -> Result<(), String> {
    let dest = out_dir.join("graph_payload.bin");
    if snapshot_path.is_file() {
        fs::copy(snapshot_path, &dest).map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(format!(
        "graph snapshot not found at {} — run discover first",
        snapshot_path.display()
    ))
}

fn payload_stats(
    snapshot_path: &Path,
    backend: &MemoryBackend,
) -> Result<(u64, u64, String), String> {
    if snapshot_path.is_file() {
        let bytes = fs::read(snapshot_path).map_err(|e| e.to_string())?;
        if bytes.len() >= 92 && &bytes[0..4] == b"RBGR" {
            let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
            let edge_count = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
            let digest = std::str::from_utf8(&bytes[28..92])
                .unwrap_or("")
                .trim_end_matches('\0')
                .to_string();
            return Ok((node_count, edge_count, digest));
        }
    }
    Ok((
        backend.node_count() as u64,
        backend.edge_count() as u64,
        String::new(),
    ))
}

fn collect_metrics(backend: &MemoryBackend) -> MetricsSection {
    let mut function_count = 0usize;
    let mut class_count = 0usize;
    let mut complexity_sum = 0.0f64;
    let mut high_blast_radius_count = 0usize;
    let mut calls_count = 0usize;

    let _ = backend.for_each_node(|n| {
        if n.node_type == NodeType::Function {
            function_count += 1;
            if let Some(v) = n.properties.get("cyclomatic") {
                if let Ok(c) = v.parse::<f64>() {
                    complexity_sum += c;
                }
            }
            if let Some(v) = n.properties.get("blast_radius_score") {
                if let Ok(s) = v.parse::<f64>() {
                    if s > 50.0 {
                        high_blast_radius_count += 1;
                    }
                }
            }
        } else if n.node_type == NodeType::Class {
            class_count += 1;
        }
    });

    let _ = backend.for_each_edge(|e| {
        if e.edge_type == EdgeType::Calls {
            calls_count += 1;
        }
    });

    MetricsSection {
        function_count,
        class_count,
        calls_count,
        avg_complexity: complexity_sum / function_count.max(1) as f64,
        high_blast_radius_count,
    }
}
