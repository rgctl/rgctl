//! Discover implementation (index + analyze pipeline).

use super::args::OutputFormat;
use super::context::CliContext;
use rgctl_pipeline::with_large_pool;
use super::discover_cfg::{
    CfgAnalysisOptions, FileSourceCache, preload_file_sources, run_cfg_analysis_batch,
};
use super::discover_limits::{check_memory_budget, DiscoverLimits};
use super::discover_output::{DiscoverArtifacts, build_discover_response};
use super::stage_profile::{DiscoverStageReport, secs};
use crate::analysis::graph_utils::PetGraphView;
use crate::analysis::{
    AnalysisResults, AnalysisStorage, AstSkeletonArchive, BlastEngineSnapshot, BlastRadiusEngine,
    CentralityAnalyzer, CfgPdgArchive, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer,
    FIELD_WRITE_INDEX_VERSION, FieldWriteIndex, MacroCallIndex, MacroCallLookupDb, NodeLookup,
    build_and_save_field_write_index, cfg_language_list, AST_SKELETON_VERSION,
};
use crate::config::secret_detector::{DetectedSecret, SecretDetector};
use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::incremental::FileTracker;
use crate::languages::registry::LanguageRegistry;
use crate::pipeline::{PipelineConfig, PipelineStats, ProcessingPipeline};
use anyhow::Result;
use rayon::prelude::*;
use rgctl_core::memory::MemoryMonitor;
use rgctl_graph::schema::NodeType;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, info_span, warn};

/// Discover analysis flags and output paths (replaces a 16-argument function signature).
#[derive(Debug, Clone)]
pub(crate) struct AnalysisOptions<'a> {
    pub languages: Option<String>,
    pub exclude: Option<String>,
    pub with_security: bool,
    pub with_cfg: bool,
    pub with_taint: bool,
    pub taint_rules: Option<String>,
    pub with_dfg_loops: bool,
    pub with_ast_skeleton: bool,
    pub write_json_graph: bool,
    pub with_dashboard: bool,
    pub export_migration_hints: bool,
    pub with_harmonic: bool,
    pub with_kantra: bool,
    pub kantra_rules: Option<String>,
    pub kantra_catalog: Option<String>,
    pub kantra_target: Option<String>,
    pub kantra_index_only: bool,
    pub migration_preset: &'a str,
    pub migration_order: &'a str,
    pub db_path: &'a Path,
    /// Index field members even when CFG is off (full pipeline stage 1).
    pub force_materialize_fields: bool,
    /// Do not reuse an existing snapshot (re-extract).
    pub force_reindex: bool,
    /// Emit discover JSON / "next steps" footer (disable when nested in `--full`).
    pub emit_cli_summary: bool,
    /// Persist snapshots under this root (defaults to the scanned `path`).
    pub artifact_root: Option<&'a Path>,
    /// Opt-in resource limits (`--with-limits`).
    pub limits: Option<DiscoverLimits>,
    /// Reuse Stage 1 analytics artifacts when `graph_from_snapshot` (STAGE_DEEP).
    pub reuse_basic_analytics: bool,
}

/// Result of one `run_full_analysis` pass.
#[derive(Debug, Clone)]
pub(crate) struct DiscoverRunOutcome {
    pub response: super::discover_output::DiscoverJsonResponse,
    #[allow(dead_code)]
    pub reused_snapshot: bool,
    pub graph_digest: String,
}

pub(crate) fn run_full_analysis(
    ctx: &CliContext,
    path: &str,
    opts: AnalysisOptions<'_>,
) -> Result<DiscoverRunOutcome> {
    let AnalysisOptions {
        languages,
        exclude,
        with_security,
        with_cfg,
        with_taint,
        taint_rules,
        with_dfg_loops,
        with_ast_skeleton,
        write_json_graph,
        with_dashboard,
        export_migration_hints,
        with_harmonic,
        with_kantra,
        kantra_rules,
        kantra_catalog,
        kantra_target,
        kantra_index_only,
        migration_preset,
        migration_order,
        db_path,
        force_materialize_fields,
        force_reindex,
        emit_cli_summary,
        artifact_root,
        limits,
        reuse_basic_analytics,
    } = opts;

    let verbose = ctx.verbose;
    let json_output = ctx.format == OutputFormat::Json;
    let human_output = !json_output;
    let run_start = Instant::now();
    let mut profile = DiscoverStageReport::default();
    // Taint / DFG loops / AST skeleton need CFG/PDG.
    let run_cfg_pass = with_cfg || with_taint || with_dfg_loops || with_ast_skeleton;
    let materialize_fields = run_cfg_pass || force_materialize_fields;
    profile.cfg_enabled = run_cfg_pass;
    profile.security_enabled = with_security;

    if let Some(ref lim) = limits {
        lim.log_active();
        if let Some(bytes) = lim.sort_run_bytes() {
            rgctl_graph::set_sort_run_bytes_override(Some(bytes));
        }
        if let Some(n) = lim.threads {
            // Help any code paths that still use the global Rayon pool.
            if std::env::var_os("RAYON_NUM_THREADS").is_none() {
                // SAFETY: single-threaded init before worker pools start.
                unsafe { std::env::set_var("RAYON_NUM_THREADS", n.to_string()) };
            }
        }
    }
    // Clear spill sort-run override even on early return / error.
    struct ClearSortRunOverride;
    impl Drop for ClearSortRunOverride {
        fn drop(&mut self) {
            rgctl_graph::set_sort_run_bytes_override(None);
        }
    }
    let _clear_sort_run = ClearSortRunOverride;

    let root = Path::new(path);
    // Source tree scanned for files; `.rgctl/` artifacts live under `store`.
    let store = artifact_root.unwrap_or(root);
    let mut discovery = DiscoveryConfig::default();

    if let Some(langs) = languages {
        discovery.languages = Some(
            langs
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        );
    }
    let taint_lang_scope = discovery.languages.clone();

    if let Some(excludes) = exclude {
        discovery.exclude_patterns = excludes
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    // Create analysis span for the entire operation (verbose mode only)
    let _analysis_span = if verbose {
        Some(info_span!("analysis", repo = %root.display()).entered())
    } else {
        None
    };

    if human_output {
        info!("==> Analyzing: {}", root.display());
    }

    if run_cfg_pass && human_output {
        warn!("[!] Deep analysis enabled (--with-cfg / --with-taint).");
        warn!("   CFG/PDG on large codebases (>50K functions) may take several minutes.");
    }

    // Initialize memory monitoring with periodic peak sampling (#33).
    let mut mem_monitor = MemoryMonitor::new();
    mem_monitor.start_periodic_sampling(std::time::Duration::from_millis(250));

    let discovery_config = discovery.clone();
    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            discovery,
            show_progress: human_output,
            materialize_fields,
            thread_count: limits.as_ref().and_then(|l| l.threads),
            stream_channel_capacity: limits
                .as_ref()
                .and_then(|l| l.stream_channel_capacity())
                .unwrap_or_else(|| PipelineConfig::default().stream_channel_capacity),
            ..PipelineConfig::default()
        },
    );

    // Discover files (used for indexing and later for security/tracking)
    let discoverer = FileDiscoverer::with_config(Arc::clone(&registry), discovery_config.clone());
    let files = discoverer.discover(root)?;

    let snapshot_path = rgctl_graph::snapshot::MmappedGraphSnapshot::default_path(store);
    let mut file_tracker = FileTracker::load(store).unwrap_or_else(|_| FileTracker::new(store));
    let file_changes = file_tracker.detect_changes(&files)?;

    // Index the repository (or reuse snapshot when sources are unchanged).
    // Lever 1: write columnar from GraphBuilder Vecs — never build MemoryBackend for discover.
    let index_start = Instant::now();
    let graph_from_snapshot = !force_reindex && file_changes.is_empty() && snapshot_path.is_file();
    let mut cold_reused: Option<crate::analysis::ColdMetadataDb> = None;
    let (mut index_stats, graph_digest) = if graph_from_snapshot {
        let load_start = Instant::now();
        let cold = crate::analysis::ColdMetadataDb::open(&snapshot_path)?;
        let digest = cold.store().content_digest()?.to_string();
        let load_elapsed = load_start.elapsed();
        if verbose {
            debug!(
                path = %snapshot_path.display(),
                nodes = cold.node_count(),
                edges = cold.edge_count(),
                "No file changes — reusing columnar snapshot (no hydrate)"
            );
        }
        let stats = PipelineStats {
            files_discovered: files.len(),
            files_processed: files.len(),
            files_failed: 0,
            nodes_created: cold.node_count(),
            edges_created: cold.edge_count(),
            duration: load_elapsed,
            extract_duration: Duration::default(),
            graph_build_duration: load_elapsed,
            ..Default::default()
        };
        cold_reused = Some(cold);
        (stats, digest)
    } else {
        std::fs::create_dir_all(rgctl_graph::paths::artifact_dir(store))?;
        let (stats, digest) = pipeline.process_repository_to_snapshot(root, &snapshot_path, Some(store))?;
        if verbose {
            debug!(
                path = %snapshot_path.display(),
                "Graph binary snapshot compiled from segmented spill (no MemoryBackend / no Vec staging)"
            );
        }
        (stats, digest)
    };
    profile.index_pipeline.secs = secs(index_start.elapsed());
    profile.index_extract.secs = secs(index_stats.extract_duration);
    profile.extract_pass1.secs = secs(index_stats.extract_pass1_wall);
    profile.extract_read_cpu.secs = secs(index_stats.extract_read_cpu);
    profile.extract_parse_cpu.secs = secs(index_stats.extract_parse_cpu);
    profile.index_graph_build.secs = secs(index_stats.graph_build_duration);
    profile.graph_resolution_index.secs = secs(index_stats.graph_resolution_index);
    profile.graph_pass2.secs = secs(index_stats.graph_pass2);
    profile.graph_spill_columnar.secs = secs(index_stats.graph_spill_columnar);
    profile.nodes = index_stats.nodes_created;
    // Snapshot write is folded into index_graph_build (Lever 1: no separate backend rewrite).
    profile.save_snapshot.secs = 0.0;

    if materialize_fields {
        let _ = super::pipeline_status::write_digest_marker(
            store,
            super::pipeline_status::MATERIALIZED_FIELDS_DIGEST_FILE,
            &graph_digest,
        );
    }

    if human_output {
        if graph_from_snapshot {
            info!(
                "[✓] Loaded {} files from snapshot -> {} nodes, {} edges ({:.1}s)",
                index_stats.files_discovered,
                index_stats.nodes_created,
                index_stats.edges_created,
                index_stats.duration.as_secs_f64()
            );
        } else if verbose {
            info!(
                files = index_stats.files_processed,
                nodes = index_stats.nodes_created,
                edges = index_stats.edges_created,
                duration_secs = %format!("{:.1}", index_stats.duration.as_secs_f64()),
                "[✓] Indexed {} files -> {} nodes, {} edges ({:.1}s)",
                index_stats.files_processed,
                index_stats.nodes_created,
                index_stats.edges_created,
                index_stats.duration.as_secs_f64()
            );
        } else {
            info!(
                "[✓] Indexed {} files -> {} nodes, {} edges ({:.1}s)",
                index_stats.files_processed,
                index_stats.nodes_created,
                index_stats.edges_created,
                index_stats.duration.as_secs_f64()
            );
        }
    }

    if index_stats.files_failed > 0 {
        warn!(
            failed = index_stats.files_failed,
            "Skipped files due to errors"
        );
    }

    debug!("{}", mem_monitor.report());

    // Cold metadata + CSR from snapshot — no fat CodeGraph through analysis (#33 / Lever 1).
    let cold = match cold_reused {
        Some(cold) => cold,
        None => crate::analysis::ColdMetadataDb::open(&snapshot_path)?,
    };

    let analysis_path = rgctl_graph::paths::artifact_path(store, "analysis_results.bin");
    let reuse_ok = reuse_basic_analytics
        && graph_from_snapshot
        && basic_analytics_reusable(store, &graph_digest, &analysis_path);

    let functions;
    let mut analysis_results;
    let mut petgraph_view: Option<PetGraphView> = None;

    if reuse_ok {
        analysis_results = AnalysisResults::load(&analysis_path)?;
        functions = cold.collect_nodes_by_type(NodeType::Function)?;
        profile.functions = functions.len();
        profile.ingest_peak_rss_mb = mem_monitor.seal_phase().unwrap_or(0.0);
        if let Some(limit) = limits.as_ref().and_then(|l| l.max_mem_mb) {
            check_memory_budget(profile.ingest_peak_rss_mb, limit)?;
        }
        if human_output {
            info!(
                "[✓] Reused Stage 1 analytics ({} functions)",
                functions.len()
            );
        }
        if with_harmonic {
            let harmonic_current = super::pipeline_status::read_digest_marker(
                store,
                super::pipeline_status::HARMONIC_DIGEST_FILE,
            );
            if harmonic_current.as_deref() != Some(graph_digest.as_str()) {
                let centrality_start = Instant::now();
                let view = PetGraphView::from_snapshot_store(cold.store())?;
                let _ = CentralityAnalyzer::new()
                    .with_harmonic(true)
                    .analyze_columnar(&view, &mut analysis_results)?;
                profile.centrality.secs = secs(centrality_start.elapsed());
                let _ = super::pipeline_status::write_digest_marker(
                    store,
                    super::pipeline_status::HARMONIC_DIGEST_FILE,
                    &graph_digest,
                );
            }
        }
        debug!("{}", mem_monitor.report());
    } else {
        // Initialize columnar analysis results
        let mut node_ids = cold.store().all_node_ids();
        node_ids.sort_unstable();
        analysis_results = AnalysisResults::new(node_ids);

        // Complexity from cold mmap payloads.
        let complexity_start = Instant::now();
        let complexity_report = ComplexityAnalyzer::analyze_lookup(&cold)?;
        analysis_results.fill_complexity(&complexity_report);
        profile.complexity.secs = secs(complexity_start.elapsed());
        if verbose {
            debug!("✓ Complexity analysis:");
            debug!("  Functions: {}", complexity_report.functions.len());
            debug!("  Avg cyclomatic: {:.1}", complexity_report.avg_cyclomatic);
            debug!("  Max cyclomatic: {}", complexity_report.max_cyclomatic);
        }
        let high_complexity = complexity_report
            .by_level
            .get(&crate::analysis::ComplexityLevel::High)
            .copied()
            .unwrap_or(0);
        let medium_complexity = complexity_report
            .by_level
            .get(&crate::analysis::ComplexityLevel::Medium)
            .copied()
            .unwrap_or(0);
        if human_output {
            info!(
                "[✓] Analyzed {} functions (avg complexity: {:.1}, {} high, {} medium)",
                complexity_report.functions.len(),
                complexity_report.avg_cyclomatic,
                high_complexity,
                medium_complexity
            );
        }
        debug!("{}", mem_monitor.report());

        // CSR topology from columnar snapshot.
        let topo_start = Instant::now();
        let view = {
            let _span = if verbose {
                Some(info_span!("topology").entered())
            } else {
                None
            };
            let view = PetGraphView::from_snapshot_store(cold.store())?;
            debug!(
                nodes = view.node_count(),
                edges = view.edge_count(),
                "CSR topology view built"
            );
            view
        };
        profile.topology.secs = secs(topo_start.elapsed());

        functions = cold.collect_nodes_by_type(NodeType::Function)?;
        profile.functions = functions.len();
        // Seal ingest phase: absolute peak stays; analysis phase peak resets to current RSS.
        profile.ingest_peak_rss_mb = mem_monitor.seal_phase().unwrap_or(0.0);
        if let Some(limit) = limits.as_ref().and_then(|l| l.max_mem_mb) {
            check_memory_budget(profile.ingest_peak_rss_mb, limit)?;
        }
        debug!(
            ingest_peak_mb = profile.ingest_peak_rss_mb,
            "{}",
            mem_monitor.report()
        );

        // Community detection - write to columnar table
        let community_start = Instant::now();
        let community_result =
            CommunityDetector::new().detect_with_view_defaults(&view, cold.store())?;
        analysis_results.fill_community(&community_result);
        profile.community.secs = secs(community_start.elapsed());
        if human_output {
            let n_communities = community_result.communities.len();
            let n_nodes = view.node_count().max(1);
            let mut call_use_edges = 0usize;
            let _ = view.for_each_edge(|_, _, ty| {
                if matches!(
                    ty,
                    rgctl_graph::schema::EdgeType::Calls | rgctl_graph::schema::EdgeType::Uses
                ) {
                    call_use_edges += 1;
                }
            });
            // Sparse Calls/Uses relative to nodes ≈ flat call graph (communities ≈ isolates).
            let flat_call_graph = call_use_edges == 0 || call_use_edges * 10 < n_nodes;
            let degenerate = n_communities * 10 >= n_nodes * 9;
            if flat_call_graph || degenerate {
                warn!(
                    communities = n_communities,
                    nodes = n_nodes,
                    call_use_edges,
                    flat_call_graph,
                    degenerate,
                    "[!] Community partition may be meaningless (flat call graph or near 1:1 clustering)"
                );
                info!(
                    "[!] Communities: {} (modularity: {:.2}) — warn: flat or degenerate clustering",
                    n_communities, community_result.modularity
                );
            } else {
                info!(
                    "[✓] Detected {} communities (modularity: {:.2})",
                    n_communities, community_result.modularity
                );
            }
        }
        debug!("{}", mem_monitor.report());

        // Centrality: PageRank + betweenness always; harmonic only with --with-harmonic
        // (HyperBall dominates wall/RSS on flat kernel-scale graphs — #29).
        let centrality_start = Instant::now();
        let centrality_summary = CentralityAnalyzer::new()
            .with_harmonic(with_harmonic)
            .analyze_columnar(&view, &mut analysis_results)?;
        profile.centrality.secs = secs(centrality_start.elapsed());
        if with_harmonic {
            let _ = super::pipeline_status::write_digest_marker(
                store,
                super::pipeline_status::HARMONIC_DIGEST_FILE,
                &graph_digest,
            );
        }
        if verbose && !with_harmonic {
            debug!("Harmonic centrality skipped (pass --with-harmonic to enable)");
        }
        let has_betweenness = centrality_summary.has_betweenness;

        if human_output {
            if let Some((top_id, top_score)) = centrality_summary.top_pagerank.first() {
                if let Ok(Some(node)) = cold.get_node(*top_id) {
                    let short_name = node.name.split('/').next_back().unwrap_or(&node.name);
                    let (in_degree, out_degree) = analysis_results
                        .get_centrality(*top_id)
                        .map(|m| (m.in_degree, m.out_degree))
                        .unwrap_or((0, 0));

                    if verbose {
                        info!(
                            hotspot = short_name,
                            pagerank = %format!("{:.4}", top_score),
                            betweenness_enabled = has_betweenness,
                            in_degree,
                            out_degree,
                            "[*] Top hotspot: {} (PageRank: {:.4})",
                            short_name,
                            top_score
                        );
                    } else {
                        info!(
                            "[*] Top hotspot: {} (PageRank: {:.4})",
                            short_name, top_score
                        );
                    }
                }
            }
        }

        debug!("{}", mem_monitor.report());

        // Name communities after centrality so PageRank can influence labels.
        {
            let infra = analysis_results
                .community
                .as_ref()
                .and_then(|c| c.infrastructure_community_id);
            let _ = rgctl_analysis::fill_community_labels(&mut analysis_results, infra, |uuid| {
                cold.get_node(uuid).ok().flatten().map(|n| {
                    (
                        n.name.to_string(),
                        n.file_path.as_ref().map(|s| s.to_string()),
                    )
                })
            });
        }

        // Dependency analysis
        let dependency_start = Instant::now();
        let cycles = DependencyAnalyzer::find_circular_dependencies_with_lookup(&view, &cold)?;
        profile.dependency.secs = secs(dependency_start.elapsed());
        if !cycles.is_empty() && human_output {
            if verbose {
                warn!(
                    count = cycles.len(),
                    "[!] Found {} circular dependencies",
                    cycles.len()
                );
            } else {
                warn!("[!] Found {} circular dependencies", cycles.len());
            }
        } else if cycles.is_empty() {
            debug!("No circular dependencies found");
        }
        petgraph_view = Some(view);
    }

    // Kantra rule evaluation (opt-in with --with-kantra). Index runs later after all
    // `cold` mmap use — rewriting `graph.snapshot.bin` while ColdMetadataDb is open corrupts reads.
    let kantra_rules_path = with_kantra.then(|| kantra_rules.as_ref().map(std::path::PathBuf::from));
    let kantra_catalog_path =
        with_kantra.then(|| kantra_catalog.as_ref().map(std::path::PathBuf::from));
    let mut kantra_eval_graph = None;
    if with_kantra && !kantra_index_only {
        if petgraph_view.is_none() {
            petgraph_view = Some(PetGraphView::from_snapshot_store(cold.store())?);
        }
        let view = petgraph_view
            .as_ref()
            .expect("petgraph view for kantra");
        let kantra_start = Instant::now();
        let (_findings, graph) = super::kantra_discover::run_kantra_stage(
            root,
            store,
            kantra_rules_path.as_ref().and_then(|p| p.as_deref()),
            kantra_catalog_path.as_ref().and_then(|p| p.as_deref()),
            kantra_target.as_deref(),
            &files,
            &cold,
            view,
            &mut profile,
        )?;
        kantra_eval_graph = Some(graph);
        if human_output {
            info!(
                "[✓] Kantra evaluation complete ({:.1}s)",
                kantra_start.elapsed().as_secs_f64()
            );
        }
    }

    // Security analysis (opt-in with --with-security)
    if with_security {
        let security_start = Instant::now();
        if human_output {
            println!("\n✓ Security analysis:");
        }
        let findings: Vec<(PathBuf, Vec<DetectedSecret>)> = files
            .par_iter()
            .take(100)
            .filter_map(|file| {
                let content = std::fs::read_to_string(file).ok()?;
                let secrets = SecretDetector::new().scan(&content);
                if secrets.is_empty() {
                    None
                } else {
                    Some((file.clone(), secrets))
                }
            })
            .collect();

        let total_secrets: usize = findings.iter().map(|(_, secrets)| secrets.len()).sum();

        if verbose {
            for (file, secrets) in &findings {
                for secret in secrets {
                    println!(
                        "  [{}] {}:{} - {} ({:?})",
                        file.display(),
                        secret.line,
                        secret.secret_type,
                        secret.value,
                        secret.severity
                    );
                }
            }
        }
        if human_output {
            println!("  Potential secrets found: {total_secrets}");
        }
        profile.security.secs = secs(security_start.elapsed());
    }

    let output_dir = rgctl_graph::paths::artifact_path(store, "analysis");

    // CFG/PDG (+ optional taint) — opt-in with --with-cfg / --with-taint
    if run_cfg_pass {
        let cfg_start = Instant::now();
        if human_output {
            println!("\n✓ Control flow analysis:");
        }
        let storage = AnalysisStorage::new(&output_dir);
        storage.ensure_dir()?;

        if with_taint && !with_cfg && verbose {
            debug!("--with-taint implies CFG/PDG pass");
        }

        let file_sources: Option<FileSourceCache> = if with_ast_skeleton || with_cfg {
            Some(preload_file_sources(
                &functions,
                root,
                limits.as_ref().and_then(|l| l.threads),
            ))
        } else {
            None
        };

        let taint_rule_set = if with_taint {
            let cli = taint_rules.as_ref().map(std::path::PathBuf::from);
            let langs = taint_lang_scope.as_ref();
            match rgctl_analysis::taint_rules::TaintRuleSet::load_with_overlays(
                Some(Path::new(path)),
                cli.as_deref(),
                langs.map(|v| v.as_slice()),
            ) {
                Ok(set) => Some(std::sync::Arc::new(set)),
                Err(err) => {
                    warn!(error = %err, "failed to load taint rule packs; using bundled defaults");
                    rgctl_analysis::taint_rules::TaintRuleSet::bundled()
                        .ok()
                        .map(std::sync::Arc::new)
                }
            }
        } else {
            None
        };

        let batch = run_cfg_analysis_batch(
            &functions,
            &storage,
            root,
            CfgAnalysisOptions {
                verbose,
                thread_count: limits.as_ref().and_then(|l| l.threads),
                enable_taint: with_taint,
                dfg_loops: with_dfg_loops,
                taint_rules: taint_rule_set,
                with_ast_skeleton,
            },
            file_sources.as_ref(),
        );
        let success_count = batch.success_count;
        let error_count = batch.error_count;
        profile.cfg_total.secs = secs(cfg_start.elapsed());
        if let Some(sp) = batch.stage_profile {
            profile.cfg_build.secs = sp.build_cfg_secs;
            profile.cfg_dominator.secs = sp.dominator_secs;
            profile.cfg_pdg.secs = sp.pdg_secs;
            profile.cfg_taint.secs = sp.taint_secs;
        }

        let archive_path = CfgPdgArchive::default_path(store);
        let archive_start = Instant::now();
        if batch.archive_unchanged {
            if verbose {
                debug!(
                    skipped = batch.skipped_unchanged,
                    "CFG/PDG archive unchanged — skipping rewrite"
                );
            }
        } else if !batch.archive_record_bytes.is_empty() {
            if let Err(err) = CfgPdgArchive::write_preencoded_to_path(
                &archive_path,
                Some(&graph_digest),
                &batch.archive_record_bytes,
            ) {
                warn!(error = %err, "Failed to save cfg_pdg archive");
            } else if verbose {
                debug!(
                    path = %archive_path.display(),
                    entries = batch.archive_record_bytes.len(),
                    "CFG/PDG archive saved (preencoded)"
                );
            }
        } else if !batch.archive_records.is_empty() {
            let mut merged = CfgPdgArchive::open_if_exists(store)
                .ok()
                .flatten()
                .unwrap_or_default();
            merged.graph_digest = Some(graph_digest.clone());
            for record in batch.archive_records {
                merged.insert(record);
            }
            if let Err(err) = merged.write_to_path(&archive_path) {
                warn!(error = %err, "Failed to save cfg_pdg archive");
            }
        }
        profile.cfg_archive.secs = secs(archive_start.elapsed());

        // Field-write index: prefer fused worker results; fall back to archive only when needed.
        let fw_start = Instant::now();
        let fw_path = FieldWriteIndex::default_path(store);
        let fw_count = if batch.archive_unchanged
            && FieldWriteIndex::open_if_exists(store)
                .ok()
                .flatten()
                .is_some_and(|idx| idx.graph_digest.as_deref() == Some(graph_digest.as_str()))
        {
            if verbose {
                debug!("field_write index unchanged — skipping rewrite");
            }
            FieldWriteIndex::open_if_exists(store)
                .ok()
                .flatten()
                .map(|i| i.writes.len())
                .unwrap_or(0)
        } else if !batch.field_writes.is_empty() || batch.recomputed > 0 {
            let mut writes = batch.field_writes;
            if batch.cache_hits > 0 {
                if let Ok(Some(existing)) = FieldWriteIndex::open_if_exists(store) {
                    let recomputed_ids: std::collections::HashSet<_> = batch
                        .index_entries
                        .iter()
                        .map(|e| e.function_id)
                        .collect();
                    for w in existing.writes {
                        if !recomputed_ids.contains(&w.function_id) {
                            writes.push(w);
                        }
                    }
                }
            }
            writes.sort_by(|a, b| {
                (&a.file, a.line, &a.function_id, &a.member, &a.code_snippet).cmp(&(
                    &b.file,
                    b.line,
                    &b.function_id,
                    &b.member,
                    &b.code_snippet,
                ))
            });
            let count = writes.len();
            let index = FieldWriteIndex {
                version: FIELD_WRITE_INDEX_VERSION,
                graph_digest: Some(graph_digest.clone()),
                writes,
            };
            if let Err(err) = index.write_to_path(&fw_path) {
                warn!(error = %err, "Failed to save field_write index");
                0
            } else {
                count
            }
        } else {
            let preloaded = file_sources.as_ref().map(|cache| cache.sources());
            match with_large_pool(None, || {
                let archive = CfgPdgArchive::open_if_exists(store)?;
                let Some(archive) = archive else {
                    return Ok::<(PathBuf, usize), rgctl_error::Error>((fw_path.clone(), 0));
                };
                build_and_save_field_write_index(
                    store,
                    &archive,
                    &functions,
                    Some(graph_digest.clone()),
                    preloaded,
                )
            }) {
                Ok((_, count)) => count,
                Err(err) => {
                    warn!(error = %err, "Failed to save field_write index");
                    0
                }
            }
        };
        if verbose && fw_count > 0 {
            debug!(
                path = %fw_path.display(),
                writes = fw_count,
                "field_write index saved"
            );
        }
        if human_output && fw_count > 0 {
            println!("  Field writes indexed: {fw_count}");
        }
        profile.field_write.secs = secs(fw_start.elapsed());

        if with_ast_skeleton {
            let mut records = batch.skeletons;
            if batch.cache_hits > 0 && records.len() < success_count {
                if let Ok(Some(existing)) = AstSkeletonArchive::open_if_exists(store) {
                    let seen: std::collections::HashSet<_> = records
                        .iter()
                        .filter_map(|r| r.function_id)
                        .collect();
                    for rec in existing.records {
                        let keep = match rec.function_id {
                            Some(id) => !seen.contains(&id),
                            None => true,
                        };
                        if keep {
                            records.push(rec);
                        }
                    }
                }
            }
            let skel = AstSkeletonArchive {
                version: AST_SKELETON_VERSION,
                graph_digest: Some(graph_digest.clone()),
                records,
            };
            let skel_path = AstSkeletonArchive::default_path(store);
            match skel.write_to_path(&skel_path) {
                Ok(()) => {
                    if human_output {
                        println!("  AST skeletons: {} functions", skel.records.len());
                    }
                }
                Err(err) => warn!(error = %err, "Failed to save AST skeleton archive"),
            }
        }

        if human_output {
            if success_count > 0 {
                println!("  CFG/PDG/Dominance: {} functions analyzed", success_count);
                if error_count > 0 {
                    println!(
                        "  Skipped: {} functions (unsupported_language={}, missing_source={}, analysis_error={})",
                        error_count,
                        batch.skip_unsupported_language,
                        batch.skip_missing_source,
                        batch.skip_analysis_error
                    );
                }

                if batch.total_flows > 0 {
                    println!(
                        "  Taint flows: {} total ({} vulnerable)",
                        batch.total_flows, batch.vulnerable_flows
                    );
                }
            } else if !functions.is_empty() {
                println!(
                    "  No functions analyzed (CFG supported: {})",
                    cfg_language_list()
                );
                if error_count > 0 {
                    println!(
                        "  Skipped: {} functions (unsupported_language={}, missing_source={}, analysis_error={})",
                        error_count,
                        batch.skip_unsupported_language,
                        batch.skip_missing_source,
                        batch.skip_analysis_error
                    );
                }
            }
            if verbose {
                if batch.cache_hits > 0 || batch.recomputed > 0 || batch.skipped_unchanged > 0 {
                    println!(
                        "  CFG cache: {} reused ({} unchanged), {} recomputed, {} stale artifacts removed",
                        batch.cache_hits,
                        batch.skipped_unchanged,
                        batch.recomputed,
                        batch.orphans_removed
                    );
                }
                println!("{}", mem_monitor.report());
            }
        }
        if verbose {
            for skip in &batch.skips {
                info!(
                    target: "profile",
                    path = %skip.file_path,
                    symbol = %skip.symbol,
                    reason = skip.reason.as_str(),
                    "[profile] cfg skip"
                );
            }
        }
    }

    // Blast radius analysis with SCC + Dense Bitsets engine
    let blast_start = Instant::now();
    let mut high_impact_count = 0;
    let mut max_impact_score = 0.0f64;
    let mut max_impact_function = String::new();
    let mut in_cycle_count = 0;
    let build_time;
    let query_time;

    if reuse_ok {
        drop(petgraph_view);
        build_time = Duration::default();
        query_time = Duration::default();
        if verbose {
            debug!("Reused blast engine snapshot + macro call index — skipping Stage 1 rebuild");
        }
    } else {
        let view = match petgraph_view.take() {
            Some(v) => v,
            None => PetGraphView::from_snapshot_store(cold.store())?,
        };
        // Build SCC engine (one-time cost: Tarjan's + topo sort + bitset propagation)
        let engine = match BlastRadiusEngine::build_from_view_lookup(&cold, &view) {
            Ok(e) => e,
            Err(err) => {
                error!(error = %err, "[x] Blast radius engine build failed");
                info!("[✓] Analysis complete");
                return Ok(discover_outcome(
                    &index_stats,
                    run_start.elapsed().as_millis() as u64,
                    graph_from_snapshot,
                    graph_digest.clone(),
                ));
            }
        };
        // Topology view is fully consumed into the SCC engine — free DiGraph + UUID maps now.
        drop(view);
        debug!("{}", mem_monitor.report());

        if with_kantra && !kantra_index_only {
            if let Some(ref graph) = kantra_eval_graph {
                super::kantra_discover::run_kantra_enrich(
                    store,
                    &cold,
                    &analysis_results,
                    &engine,
                    graph,
                    &mut profile,
                )?;
            }
        }

        build_time = blast_start.elapsed();
        let engine_stats = engine.stats();

        debug!(
            scc_count = engine_stats.scc_count,
            dag_edges = engine_stats.dag_edges,
            build_time_secs = %format!("{:.2}", build_time.as_secs_f64()),
            compression_percent = %format!("{:.1}", (cold.node_count() - engine_stats.scc_count) as f64 / cold.node_count().max(1) as f64 * 100.0),
            avg_scc_size = %format!("{:.1}", engine_stats.avg_scc_size),
            memory_mb = %format!("{:.1}", engine_stats.memory_mb),
            "Blast radius engine built"
        );

        // Analyze all functions in parallel (O(1) lookup per function, read-only engine).
        // Flat graphs use on-demand reachability: skip bulk fill so discover does not
        // serialize ~O(functions) blast rows into macro_call_index / analysis_results
        // (linux cold: ~976s macro_index, multi‑GB artifacts). Live `blast-radius` still
        // works via the engine snapshot. See sshaaf/rgctl#28 (won't fix).
        let query_start = Instant::now();
        let skip_bulk_blast = engine.uses_on_demand_reachability();
        if skip_bulk_blast && verbose {
            debug!(
                functions = functions.len(),
                "Flat graph — skipping bulk blast-radius scan (use `blast-radius` for on-demand queries; #28)"
            );
        }
        let blast_results: Vec<(uuid::Uuid, crate::analysis::BlastRadiusResult)> = if skip_bulk_blast
        {
            Vec::new()
        } else {
            functions
                .par_iter()
                .filter_map(|func_node| {
                    engine
                        .analyze(func_node.id)
                        .ok()
                        .map(|result| (func_node.id, result))
                })
                .collect()
        };

        for (func_id, result) in &blast_results {
            if result.scc_size > 1 {
                in_cycle_count += 1;
            }
            if result.score > 50.0 {
                high_impact_count += 1;
            }
            if result.score > max_impact_score {
                max_impact_score = result.score;
                if let Ok(Some(node)) = cold.get_node(*func_id) {
                    max_impact_function = node.name.to_string();
                }
            }
        }

        query_time = query_start.elapsed();
        let blast_updates = blast_results;

        profile.blast_build.secs = secs(build_time);
        profile.blast_query.secs = secs(query_time);

        // Persist SCC engine snapshot for instant blast-radius cache misses
        let blast_snap_start = Instant::now();
        {
            let blast_path = BlastEngineSnapshot::default_path(store);
            if BlastEngineSnapshot::digest_matches(&blast_path, &graph_digest)? {
                if verbose {
                    debug!(
                        path = %blast_path.display(),
                        "Blast engine snapshot unchanged — skipping rewrite"
                    );
                }
            } else {
                let blast_snap = engine.to_engine_snapshot(graph_digest.clone());
                if let Err(err) = blast_snap.write_to_path(&blast_path) {
                    warn!(error = %err, "Failed to save blast engine snapshot");
                } else if verbose {
                    debug!(path = %blast_path.display(), "Blast engine snapshot saved");
                }
            }
        }
        profile.blast_snapshot.secs = secs(blast_snap_start.elapsed());

        // Serialize minimized macro-call index for instant blast-radius lookups
        let macro_start = Instant::now();
        {
            let macro_path = rgctl_graph::paths::artifact_path(store, "macro_call_index.bin");
            let lookup_db_path = MacroCallLookupDb::default_path(store);

            if MacroCallIndex::caches_are_current_counts(
                &macro_path,
                &lookup_db_path,
                store,
                cold.node_count(),
                cold.edge_count(),
                &graph_digest,
            )? {
                if verbose {
                    debug!(
                        path = %macro_path.display(),
                        "Macro call index unchanged — skipping rebuild"
                    );
                }
            } else {
                let fingerprint = crate::analysis::GraphFingerprint::from_topology_counts(
                    cold.node_count(),
                    cold.edge_count(),
                    Some(graph_digest.clone()),
                );
                let macro_index =
                    MacroCallIndex::from_results_with_lookup(&cold, &blast_updates, fingerprint)?;
                if let Err(err) = macro_index.save(&macro_path) {
                    warn!(error = %err, "Failed to save macro_call_index cache");
                } else if verbose {
                    debug!(
                        path = %macro_path.display(),
                        entries = macro_index.entries.len(),
                        "Macro call index saved"
                    );
                }

                let lookup_rows = macro_index.unique_lookup_rows();
                let candidate_rows = macro_index.all_candidate_rows();
                if let Err(err) = MacroCallLookupDb::replace_all(&lookup_db_path, &lookup_rows) {
                    warn!(error = %err, "Failed to save macro_call_index.db");
                } else if let Err(err) =
                    MacroCallLookupDb::replace_candidates(&lookup_db_path, &candidate_rows)
                {
                    warn!(error = %err, "Failed to save macro_call_candidates table");
                } else if let Err(err) = MacroCallLookupDb::write_meta_with_digest(
                    &lookup_db_path,
                    if write_json_graph {
                        std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0)
                    } else {
                        0
                    },
                    cold.node_count(),
                    cold.edge_count(),
                    Some(graph_digest.as_str()),
                ) {
                    warn!(error = %err, "Failed to write macro_call_index.db metadata");
                } else if verbose {
                    debug!(
                        path = %lookup_db_path.display(),
                        rows = lookup_rows.len(),
                        candidates = candidate_rows.len(),
                        "Macro call lookup DB saved"
                    );
                }
            }
        }
        profile.macro_index.secs = secs(macro_start.elapsed());

        // Write blast radius results to columnar table
        {
            let blast_data: Vec<_> = blast_updates
                .into_iter()
                .filter_map(|(node_id, result)| {
                    analysis_results
                        .get_compact_id(node_id)
                        .map(|compact_id| (compact_id, result))
                })
                .collect();

            let table = analysis_results.init_blast_radius();
            for (compact_id, result) in blast_data {
                let idx = compact_id as usize;
                table.scores[idx] = result.score as f32;
                table.direct_callers[idx] = result.direct_caller_ids.len() as u32;
                table.impact_zone_size[idx] = result.impact_zone_ids.len() as u32;
                table.scc_id[idx] = result.scc_id as u32;
                table.scc_size[idx] = result.scc_size as u32;
            }
        }
    }

    let analyzed_functions = functions.len();

    let total_time = blast_start.elapsed();

    if !max_impact_function.is_empty() && human_output {
        let short_name = max_impact_function
            .split('/')
            .next_back()
            .unwrap_or(&max_impact_function);

        if verbose {
            info!(
                function = short_name,
                score = %format!("{:.1}", max_impact_score),
                high_impact_count = high_impact_count,
                in_cycles = in_cycle_count,
                "[!] Highest impact: {} (score: {:.1}/100, {} high-impact functions)",
                short_name,
                max_impact_score,
                high_impact_count
            );
        } else {
            info!(
                "[!] Highest impact: {} (score: {:.1}/100, {} high-impact functions)",
                short_name, max_impact_score, high_impact_count
            );
        }
    }

    debug!(
        functions = analyzed_functions,
        build_time_secs = %format!("{:.2}", build_time.as_secs_f64()),
        query_time_secs = %format!("{:.3}", query_time.as_secs_f64()),
        total_time_secs = %format!("{:.2}", total_time.as_secs_f64()),
        "Blast radius analysis complete"
    );
    debug!("{}", mem_monitor.report());

    if human_output {
        info!("[✓] Analysis complete");
    }

    analysis_results.fill_structural_sketch_from_lookup(&cold)?;

    // Save analysis results (columnar format - separate from graph!)
    let save_analysis_start = Instant::now();
    std::fs::create_dir_all(rgctl_graph::paths::artifact_dir(store))?;
    analysis_results.save(&analysis_path)?;
    profile.save_analysis.secs = secs(save_analysis_start.elapsed());

    // Save graph topology (no analysis properties!)
    let save_tracker_start = Instant::now();
    if reuse_ok && graph_from_snapshot {
        // file_hashes.json already matches graph_digest — skip cold node-path sorting.
        if verbose {
            debug!("File tracker unchanged — skipping cold node-path sort");
        }
    } else {
        let node_mapping = if !index_stats.node_path_mapping.is_empty() {
            std::mem::take(&mut index_stats.node_path_mapping)
        } else {
            let mut node_path_pairs: Vec<(String, uuid::Uuid)> =
                Vec::with_capacity(cold.node_count());
            cold.for_each_node(&mut |node| {
                let raw_path = node.file_path.as_deref().or_else(|| {
                    if matches!(node.node_type, NodeType::File) {
                        Some(node.name.as_str())
                    } else {
                        None
                    }
                });
                if let Some(path) = raw_path {
                    node_path_pairs.push((crate::incremental::normalize_path_str(path), node.id));
                }
            })?;
            const PAR_SORT_NODE_PATHS_MIN: usize = 32_768;
            if node_path_pairs.len() >= PAR_SORT_NODE_PATHS_MIN {
                node_path_pairs.par_sort_unstable_by(|a, b| a.0.cmp(&b.0));
            } else {
                node_path_pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
            }
            crate::incremental::group_sorted_node_paths(node_path_pairs)
        };
        file_tracker.index_files_with_mapping(
            &files,
            node_mapping,
            Some(&index_stats.file_hashes),
        )?;
        file_tracker.save()?;
    }
    profile.save_tracker.secs = secs(save_tracker_start.elapsed());

    // Release the snapshot mmap before Kantra rewrites `graph.snapshot.bin`.
    // In-place truncate of a mapped file can SIGBUS or yield garbage length reads.
    drop(cold);

    if with_kantra {
        super::kantra_discover::run_kantra_index(
            store,
            kantra_rules_path.as_ref().and_then(|p| p.as_deref()),
            kantra_catalog_path.as_ref().and_then(|p| p.as_deref()),
            &mut profile,
        )?;
        let mut violates_count = 0usize;
        if !kantra_index_only {
            violates_count = super::kantra_discover::run_kantra_violates(store, &mut profile)?;
        }
        if human_output {
            if kantra_index_only {
                info!("[✓] Kantra rules indexed into graph (eval skipped)");
            } else {
                info!(
                    "[✓] Kantra rules indexed into graph ({violates_count} VIOLATES edges)"
                );
            }
        }
    }

    // Graph mmap snapshot was written early (before topology/analysis) to avoid
    // co-residency of PreparedGraphSnapshot with the live backend (#33).

    let mut hydrated: Option<rgctl_graph::code_graph::CodeGraph> = None;
    let need_hydrate = write_json_graph || with_dashboard || export_migration_hints;
    if need_hydrate {
        hydrated = Some(rgctl_graph::code_graph::CodeGraph::open_snapshot(
            &snapshot_path,
        )?);
    }

    if write_json_graph {
        let graph = hydrated.as_ref().expect("hydrated for json");
        let json = graph.export_json()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(db_path, &json)?;
        let saved = graph.save_to_repo(store)?;
        if verbose {
            debug!(path = %saved.display(), "Legacy JSON graph saved");
        }
    }

    // Export static dashboard bundle only when requested (#31).
    let save_dashboard_start = Instant::now();
    let dashboard_dir = rgctl_graph::paths::artifact_path(store, "dashboard");
    let mut written_artifacts = DiscoverArtifacts::default();
    if with_dashboard {
        let graph = hydrated.as_ref().expect("hydrated for dashboard");
        match rgctl_dashboard::export_dashboard_bundle_if_changed_with_context(
            graph.backend(),
            store,
            &snapshot_path,
            rgctl_dashboard::DashboardExportContext::with_analysis(&analysis_results),
        ) {
            Ok(true) => {
                written_artifacts.dashboard_dir =
                    Some(dashboard_dir.to_string_lossy().into_owned());
                if human_output {
                    info!("[✓] Dashboard: {}/index.html", dashboard_dir.display());
                }
            }
            Ok(false) => {
                if dashboard_dir.join("manifest.json").is_file() {
                    written_artifacts.dashboard_dir =
                        Some(dashboard_dir.to_string_lossy().into_owned());
                }
                if verbose {
                    debug!("Dashboard bundle unchanged — skipped re-export");
                }
            }
            Err(e) => {
                if human_output {
                    warn!("[!] Dashboard export failed: {e}");
                } else if verbose {
                    debug!(error = %e, "Dashboard bundle export failed");
                }
            }
        }
    } else if verbose {
        debug!("Dashboard export skipped (pass --with-dashboard to enable)");
    }
    profile.save_dashboard.secs = secs(save_dashboard_start.elapsed());

    if export_migration_hints {
        let migration_start = Instant::now();
        let plan_path = ctx
            .output
            .clone()
            .unwrap_or_else(|| rgctl_graph::paths::artifact_path(store, "migration_plan.json"));
        let graph = hydrated.as_ref().expect("hydrated for migration");
        match rgctl_dashboard::write_migration_plan_from_repo(
            graph.backend(),
            store,
            &plan_path,
            migration_preset,
            rgctl_analysis::MigrationOrderMode::parse(migration_order),
        ) {
            Ok(plan) => {
                written_artifacts.migration_plan =
                    Some(plan_path.to_string_lossy().into_owned());
                if json_output && ctx.output.is_none() && emit_cli_summary {
                    ctx.emit_json_value(&serde_json::to_value(&plan)?)?;
                    return Ok(discover_outcome(
                        &index_stats,
                        run_start.elapsed().as_millis() as u64,
                        graph_from_snapshot,
                        graph_digest.clone(),
                    ));
                }
                if human_output {
                    info!(
                        "[✓] Migration plan ({}): {} steps → {}",
                        plan.preset_label,
                        plan.steps.len(),
                        plan_path.display()
                    );
                }
            }
            Err(e) => {
                if human_output {
                    warn!("[!] Migration plan export skipped: {e}");
                } else if json_output && ctx.output.is_none() && emit_cli_summary {
                    ctx.emit_json_value(&serde_json::json!({
                        "error": e,
                        "migration_plan": null
                    }))?;
                    return Ok(discover_outcome(
                        &index_stats,
                        run_start.elapsed().as_millis() as u64,
                        graph_from_snapshot,
                        graph_digest.clone(),
                    ));
                }
            }
        }
        profile.migration_plan.secs = secs(migration_start.elapsed());
    }

    let analysis_size = std::fs::metadata(&analysis_path)?.len() as f64 / (1024.0 * 1024.0);
    mem_monitor.stop_periodic_sampling();
    profile.analysis_peak_rss_mb = mem_monitor.seal_phase().unwrap_or(0.0);
    if let Some(limit) = limits.as_ref().and_then(|l| l.max_mem_mb) {
        check_memory_budget(profile.analysis_peak_rss_mb.max(profile.ingest_peak_rss_mb), limit)?;
    }
    let snapshot = mem_monitor.snapshot()?;
    profile.wall_total.secs = secs(run_start.elapsed());
    profile.peak_rss_mb = snapshot.peak_mb;
    if verbose {
        profile.record();
    }

    if with_kantra {
        let findings = rgctl_graph::paths::artifact_path(store, "kantra_findings.json");
        if findings.is_file() {
            written_artifacts.kantra_findings =
                Some(findings.to_string_lossy().into_owned());
        }
    }

    if json_output && emit_cli_summary {
        let mut response =
            build_discover_response(&index_stats, run_start.elapsed().as_millis() as u64);
        if !written_artifacts.is_empty() {
            response.artifacts = Some(written_artifacts);
        }
        ctx.emit_json_value(&serde_json::to_value(&response)?)?;
    } else if !json_output && emit_cli_summary {
        info!("[✓] Saved to .rgctl/ ({:.1} MB total)", analysis_size);

        info!(
            "[✓] Completed in {:.1}s (peak {:.0} MB; ingest {:.0} MB, analysis {:.0} MB)",
            snapshot.elapsed.as_secs_f64(),
            snapshot.peak_mb,
            profile.ingest_peak_rss_mb,
            profile.analysis_peak_rss_mb
        );

        if verbose {
            debug!(
                saved_path = %analysis_path.display(),
                graph_snapshot = %snapshot_path.display(),
                size_mb = %format!("{:.1}", analysis_size),
                duration_secs = %format!("{:.1}", snapshot.elapsed.as_secs_f64()),
                peak_mb = %format!("{:.0}", snapshot.peak_mb),
                "Save complete"
            );
        }

        info!("");
        info!("[i] Next steps:");
        info!("   rgctl -f json find --type function --limit 20");
        info!("   rgctl -f json inventory --by type");
        info!("   rgctl -f json relations --edge calls --limit 20");
        info!("   rgctl slice <file> --line <N> --variable <VAR>");
        if dashboard_dir.join("manifest.json").is_file() {
            info!("   rgctl serve --open   # Dashboard + query API at http://127.0.0.1:8080");
        }
    }

    Ok(discover_outcome(
        &index_stats,
        run_start.elapsed().as_millis() as u64,
        graph_from_snapshot,
        graph_digest,
    ))
}

fn discover_outcome(
    stats: &PipelineStats,
    duration_ms: u64,
    reused_snapshot: bool,
    graph_digest: String,
) -> DiscoverRunOutcome {
    DiscoverRunOutcome {
        response: build_discover_response(stats, duration_ms),
        reused_snapshot,
        graph_digest,
    }
}

/// True when Stage 1 artifacts are present and match `graph_digest` (STAGE_DEEP reuse).
fn basic_analytics_reusable(store: &Path, graph_digest: &str, analysis_path: &Path) -> bool {
    if !analysis_path.is_file() {
        return false;
    }
    let blast_path = BlastEngineSnapshot::default_path(store);
    if !blast_path.is_file() {
        return false;
    }
    if !BlastEngineSnapshot::digest_matches(&blast_path, graph_digest).unwrap_or(false) {
        return false;
    }
    let macro_path = rgctl_graph::paths::artifact_path(store, "macro_call_index.bin");
    if !macro_path.is_file() {
        return false;
    }
    match FileTracker::load(store) {
        Ok(tracker) => !tracker.file_hashes().is_empty(),
        Err(_) => false,
    }
}
