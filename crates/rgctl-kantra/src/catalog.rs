//! Compiled Konveyor rules catalog (`RBKC`) embedded at build time.

use crate::error::{KantraError, Result};
use crate::loader::KantraRuleset;
use crate::schema::{KantraRule, KantraRulesetDoc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Magic bytes for embedded catalog files (`RBKC`).
pub const RBKC_MAGIC: &[u8; 4] = b"RBKC";

/// Current on-disk catalog format version.
pub const RBKC_VERSION: u32 = 1;

/// Compiled rules catalog (build-time YAML → runtime blob).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KantraCatalog {
    pub catalog_id: String,
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<KantraRule>,
}

/// Serializable rule payload inside an `RBKC` file (`when` as YAML text for bincode).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StoredRule {
    rule_id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    effort: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    when_yaml: String,
}

impl StoredRule {
    fn from_rule(rule: &KantraRule) -> Result<Self> {
        let when_yaml = serde_yaml::to_string(&rule.when)
            .map_err(|e| KantraError::msg(format!("rule {} when yaml: {e}", rule.rule_id)))?;
        Ok(Self {
            rule_id: rule.rule_id.clone(),
            description: rule.description.clone(),
            category: rule.category.clone(),
            effort: rule.effort,
            message: rule.message.clone(),
            labels: rule.labels.clone(),
            when_yaml,
        })
    }

    fn into_rule(self) -> Result<KantraRule> {
        let when: serde_yaml::Value = serde_yaml::from_str(&self.when_yaml)
            .map_err(|e| KantraError::msg(format!("rule {} when parse: {e}", self.rule_id)))?;
        Ok(KantraRule {
            rule_id: self.rule_id,
            description: self.description,
            category: self.category,
            effort: self.effort,
            message: self.message,
            labels: self.labels,
            when,
        })
    }
}

/// Serializable payload inside an `RBKC` file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StoredCatalog {
    version: u32,
    catalog_id: String,
    name: String,
    description: Option<String>,
    rules: Vec<StoredRule>,
}

impl KantraCatalog {
    /// Load the catalog compiled into the binary at build time.
    #[cfg(feature = "kantra-embedded-java")]
    pub fn embedded() -> Result<Self> {
        Self::from_bytes(include_bytes!(env!("RGCTL_KANTRA_CATALOG")))
    }

    /// Embedded catalog unavailable when `kantra-embedded-java` is disabled.
    #[cfg(not(feature = "kantra-embedded-java"))]
    pub fn embedded() -> Result<Self> {
        Err(KantraError::msg(
            "embedded Kantra catalog not compiled (enable feature kantra-embedded-java)",
        ))
    }

    /// Load rules from a ruleset directory tree (dev override for `--kantra-catalog`).
    pub fn load_tree(root: impl AsRef<std::path::Path>) -> Result<Self> {
        use std::collections::HashSet;

        let root = root.as_ref();
        let dirs = collect_ruleset_dirs(root);
        if dirs.is_empty() {
            return Err(KantraError::msg(format!(
                "no ruleset.yaml found under {}",
                root.display()
            )));
        }
        let mut rules = Vec::new();
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for dir in dirs {
            let rs = KantraRuleset::load(&dir)?;
            names.push(rs.doc.name.clone());
            for rule in rs.doc.rules {
                if !seen.insert(rule.rule_id.clone()) {
                    return Err(KantraError::msg(format!(
                        "duplicate ruleID {} in catalog tree",
                        rule.rule_id
                    )));
                }
                rules.push(rule);
            }
        }
        let name = if names.len() == 1 {
            names[0].clone()
        } else {
            format!("catalog:{}", root.display())
        };
        let catalog_id = format!("tree@{}", root.display());
        Ok(Self {
            catalog_id,
            name,
            description: None,
            rules,
        })
    }

    /// Parse an `RBKC` catalog blob.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 8 {
            return Err(KantraError::msg("kantra catalog truncated"));
        }
        if &bytes[0..4] != RBKC_MAGIC {
            return Err(KantraError::msg(
                "kantra catalog: bad magic (expected RBKC)",
            ));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != RBKC_VERSION {
            return Err(KantraError::msg(format!(
                "kantra catalog: unsupported version {version}"
            )));
        }
        let stored: StoredCatalog = bincode::deserialize(&bytes[8..])
            .map_err(|e| KantraError::msg(format!("kantra catalog decode: {e}")))?;
        let rules = stored
            .rules
            .into_iter()
            .map(StoredRule::into_rule)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            catalog_id: stored.catalog_id,
            name: stored.name,
            description: stored.description,
            rules,
        })
    }

    /// Encode catalog to `RBKC` bytes (used by `build.rs` and tests).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let rules = self
            .rules
            .iter()
            .map(StoredRule::from_rule)
            .collect::<Result<Vec<_>>>()?;
        let stored = StoredCatalog {
            version: RBKC_VERSION,
            catalog_id: self.catalog_id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            rules,
        };
        let mut out = Vec::with_capacity(8 + 256);
        out.extend_from_slice(RBKC_MAGIC);
        out.extend_from_slice(&RBKC_VERSION.to_le_bytes());
        bincode::serialize_into(&mut out, &stored)
            .map_err(|e| KantraError::msg(format!("kantra catalog encode: {e}")))?;
        Ok(out)
    }

    /// Rules matching `konveyor.io/target=<target>` when a filter is set.
    pub fn rules_for_eval(&self, target_filter: Option<&str>) -> Vec<KantraRule> {
        match target_filter {
            Some(target) => self
                .rules
                .iter()
                .filter(|r| rule_matches_target(r, target))
                .cloned()
                .collect(),
            None => self.rules.clone(),
        }
    }

    /// Convert to the in-memory ruleset type used by the evaluator.
    pub fn to_ruleset(&self, target_filter: Option<&str>) -> KantraRuleset {
        let rules = self.rules_for_eval(target_filter);
        let name = match target_filter {
            Some(t) => format!("{}:target={t}", self.name),
            None => self.name.clone(),
        };
        KantraRuleset {
            doc: KantraRulesetDoc {
                name,
                description: self.description.clone(),
                rules,
            },
            rules_dir: PathBuf::new(),
        }
    }
}

/// Merge ruleset-level labels onto a rule (rule labels win on duplicate keys).
pub fn merge_ruleset_labels(ruleset_labels: &[String], rule: &mut KantraRule) {
    let mut merged = ruleset_labels.to_vec();
    for label in &rule.labels {
        if !merged.iter().any(|l| l == label) {
            merged.push(label.clone());
        }
    }
    rule.labels = merged;
}

/// Whether a rule carries `konveyor.io/target=<target>`.
pub fn rule_matches_target(rule: &KantraRule, target: &str) -> bool {
    let needle = format!("konveyor.io/target={target}");
    rule.labels.iter().any(|l| l == &needle)
}

/// All `konveyor.io/target` values on a rule (ruleset + rule labels).
pub fn rule_konveyor_targets(rule: &KantraRule) -> Vec<String> {
    let mut out = Vec::new();
    for label in &rule.labels {
        if let Some(target) = label.strip_prefix("konveyor.io/target=")
            && !out.iter().any(|t| t == target)
        {
            out.push(target.to_string());
        }
    }
    out
}

/// All `konveyor.io/source` values on a rule (ruleset + rule labels).
pub fn rule_konveyor_sources(rule: &KantraRule) -> Vec<String> {
    let mut out = Vec::new();
    for label in &rule.labels {
        if let Some(source) = label.strip_prefix("konveyor.io/source=")
            && !out.iter().any(|s| s == source)
        {
            out.push(source.to_string());
        }
    }
    out
}

fn collect_ruleset_dirs(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    if root.join("ruleset.yaml").is_file() {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_ruleset_dirs(&path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::KantraRule;

    fn sample_rule(id: &str, labels: &[&str]) -> KantraRule {
        KantraRule {
            rule_id: id.into(),
            description: None,
            category: None,
            effort: None,
            message: None,
            labels: labels.iter().map(|s| (*s).to_string()).collect(),
            when: serde_yaml::from_str("builtin.filecontent:\n  pattern: x\n").unwrap(),
        }
    }

    #[test]
    fn round_trip_rbkc_bytes() {
        let catalog = KantraCatalog {
            catalog_id: "test@abc".into(),
            name: "test".into(),
            description: None,
            rules: vec![sample_rule("r1", &[])],
        };
        let bytes = catalog.to_bytes().unwrap();
        let loaded = KantraCatalog::from_bytes(&bytes).unwrap();
        assert_eq!(loaded, catalog);
    }

    #[test]
    fn embedded_catalog_loads() {
        let catalog = KantraCatalog::embedded().expect("embedded catalog");
        assert!(!catalog.catalog_id.is_empty());
        assert!(!catalog.rules.is_empty());
    }

    #[test]
    fn konveyor_targets_deduped_from_labels() {
        let rule = sample_rule(
            "r1",
            &[
                "konveyor.io/target=quarkus",
                "konveyor.io/target=quarkus",
                "konveyor.io/target=spring-boot3+",
            ],
        );
        assert_eq!(
            rule_konveyor_targets(&rule),
            vec!["quarkus", "spring-boot3+"]
        );
    }

    #[test]
    fn konveyor_sources_deduped_from_labels() {
        let rule = sample_rule(
            "r1",
            &[
                "konveyor.io/source=java-ee",
                "konveyor.io/source=java-ee",
                "konveyor.io/source=springboot",
            ],
        );
        assert_eq!(rule_konveyor_sources(&rule), vec!["java-ee", "springboot"]);
    }

    #[test]
    fn target_filter_keeps_matching_rules() {
        let catalog = KantraCatalog {
            catalog_id: "t@1".into(),
            name: "t".into(),
            description: None,
            rules: vec![
                sample_rule("a", &["konveyor.io/target=quarkus"]),
                sample_rule("b", &["konveyor.io/target=spring-boot3+"]),
            ],
        };
        let filtered = catalog.rules_for_eval(Some("quarkus"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule_id, "a");
    }

    #[test]
    fn merge_ruleset_labels_dedupes() {
        let mut rule = sample_rule("r", &["konveyor.io/target=quarkus"]);
        merge_ruleset_labels(
            &[
                "konveyor.io/source=java".into(),
                "konveyor.io/target=quarkus".into(),
            ],
            &mut rule,
        );
        assert_eq!(rule.labels.len(), 2);
    }
}
