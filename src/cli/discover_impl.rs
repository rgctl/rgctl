//! Discover implementation (index + analyze pipeline).

use anyhow::Result;
use rbuilder_graph::backend::GraphBackend;
use std::path::Path;
use tracing::{debug, error, info, info_span, warn};

pub(crate) fn run_full_analysis(
    path: &str,
    languages: Option<String>,
    exclude: Option<String>,
    verbose: bool,
    security: bool,
    cfg: bool,
    all: bool,
    db_path: &Path,
) -> Result<()> {
    let db_path = db_path;
    use crate::analysis::{CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer};
    use crate::analysis::graph_utils::PetGraphView;
    use crate::config::secret_detector::SecretDetector;
    use crate::discovery::{DiscoveryConfig, FileDiscoverer};
    use crate::incremental::FileTracker;
    use crate::languages::registry::LanguageRegistry;
    use crate::pipeline::{PipelineConfig, ProcessingPipeline};
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

    info!("==> Analyzing: {}", root.display());

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
            show_progress: true,
            ..PipelineConfig::default()
        },
    );

    // Discover files (used for indexing and later for security/tracking)
    let discoverer = FileDiscoverer::with_config(Arc::clone(&registry), discovery_config.clone());
    let files = discoverer.discover(root)?;

    // Index the repository
    let (graph, stats) = {
        let _span = if verbose { Some(info_span!("indexing").entered()) } else { None };
        pipeline.process_repository(root)?
    };

    if verbose {
        info!(
            files = stats.files_processed,
            nodes = stats.nodes_created,
            edges = stats.edges_created,
            duration_secs = %format!("{:.1}", stats.duration.as_secs_f64()),
            "[✓] Indexed {} files -> {} nodes, {} edges ({:.1}s)",
            stats.files_processed,
            stats.nodes_created,
            stats.edges_created,
            stats.duration.as_secs_f64()
        );
    } else {
        info!(
            "[✓] Indexed {} files -> {} nodes, {} edges ({:.1}s)",
            stats.files_processed,
            stats.nodes_created,
            stats.edges_created,
            stats.duration.as_secs_f64()
        );
    }

    if stats.files_failed > 0 {
        warn!(failed = stats.files_failed, "Skipped files due to errors");
    }

    debug!("{}", mem_monitor.report());

    // Initialize columnar analysis results
    use crate::analysis::AnalysisResults;
    // Zero-copy: collect node IDs directly without cloning full nodes
    let node_ids = graph.backend().all_node_ids()?;
    let mut analysis_results = AnalysisResults::new(node_ids);

    // Build PetGraphView ONCE - reused for community, centrality, and blast radius
    let petgraph_view = {
        let _span = if verbose { Some(info_span!("topology").entered()) } else { None };
        let view = PetGraphView::from_backend(graph.backend())?;
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
        let community_data: Vec<_> = community_result.assignments.iter()
            .filter_map(|(node_id, community_id)| {
                analysis_results.get_compact_id(*node_id).map(|compact_id| (compact_id, *community_id))
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

    debug!("{}", mem_monitor.report());

    // Complexity analysis - write to columnar table
    let complexity_report = ComplexityAnalyzer::analyze(graph.backend())?;
    {
        // Collect data with compact IDs first
        let complexity_data: Vec<_> = complexity_report.functions.iter()
            .filter_map(|func| {
                analysis_results.get_compact_id(func.node.id).map(|compact_id| {
                    (compact_id, func.cyclomatic as u32, func.cognitive as u32)
                })
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

    let high_complexity = complexity_report.by_level.get(&crate::analysis::ComplexityLevel::High).unwrap_or(&0);
    let medium_complexity = complexity_report.by_level.get(&crate::analysis::ComplexityLevel::Medium).unwrap_or(&0);

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

    debug!("{}", mem_monitor.report());

    // Centrality analysis - write to columnar table
    // PageRank is fast (< 1s even on 187K nodes)
    // Betweenness auto-skips internally for graphs > 500 nodes
    let centrality_report = CentralityAnalyzer::new().analyze_with_view(&petgraph_view)?;
    {
        // Collect data with compact IDs first
        let centrality_data: Vec<_> = centrality_report.scores.iter()
            .filter_map(|(node_id, scores)| {
                analysis_results.get_compact_id(*node_id).map(|compact_id| (compact_id, scores))
            })
            .collect();

        // Now update table
        let table = analysis_results.init_centrality();
        for (compact_id, scores) in centrality_data {
            let idx = compact_id as usize;
            table.pagerank[idx] = scores.pagerank as f32;
            table.betweenness[idx] = scores.betweenness as f32;
            table.in_degree[idx] = scores.in_degree as u32;
            table.out_degree[idx] = scores.out_degree as u32;
        }
    }

    // Check if we have betweenness data
    let has_betweenness = centrality_report.scores.values().any(|s| s.betweenness > 0.0);

    if let Some((top_id, top_score)) = centrality_report.top_pagerank.first() {
        if let Ok(Some(node)) = graph.backend().get_node(*top_id) {
            let short_name = node.name.split('/').last().unwrap_or(&node.name);

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
                    short_name,
                    top_score
                );
            }
        }
    }

    debug!("{}", mem_monitor.report());

    // Dependency analysis
    let cycles = DependencyAnalyzer::find_circular_dependencies(graph.backend())?;
    if cycles.len() > 0 {
        if verbose {
            warn!(
                count = cycles.len(),
                "[!] Found {} circular dependencies",
                cycles.len()
            );
        } else {
            warn!("[!] Found {} circular dependencies", cycles.len());
        }
    } else {
        debug!("No circular dependencies found");
    }

    // Security analysis (opt-in with --security or --all)
    if security || all {
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
        println!("\n✓ Control flow analysis:");
        use crate::analysis::{
            build_cfg_for_function, AnalysisStorage, DominatorTree, FunctionAnalysis,
            ProgramDependenceGraph,
        };

        let storage = AnalysisStorage::new(&output_dir);
        storage.ensure_dir()?;

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

        // Determine language from file extension
        let lang = if file_path.ends_with(".rs") {
            "rust"
        } else if file_path.ends_with(".py") {
            "python"
        } else if file_path.ends_with(".java") {
            "java"
        } else {
            // Skip unsupported languages for CFG
            error_count += 1;
            continue;
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
        let dom_data = cfg_data.as_ref().map(|cfg| DominatorTree::build(cfg));

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
        } else {
            error_count += 1;
        }
    }

    if success_count > 0 {
        println!("  CFG/PDG/Dominance: {} functions analyzed", success_count);
        if error_count > 0 {
            println!("  Skipped: {} functions (unsupported language or parse error)", error_count);
        }

        // Export consolidated file
        let export_path = output_dir.join("all_analyses.json");
        if storage.export_all(&export_path).is_ok()
            && verbose {
                println!("  Exported to {}", export_path.display());
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
            println!("  Taint flows: {} total ({} vulnerable)", total_flows, vulnerable_flows);
        }
        } else if !functions.is_empty() {
            println!("  No functions analyzed (Rust/Python only)");
        }
        if verbose {
            println!("{}", mem_monitor.report());
        }
    }

    // Blast radius analysis with SCC + Dense Bitsets engine
    use crate::analysis::BlastRadiusEngine;
    use std::time::Instant;

    let blast_start = Instant::now();

    // Build SCC engine (one-time cost: Tarjan's + topo sort + bitset propagation)
    let engine = match BlastRadiusEngine::build(backend) {
        Ok(e) => e,
        Err(err) => {
            error!(error = %err, "[x] Blast radius engine build failed");
            info!("[✓] Analysis complete");
            return Ok(());
        }
    };

    let build_time = blast_start.elapsed();
    let stats = engine.stats();

    debug!(
        scc_count = stats.scc_count,
        dag_edges = stats.dag_edges,
        build_time_secs = %format!("{:.2}", build_time.as_secs_f64()),
        compression_percent = %format!("{:.1}", (graph.node_count() - stats.scc_count) as f64 / graph.node_count() as f64 * 100.0),
        avg_scc_size = %format!("{:.1}", stats.avg_scc_size),
        memory_mb = %format!("{:.1}", stats.memory_mb),
        "Blast radius engine built"
    );

    // Analyze all functions (O(1) lookup per function!)
    let query_start = Instant::now();
    let mut blast_updates = Vec::new();
    let mut high_impact_count = 0;
    let mut max_impact_score = 0.0f64;
    let mut max_impact_function = String::new();
    let mut in_cycle_count = 0;

    for func_node in &functions {
        if let Ok(result) = engine.analyze(func_node.id) {
            if result.scc_size > 1 {
                in_cycle_count += 1;
            }

            if result.score > 50.0 {
                high_impact_count += 1;
            }

            if result.score > max_impact_score {
                max_impact_score = result.score;
                max_impact_function = func_node.name.clone();
            }

            blast_updates.push((func_node.id, result));
        }
    }

    let query_time = query_start.elapsed();

    // Write blast radius results to columnar table
    {
        // Collect data with compact IDs first
        let blast_data: Vec<_> = blast_updates.into_iter()
            .filter_map(|(node_id, result)| {
                analysis_results.get_compact_id(node_id).map(|compact_id| (compact_id, result))
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

    if !max_impact_function.is_empty() {
        let short_name = max_impact_function.split('/').last().unwrap_or(&max_impact_function);

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
                short_name,
                max_impact_score,
                high_impact_count
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

    info!("[✓] Analysis complete");

    // Save analysis results (columnar format - separate from graph!)
    let analysis_path = root.join(".rbuilder/analysis_results.bin");
    std::fs::create_dir_all(root.join(".rbuilder"))?;
    analysis_results.save(&analysis_path)?;

    // Save graph topology (no analysis properties!)
    let mut tracker = FileTracker::new(root);
    tracker.index_files(&files, &graph)?;
    tracker.save()?;

    let json = graph.export_json()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(db_path, json)?;
    let saved = graph.save_to_repo(root)?;

    // Export HTML dashboard
    use crate::export::export_html_dashboard;
    let html_path = root.join(".rbuilder/dashboard.html");
    let dashboard_exported = export_html_dashboard(
        graph.backend(),
        Some(&output_dir),
        &html_path,
    ).is_ok();

    let analysis_size = std::fs::metadata(&analysis_path)?.len() as f64 / (1024.0 * 1024.0);
    let snapshot = mem_monitor.snapshot();

    info!("[✓] Saved to .rbuilder/ ({:.1} MB total)", analysis_size);

    if dashboard_exported {
        info!("[✓] Dashboard: {}", html_path.display());
    }

    info!(
        "[✓] Completed in {:.1}s (peak memory: {:.0} MB)",
        snapshot.elapsed.as_secs_f64(),
        snapshot.peak_mb
    );

    if verbose {
        debug!(
            saved_path = %analysis_path.display(),
            graph_path = %saved.display(),
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
    info!("   rbuilder -f html-dashboard -o .rbuilder/dashboard.html");
    if dashboard_exported {
        info!("   open {}   # View dashboard", html_path.file_name().unwrap().to_str().unwrap());
    }

    Ok(())
}
