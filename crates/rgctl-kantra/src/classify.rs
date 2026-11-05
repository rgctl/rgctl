//! Classify Kantra rules by supported providers.

use crate::eval::builtin_path::unsupported_xpath_reason;
use crate::eval::compile_kantra_pattern;
use crate::schema::{KantraRule, RuleSupport, WhenClause};

/// Classification result for one rule.
#[derive(Debug, Clone)]
pub struct ClassifiedRule {
    pub rule_index: usize,
    pub support: RuleSupport,
    pub reason: Option<String>,
    pub clause: WhenClause,
}

const UNSUPPORTED_PROVIDERS: &[&str] =
    &["annotated.elements", "java.referenced.annotated.elements"];

/// Classify all rules in a ruleset.
pub fn classify_rules(rules: &[KantraRule]) -> Vec<ClassifiedRule> {
    rules
        .iter()
        .enumerate()
        .map(|(i, rule)| {
            let clause = WhenClause::from_value(&rule.when);
            if rule.is_tag_only() {
                return ClassifiedRule {
                    rule_index: i,
                    support: RuleSupport::Unsupported,
                    reason: Some("tag-only rule (no category or effort)".into()),
                    clause,
                };
            }
            let providers = clause.providers();
            let unsupported: Vec<_> = providers
                .iter()
                .filter(|p| is_unsupported_provider(p))
                .copied()
                .collect();
            let supported_count = providers
                .iter()
                .filter(|p| is_supported_provider(p))
                .count();
            let (support, reason) = if unsupported.is_empty() {
                (RuleSupport::Supported, None)
            } else if supported_count > 0 {
                (
                    RuleSupport::Partial,
                    Some(format!("unsupported: {}", unsupported.join(", "))),
                )
            } else {
                (
                    RuleSupport::Unsupported,
                    Some(format!("unsupported: {}", unsupported.join(", "))),
                )
            };
            let (support, reason) = match invalid_regex_reason(&clause) {
                Some(regex_err) => (RuleSupport::Unsupported, Some(regex_err)),
                None => (support, reason),
            };
            let (support, reason) = match unsupported_clause_reason(&clause) {
                Some(xpath_err) => (RuleSupport::Unsupported, Some(xpath_err)),
                None => (support, reason),
            };
            ClassifiedRule {
                rule_index: i,
                support,
                reason,
                clause,
            }
        })
        .collect()
}

fn is_unsupported_provider(provider: &str) -> bool {
    UNSUPPORTED_PROVIDERS
        .iter()
        .any(|u| provider == *u || provider.contains("annotated.elements"))
}

fn is_supported_provider(provider: &str) -> bool {
    matches!(
        provider,
        "builtin.filecontent"
            | "builtin.file"
            | "builtin.hasTags"
            | "builtin.xml"
            | "builtin.json"
            | "go.referenced"
            | "java.referenced"
            | "java.dependency"
            | "go.dependency"
    )
}

fn invalid_regex_reason(clause: &WhenClause) -> Option<String> {
    for pattern in clause.regex_patterns() {
        if pattern.is_empty() {
            continue;
        }
        if let Err(e) = compile_kantra_pattern(pattern) {
            return Some(format!("invalid regex: {e}"));
        }
    }
    None
}

fn unsupported_clause_reason(clause: &WhenClause) -> Option<String> {
    match clause {
        WhenClause::BuiltinXml { xpath, .. } => unsupported_xpath_reason(xpath),
        WhenClause::JavaReferenced {
            annotated_pattern: Some(_),
            ..
        } => {
            // annotated.pattern is supported for ANNOTATION; annotated.elements already
            // classified via Unsupported provider. Nothing extra here.
            None
        }
        WhenClause::And(items) | WhenClause::Or(items) => {
            items.iter().find_map(unsupported_clause_reason)
        }
        WhenClause::Not(inner) => unsupported_clause_reason(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::KantraRule;

    fn rule(when: &str) -> KantraRule {
        KantraRule {
            rule_id: "t".into(),
            description: None,
            links: Vec::new(),
            category: None,
            effort: None,
            message: None,
            labels: vec![],
            tag: Vec::new(),
            when: serde_yaml::from_str(when).unwrap(),
        }
    }

    #[test]
    fn tag_only_rule_is_unsupported() {
        let mut r = rule("java.referenced:\n  pattern: org.jboss.seam*\n");
        r.tag = vec!["Seam".into()];
        let c = classify_rules(&[r]);
        assert_eq!(c[0].support, RuleSupport::Unsupported);
        assert_eq!(
            c[0].reason.as_deref(),
            Some("tag-only rule (no category or effort)")
        );
    }

    #[test]
    fn hybrid_tag_and_effort_still_supported() {
        let mut r = rule("java.referenced:\n  pattern: javax.ejb*\n");
        r.tag = vec!["EJB".into()];
        r.effort = Some(3);
        r.category = Some("mandatory".into());
        let c = classify_rules(&[r]);
        assert_eq!(c[0].support, RuleSupport::Supported);
    }

    #[test]
    fn filecontent_supported() {
        let c = classify_rules(&[rule("builtin.filecontent:\n  pattern: x\n")]);
        assert_eq!(c[0].support, RuleSupport::Supported);
    }

    #[test]
    fn java_dependency_supported() {
        let c = classify_rules(&[rule("java.dependency:\n  name: foo\n")]);
        assert_eq!(c[0].support, RuleSupport::Supported);
    }

    #[test]
    fn xml_supported() {
        let c = classify_rules(&[rule("builtin.xml:\n  xpath: //x\n")]);
        assert_eq!(c[0].support, RuleSupport::Supported);
    }

    #[test]
    fn leading_star_pattern_is_classified_supported() {
        let c = classify_rules(&[rule(
            "java.referenced:\n  pattern: '*EntityManager'\n  location: IMPORT\n",
        )]);
        assert_eq!(c[0].support, RuleSupport::Supported);
        assert!(c[0].reason.is_none());
    }

    #[test]
    fn truly_invalid_regex_still_unsupported() {
        let c = classify_rules(&[rule(
            "java.referenced:\n  pattern: '(unclosed'\n  location: IMPORT\n",
        )]);
        assert_eq!(c[0].support, RuleSupport::Unsupported);
        assert!(c[0].reason.as_ref().unwrap().contains("invalid regex"));
    }

    #[test]
    fn unsupported_xpath_is_classified() {
        let c = classify_rules(&[rule("builtin.xml:\n  xpath: '//foo[position()=1]'\n")]);
        assert_eq!(c[0].support, RuleSupport::Unsupported);
        assert!(c[0].reason.as_ref().unwrap().contains("unsupported xpath"));
    }
}
