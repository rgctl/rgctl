//! `rbuilder blast-radius` — SCC macro impact analysis.

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::Result;
use super::policy_file::PolicyFile;
use crate::analysis::{
    trace_blast_to_slices, BlastRadiusEngine, CentralityAnalyzer, PetGraphView,
};
use crate::graph::backend::GraphBackend;
use serde_json::json;
use std::path::Path;

pub struct BlastRadiusArgs {
    pub symbol: String,
    pub depth: Option<usize>,
    pub policy_file: Option<String>,
    pub no_policy: bool,
}

pub fn run(ctx: &CliContext, args: BlastRadiusArgs) -> Result<()> {
    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let engine = BlastRadiusEngine::build(backend)?;

    let registry = if args.no_policy {
        None
    } else if let Some(path) = &args.policy_file {
        Some(PolicyFile::load(Path::new(path))?.into_registry())
    } else {
        None
    };

    let centrality = if registry.is_some() {
        let view = PetGraphView::from_backend(backend)?;
        Some(CentralityAnalyzer::new().analyze_with_view(&view)?.scores)
    } else {
        None
    };

    let (id, _) = crate::analysis::resolve_unique_symbol(backend, &args.symbol)?;
    let result = if let Some(ref reg) = registry {
        engine.analyze_with_policy(id, Some(backend), Some(reg), centrality.as_ref())?
    } else {
        engine.analyze(id)?
    };

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

    if ctx.format == OutputFormat::Json {
        let mut handoffs = Vec::new();
        if let Ok(trace) = trace_blast_to_slices(backend, &ctx.repo, &args.symbol) {
            for seed in &trace.handoffs {
                handoffs.push(json!({
                    "callee": seed.callee_name,
                    "param": seed.param_name,
                    "index": seed.param_index,
                }));
            }
        }
        return ctx.emit_json_value(&json!({
            "symbol": args.symbol,
            "score": result.score,
            "direct_callers": direct_names,
            "impact_zone": impact_names,
            "handoffs": handoffs,
        }));
    }

    println!("Blast radius for '{}'", args.symbol);
    println!("  Score: {:.1}/100", result.score);
    println!("  Direct callers: {}", direct_names.len());
    println!("  Impact zone: {}", impact_names.len());
    if !direct_names.is_empty() {
        println!("  Callers: {}", direct_names.join(", "));
    }
    if !impact_names.is_empty() {
        println!("  Impact: {}", impact_names.join(", "));
    }

    if let Ok(trace) = trace_blast_to_slices(backend, &ctx.repo, &args.symbol) {
        for (func_id, name, slice) in &trace.slices {
            if slice.lines.is_empty() {
                continue;
            }
            let mut lines: Vec<_> = slice.lines.iter().copied().collect();
            lines.sort_unstable();
            println!(
                "  Slice '{name}' ({func_id}): lines {}",
                lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(", ")
            );
        }
    }
    Ok(())
}
