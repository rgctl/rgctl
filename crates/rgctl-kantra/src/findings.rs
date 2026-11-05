//! Kantra findings artifact schema.

use crate::catalog::{rule_konveyor_sources, rule_konveyor_targets};
use crate::schema::KantraRule;
use serde::{Deserialize, Serialize};

/// Graph + analysis enrichment for a violation (schema v2).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ViolationEnrichment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_id: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagerank: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blast_radius_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact_zone_size: Option<usize>,
}

/// One rule violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KantraViolation {
    pub rule_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub file: String,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub matched_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<u32>,
    /// `"high"` for AST/graph/call-site hits; `"low"` for lexical fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// Location kind that produced the hit (e.g. `CONSTRUCTOR_CALL`, `METHOD_CALL`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<ViolationEnrichment>,
}

/// Rule skipped during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedRule {
    pub rule_id: String,
    pub reason: String,
}

/// Discover-time Kantra findings artifact (`.rgctl/kantra_findings.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KantraFindings {
    pub schema_version: u32,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    pub ruleset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_filter: Option<String>,
    pub evaluated_rules: usize,
    pub violations: Vec<KantraViolation>,
    pub skipped_rules: Vec<SkippedRule>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cache_hits: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cache_misses: usize,
}

fn is_zero(v: &usize) -> bool {
    *v == 0
}

impl KantraFindings {
    /// Build findings from evaluation output.
    pub fn new(
        ruleset_name: &str,
        catalog_id: Option<&str>,
        target_filter: Option<&str>,
        evaluated: usize,
    ) -> Self {
        Self {
            schema_version: 2,
            command: "kantra_findings".into(),
            catalog_id: catalog_id.map(str::to_string),
            ruleset: ruleset_name.into(),
            target_filter: target_filter.map(str::to_string),
            evaluated_rules: evaluated,
            violations: Vec::new(),
            skipped_rules: Vec::new(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Sort violations deterministically.
    pub fn sort_violations(&mut self) {
        self.violations
            .sort_by(|a, b| (&a.file, a.line, &a.rule_id).cmp(&(&b.file, b.line, &b.rule_id)));
    }

    /// Attach message/category/targets/sources/effort from rule metadata.
    pub fn enrich_from_rule(&mut self, rule: &KantraRule, idx: usize) {
        if let Some(v) = self.violations.get_mut(idx) {
            if v.message.is_none() {
                v.message = rule.message.clone();
            }
            if v.category.is_none() {
                v.category = rule.category.clone();
            }
            if v.targets.is_empty() {
                v.targets = rule_konveyor_targets(rule);
            }
            if v.sources.is_empty() {
                v.sources = rule_konveyor_sources(rule);
            }
            if v.effort.is_none() {
                v.effort = rule.effort;
            }
        }
    }

    /// Parse findings JSON (tolerant of schema v1 missing enrichment fields).
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rule() -> KantraRule {
        KantraRule {
            rule_id: "r1".into(),
            description: None,
            links: Vec::new(),
            category: Some("mandatory".into()),
            effort: Some(3),
            message: Some("migrate me".into()),
            labels: vec![
                "konveyor.io/target=quarkus".into(),
                "konveyor.io/target=jakarta-ee9+".into(),
                "konveyor.io/source=java-ee".into(),
            ],
            tag: Vec::new(),
            when: serde_yaml::from_str("builtin.filecontent:\n  pattern: x\n").unwrap(),
        }
    }

    #[test]
    fn enrich_from_rule_populates_targets_sources_effort() {
        let mut findings = KantraFindings::new("test", Some("c@1"), None, 1);
        findings.violations.push(KantraViolation {
            rule_id: "r1".into(),
            category: None,
            file: "a.java".into(),
            line: 1,
            message: None,
            matched_by: "builtin.filecontent".into(),
            symbol: None,
            targets: Vec::new(),
            sources: Vec::new(),
            effort: None,
            confidence: None,
            matched_location: None,
            enrichment: None,
        });
        findings.enrich_from_rule(&sample_rule(), 0);
        let v = &findings.violations[0];
        assert_eq!(v.message.as_deref(), Some("migrate me"));
        assert_eq!(v.category.as_deref(), Some("mandatory"));
        assert_eq!(v.targets, vec!["quarkus", "jakarta-ee9+"]);
        assert_eq!(v.sources, vec!["java-ee"]);
        assert_eq!(v.effort, Some(3));
    }

    #[test]
    fn from_json_tolerates_missing_targets_sources_effort() {
        let raw = r#"{
            "schema_version": 2,
            "command": "kantra_findings",
            "ruleset": "t",
            "evaluated_rules": 1,
            "violations": [{
                "rule_id": "r1",
                "file": "a.java",
                "line": 1,
                "matched_by": "builtin.filecontent"
            }],
            "skipped_rules": []
        }"#;
        let findings = KantraFindings::from_json(raw.as_bytes()).unwrap();
        assert!(findings.violations[0].targets.is_empty());
        assert!(findings.violations[0].sources.is_empty());
        assert_eq!(findings.violations[0].effort, None);
    }
}
