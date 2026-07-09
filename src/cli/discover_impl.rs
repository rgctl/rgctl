//! Discover implementation (index + analyze pipeline).

use super::args::OutputFormat;
use super::context::CliContext;
use super::discover_output::build_discover_response;
use anyhow::Result;
use rbuilder_graph::backend::GraphBackend;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, error, info, info_span, warn};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_full_analysis(
    ctx: &CliContext,
    path: &str,
    languages: Option<String>,
    exclude: Option<String>,
    security: bool,
    cfg: bool,
    all: bool,
    write_json_graph: bool,
    export_migration_plan: bool,
    migration_preset: &str,
    migration_order: &str,
    db_path: &Path,
) -> Result<()> {
    let verbose = ctx.verbose;
    let json_output = ctx.format == OutputFormat::Json;
    let human_output = !json_output;
    let run_start = Instant::now();
    use crate::analysis::graph_utils::PetGraphView;
    use crate::analysis::{
        CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer,
    };
    use crate::config::secret_detector::SecretDetector;
    use crate::discovery::{DiscoveryConfig, FileDiscoverer};
    use crate::incremental::FileTracker;
    use crate::languages::registry::LanguageRegistry;
    use crate::pipeline::{PipelineConfig, ProcessingPipeline};
    use rayon::prelude::*;
    use rbuilder_graph::PreparedGraphSnapshot;
    use std::path::Path;
    use std::sync::Arc;

    let root = Path::new(path);
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

    // Show warning for --all flag
    if all {
        warn!("[!] WARNING: --all flag enables all analyses including CFG/PDG.");
        warn!("   This may take several minutes on large codebases (>50K functions).");
        warn!("   For faster analysis, run without --all (default mode).");
    }

    // Initialize memory monitoring
    use rbuilder_core::memory::MemoryMonitor;
    let mem_monitor = MemoryMonitor::new();

    let discovery_config = discovery.clone();
    let registry = LanguageRegistry::new().into();
    let pipeline = ProcessingPipeline::with_config(
        Arc::clone(&registry),
        PipelineConfig {
            discovery,
            show_progress: human_output,
            ..PipelineConfig::default()
        },
    );

    // Discover files (used for indexing and later for security/tracking)
    let discoverer = FileDiscoverer::with_config(Arc::clone(&registry), discovery_config.clone());
    let files = discoverer.discover(root)?;

    // Index the repository
    let (graph, index_stats) = {
        let _span = if verbose {
            Some(info_span!("indexing").entered())
        } else {
            None
        };
        pipeline.process_repository(root)?
    };

    if human_output {
        if verbose {
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

    // One prepared snapshot for topology views, digest, and mmap write (Sprint B dedup).
    let prepared = PreparedGraphSnapshot::from_backend(graph.backend())?;
    let graph_digest = prepared.content_digest.clone();

    // Initialize columnar analysis results
    use crate::analysis::AnalysisResults;
    // Zero-copy: collect node IDs directly without cloning full nodes
    let node_ids = graph.backend().all_node_ids()?;
    let mut analysis_results = AnalysisResults::new(node_ids);

    // Build PetGraphView ONCE from prepared snapshot — reused for community, centrality, blast radius
    let petgraph_view = {
        let _span = if verbose {
            Some(info_span!("topology").entered())
        } else {
            None
        };
        let view = PetGraphView::from_prepared(&prepared)?;
        debug!(
            nodes = view.directed.node_count(),
            edges = view.directed.edge_count(),
            "Topology view built"
        );
        view
    };

    // Community detection - write to columnar table
    let community_result = CommunityDetector::new().detect_with_view(&petgraph_view)?;
    {
        // Collect data with compact IDs first
        let community_data: Vec<_> = community_result
            .assignments
            .iter()
            .filter_map(|(node_id, community_id)| {
                analysis_results
                    .get_compact_id(*node_id)
                    .map(|compact_id| (compact_id, *community_id))
            })
            .collect();

        // Now update table
        let table = analysis_results.init_community();
        table.modularity = community_result.modularity;
        table.num_communities = community_result.communities.len();
        for (compact_id, community_id) in community_data {
            table.assignments[compact_id as usize] = community_id;
        }
    }

    if human_output {
        if verbose {
            info!(
                communities = community_result.communities.len(),
                modularity = %format!("{:.2}", community_result.modularity),
                "[✓] Detected {} communities (modularity: {:.2})",
                community_result.communities.len(),
                community_result.modularity
            );
        } else {
            info!(
                "[✓] Detected {} communities (modularity: {:.2})",
                community_result.communities.len(),
                community_result.modularity
            );
        }
    }

    debug!("{}", mem_monitor.report());

    // Complexity analysis - write to columnar table
    let complexity_report = ComplexityAnalyzer::analyze(graph.backend())?;
    {
        // Collect data with compact IDs first
        let complexity_data: Vec<_> = complexity_report
            .functions
            .iter()
            .filter_map(|func| {
                analysis_results
                    .get_compact_id(func.node.id)
                    .map(|compact_id| (compact_id, func.cyclomatic as u32, func.cognitive as u32))
            })
            .collect();

        // Now update table
        let table = analysis_results.init_complexity();
        table.avg_cyclomatic = complexity_report.avg_cyclomatic;
        table.max_cyclomatic = complexity_report.max_cyclomatic as u32;
        for (compact_id, cyclomatic, cognitive) in complexity_data {
            table.cyclomatic[compact_id as usize] = cyclomatic;
            table.cognitive[compact_id as usize] = cognitive;
        }
    }

    if verbose {
        debug!("✓ Complexity analysis:");
        debug!("  Functions: {}", complexity_report.functions.len());
        debug!("  Avg cyclomatic: {:.1}", complexity_report.avg_cyclomatic);
        debug!("  Max cyclomatic: {}", complexity_report.max_cyclomatic);
        for (level, count) in &complexity_report.by_level {
            debug!("    {:?}: {}", level, count);
        }
        debug!("{}", mem_monitor.report());
    }

    let high_complexity = complexity_report
        .by_level
        .get(&crate::analysis::ComplexityLevel::High)
        .unwrap_or(&0);
    let medium_complexity = complexity_report
        .by_level
        .get(&crate::analysis::ComplexityLevel::Medium)
        .unwrap_or(&0);

    if human_output {
        if verbose {
            info!(
                functions = complexity_report.functions.len(),
                avg_cyclomatic = %format!("{:.1}", complexity_report.avg_cyclomatic),
                high = high_complexity,
                medium = medium_complexity,
                "[✓] Analyzed {} functions (avg complexity: {:.1}, {} high, {} medium)",
                complexity_report.functions.len(),
                complexity_report.avg_cyclomatic,
                high_complexity,
                medium_complexity
            );
        } else {
            info!(
                "[✓] Analyzed {} functions (avg complexity: {:.1}, {} high, {} medium)",
                complexity_report.functions.len(),
                complexity_report.avg_cyclomatic,
                high_complexity,
                medium_complexity
            );
        }
    }

    debug!("{}", mem_monitor.report());

    // Centrality analysis — exact below 500 nodes; sampled betweenness + HyperBall harmonic above.
    let centrality_report = CentralityAnalyzer::new().analyze_with_view(&petgraph_view)?;
    {
        // Collect data with compact IDs first
        let centrality_data: Vec<_> = centrality_report
            .scores
            .iter()
            .filter_map(|(node_id, scores)| {
                analysis_results
                    .get_compact_id(*node_id)
                    .map(|compact_id| (compact_id, scores))
            })
            .collect();

        // Now update table
        let table = analysis_results.init_centrality();
        for (compact_id, scores) in centrality_data {
            let idx = compact_id as usize;
            table.pagerank[idx] = scores.pagerank as f32;
            table.betweenness[idx] = scores.betweenness as f32;
            table.harmonic[idx] = scores.harmonic as f32;
            table.in_degree[idx] = scores.in_degree as u32;
            table.out_degree[idx] = scores.out_degree as u32;
        }
    }

    // Check if we have betweenness data
    let has_betweenness = centrality_report
        .scores
        .values()
        .any(|s| s.betweenness > 0.0);

    if human_output {
        if let Some((top_id, top_score)) = centrality_report.top_pagerank.first() {
            if let Ok(Some(node)) = graph.backend().get_node(*top_id) {
                let short_name = node.name.split('/').next_back().unwrap_or(&node.name);

                if verbose {
                    info!(
                        hotspot = short_name,
                        pagerank = %format!("{:.4}", top_score),
                        betweenness_enabled = has_betweenness,
                        in_degree = centrality_report.scores.get(top_id).map(|s| s.in_degree).unwrap_or(0),
                        out_degree = centrality_report.scores.get(top_id).map(|s| s.out_degree).unwrap_or(0),
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

    // Dependency analysis
    let cycles = DependencyAnalyzer::find_circular_dependencies(graph.backend())?;
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

    // Security analysis (opt-in with --security or --all)
    if (security || all) && human_output {
        println!("\n✓ Security analysis:");
        let detector = SecretDetector::new();
        let mut total_secrets = 0usize;

        for file in files.iter().take(100) {
            if let Ok(content) = std::fs::read_to_string(file) {
                let found = detector.scan(&content);
                total_secrets += found.len();

                if verbose {
                    for secret in &found {
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
        }
        println!("  Potential secrets found: {total_secrets}");
    }

    // Get backend and functions for later use (blast radius, etc.)
    use rbuilder_graph::schema::NodeType;
    let backend = graph.backend();
    let functions = backend.collect_nodes_by_type(NodeType::Function)?;
    let output_dir = root.join(".rbuilder/analysis");

    // CFG/PDG/Dominance analysis (opt-in with --cfg or --all)
    if cfg || all {
        if human_output {
            println!("\n✓ Control flow analysis:");
        }
        use crate::analysis::{
            build_cfg_for_function, cfg_language_id_from_path, cfg_language_list,
            AnalysisStorage, CfgPdgArchive, CfgPdgRecord, DominatorTree, FunctionAnalysis,
            ProgramDependenceGraph,
        };
        use rbuilder_graph::code_index::hash_code;

        let storage = AnalysisStorage::new(&output_dir);
        storage.ensure_dir()?;
        let mut cfg_archive = CfgPdgArchive {
            graph_digest: Some(graph_digest.clone()),
            ..CfgPdgArchive::default()
        };

        let mut success_count = 0;
        let mut error_count = 0;

        for func_node in &functions {
            // Get function source
            let file_path = match &func_node.file_path {
                Some(p) => p,
                None => {
                    error_count += 1;
                    continue;
                }
            };

            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(_) => {
                    error_count += 1;
                    continue;
                }
            };

            let lang = match cfg_language_id_from_path(std::path::Path::new(file_path)) {
                Some(lang) => lang,
                None => {
                    error_count += 1;
                    continue;
                }
            };

            // Build CFG
            let cfg_result = build_cfg_for_function(lang, &source, &func_node.name);

            let cfg_data = match cfg_result {
                Ok(c) => Some(c),
                Err(_) => {
                    error_count += 1;
                    continue;
                }
            };

            // Build PDG
            let pdg_data = if let Some(ref cfg) = cfg_data {
                ProgramDependenceGraph::build(cfg, source.as_bytes()).ok()
            } else {
                None
            };

            // Build Dominance
            let dom_data = cfg_data.as_ref().map(DominatorTree::build);

            // Run Taint Analysis
            use crate::analysis::TaintAnalyzer;
            let taint_data = if let (Some(ref cfg), Some(ref pdg)) = (&cfg_data, &pdg_data) {
                let mut analyzer = TaintAnalyzer::new(pdg, cfg);
                analyzer.detect_patterns(lang);
                let flows = analyzer.analyze();
                if flows.is_empty() {
                    None
                } else {
                    Some(flows)
                }
            } else {
                None
            };

            // Store analysis
            let analysis = FunctionAnalysis {
                function_id: func_node.id,
                function_name: func_node.name.clone(),
                file_path: file_path.clone(),
                cfg: cfg_data,
                pdg: pdg_data,
                dominance: dom_data,
                taint: taint_data,
            };

            if storage.save_function(&analysis).is_ok() {
                success_count += 1;
                if let (Some(cfg), Some(pdg)) = (&analysis.cfg, &analysis.pdg) {
                    cfg_archive.insert(CfgPdgRecord {
                        function_id: func_node.id,
                        code_hash: hash_code(&source),
                        function_name: func_node.name.clone(),
                        file_path: Some(file_path.clone()),
                        cfg: cfg.clone(),
                        pdg: pdg.clone(),
                    });
                }
            } else {
                error_count += 1;
            }
        }

        if !cfg_archive.records.is_empty() {
            let archive_path = CfgPdgArchive::default_path(root);
            if let Err(err) = cfg_archive.write_to_path(&archive_path) {
                warn!(error = %err, "Failed to save cfg_pdg archive");
            } else if verbose {
                debug!(
                    path = %archive_path.display(),
                    entries = cfg_archive.records.len(),
                    "CFG/PDG archive saved"
                );
            }
        }

        if human_output {
            if success_count > 0 {
                println!("  CFG/PDG/Dominance: {} functions analyzed", success_count);
                if error_count > 0 {
                    println!(
                        "  Skipped: {} functions (unsupported language or parse error)",
                        error_count
                    );
                }

                // Taint analysis summary
                let all_analyses = storage.load_all().unwrap_or_default();
                let mut total_flows = 0;
                let mut vulnerable_flows = 0;
                for analysis in &all_analyses {
                    if let Some(ref flows) = analysis.taint {
                        total_flows += flows.len();
                        vulnerable_flows += flows.iter().filter(|f| f.is_vulnerable()).count();
                    }
                }
                if total_flows > 0 {
                    println!(
                        "  Taint flows: {} total ({} vulnerable)",
                        total_flows, vulnerable_flows
                    );
                }
            } else if !functions.is_empty() {
                println!(
                    "  No functions analyzed (CFG supported: {})",
                    cfg_language_list()
                );
            }
            if verbose {
                println!("{}", mem_monitor.report());
            }
        }
    }

    // Blast radius analysis with SCC + Dense Bitsets engine
    use crate::analysis::BlastRadiusEngine;

    let blast_start = Instant::now();

    // Build SCC engine (one-time cost: Tarjan's + topo sort + bitset propagation)
    let engine = match BlastRadiusEngine::build_from_view(backend, &petgraph_view) {
        Ok(e) => e,
        Err(err) => {
            error!(error = %err, "[x] Blast radius engine build failed");
            info!("[✓] Analysis complete");
            return Ok(());
        }
    };

    let build_time = blast_start.elapsed();
    let engine_stats = engine.stats();

    debug!(
        scc_count = engine_stats.scc_count,
        dag_edges = engine_stats.dag_edges,
        build_time_secs = %format!("{:.2}", build_time.as_secs_f64()),
        compression_percent = %format!("{:.1}", (graph.node_count() - engine_stats.scc_count) as f64 / graph.node_count() as f64 * 100.0),
        avg_scc_size = %format!("{:.1}", engine_stats.avg_scc_size),
        memory_mb = %format!("{:.1}", engine_stats.memory_mb),
        "Blast radius engine built"
    );

    // Analyze all functions in parallel (O(1) lookup per function, read-only engine)
    let query_start = Instant::now();
    let blast_results: Vec<(uuid::Uuid, crate::analysis::BlastRadiusResult)> = functions
        .par_iter()
        .filter_map(|func_node| {
            engine
                .analyze(func_node.id)
                .ok()
                .map(|result| (func_node.id, result))
        })
        .collect();

    let mut high_impact_count = 0;
    let mut max_impact_score = 0.0f64;
    let mut max_impact_function = String::new();
    let mut in_cycle_count = 0;

    for (func_id, result) in &blast_results {
        if result.scc_size > 1 {
            in_cycle_count += 1;
        }
        if result.score > 50.0 {
            high_impact_count += 1;
        }
        if result.score > max_impact_score {
            max_impact_score = result.score;
            if let Ok(Some(node)) = backend.get_node(*func_id) {
                max_impact_function = node.name.clone();
            }
        }
    }

    let query_time = query_start.elapsed();
    let blast_updates = blast_results;

    // Persist SCC engine snapshot for instant blast-radius cache misses
    {
        use crate::analysis::BlastEngineSnapshot;
        let blast_snap = engine.to_engine_snapshot(graph_digest.clone());
        let blast_path = BlastEngineSnapshot::default_path(root);
        if let Err(err) = blast_snap.write_to_path(&blast_path) {
            warn!(error = %err, "Failed to save blast engine snapshot");
        } else if verbose {
            debug!(path = %blast_path.display(), "Blast engine snapshot saved");
        }
    }

    // Serialize minimized macro-call index for instant blast-radius lookups
    {
        use crate::analysis::MacroCallIndex;
        let macro_index = MacroCallIndex::from_results(
            db_path,
            backend,
            &blast_updates,
            Some(graph_digest.clone()),
        )?;
        let macro_path = root.join(".rbuilder/macro_call_index.bin");
        if let Err(err) = macro_index.save(&macro_path) {
            warn!(error = %err, "Failed to save macro_call_index cache");
        } else if verbose {
            debug!(
                path = %macro_path.display(),
                entries = macro_index.entries.len(),
                "Macro call index saved"
            );
        }

        use crate::analysis::MacroCallLookupDb;
        let lookup_db_path = MacroCallLookupDb::default_path(root);
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
            backend.node_count(),
            backend.edge_count(),
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

    // Write blast radius results to columnar table
    {
        // Collect data with compact IDs first
        let blast_data: Vec<_> = blast_updates
            .into_iter()
            .filter_map(|(node_id, result)| {
                analysis_results
                    .get_compact_id(node_id)
                    .map(|compact_id| (compact_id, result))
            })
            .collect();

        // Now update table
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

    // Save analysis results (columnar format - separate from graph!)
    let analysis_path = root.join(".rbuilder/analysis_results.bin");
    std::fs::create_dir_all(root.join(".rbuilder"))?;
    analysis_results.save(&analysis_path)?;

    // Save graph topology (no analysis properties!)
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph)?;
    tracker.save()?;

    std::fs::create_dir_all(root.join(".rbuilder"))?;
    let snapshot_path = rbuilder_graph::snapshot::MmappedGraphSnapshot::default_path(root);
    prepared.write_to_path(&snapshot_path)?;
    if verbose {
        debug!(path = %snapshot_path.display(), "Graph binary snapshot saved");
    }

    if write_json_graph {
        let json = graph.export_json()?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(db_path, &json)?;
        let saved = graph.save_to_repo(root)?;
        if verbose {
            debug!(path = %saved.display(), "Legacy JSON graph saved");
        }
    }

    // Export static dashboard bundle (Phase 0+1 — see docs/dashboard-design.md)
    let dashboard_dir = root.join(".rbuilder/dashboard");
    match rbuilder_dashboard::export_dashboard_bundle(graph.backend(), root, &snapshot_path) {
        Ok(()) => {
            if human_output {
                info!("[✓] Dashboard: {}/index.html", dashboard_dir.display());
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

    if export_migration_plan {
        let plan_path = ctx
            .output
            .clone()
            .unwrap_or_else(|| root.join(".rbuilder/migration_plan.json"));
        match rbuilder_dashboard::write_migration_plan_from_repo(
            graph.backend(),
            root,
            &plan_path,
            migration_preset,
            rbuilder_analysis::MigrationOrderMode::parse(migration_order),
        ) {
            Ok(plan) => {
                if json_output && ctx.output.is_none() {
                    ctx.emit_json_value(&serde_json::to_value(&plan)?)?;
                    return Ok(());
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
                } else if json_output && ctx.output.is_none() {
                    ctx.emit_json_value(&serde_json::json!({
                        "error": e,
                        "migration_plan": null
                    }))?;
                    return Ok(());
                }
            }
        }
    }

    let analysis_size = std::fs::metadata(&analysis_path)?.len() as f64 / (1024.0 * 1024.0);
    let snapshot = mem_monitor.snapshot();

    if json_output {
        let response =
            build_discover_response(&index_stats, run_start.elapsed().as_millis() as u64);
        ctx.emit_json_value(&serde_json::to_value(&response)?)?;
    } else {
        info!("[✓] Saved to .rbuilder/ ({:.1} MB total)", analysis_size);

        info!(
            "[✓] Completed in {:.1}s (peak memory: {:.0} MB)",
            snapshot.elapsed.as_secs_f64(),
            snapshot.peak_mb
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
        info!("   rbuilder gql \"MATCH (n:Function) RETURN n\"  # Query the graph");
        info!("   rbuilder slice <file> --line <N> --variable <VAR>");
        if dashboard_dir.join("manifest.json").is_file() {
            info!(
                "   rbuilder serve --open   # Dashboard + query API at http://127.0.0.1:8080"
            );
        }
    }

    Ok(())
}
