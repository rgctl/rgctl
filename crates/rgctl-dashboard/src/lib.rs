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

pub use bundle::{
    BundleVersion, DASHBOARD_DIR_NAME, DASHBOARD_LEGACY_ENV, DASHBOARD_V2_ENV,
    default_dashboard_path, dist_embedded, ensure_static_shell, set_bundle_override,
    warn_if_legacy_selected,
};
pub use communities::{COMMUNITIES_FILE, COMMUNITIES_SCHEMA_VERSION, CommunitiesPayload};
pub use dataflow_export::{DATAFLOW_INDEX_FILE, DataflowExportSummary};
pub use export_context::DashboardExportContext;
pub use kantra_export::{KANTRA_INDEX_FILE, KantraExportSummary};
pub use manifest::{
    AnalysisSection, DashboardCapabilities, DashboardManifest, KantraSection,
    MANIFEST_SCHEMA_VERSION, MetricsSection, SemanticSection, ViewSection,
};
pub use metagraph::{
    COMMUNITY_ONLY_THRESHOLD, METAGRAPH_FILE, MetagraphExport, MetagraphPayload, PackageAggregation,
};
pub use migration_export::{
    MIGRATION_GRAPH_FILE, MIGRATION_PLAN_FILE, MigrationExportSummary,
    export_default_migration_plan, export_migration_graph, write_migration_plan,
    write_migration_plan_from_repo, write_migration_plan_from_repo_with_context,
};
pub use mutations_export::{MUTATIONS_INDEX_FILE, MutationsExportSummary};
pub use slice_export::{SLICE_INDEX_FILE, SliceExportSummary};
pub use taint_export::{TAINT_INDEX_FILE, TaintExportSummary};

use blast_export::{export_blast_bundle_with_graph, load_columnar_uuid_indices_from_graph};
use bundle::{extract_static_assets, inject_manifest_bootstrap, selected_version};
use dataflow_export::export_dataflow_index;
use function_metrics_export::export_function_metrics;
use kantra_export::export_kantra_bundle_with_inputs;
use manifest::DashboardManifest as Manifest;
use metagraph::write_metagraph;
use mutations_export::export_mutations_index_with_analysis;
use profile::profile_stage;
use rgctl_analysis::storage::AnalysisStorage;
use rgctl_graph::graph_source::GraphSource;
use rgctl_graph::schema::{EdgeType, NodeType};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use taint_export::export_taint_bundle_with_index;

/// Bump when fingerprint inputs / hashing scheme changes (forces warm re-export).
const EXPORT_FINGERPRINT_SCHEMA: u32 = 2;

/// Write dashboard bundle: static UI, manifest, graph payload copy.
pub fn export_dashboard_bundle(
    graph: GraphSource<'_>,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<(), String> {
    export_dashboard_bundle_with_context(
        graph,
        repo_root,
        snapshot_path,
        DashboardExportContext::default(),
    )
}

/// Write dashboard bundle using in-memory analysis from discover (avoids reloading results).
pub fn export_dashboard_bundle_with_context(
    graph: GraphSource<'_>,
    repo_root: &Path,
    snapshot_path: &Path,
    ctx: DashboardExportContext<'_>,
) -> Result<(), String> {
    let fingerprint = compute_export_fingerprint(graph, repo_root);
    export_dashboard_bundle_inner(graph, repo_root, snapshot_path, false, ctx, fingerprint)
}

/// Export dashboard only when semantic content fingerprint is unchanged.
pub fn export_dashboard_bundle_if_changed(
    graph: GraphSource<'_>,
    repo_root: &Path,
    snapshot_path: &Path,
) -> Result<bool, String> {
    export_dashboard_bundle_if_changed_with_context(
        graph,
        repo_root,
        snapshot_path,
        DashboardExportContext::default(),
    )
}

/// Export dashboard when fingerprint changed, with optional in-memory analysis.
pub fn export_dashboard_bundle_if_changed_with_context(
    graph: GraphSource<'_>,
    repo_root: &Path,
    snapshot_path: &Path,
    ctx: DashboardExportContext<'_>,
) -> Result<bool, String> {
    // Stated before the work rather than after it: a bundle that was requested and cannot be served is
    // worth saying at the start, not at the end of a long discover.
    bundle::warn_if_selection_unavailable();
    bundle::warn_if_legacy_selected();

    let out_dir = bundle::default_dashboard_path(repo_root);
    let manifest_path = out_dir.join("manifest.json");
    let fingerprint = compute_export_fingerprint(graph, repo_root);
    let version = bundle::selected_version_tag();
    if manifest_path.is_file() {
        if let Ok(bytes) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<Manifest>(&bytes) {
                // Two conditions, not one. Matching fingerprints mean "the graph has not changed since this
                // bundle was produced"; only the recorded version says *which* bundle it was. Without the
                // second comparison, switching bundles matched, returned early, and left the previous
                // bundle extracted — a silent no-op that looks like the flag being ignored.
                let same_graph =
                    manifest.export_fingerprint.as_deref() == Some(fingerprint.as_str());
                let same_bundle = manifest.bundle_version.as_deref() == Some(version);
                // index.html is the serve entrypoint; if it was deleted, rewrite even when
                // the fingerprint still matches (full-pipeline warm recovery).
                if same_graph && same_bundle && out_dir.join("index.html").is_file() {
                    return Ok(false);
                }
            }
        }
    }
    export_dashboard_bundle_inner(graph, repo_root, snapshot_path, true, ctx, fingerprint)?;
    Ok(true)
}

fn export_dashboard_bundle_inner(
    graph: GraphSource<'_>,
    repo_root: &Path,
    snapshot_path: &Path,
    replace_out_dir: bool,
    ctx: DashboardExportContext<'_>,
    export_fingerprint: String,
) -> Result<(), String> {
    let out_dir = bundle::default_dashboard_path(repo_root);
    let trash_handle = if replace_out_dir && out_dir.exists() {
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
            // Unlink previous bundle off the critical path; join before return.
            Ok::<_, String>(Some(std::thread::spawn(move || {
                let _ = fs::remove_dir_all(&trash);
            })))
        })?
    } else {
        None
    };
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    profile_stage("extract_static_assets", || {
        extract_static_assets(&out_dir).map_err(|e| e.to_string())
    })?;

    let (payload_stats_res, uuid_to_index_res) = profile_stage("payload_and_uuid", || {
        rayon::join(
            || payload_stats(snapshot_path, graph),
            || load_columnar_uuid_indices_from_graph(graph, snapshot_path),
        )
    });
    let (node_count, edge_count, digest) = payload_stats_res?;
    let uuid_to_index = uuid_to_index_res?;

    let metrics = profile_stage("collect_metrics", || collect_metrics(graph, ctx.analysis));

    // Load analysis_index.bin once for cfg/slice, mutations, and taint exporters.
    let analysis_index = profile_stage("load_analysis_index", || {
        let storage = AnalysisStorage::new(repo_root.join(".rgctl/analysis"));
        storage.load_analysis_index().ok()
    });
    let analysis_index_ref = analysis_index.as_ref();

    let (export, package_agg) = profile_stage("write_metagraph", || {
        write_metagraph(
            graph,
            snapshot_path,
            &out_dir,
            node_count,
            ctx.analysis,
            &uuid_to_index,
        )
    })?;
    let streamed = profile_stage("export_cfg_slice", || {
        analysis_stream_export::export_cfg_slice_from_storage_with_index(
            graph,
            repo_root,
            &out_dir,
            analysis_index_ref,
        )
    })?;
    let cfg_summary = streamed.cfg;
    let slice_summary = streamed.slice;
    let mut source_catalog = streamed.source_catalog;
    let dataflow_functions = streamed.dataflow;

    // Overlap Kantra file shards with independent sidecars (blast/taint/migration/…).
    let (sidecars_res, kantra_res) = profile_stage("export_sidecars_and_kantra", || {
        rayon::join(
            || {
                export_sidecars_parallel(SidecarJob {
                    graph,
                    repo_root,
                    snapshot_path,
                    out_dir: &out_dir,
                    ctx,
                    uuid_to_index: &uuid_to_index,
                    node_count,
                    dataflow_functions,
                    package_agg: &package_agg,
                    analysis_index: analysis_index_ref,
                })
            },
            || {
                export_kantra_bundle_with_inputs(
                    repo_root,
                    &out_dir,
                    ctx.kantra_findings,
                    Some(&mut source_catalog),
                )
            },
        )
    });
    let sidecars = sidecars_res?;
    let kantra_summary = kantra_res?;
    if let Some(ref graph) = sidecars.migration_graph {
        profile_stage("export_migration_plan", || {
            migration_export::export_default_migration_plan(graph, &out_dir)
        })?;
    }
    let semantic_summary = semantic_section(repo_root);
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

    if let Some(handle) = trash_handle {
        let _ = handle.join();
    }

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
    graph: GraphSource<'a>,
    repo_root: &'a Path,
    snapshot_path: &'a Path,
    out_dir: &'a Path,
    ctx: DashboardExportContext<'a>,
    uuid_to_index: &'a HashMap<uuid::Uuid, u32>,
    node_count: u64,
    dataflow_functions: Vec<dataflow_export::DataflowFunctionEntry>,
    package_agg: &'a PackageAggregation,
    analysis_index: Option<&'a analysis_stream_export::AnalysisIndexMap>,
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
        graph,
        repo_root,
        snapshot_path,
        out_dir,
        ctx,
        uuid_to_index,
        node_count,
        dataflow_functions,
        package_agg,
        analysis_index,
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
                        export_mutations_index_with_analysis(repo_root, out_dir, analysis_index)
                    })
                },
            )
        },
        || {
            rayon::join(
                || {
                    profile_stage("export_taint", || {
                        export_taint_bundle_with_index(repo_root, out_dir, analysis_index)
                    })
                },
                || {
                    rayon::join(
                        || {
                            profile_stage("export_blast", || {
                                export_blast_bundle_with_graph(
                                    Some(graph),
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
                                    if selected_version() == BundleVersion::V1 {
                                        profile_stage("export_function_metrics", || {
                                            export_function_metrics(
                                                snapshot_path,
                                                out_dir,
                                                node_count,
                                                ctx,
                                                uuid_to_index,
                                            )
                                        })
                                    } else {
                                        Ok(())
                                    }
                                },
                                || {
                                    profile_stage("export_migration", || {
                                        migration_export::export_migration_graph_with_aggregation(
                                            graph,
                                            repo_root,
                                            out_dir,
                                            ctx,
                                            Some(package_agg),
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
    graph: GraphSource<'_>,
) -> Result<(u64, u64, String), String> {
    if let Some(digest) = graph.snapshot_content_digest() {
        return Ok((
            graph.node_count() as u64,
            graph.edge_count() as u64,
            digest.to_string(),
        ));
    }
    if snapshot_path.is_file() {
        let mut f = fs::File::open(snapshot_path).map_err(|e| e.to_string())?;
        let mut header = [0u8; 92];
        if f.read_exact(&mut header).is_ok() && &header[0..4] == b"RBGR" {
            let node_count = u64::from_le_bytes(header[12..20].try_into().unwrap());
            let edge_count = u64::from_le_bytes(header[20..28].try_into().unwrap());
            let digest = std::str::from_utf8(&header[28..92])
                .unwrap_or("")
                .trim_end_matches('\0')
                .to_string();
            return Ok((node_count, edge_count, digest));
        }
    }
    Ok((
        graph.node_count() as u64,
        graph.edge_count() as u64,
        String::new(),
    ))
}

/// O(1) export fingerprint: schema + bundle version + snapshot `content_digest` + artifact
/// file metadata (`len`, `modified`) — never clones/sorts Function nodes or deserializes
/// `analysis_index.bin`.
fn compute_export_fingerprint(graph: GraphSource<'_>, repo_root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&EXPORT_FINGERPRINT_SCHEMA.to_le_bytes());
    hasher.update(bundle::selected_version_tag().as_bytes());
    hasher.update(&(graph.node_count() as u64).to_le_bytes());
    hasher.update(&(graph.edge_count() as u64).to_le_bytes());

    if let Some(digest) = graph.snapshot_content_digest() {
        hasher.update(b"snapshot_digest_v1");
        hasher.update(digest.as_bytes());
    } else if let Some(bytes) = graph.snapshot_content_digest_bytes() {
        hasher.update(b"snapshot_digest_bytes_v1");
        hasher.update(&bytes);
    } else if let Some(backend) = graph.as_memory() {
        // In-memory test graphs without a snapshot file: sample topology only (no Function clone/sort).
        hasher.update(b"memory_fallback_v2");
        hasher.update(&(backend.node_count() as u64).to_le_bytes());
        hasher.update(&(backend.edge_count() as u64).to_le_bytes());
        let mut sample = 0u64;
        let _ = backend.for_each_node(|n| {
            if sample < 64 {
                hasher.update(n.id.as_bytes());
                hasher.update(n.name.as_bytes());
                sample += 1;
            }
        });
    }

    hash_file_meta(
        &mut hasher,
        b"analysis_index_v1",
        &repo_root.join(".rgctl/analysis/analysis_index.bin"),
    );
    hash_file_meta(
        &mut hasher,
        b"semantic_index_v1",
        &rgctl_analysis::SemanticIndex::default_path(repo_root),
    );
    hash_file_meta(
        &mut hasher,
        b"field_write_v1",
        &rgctl_analysis::FieldWriteIndex::default_path(repo_root),
    );
    hash_file_meta(
        &mut hasher,
        b"kantra_findings_v1",
        &rgctl_graph::paths::artifact_path(repo_root, "kantra_findings.json"),
    );

    hasher.finalize().to_hex().to_string()
}

fn hash_file_meta(hasher: &mut blake3::Hasher, tag: &[u8], path: &Path) {
    hasher.update(tag);
    if !path.is_file() {
        hasher.update(&[0u8]);
        return;
    }
    hasher.update(&[1u8]);
    if let Ok(meta) = fs::metadata(path) {
        hasher.update(&meta.len().to_le_bytes());
        if let Ok(modified) = meta.modified() {
            if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(&dur.as_secs().to_le_bytes());
                hasher.update(&dur.subsec_nanos().to_le_bytes());
            }
        }
    }
}

fn collect_metrics(
    graph: GraphSource<'_>,
    analysis: Option<&rgctl_analysis::AnalysisResults>,
) -> MetricsSection {
    let mut function_count = 0usize;
    let mut class_count = 0usize;
    let mut complexity_sum = 0.0f64;
    let mut high_blast_radius_count = 0usize;
    let mut calls_count = 0usize;
    let blast_scores = analysis.and_then(|a| a.blast_radius.as_ref());

    let _ = graph.for_each_hot_node(|n| {
        if n.node_type == NodeType::Function {
            function_count += 1;
            if let Some(c) = n.cyclomatic {
                complexity_sum += c;
            }
            if let (Some(results), Some(blast)) = (analysis, blast_scores) {
                if let Some(compact_id) = results.get_compact_id(n.id) {
                    if let Some(&score) = blast.scores.get(compact_id as usize) {
                        if score > 50.0 {
                            high_blast_radius_count += 1;
                        }
                    }
                }
            }
        } else if n.node_type == NodeType::Class {
            class_count += 1;
        }
    });

    // Fallback when AnalysisResults was not supplied: check cold properties on MemoryBackend.
    if analysis.is_none() {
        if let Some(backend) = graph.as_memory() {
            let _ = backend.for_each_node(|n| {
                if n.node_type == NodeType::Function {
                    if let Some(v) = n.properties.get("blast_radius_score") {
                        if let Ok(s) = v.parse::<f64>() {
                            if s > 50.0 {
                                high_blast_radius_count += 1;
                            }
                        }
                    }
                }
            });
        }
    }

    let _ = graph.for_each_edge_type(|edge_type| {
        if edge_type == EdgeType::Calls {
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

    // Do **not** `SemanticIndex::load` here: on metasfresh the index is ~50 MiB and
    // full bincode deserialize added ~2.4 s to save_dashboard. Manifest only needs
    // presence + cheap file metadata; detailed counts stay optional until a sidecar
    // meta format exists.
    let path = SemanticIndex::default_path(repo_root);
    if !path.is_file() {
        return None;
    }
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() == 0 {
        return None;
    }
    Some(manifest::SemanticSection {
        available: true,
        functions_indexed: 0,
        model_id: "semantic_index.bin".into(),
        dimensions: 0,
        graph_digest: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::MemoryBackend;
    use tempfile::TempDir;

    #[test]
    fn v2_skips_function_metrics_file_when_sidecars_run() {
        // Direct gate: V2 export_function_metrics path is skipped in export_sidecars_parallel.
        set_bundle_override(Some(BundleVersion::V2));
        assert_eq!(selected_version(), BundleVersion::V2);
        set_bundle_override(Some(BundleVersion::V1));
        assert_eq!(selected_version(), BundleVersion::V1);
        set_bundle_override(None);
    }

    /// Light mmap-backed export smoke when a local corpus snapshot is present.
    #[test]
    fn optional_mmap_backed_export_smoke() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/discourse");
        let snapshot_path = root.join(".rgctl/graph.snapshot.bin");
        if !snapshot_path.is_file() {
            return;
        }
        let Ok(snapshot) = rgctl_graph::snapshot::MmappedGraphSnapshot::open(&snapshot_path) else {
            return;
        };
        let tmp = TempDir::new().unwrap();
        // Point artifact root at a temp copy of .rgctl layout expectations via symlink-free
        // export into an isolated out dir by overriding via a fake repo with linked snapshot.
        let fake_repo = tmp.path().join("repo");
        let rgctl = fake_repo.join(".rgctl");
        fs::create_dir_all(&rgctl).unwrap();
        // Hard-link or copy snapshot so copy_graph_payload succeeds.
        let fake_snap = rgctl.join("graph.snapshot.bin");
        let _ = export_util::link_or_copy(&snapshot_path, &fake_snap);
        if !fake_snap.is_file() {
            return;
        }
        set_bundle_override(Some(BundleVersion::V2));
        let result = export_dashboard_bundle(GraphSource::mmap(&snapshot), &fake_repo, &fake_snap);
        set_bundle_override(None);
        let Ok(()) = result else {
            return;
        };
        let dash = bundle::default_dashboard_path(&fake_repo);
        assert!(dash.join("manifest.json").is_file());
        assert!(
            !dash
                .join(function_metrics_export::FUNCTION_METRICS_FILE)
                .exists()
        );
        assert!(!dash.join(blast_export::BLAST_SNAPSHOT_BUNDLE_NAME).exists());
        assert!(dash.join("metagraph.json").is_file());
    }

    #[test]
    fn memory_graph_export_builds_without_snapshot_fails_cleanly() {
        let tmp = TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let err = export_dashboard_bundle(
            GraphSource::memory(&backend),
            tmp.path(),
            &tmp.path().join("missing.bin"),
        )
        .unwrap_err();
        assert!(err.contains("graph snapshot not found"));
    }

    #[test]
    fn payload_stats_and_collect_metrics_work_on_mmap_with_analysis() {
        use rgctl_analysis::{AnalysisResults, BlastRadiusTable};
        use rgctl_graph::backend::GraphBackend;
        use rgctl_graph::columnar_snapshot::write_columnar_from_backend;
        use rgctl_graph::schema::{Edge, Node};
        use rgctl_graph::snapshot::MmappedGraphSnapshot;

        let mut backend = MemoryBackend::new();
        let fn_high = Node::new(NodeType::Function, "hot_fn");
        let fn_low = Node::new(NodeType::Function, "cold_fn");
        let cls = Node::new(NodeType::Class, "Service");
        let id_high = fn_high.id;
        let id_low = fn_low.id;
        let id_cls = cls.id;
        backend.insert_node(fn_high).unwrap();
        backend.insert_node(fn_low).unwrap();
        backend.insert_node(cls).unwrap();
        backend
            .insert_edge(Edge::new(id_high, id_low, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_cls, id_high, EdgeType::Contains))
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let snap_path = tmp.path().join("graph.snapshot.bin");
        let expected_digest = write_columnar_from_backend(&backend, &snap_path).unwrap();
        let snap = MmappedGraphSnapshot::open(&snap_path).unwrap();
        let src = GraphSource::mmap(&snap);

        // 1. payload_stats reads from GraphSource::Mmap even if snapshot_path does not exist.
        let (nodes, edges, digest) =
            payload_stats(&tmp.path().join("nonexistent.bin"), src).unwrap();
        assert_eq!(nodes, 3);
        assert_eq!(edges, 2);
        assert_eq!(digest, expected_digest);

        // 2. payload_stats 92-byte header fallback when GraphSource is Memory (no mmap digest).
        let (nodes_f, edges_f, digest_f) =
            payload_stats(&snap_path, GraphSource::memory(&backend)).unwrap();
        assert_eq!(nodes_f, 3);
        assert_eq!(edges_f, 2);
        assert_eq!(digest_f, expected_digest);

        // 3. collect_metrics counts Calls edges and high_blast_radius_count from AnalysisResults.
        let mut results = AnalysisResults::new(vec![id_high, id_low, id_cls]);
        let mut blast = BlastRadiusTable::with_capacity(3);
        blast.scores[0] = 85.0; // > 50.0 (high blast)
        blast.scores[1] = 12.0; // <= 50.0
        results.blast_radius = Some(blast);

        let metrics = collect_metrics(src, Some(&results));
        assert_eq!(metrics.function_count, 2);
        assert_eq!(metrics.class_count, 1);
        assert_eq!(metrics.calls_count, 1);
        assert_eq!(metrics.high_blast_radius_count, 1);
    }
}
