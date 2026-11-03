//! `rgctl vuln` and `rgctl deps` — OSV triage, deps match, and reachability analyze.

use super::OutputFormat;
use super::context::CliContext;
use anyhow::{Context, Result};
use rgctl_graph::paths::artifact_path;
use rgctl_security::{
    DepsCheckOpts, DepsVerdict, SinkCallSite, SinkPathSummary, VulnAnalyzeOpts,
    affected_methods_from_osv_json, build_sink_first_result, build_sink_first_result_with_sites,
    deps_check, missing_cfg_error_message, resolve_package, triage_osv_path, vuln_analyze,
};
use std::path::{Path, PathBuf};

pub fn run_vuln_triage(ctx: &CliContext, osv: PathBuf) -> Result<()> {
    let triage = triage_osv_path(&osv).map_err(|e| anyhow::anyhow!("{e}"))?;
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&triage).context("serialize")?)?;
    } else {
        ctx.stdout_line(&format!(
            "id={} packages={} versions_array_not_authoritative={}",
            triage.id,
            triage.packages.len(),
            triage.versions_array_not_authoritative
        ))?;
        for p in &triage.packages {
            ctx.stdout_line(&format!(
                "  {} {} introduced={:?} fixed={:?}",
                p.ecosystem, p.name, p.introduced, p.fixed
            ))?;
        }
    }
    Ok(())
}

pub fn run_deps_check(
    ctx: &CliContext,
    osv: PathBuf,
    include_jars: Vec<PathBuf>,
    include_node_modules: Vec<PathBuf>,
) -> Result<()> {
    let opts = DepsCheckOpts {
        include_jars,
        include_node_modules,
        ..Default::default()
    };
    let result = deps_check(&ctx.repo, &osv, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&result).context("serialize")?)?;
    } else {
        let verdict = match result.verdict {
            DepsVerdict::NotAffected => "not_affected",
            DepsVerdict::AffectedCandidate => "affected_candidate",
        };
        ctx.stdout_line(&format!(
            "verdict={verdict} osv_id={} checked={:?} matched={}",
            result.osv_id,
            result.checked,
            result.matched.len()
        ))?;
        for m in &result.matched {
            ctx.stdout_line(&format!(
                "  {} {} version={:?} in_range={} src={}",
                m.ecosystem, m.name, m.version, m.in_range, m.provenance
            ))?;
        }
    }
    Ok(())
}

/// `rgctl vuln analyze --osv …`
pub fn run_vuln_analyze(
    ctx: &CliContext,
    osv: PathBuf,
    include_jars: Vec<PathBuf>,
    include_node_modules: Vec<PathBuf>,
) -> Result<()> {
    let osv_bytes = std::fs::read(&osv).with_context(|| format!("read {}", osv.display()))?;
    let methods = affected_methods_from_osv_json(&osv_bytes);

    // Fast deps-only path first (also used for early exit).
    let deps_preview = deps_check(
        &ctx.repo,
        &osv,
        &DepsCheckOpts {
            include_jars: include_jars.clone(),
            include_node_modules: include_node_modules.clone(),
            ..Default::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut opts = VulnAnalyzeOpts {
        include_jars,
        include_node_modules,
        skip_graph: false,
        ..Default::default()
    };

    if deps_preview.verdict == DepsVerdict::NotAffected {
        opts.skip_graph = true;
        let result = vuln_analyze(&ctx.repo, &osv, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
        return emit_analyze(ctx, &result);
    }

    // Package-aware import count + discovered call sites (from graph + parked_relations.bin).
    let (import_hits, callers, call_sites, path_summaries) =
        graph_package_signals(ctx, &deps_preview, &methods);
    opts.import_hits = Some(import_hits);
    opts.affected_method_callers = Some(callers.clone());

    let cfg_available = cfg_archive_present(&ctx.repo);
    opts.cfg_available = Some(cfg_available);

    let primary_sink = methods
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown_sink".into());
    let sink_in_graph = !call_sites.is_empty() || symbol_in_graph(ctx, &primary_sink);
    let sink_taint = build_sink_first_result_with_sites(
        &primary_sink,
        8,
        cfg_available,
        sink_in_graph,
        callers,
        call_sites,
        path_summaries,
    );
    opts.sink_taint = Some(sink_taint);

    let result = vuln_analyze(&ctx.repo, &osv, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    emit_analyze(ctx, &result)
}

fn emit_analyze(ctx: &CliContext, result: &rgctl_security::VulnAnalyzeResult) -> Result<()> {
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(result)?)?;
    } else {
        ctx.stdout_line(&format!(
            "osv={} deps={} exploitability={:?} phases={:?}",
            result.osv_id, result.deps_verdict, result.exploitability, result.phases
        ))?;
        if let Some(notes) = &result.notes {
            ctx.stdout_line(notes)?;
        }
    }
    Ok(())
}

fn cfg_archive_present(repo: &Path) -> bool {
    artifact_path(repo, "analysis/cfg_pdg.archive.bin").is_file()
        || artifact_path(repo, "analysis/cfg_pdg.bin").is_file()
        || artifact_path(repo, "analysis")
            .join("cfg_pdg.archive.bin")
            .is_file()
        || artifact_path(repo, "analysis")
            .join("cfg_pdg.bin")
            .is_file()
}

fn graph_package_signals(
    ctx: &CliContext,
    deps: &rgctl_security::DepsCheckResult,
    methods: &[String],
) -> (usize, Vec<String>, Vec<SinkCallSite>, Vec<SinkPathSummary>) {
    let Ok(Some(store)) = ctx.open_snapshot_store() else {
        return (0, vec![], vec![], vec![]);
    };
    let q = rgctl_graph::StructuredQuery::new(store.as_ref());
    let mut import_hits = 0usize;
    let mut pkg_prefix: Option<String> = None;
    let mut file_imports: Vec<(String, String)> = Vec::new();
    if let Some(m) = deps.matched.first()
        && let Ok(res) = resolve_package(&m.name)
        && let Some(prefix) = res.import_prefixes.first()
    {
        pkg_prefix = Some(prefix.clone());
        let filters = rgctl_graph::QueryFilters {
            node_type: Some(rgctl_graph::schema::NodeType::Import),
            limit: Some(200),
            ..Default::default()
        };
        if let Ok(found) = q.find(Some(&format!("*{prefix}*")), &filters) {
            import_hits = found.total;
            for ent in found.entities {
                if let Some(f) = ent.file {
                    file_imports.push((f, ent.name));
                }
            }
        }
    }
    let mut callers = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in methods {
        let short = m.rsplit('.').next().unwrap_or(m.as_str());
        for seed in [m.as_str(), short] {
            if let Ok(nb) = q.call_neighbors(
                seed,
                true,
                1,
                &rgctl_graph::QueryFilters {
                    limit: Some(50),
                    ..Default::default()
                },
            ) {
                for n in nb.neighbors {
                    if seen.insert(n.name.clone()) {
                        callers.push(n.name);
                    }
                }
            }
        }
    }

    // Also inspect `.rgctl/parked_relations.bin` for external library calls recorded during discover.
    let mut call_sites = Vec::new();
    let mut path_summaries = Vec::new();
    let parked_path = artifact_path(&ctx.repo, rgctl_extraction::PARKED_RELATIONS_FILE);
    if let Ok(parked) = rgctl_extraction::load_parked_relations(&parked_path) {
        for rec in &parked {
            if rec.relation_type != rgctl_plugin_api::RelationType::Calls {
                continue;
            }
            for m in methods {
                if parked_call_matches_osv_method(rec, m, pkg_prefix.as_deref(), &file_imports) {
                    let caller_fqn = store
                        .get_node(rec.from_node_id)
                        .ok()
                        .flatten()
                        .and_then(|n| n.qualified_name.map(|s| s.to_string()))
                        .unwrap_or_else(|| rec.from_qualified_name.clone());
                    let rel_file = rec
                        .from_file
                        .strip_prefix(&format!("{}/", ctx.repo.display()))
                        .unwrap_or(&rec.from_file)
                        .to_string();
                    if seen.insert(caller_fqn.clone()) {
                        callers.push(caller_fqn.clone());
                    }
                    let site = SinkCallSite {
                        caller: caller_fqn.clone(),
                        target_method: m.clone(),
                        file: rel_file.clone(),
                        line: rec.call_site_line,
                    };
                    if !call_sites.contains(&site) {
                        let sink_desc = format!("{} ({}:{})", m, rel_file, rec.call_site_line);
                        // Walk callers of this function in the structural graph for end-to-end paths.
                        let mut added_upstream = false;
                        let caller_short = caller_fqn.rsplit('.').next().unwrap_or(&caller_fqn);
                        for seed in [caller_fqn.as_str(), caller_short] {
                            if let Ok(nb) = q.call_neighbors(
                                seed,
                                true,
                                1,
                                &rgctl_graph::QueryFilters {
                                    limit: Some(10),
                                    ..Default::default()
                                },
                            ) && !nb.neighbors.is_empty()
                            {
                                for up in nb.neighbors {
                                    path_summaries.push(SinkPathSummary {
                                        from: up.qualified_name.unwrap_or(up.name),
                                        to: sink_desc.clone(),
                                        hops: Some(2),
                                    });
                                    added_upstream = true;
                                }
                                break;
                            }
                        }
                        if !added_upstream {
                            path_summaries.push(SinkPathSummary {
                                from: caller_fqn.clone(),
                                to: sink_desc,
                                hops: Some(1),
                            });
                        }
                        call_sites.push(site);
                    }
                }
            }
        }
    }

    (import_hits, callers, call_sites, path_summaries)
}

fn parked_call_matches_osv_method(
    rec: &rgctl_extraction::ParkedRelationRecord,
    osv_method: &str,
    pkg_prefix: Option<&str>,
    file_imports: &[(String, String)],
) -> bool {
    let mut parts = osv_method.rsplitn(3, '.');
    let Some(short_method) = parts.next() else {
        return false;
    };
    let class_name = parts.next();

    let raw = rec.to_name.as_str();
    let method_matches = raw == short_method
        || raw == osv_method
        || raw.ends_with(&format!(".{short_method}"))
        || rec
            .to_qualified_hint
            .as_deref()
            .is_some_and(|h| h == osv_method || h.ends_with(&format!(".{short_method}")));
    if !method_matches {
        return false;
    }

    // Confirm class or package hint (or file-level import of the vulnerable class/wildcard).
    let Some(cls) = class_name else {
        return true;
    };
    let file_imports_cls = file_imports.iter().any(|(f, imp)| {
        let same_file =
            f == &rec.from_file || f.ends_with(&rec.from_file) || rec.from_file.ends_with(f);
        same_file && (imp.contains(cls) || imp.contains(".*"))
    });
    file_imports_cls
        || raw.contains(cls)
        || rec
            .to_qualified_hint
            .as_deref()
            .is_some_and(|h| h.contains(cls))
        || rec.to_type_hint.as_deref().is_some_and(|t| t.contains(cls))
        || rec
            .import_module_hint
            .as_deref()
            .is_some_and(|i| i.contains(cls) || pkg_prefix.is_some_and(|p| i.contains(p)))
}

fn symbol_in_graph(ctx: &CliContext, sink: &str) -> bool {
    let Ok(Some(store)) = ctx.open_snapshot_store() else {
        return false;
    };
    let q = rgctl_graph::StructuredQuery::new(store.as_ref());
    let short = sink.rsplit('.').next().unwrap_or(sink);
    for seed in [sink, short] {
        if let Ok(found) = q.find(
            Some(seed),
            &rgctl_graph::QueryFilters {
                exact: true,
                limit: Some(5),
                ..Default::default()
            },
        ) && found.total > 0
        {
            return true;
        }
    }
    false
}

/// `rgctl taint --sink … --source external`
pub fn run_sink_taint(ctx: &CliContext, sink: String, source: String, depth: usize) -> Result<()> {
    if source != "external" {
        anyhow::bail!("only --source external is supported in v1 (got {source})");
    }
    let cfg_available = cfg_archive_present(&ctx.repo);
    if !cfg_available {
        anyhow::bail!("{}", missing_cfg_error_message());
    }
    let sink_in_graph = symbol_in_graph(ctx, &sink);
    let mut callers = Vec::new();
    if let Ok(Some(store)) = ctx.open_snapshot_store() {
        let q = rgctl_graph::StructuredQuery::new(store.as_ref());
        let short = sink.rsplit('.').next().unwrap_or(sink.as_str());
        let mut seen = std::collections::HashSet::new();
        for seed in [sink.as_str(), short] {
            if let Ok(nb) = q.call_neighbors(
                seed,
                true,
                depth.max(1),
                &rgctl_graph::QueryFilters {
                    limit: Some(50),
                    ..Default::default()
                },
            ) {
                for n in nb.neighbors {
                    if seen.insert(n.name.clone()) {
                        callers.push(n.name);
                    }
                }
            }
        }
    }
    // Ensure overlays compile (declarative engine).
    let _ = rgctl_security::rules_with_osv_overlays(None, std::slice::from_ref(&sink))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let result =
        build_sink_first_result(&sink, depth, cfg_available, sink_in_graph, callers, vec![]);
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&result)?)?;
    } else {
        ctx.stdout_line(&format!(
            "sink={} resolution={:?} callers={} paths={}",
            result.sink,
            result.sink_resolution,
            result.sink_callers.len(),
            result.paths.len()
        ))?;
        if let Some(h) = &result.honesty {
            ctx.stdout_line(h)?;
        }
    }
    Ok(())
}
