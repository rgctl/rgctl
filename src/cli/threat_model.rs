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
use rgctl_analysis::{AnalysisResults, TaintSink};
use rgctl_graph::paths::artifact_path;
use rgctl_security::{
    DiscoveredPdgSink, FunctionPdgReport, FunctionPdgRequest, OSV_CACHE_REL, PdgSinkStatus,
    ThreatModelBuildOpts, build_threat_model, classify_callee_sanitizer, classify_callee_sink,
    render_threat_model_markdown, render_threat_model_mermaid,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

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

struct CachedFileSource {
    source: String,
    lang: String,
    locations: Option<HashMap<String, FunctionLocation>>,
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

    let parked_path = artifact_path(&ctx.repo, rgctl_extraction::PARKED_RELATIONS_FILE);
    let parked_relations =
        rgctl_extraction::load_parked_relations(&parked_path).unwrap_or_default();

    let analysis_path = artifact_path(&ctx.repo, "analysis_results.bin");
    let analysis = AnalysisResults::load(&analysis_path).ok();
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

    let mut file_cache: HashMap<String, Option<CachedFileSource>> = HashMap::new();
    let mut fn_cache: HashMap<(String, String), FunctionPdgReport> = HashMap::new();

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
        },
        |req: FunctionPdgRequest<'_>| {
            if args.no_lazy_cfg {
                return FunctionPdgReport::default();
            }
            inspect_function_pdg_lazy(&ctx.repo, taint_rules, &mut file_cache, &mut fn_cache, &req)
        },
    )?;

    match ctx.format {
        OutputFormat::Json => ctx.emit_serializable(&report),
        OutputFormat::Mermaid => ctx.emit(&render_threat_model_mermaid(&report)),
        OutputFormat::Sarif => bail!(
            "`rgctl vuln threat-model` does not emit SARIF (use `-f json`, `-f text`, or `-f mermaid`; use `rgctl -f sarif vuln scan` for SARIF 2.1.0)"
        ),
        OutputFormat::Text | OutputFormat::Graphviz => {
            ctx.emit(&render_threat_model_markdown(&report))
        }
    }
}

fn inspect_function_pdg_lazy(
    repo_root: &Path,
    taint_rules: &TaintRuleSet,
    file_cache: &mut HashMap<String, Option<CachedFileSource>>,
    fn_cache: &mut HashMap<(String, String), FunctionPdgReport>,
    req: &FunctionPdgRequest<'_>,
) -> FunctionPdgReport {
    let fn_key = (req.file.to_string(), req.function_fqn.to_string());
    if let Some(existing) = fn_cache.get(&fn_key) {
        return existing.clone();
    }

    let cached_file = file_cache.entry(req.file.to_string()).or_insert_with(|| {
        let rel_path = Path::new(req.file);
        let abs_path = if rel_path.is_absolute() {
            rel_path.to_path_buf()
        } else {
            repo_root.join(rel_path)
        };
        let lang_id = crate::analysis::language_id_from_path(&abs_path)?;
        let lang = language_from_path(&abs_path);
        if lang == "markdown" || lang_id == "hcl" {
            return None;
        }
        let source = std::fs::read_to_string(&abs_path).ok()?;
        let locations = index_function_locations(&lang, source.as_bytes())
            .ok()
            .map(|(_, locs)| locs);
        Some(CachedFileSource {
            source,
            lang,
            locations,
        })
    });

    let Some(file_entry) = cached_file.as_ref() else {
        let empty = FunctionPdgReport::default();
        fn_cache.insert(fn_key, empty.clone());
        return empty;
    };

    let resolved_fn = file_entry.locations.as_ref().and_then(|locs| {
        // Probe `start_line` and `start_line + 1` (accounts for decorator line vs `def`/`fn` line).
        line_start_byte(&file_entry.source, req.start_line as usize)
            .and_then(|off| function_containing_line(locs, off))
            .or_else(|| {
                line_start_byte(
                    &file_entry.source,
                    req.start_line.saturating_add(1) as usize,
                )
                .and_then(|off| function_containing_line(locs, off))
            })
    });
    let fallback_short = req
        .function_fqn
        .rsplit(['.', ':', '\\'])
        .next()
        .unwrap_or(req.function_fqn)
        .to_string();
    let (fn_name, start_byte, end_byte, fn_id) = match resolved_fn {
        Some((name, loc)) => (name, Some(loc.start_byte), Some(loc.end_byte), None),
        None => (fallback_short, None, None, Some(req.function_id)),
    };

    let Ok(record) = ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
        repo_root,
        language: &file_entry.lang,
        source: &file_entry.source,
        function_name: &fn_name,
        file_path: Some(req.file),
        start_byte,
        end_byte,
        function_id: fn_id,
        persist: false,
    }) else {
        let empty = FunctionPdgReport::default();
        fn_cache.insert(fn_key, empty.clone());
        return empty;
    };

    let pdg = record.pdg.as_ref();
    let cfg = record.cfg.as_ref();
    let mut analyzer = TaintAnalyzer::new(pdg, cfg);
    analyzer.detect_patterns_with_rules(&file_entry.lang, taint_rules);
    let flows = analyzer.analyze();

    let compiled_lang = taint_rules.for_language(&file_entry.lang);
    let mut has_sanitizer = false;
    let mut has_audit_log = false;
    let mut by_line: BTreeMap<(u32, String), DiscoveredPdgSink> = BTreeMap::new();

    for (&node_id, node) in &pdg.nodes {
        let text = node.statement.text.trim();
        if text.is_empty() {
            continue;
        }
        let line = node.statement.line as u32;
        let extracted_callee = extract_call_callee(text, text);

        if compiled_lang.and_then(|c| c.classify(text).2).is_some()
            || classify_callee_sanitizer(taint_rules, &file_entry.lang, &extracted_callee)
        {
            has_sanitizer = true;
        }

        let sink_opt = compiled_lang
            .and_then(|c| c.classify(text).1)
            .or_else(|| classify_callee_sink(taint_rules, &file_entry.lang, &extracted_callee));

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
                    vuln_src_line = Some(sline);
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

        let key = (line, format!("{sink_kind:?}"));
        by_line.insert(
            key,
            DiscoveredPdgSink {
                sink_kind,
                callee: extracted_callee,
                line,
                status,
            },
        );
    }

    let report = FunctionPdgReport {
        evaluated: true,
        has_sanitizer,
        has_audit_log,
        sinks: by_line.into_values().collect(),
    };
    fn_cache.insert(fn_key, report.clone());
    report
}
