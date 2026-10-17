//! Export `.rgctl/dashboard/` static bundle after discover.

mod analysis_stream_export;
mod blast_export;
mod bundle;
mod cfg_export;
mod cfg_record_pack;
mod communities;
mod dataflow_export;
mod export_context;
mod export_util;
mod function_meta;
mod function_metrics_export;
mod kantra_export;
mod manifest;
mod metagraph;
mod migration_export;
mod mutations_export;
mod profile;
mod slice_export;
mod source_catalog;
mod taint_export;

pub use bundle::{DASHBOARD_DIR_NAME, default_dashboard_path, dist_embedded};
pub use communities::{COMMUNITIES_FILE, COMMUNITIES_SCHEMA_VERSION, CommunitiesPayload};
pub use dataflow_export::{DATAFLOW_INDEX_FILE, DataflowExportSummary};
pub use export_context::DashboardExportContext;
pub use manifest::{
    AnalysisSection, DashboardManifest, KantraSection, MANIFEST_SCHEMA_VERSION, MetricsSection,
    SemanticSection, ViewSection,
};
pub use metagraph::{COMMUNITY_ONLY_THRESHOLD, METAGRAPH_FILE, MetagraphExport, MetagraphPayload};
pub use migration_export::{
    MIGRATION_GRAPH_FILE, MIGRATION_PLAN_FILE, MigrationExportSummary,
    export_default_migration_plan, export_migration_graph, write_migration_plan,
    write_migration_plan_from_repo, write_migration_plan_from_repo_with_context,
};
pub use mutations_export::{MUTATIONS_INDEX_FILE, MutationsExportSummary};
pub use kantra_export::{KANTRA_INDEX_FILE, KantraExportSummary};
pub use slice_export::{SLICE_INDEX_FILE, SliceExportSummary};
pub use taint_export::{TAINT_INDEX_FILE, TaintExportSummary};

use blast_export::{export_blast_bundle, load_columnar_uuid_indices};
use bundle::{extract_static_assets, inject_manifest_bootstrap};
use dataflow_export::export_dataflow_index;
use function_metrics_export::export_function_metrics;
use manifest::DashboardManifest as Manifest;
use metagraph::write_metagraph;
use mutations_export::export_mutations_index;
use profile::profile_stage;
use rgctl_analysis::storage::AnalysisStorage;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::{EdgeType, NodeType};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use kantra_export::export_kantra_bundle;
use taint_export::export_taint_bundle;

/// Write dashboard bundle: static UI, manifest, graph payload copy.
pub fn export_dashboard_bundle(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<(), String> {
    export_dashboard_bundle_with_context(
        backend,
        repo_root,
        snapshot_path,
        DashboardExportContext::default(),
    )
}

/// Write dashboard bundle using in-memory analysis from discover (avoids reloading results).
pub fn export_dashboard_bundle_with_context(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
    ctx: DashboardExportContext<'_>,
) -> Result<(), String> {
    export_dashboard_bundle_inner(backend, repo_root, snapshot_path, false, ctx)
}

/// Export dashboard only when semantic content fingerprint is unchanged.
pub fn export_dashboard_bundle_if_changed(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<bool, String> {
    export_dashboard_bundle_if_changed_with_context(
        backend,
        repo_root,
        snapshot_path,
        DashboardExportContext::default(),
    )
}

/// Export dashboard when fingerprint changed, with optional in-memory analysis.
pub fn export_dashboard_bundle_if_changed_with_context(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
    ctx: DashboardExportContext<'_>,
) -> Result<bool, String> {
    // Stated before the work rather than after it: a bundle that was requested and cannot be served is
    // worth saying at the start, not at the end of a long discover.
    bundle::warn_if_selection_unavailable();

    let out_dir = bundle::default_dashboard_path(repo_root);
    let manifest_path = out_dir.join("manifest.json");
    let fingerprint = compute_export_fingerprint(backend, repo_root);
    let version = bundle::selected_version_tag();
    if manifest_path.is_file() {
        if let Ok(bytes) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<Manifest>(&bytes) {
                // Two conditions, not one. Matching fingerprints mean "the graph has not changed since this
                // bundle was produced"; only the recorded version says *which* bundle it was. Without the
                // second comparison, switching bundles matched, returned early, and left the previous
                // bundle extracted — a silent no-op that looks like the flag being ignored.
                let same_graph = manifest.export_fingerprint.as_deref() == Some(fingerprint.as_str());
                let same_bundle = manifest.bundle_version.as_deref() == Some(version);
                if same_graph && same_bundle {
                    return Ok(false);
                }
            }
        }
    }
    export_dashboard_bundle_inner(backend, repo_root, snapshot_path, true, ctx)?;
    Ok(true)
}

fn export_dashboard_bundle_inner(
    backend: &MemoryBackend,
    repo_root: &Path,
    snapshot_path: &Path,
    replace_out_dir: bool,
    ctx: DashboardExportContext<'_>,
) -> Result<(), String> {
    let out_dir = bundle::default_dashboard_path(repo_root);
    if replace_out_dir && out_dir.exists() {
        profile_stage("replace_out_dir", || {
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
            Ok::<(), String>(())
        })?;
    }
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    profile_stage("extract_static_assets", || {
        extract_static_assets(&out_dir).map_err(|e| e.to_string())
    })?;

    let (payload_stats_res, uuid_to_index_res) = profile_stage("payload_and_uuid", || {
        rayon::join(
            || payload_stats(snapshot_path, backend),
            || {
                if snapshot_path.is_file() {
                    load_columnar_uuid_indices(snapshot_path)
                } else {
                    Ok(HashMap::new())
                }
            },
        )
    });
    let (node_count, edge_count, digest) = payload_stats_res?;
    let uuid_to_index = uuid_to_index_res?;

    let (export_fingerprint, metrics) = profile_stage("fingerprint_and_metrics", || {
        rayon::join(
            || compute_export_fingerprint(backend, repo_root),
            || collect_metrics(backend),
        )
    });

    let export = profile_stage("write_metagraph", || {
        write_metagraph(
            backend,
            snapshot_path,
            &out_dir,
            node_count,
            ctx.analysis,
            &uuid_to_index,
        )
    })?;
    let streamed = profile_stage("export_cfg_slice", || {
        analysis_stream_export::export_cfg_slice_from_storage(backend, repo_root, &out_dir)
    })?;
    let cfg_summary = streamed.cfg;
    let slice_summary = streamed.slice;

    let sidecars = profile_stage("export_sidecars", || {
        export_sidecars_parallel(SidecarJob {
            backend,
            repo_root,
            snapshot_path,
            out_dir: &out_dir,
            ctx,
            uuid_to_index: &uuid_to_index,
            node_count,
            dataflow_functions: streamed.dataflow,
        })
    })?;
    if let Some(ref graph) = sidecars.migration_graph {
        profile_stage("export_migration_plan", || {
            migration_export::export_default_migration_plan(graph, &out_dir)
        })?;
    }
    let semantic_summary = semantic_section(repo_root);
    let kantra_summary = profile_stage("export_kantra", || export_kantra_bundle(repo_root, &out_dir))?;
    let mut manifest = Manifest::with_phases(
        node_count,
        edge_count,
        digest,
        export_fingerprint,
        metrics,
        &export,
        &cfg_summary,
        &slice_summary,
        &sidecars.blast,
        &sidecars.dataflow,
        &sidecars.mutations,
        &sidecars.taint,
        &sidecars.migration,
        semantic_summary,
    );
    manifest.kantra = kantra_summary.manifest_section();
    let (manifest_json, manifest_serialize_secs) = profile_stage("manifest_serialize", || {
        let start = std::time::Instant::now();
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        Ok::<_, String>((json, start.elapsed().as_secs_f64()))
    })?;
    tracing::info!(
        target: "profile",
        serialize_secs = manifest_serialize_secs,
        json_bytes = manifest_json.len(),
        "[profile] save_dashboard json serialize"
    );
    profile_stage("manifest_write", || {
        fs::write(out_dir.join("manifest.json"), &manifest_json).map_err(|e| e.to_string())
    })?;
    profile_stage("inject_manifest_bootstrap", || {
        inject_manifest_bootstrap(&out_dir, &manifest_json).map_err(|e| e.to_string())
    })?;

    profile_stage("copy_graph_payload", || {
        copy_graph_payload(snapshot_path, &out_dir)
    })?;

    Ok(())
}

fn copy_graph_payload(snapshot_path: &Path, out_dir: &Path) -> Result<(), String> {
    let dest = out_dir.join("graph_payload.bin");
    if snapshot_path.is_file() {
        export_util::link_or_copy(snapshot_path, &dest)?;
        return Ok(());
    }
    Err(format!(
        "graph snapshot not found at {} — run discover first",
        snapshot_path.display()
    ))
}

struct SidecarJob<'a> {
    backend: &'a MemoryBackend,
    repo_root: &'a Path,
    snapshot_path: &'a Path,
    out_dir: &'a Path,
    ctx: DashboardExportContext<'a>,
    uuid_to_index: &'a HashMap<uuid::Uuid, u32>,
    node_count: u64,
    dataflow_functions: Vec<dataflow_export::DataflowFunctionEntry>,
}

struct SidecarExport {
    dataflow: DataflowExportSummary,
    mutations: MutationsExportSummary,
    taint: taint_export::TaintExportSummary,
    blast: blast_export::BlastExportSummary,
    migration: MigrationExportSummary,
    migration_graph: Option<rgctl_analysis::MigrationGraphPayload>,
}

/// Independent dashboard artifacts after CFG/slice. Rayon (not Tokio): CPU + many small files.
fn export_sidecars_parallel(job: SidecarJob<'_>) -> Result<SidecarExport, String> {
    let SidecarJob {
        backend,
        repo_root,
        snapshot_path,
        out_dir,
        ctx,
        uuid_to_index,
        node_count,
        dataflow_functions,
    } = job;
    let dataflow_rows = dataflow_functions;
    let ((dataflow, mutations), (taint, (blast, (metrics, migration)))) = rayon::join(
        || {
            rayon::join(
                || {
                    profile_stage("export_dataflow", || {
                        export_dataflow_index(dataflow_rows, out_dir)
                    })
                },
                || {
                    profile_stage("export_mutations", || {
                        export_mutations_index(repo_root, out_dir)
                    })
                },
            )
        },
        || {
            rayon::join(
                || profile_stage("export_taint", || export_taint_bundle(repo_root, out_dir)),
                || {
                    rayon::join(
                        || {
                            profile_stage("export_blast", || {
                                export_blast_bundle(
                                    repo_root,
                                    snapshot_path,
                                    out_dir,
                                    ctx,
                                    uuid_to_index,
                                )
                            })
                        },
                        || {
                            rayon::join(
                                || {
                                    profile_stage("export_function_metrics", || {
                                        export_function_metrics(
                                            snapshot_path,
                                            out_dir,
                                            node_count,
                                            ctx,
                                            uuid_to_index,
                                        )
                                    })
                                },
                                || {
                                    profile_stage("export_migration", || {
                                        migration_export::export_migration_graph(
                                            backend, repo_root, out_dir, ctx,
                                        )
                                    })
                                },
                            )
                        },
                    )
                },
            )
        },
    );

    let dataflow = dataflow?;
    let mutations = mutations?;
    let taint = taint?;
    let blast = blast?;
    metrics?;
    let (migration, migration_graph) = migration?;
    Ok(SidecarExport {
        dataflow,
        mutations,
        taint,
        blast,
        migration,
        migration_graph,
    })
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

    let storage = AnalysisStorage::new(repo_root.join(".rgctl/analysis"));
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

    let semantic_path = rgctl_analysis::SemanticIndex::default_path(repo_root);
    if semantic_path.is_file() {
        hasher.update(b"semantic_index_v1");
        if let Ok(meta) = std::fs::metadata(&semantic_path) {
            hasher.update(&meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified() {
                if let Ok(secs) = modified.duration_since(std::time::UNIX_EPOCH) {
                    hasher.update(&secs.as_secs().to_le_bytes());
                }
            }
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

fn semantic_section(repo_root: &Path) -> Option<manifest::SemanticSection> {
    use rgctl_analysis::SemanticIndex;

    let path = SemanticIndex::default_path(repo_root);
    if !path.is_file() {
        return None;
    }
    let index = SemanticIndex::load(&path).ok()?;
    if index.is_empty() {
        return None;
    }
    Some(manifest::SemanticSection {
        available: true,
        functions_indexed: index.len(),
        model_id: index.model_id,
        dimensions: index.dimensions,
        graph_digest: index.graph_digest,
    })
}

