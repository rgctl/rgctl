//! Blast-radius (impact) command.

use crate::blast_json::{
    NodeLookup, build_from_engine_result, response_to_json, skipped_gatekeeping,
};
use crate::command::ImpactArgs;
use crate::error::{Result, ServiceError};
use rgctl_analysis::{
    BlastRadiusEngine, MacroCallIndex, MacroCallLookupDb, PetGraphView, candidates_from_backend,
    filter_impact_by_caller_depth, impact_score_from_counts, parse_fqn_symbol, resolve_symbol_uuid,
    try_load_engine, try_parse_symbol_uuid,
};
use rgctl_graph::CodeGraph;
use rgctl_graph::backend::GraphBackend;
use serde_json::Value;
use std::path::Path;
use uuid::Uuid;

/// Run blast-radius on a loaded graph.
pub fn run_impact(graph: &CodeGraph, repo: &Path, args: &ImpactArgs) -> Result<Value> {
    if args.symbol.trim().is_empty() {
        return Err(ServiceError::InvalidParams("`symbol` is required".into()));
    }
    let parsed = parse_fqn_symbol(&args.symbol, args.class.clone(), args.file.clone());
    let backend = graph.backend();
    let digest = crate::session::graph_digest_for(repo);
    let view = PetGraphView::from_backend(backend).map_err(ServiceError::from)?;
    let max_depth = args.depth.unwrap_or(usize::MAX);
    let caller_depth_limit = if max_depth == usize::MAX {
        None
    } else {
        Some(max_depth)
    };

    if parsed.class_filter.is_none()
        && let Some((type_id, member_ids)) = try_resolve_type_members(backend, &parsed)
    {
        let (result, impact_ids) = aggregate_type_blast(
            backend,
            repo,
            digest.as_deref(),
            &view,
            type_id,
            &member_ids,
            max_depth,
        )?;
        let lookup = NodeLookup::Backend(backend);
        let response = build_from_engine_result(
            &args.symbol,
            parsed.class_filter.clone(),
            &result,
            &result.direct_caller_ids,
            &impact_ids,
            result.score,
            caller_depth_limit,
            lookup,
            skipped_gatekeeping(),
        );
        return Ok(response_to_json(&response));
    }

    let (id, _) = resolve_target(backend, repo, &parsed)?;
    let result = resolve_blast_result(backend, repo, id, digest.as_deref())?;
    let function_impact =
        BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids)
            .map_err(ServiceError::from)?;
    let impact_ids = if max_depth == usize::MAX {
        function_impact
    } else {
        filter_impact_by_caller_depth(&view, id, &function_impact, max_depth)
    };
    let score = if max_depth == usize::MAX {
        result.score
    } else {
        impact_score_from_counts(result.direct_caller_ids.len(), impact_ids.len())
    };
    let lookup = NodeLookup::Backend(backend);
    let response = build_from_engine_result(
        &args.symbol,
        parsed.class_filter.clone(),
        &result,
        &result.direct_caller_ids,
        &impact_ids,
        score,
        caller_depth_limit,
        lookup,
        skipped_gatekeeping(),
    );
    Ok(response_to_json(&response))
}

fn try_resolve_type_members(
    backend: &rgctl_graph::backend::MemoryBackend,
    parsed: &rgctl_analysis::ParsedSymbol,
) -> Option<(Uuid, Vec<Uuid>)> {
    use rgctl_graph::schema::NodeType;
    let nodes = backend.find_nodes_by_name(&parsed.target_name).ok()?;
    if nodes.iter().any(|n| {
        n.node_type == NodeType::Function
            && !n
                .qualified_name
                .as_deref()
                .is_some_and(|qn| qn.ends_with(".<init>") || qn.ends_with("::<init>"))
    }) {
        return None;
    }
    let mut type_candidates: Vec<_> = nodes
        .into_iter()
        .filter(|n| {
            matches!(
                n.node_type,
                NodeType::Class | NodeType::Interface | NodeType::Struct
            )
        })
        .filter(|n| {
            if let Some(ref ff) = parsed.file_filter {
                let p = n.file_path.as_deref().unwrap_or("").replace('\\', "/");
                let f = ff.replace('\\', "/");
                if !p.contains(&f) {
                    return false;
                }
            }
            true
        })
        .collect();
    if type_candidates.len() != 1 {
        return None;
    }
    let type_node = type_candidates.pop()?;
    let qn = type_node
        .qualified_name
        .as_deref()
        .unwrap_or(type_node.name.as_str());
    let dot_prefix = format!("{qn}.");
    let colon_prefix = format!("{qn}::");
    let simple_dot = format!("{}.", type_node.name);
    let simple_colon = format!("{}::", type_node.name);
    let fn_nodes = backend.find_nodes_by_type(NodeType::Function).ok()?;
    let mut members = Vec::new();
    for fn_node in fn_nodes {
        if let Some(fqn) = fn_node.qualified_name.as_deref()
            && (fqn.starts_with(&dot_prefix) || fqn.starts_with(&colon_prefix))
        {
            members.push(fn_node.id);
            continue;
        }
        if type_node.file_path.is_some()
            && fn_node.file_path == type_node.file_path
            && fn_node
                .qualified_name
                .as_deref()
                .is_some_and(|fqn| fqn.starts_with(&simple_dot) || fqn.starts_with(&simple_colon))
        {
            members.push(fn_node.id);
        }
    }
    Some((type_node.id, members))
}

fn aggregate_type_blast(
    backend: &rgctl_graph::backend::MemoryBackend,
    repo: &Path,
    graph_digest: Option<&str>,
    view: &PetGraphView,
    type_id: Uuid,
    member_ids: &[Uuid],
    max_depth: usize,
) -> Result<(rgctl_analysis::BlastRadiusResult, Vec<Uuid>)> {
    use std::collections::HashSet;
    let member_set: HashSet<Uuid> = member_ids.iter().copied().collect();
    let mut direct_seen = HashSet::new();
    let mut direct_caller_ids = Vec::new();
    let mut impact_seen = HashSet::new();
    let mut impact_zone_ids = Vec::new();
    let mut depth_filtered_seen = HashSet::new();
    let mut depth_filtered_ids = Vec::new();

    for &mid in member_ids {
        let Ok(res) = resolve_blast_result(backend, repo, mid, graph_digest) else {
            continue;
        };
        for &cid in &res.direct_caller_ids {
            if cid != type_id && !member_set.contains(&cid) && direct_seen.insert(cid) {
                direct_caller_ids.push(cid);
            }
        }
        for &iid in &res.impact_zone_ids {
            if iid != type_id && !member_set.contains(&iid) && impact_seen.insert(iid) {
                impact_zone_ids.push(iid);
            }
        }
        let fn_impact = BlastRadiusEngine::filter_function_impact(backend, &res.impact_zone_ids)
            .map_err(ServiceError::from)?;
        let filtered = if max_depth == usize::MAX {
            fn_impact
        } else {
            filter_impact_by_caller_depth(view, mid, &fn_impact, max_depth)
        };
        for iid in filtered {
            if iid != type_id && !member_set.contains(&iid) && depth_filtered_seen.insert(iid) {
                depth_filtered_ids.push(iid);
            }
        }
    }

    let score = impact_score_from_counts(direct_caller_ids.len(), depth_filtered_ids.len());
    let result = rgctl_analysis::BlastRadiusResult {
        symbol_id: type_id,
        direct_caller_ids,
        impact_zone_ids,
        score,
        scc_id: 0,
        scc_size: member_ids.len().max(1),
    };
    Ok((result, depth_filtered_ids))
}

fn resolve_target(
    backend: &rgctl_graph::backend::MemoryBackend,
    repo: &Path,
    parsed: &rgctl_analysis::ParsedSymbol,
) -> Result<(Uuid, String)> {
    if let Some(id) = try_parse_symbol_uuid(&parsed.target_name) {
        let name = backend
            .get_node(id)
            .ok()
            .flatten()
            .map(|n| n.name.to_string())
            .unwrap_or_else(|| parsed.target_name.clone());
        return Ok((id, name));
    }
    let lookup_db = MacroCallLookupDb::default_path(repo);
    if MacroCallLookupDb::is_valid_for_repo(&lookup_db, repo).unwrap_or(false) {
        let candidates = MacroCallLookupDb::get_candidates(&lookup_db, &parsed.target_name)
            .map_err(ServiceError::from)?;
        if !candidates.is_empty() {
            let id = resolve_symbol_uuid(&candidates, parsed).map_err(ServiceError::from)?;
            return Ok((id, parsed.target_name.clone()));
        }
    }
    let index_path = MacroCallIndex::default_path(repo);
    if let Ok(Some(index)) = MacroCallIndex::load(&index_path) {
        if index.is_valid_for_repo(repo).unwrap_or(false) {
            let candidates = index.get_candidates(&parsed.target_name);
            if !candidates.is_empty() {
                let id = resolve_symbol_uuid(&candidates, parsed).map_err(ServiceError::from)?;
                return Ok((id, parsed.target_name.clone()));
            }
        }
    }
    let candidates = candidates_from_backend(backend, &parsed.target_name)?;
    if candidates.is_empty() {
        return Err(ServiceError::Failed(format!(
            "Node not found: {}",
            parsed.target_name
        )));
    }
    let id = resolve_symbol_uuid(&candidates, parsed).map_err(ServiceError::from)?;
    Ok((id, parsed.target_name.clone()))
}

fn resolve_blast_result(
    backend: &rgctl_graph::backend::MemoryBackend,
    repo: &Path,
    symbol_id: Uuid,
    graph_digest: Option<&str>,
) -> Result<rgctl_analysis::BlastRadiusResult> {
    if let Some(digest) = graph_digest {
        if let Some(engine) = try_load_engine(repo, digest).map_err(ServiceError::from)? {
            return engine.analyze(symbol_id).map_err(ServiceError::from);
        }
    }
    let engine = BlastRadiusEngine::build(backend).map_err(ServiceError::from)?;
    engine.analyze(symbol_id).map_err(ServiceError::from)
}
