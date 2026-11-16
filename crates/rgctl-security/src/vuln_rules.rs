//! Declarative vulnerability trigger-condition rules (`.rgctl/vuln-rules.d/*.yaml`).
//!
//! Many CVEs are exploitable only when an explicit feature flag, configuration option, or receiver
//! setup is enabled (e.g., `DeserializationFeature.UNWRAP_SINGLE_VALUE_ARRAYS` on Jackson
//! `ObjectMapper`, `enableDefaultTyping`, or `YAML.unsafe_load`).
//!
//! Rules under `{repo}/.rgctl/vuln-rules.d/*.yaml` allow teams (and advisory bundles) to declare
//! required source predicates before an advisory is marked `exploitable`.

use crate::error::SecurityError;
use crate::vuln_analyze::{ExploitabilityVerdict, VulnAnalyzeResult};
use openvex::{Justification, Status};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Relative directory under `{repo}/.rgctl/`.
pub const VULN_RULES_DIR: &str = "vuln-rules.d";

/// Verdict to emit when a rule's required trigger patterns are absent from local source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissingTriggerPolicy {
    /// Downgrade to `not_exploitable` (`InlineMitigationsAlreadyExist`) unless receiver escapes.
    #[default]
    NotExploitable,
    /// Downgrade to `under_investigation` when configuration cannot be ruled out globally.
    UnderInvestigation,
}

/// One declarative conditional-exploitability rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnTriggerRule {
    /// Rule identifier.
    pub id: String,
    /// Advisory IDs or CVE aliases governed by this rule (case-insensitive match).
    #[serde(default)]
    pub advisories: Vec<String>,
    /// Optional package coordinate substring (e.g. `jackson-databind`).
    #[serde(default)]
    pub package: Option<String>,
    /// At least one of these tokens/patterns must appear in the call-site slice/file for the
    /// vulnerability trigger condition to be satisfied.
    #[serde(default)]
    pub required_patterns: Vec<String>,
    /// Policy when `required_patterns` are absent.
    #[serde(default)]
    pub missing_verdict: MissingTriggerPolicy,
    /// Human-readable explanation appended to `notes` and OpenVEX `impact_statement`.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RuleFileEnvelope {
    Multi { rules: Vec<VulnTriggerRule> },
    Single(VulnTriggerRule),
    List(Vec<VulnTriggerRule>),
}

/// Load all `*.yaml` / `*.yml` trigger rules from `{repo}/.rgctl/vuln-rules.d/`.
pub fn load_vuln_trigger_rules(repo: &Path) -> Result<Vec<VulnTriggerRule>, SecurityError> {
    let dir = rgctl_graph::paths::artifact_path(repo, VULN_RULES_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<_> = WalkDir::new(&dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| matches!(p.extension().and_then(|s| s.to_str()), Some("yaml" | "yml")))
        .collect();
    files.sort();

    let mut rules = Vec::new();
    for path in files {
        let bytes = fs::read(&path).map_err(|e| SecurityError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let parsed: RuleFileEnvelope = serde_yaml::from_slice(&bytes)
            .map_err(|e| SecurityError::Msg(format!("parse {}: {e}", path.display())))?;
        match parsed {
            RuleFileEnvelope::Multi { rules: r } | RuleFileEnvelope::List(r) => rules.extend(r),
            RuleFileEnvelope::Single(r) => rules.push(r),
        }
    }
    Ok(rules)
}

/// Evaluate matching [`VulnTriggerRule`]s against an already-computed [`VulnAnalyzeResult`],
/// adjusting `exploitability`, `openvex`, and `notes` when required trigger preconditions are absent.
pub fn apply_trigger_rules(
    repo: &Path,
    rules: &[VulnTriggerRule],
    result: &mut VulnAnalyzeResult,
    strict_reachability: bool,
) {
    if rules.is_empty()
        || !matches!(
            result.exploitability,
            ExploitabilityVerdict::Exploitable | ExploitabilityVerdict::UnderInvestigation
        )
    {
        return;
    }

    for rule in rules {
        if rule.required_patterns.is_empty() {
            continue;
        }
        let id_matches = rule.advisories.is_empty()
            || rule
                .advisories
                .iter()
                .any(|a| result.osv_id.eq_ignore_ascii_case(a) || result.osv_id.contains(a));
        let pkg_matches = match (&rule.package, &result.package) {
            (Some(req_pkg), Some(res_pkg)) => {
                res_pkg.name.contains(req_pkg) || req_pkg.contains(&res_pkg.name)
            }
            (Some(_), None) => false,
            (None, _) => true,
        };
        if !id_matches || !pkg_matches {
            continue;
        }

        let Some(st) = &result.sink_taint else {
            continue;
        };
        if st.call_sites.is_empty() {
            continue;
        }

        let mut any_pattern_present = false;
        let mut any_receiver_escapes = false;

        for site in &st.call_sites {
            let p = Path::new(&site.file);
            let abs = if p.is_absolute() {
                p.to_path_buf()
            } else {
                repo.join(p)
            };
            let Ok(src) = fs::read_to_string(&abs) else {
                any_receiver_escapes = true;
                continue;
            };
            if rule.required_patterns.iter().any(|pat| src.contains(pat)) {
                any_pattern_present = true;
                break;
            }
            if receiver_likely_escapes_local_file(&src, site.line as usize, &site.target_method) {
                any_receiver_escapes = true;
            }
        }

        if any_pattern_present {
            continue;
        }

        let reason = rule.reason.clone().unwrap_or_else(|| {
            format!(
                "Trigger precondition {:?} from rule `{}` not enabled at call site",
                rule.required_patterns, rule.id
            )
        });

        let target_policy = if any_receiver_escapes {
            MissingTriggerPolicy::UnderInvestigation
        } else {
            rule.missing_verdict
        };

        match target_policy {
            MissingTriggerPolicy::NotExploitable => {
                result.exploitability = ExploitabilityVerdict::NotExploitable;
                if let Some(stmt) = result.openvex.statements.first_mut() {
                    stmt.status = Status::NotAffected;
                    stmt.justification = Some(Justification::InlineMitigationsAlreadyExist);
                    stmt.impact_statement = Some(reason.clone());
                }
                result.notes = Some(format!(
                    "Downgraded by trigger rule `{}`: {reason}",
                    rule.id
                ));
            }
            MissingTriggerPolicy::UnderInvestigation => {
                if strict_reachability {
                    result.exploitability = ExploitabilityVerdict::Exploitable;
                    if let Some(stmt) = result.openvex.statements.first_mut() {
                        stmt.status = Status::Affected;
                        stmt.justification = None;
                    }
                    result.notes = Some(format!(
                        "Trigger rule `{}` inconclusive (receiver escapes local file; elevated by --strict-reachability): {reason}",
                        rule.id
                    ));
                } else {
                    result.exploitability = ExploitabilityVerdict::UnderInvestigation;
                    if let Some(stmt) = result.openvex.statements.first_mut() {
                        stmt.status = Status::UnderInvestigation;
                        stmt.justification = None;
                    }
                    result.notes = Some(format!(
                        "Trigger rule `{}` inconclusive (receiver configuration escapes local file): {reason}",
                        rule.id
                    ));
                }
            }
        }
    }
}

fn receiver_likely_escapes_local_file(source: &str, call_line: usize, target_method: &str) -> bool {
    let short_method = target_method
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(target_method);
    let Some(line_str) = source.lines().nth(call_line.saturating_sub(1)) else {
        return false;
    };
    let needle = format!(".{short_method}(");
    let Some(pos) = line_str.find(&needle) else {
        return false;
    };
    let recv: String = line_str[..pos]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if recv.is_empty() || line_str[..pos].contains("new ") {
        return false;
    }
    // If the file constructs `recv = new ...` locally, receiver configuration is local;
    // if `recv` only appears in a parameter signature / `@Autowired` / `@Inject`, it escapes.
    let locally_constructed = source.contains(&format!("{recv} = new "))
        || source.contains(&format!("{recv}=new "))
        || source.contains(&format!("{recv} := "));
    if locally_constructed {
        return false;
    }
    source.contains(&format!(" {recv})")) || source.contains(&format!(" {recv},"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink_taint::{SinkCallSite, SinkFirstTaintResult, SinkPathSummary, SinkResolution};
    use crate::vuln_analyze::VULN_ANALYZE_SCHEMA_VERSION;
    use openvex::{Metadata, OpenVex, Statement};
    use tempfile::tempdir;

    #[test]
    fn declarative_rule_downgrades_when_feature_flag_absent_and_respects_escape() {
        let dir = tempdir().unwrap();
        let rules_dir = rgctl_graph::paths::artifact_path(dir.path(), VULN_RULES_DIR);
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(
            rules_dir.join("jackson.yaml"),
            r#"
rules:
  - id: jackson-unwrap-array
    advisories: ["CVE-2022-42004"]
    required_patterns: ["UNWRAP_SINGLE_VALUE_ARRAYS"]
    missing_verdict: not_exploitable
    reason: "Requires UNWRAP_SINGLE_VALUE_ARRAYS on ObjectMapper"
"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("LocalSvc.java"),
            "class LocalSvc {\n  void run(String s) throws Exception {\n    ObjectMapper m = new ObjectMapper();\n    m.readValue(s, Object.class);\n  }\n}\n",
        )
        .unwrap();

        let rules = load_vuln_trigger_rules(dir.path()).unwrap();
        assert_eq!(rules.len(), 1);

        let mut res = VulnAnalyzeResult {
            schema_version: VULN_ANALYZE_SCHEMA_VERSION.into(),
            osv_id: "CVE-2022-42004".into(),
            deps_verdict: "affected_candidate".into(),
            exploitability: ExploitabilityVerdict::Exploitable,
            phases: vec!["triage".into()],
            package: None,
            bundled_presence: None,
            sink_taint: Some(SinkFirstTaintResult {
                schema_version: "1".into(),
                sink: "ObjectMapper.readValue".into(),
                source: "external".into(),
                sink_resolution: SinkResolution::Resolved,
                depth: 8,
                cfg_available: true,
                sink_callers: vec!["LocalSvc.run".into()],
                call_sites: vec![SinkCallSite {
                    caller: "LocalSvc.run".into(),
                    target_method: "ObjectMapper.readValue".into(),
                    file: "LocalSvc.java".into(),
                    line: 4,
                }],
                caller_chains: vec![],
                all_sites_mitigated: false,
                paths: vec![SinkPathSummary {
                    from: "LocalSvc.run (LocalSvc.java:2)".into(),
                    to: "ObjectMapper.readValue (LocalSvc.java:4)".into(),
                    hops: Some(1),
                }],
                honesty: None,
            }),
            openvex: OpenVex {
                metadata: Metadata {
                    context: "https://openvex.dev/ns/v0.2.0".into(),
                    id: "vex:CVE-2022-42004".into(),
                    author: "rgctl".into(),
                    role: "document creator".into(),
                    timestamp: None,
                    version: "1".into(),
                    tooling: None,
                    supplier: None,
                },
                statements: vec![Statement {
                    vulnerability: Some("CVE-2022-42004".into()),
                    vuln_description: None,
                    timestamp: None,
                    products: vec![
                        "pkg:maven/com.fasterxml.jackson.core/jackson-databind@2.13.4".into(),
                    ],
                    subcomponents: vec![],
                    status: Status::Affected,
                    status_notes: None,
                    justification: None,
                    impact_statement: None,
                    action_statement: None,
                    action_statement_timestamp: None,
                }],
            },
            notes: None,
        };

        apply_trigger_rules(dir.path(), &rules, &mut res, false);
        assert_eq!(res.exploitability, ExploitabilityVerdict::NotExploitable);
        assert_eq!(res.openvex.statements[0].status, Status::NotAffected);
        assert_eq!(
            res.openvex.statements[0].justification,
            Some(Justification::InlineMitigationsAlreadyExist)
        );
    }
}
