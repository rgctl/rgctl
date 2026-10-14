//! `rgctl slice` — line-level slicing and taint policy checks.

use super::args::{OutputFormat, SliceDirection, SliceView};
use super::context::{CliContext, language_from_path};
use super::markup::markup_context_unsupported;
use super::slice_output::{
    cfg_topology_json, pdg_topology_json, taint_slice_json, text_slice_json,
};
use crate::analysis::{
    BackwardSlicer, ForwardSlicer, FunctionLocation, ProgramDependenceGraph, SliceCriterion,
    TaintAnalyzer, build_cfg_for_function, index_function_locations,
};
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Byte offset at which a 1-based `line` begins, if the line exists.
fn line_start_byte(source: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    if line == 1 {
        return Some(0);
    }
    let mut current = 1;
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            current += 1;
            if current == line {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Name of the function whose span contains `offset`, preferring the innermost one.
///
/// Nested functions mean several spans can contain a line; the smallest span is the function the
/// line actually belongs to.
fn function_containing_line(
    locations: &HashMap<String, FunctionLocation>,
    offset: usize,
) -> Option<String> {
    locations
        .iter()
        .filter(|(_, loc)| loc.start_byte <= offset && offset <= loc.end_byte)
        .min_by_key(|(_, loc)| loc.end_byte - loc.start_byte)
        .map(|(name, _)| name.clone())
}

/// Add the function that was actually searched to a criterion lookup failure.
///
/// The slicer reports `no PDG node for <var> at line <N>`, which points at the variable and the
/// line — the two things the caller supplied and which are usually correct. Naming the function and
/// listing the file's other candidates makes the failure attributable to the real cause.
fn name_the_function_searched(
    err: anyhow::Error,
    fn_name: &str,
    locations: &HashMap<String, FunctionLocation>,
    explicit_function: bool,
) -> anyhow::Error {
    let msg = err.to_string();
    // The analysis error renders as `Resource not found: no PDG node for …`, so match on the
    // message body rather than its start.
    if !msg.contains("no PDG node for") {
        return err;
    }
    let provenance = if explicit_function {
        "from --function"
    } else {
        "resolved from the criterion line"
    };
    let mut names: Vec<&str> = locations.keys().map(String::as_str).collect();
    names.sort_unstable();
    let candidates = if names.is_empty() {
        String::new()
    } else {
        format!(
            " This file defines: {}.",
            names
                .iter()
                .map(|n| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    anyhow::anyhow!("{msg} — searched only '{fn_name}' ({provenance}).{candidates}")
}

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
    let path = resolve_slice_path(ctx, &args.file)?;
    if let Some(msg) = markup_context_unsupported("slice", &path) {
        bail!(msg);
    }
    let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let lang = args
        .language
        .clone()
        .unwrap_or_else(|| language_from_path(&path));
    // Which function to slice within. `--function` wins when given. Otherwise resolve the
    // function that *contains* the criterion line.
    //
    // The previous default was the file stem, which is not a defensible guess: files routinely
    // hold several functions, and the stem may match none of them. In `src/probe.ts` holding
    // `calc`, it produced `function 'probe' not found`; when the stem *did* match one function but
    // the line belonged to another in the same file, it produced `no PDG node for <var> at line
    // <N>` — blaming the variable and the line, the two things the caller actually supplied.
    let (_, locations) = index_function_locations(&lang, source.as_bytes())?;
    let explicit_function = args.function.is_some();
    let fn_name = match args.function.clone() {
        Some(name) => name,
        None => line_start_byte(&source, args.line)
            .and_then(|offset| function_containing_line(&locations, offset))
            // Nothing contains the line. Keep the historical fallback so behaviour changes only
            // where the old default was plainly wrong.
            .unwrap_or_else(|| {
                if lang == "python" {
                    "process".to_string()
                } else {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("main")
                        .to_string()
                }
            }),
    };

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
        SliceDirection::Backward => BackwardSlicer::new(&pdg, &cfg).slice(criterion.clone()),
        SliceDirection::Forward => ForwardSlicer::new(&pdg, &cfg).slice(criterion.clone()),
    }
    .map_err(|e| name_the_function_searched(e.into(), &fn_name, &locations, explicit_function))?;

    match args.view {
        SliceView::Cfg => {
            cfg.prune_unreachable_blocks();
            if ctx.format == OutputFormat::Json {
                let response = cfg_topology_json(&path.display().to_string(), &fn_name, &cfg);
                ctx.emit_json_value(&serde_json::to_value(&response)?)?;
            } else {
                println!(
                    "CFG: {} blocks, {} edges",
                    cfg.blocks.len(),
                    cfg.edges.len()
                );
            }
        }
        SliceView::Pdg => {
            if ctx.format == OutputFormat::Json {
                let response =
                    pdg_topology_json(&path.display().to_string(), &fn_name, &pdg, false);
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
        SliceView::Text => render_slice_text(
            ctx,
            &args.file,
            &fn_name,
            &criterion,
            &slice,
            &pdg,
            args.direction,
        )?,
    }
    Ok(())
}

/// Resolve a slice source path: prefer as-given, then relative to `--repo`.
fn resolve_slice_path(ctx: &CliContext, file: &str) -> Result<PathBuf> {
    let as_given = PathBuf::from(file);
    if as_given.is_file() {
        return Ok(as_given);
    }
    if as_given.is_absolute() {
        bail!("source file not found: {}", as_given.display());
    }
    let under_repo = ctx.repo.join(file);
    if under_repo.is_file() {
        return Ok(under_repo);
    }
    bail!(
        "source file not found: {file} (also tried {})",
        under_repo.display()
    );
}

fn render_slice_text(
    ctx: &CliContext,
    display_path: &str,
    function: &str,
    criterion: &SliceCriterion,
    slice: &crate::analysis::CodeSlice,
    pdg: &ProgramDependenceGraph,
    direction: SliceDirection,
) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        let response = text_slice_json(
            display_path,
            criterion,
            &format!("{direction:?}").to_lowercase(),
            slice,
            pdg,
        );
        return ctx.emit_json_value(&serde_json::to_value(&response)?);
    }
    println!(
        "{direction:?} slice for {display_path}:{} (variable: {})",
        criterion.line, criterion.variable
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

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_FUNCTIONS: &str = "export function useAgentPositions() {\n  const x = 1;\n  return x;\n}\n\nexport function fetchPositions(ids: number[]) {\n  const userIds = ids.map(String);\n  const total = userIds.length;\n  return total;\n}\n";

    fn locations(language: &str, source: &str) -> HashMap<String, FunctionLocation> {
        index_function_locations(language, source.as_bytes())
            .expect("index function locations")
            .1
    }

    #[test]
    fn line_start_byte_is_one_based() {
        let source = "a\nbb\nccc\n";
        assert_eq!(line_start_byte(source, 1), Some(0));
        assert_eq!(line_start_byte(source, 2), Some(2));
        assert_eq!(line_start_byte(source, 3), Some(5));
        assert_eq!(line_start_byte(source, 0), None, "lines are 1-based");
        assert_eq!(line_start_byte(source, 99), None, "past end of file");
    }

    /// The defect: the function was taken from the file stem, so a line belonging to the second
    /// function in a file was looked up in the first, and the error blamed the variable and line.
    #[test]
    fn resolves_the_function_that_contains_the_line() {
        let locs = locations("typescript", TWO_FUNCTIONS);
        let at = |line: usize| {
            function_containing_line(&locs, line_start_byte(TWO_FUNCTIONS, line).unwrap())
        };
        assert_eq!(at(2).as_deref(), Some("useAgentPositions"));
        assert_eq!(at(8).as_deref(), Some("fetchPositions"));
    }

    #[test]
    fn prefers_the_innermost_function_for_nested_spans() {
        let source = "function outer() {\n  function inner() {\n    const y = 1;\n    return y;\n  }\n  return inner();\n}\n";
        let locs = locations("typescript", source);
        let offset = line_start_byte(source, 3).unwrap();
        assert_eq!(
            function_containing_line(&locs, offset).as_deref(),
            Some("inner"),
            "several spans contain line 3; the smallest is the one it belongs to"
        );
    }

    #[test]
    fn no_function_contains_a_line_outside_every_span() {
        let source = format!("{TWO_FUNCTIONS}\n// trailing comment\n");
        let locs = locations("typescript", source.as_str());
        let offset = line_start_byte(&source, 12).unwrap();
        assert_eq!(function_containing_line(&locs, offset), None);
    }

    #[test]
    fn names_the_function_that_was_searched() {
        let locs = locations("typescript", TWO_FUNCTIONS);
        let err = anyhow::anyhow!("Resource not found: no PDG node for total at line 8");
        let out = name_the_function_searched(err, "useAgentPositions", &locs, false).to_string();
        assert!(out.contains("no PDG node for total at line 8"), "{out}");
        assert!(out.contains("'useAgentPositions'"), "{out}");
        assert!(out.contains("resolved from the criterion line"), "{out}");
        assert!(
            out.contains("'fetchPositions'"),
            "the other candidate should be listed: {out}"
        );
    }

    #[test]
    fn attributes_the_choice_to_an_explicit_function_flag() {
        let locs = locations("typescript", TWO_FUNCTIONS);
        let err = anyhow::anyhow!("Resource not found: no PDG node for total at line 8");
        let out = name_the_function_searched(err, "useAgentPositions", &locs, true).to_string();
        assert!(out.contains("from --function"), "{out}");
    }

    /// Unrelated failures must not be rewritten.
    #[test]
    fn leaves_other_errors_untouched() {
        let locs = locations("typescript", TWO_FUNCTIONS);
        let err = anyhow::anyhow!("Resource not found: function 'nope' not found");
        let out = name_the_function_searched(err, "useAgentPositions", &locs, false).to_string();
        assert_eq!(out, "Resource not found: function 'nope' not found");
    }
}
