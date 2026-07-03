//! `rbuilder slice` — line-level slicing and taint policy checks.

use super::args::{OutputFormat, SliceDirection, SliceView};
use super::context::{language_from_path, CliContext};
use super::slice_output::{
    cfg_topology_json, pdg_topology_json, taint_slice_json, text_slice_json,
};
use anyhow::{Context, Result};
use crate::analysis::{
    build_cfg_for_function, BackwardSlicer, ProgramDependenceGraph, SliceCriterion, TaintAnalyzer,
};
use crate::analysis::pdg::PdgNodeId;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;

pub struct SliceArgs {
    pub file: String,
    pub line: usize,
    pub variable: String,
    pub function: Option<String>,
    pub language: Option<String>,
    pub direction: SliceDirection,
    pub taint: bool,
    pub view: SliceView,
}

pub fn run(ctx: &CliContext, args: SliceArgs) -> Result<()> {
    let path = Path::new(&args.file);
    let source = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let lang = args
        .language
        .clone()
        .unwrap_or_else(|| language_from_path(path));
    let fn_name = args.function.clone().unwrap_or_else(|| {
        if lang == "python" {
            "process".to_string()
        } else {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("main")
                .to_string()
        }
    });

    let mut cfg = build_cfg_for_function(&lang, &source, &fn_name)?;
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
    let criterion = SliceCriterion {
        variable: args.variable.clone(),
        line: args.line,
    };

    if args.taint {
        let mut analyzer = TaintAnalyzer::new(&pdg, &cfg);
        analyzer.detect_patterns(&lang);
        let flows = analyzer
            .analyze_with_policy()
            .map_err(|v| anyhow::anyhow!(v.to_string()))?;
        if ctx.format == OutputFormat::Json {
            let response = taint_slice_json(
                &args.file,
                &fn_name,
                args.line,
                &args.variable,
                flows.len(),
                flows.iter().filter(|f| f.is_vulnerable()).count(),
            );
            return ctx.emit_json_value(&serde_json::to_value(&response)?);
        }
        println!(
            "Taint flows: {} ({} vulnerable)",
            flows.len(),
            flows.iter().filter(|f| f.is_vulnerable()).count()
        );
        return Ok(());
    }

    let slice = match args.direction {
        SliceDirection::Backward => BackwardSlicer::new(&pdg, &cfg).slice(criterion.clone())?,
        SliceDirection::Forward => forward_slice(&pdg, &cfg, &criterion)?,
    };

    match args.view {
        SliceView::Cfg => {
            cfg.prune_unreachable_blocks();
            if ctx.format == OutputFormat::Json {
                let response = cfg_topology_json(
                    &path.display().to_string(),
                    &fn_name,
                    &cfg,
                );
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else {
                println!("CFG: {} blocks, {} edges", cfg.blocks.len(), cfg.edges.len());
            }
        }
        SliceView::Pdg => {
            if ctx.format == OutputFormat::Json {
                let response = pdg_topology_json(
                    &path.display().to_string(),
                    &fn_name,
                    &pdg,
                    false,
                );
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else {
                println!(
                    "PDG: {} nodes, {} data deps, {} control deps",
                    pdg.nodes.len(),
                    pdg.data_deps.len(),
                    pdg.control_deps.len()
                );
            }
        }
        SliceView::Text => render_slice_text(ctx, path, &fn_name, &criterion, &slice, &pdg, args.direction)?,
    }
    Ok(())
}

fn forward_slice(
    pdg: &ProgramDependenceGraph,
    cfg: &crate::analysis::ControlFlowGraph,
    criterion: &SliceCriterion,
) -> Result<crate::analysis::CodeSlice> {
    let criterion_node = pdg
        .nodes
        .values()
        .find(|n| {
            n.statement.line == criterion.line
                && (n.used_vars.contains(&criterion.variable)
                    || n.defined_vars.contains(&criterion.variable))
        })
        .map(|n| n.id)
        .ok_or_else(|| anyhow::anyhow!("criterion not found in PDG"))?;

    let mut slice = HashSet::<PdgNodeId>::new();
    let mut work = VecDeque::from([criterion_node]);
    while let Some(id) = work.pop_front() {
        if !slice.insert(id) {
            continue;
        }
        for dep in pdg.data_deps.iter().filter(|d| d.from == id) {
            work.push_back(dep.to);
        }
        for ctrl in pdg.control_deps.iter().filter(|c| c.controller == id) {
            work.push_back(ctrl.dependent);
        }
    }

    let lines: HashSet<usize> = slice
        .iter()
        .filter_map(|id| pdg.nodes.get(id).map(|n| n.statement.line))
        .collect();
    let total_lines = cfg
        .blocks
        .values()
        .map(|b| b.statements.len())
        .sum::<usize>()
        .max(1);
    let line_count = lines.len();
    Ok(crate::analysis::CodeSlice {
        criterion: criterion.clone(),
        statements: slice,
        lines,
        reduction_percent: 100.0 * (1.0 - (line_count as f64 / total_lines as f64)),
    })
}

fn render_slice_text(
    ctx: &CliContext,
    path: &Path,
    function: &str,
    criterion: &SliceCriterion,
    slice: &crate::analysis::CodeSlice,
    pdg: &ProgramDependenceGraph,
    direction: SliceDirection,
) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        let response = text_slice_json(
            &path.display().to_string(),
            criterion,
            &format!("{direction:?}").to_lowercase(),
            slice,
            pdg,
        );
        return ctx.emit_json_value(&serde_json::to_value(&response)?);
    }
    println!(
        "{direction:?} slice for {}:{} (variable: {})",
        path.display(),
        criterion.line,
        criterion.variable
    );
    println!("Reduction: {:.1}%", slice.reduction_percent);
    let mut lines: Vec<_> = slice.lines.iter().copied().collect();
    lines.sort_unstable();
    for ln in lines {
        println!("  {ln}");
    }
    let _ = function;
    Ok(())
}
