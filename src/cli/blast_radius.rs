//! `rbuilder blast-radius` — SCC macro impact analysis.

use super::args::OutputFormat;
use super::blast_radius_output::{
    build_from_cache_entry, build_from_engine_result, emit_text, evaluate_gatekeeping,
    handoffs_from_seeds, response_to_json, skipped_gatekeeping, BlastRadiusResponse, NodeLookup,
};
use super::context::CliContext;
use super::query_daemon;
use anyhow::Result;
use super::policy_file::PolicyFile;
use crate::analysis::{
    candidates_from_backend, candidates_from_snapshot, filter_impact_by_caller_depth,
    impact_score_from_counts, parse_fqn_symbol, resolve_handoff_seeds, resolve_symbol_uuid,
    trace_blast_to_slices_with_blast, try_load_engine, try_parse_symbol_uuid, BlastRadiusEngine,
    BlastRadiusResult, MacroCallIndex, MacroIndexEntry, MacroCallLookupDb, PetGraphView,
};
use rbuilder_graph::SnapshotNodeStore;
use crate::graph::backend::GraphBackend;
use std::path::Path;
use uuid::Uuid;

pub struct BlastRadiusArgs {
    pub symbol: String,
    pub depth: Option<usize>,
    pub policy_file: Option<String>,
    pub no_policy: bool,
    pub with_slices: bool,
    pub class: Option<String>,
    pub file: Option<String>,
}

struct PreparedImpact {
    impact_ids: Vec<Uuid>,
    score: f64,
    caller_depth_limit: Option<usize>,
}

fn max_caller_depth(args: &BlastRadiusArgs) -> usize {
    args.depth.unwrap_or(usize::MAX)
}

fn depth_limit_metric(max_depth: usize) -> Option<usize> {
    if max_depth == usize::MAX {
        None
    } else {
        Some(max_depth)
    }
}

fn prepare_engine_impact(
    view: &PetGraphView,
    target_id: Uuid,
    result: &BlastRadiusResult,
    function_impact_ids: &[Uuid],
    max_depth: usize,
) -> PreparedImpact {
    let impact_ids = if max_depth == usize::MAX {
        function_impact_ids.to_vec()
    } else {
        filter_impact_by_caller_depth(view, target_id, function_impact_ids, max_depth)
    };
    let score = if max_depth == usize::MAX {
        result.score
    } else {
        impact_score_from_counts(result.direct_caller_ids.len(), impact_ids.len())
    };
    PreparedImpact {
        impact_ids,
        score,
        caller_depth_limit: depth_limit_metric(max_depth),
    }
}

fn prepare_cache_impact(
    view: &PetGraphView,
    entry: &MacroIndexEntry,
    max_depth: usize,
) -> PreparedImpact {
    let impact_ids = if max_depth == usize::MAX {
        entry.impact_zone_ids.clone()
    } else {
        filter_impact_by_caller_depth(view, entry.id, &entry.impact_zone_ids, max_depth)
    };
    let score = if max_depth == usize::MAX {
        entry.score
    } else {
        impact_score_from_counts(entry.direct_caller_ids.len(), impact_ids.len())
    };
    PreparedImpact {
        impact_ids,
        score,
        caller_depth_limit: depth_limit_metric(max_depth),
    }
}

fn parsed_from_args(args: &BlastRadiusArgs) -> crate::analysis::ParsedSymbol {
    parse_fqn_symbol(&args.symbol, args.class.clone(), args.file.clone())
}

fn node_lookup<'a>(
    backend: Option<&'a crate::graph::backend::MemoryBackend>,
    store: Option<&'a SnapshotNodeStore>,
) -> NodeLookup<'a> {
    match (backend, store) {
        (Some(b), _) => NodeLookup::Backend(b),
        (None, Some(s)) => NodeLookup::Snapshot(s),
        _ => NodeLookup::None,
    }
}

fn try_fast_cached_lookup(
    ctx: &CliContext,
    args: &BlastRadiusArgs,
    parsed: &crate::analysis::ParsedSymbol,
) -> Result<Option<BlastRadiusResponse>> {
    let session = ctx.snapshot_session()?;
    let lookup = node_lookup(
        None,
        session.as_ref().map(|s| s.store.as_ref()),
    );
    let max_depth = max_caller_depth(args);
    let view = session
        .as_ref()
        .and_then(|s| PetGraphView::from_snapshot_store(&s.store).ok());
    if max_depth != usize::MAX && view.is_none() {
        return Ok(None);
    }

    let lookup_db = MacroCallLookupDb::default_path(&ctx.repo);
    if MacroCallLookupDb::is_valid_for_repo(&lookup_db, &ctx.repo)? {
        if parsed.class_filter.is_none() && parsed.file_filter.is_none() {
            if let Some(row) = MacroCallLookupDb::lookup(&lookup_db, &parsed.target_name)? {
                let entry = MacroCallLookupDb::index_entry_from_lookup_row(row);
                let prepared = view
                    .as_ref()
                    .map(|v| prepare_cache_impact(v, &entry, max_depth))
                    .unwrap_or(PreparedImpact {
                        impact_ids: entry.impact_zone_ids.clone(),
                        score: entry.score,
                        caller_depth_limit: None,
                    });
                return Ok(Some(build_from_cache_entry(
                    &entry,
                    skipped_gatekeeping(),
                    lookup,
                    &prepared.impact_ids,
                    prepared.score,
                    prepared.caller_depth_limit,
                )));
            }
        }
        if let Some(entry) = MacroCallLookupDb::lookup_resolved(&lookup_db, parsed)? {
            let prepared = view
                .as_ref()
                .map(|v| prepare_cache_impact(v, &entry, max_depth))
                .unwrap_or(PreparedImpact {
                    impact_ids: entry.impact_zone_ids.clone(),
                    score: entry.score,
                    caller_depth_limit: None,
                });
            return Ok(Some(build_from_cache_entry(
                &entry,
                skipped_gatekeeping(),
                lookup,
                &prepared.impact_ids,
                prepared.score,
                prepared.caller_depth_limit,
            )));
        }
    }

    let index_path = MacroCallIndex::default_path(&ctx.repo);
    let Some(index) = MacroCallIndex::load(&index_path)? else {
        return Ok(None);
    };

    if !index.is_valid_for_repo(&ctx.repo)? {
        return Ok(None);
    }

    let candidates = index.get_candidates(&parsed.target_name);
    if candidates.is_empty() {
        return Ok(None);
    }
    let id = resolve_symbol_uuid(&candidates, parsed)?;
    let cache_entry = candidates
        .into_iter()
        .find(|c| c.id == id)
        .expect("candidate entry");
    if cache_entry.direct_callers.is_empty() && cache_entry.impact_zone.is_empty() {
        return Ok(None);
    }

    let prepared = view
        .as_ref()
        .map(|v| prepare_cache_impact(v, &cache_entry, max_depth))
        .unwrap_or(PreparedImpact {
            impact_ids: cache_entry.impact_zone_ids.clone(),
            score: cache_entry.score,
            caller_depth_limit: None,
        });
    Ok(Some(build_from_cache_entry(
        &cache_entry,
        skipped_gatekeeping(),
        lookup,
        &prepared.impact_ids,
        prepared.score,
        prepared.caller_depth_limit,
    )))
}

fn resolve_target_uuid(
    backend: &crate::graph::backend::MemoryBackend,
    ctx: &CliContext,
    parsed: &crate::analysis::ParsedSymbol,
) -> Result<(Uuid, String)> {
    resolve_target_uuid_impl(ctx, parsed, Some(backend), None)
}

fn resolve_target_uuid_snapshot(
    ctx: &CliContext,
    parsed: &crate::analysis::ParsedSymbol,
    store: &SnapshotNodeStore,
) -> Result<(Uuid, String)> {
    resolve_target_uuid_impl(ctx, parsed, None, Some(store))
}

fn resolve_target_uuid_impl(
    ctx: &CliContext,
    parsed: &crate::analysis::ParsedSymbol,
    backend: Option<&crate::graph::backend::MemoryBackend>,
    store: Option<&SnapshotNodeStore>,
) -> Result<(Uuid, String)> {
    if let Some(id) = try_parse_symbol_uuid(&parsed.target_name) {
        let name = if let Some(store) = store {
            store
                .get_node(id)?
                .map(|n| n.name.clone())
                .unwrap_or_else(|| parsed.target_name.clone())
        } else {
            backend
                .expect("backend required when store absent")
                .get_node(id)?
                .map(|n| n.name.clone())
                .unwrap_or_else(|| parsed.target_name.clone())
        };
        return Ok((id, name));
    }

    let lookup_db = MacroCallLookupDb::default_path(&ctx.repo);
    if MacroCallLookupDb::is_valid_for_repo(&lookup_db, &ctx.repo)? {
        let candidates = MacroCallLookupDb::get_candidates(&lookup_db, &parsed.target_name)?;
        if !candidates.is_empty() {
            let id = resolve_symbol_uuid(&candidates, parsed)?;
            return Ok((id, parsed.target_name.clone()));
        }
    }

    let index_path = MacroCallIndex::default_path(&ctx.repo);
    if let Some(index) = MacroCallIndex::load(&index_path)? {
        if index.is_valid_for_repo(&ctx.repo)? {
            let candidates = index.get_candidates(&parsed.target_name);
            if !candidates.is_empty() {
                let id = resolve_symbol_uuid(&candidates, parsed)?;
                return Ok((id, parsed.target_name.clone()));
            }
        }
    }

    let mut candidates = if let Some(store) = store {
        candidates_from_snapshot(store, &parsed.target_name)?
    } else {
        candidates_from_backend(
            backend.expect("backend required when store absent"),
            &parsed.target_name,
        )?
    };
    if candidates.is_empty() {
        return Err(rbuilder_error::Error::NodeNotFound(parsed.target_name.clone()).into());
    }

    if let Ok(Some(index)) = MacroCallIndex::load(&MacroCallIndex::default_path(&ctx.repo)) {
        for candidate in &mut candidates {
            if let Some(entry) = index.get(candidate.id) {
                candidate.score = entry.score;
                candidate.direct_callers = entry.direct_caller_names.clone();
                candidate.impact_zone = entry.impact_function_names.clone();
            }
        }
    }

    let id = resolve_symbol_uuid(&candidates, parsed)?;
    Ok((id, parsed.target_name.clone()))
}

fn try_snapshot_lite_path(
    ctx: &CliContext,
    args: &BlastRadiusArgs,
    parsed: &crate::analysis::ParsedSymbol,
    store: &SnapshotNodeStore,
    digest: &str,
) -> Result<Option<()>> {
    let Some(engine) = try_load_engine(&ctx.repo, digest)? else {
        return Ok(None);
    };

    let response = build_lite_response(ctx, args, parsed, store, &engine)?;
    emit_output(ctx, &response)?;
    Ok(Some(()))
}

/// Lite blast-radius response using mmap store + warm engine (no full graph hydrate).
pub(crate) fn build_lite_response(
    ctx: &CliContext,
    args: &BlastRadiusArgs,
    parsed: &crate::analysis::ParsedSymbol,
    store: &SnapshotNodeStore,
    engine: &BlastRadiusEngine,
) -> Result<BlastRadiusResponse> {
    let (id, _resolved_name) = resolve_target_uuid_snapshot(ctx, parsed, store)?;
    let result = engine.analyze(id)?;
    let view = PetGraphView::from_snapshot_store(store)?;
    let function_impact = store.filter_function_impact(&result.impact_zone_ids)?;
    let max_depth = max_caller_depth(args);
    let prepared = prepare_engine_impact(&view, id, &result, &function_impact, max_depth);
    let lookup = NodeLookup::Snapshot(store);
    Ok(build_from_engine_result(
        &args.symbol,
        parsed.class_filter.clone(),
        &result,
        &result.direct_caller_ids,
        &prepared.impact_ids,
        prepared.score,
        prepared.caller_depth_limit,
        lookup,
        skipped_gatekeeping(),
    ))
}

fn resolve_blast_result(
    backend: &crate::graph::backend::MemoryBackend,
    repo: &Path,
    symbol_id: uuid::Uuid,
    graph_digest: Option<&str>,
) -> Result<BlastRadiusResult> {
    if let Some(digest) = graph_digest {
        if let Some(engine) = try_load_engine(repo, digest)? {
            return Ok(engine.analyze(symbol_id)?);
        }
    }

    let index_path = MacroCallIndex::default_path(repo);
    if let Some(index) = MacroCallIndex::load(&index_path)? {
        if index.is_valid_for(backend) || index.is_valid_for_repo(repo)? {
            if let Some(entry) = index.get(symbol_id) {
                return Ok(MacroCallIndex::to_blast_result(entry, symbol_id));
            }
        }
    }

    let engine = BlastRadiusEngine::build(backend)?;
    Ok(engine.analyze(symbol_id)?)
}

fn emit_output(ctx: &CliContext, response: &BlastRadiusResponse) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&response_to_json(response));
    }
    ctx.emit(&emit_text(response))
}

pub fn run(ctx: &CliContext, args: BlastRadiusArgs) -> Result<()> {
    let parsed = parsed_from_args(&args);
    let needs_full_graph = args.with_slices || args.policy_file.is_some();

    if !needs_full_graph {
        if let Some(response) = try_fast_cached_lookup(ctx, &args, &parsed)? {
            return emit_output(ctx, &response);
        }

        if let Some(session) = ctx.snapshot_session()? {
            if let Some(response) = query_daemon::try_client_blast_radius(
                ctx,
                &args,
                &parsed,
                session.digest.as_ref(),
            )? {
                return emit_output(ctx, &response);
            }

            if try_snapshot_lite_path(
                ctx,
                &args,
                &parsed,
                session.store.as_ref(),
                session.digest.as_ref(),
            )?
            .is_some()
            {
                return Ok(());
            }
        }
    }

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let snapshot_store = ctx.open_snapshot_store()?;
    let graph_view = snapshot_store
        .as_ref()
        .map(|store| PetGraphView::from_snapshot_store(store))
        .transpose()?;
    let built_view;
    let view_for_depth: &PetGraphView = match graph_view.as_ref() {
        Some(v) => v,
        None => {
            built_view = PetGraphView::from_backend(backend)?;
            &built_view
        }
    };
    let graph_view_ref = graph_view.as_ref();

    let registry = if args.no_policy {
        None
    } else if let Some(path) = &args.policy_file {
        Some(PolicyFile::load(Path::new(path))?.into_registry())
    } else {
        None
    };

    let (id, resolved_name) = resolve_target_uuid(backend, ctx, &parsed)?;
    let graph_digest = ctx.graph_digest().ok().flatten();
    let result = resolve_blast_result(backend, &ctx.repo, id, graph_digest.as_deref())?;

    let max_depth = max_caller_depth(&args);
    let function_impact = BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids)?;
    let prepared = prepare_engine_impact(view_for_depth, id, &result, &function_impact, max_depth);
    let impact_ids = prepared.impact_ids;

    let slice_trace = if args.with_slices {
        trace_blast_to_slices_with_blast(backend, &ctx.repo, id, &resolved_name, &result).ok()
    } else {
        None
    };

    let handoffs = if args.with_slices {
        resolve_handoff_seeds(backend, &result, id)
            .map(|seeds| handoffs_from_seeds(&seeds))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let gatekeeping = evaluate_gatekeeping(
        registry.as_ref(),
        backend,
        graph_view_ref,
        id,
        &impact_ids,
        handoffs,
    )?;

    let lookup = node_lookup(
        Some(backend),
        snapshot_store.as_deref(),
    );
    let response = build_from_engine_result(
        &args.symbol,
        parsed.class_filter.clone(),
        &result,
        &result.direct_caller_ids,
        &impact_ids,
        prepared.score,
        prepared.caller_depth_limit,
        lookup,
        gatekeeping,
    );
    emit_output(ctx, &response)?;

    if response.gatekeeping.policy_status == "VIOLATED" {
        std::process::exit(1);
    }

    if let Some(trace) = slice_trace {
        for (func_id, name, slice) in &trace.slices {
            if slice.lines.is_empty() {
                continue;
            }
            let mut lines: Vec<_> = slice.lines.iter().copied().collect();
            lines.sort_unstable();
            println!(
                "  Slice '{name}' ({func_id}): lines {}",
                lines
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}
