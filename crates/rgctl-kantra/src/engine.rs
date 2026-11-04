//! Kantra evaluation engine.

use crate::cache::{KantraFileCache, hash_file_content};
use crate::catalog::KantraCatalog;
use crate::classify::{ClassifiedRule, classify_rules};
use crate::error::Result;
use crate::eval::MatchSite;
use crate::eval::builtin_path::{eval_builtin_json, eval_builtin_xml};
use crate::eval::compose::eval_compose;
use crate::eval::dependency::eval_dependency;
use crate::eval::file::eval_file;
use crate::eval::filecontent::{SourceCache, eval_filecontent};
use crate::eval::go_referenced::eval_go_referenced;
use crate::eval::has_tags::eval_has_tags;
use crate::eval::java_referenced::eval_java_referenced;
use crate::findings::{KantraFindings, SkippedRule};
use crate::loader::KantraRuleset;
use crate::schema::{RuleSupport, WhenClause};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Lightweight graph node for Kantra evaluation.
#[derive(Debug, Clone)]
pub struct EvalNode {
    pub id: Option<uuid::Uuid>,
    pub node_type: String,
    pub name: String,
    pub qualified_name: Option<String>,
    pub file_path: Option<String>,
    pub start_line: Option<usize>,
    pub labels: Vec<String>,
}

/// Inheritance / annotation edge for Java referenced rules.
#[derive(Debug, Clone)]
pub struct EvalEdge {
    pub edge_type: String,
    pub from_name: String,
    pub from_qualified: String,
    pub to_name: String,
    pub to_qualified: String,
    pub file_path: String,
    pub line: usize,
}

/// Graph inputs for evaluation.
#[derive(Debug, Clone, Default)]
pub struct EvalGraph {
    pub nodes: Vec<EvalNode>,
    pub edges: Vec<EvalEdge>,
}

/// Discover evaluation context.
pub struct EvalContext<'a> {
    pub repo_root: &'a Path,
    pub files: &'a [PathBuf],
    pub sources: &'a SourceCache,
    pub graph: &'a EvalGraph,
    pub cache: Option<&'a mut KantraFileCache>,
    pub cached_files: HashSet<String>,
}

/// Sub-stage timings (seconds).
#[derive(Debug, Clone, Default)]
pub struct EvalStageTimings {
    pub load_secs: f64,
    pub filecontent_secs: f64,
    pub referenced_secs: f64,
    pub compose_secs: f64,
    pub total_secs: f64,
}

/// Kantra rules evaluator.
pub struct KantraEngine {
    ruleset: KantraRuleset,
    classified: Vec<ClassifiedRule>,
    catalog_id: Option<String>,
    target_filter: Option<String>,
}

impl KantraEngine {
    /// Load and classify a ruleset from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<(Self, f64)> {
        let load_start = Instant::now();
        let ruleset = KantraRuleset::load(path)?;
        let classified = classify_rules(&ruleset.doc.rules);
        let load_secs = load_start.elapsed().as_secs_f64();
        Ok((
            Self {
                ruleset,
                classified,
                catalog_id: None,
                target_filter: None,
            },
            load_secs,
        ))
    }

    /// Build evaluator from a compiled catalog (embedded or decoded blob).
    pub fn from_catalog(
        catalog: KantraCatalog,
        target_filter: Option<&str>,
    ) -> Result<(Self, f64)> {
        let load_start = Instant::now();
        let catalog_id = catalog.catalog_id.clone();
        let target_owned = target_filter.map(str::to_string);
        let ruleset = catalog.to_ruleset(target_filter);
        let classified = classify_rules(&ruleset.doc.rules);
        let load_secs = load_start.elapsed().as_secs_f64();
        Ok((
            Self {
                ruleset,
                classified,
                catalog_id: Some(catalog_id),
                target_filter: target_owned,
            },
            load_secs,
        ))
    }

    /// Embedded catalog identity when loaded from `RBKC`.
    pub fn catalog_id(&self) -> Option<&str> {
        self.catalog_id.as_deref()
    }

    /// Active `konveyor.io/target` filter, if any.
    pub fn target_filter(&self) -> Option<&str> {
        self.target_filter.as_deref()
    }

    /// Ruleset display name.
    pub fn ruleset_name(&self) -> &str {
        &self.ruleset.doc.name
    }

    /// Evaluate all supported rules.
    pub fn evaluate(
        &self,
        ctx: &mut EvalContext<'_>,
    ) -> Result<(KantraFindings, EvalStageTimings)> {
        let total_start = Instant::now();
        let mut timings = EvalStageTimings::default();
        let mut findings = KantraFindings::new(
            &self.ruleset.doc.name,
            self.catalog_id.as_deref(),
            self.target_filter.as_deref(),
            self.ruleset.doc.rules.len(),
        );
        if let Some(cache) = ctx.cache.as_mut() {
            for (rel, content) in ctx.sources {
                let hash = hash_file_content(content.as_str());
                if let Some(vs) = cache.get(rel, &hash) {
                    findings.violations.extend(vs);
                    ctx.cached_files.insert(rel.clone());
                }
            }
            findings.cache_hits = cache.hits;
            findings.cache_misses = cache.misses;
        }
        let mut fresh_file_violations: HashMap<
            String,
            (String, Vec<crate::findings::KantraViolation>),
        > = HashMap::new();

        for item in &self.classified {
            let rule = &self.ruleset.doc.rules[item.rule_index];
            if item.support == RuleSupport::Unsupported {
                findings.skipped_rules.push(SkippedRule {
                    rule_id: rule.rule_id.clone(),
                    reason: item.reason.clone().unwrap_or_else(|| "unsupported".into()),
                });
                continue;
            }
            if item.support == RuleSupport::Partial {
                findings.skipped_rules.push(SkippedRule {
                    rule_id: rule.rule_id.clone(),
                    reason: item.reason.clone().unwrap_or_else(|| "partial".into()),
                });
            }

            let rule_start = Instant::now();
            let mut violations = match if is_composite(&item.clause) {
                let compose_start = Instant::now();
                let mut leaf_sites = Vec::new();
                if let Err(e) = collect_leaf_sites(&item.clause, &mut leaf_sites, rule, ctx) {
                    findings.skipped_rules.push(SkippedRule {
                        rule_id: rule.rule_id.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
                let mut idx = 0;
                let out = eval_compose(&item.clause, &rule.rule_id, &leaf_sites, &mut idx);
                timings.compose_secs += compose_start.elapsed().as_secs_f64();
                Ok(out)
            } else {
                eval_leaf(&item.clause, rule, ctx)
            } {
                Ok(v) => v,
                Err(e) => {
                    findings.skipped_rules.push(SkippedRule {
                        rule_id: rule.rule_id.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            };
            enrich_violations(&mut violations, rule);
            for v in &violations {
                if v.matched_by == "builtin.filecontent" {
                    let hash = ctx
                        .sources
                        .get(&v.file)
                        .map(|c| hash_file_content(c.as_str()))
                        .unwrap_or_default();
                    fresh_file_violations
                        .entry(v.file.clone())
                        .or_insert((hash, Vec::new()))
                        .1
                        .push(v.clone());
                }
            }
            let elapsed = rule_start.elapsed().as_secs_f64();
            if matches!(
                item.clause,
                WhenClause::FileContent { .. } | WhenClause::File { .. }
            ) {
                timings.filecontent_secs += elapsed;
            } else {
                timings.referenced_secs += elapsed;
            }
            findings.violations.extend(violations);
        }

        findings.sort_violations();
        if let Some(cache) = ctx.cache.as_mut() {
            for (rel, content) in ctx.sources {
                if ctx.cached_files.contains(rel) {
                    continue;
                }
                let hash = hash_file_content(content.as_str());
                let violations = fresh_file_violations
                    .remove(rel.as_str())
                    .map(|(_, vs)| vs)
                    .unwrap_or_default();
                cache.put(rel.clone(), hash, violations);
            }
            findings.cache_hits = cache.hits;
            findings.cache_misses = cache.misses;
        }
        timings.total_secs = total_start.elapsed().as_secs_f64();
        Ok((findings, timings))
    }
}

fn is_composite(clause: &WhenClause) -> bool {
    matches!(
        clause,
        WhenClause::And(_) | WhenClause::Or(_) | WhenClause::Not(_)
    )
}

fn enrich_violations(
    violations: &mut [crate::findings::KantraViolation],
    rule: &crate::schema::KantraRule,
) {
    let targets = crate::catalog::rule_konveyor_targets(rule);
    let sources = crate::catalog::rule_konveyor_sources(rule);
    for v in violations {
        if v.message.is_none() {
            v.message = rule.message.clone();
        }
        if v.category.is_none() {
            v.category = rule.category.clone();
        }
        if v.targets.is_empty() {
            v.targets = targets.clone();
        }
        if v.sources.is_empty() {
            v.sources = sources.clone();
        }
        if v.effort.is_none() {
            v.effort = rule.effort;
        }
    }
}

fn eval_leaf(
    clause: &WhenClause,
    rule: &crate::schema::KantraRule,
    ctx: &EvalContext<'_>,
) -> Result<Vec<crate::findings::KantraViolation>> {
    match clause {
        WhenClause::FileContent {
            pattern,
            file_pattern,
        } => eval_filecontent(
            &rule.rule_id,
            pattern,
            file_pattern.as_deref(),
            ctx.repo_root,
            ctx.files,
            ctx.sources,
            &ctx.cached_files,
        )
        .map_err(crate::error::KantraError::from),
        WhenClause::File { pattern } => eval_file(&rule.rule_id, pattern, ctx.repo_root, ctx.files)
            .map_err(|e| crate::error::KantraError::msg(e.to_string())),
        WhenClause::HasTags { tags } => Ok(eval_has_tags(&rule.rule_id, tags, &ctx.graph.nodes)),
        WhenClause::GoReferenced { pattern } => {
            eval_go_referenced(&rule.rule_id, pattern, &ctx.graph.nodes)
                .map_err(crate::error::KantraError::from)
        }
        WhenClause::JavaReferenced {
            pattern,
            location,
            annotated_pattern: _,
        } => eval_java_referenced(
            &rule.rule_id,
            pattern,
            location.as_deref(),
            &ctx.graph.nodes,
            &ctx.graph.edges,
            ctx.repo_root,
            ctx.sources,
        )
        .map_err(crate::error::KantraError::from),
        WhenClause::JavaDependency {
            name,
            nameregex,
            lowerbound,
            upperbound,
        } => Ok(eval_dependency(
            &rule.rule_id,
            "maven",
            name,
            nameregex.as_deref(),
            lowerbound.as_deref(),
            upperbound.as_deref(),
            &ctx.graph.nodes,
        )),
        WhenClause::GoDependency {
            name,
            nameregex,
            lowerbound,
            upperbound,
        } => Ok(eval_dependency(
            &rule.rule_id,
            "golang",
            name,
            nameregex.as_deref(),
            lowerbound.as_deref(),
            upperbound.as_deref(),
            &ctx.graph.nodes,
        )),
        WhenClause::BuiltinXml {
            xpath,
            file_pattern,
        } => Ok(eval_builtin_xml(
            &rule.rule_id,
            xpath,
            file_pattern.as_deref(),
            ctx.repo_root,
            ctx.files,
            ctx.sources,
        )),
        WhenClause::BuiltinJson {
            jsonpath,
            file_pattern,
        } => Ok(eval_builtin_json(
            &rule.rule_id,
            jsonpath,
            file_pattern.as_deref(),
            ctx.repo_root,
            ctx.files,
            ctx.sources,
        )),
        WhenClause::Unsupported { .. } => Ok(Vec::new()),
        WhenClause::And(_) | WhenClause::Or(_) | WhenClause::Not(_) => Ok(Vec::new()),
    }
}

fn collect_leaf_sites(
    clause: &WhenClause,
    leaf_sites: &mut Vec<Vec<MatchSite>>,
    rule: &crate::schema::KantraRule,
    ctx: &EvalContext<'_>,
) -> Result<()> {
    match clause {
        WhenClause::And(items) | WhenClause::Or(items) => {
            for item in items {
                collect_leaf_sites(item, leaf_sites, rule, ctx)?;
            }
        }
        WhenClause::Not(inner) => collect_leaf_sites(inner, leaf_sites, rule, ctx)?,
        leaf => {
            let hits = eval_leaf(leaf, rule, ctx)?;
            leaf_sites.push(
                hits.iter()
                    .map(|v| MatchSite::new(&v.file, v.line))
                    .collect(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::filecontent::SourceCache;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn evaluates_fixture_filecontent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ruleset.yaml"), "name: t\n").unwrap();
        fs::write(
            dir.path().join("rules.yaml"),
            r#"
- ruleID: r1
  message: found
  category: mandatory
  when:
    builtin.filecontent:
      pattern: UNIQUE_TOKEN_XYZ
"#,
        )
        .unwrap();

        let mut sources = SourceCache::new();
        sources.insert("foo.go".into(), Arc::new("UNIQUE_TOKEN_XYZ\n".into()));
        let files = vec![dir.path().join("foo.go")];
        let (engine, _) = KantraEngine::load(dir.path()).unwrap();
        let mut ctx = EvalContext {
            repo_root: dir.path(),
            files: &files,
            sources: &sources,
            graph: &EvalGraph::default(),
            cache: None,
            cached_files: HashSet::new(),
        };
        let (findings, _) = engine.evaluate(&mut ctx).unwrap();
        assert!(!findings.violations.is_empty());
        assert_eq!(findings.violations[0].rule_id, "r1");
    }
}
