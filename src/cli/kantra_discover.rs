//! Kantra discover stage integration.

use crate::analysis::graph_utils::PetGraphView;
use crate::analysis::{AnalysisResults, BlastRadiusEngine, ColdMetadataDb, NodeLookup};
use crate::cli::stage_profile::DiscoverStageReport;
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rgctl_graph::schema::{EdgeType, Node, NodeType};
use rgctl_graph::snapshot::SNAPSHOT_FILE;
use rgctl_kantra::eval::filecontent::SourceCache;
use rgctl_kantra::loader::KantraRuleset;
use rgctl_kantra::{
    EvalCallSite, EvalContext, EvalEdge, EvalGraph, EvalNode, KantraCatalog, KantraEngine,
    KantraFileCache, KantraFindings, NodeMetrics, ViolationResolver, cache_dir, enrich_findings,
    rewrite_snapshot_with_catalog, rewrite_snapshot_with_violations, ruleset_hash,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

const TEXT_EXTENSIONS: &[&str] = &[
    ".go",
    ".java",
    ".kt",
    ".scala",
    ".py",
    ".rs",
    ".js",
    ".ts",
    ".tsx",
    ".jsx",
    ".c",
    ".h",
    ".cpp",
    ".hpp",
    ".cs",
    ".md",
    ".mdx",
    ".yaml",
    ".yml",
    ".xml",
    ".json",
    ".properties",
    ".gradle",
    ".kts",
    ".sql",
];

/// Resolve the rules catalog for index/eval (embedded, tree, or single ruleset dir).
pub fn resolve_kantra_catalog(
    rules_path: Option<&Path>,
    catalog_path: Option<&Path>,
) -> Result<KantraCatalog> {
    if let Some(path) = rules_path {
        let rs = KantraRuleset::load(path).map_err(|e| anyhow::anyhow!("kantra ruleset: {e}"))?;
        return Ok(KantraCatalog {
            catalog_id: format!("dir@{}", path.display()),
            name: rs.doc.name,
            description: rs.doc.description,
            rules: rs.doc.rules,
        });
    }
    if let Some(root) = catalog_path {
        return KantraCatalog::load_tree(root).map_err(|e| anyhow::anyhow!("kantra catalog: {e}"));
    }
    KantraCatalog::embedded().map_err(|e| anyhow::anyhow!("kantra embedded catalog: {e}"))
}

/// Hydrate `KantraRule` nodes into `graph.snapshot.bin`.
pub fn run_kantra_index(
    store: &Path,
    rules_path: Option<&Path>,
    catalog_path: Option<&Path>,
    profile: &mut DiscoverStageReport,
) -> Result<()> {
    let start = Instant::now();
    let catalog = resolve_kantra_catalog(rules_path, catalog_path)?;
    let snapshot_path = rgctl_graph::paths::artifact_path(store, SNAPSHOT_FILE);
    rewrite_snapshot_with_catalog(&snapshot_path, &catalog)
        .map_err(|e| anyhow::anyhow!("kantra index: {e}"))?;
    profile.kantra_index.secs = start.elapsed().as_secs_f64();
    Ok(())
}

/// Materialize `VIOLATES` edges after rule nodes are indexed.
pub fn run_kantra_violates(store: &Path, profile: &mut DiscoverStageReport) -> Result<usize> {
    let start = Instant::now();
    let findings_path = rgctl_graph::paths::artifact_path(store, "kantra_findings.json");
    let bytes =
        fs::read(&findings_path).with_context(|| format!("read {}", findings_path.display()))?;
    let findings: KantraFindings =
        serde_json::from_slice(&bytes).context("parse kantra findings for VIOLATES")?;
    let snapshot_path = rgctl_graph::paths::artifact_path(store, SNAPSHOT_FILE);
    let count = rewrite_snapshot_with_violations(&snapshot_path, &findings)
        .map_err(|e| anyhow::anyhow!("kantra violates: {e}"))?;
    profile.kantra_violates.secs = start.elapsed().as_secs_f64();
    Ok(count)
}

/// Enrich findings with community / centrality / blast-radius metrics.
pub fn run_kantra_enrich(
    store: &Path,
    cold: &ColdMetadataDb,
    analysis: &AnalysisResults,
    blast: &BlastRadiusEngine,
    eval_graph: &EvalGraph,
    profile: &mut DiscoverStageReport,
) -> Result<KantraFindings> {
    let start = Instant::now();
    let findings_path = rgctl_graph::paths::artifact_path(store, "kantra_findings.json");
    let bytes =
        fs::read(&findings_path).with_context(|| format!("read {}", findings_path.display()))?;
    let mut findings: KantraFindings =
        serde_json::from_slice(&bytes).context("parse kantra findings for enrich")?;

    let resolver = ViolationResolver::from_eval_nodes(&eval_graph.nodes);
    resolver.attach_node_ids(&mut findings.violations);

    let metrics = build_node_metrics(cold, analysis, blast);
    enrich_findings(&mut findings, &metrics);

    let json = serde_json::to_string_pretty(&findings).context("serialize kantra findings")?;
    fs::write(&findings_path, json)
        .with_context(|| format!("write {}", findings_path.display()))?;
    profile.kantra_enrich.secs = start.elapsed().as_secs_f64();
    Ok(findings)
}

fn build_node_metrics(
    cold: &ColdMetadataDb,
    analysis: &AnalysisResults,
    blast: &BlastRadiusEngine,
) -> HashMap<Uuid, NodeMetrics> {
    let mut out = HashMap::new();
    let _ = cold.for_each_node(&mut |node| {
        let mut metrics = NodeMetrics::default();
        if let Some(c) = analysis.get_community(node.id) {
            metrics.community_id = Some(c);
        }
        if let Some(cent) = analysis.get_centrality(node.id) {
            metrics.pagerank = Some(cent.pagerank);
        }
        if let Ok(result) = blast.analyze(node.id) {
            metrics.blast_radius_score = Some(result.score);
            metrics.impact_zone_size = Some(result.impact_zone_ids.len());
        }
        if metrics.community_id.is_some()
            || metrics.pagerank.is_some()
            || metrics.blast_radius_score.is_some()
        {
            out.insert(node.id, metrics);
        }
    });
    out
}

/// Run Kantra evaluation and write `.rgctl/kantra_findings.json`.
#[allow(clippy::too_many_arguments)]
pub fn run_kantra_stage(
    repo_root: &Path,
    store: &Path,
    rules_path: Option<&Path>,
    catalog_path: Option<&Path>,
    kantra_target: Option<&str>,
    files: &[PathBuf],
    cold: &ColdMetadataDb,
    petgraph_view: &PetGraphView,
    profile: &mut DiscoverStageReport,
) -> Result<(KantraFindings, EvalGraph)> {
    let kantra_start = Instant::now();
    let catalog = resolve_kantra_catalog(rules_path, catalog_path)?;
    let (engine, load_secs) = KantraEngine::from_catalog(catalog.clone(), kantra_target)
        .map_err(|e| anyhow::anyhow!("kantra catalog: {e}"))?;
    profile.kantra_load.secs = load_secs;

    let preload_start = Instant::now();
    let kantra_files: std::borrow::Cow<'_, [PathBuf]> = if engine.needs_xml_or_json_files() {
        std::borrow::Cow::Owned(collect_kantra_files(repo_root, files))
    } else {
        std::borrow::Cow::Borrowed(files)
    };
    let sources = preload_discovered_sources(repo_root, &kantra_files, None);
    profile.kantra_filecontent.secs = preload_start.elapsed().as_secs_f64();

    let graph = build_eval_graph(store, cold, petgraph_view, engine.needs_call_sites())?;
    let rs_hash = ruleset_hash(
        engine.catalog_id().unwrap_or("unknown"),
        catalog.rules.len(),
    );
    let mut file_cache = KantraFileCache::load(&cache_dir(store), &rs_hash);

    let mut ctx = EvalContext {
        repo_root,
        files: &kantra_files,
        sources: &sources,
        graph: &graph,
        cache: Some(&mut file_cache),
        cached_files: HashSet::new(),
    };

    let ref_start = Instant::now();
    let (mut findings, timings) = engine
        .evaluate(&mut ctx)
        .map_err(|e| anyhow::anyhow!("kantra evaluate: {e}"))?;
    profile.kantra_referenced.secs = ref_start.elapsed().as_secs_f64();
    profile.kantra_compose.secs = timings.compose_secs;
    profile.kantra_filecontent.secs += timings.filecontent_secs;
    profile.kantra_eval.secs = kantra_start.elapsed().as_secs_f64();

    let resolver = ViolationResolver::from_eval_nodes(&graph.nodes);
    resolver.attach_node_ids(&mut findings.violations);

    file_cache.save(&cache_dir(store))?;

    let out_path = rgctl_graph::paths::artifact_path(store, "kantra_findings.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&findings).context("serialize kantra findings")?;
    fs::write(&out_path, json).with_context(|| format!("write {}", out_path.display()))?;
    Ok((findings, graph))
}

/// Read discovered source files into a cache (parallel).
pub fn preload_discovered_sources(
    repo_root: &Path,
    files: &[PathBuf],
    thread_count: Option<usize>,
) -> SourceCache {
    let paths: HashSet<String> = files
        .iter()
        .filter(|p| is_text_file(p))
        .map(|p| {
            p.strip_prefix(repo_root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    rgctl_pipeline::with_pool(thread_count, || {
        paths
            .par_iter()
            .filter_map(|rel| {
                let read_path = if Path::new(rel).is_file() {
                    PathBuf::from(rel)
                } else {
                    repo_root.join(rel)
                };
                let content = fs::read_to_string(&read_path).ok()?;
                Some((rel.clone(), Arc::new(content)))
            })
            .collect()
    })
}

fn collect_kantra_files(repo_root: &Path, discovered: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<PathBuf> = discovered.iter().cloned().collect();
    let mut out: Vec<PathBuf> = discovered.to_vec();
    for entry in ignore::WalkBuilder::new(repo_root)
        .git_ignore(true)
        .hidden(false)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                ".git" | ".rgctl" | ".rbuilder" | "target" | "node_modules" | "dist"
            )
        })
        .build()
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.to_string_lossy();
        if (name.ends_with(".xml")
            || name.ends_with(".json")
            || name.ends_with(".properties")
            || name.ends_with(".yaml")
            || name.ends_with(".yml"))
            && seen.insert(path.to_path_buf())
        {
            out.push(path.to_path_buf());
        }
    }
    out.sort();
    out
}

fn is_text_file(path: &Path) -> bool {
    let name = path.to_string_lossy();
    TEXT_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

fn build_eval_graph(
    rgctl_dir: &Path,
    cold: &ColdMetadataDb,
    view: &PetGraphView,
    needs_call_sites: bool,
) -> Result<EvalGraph> {
    let all_ids = cold.store().all_node_ids();
    let mut graph = EvalGraph {
        nodes: Vec::with_capacity(all_ids.len()),
        edges: Vec::new(),
        call_sites: Vec::new(),
    };
    let mut node_idx: HashMap<Uuid, usize> = HashMap::with_capacity(all_ids.len());
    for id in all_ids {
        let Some(node) = cold.get_node(id)? else {
            continue;
        };
        if !matches!(
            node.node_type,
            NodeType::Import
                | NodeType::Class
                | NodeType::Interface
                | NodeType::Enum
                | NodeType::Annotation
                | NodeType::Function
                | NodeType::Module
                | NodeType::File
                | NodeType::Variable
                | NodeType::Dependency
        ) {
            continue;
        }
        let idx = graph.nodes.len();
        graph.nodes.push(node_to_eval(&node));
        node_idx.insert(id, idx);
    }

    let index_to_uuid = &view.index_to_uuid;
    view.topo.for_each_edge(|src, dst, edge_type| {
        let edge_name = match edge_type {
            EdgeType::Extends => "EXTENDS",
            EdgeType::Implements => "IMPLEMENTS",
            EdgeType::AnnotatedWith => "ANNOTATED_WITH",
            EdgeType::ExposesRoute => "EXPOSES_ROUTE",
            EdgeType::Injects => "INJECTS",
            EdgeType::Calls if needs_call_sites => "CALLS",
            EdgeType::Instantiates if needs_call_sites => "INSTANTIATES",
            _ => return,
        };
        let Some(&from_id) = index_to_uuid.get(src as usize) else {
            return;
        };
        let Some(&to_id) = index_to_uuid.get(dst as usize) else {
            return;
        };
        let (from_name, from_qualified, file_path, line) = if let Some(&i) = node_idx.get(&from_id)
        {
            let n = &graph.nodes[i];
            (
                n.name.clone(),
                n.qualified_name.clone().unwrap_or_else(|| n.name.clone()),
                n.file_path.clone().unwrap_or_default(),
                n.start_line.unwrap_or(1),
            )
        } else if let Ok(Some(from_node)) = cold.get_node(from_id) {
            (
                from_node.name.to_string(),
                from_node
                    .qualified_name
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| from_node.name.to_string()),
                from_node
                    .file_path
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                from_node.start_line.unwrap_or(1),
            )
        } else {
            return;
        };
        let (to_name, to_qualified) = if let Some(&i) = node_idx.get(&to_id) {
            let n = &graph.nodes[i];
            (
                n.name.clone(),
                n.qualified_name.clone().unwrap_or_else(|| n.name.clone()),
            )
        } else if let Ok(Some(to_node)) = cold.get_node(to_id) {
            (
                to_node.name.to_string(),
                to_node
                    .qualified_name
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| to_node.name.to_string()),
            )
        } else {
            return;
        };
        graph.edges.push(EvalEdge {
            edge_type: edge_name.to_string(),
            from_name,
            from_qualified,
            to_name,
            to_qualified,
            file_path,
            line,
        });
    })?;

    if needs_call_sites {
        let artifact_dir = rgctl_graph::paths::artifact_dir(rgctl_dir);
        if let Ok(parked) = rgctl_extraction::read_parked_relations(&artifact_dir) {
            for rec in parked {
                let relation_type = match rec.relation_type {
                    rgctl_plugin_api::RelationType::Calls => "Calls",
                    rgctl_plugin_api::RelationType::Instantiates => "Instantiates",
                    _ => continue,
                };
                graph.call_sites.push(EvalCallSite {
                    caller_id: rec.from_node_id,
                    file_path: rec.from_file,
                    line: rec.call_site_line as usize,
                    relation_type: relation_type.into(),
                    to_name: rec.to_name,
                    to_qualified_hint: rec.to_qualified_hint,
                    to_type_hint: rec.to_type_hint,
                    import_module_hint: rec.import_module_hint,
                });
            }
        }
    }

    Ok(graph)
}

fn node_to_eval(node: &Node) -> EvalNode {
    let props = &node.properties;
    let mut param_type_fqns = Vec::new();
    let mut field_type_fqns = Vec::new();
    let mut annotations = Vec::new();
    for (k, v) in props {
        if let Some(rest) = k.strip_prefix("param_type_fqn:") {
            let _ = rest;
            param_type_fqns.push(v.clone());
        } else if k == "field_type_fqns" || k.starts_with("field_type_fqn") || k == "field_type" {
            field_type_fqns.push(v.clone());
        } else if k == "annotations" || k.starts_with("annotation:") {
            annotations.push(v.clone());
        }
    }
    if node.node_type == NodeType::Variable {
        if let Some(rt_fqn) = props.get("return_type_fqn") {
            field_type_fqns.push(rt_fqn.clone());
        }
        if let Some(rt) = node.return_type.as_ref() {
            field_type_fqns.push(rt.to_string());
        }
    }
    let mut throws: Vec<String> = Vec::new();
    for key in ["throws_fqns", "throws"] {
        if let Some(s) = props.get(key) {
            for item in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                if !throws.iter().any(|existing| existing == item) {
                    throws.push(item.to_string());
                }
            }
        }
    }
    EvalNode {
        id: Some(node.id),
        node_type: format!("{:?}", node.node_type),
        name: node.name.to_string(),
        qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
        file_path: node.file_path.as_ref().map(|s| s.to_string()),
        start_line: node.start_line,
        labels: node.labels.clone(),
        return_type_fqn: props.get("return_type_fqn").cloned(),
        field_type_fqns,
        param_type_fqns,
        throws,
        annotations,
        version: props.get("version").cloned(),
    }
}

/// Validate Kantra CLI flags.
pub fn validate_kantra_flags(
    with_kantra: bool,
    kantra_rules: &Option<String>,
    kantra_catalog: &Option<String>,
) -> Result<()> {
    if !with_kantra {
        return Ok(());
    }
    if kantra_rules.as_ref().is_some_and(|s| !s.is_empty())
        && kantra_catalog.as_ref().is_some_and(|s| !s.is_empty())
    {
        bail!("use only one of --kantra-rules or --kantra-catalog");
    }
    Ok(())
}
