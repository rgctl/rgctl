//! `rgctl rules` — Kantra evaluation and findings queries against the session.

use super::OutputFormat;
use super::context::CliContext;
use super::kantra_discover::{
    preload_discovered_sources, resolve_kantra_catalog, run_kantra_index, run_kantra_violates,
};
use super::stage_profile::DiscoverStageReport;
use anyhow::{Context, Result, bail};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_graph::snapshot::SNAPSHOT_FILE;
use rgctl_kantra::{
    EvalContext, EvalEdge, EvalGraph, EvalNode, KantraCatalog, KantraEngine, KantraFileCache,
    KantraFindings, KantraViolation, ViolationResolver, cache_dir, rule_konveyor_sources,
    rule_konveyor_targets, ruleset_hash,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// `rgctl rules run [DIR] [--target] [--catalog]`.
///
/// When `DIR` and `--catalog` are omitted, evaluates the embedded Kantra catalog.
pub fn run_rules(
    ctx: &CliContext,
    rules_dir: Option<PathBuf>,
    target: Option<String>,
    catalog: Option<PathBuf>,
) -> Result<()> {
    if let Some(ref dir) = rules_dir
        && catalog.is_none()
        && !dir.is_dir()
    {
        bail!("rules path is not a directory: {}", dir.display());
    }
    let store = &ctx.repo;
    let snapshot = rgctl_graph::paths::artifact_path(store, SNAPSHOT_FILE);
    if !snapshot.exists() {
        bail!("Graph snapshot not found (run `rgctl discover` first)");
    }

    let rules_opt = if catalog.is_some() {
        None
    } else {
        rules_dir.as_deref()
    };
    let catalog_opt = catalog.as_deref();

    let mut profile = DiscoverStageReport::default();
    run_kantra_index(store, rules_opt, catalog_opt, &mut profile)?;

    let catalog = resolve_kantra_catalog(rules_opt, catalog_opt)?;
    let (engine, _) = KantraEngine::from_catalog(catalog.clone(), target.as_deref())
        .map_err(|e| anyhow::anyhow!("kantra catalog: {e}"))?;

    let files = collect_source_files(&ctx.repo)?;
    let sources = preload_discovered_sources(&ctx.repo, &files, None);
    let graph = build_eval_graph_from_snapshot(&snapshot)?;
    let rs_hash = ruleset_hash(
        engine.catalog_id().unwrap_or("unknown"),
        catalog.rules.len(),
    );
    let mut file_cache = KantraFileCache::load(&cache_dir(store), &rs_hash);
    let mut eval_ctx = EvalContext {
        repo_root: &ctx.repo,
        files: &files,
        sources: &sources,
        graph: &graph,
        cache: Some(&mut file_cache),
        cached_files: HashSet::new(),
    };
    let (mut findings, _) = engine
        .evaluate(&mut eval_ctx)
        .map_err(|e| anyhow::anyhow!("kantra evaluate: {e}"))?;
    let resolver = ViolationResolver::from_eval_nodes(&graph.nodes);
    resolver.attach_node_ids(&mut findings.violations);
    findings.command = "rules_run".into();
    file_cache.save(&cache_dir(store))?;

    let out_path = rgctl_graph::paths::artifact_path(store, "kantra_findings.json");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&findings).context("serialize kantra findings")?;
    fs::write(&out_path, &json).with_context(|| format!("write {}", out_path.display()))?;

    // Best-effort VIOLATES edges (snapshot rewrite).
    let _ = run_kantra_violates(store, &mut profile);

    let all_refs: Vec<&KantraViolation> = findings.violations.iter().collect();
    let summary = findings_summary_refs(&all_refs);
    let per_rule = build_per_rule_summary(&all_refs);

    if ctx.format == OutputFormat::Json {
        let compact_violations: Vec<Value> = all_refs
            .iter()
            .take(50)
            .map(|v| compact_violation_json(v))
            .collect();
        let mut obj = Map::new();
        obj.insert("schema_version".into(), json!(findings.schema_version));
        obj.insert("command".into(), json!("rules_run"));
        if let Some(ref cid) = findings.catalog_id {
            obj.insert("catalog_id".into(), json!(cid));
        }
        obj.insert("ruleset".into(), json!(findings.ruleset));
        if let Some(ref tf) = findings.target_filter {
            obj.insert("target_filter".into(), json!(tf));
        }
        obj.insert("evaluated_rules".into(), json!(findings.evaluated_rules));
        obj.insert(
            "skipped_rules_count".into(),
            json!(findings.skipped_rules.len()),
        );
        obj.insert(
            "findings_path".into(),
            Value::String(out_path.display().to_string()),
        );
        obj.insert("summary".into(), summary);
        obj.insert("per_rule".into(), Value::Array(per_rule));
        obj.insert(
            "returned_violations".into(),
            json!(compact_violations.len()),
        );
        obj.insert("violations".into(), Value::Array(compact_violations));
        ctx.emit_json_value(&Value::Object(obj))?;
    } else {
        ctx.stdout_line(&format!(
            "rules: {} violations ({} rules, {} files), {} skipped → {}",
            findings.violations.len(),
            summary
                .get("unique_rules")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            summary
                .get("unique_files")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            findings.skipped_rules.len(),
            out_path.display()
        ))?;
    }
    Ok(())
}

/// `rgctl rules findings [--target] [--source] [--category] [--file] [--limit]`.
pub fn run_findings(
    ctx: &CliContext,
    target: Option<String>,
    source: Option<String>,
    category: Option<String>,
    file: Option<String>,
    limit: Option<usize>,
) -> Result<()> {
    let path = rgctl_graph::paths::artifact_path(&ctx.repo, "kantra_findings.json");
    if !path.exists() {
        bail!(
            "kantra findings not found at {} (run `rgctl discover --with-kantra` or `rgctl rules run` first)",
            path.display()
        );
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let findings = KantraFindings::from_json(&bytes).context("parse kantra_findings.json")?;

    let target_filters = split_csv(target.as_deref());
    let source_filters = split_csv(source.as_deref());
    let category_lc = category.as_deref().map(|s| s.to_ascii_lowercase());
    let file_substr = file.as_deref();

    let mut filtered: Vec<&KantraViolation> = findings
        .violations
        .iter()
        .filter(|v| {
            if !target_filters.is_empty()
                && !v
                    .targets
                    .iter()
                    .any(|t| target_filters.iter().any(|f| f == t))
            {
                return false;
            }
            if !source_filters.is_empty()
                && !v
                    .sources
                    .iter()
                    .any(|s| source_filters.iter().any(|f| f == s))
            {
                return false;
            }
            if let Some(cat) = category_lc.as_deref() {
                let Some(vc) = v.category.as_deref() else {
                    return false;
                };
                if !vc.eq_ignore_ascii_case(cat) {
                    return false;
                }
            }
            if let Some(substr) = file_substr
                && !v.file.contains(substr)
            {
                return false;
            }
            true
        })
        .collect();

    // Compute full-match summary & per_rule BEFORE applying `--limit` truncation.
    let total_matched = filtered.len();
    let summary = findings_summary_refs(&filtered);
    let per_rule = build_per_rule_summary(&filtered);

    if let Some(n) = limit {
        filtered.truncate(n);
    }

    if ctx.format == OutputFormat::Json {
        let violations: Vec<Value> = filtered.iter().map(|v| compact_violation_json(v)).collect();
        let mut obj = Map::new();
        obj.insert("schema_version".into(), json!(1));
        obj.insert("command".into(), json!("rules_findings"));
        obj.insert("total_matched".into(), json!(total_matched));
        obj.insert("returned".into(), json!(violations.len()));
        if let Some(t) = target {
            obj.insert("target_filter".into(), json!(t));
        }
        if let Some(s) = source {
            obj.insert("source_filter".into(), json!(s));
        }
        if let Some(c) = category {
            obj.insert("category_filter".into(), json!(c));
        }
        if let Some(f) = file {
            obj.insert("file_filter".into(), json!(f));
        }
        obj.insert("summary".into(), summary);
        obj.insert("per_rule".into(), Value::Array(per_rule));
        obj.insert("violations".into(), Value::Array(violations));
        ctx.emit_json_value(&Value::Object(obj))?;
    } else {
        ctx.stdout_line(&format!(
            "rules findings: {} matched (showing {}), {} unique rules",
            total_matched,
            filtered.len(),
            per_rule.len()
        ))?;
        for v in filtered.iter().take(20) {
            ctx.stdout_line(&format!(
                "  {}:{}  {}  {:?}",
                v.file, v.line, v.rule_id, v.targets
            ))?;
        }
        if filtered.len() > 20 {
            ctx.stdout_line(&format!("  … {} more", filtered.len() - 20))?;
        }
    }
    Ok(())
}

/// `rgctl rules targets` — list embedded catalog targets/sources (+ hit counts when findings exist).
pub fn run_targets(ctx: &CliContext) -> Result<()> {
    let catalog =
        KantraCatalog::embedded().map_err(|e| anyhow::anyhow!("kantra embedded catalog: {e}"))?;

    let mut target_rules: BTreeMap<String, usize> = BTreeMap::new();
    let mut source_rules: BTreeMap<String, usize> = BTreeMap::new();
    for rule in &catalog.rules {
        for t in rule_konveyor_targets(rule) {
            *target_rules.entry(t).or_default() += 1;
        }
        for s in rule_konveyor_sources(rule) {
            *source_rules.entry(s).or_default() += 1;
        }
    }

    let findings_path = rgctl_graph::paths::artifact_path(&ctx.repo, "kantra_findings.json");
    let mut target_hits: BTreeMap<String, usize> = BTreeMap::new();
    let mut source_hits: BTreeMap<String, usize> = BTreeMap::new();
    let mut findings_present = false;
    if findings_path.exists()
        && let Ok(bytes) = fs::read(&findings_path)
        && let Ok(findings) = KantraFindings::from_json(&bytes)
    {
        findings_present = true;
        for v in &findings.violations {
            for t in &v.targets {
                *target_hits.entry(t.clone()).or_default() += 1;
            }
            for s in &v.sources {
                *source_hits.entry(s.clone()).or_default() += 1;
            }
        }
    }

    let targets: Vec<Value> = target_rules
        .iter()
        .map(|(name, rules)| {
            let mut obj = json!({
                "name": name,
                "rules": rules,
            });
            if findings_present {
                obj.as_object_mut().unwrap().insert(
                    "hits".into(),
                    json!(target_hits.get(name).copied().unwrap_or(0)),
                );
            }
            obj
        })
        .collect();
    let sources: Vec<Value> = source_rules
        .iter()
        .map(|(name, rules)| {
            let mut obj = json!({
                "name": name,
                "rules": rules,
            });
            if findings_present {
                obj.as_object_mut().unwrap().insert(
                    "hits".into(),
                    json!(source_hits.get(name).copied().unwrap_or(0)),
                );
            }
            obj
        })
        .collect();

    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&json!({
            "schema_version": 1,
            "command": "rules_targets",
            "catalog_id": catalog.catalog_id,
            "findings_present": findings_present,
            "targets": targets,
            "sources": sources,
        }))?;
    } else {
        ctx.stdout_line(&format!(
            "kantra targets ({} catalog rules):",
            catalog.rules.len()
        ))?;
        for t in &targets {
            let name = t.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let rules = t.get("rules").and_then(|v| v.as_u64()).unwrap_or(0);
            if let Some(hits) = t.get("hits").and_then(|v| v.as_u64()) {
                ctx.stdout_line(&format!("  {name}: {rules} rules, {hits} hits"))?;
            } else {
                ctx.stdout_line(&format!("  {name}: {rules} rules"))?;
            }
        }
    }
    Ok(())
}

fn findings_summary_refs(violations: &[&KantraViolation]) -> Value {
    let mut rules = HashSet::new();
    let mut files = HashSet::new();
    for v in violations {
        rules.insert(v.rule_id.as_str());
        files.insert(v.file.as_str());
    }
    json!({
        "violations": violations.len(),
        "unique_rules": rules.len(),
        "unique_files": files.len(),
    })
}

fn build_per_rule_summary(violations: &[&KantraViolation]) -> Vec<Value> {
    let mut by_rule: BTreeMap<&str, (usize, &KantraViolation)> = BTreeMap::new();
    for v in violations {
        let entry = by_rule.entry(v.rule_id.as_str()).or_insert((0, *v));
        entry.0 += 1;
    }
    by_rule
        .into_iter()
        .map(|(rule_id, (count, sample))| {
            let mut m = Map::new();
            m.insert("rule_id".into(), json!(rule_id));
            m.insert("count".into(), json!(count));
            if let Some(ref cat) = sample.category {
                m.insert("category".into(), json!(cat));
            }
            if let Some(eff) = sample.effort {
                m.insert("effort".into(), json!(eff));
            }
            if !sample.targets.is_empty() {
                m.insert("targets".into(), json!(sample.targets));
            }
            if let Some(ref msg) = sample.message {
                m.insert("message".into(), json!(msg));
            }
            Value::Object(m)
        })
        .collect()
}

fn compact_violation_json(v: &KantraViolation) -> Value {
    let mut val = serde_json::to_value(v).unwrap_or(Value::Null);
    if let Some(obj) = val.as_object_mut() {
        // Rule message is already emitted once per rule in `per_rule[]` to save tokens.
        obj.remove("message");
    }
    val
}

fn split_csv(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn collect_source_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(repo)
        .git_ignore(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if path.is_file() {
            out.push(path.to_path_buf());
        }
    }
    Ok(out)
}

fn build_eval_graph_from_snapshot(snapshot: &Path) -> Result<EvalGraph> {
    let store = rgctl_graph::SnapshotNodeStore::open(snapshot)?;
    let mut graph = EvalGraph::default();
    for id in store.all_node_ids() {
        let Some(node) = store.get_node(id)? else {
            continue;
        };
        if !matches!(
            node.node_type,
            NodeType::Import
                | NodeType::Class
                | NodeType::Interface
                | NodeType::Enum
                | NodeType::Annotation
                | NodeType::Function
                | NodeType::Module
                | NodeType::File
        ) {
            continue;
        }
        graph.nodes.push(EvalNode {
            id: Some(node.id),
            node_type: format!("{:?}", node.node_type),
            name: node.name.to_string(),
            qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
            file_path: node.file_path.as_ref().map(|s| s.to_string()),
            start_line: node.start_line,
            labels: node.labels.clone(),
        });
    }
    store.for_each_edge(|from, to, et| {
        let edge_name = match et {
            EdgeType::Extends => "EXTENDS",
            EdgeType::Implements => "IMPLEMENTS",
            EdgeType::AnnotatedWith => "ANNOTATED_WITH",
            EdgeType::ExposesRoute => "EXPOSES_ROUTE",
            EdgeType::Injects => "INJECTS",
            _ => return Ok(()),
        };
        let Some(from_node) = store.get_node(from)? else {
            return Ok(());
        };
        let Some(to_node) = store.get_node(to)? else {
            return Ok(());
        };
        graph.edges.push(EvalEdge {
            edge_type: edge_name.to_string(),
            from_name: from_node.name.to_string(),
            from_qualified: from_node
                .qualified_name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| from_node.name.to_string()),
            to_name: to_node.name.to_string(),
            to_qualified: to_node
                .qualified_name
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| to_node.name.to_string()),
            file_path: from_node
                .file_path
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            line: from_node.start_line.unwrap_or(1),
        });
        Ok(())
    })?;
    Ok(graph)
}
