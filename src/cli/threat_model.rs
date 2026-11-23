//! `rgctl vuln threat-model` (alias `surface`) — deterministic threat-model evidence pack,
//! Mermaid Data-Flow Diagram, and rule-based STRIDE candidate synthesis (#229).

use super::OutputFormat;
use super::context::{CliContext, language_from_path};
use super::vuln_deps::{
    AnalyzeCliArgs, evaluate_repo_vulns, function_containing_line, line_start_byte,
};
use crate::analysis::{
    EnsureCfgPdgOpts, FunctionLocation, TaintAnalyzer, TaintRuleSet, ensure_cfg_pdg_for_source,
    index_function_locations,
};
use anyhow::{Result, bail};
use rayon::prelude::*;
use rgctl_analysis::{AnalysisResults, Sanitizer, TaintSink};
use rgctl_graph::paths::artifact_path;
use rgctl_security::{
    DiscoveredPdgSink, FunctionPdgReport, FunctionPdgRequest, OSV_CACHE_REL, PdgSinkStatus,
    ThreatModelBuildOpts, build_threat_model, classify_callee_sanitizer, classify_callee_sink,
    render_threat_model_markdown, render_threat_model_mermaid,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct ThreatModelCliArgs {
    pub osv: Option<PathBuf>,
    pub from_report: Option<PathBuf>,
    pub sbom: Option<PathBuf>,
    pub offline: bool,
    pub online: bool,
    pub osv_api_url: Option<String>,
    pub include_jars: Vec<PathBuf>,
    pub include_node_modules: Vec<PathBuf>,
    pub with_dep_stubs: bool,
    pub max_depth: usize,
    pub max_pdg_functions: usize,
    pub no_lazy_cfg: bool,
}

#[derive(Deserialize)]
struct CallableResolutionHeader {
    #[serde(default)]
    unresolved_calls: usize,
}

fn read_unresolved_calls_count(repo: &Path) -> usize {
    let path = artifact_path(repo, "callable_resolution.json");
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CallableResolutionHeader>(&bytes).ok())
        .map(|h| h.unresolved_calls)
        .unwrap_or(0)
}

fn local_osv_cache_has_json(repo: &Path) -> bool {
    let dir = repo.join(OSV_CACHE_REL);
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
}

/// Extract a concise callee identifier from a statement (e.g. `subprocess.run(cmd)` -> `subprocess.run`).
fn extract_call_callee(stmt: &str, fallback: &str) -> String {
    if let Some(paren_pos) = stmt.find('(') {
        let prefix = stmt[..paren_pos].trim_end();
        let start = prefix
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '\\')))
            .map(|i| i + 1)
            .unwrap_or(0);
        let cand = &prefix[start..];
        if !cand.is_empty()
            && !matches!(
                cand,
                "if" | "while" | "for" | "match" | "switch" | "return" | "catch"
            )
        {
            return cand.to_string();
        }
    }
    fallback.to_string()
}

/// Execute `rgctl vuln threat-model`.
pub fn run_vuln_threat_model(ctx: &CliContext, args: ThreatModelCliArgs) -> Result<()> {
    let run_start = std::time::Instant::now();
    let mut stage = std::time::Instant::now();
    let mut lap = |name: &str| {
        tracing::info!(
            target: "profile",
            "[profile] threat-model stage {name}: {:.3}s",
            stage.elapsed().as_secs_f64()
        );
        stage = std::time::Instant::now();
    };
    let Some(store) = ctx.open_snapshot_store()? else {
        bail!(
            "Graph not found at {} (run `rgctl discover` first)",
            ctx.repo.display()
        );
    };
    let Some(columnar) = store.columnar() else {
        bail!(
            "Columnar snapshot not found at {} (run `rgctl discover` first)",
            ctx.repo.display()
        );
    };

    lap("open_snapshot");

    let parked_path = artifact_path(&ctx.repo, rgctl_extraction::PARKED_RELATIONS_FILE);
    let parked_relations =
        rgctl_extraction::load_parked_relations(&parked_path).unwrap_or_default();
    lap("load_parked_relations");

    let analysis_path = artifact_path(&ctx.repo, "analysis_results.bin");
    let analysis = AnalysisResults::load(&analysis_path).ok();
    lap("load_analysis_results");
    let unresolved_calls_count = read_unresolved_calls_count(&ctx.repo);

    // Offline-by-default determinism: only evaluate OSV advisories when a local override,
    // a populated `.rgctl/osv/` cache, or explicit `--online` is provided.
    let should_eval_osv = args.osv.is_some()
        || args.from_report.is_some()
        || (args.online && !args.offline)
        || local_osv_cache_has_json(&ctx.repo);

    let (vuln_results, osv_evaluated) = if should_eval_osv {
        let offline = !args.online || args.offline;
        let eval_args = AnalyzeCliArgs {
            osv: args.osv,
            from_report: args.from_report,
            sbom: args.sbom,
            offline,
            osv_api_url: args.osv_api_url,
            include_jars: args.include_jars,
            include_node_modules: args.include_node_modules,
            with_dep_stubs: args.with_dep_stubs,
            strict_reachability: false,
        };
        (evaluate_repo_vulns(ctx, &eval_args)?, true)
    } else {
        (Vec::new(), false)
    };
    lap("osv_eval");

    let project_overlay_rules = ctx
        .repo
        .join(".rgctl")
        .join("taint-rules.d")
        .is_dir()
        .then(|| TaintRuleSet::load_with_overlays(Some(&ctx.repo), None, None).ok())
        .flatten();
    let empty_rules = TaintRuleSet::empty();
    let taint_rules: &TaintRuleSet = project_overlay_rules
        .as_ref()
        .or_else(|| TaintRuleSet::bundled_shared().as_ref().ok())
        .unwrap_or(&empty_rules);

    let mut pdg_secs = 0.0f64;

    let report = build_threat_model(
        ThreatModelBuildOpts {
            repo_root: &ctx.repo,
            columnar,
            parked_relations: &parked_relations,
            analysis: analysis.as_ref(),
            unresolved_calls_count,
            vuln_results: &vuln_results,
            osv_evaluated,
            max_depth: args.max_depth,
            // `--no-lazy-cfg` means a zero PDG budget, so every skipped function is reported.
            max_pdg_functions: if args.no_lazy_cfg {
                0
            } else {
                args.max_pdg_functions
            },
        },
        |reqs: &[FunctionPdgRequest<'_>]| {
            if args.no_lazy_cfg || reqs.is_empty() {
                return vec![FunctionPdgReport::default(); reqs.len()];
            }
            let t = std::time::Instant::now();
            let out = inspect_functions_pdg_parallel(&ctx.repo, taint_rules, reqs);
            pdg_secs += t.elapsed().as_secs_f64();
            out
        },
    )?;
    tracing::info!(
        target: "profile",
        "[profile] threat-model lazy_pdg (inside build): {pdg_secs:.3}s"
    );
    lap("build_threat_model");

    let emitted = match ctx.format {
        OutputFormat::Json => ctx.emit_serializable(&report),
        OutputFormat::Mermaid => ctx.emit(&render_threat_model_mermaid(&report)),
        OutputFormat::Sarif => bail!(
            "`rgctl vuln threat-model` does not emit SARIF (use `-f json`, `-f text`, or `-f mermaid`; use `rgctl -f sarif vuln scan` for SARIF 2.1.0)"
        ),
        OutputFormat::Text | OutputFormat::Graphviz => {
            ctx.emit(&render_threat_model_markdown(&report))
        }
    };
    lap("emit");
    tracing::info!(
        target: "profile",
        "[profile] threat-model total: {:.3}s",
        run_start.elapsed().as_secs_f64()
    );
    emitted
}

/// Evaluate lazy CFG/PDG taint across entry-reachable functions grouped by file in parallel.
///
/// Keyed by `function_id: Uuid` + exact `decl_byte_span` from the columnar snapshot so overloaded
/// or same-short-name methods in a file never collide, and full-file `index_function_locations`
/// parsing is skipped whenever `decl_byte_span` is present.
fn inspect_functions_pdg_parallel(
    repo_root: &Path,
    taint_rules: &TaintRuleSet,
    reqs: &[FunctionPdgRequest<'_>],
) -> Vec<FunctionPdgReport> {
    let mut by_file: BTreeMap<&str, Vec<(usize, &FunctionPdgRequest<'_>)>> = BTreeMap::new();
    for (idx, req) in reqs.iter().enumerate() {
        by_file.entry(req.file).or_default().push((idx, req));
    }
    let groups: Vec<(&str, Vec<(usize, &FunctionPdgRequest<'_>)>)> = by_file.into_iter().collect();

    let group_results: Vec<Vec<(usize, FunctionPdgReport)>> = groups
        .par_iter()
        .map(|(file, items)| inspect_file_group(repo_root, taint_rules, file, items))
        .collect();

    let mut out = vec![FunctionPdgReport::default(); reqs.len()];
    for batch in group_results {
        for (orig_idx, rep) in batch {
            out[orig_idx] = rep;
        }
    }
    out
}

fn inspect_file_group(
    repo_root: &Path,
    taint_rules: &TaintRuleSet,
    file: &str,
    items: &[(usize, &FunctionPdgRequest<'_>)],
) -> Vec<(usize, FunctionPdgReport)> {
    let rel_path = Path::new(file);
    let abs_path = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        repo_root.join(rel_path)
    };
    let Some(lang_id) = crate::analysis::language_id_from_path(&abs_path) else {
        return items
            .iter()
            .map(|&(idx, _)| (idx, FunctionPdgReport::default()))
            .collect();
    };
    let lang = language_from_path(&abs_path);
    if lang == "markdown" || lang_id == "hcl" {
        return items
            .iter()
            .map(|&(idx, _)| (idx, FunctionPdgReport::default()))
            .collect();
    }
    let Ok(source) = std::fs::read_to_string(&abs_path) else {
        return items
            .iter()
            .map(|&(idx, _)| (idx, FunctionPdgReport::default()))
            .collect();
    };

    // Only parse a fallback location map when at least one request lacked a snapshot byte span.
    let fallback_locations: Option<HashMap<String, FunctionLocation>> = items
        .iter()
        .any(|(_, r)| r.decl_byte_span.is_none())
        .then(|| {
            index_function_locations(&lang, source.as_bytes())
                .ok()
                .map(|(_, locs)| locs)
        })
        .flatten();

    let mut fn_cache: HashMap<Uuid, FunctionPdgReport> = HashMap::with_capacity(items.len());
    let mut results = Vec::with_capacity(items.len());

    for &(orig_idx, req) in items {
        if let Some(existing) = fn_cache.get(&req.function_id) {
            results.push((orig_idx, existing.clone()));
            continue;
        }
        let rep = evaluate_single_function_pdg(
            repo_root,
            taint_rules,
            &lang,
            &source,
            fallback_locations.as_ref(),
            req,
        );
        fn_cache.insert(req.function_id, rep.clone());
        results.push((orig_idx, rep));
    }
    results
}

fn evaluate_single_function_pdg(
    repo_root: &Path,
    taint_rules: &TaintRuleSet,
    lang: &str,
    source: &str,
    fallback_locations: Option<&HashMap<String, FunctionLocation>>,
    req: &FunctionPdgRequest<'_>,
) -> FunctionPdgReport {
    let short_name = req
        .function_fqn
        .rsplit(['.', ':', '\\'])
        .next()
        .unwrap_or(req.function_fqn)
        .to_string();

    let (fn_name, start_byte, end_byte) = if let Some((s, e)) = req.decl_byte_span
        && s < e
        && e <= source.len()
    {
        (short_name, Some(s), Some(e))
    } else if let Some(locs) = fallback_locations
        && let Some((name, loc)) = line_start_byte(source, req.start_line as usize)
            .and_then(|off| function_containing_line(locs, off))
            .or_else(|| {
                line_start_byte(source, req.start_line.saturating_add(1) as usize)
                    .and_then(|off| function_containing_line(locs, off))
            })
    {
        (name, Some(loc.start_byte), Some(loc.end_byte))
    } else {
        (short_name, None, None)
    };

    let Ok(record) = ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
        repo_root,
        language: lang,
        source,
        function_name: &fn_name,
        file_path: Some(req.file),
        start_byte,
        end_byte,
        function_id: Some(req.function_id),
        persist: false,
    }) else {
        return FunctionPdgReport::default();
    };

    let pdg = record.pdg.as_ref();
    let cfg = record.cfg.as_ref();
    let mut analyzer = TaintAnalyzer::new(pdg, cfg);
    analyzer.detect_patterns_with_rules(lang, taint_rules);
    let flows = analyzer.analyze();

    let compiled_lang = taint_rules.for_language(lang);
    let mut has_sanitizer = false;
    let mut has_audit_log = false;
    let mut by_line: BTreeMap<(u32, String), DiscoveredPdgSink> = BTreeMap::new();

    // `pdg.nodes` is a HashMap: visit statements in source order so same-line merges are stable.
    let mut ordered_nodes: Vec<_> = pdg.nodes.iter().collect();
    ordered_nodes.sort_by(|(ida, a), (idb, b)| {
        (a.statement.line, a.statement.text.as_str(), **ida).cmp(&(
            b.statement.line,
            b.statement.text.as_str(),
            **idb,
        ))
    });
    for (&node_id, node) in ordered_nodes {
        let text = node.statement.text.trim();
        if text.is_empty() {
            continue;
        }
        let line = node.statement.line as u32;
        let extracted_callee = extract_call_callee(text, text);

        // Exclude generic numeric/type casts (`int(`, `parseInt(`) from function-wide sanitizer
        // presence; `TaintAnalyzer` already tracks dominator casts per-variable on `flows`.
        if compiled_lang
            .and_then(|c| c.classify(text).2)
            .is_some_and(|s| !matches!(s, Sanitizer::TypeCast(_)))
            || classify_callee_sanitizer(taint_rules, lang, &extracted_callee)
        {
            has_sanitizer = true;
        }

        let sink_opt = compiled_lang
            .and_then(|c| c.classify(text).1)
            .or_else(|| classify_callee_sink(taint_rules, lang, &extracted_callee));

        let Some(sink_kind) = sink_opt else {
            continue;
        };
        if sink_kind == TaintSink::LogOutput {
            has_audit_log = true;
            continue;
        }

        let mut vuln_src_line: Option<u32> = None;
        let mut sanitizers: Vec<String> = Vec::new();
        for flow in &flows {
            if flow.sink == node_id {
                if flow.is_vulnerable() {
                    let sline = pdg
                        .nodes
                        .get(&flow.source)
                        .map(|n| n.statement.line as u32)
                        .unwrap_or(line);
                    // `flows` order is not stable: report the earliest proven source line.
                    vuln_src_line = Some(vuln_src_line.map_or(sline, |cur| cur.min(sline)));
                } else {
                    sanitizers.extend(flow.sanitizers.iter().map(|s| format!("{s:?}")));
                }
            }
        }

        let status = if let Some(source_line) = vuln_src_line {
            PdgSinkStatus::ProvenUnsanitizedFlow { source_line }
        } else if !sanitizers.is_empty() {
            sanitizers.sort();
            sanitizers.dedup();
            PdgSinkStatus::SanitizedFlow { sanitizers }
        } else {
            PdgSinkStatus::NoLocalSourceFlow
        };

        // Several statements can share a line: keep the strongest evidence
        // (proven > sanitized > no local source), then the lexicographically first callee.
        let key = (line, format!("{sink_kind:?}"));
        let candidate = DiscoveredPdgSink {
            sink_kind,
            callee: extracted_callee,
            line,
            status,
        };
        match by_line.entry(key) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(candidate);
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                let rank = |s: &PdgSinkStatus| match s {
                    PdgSinkStatus::ProvenUnsanitizedFlow { .. } => 0u8,
                    PdgSinkStatus::SanitizedFlow { .. } => 1,
                    _ => 2,
                };
                let cur = e.get();
                if (rank(&candidate.status), candidate.callee.as_str())
                    < (rank(&cur.status), cur.callee.as_str())
                {
                    e.insert(candidate);
                }
            }
        }
    }

    FunctionPdgReport {
        evaluated: true,
        has_sanitizer,
        has_audit_log,
        sinks: by_line.into_values().collect(),
    }
}
