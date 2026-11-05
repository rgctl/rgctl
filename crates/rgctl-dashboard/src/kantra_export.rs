//! Kantra migration-rule violations for the dashboard bundle.

use crate::export_util::write_json_compact;
use crate::source_catalog::{SourceCatalog, build_source_catalog, write_source_index};
use rayon::prelude::*;
use rgctl_kantra::{
    KantraCatalog, KantraFindings, KantraLink, KantraViolation, SkippedRule, rule_konveyor_sources,
    rule_konveyor_targets,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_sources: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<KantraLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<u32>,
}

/// Owned file shard shape (deserialize for tests / external readers).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct KantraFileBundle {
    pub schema_version: u32,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub violations: Vec<KantraViolation>,
}

/// Borrowed file shard written without cloning [`KantraViolation`] payloads.
#[derive(Serialize)]
struct KantraFileBundleView<'a> {
    schema_version: u32,
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_id: Option<&'a str>,
    violations: Vec<&'a KantraViolation>,
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

#[allow(dead_code)] // Public convenience wrapper; production path uses `_with_inputs`.
pub fn export_kantra_bundle(
    repo_root: &Path,
    out_dir: &Path,
) -> Result<KantraExportSummary, String> {
    export_kantra_bundle_with_inputs(repo_root, out_dir, None, None)
}

/// Export kantra artifacts, optionally reusing in-memory findings and a shared source catalog.
pub fn export_kantra_bundle_with_inputs(
    repo_root: &Path,
    out_dir: &Path,
    findings: Option<&KantraFindings>,
    source_catalog: Option<&mut SourceCatalog>,
) -> Result<KantraExportSummary, String> {
    let owned_findings;
    let findings = match findings {
        Some(f) => f,
        None => {
            let findings_path =
                rgctl_graph::paths::artifact_path(repo_root, "kantra_findings.json");
            if !findings_path.is_file() {
                return Ok(KantraExportSummary::default());
            }
            let bytes = fs::read(&findings_path).map_err(|e| e.to_string())?;
            owned_findings = KantraFindings::from_json(&bytes).map_err(|e| e.to_string())?;
            &owned_findings
        }
    };
    export_kantra_from_findings(repo_root, out_dir, findings, source_catalog)
}

fn export_kantra_from_findings(
    repo_root: &Path,
    out_dir: &Path,
    findings: &KantraFindings,
    source_catalog: Option<&mut SourceCatalog>,
) -> Result<KantraExportSummary, String> {
    let rule_meta = load_rule_metadata_map(findings.catalog_id.as_deref());

    let detail_root = out_dir.join(KANTRA_DETAIL_DIR);
    if detail_root.exists() {
        fs::remove_dir_all(&detail_root).map_err(|e| e.to_string())?;
    }
    let files_dir = detail_root.join("files");
    fs::create_dir_all(&files_dir).map_err(|e| e.to_string())?;

    let mut by_category = HashMap::new();
    let mut by_target: HashMap<String, usize> = HashMap::new();
    let mut by_source: HashMap<String, usize> = HashMap::new();
    // relative path → (absolute path, violation indices)
    let mut by_file: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    let mut by_rule: HashMap<String, RuleAgg> = HashMap::new();

    for (idx, v) in findings.violations.iter().enumerate() {
        let cat = v.category.as_deref().unwrap_or("uncategorized");
        *by_category.entry(cat.to_string()).or_insert(0) += 1;
        let meta = rule_meta.get(&v.rule_id);
        let targets: &[String] = if !v.targets.is_empty() {
            &v.targets
        } else {
            meta.map(|m| m.targets.as_slice()).unwrap_or(&[])
        };
        let sources: &[String] = if !v.sources.is_empty() {
            &v.sources
        } else {
            meta.map(|m| m.sources.as_slice()).unwrap_or(&[])
        };
        for target in targets {
            *by_target.entry(target.clone()).or_insert(0) += 1;
        }
        for source in sources {
            *by_source.entry(source.clone()).or_insert(0) += 1;
        }

        let rel = relativize_path(repo_root, &v.file);
        let abs = absolute_source_path(repo_root, &v.file);
        by_file
            .entry(rel)
            .or_insert_with(|| (abs, Vec::new()))
            .1
            .push(idx);

        let entry = by_rule.entry(v.rule_id.clone()).or_insert_with(|| RuleAgg {
            count: 0,
            category: v.category.clone(),
            message: v.message.clone(),
            description: meta.and_then(|m| m.description.clone()),
            links: meta.map(|m| m.links.clone()).unwrap_or_default(),
            targets: targets.to_vec(),
            sources: sources.to_vec(),
            effort: v.effort.or_else(|| meta.and_then(|m| m.effort)),
        });
        entry.count += 1;
        if entry.category.is_none() {
            entry.category = v.category.clone();
        }
        if entry.message.is_none() {
            entry.message = v.message.clone();
        }
        if entry.effort.is_none() {
            entry.effort = v.effort.or_else(|| meta.and_then(|m| m.effort));
        }
        if entry.targets.is_empty() {
            entry.targets = targets.to_vec();
        }
        if entry.sources.is_empty() {
            entry.sources = sources.to_vec();
        }
    }

    let mut local_catalog = SourceCatalog::default();
    let catalog = source_catalog.unwrap_or(&mut local_catalog);
    ensure_kantra_sources(out_dir, catalog, &by_file)?;

    let file_entries: Vec<(KantraFileEntry, Vec<usize>)> = by_file
        .into_iter()
        .map(|(path, (abs, mut indices))| {
            indices.sort_by(|&a, &b| {
                let va = &findings.violations[a];
                let vb = &findings.violations[b];
                (va.line, &va.rule_id).cmp(&(vb.line, &vb.rule_id))
            });
            let id = file_id(&path);
            let mut categories = HashMap::new();
            let mut target_categories: HashMap<String, HashMap<String, usize>> = HashMap::new();
            let mut max_blast = 0.0_f64;
            for &vi in &indices {
                let v = &findings.violations[vi];
                let cat = v.category.as_deref().unwrap_or("uncategorized").to_string();
                *categories.entry(cat.clone()).or_insert(0) += 1;
                let targets: &[String] = if !v.targets.is_empty() {
                    &v.targets
                } else {
                    rule_meta
                        .get(&v.rule_id)
                        .map(|m| m.targets.as_slice())
                        .unwrap_or(&[])
                };
                for target in targets {
                    target_categories
                        .entry(target.clone())
                        .or_default()
                        .entry(cat.clone())
                        .and_modify(|n| *n += 1)
                        .or_insert(1);
                }
                if let Some(b) = v.enrichment.as_ref().and_then(|e| e.blast_radius_score) {
                    max_blast = max_blast.max(b);
                }
            }
            let source_id = catalog.get(&abs).map(|e| e.source_id.clone());
            let entry = KantraFileEntry {
                id: id.clone(),
                path,
                count: indices.len(),
                categories,
                target_categories,
                max_blast,
                source_id,
            };
            (entry, indices)
        })
        .collect();

    file_entries.par_iter().try_for_each(|(entry, indices)| {
        let violations: Vec<&KantraViolation> =
            indices.iter().map(|&i| &findings.violations[i]).collect();
        let bundle = KantraFileBundleView {
            schema_version: 1,
            path: &entry.path,
            source_id: entry.source_id.as_deref(),
            violations,
        };
        write_json_compact(&files_dir.join(format!("{}.json", entry.id)), &bundle)
    })?;

    let mut files: Vec<KantraFileEntry> = file_entries.into_iter().map(|(e, _)| e).collect();
    files.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));

    let mut rules: Vec<KantraRuleEntry> = by_rule
        .into_iter()
        .map(|(rule_id, agg)| KantraRuleEntry {
            rule_id,
            count: agg.count,
            category: agg.category,
            message: agg.message,
            description: agg.description,
            links: agg.links,
            targets: agg.targets,
            sources: agg.sources,
            effort: agg.effort,
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

    let mut available_sources: Vec<String> = by_source.into_keys().collect();
    available_sources.sort();

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
        available_sources,
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

fn ensure_kantra_sources(
    out_dir: &Path,
    catalog: &mut SourceCatalog,
    by_file: &HashMap<String, (String, Vec<usize>)>,
) -> Result<(), String> {
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for (abs, _) in by_file.values() {
        if !seen.insert(abs.clone()) {
            continue;
        }
        if catalog.get(abs).is_none() {
            missing.push(abs.clone());
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    let extra = build_source_catalog(out_dir, &missing)?;
    catalog.merge_from(&extra);
    write_source_index(out_dir, &catalog.entries())?;
    Ok(())
}

fn file_id(path: &str) -> String {
    blake3::hash(path.as_bytes()).to_hex()[..16].to_string()
}

#[derive(Debug, Clone, Default)]
struct RuleAgg {
    count: usize,
    category: Option<String>,
    message: Option<String>,
    description: Option<String>,
    links: Vec<KantraLink>,
    targets: Vec<String>,
    sources: Vec<String>,
    effort: Option<u32>,
}

#[derive(Debug, Clone, Default)]
struct RuleMetadata {
    description: Option<String>,
    links: Vec<KantraLink>,
    targets: Vec<String>,
    sources: Vec<String>,
    effort: Option<u32>,
}

fn load_rule_metadata_map(catalog_id: Option<&str>) -> HashMap<String, RuleMetadata> {
    let Ok(catalog) = KantraCatalog::embedded() else {
        return HashMap::new();
    };
    if let Some(expected) = catalog_id
        && expected != catalog.catalog_id
    {
        tracing::warn!(
            expected = expected,
            embedded = %catalog.catalog_id,
            "kantra export: findings catalog_id differs from embedded catalog; rule metadata may be incomplete"
        );
    }
    catalog
        .rules
        .iter()
        .map(|rule| {
            (
                rule.rule_id.clone(),
                RuleMetadata {
                    description: rule.description.clone(),
                    links: rule.links.clone(),
                    targets: rule_konveyor_targets(rule),
                    sources: rule_konveyor_sources(rule),
                    effort: rule.effort,
                },
            )
        })
        .collect()
}

fn relativize_path(repo_root: &Path, file: &str) -> String {
    let p = PathBuf::from(file);
    p.strip_prefix(repo_root)
        .unwrap_or(&p)
        .to_string_lossy()
        .into_owned()
}

fn absolute_source_path(repo_root: &Path, file: &str) -> String {
    let p = PathBuf::from(file);
    if p.is_absolute() {
        p.to_string_lossy().into_owned()
    } else {
        repo_root.join(p).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_kantra::ViolationEnrichment;
    use tempfile::TempDir;

    fn sample_findings(src: &Path) -> KantraFindings {
        KantraFindings {
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
                    confidence: None,
                    matched_location: None,
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
                    confidence: None,
                    matched_location: None,
                    enrichment: None,
                },
            ],
            skipped_rules: vec![SkippedRule {
                rule_id: "skip-1".into(),
                reason: "unsupported".into(),
            }],
            cache_hits: 1,
            cache_misses: 0,
        }
    }

    #[test]
    fn export_writes_index_and_file_shards() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let src = repo.join("src/Foo.java");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        fs::write(&src, "class Foo {}\n").unwrap();

        let findings = sample_findings(&src);
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
        assert_eq!(
            index
                .available_targets
                .iter()
                .map(|t| t.target.as_str())
                .collect::<Vec<_>>(),
            vec!["quarkus"]
        );
        let rule_a = index
            .rules
            .iter()
            .find(|r| r.rule_id == "rule-a")
            .expect("rule-a");
        assert_eq!(rule_a.effort, Some(1));
        assert_eq!(rule_a.targets, vec!["quarkus"]);

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
        assert!(out.join(crate::source_catalog::SOURCE_INDEX_FILE).is_file());
    }

    #[test]
    fn export_reuses_in_memory_findings_and_catalog() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let src = repo.join("src/Foo.java");
        fs::create_dir_all(src.parent().unwrap()).unwrap();
        fs::write(&src, "class Foo {}\n").unwrap();
        let findings = sample_findings(&src);
        let out = tmp.path().join("dashboard");
        fs::create_dir_all(&out).unwrap();

        let mut catalog = SourceCatalog::default();
        let summary =
            export_kantra_bundle_with_inputs(&repo, &out, Some(&findings), Some(&mut catalog))
                .unwrap();
        assert!(summary.available);
        assert_eq!(summary.violation_count, 2);
        assert!(!catalog.is_empty());
        // No disk findings file — export must not require it when findings are passed in.
        assert!(!repo.join(".rgctl/kantra_findings.json").is_file());
    }

    #[test]
    fn missing_findings_is_noop() {
        let tmp = TempDir::new().unwrap();
        let summary = export_kantra_bundle(tmp.path(), tmp.path()).unwrap();
        assert!(!summary.available);
    }
}
