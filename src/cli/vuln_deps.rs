//! `rgctl vuln` and `rgctl deps` — OSV triage, network fetch, local overrides,
//! single-mmap batch scan, transitive stub stitching, and lazy PDG reachability analyze.

use super::context::{CliContext, language_from_path};
use super::{OutputFormat, VulnCommands};
use crate::analysis::{
    EnsureCfgPdgOpts, FunctionLocation, TaintAnalyzer, ensure_cfg_pdg_for_source,
    index_function_locations,
};
use anyhow::{Context, Result};
use rayon::prelude::*;
use rgctl_graph::paths::artifact_path;
use rgctl_security::{
    AdvisorySourceOpts, DepHit, DepStubsIndex, DepsCheckOpts, DepsCheckResult, DepsMatch,
    DepsVerdict, LoadedAdvisory, OsvFetchOpts, SinkCallSite, SinkPathSummary, VulnAnalyzeOpts,
    VulnAnalyzeResult, VulnTriggerRule, analyze_result_to_sarif, analyze_results_to_sarif,
    apply_trigger_rules, build_sink_first_result, build_sink_first_result_with_sites,
    build_vuln_scan_result, deps_check, deps_check_against_inventory, fetch_osv_for_repo,
    find_transitive_stub_paths, load_or_build_dep_stubs, load_vuln_trigger_rules,
    missing_cfg_error_message, resolve_advisories_and_inventory, resolve_package_with_ecosystem,
    rules_with_osv_overlays, triage_osv_path, vuln_analyze,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub fn dispatch_vuln_command(ctx: &CliContext, action: VulnCommands) -> Result<()> {
    match action {
        VulnCommands::Triage { osv } => run_vuln_triage(ctx, osv),
        VulnCommands::Fetch {
            osv_api_url,
            output_dir,
            include_jars,
            include_node_modules,
            sbom,
            timeout_secs,
        } => run_vuln_fetch(
            ctx,
            osv_api_url,
            output_dir,
            include_jars,
            include_node_modules,
            sbom,
            timeout_secs,
        ),
        VulnCommands::Analyze {
            osv,
            from_report,
            sbom,
            offline,
            osv_api_url,
            include_jars,
            include_node_modules,
            with_dep_stubs,
            strict_reachability,
        } => run_vuln_analyze(
            ctx,
            AnalyzeCliArgs {
                osv,
                from_report,
                sbom,
                offline,
                osv_api_url,
                include_jars,
                include_node_modules,
                with_dep_stubs,
                strict_reachability,
            },
        ),
        VulnCommands::Scan {
            osv,
            from_report,
            sbom,
            offline,
            osv_api_url,
            include_jars,
            include_node_modules,
            with_dep_stubs,
            strict_reachability,
        } => run_vuln_scan(
            ctx,
            AnalyzeCliArgs {
                osv,
                from_report,
                sbom,
                offline,
                osv_api_url,
                include_jars,
                include_node_modules,
                with_dep_stubs,
                strict_reachability,
            },
            "vuln scan",
        ),
    }
}

pub struct AnalyzeCliArgs {
    pub osv: Option<PathBuf>,
    pub from_report: Option<PathBuf>,
    pub sbom: Option<PathBuf>,
    pub offline: bool,
    pub osv_api_url: Option<String>,
    pub include_jars: Vec<PathBuf>,
    pub include_node_modules: Vec<PathBuf>,
    pub with_dep_stubs: bool,
    pub strict_reachability: bool,
}

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

pub fn run_vuln_fetch(
    ctx: &CliContext,
    osv_api_url: Option<String>,
    output_dir: Option<PathBuf>,
    include_jars: Vec<PathBuf>,
    include_node_modules: Vec<PathBuf>,
    sbom: Option<PathBuf>,
    timeout_secs: u64,
) -> Result<()> {
    let mut opts = OsvFetchOpts::for_repo(&ctx.repo, osv_api_url, output_dir);
    opts.include_jars = include_jars;
    opts.include_node_modules = include_node_modules;
    opts.sbom = sbom;
    opts.timeout_secs = timeout_secs;

    let summary = fetch_osv_for_repo(&ctx.repo, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&summary).context("serialize")?)?;
    } else {
        ctx.stdout_line(&format!(
            "queried={} matched={} output_dir={}",
            summary.packages_queried, summary.advisories_matched, summary.output_dir
        ))?;
        for id in &summary.advisory_ids {
            ctx.stdout_line(&format!("  {id}"))?;
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
                "  {} {} version={:?} in_range={} direct={} src={}",
                m.ecosystem, m.name, m.version, m.in_range, m.is_direct, m.provenance
            ))?;
        }
    }
    Ok(())
}

struct PreindexedImport {
    name: String,
    qualified_name: Option<String>,
    file: Option<String>,
}

/// Shared mmap & sidecar artifacts opened ONCE across single or Rayon-batch advisory evaluation.
struct SharedVulnGraphResources {
    repo: PathBuf,
    canon_repo: Option<PathBuf>,
    store: Option<Arc<rgctl_graph::SnapshotNodeStore>>,
    parked: Vec<rgctl_extraction::ParkedRelationRecord>,
    /// Pre-grouped indices into `parked` keyed by short callee identifier (`O(1)` lookup per OSV method).
    parked_calls_by_tail: std::collections::HashMap<String, Vec<usize>>,
    /// All `NodeType::Import` rows pre-scanned once without truncation or cold extension bincode touches.
    imports: Vec<PreindexedImport>,
    blast_engine: Option<rgctl_analysis::BlastRadiusEngine>,
    dep_stubs: Option<DepStubsIndex>,
    trigger_rules: Vec<VulnTriggerRule>,
    cfg_archive_exists: bool,
}

fn collect_all_imports(store: &rgctl_graph::SnapshotNodeStore) -> Vec<PreindexedImport> {
    if let Some(col) = store.columnar() {
        let mut out = Vec::new();
        for idx in 0..col.node_count() {
            if col.node_type_at(idx).ok() != Some(rgctl_graph::schema::NodeType::Import) {
                continue;
            }
            let Ok(name) = col.node_name_str(idx) else {
                continue;
            };
            let qn = col
                .node_qualified_name_str(idx)
                .ok()
                .flatten()
                .map(str::to_string);
            let file = col
                .node_file_path_str(idx)
                .ok()
                .flatten()
                .map(str::to_string);
            out.push(PreindexedImport {
                name: name.to_string(),
                qualified_name: qn,
                file,
            });
        }
        return out;
    }
    let q = rgctl_graph::StructuredQuery::new(store);
    let filters = rgctl_graph::QueryFilters {
        node_type: Some(rgctl_graph::schema::NodeType::Import),
        limit: None,
        ..Default::default()
    };
    q.find(None, &filters)
        .map(|found| {
            found
                .entities
                .into_iter()
                .map(|ent| PreindexedImport {
                    name: ent.name,
                    qualified_name: ent.qualified_name,
                    file: ent.file,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn build_parked_calls_index(
    parked: &[rgctl_extraction::ParkedRelationRecord],
) -> std::collections::HashMap<String, Vec<usize>> {
    let mut index: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (idx, rec) in parked.iter().enumerate() {
        if rec.relation_type != rgctl_plugin_api::RelationType::Calls {
            continue;
        }
        let raw = rec.to_name.split('\u{1f}').next().unwrap_or(&rec.to_name);
        let raw_tail = raw.rsplit(['.', ':', '\\', '/']).next().unwrap_or(raw);
        if !raw_tail.is_empty() {
            index.entry(raw_tail.to_string()).or_default().push(idx);
        }
        if let Some(qhint) = rec
            .to_qualified_hint
            .as_deref()
            .map(|h| h.split('\u{1f}').next().unwrap_or(h))
        {
            let q_tail = qhint.rsplit(['.', ':', '\\', '/']).next().unwrap_or(qhint);
            if !q_tail.is_empty() && q_tail != raw_tail {
                index.entry(q_tail.to_string()).or_default().push(idx);
            }
        }
    }
    index
}

impl SharedVulnGraphResources {
    fn open(ctx: &CliContext, args: &AnalyzeCliArgs) -> Result<Self> {
        let store = ctx.open_snapshot_store().ok().flatten();
        let canon_repo = ctx.repo.canonicalize().ok();
        let parked_path = artifact_path(&ctx.repo, rgctl_extraction::PARKED_RELATIONS_FILE);
        let parked = rgctl_extraction::load_parked_relations(&parked_path).unwrap_or_default();
        let parked_calls_by_tail = build_parked_calls_index(&parked);
        let imports = store
            .as_deref()
            .map(collect_all_imports)
            .unwrap_or_default();
        let graph_digest = ctx.graph_digest().ok().flatten().unwrap_or_default();
        let blast_engine = rgctl_analysis::try_load_engine(&ctx.repo, &graph_digest)
            .ok()
            .flatten();
        let dep_stubs = if args.with_dep_stubs {
            Some(
                load_or_build_dep_stubs(&ctx.repo, &args.include_jars, &args.include_node_modules)
                    .map_err(|e| anyhow::anyhow!("{e}"))?,
            )
        } else {
            None
        };
        let trigger_rules =
            load_vuln_trigger_rules(&ctx.repo).map_err(|e| anyhow::anyhow!("{e}"))?;
        let cfg_archive_exists = cfg_archive_present(&ctx.repo);

        Ok(Self {
            repo: ctx.repo.clone(),
            canon_repo,
            store,
            parked,
            parked_calls_by_tail,
            imports,
            blast_engine,
            dep_stubs,
            trigger_rules,
            cfg_archive_exists,
        })
    }
}

/// `rgctl vuln analyze` — single OSV file OR batch (`--osv <dir>`, `--from-report`, `--offline`).
pub fn run_vuln_analyze(ctx: &CliContext, args: AnalyzeCliArgs) -> Result<()> {
    if args.osv.is_none() && args.from_report.is_none() && !args.offline {
        anyhow::bail!(
            "vuln analyze requires `--osv <file|dir>`, `--from-report <path>`, or `--offline` \
             (use `rgctl vuln scan` to auto-fetch OSV advisories from the network)"
        );
    }

    // Batch path when `--osv` points to a directory, or `--from-report` / `--offline` is supplied.
    let is_single_file = args.osv.as_ref().is_some_and(|p| p.is_file())
        && args.from_report.is_none()
        && !args.offline;
    if !is_single_file {
        return run_vuln_scan(ctx, args, "vuln analyze");
    }

    let (advisories, inventory) = resolve_advisories_and_inventory(
        &ctx.repo,
        &AdvisorySourceOpts {
            osv: args.osv.clone(),
            from_report: args.from_report.clone(),
            sbom: args.sbom.clone(),
            offline: args.offline,
            api_url: args.osv_api_url.clone(),
            include_jars: args.include_jars.clone(),
            include_node_modules: args.include_node_modules.clone(),
        },
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let Some(first) = advisories.into_iter().next() else {
        anyhow::bail!("no OSV advisory loaded");
    };

    let shared = SharedVulnGraphResources::open(ctx, &args)?;
    let checked = build_checked_labels(&args);
    let result = evaluate_one_advisory(&shared, &first, &inventory, &checked, &args, true)?;
    emit_analyze(ctx, &result)
}

/// `rgctl vuln scan` (and batch `vuln analyze`) — single-mmap + Rayon parallel evaluation.
pub fn run_vuln_scan(
    ctx: &CliContext,
    args: AnalyzeCliArgs,
    command_label: &'static str,
) -> Result<()> {
    let (advisories, inventory) = resolve_advisories_and_inventory(
        &ctx.repo,
        &AdvisorySourceOpts {
            osv: args.osv.clone(),
            from_report: args.from_report.clone(),
            sbom: args.sbom.clone(),
            offline: args.offline,
            api_url: args.osv_api_url.clone(),
            include_jars: args.include_jars.clone(),
            include_node_modules: args.include_node_modules.clone(),
        },
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let shared = SharedVulnGraphResources::open(ctx, &args)?;
    let checked = build_checked_labels(&args);
    let persist_lazy_cfg = advisories.len() == 1;

    let evaluated: Result<Vec<VulnAnalyzeResult>> = advisories
        .par_iter()
        .map(|adv| {
            evaluate_one_advisory(&shared, adv, &inventory, &checked, &args, persist_lazy_cfg)
        })
        .collect();
    let results = evaluated?;

    let scan = build_vuln_scan_result(command_label, results);
    emit_scan(ctx, &scan)
}

fn build_checked_labels(args: &AnalyzeCliArgs) -> Vec<String> {
    let mut checked = vec!["manifests".to_string(), "lockfiles".to_string()];
    if !args.include_jars.is_empty() {
        checked.push("jars".into());
    }
    if !args.include_node_modules.is_empty() {
        checked.push("node_modules".into());
    }
    if args.sbom.is_some() {
        checked.push("sbom".into());
    }
    if args.from_report.is_some() {
        checked.push("scanner_report".into());
    }
    checked
}

fn evaluate_one_advisory(
    shared: &SharedVulnGraphResources,
    adv: &LoadedAdvisory,
    inventory: &[DepHit],
    checked: &[String],
    args: &AnalyzeCliArgs,
    persist_lazy_cfg: bool,
) -> Result<VulnAnalyzeResult> {
    let mut methods: Vec<String> = Vec::new();
    for pkg in &adv.triage.packages {
        for sym in pkg.symbol_coverage.symbols() {
            if !methods.contains(sym) {
                methods.push(sym.clone());
            }
        }
    }

    let mut deps_preview =
        deps_check_against_inventory(adv.triage.clone(), inventory, checked.to_vec())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

    // If `--with-dep-stubs` found bytecode/source references to the OSV's affected methods inside
    // a shaded/Gradle JAR that lacked `META-INF/maven/pom.xml`, synthesize an affected candidate
    // match so transitive call-stub stitching is not skipped!
    if deps_preview.verdict == DepsVerdict::NotAffected
        && let Some(stubs) = &shared.dep_stubs
        && let Some(shaded_match) = infer_shaded_jar_match(&adv.triage, &methods, stubs)
    {
        deps_preview.matched.push(shaded_match);
        deps_preview.verdict = DepsVerdict::AffectedCandidate;
    }

    let mut opts = VulnAnalyzeOpts {
        include_jars: args.include_jars.clone(),
        include_node_modules: args.include_node_modules.clone(),
        skip_graph: false,
        strict_reachability: args.strict_reachability,
        precomputed_deps: Some(deps_preview.clone()),
        ..Default::default()
    };

    let dummy_path = Path::new(&adv.source_label);
    if deps_preview.verdict == DepsVerdict::NotAffected {
        opts.skip_graph = true;
        let result =
            vuln_analyze(&shared.repo, dummy_path, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(result);
    }

    let signals = graph_package_signals(shared, &deps_preview, &methods, persist_lazy_cfg);
    opts.import_hits = Some(signals.import_hits);
    opts.affected_method_callers = Some(signals.callers.clone());

    let cfg_available = shared.cfg_archive_exists || signals.lazy_cfg_evaluated;
    opts.cfg_available = Some(cfg_available);

    let primary_sink = methods.first().cloned();
    let sink_in_graph = !signals.call_sites.is_empty()
        || primary_sink
            .as_deref()
            .is_some_and(|s| symbol_in_graph(shared, s));

    let sink_taint = primary_sink.as_deref().map(|sink| {
        let mut st = build_sink_first_result_with_sites(
            sink,
            8,
            cfg_available,
            sink_in_graph,
            signals.callers,
            signals.call_sites,
            signals.taint_paths,
        );
        st.caller_chains = signals.caller_chains;
        st.all_sites_mitigated = signals.all_sites_mitigated;
        st
    });
    opts.sink_taint = sink_taint;

    let mut result =
        vuln_analyze(&shared.repo, dummy_path, &opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    apply_trigger_rules(
        &shared.repo,
        &shared.trigger_rules,
        &mut result,
        args.strict_reachability,
    );
    Ok(result)
}

fn infer_shaded_jar_match(
    triage: &rgctl_security::OsvTriage,
    methods: &[String],
    stubs: &DepStubsIndex,
) -> Option<DepsMatch> {
    let pkg = triage.packages.first()?;
    let res = resolve_package_with_ecosystem(Some(&pkg.ecosystem), &pkg.name).ok();
    let prefixes = res.map(|r| r.import_prefixes).unwrap_or_default();

    for stub in &stubs.stubs {
        let hits_method = methods.iter().any(|m| {
            let short = m.rsplit(['.', ':']).next().unwrap_or(m.as_str());
            stub.to_method == short
                && (m.contains(&stub.to_owner)
                    || prefixes
                        .iter()
                        .any(|p| !p.is_empty() && stub.to_owner.contains(p)))
        });
        if hits_method {
            return Some(DepsMatch {
                ecosystem: pkg.ecosystem.clone(),
                name: pkg.name.clone(),
                version: None,
                provenance: format!("dep_stubs:{}", stub.provenance),
                in_range: true,
                is_direct: false,
            });
        }
    }
    None
}

fn emit_analyze(ctx: &CliContext, result: &VulnAnalyzeResult) -> Result<()> {
    match ctx.format {
        OutputFormat::Json => {
            ctx.emit_json_value(&serde_json::to_value(result)?)?;
        }
        OutputFormat::Sarif => {
            let sarif = analyze_result_to_sarif(result);
            ctx.emit(&serde_json::to_string_pretty(&sarif)?)?;
        }
        _ => {
            ctx.stdout_line(&format!(
                "osv={} deps={} exploitability={:?} phases={:?}",
                result.osv_id, result.deps_verdict, result.exploitability, result.phases
            ))?;
            if let Some(notes) = &result.notes {
                ctx.stdout_line(notes)?;
            }
        }
    }
    Ok(())
}

fn emit_scan(ctx: &CliContext, scan: &rgctl_security::VulnScanResult) -> Result<()> {
    match ctx.format {
        OutputFormat::Json => {
            ctx.emit_json_value(&serde_json::to_value(scan)?)?;
        }
        OutputFormat::Sarif => {
            let sarif = analyze_results_to_sarif(&scan.results);
            ctx.emit(&serde_json::to_string_pretty(&sarif)?)?;
        }
        _ => {
            ctx.stdout_line(&format!(
                "total={} exploitable={} under_investigation={} not_exploitable={} not_affected={}",
                scan.counts.total,
                scan.counts.exploitable,
                scan.counts.under_investigation,
                scan.counts.not_exploitable,
                scan.counts.not_affected
            ))?;
            for r in &scan.results {
                ctx.stdout_line(&format!(
                    "  {} exploitability={:?} deps={}",
                    r.osv_id, r.exploitability, r.deps_verdict
                ))?;
            }
        }
    }
    Ok(())
}

fn cfg_archive_present(repo: &Path) -> bool {
    artifact_path(repo, "analysis/cfg_pdg.archive.bin").is_file()
        || artifact_path(repo, "analysis/cfg_pdg.bin").is_file()
}

struct GraphPackageSignals {
    import_hits: usize,
    callers: Vec<String>,
    call_sites: Vec<SinkCallSite>,
    caller_chains: Vec<SinkPathSummary>,
    taint_paths: Vec<SinkPathSummary>,
    all_sites_mitigated: bool,
    lazy_cfg_evaluated: bool,
}

#[derive(Clone)]
struct DiscoveredSite {
    caller_node_id: Option<Uuid>,
    site: SinkCallSite,
    /// Local call expression inside `site.file` (differs from `site.target_method` when stitched via `dep_stubs.bin`).
    local_call_target: String,
    /// Optional transitive chain suffix from wrapper through `dep_stubs.bin` to OSV sink.
    transitive_chain: Option<Vec<String>>,
}

fn relativize_repo_path(repo: &Path, canon_repo: Option<&Path>, raw: &str) -> String {
    let p = Path::new(raw);
    if let Ok(rel) = p.strip_prefix(repo) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    if let Some(canon) = canon_repo
        && let Ok(rel) = p.strip_prefix(canon)
    {
        return rel.to_string_lossy().replace('\\', "/");
    }
    if let Ok(canon_p) = p.canonicalize()
        && let Some(canon) = canon_repo
        && let Ok(rel) = canon_p.strip_prefix(canon)
    {
        return rel.to_string_lossy().replace('\\', "/");
    }
    raw.to_string()
}

fn graph_package_signals(
    shared: &SharedVulnGraphResources,
    deps: &DepsCheckResult,
    methods: &[String],
    persist_lazy_cfg: bool,
) -> GraphPackageSignals {
    let Some(store) = shared.store.as_ref() else {
        return GraphPackageSignals {
            import_hits: 0,
            callers: vec![],
            call_sites: vec![],
            caller_chains: vec![],
            taint_paths: vec![],
            all_sites_mitigated: false,
            lazy_cfg_evaluated: false,
        };
    };
    let q = rgctl_graph::StructuredQuery::new(store.as_ref());
    let mut import_hits = 0usize;
    let mut pkg_prefixes: Vec<String> = Vec::new();
    let mut file_imports: Vec<(String, String)> = Vec::new();

    if let Some(m) = deps
        .matched
        .iter()
        .find(|m| m.in_range)
        .or_else(|| deps.matched.first())
        && let Ok(res) = resolve_package_with_ecosystem(Some(&m.ecosystem), &m.name)
    {
        pkg_prefixes = res.import_prefixes;
        for ent in &shared.imports {
            let qn = ent.qualified_name.as_deref().unwrap_or("");
            let hits_prefix = pkg_prefixes
                .iter()
                .any(|p| !p.is_empty() && (ent.name.contains(p) || qn.contains(p)));
            if hits_prefix {
                import_hits += 1;
                if let Some(f) = &ent.file {
                    let label = if qn.is_empty() {
                        ent.name.clone()
                    } else {
                        format!("{} {}", ent.name, qn)
                    };
                    file_imports.push((f.clone(), label));
                }
            }
        }
    }

    let mut callers = Vec::new();
    let mut seen = HashSet::new();
    let mut discovered_sites: Vec<DiscoveredSite> = Vec::new();

    for m in methods {
        let short = m.rsplit(['.', ':', '\\', '/']).next().unwrap_or(m.as_str());
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
                    let caller_name = n.qualified_name.clone().unwrap_or_else(|| n.name.clone());
                    if seen.insert(caller_name.clone()) {
                        callers.push(caller_name.clone());
                    }
                    if let (Some(f), Some(line)) = (&n.file, n.line) {
                        let rel_file =
                            relativize_repo_path(&shared.repo, shared.canon_repo.as_deref(), f);
                        let site = SinkCallSite {
                            caller: caller_name,
                            target_method: m.clone(),
                            file: rel_file,
                            line: line as u32,
                        };
                        if !discovered_sites.iter().any(|d| d.site == site) {
                            discovered_sites.push(DiscoveredSite {
                                caller_node_id: Uuid::parse_str(&n.id).ok(),
                                local_call_target: m.clone(),
                                transitive_chain: None,
                                site,
                            });
                        }
                    }
                }
            }
        }
    }

    // 1. Direct external calls recorded in `.rgctl/parked_relations.bin` (O(1) short-callee lookup).
    let mut caller_chains = Vec::new();
    for m in methods {
        let sep = if m.contains("::") {
            "::"
        } else if m.contains('\\') {
            "\\"
        } else {
            "."
        };
        let short_method = m.rsplit(sep).next().unwrap_or(m.as_str());
        let Some(indices) = shared.parked_calls_by_tail.get(short_method) else {
            continue;
        };
        for &idx in indices {
            let rec = &shared.parked[idx];
            if parked_call_matches_osv_method(rec, m, &pkg_prefixes, &file_imports) {
                let caller_fqn = store
                    .get_node(rec.from_node_id)
                    .ok()
                    .flatten()
                    .and_then(|n| n.qualified_name.map(|s| s.to_string()))
                    .unwrap_or_else(|| rec.from_qualified_name.clone());
                let rel_file = relativize_repo_path(
                    &shared.repo,
                    shared.canon_repo.as_deref(),
                    &rec.from_file,
                );
                if seen.insert(caller_fqn.clone()) {
                    callers.push(caller_fqn.clone());
                }
                let site = SinkCallSite {
                    caller: caller_fqn.clone(),
                    target_method: m.clone(),
                    file: rel_file.clone(),
                    line: rec.call_site_line,
                };
                if !discovered_sites.iter().any(|d| d.site == site) {
                    let sink_desc = format!("{} ({}:{})", m, rel_file, rec.call_site_line);
                    push_upstream_caller_chains(
                        &q,
                        &shared.repo,
                        shared.canon_repo.as_deref(),
                        &caller_fqn,
                        &sink_desc,
                        &mut caller_chains,
                    );
                    discovered_sites.push(DiscoveredSite {
                        caller_node_id: Some(rec.from_node_id),
                        local_call_target: rec
                            .to_name
                            .split('\u{1f}')
                            .next()
                            .unwrap_or(&rec.to_name)
                            .to_string(),
                        transitive_chain: None,
                        site,
                    });
                }
            }
        }
    }

    // 2. Opt-in transitive stitching across `.rgctl/dep_stubs.bin` (`App -> Wrapper -> VulnSink`).
    if let Some(stubs) = &shared.dep_stubs {
        let stitched = find_transitive_stub_paths(&shared.parked, stubs, methods, 4);
        for hit in stitched {
            let caller_fqn = store
                .get_node(hit.from_node_id)
                .ok()
                .flatten()
                .and_then(|n| n.qualified_name.map(|s| s.to_string()))
                .unwrap_or(hit.caller_fqn);
            let rel_file =
                relativize_repo_path(&shared.repo, shared.canon_repo.as_deref(), &hit.from_file);
            if seen.insert(caller_fqn.clone()) {
                callers.push(caller_fqn.clone());
            }
            let site = SinkCallSite {
                caller: caller_fqn.clone(),
                target_method: hit.target_sink.clone(),
                file: rel_file.clone(),
                line: hit.call_site_line,
            };
            if !discovered_sites.iter().any(|d| d.site == site) {
                let chain_str = hit.stub_chain.join(" -> ");
                let sink_desc = format!("{chain_str} ({rel_file}:{})", hit.call_site_line);
                caller_chains.push(SinkPathSummary {
                    from: format!("{caller_fqn} ({rel_file}:{})", hit.call_site_line),
                    to: sink_desc,
                    hops: Some(hit.stub_chain.len()),
                });
                discovered_sites.push(DiscoveredSite {
                    caller_node_id: Some(hit.from_node_id),
                    local_call_target: hit.wrapper_target,
                    transitive_chain: Some(hit.stub_chain),
                    site,
                });
            }
        }
    }

    // 3. Augment structural caller chains with `BlastRadiusEngine` reverse reachability (depth <= 8).
    if let (Some(engine), Some(first_hit)) = (&shared.blast_engine, discovered_sites.first()) {
        let sink_desc = format!(
            "{} ({}:{})",
            first_hit.site.target_method, first_hit.site.file, first_hit.site.line
        );
        augment_paths_with_blast_radius(
            engine,
            store.as_ref(),
            &discovered_sites,
            &sink_desc,
            8,
            &mut caller_chains,
        );
    }

    // 4. Lazy per-call-site CFG/PDG taint + inline mitigation verification.
    let mut taint_paths = Vec::new();
    let mut site_verdicts = Vec::with_capacity(discovered_sites.len());
    let mut lazy_cfg_evaluated = false;

    for hit in &discovered_sites {
        let local_targets: Vec<String> = if hit.local_call_target != hit.site.target_method {
            let mut v = methods.to_vec();
            v.push(hit.local_call_target.clone());
            v
        } else {
            methods.to_vec()
        };
        let (verdict, evaluated) =
            evaluate_call_site_pdg(&shared.repo, hit, &local_targets, persist_lazy_cfg);
        if evaluated {
            lazy_cfg_evaluated = true;
        }
        if let SiteVerdict::Vulnerable { source_desc } = &verdict {
            let to_desc = match &hit.transitive_chain {
                Some(chain) => format!(
                    "{} ({}:{})",
                    chain.join(" -> "),
                    hit.site.file,
                    hit.site.line
                ),
                None => format!(
                    "{} ({}:{})",
                    hit.site.target_method, hit.site.file, hit.site.line
                ),
            };
            let hops = hit.transitive_chain.as_ref().map_or(1, |chain| chain.len());
            taint_paths.push(SinkPathSummary {
                from: source_desc.clone(),
                to: to_desc,
                hops: Some(hops),
            });
        }
        site_verdicts.push(verdict);
    }

    let all_sites_mitigated = !site_verdicts.is_empty()
        && site_verdicts
            .iter()
            .all(|v| matches!(v, SiteVerdict::Mitigated));
    let call_sites = discovered_sites.into_iter().map(|d| d.site).collect();

    GraphPackageSignals {
        import_hits,
        callers,
        call_sites,
        caller_chains,
        taint_paths,
        all_sites_mitigated,
        lazy_cfg_evaluated,
    }
}

fn push_upstream_caller_chains(
    q: &rgctl_graph::StructuredQuery<'_>,
    repo: &Path,
    canon_repo: Option<&Path>,
    caller_fqn: &str,
    sink_desc: &str,
    caller_chains: &mut Vec<SinkPathSummary>,
) {
    let mut added_upstream = false;
    let caller_short = caller_fqn
        .rsplit(['.', ':', '\\'])
        .next()
        .unwrap_or(caller_fqn);
    for seed in [caller_fqn, caller_short] {
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
                let from_label = match (&up.file, up.line) {
                    (Some(uf), Some(ul)) => {
                        let rel_uf = relativize_repo_path(repo, canon_repo, uf);
                        format!(
                            "{} ({}:{})",
                            up.qualified_name.unwrap_or(up.name),
                            rel_uf,
                            ul
                        )
                    }
                    _ => up.qualified_name.unwrap_or(up.name),
                };
                caller_chains.push(SinkPathSummary {
                    from: from_label,
                    to: sink_desc.to_string(),
                    hops: Some(2),
                });
                added_upstream = true;
            }
            break;
        }
    }
    if !added_upstream {
        caller_chains.push(SinkPathSummary {
            from: caller_fqn.to_string(),
            to: sink_desc.to_string(),
            hops: Some(1),
        });
    }
}

fn parked_call_matches_osv_method(
    rec: &rgctl_extraction::ParkedRelationRecord,
    osv_method: &str,
    pkg_prefixes: &[String],
    file_imports: &[(String, String)],
) -> bool {
    let sep = if osv_method.contains("::") {
        "::"
    } else if osv_method.contains('\\') {
        "\\"
    } else {
        "."
    };
    let mut parts = osv_method.rsplitn(3, sep);
    let Some(short_method) = parts.next() else {
        return false;
    };
    let class_name = parts.next();

    let raw = rec.to_name.split('\u{1f}').next().unwrap_or(&rec.to_name);
    let raw_tail = raw.rsplit(['.', ':', '\\', '/']).next().unwrap_or(raw);
    let qhint = rec
        .to_qualified_hint
        .as_deref()
        .map(|h| h.split('\u{1f}').next().unwrap_or(h));
    let method_matches = raw == short_method
        || raw_tail == short_method
        || raw == osv_method
        || qhint.is_some_and(|h| {
            h == osv_method
                || h.ends_with(&format!(".{short_method}"))
                || h.ends_with(&format!("::{short_method}"))
                || h.ends_with(&format!("\\{short_method}"))
        });
    if !method_matches {
        return false;
    }

    let Some(cls) = class_name else {
        // Bare function/command sink (e.g. Bash CLI tools, C functions, Python imported fns):
        // match when file imports the package, when file is a shell script (CLI commands come from $PATH / SBOM),
        // or when the bare symbol itself starts with the normalized package prefix.
        if pkg_prefixes.is_empty()
            || rec.from_file.ends_with(".sh")
            || rec.from_file.ends_with(".bash")
            || pkg_prefixes.iter().any(|p| {
                let norm = p.replace('-', "_");
                !norm.is_empty() && short_method.starts_with(&norm)
            })
        {
            return true;
        }
        return file_imports.iter().any(|(f, imp)| {
            let same_file =
                f == &rec.from_file || f.ends_with(&rec.from_file) || rec.from_file.ends_with(f);
            same_file
                && pkg_prefixes
                    .iter()
                    .any(|p| !p.is_empty() && imp.contains(p))
        }) || rec
            .import_module_hint
            .as_deref()
            .is_some_and(|i| pkg_prefixes.iter().any(|p| !p.is_empty() && i.contains(p)));
    };

    let cls_tail = cls.rsplit(['.', ':', '\\', '/']).next().unwrap_or(cls);
    let file_imports_cls = file_imports.iter().any(|(f, imp)| {
        let same_file =
            f == &rec.from_file || f.ends_with(&rec.from_file) || rec.from_file.ends_with(f);
        same_file
            && (imp.contains(cls_tail)
                || imp.contains(".*")
                || imp.contains("::*")
                || pkg_prefixes
                    .iter()
                    .any(|p| !p.is_empty() && imp.contains(p)))
    });
    file_imports_cls
        || raw.contains(cls_tail)
        || qhint.is_some_and(|h| h.contains(cls_tail))
        || rec
            .to_type_hint
            .as_deref()
            .is_some_and(|t| t.contains(cls_tail))
        || rec.import_module_hint.as_deref().is_some_and(|i| {
            i.contains(cls_tail) || pkg_prefixes.iter().any(|p| !p.is_empty() && i.contains(p))
        })
}

fn symbol_in_graph(shared: &SharedVulnGraphResources, sink: &str) -> bool {
    let Some(store) = shared.store.as_ref() else {
        return false;
    };
    let q = rgctl_graph::StructuredQuery::new(store.as_ref());
    let short = sink.rsplit(['.', ':', '\\']).next().unwrap_or(sink);
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

/// Augment structural caller chains using pre-loaded `BlastRadiusEngine` reverse reachability.
fn augment_paths_with_blast_radius(
    engine: &rgctl_analysis::BlastRadiusEngine,
    store: &rgctl_graph::SnapshotNodeStore,
    sites: &[DiscoveredSite],
    sink_desc: &str,
    depth: usize,
    caller_chains: &mut Vec<SinkPathSummary>,
) {
    for hit in sites.iter().take(20) {
        let caller_id = hit.caller_node_id.or_else(|| {
            store
                .find_nodes_by_name(&hit.site.caller)
                .ok()
                .and_then(|nodes| nodes.first().map(|n| n.id))
        });
        let Some(caller_id) = caller_id else {
            continue;
        };
        let Ok(result) = engine.analyze(caller_id) else {
            continue;
        };
        for &upstream_id in result.direct_caller_ids.iter().take(depth.min(5)) {
            if let Ok(Some(upstream_node)) = store.get_node(upstream_id) {
                let name = upstream_node
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&upstream_node.name)
                    .to_string();
                if !caller_chains.iter().any(|p| p.from.starts_with(&name)) {
                    caller_chains.push(SinkPathSummary {
                        from: name,
                        to: sink_desc.to_string(),
                        hops: Some(2),
                    });
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SiteVerdict {
    Vulnerable { source_desc: String },
    Mitigated,
    Inconclusive,
}

fn line_start_byte(source: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }
    if line == 1 {
        return Some(0);
    }
    let mut current = 1usize;
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

fn function_containing_line(
    locations: &HashMap<String, FunctionLocation>,
    offset: usize,
) -> Option<(String, FunctionLocation)> {
    locations
        .iter()
        .filter(|(_, loc)| loc.start_byte <= offset && offset < loc.end_byte)
        .min_by_key(|(_, loc)| loc.end_byte - loc.start_byte)
        .map(|(name, loc)| (name.clone(), *loc))
}

/// Evaluate intra-procedural PDG taint and constant/sanitizer mitigation for one call site.
///
/// Returns `(SiteVerdict, cfg_evaluated)` without requiring upfront `discover --with-cfg`.
fn evaluate_call_site_pdg(
    repo: &Path,
    hit: &DiscoveredSite,
    affected_methods: &[String],
    persist_lazy_cfg: bool,
) -> (SiteVerdict, bool) {
    let rel_path = Path::new(&hit.site.file);
    let abs_path = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        repo.join(rel_path)
    };
    let Some(lang_id) = crate::analysis::language_id_from_path(&abs_path) else {
        return (SiteVerdict::Inconclusive, false);
    };
    let lang = language_from_path(&abs_path);
    if lang == "markdown" || lang_id == "hcl" {
        return (SiteVerdict::Inconclusive, false);
    }
    let Ok(source) = std::fs::read_to_string(&abs_path) else {
        return (SiteVerdict::Inconclusive, false);
    };
    let Ok(rules) = rules_with_osv_overlays(Some(std::slice::from_ref(&lang)), affected_methods)
    else {
        return (SiteVerdict::Inconclusive, false);
    };

    // Locate the exact innermost function span containing `site.line` (disambiguates overloads).
    let locations = index_function_locations(&lang, source.as_bytes())
        .ok()
        .map(|(_, locs)| locs);
    let resolved_fn = locations.as_ref().and_then(|locs| {
        line_start_byte(&source, hit.site.line as usize)
            .and_then(|off| function_containing_line(locs, off))
    });

    let fallback_short = hit
        .site
        .caller
        .rsplit(['.', ':', '\\'])
        .next()
        .unwrap_or(&hit.site.caller)
        .to_string();
    let (fn_name, start_byte, end_byte, fn_id) = match resolved_fn {
        Some((name, loc)) => (name, Some(loc.start_byte), Some(loc.end_byte), None),
        None => (fallback_short, None, None, hit.caller_node_id),
    };

    let Ok(record) = ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
        repo_root: repo,
        language: &lang,
        source: &source,
        function_name: &fn_name,
        file_path: Some(&hit.site.file),
        start_byte,
        end_byte,
        function_id: fn_id,
        persist: persist_lazy_cfg,
    }) else {
        return (SiteVerdict::Inconclusive, false);
    };

    let pdg = record.pdg.as_ref();
    let cfg = record.cfg.as_ref();
    let mut analyzer = TaintAnalyzer::new(pdg, cfg);
    analyzer.detect_patterns_with_rules(&lang, &rules);
    let flows = analyzer.analyze();

    let short_method = hit
        .local_call_target
        .rsplit(['.', ':', '\\', '/'])
        .next()
        .unwrap_or(hit.local_call_target.as_str());

    let matches_target = |stmt_line: usize, stmt_text: &str| -> bool {
        stmt_line == hit.site.line as usize
            || (!short_method.is_empty() && stmt_text.contains(short_method))
    };

    // 1. Check flows reaching this specific OSV sink method / call-site line.
    let mut matching_flows = Vec::new();
    for flow in &flows {
        if let Some(sink_node) = pdg.nodes.get(&flow.sink)
            && matches_target(sink_node.statement.line, &sink_node.statement.text)
        {
            matching_flows.push(flow);
        }
    }

    if let Some(vuln_flow) = matching_flows.iter().find(|f| f.is_vulnerable()) {
        let src_desc = pdg
            .nodes
            .get(&vuln_flow.source)
            .map(|n| {
                format!(
                    "{} ({}:{})",
                    hit.site.caller, hit.site.file, n.statement.line
                )
            })
            .unwrap_or_else(|| {
                format!("{} ({}:{})", hit.site.caller, hit.site.file, hit.site.line)
            });
        return (
            SiteVerdict::Vulnerable {
                source_desc: src_desc,
            },
            true,
        );
    }

    // All taint flows reaching this sink were dominated by a sanitizer!
    if !matching_flows.is_empty() && matching_flows.iter().all(|f| !f.is_vulnerable()) {
        return (SiteVerdict::Mitigated, true);
    }

    // 2. Check whether the call expression on `site.line` passes strictly constant-literal arguments.
    let line_text = source
        .lines()
        .nth((hit.site.line as usize).saturating_sub(1))
        .unwrap_or("");
    if is_constant_literal_call(line_text, short_method) {
        return (SiteVerdict::Mitigated, true);
    }

    (SiteVerdict::Inconclusive, true)
}

/// Returns `true` iff `line_text` invokes `short_method` with strictly compile-time constant
/// literals (uninterpolated strings, numbers, booleans, atoms, `null`/`None`/`nil`, or `.class`
/// literals) and zero variable/call expressions.
fn is_constant_literal_call(line_text: &str, short_method: &str) -> bool {
    if short_method.is_empty() {
        return false;
    }
    let needle = format!("{short_method}(");
    if let Some(start_idx) = line_text.find(&needle) {
        let after_open = &line_text[start_idx + needle.len()..];
        return check_paren_constant_args(after_open);
    }

    // Shell / Ruby / Puppet command-style invocation without parens: `short_method 'const'`
    let trimmed = line_text.trim();
    if let Some(rest) = trimmed.strip_prefix(short_method)
        && rest.starts_with(char::is_whitespace)
    {
        let args = rest.trim();
        if args.is_empty()
            || args.contains('$')
            || args.contains('`')
            || args.contains('(')
            || args.contains('|')
            || args.contains(';')
        {
            return false;
        }
        return check_paren_constant_args(&format!("{args})"));
    }

    false
}

fn check_paren_constant_args(after_open: &str) -> bool {
    let mut stripped = String::new();
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    let mut found_close = false;
    let mut chars = after_open.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(q) = in_quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            // Reject double-quote interpolation: `${x}`, `$x` (PHP/Bash/Dart/Kotlin/Groovy/Puppet),
            // and `#{x}` (Ruby/Elixir).
            if q == '"' {
                if ch == '$'
                    && chars
                        .peek()
                        .is_some_and(|n| *n == '{' || *n == '_' || n.is_ascii_alphanumeric())
                {
                    return false;
                }
                if ch == '#' && chars.peek() == Some(&'{') {
                    return false;
                }
            }
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_quote = Some(ch);
            }
            '`' => return false,
            '(' => return false,
            ')' => {
                found_close = true;
                break;
            }
            _ => stripped.push(ch),
        }
    }

    if !found_close || in_quote.is_some() {
        return false;
    }

    for arg in stripped.split(',') {
        let tok = arg.trim();
        if tok.is_empty() {
            continue;
        }
        let is_bool_or_null = matches!(
            tok,
            "true" | "false" | "True" | "False" | "null" | "None" | "nil" | "NULL" | "undefined"
        );
        if is_bool_or_null {
            continue;
        }
        // Ruby / Elixir symbol or atom literal (`:safe_mode`).
        if let Some(atom) = tok.strip_prefix(':')
            && !atom.is_empty()
            && atom.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        // Numeric literal (`42`, `-3.14`, `0x10`, `1_000L`).
        let num_candidate = tok.trim_start_matches(['+', '-']);
        if !num_candidate.is_empty()
            && num_candidate
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            && num_candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        {
            continue;
        }
        // Java/Kotlin class literal (`Health.class`, `Health::class`, `Health::class.java`).
        if let Some(type_part) = tok
            .strip_suffix(".class")
            .or_else(|| tok.strip_suffix("::class.java"))
            .or_else(|| tok.strip_suffix("::class"))
            && !type_part.is_empty()
            && type_part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '$')
        {
            continue;
        }
        return false;
    }

    true
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
    let store_opt = ctx.open_snapshot_store().ok().flatten();
    let short = sink
        .rsplit(['.', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(sink.as_str());
    let mut sink_in_graph = false;
    let mut callers = Vec::new();
    let mut seen = HashSet::new();

    if let Some(store) = store_opt.as_ref() {
        let q = rgctl_graph::StructuredQuery::new(store.as_ref());
        for seed in [sink.as_str(), short] {
            if let Ok(found) = q.find(
                Some(seed),
                &rgctl_graph::QueryFilters {
                    exact: true,
                    limit: Some(5),
                    ..Default::default()
                },
            ) && found.total > 0
            {
                sink_in_graph = true;
            }
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

    // Load discover-time taint flows recorded under `.rgctl/analysis/` when `--with-taint` ran.
    let mut path_summaries = Vec::new();
    let analysis_dir = ctx.repo.join(".rgctl/analysis");
    if analysis_dir.is_dir() {
        let storage = rgctl_analysis::AnalysisStorage::new(&analysis_dir);
        if let Ok(index) = storage.load_analysis_index() {
            let mut entries: Vec<_> = index.values().filter(|e| e.flow_count > 0).collect();
            entries.sort_by(|a, b| a.stable_key.cmp(&b.stable_key));
            for entry in entries {
                let Ok(Some(analysis)) = storage.load_function(entry.function_id) else {
                    continue;
                };
                let Some(flows) = analysis.taint.as_ref() else {
                    continue;
                };
                let pdg = analysis.pdg.as_deref();
                for flow in flows {
                    if !flow.is_vulnerable() {
                        continue;
                    }
                    let sink_stmt = pdg
                        .and_then(|p| p.nodes.get(&flow.sink))
                        .map(|n| (n.statement.line, n.statement.text.as_str()));
                    let src_stmt = pdg
                        .and_then(|p| p.nodes.get(&flow.source))
                        .map(|n| (n.statement.line, n.statement.text.as_str()));
                    let sink_text = sink_stmt.map(|(_, t)| t).unwrap_or("");
                    let matches_sink = (!sink.is_empty() && sink_text.contains(sink.as_str()))
                        || (!short.is_empty() && sink_text.contains(short))
                        || analysis.function_name == sink
                        || analysis.function_name == short;
                    if !matches_sink {
                        continue;
                    }
                    sink_in_graph = true;
                    if seen.insert(analysis.function_name.clone()) {
                        callers.push(analysis.function_name.clone());
                    }
                    let from_desc = match src_stmt {
                        Some((line, text)) => format!(
                            "{}:{} ({:?}: {})",
                            analysis.function_name,
                            line,
                            flow.source_type,
                            text.trim()
                        ),
                        None => format!("{} ({:?})", analysis.function_name, flow.source_type),
                    };
                    let to_desc = match sink_stmt {
                        Some((line, text)) => format!(
                            "{}:{} ({:?}: {})",
                            analysis.function_name,
                            line,
                            flow.sink_type,
                            text.trim()
                        ),
                        None => format!("{} ({:?})", analysis.function_name, flow.sink_type),
                    };
                    path_summaries.push(rgctl_security::SinkPathSummary {
                        from: from_desc,
                        to: to_desc,
                        hops: Some(flow.path.len()),
                    });
                }
            }
        }
    }

    let _ = rgctl_security::rules_with_osv_overlays(None, std::slice::from_ref(&sink))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let result = build_sink_first_result(
        &sink,
        depth,
        cfg_available,
        sink_in_graph,
        callers,
        path_summaries,
    );
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
