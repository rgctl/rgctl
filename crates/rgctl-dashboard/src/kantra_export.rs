//! Kantra migration-rule violations for the dashboard bundle.

use crate::export_util::write_json_compact;
use crate::source_catalog::{
    SOURCE_INDEX_FILE, SourceFileEntry, ensure_source_file, write_source_index,
};
use rayon::prelude::*;
use rgctl_kantra::{
    KantraCatalog, KantraFindings, KantraViolation, SkippedRule, rule_konveyor_targets,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const KANTRA_INDEX_FILE: &str = "kantra_index.json";
pub const KANTRA_DETAIL_DIR: &str = "kantra";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    pub detail_dir: String,
    pub catalog_id: Option<String>,
    pub ruleset: String,
    pub target_filter: Option<String>,
    pub evaluated_rules: usize,
    pub violation_count: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub by_category: HashMap<String, usize>,
    pub file_count: usize,
    pub rule_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_targets: Vec<KantraTargetEntry>,
    pub files: Vec<KantraFileEntry>,
    pub rules: Vec<KantraRuleEntry>,
    pub skipped_rules: Vec<SkippedRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraTargetEntry {
    pub target: String,
    pub violation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraFileEntry {
    pub id: String,
    pub path: String,
    pub count: usize,
    pub categories: HashMap<String, usize>,
    pub max_blast: f64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub target_categories: HashMap<String, HashMap<String, usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraRuleEntry {
    pub rule_id: String,
    pub count: usize,
    pub category: Option<String>,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KantraFileBundle {
    pub schema_version: u32,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub violations: Vec<KantraViolation>,
}

#[derive(Debug, Default, Clone)]
pub struct KantraExportSummary {
    pub available: bool,
    pub violation_count: usize,
    pub evaluated_rules: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub by_category: HashMap<String, usize>,
    pub catalog_id: Option<String>,
    pub ruleset: Option<String>,
    pub target_filter: Option<String>,
    pub file_count: usize,
    pub rule_count: usize,
}

impl KantraExportSummary {
    pub fn manifest_section(&self) -> Option<crate::manifest::KantraSection> {
        if !self.available {
            return None;
        }
        Some(crate::manifest::KantraSection {
            available: true,
            violation_count: self.violation_count,
            evaluated_rules: self.evaluated_rules,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            by_category: self.by_category.clone(),
            index_path: Some(KANTRA_INDEX_FILE.into()),
            detail_dir: Some(KANTRA_DETAIL_DIR.into()),
            catalog_id: self.catalog_id.clone(),
            ruleset: self.ruleset.clone(),
            target_filter: self.target_filter.clone(),
            file_count: self.file_count,
            rule_count: self.rule_count,
        })
    }
}

pub fn export_kantra_bundle(
    repo_root: &Path,
    out_dir: &Path,
) -> Result<KantraExportSummary, String> {
    let findings_path = rgctl_graph::paths::artifact_path(repo_root, "kantra_findings.json");
    if !findings_path.is_file() {
        return Ok(KantraExportSummary::default());
    }
    let bytes = fs::read(&findings_path).map_err(|e| e.to_string())?;
    let findings = KantraFindings::from_json(&bytes).map_err(|e| e.to_string())?;
    let rule_targets = load_rule_targets_map(findings.catalog_id.as_deref());

    let detail_root = out_dir.join(KANTRA_DETAIL_DIR);
    if detail_root.exists() {
        fs::remove_dir_all(&detail_root).map_err(|e| e.to_string())?;
    }
    let files_dir = detail_root.join("files");
    fs::create_dir_all(&files_dir).map_err(|e| e.to_string())?;
    let source_cache = Mutex::new(HashMap::<String, SourceFileEntry>::new());
    let mut new_sources: Vec<SourceFileEntry> = Vec::new();

    let mut by_category = HashMap::new();
    let mut by_target: HashMap<String, usize> = HashMap::new();
    let mut by_file: HashMap<String, Vec<KantraViolation>> = HashMap::new();
    let mut by_rule: HashMap<String, (usize, Option<String>, Option<String>)> = HashMap::new();

    for v in &findings.violations {
        let cat = v.category.as_deref().unwrap_or("uncategorized");
        *by_category.entry(cat.to_string()).or_insert(0) += 1;
        for target in targets_for_rule(&rule_targets, &v.rule_id) {
            *by_target.entry(target).or_insert(0) += 1;
        }

        let rel = relativize_path(repo_root, &v.file);
        by_file.entry(rel).or_default().push(v.clone());

        let entry =
            by_rule
                .entry(v.rule_id.clone())
                .or_insert((0, v.category.clone(), v.message.clone()));
        entry.0 += 1;
        if entry.1.is_none() {
            entry.1 = v.category.clone();
        }
        if entry.2.is_none() {
            entry.2 = v.message.clone();
        }
    }

    let file_entries: Vec<(KantraFileEntry, Vec<KantraViolation>)> = by_file
        .into_iter()
        .map(|(path, mut violations)| {
            violations.sort_by(|a, b| (a.line, &a.rule_id).cmp(&(b.line, &b.rule_id)));
            let id = file_id(&path);
            let mut categories = HashMap::new();
            let mut target_categories: HashMap<String, HashMap<String, usize>> = HashMap::new();
            let mut max_blast = 0.0_f64;
            for v in &violations {
                let cat = v.category.as_deref().unwrap_or("uncategorized").to_string();
                *categories.entry(cat.clone()).or_insert(0) += 1;
                for target in targets_for_rule(&rule_targets, &v.rule_id) {
                    target_categories
                        .entry(target)
                        .or_default()
                        .entry(cat.clone())
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                }
                if let Some(b) = v.enrichment.as_ref().and_then(|e| e.blast_radius_score) {
                    max_blast = max_blast.max(b);
                }
            }
            let abs_path = repo_root.join(&path);
            let source_id = abs_path
                .to_str()
                .and_then(|abs| ensure_source_file(out_dir, abs, &source_cache))
                .map(|entry| {
                    new_sources.push(entry.clone());
                    entry.source_id
                });
            let entry = KantraFileEntry {
                id: id.clone(),
                path,
                count: violations.len(),
                categories,
                target_categories,
                max_blast,
                source_id,
            };
            (entry, violations)
        })
        .collect();

    merge_source_index(out_dir, &new_sources)?;

    file_entries
        .par_iter()
        .try_for_each(|(entry, violations)| {
            let bundle = KantraFileBundle {
                schema_version: 1,
                path: entry.path.clone(),
                source_id: entry.source_id.clone(),
                violations: violations.clone(),
            };
            write_json_compact(&files_dir.join(format!("{}.json", entry.id)), &bundle)
        })?;

    let mut files: Vec<KantraFileEntry> = file_entries.into_iter().map(|(e, _)| e).collect();
    files.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));

    let mut rules: Vec<KantraRuleEntry> = by_rule
        .into_iter()
        .map(|(rule_id, (count, category, message))| KantraRuleEntry {
            targets: targets_for_rule(&rule_targets, &rule_id),
            rule_id,
            count,
            category,
            message,
        })
        .collect();
    rules.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });

    let mut available_targets: Vec<KantraTargetEntry> = by_target
        .into_iter()
        .map(|(target, violation_count)| KantraTargetEntry {
            target,
            violation_count,
        })
        .collect();
    available_targets.sort_by(|a, b| {
        b.violation_count
            .cmp(&a.violation_count)
            .then_with(|| a.target.cmp(&b.target))
    });

    let index = KantraIndexPayload {
        schema_version: 1,
        available: true,
        detail_dir: KANTRA_DETAIL_DIR.into(),
        catalog_id: findings.catalog_id.clone(),
        ruleset: findings.ruleset.clone(),
        target_filter: findings.target_filter.clone(),
        evaluated_rules: findings.evaluated_rules,
        violation_count: findings.violations.len(),
        cache_hits: findings.cache_hits,
        cache_misses: findings.cache_misses,
        by_category: by_category.clone(),
        file_count: files.len(),
        rule_count: rules.len(),
        available_targets,
        files,
        rules,
        skipped_rules: findings.skipped_rules.clone(),
    };
    write_json_compact(&out_dir.join(KANTRA_INDEX_FILE), &index)?;

    Ok(KantraExportSummary {
        available: true,
        violation_count: index.violation_count,
        evaluated_rules: index.evaluated_rules,
        cache_hits: index.cache_hits,
        cache_misses: index.cache_misses,
        by_category,
        catalog_id: index.catalog_id.clone(),
        ruleset: Some(index.ruleset.clone()),
        target_filter: index.target_filter.clone(),
        file_count: index.file_count,
        rule_count: index.rule_count,
    })
}

fn file_id(path: &str) -> String {
    blake3::hash(path.as_bytes()).to_hex()[..16].to_string()
}

fn load_rule_targets_map(catalog_id: Option<&str>) -> HashMap<String, Vec<String>> {
    let Ok(catalog) = KantraCatalog::embedded() else {
        return HashMap::new();
    };
    if let Some(expected) = catalog_id {
        if expected != catalog.catalog_id {
            tracing::warn!(
                expected = expected,
                embedded = %catalog.catalog_id,
                "kantra export: findings catalog_id differs from embedded catalog; rule targets may be incomplete"
            );
        }
    }
    catalog
        .rules
        .iter()
        .map(|rule| (rule.rule_id.clone(), rule_konveyor_targets(rule)))
        .collect()
}

fn targets_for_rule(rule_targets: &HashMap<String, Vec<String>>, rule_id: &str) -> Vec<String> {
    rule_targets.get(rule_id).cloned().unwrap_or_default()
}

fn relativize_path(repo_root: &Path, file: &str) -> String {
    let p = PathBuf::from(file);
    p.strip_prefix(repo_root)
        .unwrap_or(&p)
        .to_string_lossy()
        .into_owned()
}

fn merge_source_index(out_dir: &Path, new_entries: &[SourceFileEntry]) -> Result<(), String> {
    if new_entries.is_empty() {
        return Ok(());
    }
    let path = out_dir.join(SOURCE_INDEX_FILE);
    let mut by_path: HashMap<String, SourceFileEntry> = if path.is_file() {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        serde_json::from_slice::<crate::source_catalog::SourceIndexPayload>(&bytes)
            .map(|payload| {
                payload
                    .files
                    .into_iter()
                    .map(|entry| (entry.file_path.clone(), entry))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };
    for entry in new_entries {
        by_path
            .entry(entry.file_path.clone())
            .or_insert_with(|| entry.clone());
    }
    let merged: Vec<SourceFileEntry> = by_path.into_values().collect();
    write_source_index(out_dir, &merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_kantra::ViolationEnrichment;
    use tempfile::TempDir;

    #[test]
    fn export_writes_index_and_file_shards() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let src = repo.join("src/Foo.java");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        fs::write(&src, "class Foo {}\n").unwrap();

        let findings = KantraFindings {
            schema_version: 2,
            command: "kantra_findings".into(),
            catalog_id: Some("fixture@test".into()),
            ruleset: "test-rules".into(),
            target_filter: None,
            evaluated_rules: 2,
            violations: vec![
                KantraViolation {
                    rule_id: "rule-a".into(),
                    category: Some("mandatory".into()),
                    file: src.display().to_string(),
                    line: 1,
                    message: Some("hit a".into()),
                    matched_by: "java.referenced".into(),
                    symbol: Some("javax.ejb.Stateless".into()),
                    targets: vec!["quarkus".into()],
                    sources: Vec::new(),
                    effort: Some(1),
                    enrichment: Some(ViolationEnrichment {
                        blast_radius_score: Some(3.5),
                        ..Default::default()
                    }),
                },
                KantraViolation {
                    rule_id: "rule-b".into(),
                    category: Some("potential".into()),
                    file: src.display().to_string(),
                    line: 1,
                    message: Some("hit b".into()),
                    matched_by: "filecontent".into(),
                    symbol: None,
                    targets: Vec::new(),
                    sources: Vec::new(),
                    effort: None,
                    enrichment: None,
                },
            ],
            skipped_rules: vec![SkippedRule {
                rule_id: "skip-1".into(),
                reason: "unsupported".into(),
            }],
            cache_hits: 1,
            cache_misses: 0,
        };
        let rgctl = repo.join(".rgctl");
        fs::create_dir_all(&rgctl).unwrap();
        fs::write(
            rgctl.join("kantra_findings.json"),
            serde_json::to_vec_pretty(&findings).unwrap(),
        )
        .unwrap();

        let out = tmp.path().join("dashboard");
        fs::create_dir_all(&out).unwrap();
        let summary = export_kantra_bundle(&repo, &out).unwrap();
        assert!(summary.available);
        assert_eq!(summary.violation_count, 2);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.rule_count, 2);

        let index: KantraIndexPayload =
            serde_json::from_slice(&fs::read(out.join(KANTRA_INDEX_FILE)).unwrap()).unwrap();
        assert!(index.available);
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].count, 2);
        assert_eq!(index.rules.len(), 2);
        assert_eq!(index.skipped_rules.len(), 1);
        assert!(index.available_targets.is_empty());

        let shard: KantraFileBundle = serde_json::from_slice(
            &fs::read(
                out.join(KANTRA_DETAIL_DIR)
                    .join("files")
                    .join(format!("{}.json", index.files[0].id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(shard.violations.len(), 2);
        assert_eq!(shard.path, "src/Foo.java");
        assert!(shard.source_id.is_some());
        assert!(
            out.join("sources")
                .join(format!("{}.txt", shard.source_id.unwrap()))
                .is_file()
        );
    }

    #[test]
    fn missing_findings_is_noop() {
        let tmp = TempDir::new().unwrap();
        let summary = export_kantra_bundle(tmp.path(), tmp.path()).unwrap();
        assert!(!summary.available);
    }
}
