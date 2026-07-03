//! `rbuilder blast-radius` — SCC macro impact analysis.

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::Result;
use super::policy_file::PolicyFile;
use crate::analysis::{
    candidates_from_backend, candidates_from_snapshot, evaluate_policies, parse_fqn_symbol,
    resolve_symbol_uuid, trace_blast_to_slices_with_blast, try_load_engine, try_parse_symbol_uuid,
    BlastRadiusEngine, BlastRadiusResult, CentralityAnalyzer, MacroCallIndex, MacroCallLookupDb,
    PetGraphView,
};
use rbuilder_graph::SnapshotNodeStore;
use crate::graph::backend::GraphBackend;
use serde_json::json;
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

struct BlastRadiusOutput {
    symbol: String,
    score: f64,
    direct_names: Vec<String>,
    impact_names: Vec<String>,
}

fn parsed_from_args(args: &BlastRadiusArgs) -> crate::analysis::ParsedSymbol {
    parse_fqn_symbol(&args.symbol, args.class.clone(), args.file.clone())
}

fn try_fast_cached_lookup(
    ctx: &CliContext,
    parsed: &crate::analysis::ParsedSymbol,
) -> Result<Option<BlastRadiusOutput>> {
    let lookup_db = MacroCallLookupDb::default_path(&ctx.repo);
    if MacroCallLookupDb::is_valid_for_repo(&lookup_db, &ctx.repo)? {
        if let Some(entry) = MacroCallLookupDb::lookup_resolved(&lookup_db, parsed)? {
            return Ok(Some(BlastRadiusOutput {
                symbol: args_display_symbol(parsed),
                score: entry.score,
                direct_names: entry.direct_callers,
                impact_names: entry.impact_zone,
            }));
        }
    }

    // Fallback: bulk bincode index (slower — loads entire cache file).
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
    let entry = index.get(id).expect("candidate entry");
    if entry.direct_caller_names.is_empty() && entry.impact_function_names.is_empty() {
        return Ok(None);
    }

    Ok(Some(BlastRadiusOutput {
        symbol: args_display_symbol(parsed),
        score: entry.score,
        direct_names: entry.direct_caller_names.clone(),
        impact_names: entry.impact_function_names.clone(),
    }))
}

fn args_display_symbol(parsed: &crate::analysis::ParsedSymbol) -> String {
    match (&parsed.class_filter, &parsed.file_filter) {
        (Some(class), _) => format!("{class}::{}", parsed.target_name),
        (None, Some(file)) => format!("{file}::{}", parsed.target_name),
        _ => parsed.target_name.clone(),
    }
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
                .get_node(id)
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
        candidates_from_snapshot(store, &parsed.target_name)
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

    let (id, _resolved_name) = resolve_target_uuid_snapshot(ctx, parsed, store)?;
    let result = engine.analyze(id)?;

    let impact_ids = store.filter_function_impact(&result.impact_zone_ids);
    let mut impact_names: Vec<String> = impact_ids
        .iter()
        .filter_map(|nid| store.get_node(*nid).map(|n| n.name.clone()))
        .collect();
    impact_names.sort();

    let direct_names: Vec<String> = result
        .direct_caller_ids
        .iter()
        .filter_map(|nid| store.get_node(*nid).map(|n| n.name.clone()))
        .collect();

    let output = BlastRadiusOutput {
        symbol: args.symbol.clone(),
        score: result.score,
        direct_names,
        impact_names,
    };
    emit_output(ctx, &output, Vec::new())?;
    Ok(Some(()))
}

fn resolve_blast_result(
    backend: &crate::graph::backend::MemoryBackend,
    repo: &Path,
    _graph_db: &Path,
    graph_digest: Option<&str>,
    symbol_id: uuid::Uuid,
    registry: Option<&crate::analysis::PolicyRegistry>,
    graph_view: Option<&PetGraphView>,
) -> Result<BlastRadiusResult> {
    if let Some(digest) = graph_digest {
        if let Some(engine) = try_load_engine(repo, digest)? {
            let result = engine.analyze(symbol_id)?;
            if let Some(reg) = registry {
                let built_view;
                let view = match graph_view {
                    Some(v) => v,
                    None => {
                        built_view = PetGraphView::from_backend(backend)?;
                        &built_view
                    }
                };
                evaluate_with_view(symbol_id, &result, reg, backend, view)?;
            }
            return Ok(result);
        }
    }

    let index_path = MacroCallIndex::default_path(repo);
    if let Some(index) = MacroCallIndex::load(&index_path)? {
        if index.is_valid_for(backend) || index.is_valid_for_repo(repo)? {
            if let Some(entry) = index.get(symbol_id) {
                let result = MacroCallIndex::to_blast_result(entry, symbol_id);
                if let Some(reg) = registry {
                    let built_view;
                    let view = match graph_view {
                        Some(v) => v,
                        None => {
                            built_view = PetGraphView::from_backend(backend)?;
                            &built_view
                        }
                    };
                    evaluate_with_view(symbol_id, &result, reg, backend, view)?;
                }
                return Ok(result);
            }
        }
    }

    let engine = BlastRadiusEngine::build(backend)?;
    let centrality = if registry.is_some() {
        let built_view;
        let view = match graph_view {
            Some(v) => v,
            None => {
                built_view = PetGraphView::from_backend(backend)?;
                &built_view
            }
        };
        Some(CentralityAnalyzer::new().analyze_with_view(view)?.scores)
    } else {
        None
    };

    if let Some(reg) = registry {
        Ok(engine.analyze_with_policy(
            symbol_id,
            Some(backend),
            Some(reg),
            centrality.as_ref(),
        )?)
    } else {
        Ok(engine.analyze(symbol_id)?)
    }
}

fn evaluate_with_view(
    symbol_id: Uuid,
    result: &BlastRadiusResult,
    reg: &crate::analysis::PolicyRegistry,
    backend: &crate::graph::backend::MemoryBackend,
    view: &PetGraphView,
) -> Result<()> {
    let centrality = CentralityAnalyzer::new().analyze_with_view(view)?.scores;
    evaluate_policies(
        symbol_id,
        &result.impact_zone_ids,
        reg,
        backend,
        Some(&centrality),
    )?;
    Ok(())
}

fn emit_output(
    ctx: &CliContext,
    output: &BlastRadiusOutput,
    handoffs: Vec<serde_json::Value>,
) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&json!({
            "symbol": output.symbol,
            "score": output.score,
            "direct_callers": output.direct_names,
            "impact_zone": output.impact_names,
            "handoffs": handoffs,
        }));
    }

    println!("Blast radius for '{}'", output.symbol);
    println!("  Score: {:.1}/100", output.score);
    println!("  Direct callers: {}", output.direct_names.len());
    println!("  Impact zone: {}", output.impact_names.len());
    if !output.direct_names.is_empty() {
        println!("  Callers: {}", output.direct_names.join(", "));
    }
    if !output.impact_names.is_empty() {
        println!("  Impact: {}", output.impact_names.join(", "));
    }
    Ok(())
}

pub fn run(ctx: &CliContext, args: BlastRadiusArgs) -> Result<()> {
    let parsed = parsed_from_args(&args);
    let needs_full_graph = args.with_slices || args.policy_file.is_some();

    if !needs_full_graph {
        if let Some(output) = try_fast_cached_lookup(ctx, &parsed)? {
            return emit_output(ctx, &output, Vec::new());
        }

        if let (Some(store), Some(digest)) = (
            ctx.open_snapshot_store()?,
            ctx.graph_digest()?.filter(|d| !d.is_empty()),
        ) {
            if try_snapshot_lite_path(ctx, &args, &parsed, &store, &digest)?.is_some() {
                return Ok(());
            }
        }
    }

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let graph_view = ctx
        .open_snapshot_store()?
        .as_ref()
        .map(|store| PetGraphView::from_prepared(store.prepared()))
        .transpose()?;
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
    let result = resolve_blast_result(
        backend,
        &ctx.repo,
        &ctx.db,
        graph_digest.as_deref(),
        id,
        registry.as_ref(),
        graph_view_ref,
    )?;

    let _depth = args.depth.unwrap_or(usize::MAX);
    let impact_ids = BlastRadiusEngine::filter_function_impact(backend, &result.impact_zone_ids)?;
    let mut impact_names: Vec<String> = impact_ids
        .iter()
        .filter_map(|nid| backend.get_node(*nid).ok().flatten().map(|n| n.name.clone()))
        .collect();
    impact_names.sort();

    let direct_names: Vec<String> = result
        .direct_caller_ids
        .iter()
        .filter_map(|nid| backend.get_node(*nid).ok().flatten().map(|n| n.name.clone()))
        .collect();

    let slice_trace = if args.with_slices {
        trace_blast_to_slices_with_blast(backend, &ctx.repo, id, &resolved_name, &result).ok()
    } else {
        None
    };

    let handoffs: Vec<_> = slice_trace
        .as_ref()
        .map(|trace| {
            trace
                .handoffs
                .iter()
                .map(|seed| {
                    json!({
                        "callee": seed.callee_name,
                        "param": seed.param_name,
                        "index": seed.param_index,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let output = BlastRadiusOutput {
        symbol: args.symbol.clone(),
        score: result.score,
        direct_names,
        impact_names,
    };
    emit_output(ctx, &output, handoffs)?;

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
