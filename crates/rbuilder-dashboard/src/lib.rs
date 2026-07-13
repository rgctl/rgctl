//! Export `.rbuilder/dashboard/` static bundle after discover.

mod analysis_stream_export;
mod blast_export;
mod bundle;
mod cfg_export;
mod communities;
mod dataflow_export;
mod export_util;
mod function_meta;
mod function_metrics_export;
mod manifest;
mod metagraph;
mod migration_export;
mod slice_export;
mod source_catalog;
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
use dataflow_export::export_dataflow_index;
use manifest::DashboardManifest as Manifest;
use metagraph::write_metagraph;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use std::fs;
use std::path::Path;
use taint_export::export_taint_bundle;
use rbuilder_analysis::storage::AnalysisStorage;

/// Write dashboard bundle: static UI, manifest, graph payload copy.
pub fn export_dashboard_bundle(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<(), String> {
    export_dashboard_bundle_inner(backend, repo_root, snapshot_path, false)?;
    Ok(())
}

/// Export dashboard only when semantic content fingerprint is unchanged.
pub fn export_dashboard_bundle_if_changed(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<bool, String> {
    let out_dir = bundle::default_dashboard_path(repo_root);
    let manifest_path = out_dir.join("manifest.json");
    let fingerprint = compute_export_fingerprint(backend, repo_root);
    if manifest_path.is_file() {
        if let Ok(bytes) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<Manifest>(&bytes) {
                if manifest.export_fingerprint.as_deref() == Some(fingerprint.as_str()) {
                    return Ok(false);
                }
            }
        }
    }
    let _ = snapshot_path;
    export_dashboard_bundle_inner(backend, repo_root, snapshot_path, true)?;
    Ok(true)
}

fn export_dashboard_bundle_inner(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
    replace_out_dir: bool,
) -> Result<(), String> {
    let out_dir = bundle::default_dashboard_path(repo_root);
    if replace_out_dir && out_dir.exists() {
        let trash = out_dir.with_file_name(format!(
            "{}.trash.{}",
            out_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("dashboard"),
            std::process::id()
        ));
        if trash.exists() {
            let _ = fs::remove_dir_all(&trash);
        }
        fs::rename(&out_dir, &trash).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir_all(&trash);
    }
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    extract_static_assets(&out_dir)?;

    let (node_count, edge_count, digest) = payload_stats(snapshot_path, backend)?;
    let export_fingerprint = compute_export_fingerprint(backend, repo_root);
    let metrics = collect_metrics(backend);

    let export = write_metagraph(backend, snapshot_path, &out_dir, node_count)?;
    let streamed =
        analysis_stream_export::export_cfg_slice_from_storage(backend, repo_root, &out_dir)?;
    let cfg_summary = streamed.cfg;
    let slice_summary = streamed.slice;
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
        export_fingerprint,
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

/// Hash graph topology + function body hashes + analysis index for incremental export skip.
fn compute_export_fingerprint(backend: &MemoryBackend, repo_root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(backend.node_count() as u64).to_le_bytes());
    hasher.update(&(backend.edge_count() as u64).to_le_bytes());

    if let Ok(functions) = backend.collect_nodes_by_type(NodeType::Function) {
        let mut refs: Vec<(&str, &str, &str)> = functions
            .iter()
            .filter_map(|f| {
                Some((
                    f.file_path.as_deref()?,
                    f.name.as_str(),
                    f.code_hash.as_deref()?,
                ))
            })
            .collect();
        refs.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then_with(|| a.1.cmp(b.1))
                .then_with(|| a.2.cmp(b.2))
        });
        for (path, name, hash) in refs {
            hasher.update(path.as_bytes());
            hasher.update(name.as_bytes());
            hasher.update(hash.as_bytes());
        }
    }

    let storage = AnalysisStorage::new(repo_root.join(".rbuilder/analysis"));
    if let Ok(index) = storage.load_analysis_index() {
        hasher.update(&(index.len() as u64).to_le_bytes());
        let mut keys: Vec<_> = index.keys().collect();
        keys.sort();
        for key in keys {
            let entry = &index[key];
            hasher.update(key.as_bytes());
            hasher.update(entry.code_hash.as_bytes());
            hasher.update(&(entry.flow_count as u64).to_le_bytes());
            hasher.update(&(entry.vulnerable_count as u64).to_le_bytes());
        }
    }

    hasher.finalize().to_hex().to_string()
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
