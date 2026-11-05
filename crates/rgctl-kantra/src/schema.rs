//! Kantra YAML rule types.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;

/// Parsed Kantra ruleset metadata + rules.
#[derive(Debug, Clone)]
pub struct KantraRulesetDoc {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<KantraRule>,
}

/// External reference link on a Kantra rule (`links:` YAML).
///
/// Konveyor rulesets use either `url:` or `link:` for the href (`alias` for YAML;
/// bincode uses positional `Serialize`/`Deserialize` field order).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KantraLink {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(alias = "link")]
    pub url: String,
}

/// One Kantra rule from YAML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KantraRule {
    #[serde(rename = "ruleID")]
    pub rule_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub links: Vec<KantraLink>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub effort: Option<u32>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Konveyor technology-discovery tags (`tag:`). Tag-only rules are non-incident.
    #[serde(default)]
    pub tag: Vec<String>,
    pub when: Value,
}

impl KantraRule {
    /// Konveyor tag-only rules emit technology tags, not migration incidents.
    pub fn is_tag_only(&self) -> bool {
        !self.tag.is_empty() && self.category.is_none() && self.effort.is_none()
    }
}

/// Support level for a rule after classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSupport {
    Supported,
    Partial,
    Unsupported,
}

/// Parsed `when` clause tree.
#[derive(Debug, Clone)]
pub enum WhenClause {
    FileContent {
        pattern: String,
        file_pattern: Option<String>,
    },
    File {
        pattern: String,
    },
    HasTags {
        tags: Vec<String>,
    },
    GoReferenced {
        pattern: String,
    },
    JavaReferenced {
        pattern: String,
        location: Option<String>,
        annotated_pattern: Option<String>,
    },
    JavaDependency {
        name: String,
        nameregex: Option<String>,
        lowerbound: Option<String>,
        upperbound: Option<String>,
    },
    GoDependency {
        name: String,
        nameregex: Option<String>,
        lowerbound: Option<String>,
        upperbound: Option<String>,
    },
    BuiltinXml {
        xpath: String,
        file_pattern: Option<String>,
    },
    BuiltinJson {
        jsonpath: String,
        file_pattern: Option<String>,
    },
    And(Vec<WhenClause>),
    Or(Vec<WhenClause>),
    Not(Box<WhenClause>),
    Unsupported {
        provider: String,
    },
}

impl WhenClause {
    /// Parse a Kantra `when` YAML value into a clause tree.
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Mapping(map) => {
                if let Some(and) = map.get(Value::from("and"))
                    && let Value::Sequence(items) = and
                {
                    return WhenClause::And(items.iter().map(WhenClause::from_value).collect());
                }
                if let Some(or) = map.get(Value::from("or"))
                    && let Value::Sequence(items) = or
                {
                    return WhenClause::Or(items.iter().map(WhenClause::from_value).collect());
                }
                if let Some(not) = map.get(Value::from("not")) {
                    if let Value::Bool(is_negated) = not {
                        let mut rest = map.clone();
                        rest.remove(Value::from("not"));
                        let inner = WhenClause::from_value(&Value::Mapping(rest));
                        return if *is_negated {
                            WhenClause::Not(Box::new(inner))
                        } else {
                            inner
                        };
                    }
                    return WhenClause::Not(Box::new(WhenClause::from_value(not)));
                }
                if let Some(cond) = map.get(Value::from("condition")) {
                    return WhenClause::from_value(cond);
                }
                for (key, val) in map {
                    let provider = match key.as_str() {
                        Some(s) => s,
                        None => continue,
                    };
                    if matches!(provider, "as" | "from" | "ignore") {
                        continue;
                    }
                    return parse_provider(provider, val);
                }
                WhenClause::Unsupported {
                    provider: "empty".into(),
                }
            }
            _ => WhenClause::Unsupported {
                provider: "invalid".into(),
            },
        }
    }

    /// Collect provider names for classification.
    pub fn providers(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_providers(&mut out);
        out
    }

    /// Regex patterns used by supported evaluators (for compile-time validation).
    pub fn regex_patterns(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_regex_patterns(&mut out);
        out
    }

    fn collect_regex_patterns<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            WhenClause::FileContent { pattern, .. } => out.push(pattern),
            WhenClause::GoReferenced { pattern } => out.push(pattern),
            WhenClause::JavaReferenced {
                pattern,
                annotated_pattern,
                ..
            } => {
                out.push(pattern);
                if let Some(p) = annotated_pattern {
                    out.push(p);
                }
            }
            WhenClause::And(items) | WhenClause::Or(items) => {
                for item in items {
                    item.collect_regex_patterns(out);
                }
            }
            WhenClause::Not(inner) => inner.collect_regex_patterns(out),
            WhenClause::File { .. }
            | WhenClause::HasTags { .. }
            | WhenClause::JavaDependency { .. }
            | WhenClause::GoDependency { .. }
            | WhenClause::BuiltinXml { .. }
            | WhenClause::BuiltinJson { .. }
            | WhenClause::Unsupported { .. } => {}
        }
    }

    fn collect_providers<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            WhenClause::FileContent { .. } => out.push("builtin.filecontent"),
            WhenClause::File { .. } => out.push("builtin.file"),
            WhenClause::HasTags { .. } => out.push("builtin.hasTags"),
            WhenClause::GoReferenced { .. } => out.push("go.referenced"),
            WhenClause::JavaReferenced { .. } => out.push("java.referenced"),
            WhenClause::JavaDependency { .. } => out.push("java.dependency"),
            WhenClause::GoDependency { .. } => out.push("go.dependency"),
            WhenClause::BuiltinXml { .. } => out.push("builtin.xml"),
            WhenClause::BuiltinJson { .. } => out.push("builtin.json"),
            WhenClause::And(items) | WhenClause::Or(items) => {
                for item in items {
                    item.collect_providers(out);
                }
            }
            WhenClause::Not(inner) => inner.collect_providers(out),
            WhenClause::Unsupported { provider } => out.push(provider),
        }
    }
}

fn parse_provider(provider: &str, val: &Value) -> WhenClause {
    match provider {
        "builtin.filecontent" => {
            let (pattern, file_pattern) = pattern_fields(val);
            WhenClause::FileContent {
                pattern,
                file_pattern,
            }
        }
        "builtin.file" => WhenClause::File {
            pattern: string_field(val, "pattern").unwrap_or_default(),
        },
        "builtin.hasTags" => WhenClause::HasTags {
            tags: tags_field(val),
        },
        "go.referenced" => WhenClause::GoReferenced {
            pattern: string_field(val, "pattern").unwrap_or_default(),
        },
        "java.referenced" => {
            let location = string_field(val, "location");
            let annotated_pattern = val
                .get("annotated")
                .and_then(|a| a.get("pattern"))
                .and_then(|p| p.as_str())
                .map(str::to_string);
            if val.get("annotated.elements").is_some() {
                return WhenClause::Unsupported {
                    provider: "java.referenced.annotated.elements".into(),
                };
            }
            WhenClause::JavaReferenced {
                pattern: string_field(val, "pattern").unwrap_or_default(),
                location,
                annotated_pattern,
            }
        }
        "java.dependency" => WhenClause::JavaDependency {
            name: string_field(val, "name").unwrap_or_default(),
            nameregex: string_field(val, "nameregex").or_else(|| string_field(val, "name_regex")),
            lowerbound: string_field(val, "lowerbound"),
            upperbound: string_field(val, "upperbound"),
        },
        "go.dependency" => WhenClause::GoDependency {
            name: string_field(val, "name").unwrap_or_default(),
            nameregex: string_field(val, "nameregex").or_else(|| string_field(val, "name_regex")),
            lowerbound: string_field(val, "lowerbound"),
            upperbound: string_field(val, "upperbound"),
        },
        "builtin.xml" => WhenClause::BuiltinXml {
            xpath: string_field(val, "xpath")
                .or_else(|| string_field(val, "pattern"))
                .unwrap_or_default(),
            file_pattern: string_field(val, "filePattern")
                .or_else(|| string_field(val, "filepattern"))
                .or_else(|| first_seq_string_field(val, "filepaths"))
                .or_else(|| first_seq_string_field(val, "filePaths")),
        },
        "builtin.json" => WhenClause::BuiltinJson {
            jsonpath: string_field(val, "jsonpath")
                .or_else(|| string_field(val, "pattern"))
                .unwrap_or_default(),
            file_pattern: string_field(val, "filePattern")
                .or_else(|| string_field(val, "filepattern"))
                .or_else(|| first_seq_string_field(val, "filepaths"))
                .or_else(|| first_seq_string_field(val, "filePaths")),
        },
        other => WhenClause::Unsupported {
            provider: other.to_string(),
        },
    }
}

fn pattern_fields(val: &Value) -> (String, Option<String>) {
    (
        string_field(val, "pattern").unwrap_or_default(),
        string_field(val, "filePattern").or_else(|| string_field(val, "filepattern")),
    )
}

fn string_field(val: &Value, key: &str) -> Option<String> {
    val.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn first_seq_string_field(val: &Value, key: &str) -> Option<String> {
    val.get(key)
        .and_then(|v| v.as_sequence())
        .and_then(|seq| seq.first())
        .and_then(|v| v.as_str())
        .filter(|s| !s.contains("{{"))
        .map(str::to_string)
}

fn tags_field(val: &Value) -> Vec<String> {
    if let Some(seq) = val.as_sequence() {
        return seq
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
    }
    if let Some(map) = val.as_mapping() {
        return map
            .get(Value::from("tags"))
            .and_then(|t| t.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
    }
    Vec::new()
}

/// Ruleset metadata from `ruleset.yaml`.
#[derive(Debug, Deserialize)]
pub struct RulesetMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

/// Index rules by id for composite evaluation.
pub fn index_rules(rules: &[KantraRule]) -> HashMap<String, usize> {
    rules
        .iter()
        .enumerate()
        .map(|(i, r)| (r.rule_id.clone(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filecontent_when() {
        let yaml = r#"
builtin.filecontent:
  pattern: hkdf
  filePattern: "**/*.go"
"#;
        let val: Value = serde_yaml::from_str(yaml).unwrap();
        let clause = WhenClause::from_value(&val);
        match clause {
            WhenClause::FileContent {
                pattern,
                file_pattern,
            } => {
                assert_eq!(pattern, "hkdf");
                assert_eq!(file_pattern.as_deref(), Some("**/*.go"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_and_when() {
        let yaml = r#"
and:
  - builtin.filecontent:
      pattern: foo
  - go.referenced:
      pattern: bar
"#;
        let val: Value = serde_yaml::from_str(yaml).unwrap();
        let clause = WhenClause::from_value(&val);
        assert!(matches!(clause, WhenClause::And(items) if items.len() == 2));
    }

    #[test]
    fn deserializes_tag_description_and_links() {
        let yaml = r#"
- ruleID: seam-java-00000
  description: Detects Seam usage
  tag:
    - Seam
  links:
    - title: Docs
      url: https://example.com/seam
  when:
    java.referenced:
      pattern: org.jboss.seam*
"#;
        let rules: Vec<KantraRule> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];
        assert_eq!(rule.description.as_deref(), Some("Detects Seam usage"));
        assert_eq!(rule.tag, vec!["Seam"]);
        assert_eq!(rule.links.len(), 1);
        assert_eq!(rule.links[0].title.as_deref(), Some("Docs"));
        assert_eq!(rule.links[0].url, "https://example.com/seam");
        assert!(rule.is_tag_only());
    }

    #[test]
    fn hybrid_tag_and_effort_is_not_tag_only() {
        let yaml = r#"
- ruleID: hybrid-00001
  tag:
    - EJB
  effort: 3
  category: mandatory
  when:
    java.referenced:
      pattern: javax.ejb*
"#;
        let rules: Vec<KantraRule> = serde_yaml::from_str(yaml).unwrap();
        assert!(!rules[0].is_tag_only());
    }
}
