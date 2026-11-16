//! SARIF 2.1.0 output for `rgctl vuln analyze/scan -f sarif` (task 2.4).
//!
//! Emits valid SARIF 2.1.0 JSON with `codeFlows` / `threadFlows` mapped to exact
//! `file:line` locations from upstream callers through sink call sites.
//!
//! Reference: <https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html>

use serde::Serialize;
use serde_json::Value;

use crate::vuln_analyze::{ExploitabilityVerdict, VulnAnalyzeResult};

/// SARIF 2.1.0 schema URL.
pub const SARIF_SCHEMA: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json";

/// SARIF 2.1.0 root document.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: &'static str,
    pub version: &'static str,
    pub runs: Vec<SarifRun>,
}

/// One tool run.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

/// Tool descriptor.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifTool {
    pub driver: SarifDriver,
}

/// Tool driver / rules.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifDriver {
    pub name: &'static str,
    pub semantic_version: String,
    pub information_uri: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SarifRule>,
}

/// One SARIF rule (one per OSV advisory).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRule {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub short_description: SarifMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_description: Option<SarifMessage>,
    pub help: SarifMessage,
    pub properties: Value,
}

/// A SARIF message string.
#[derive(Debug, Serialize)]
pub struct SarifMessage {
    pub text: String,
}

/// One SARIF result (finding / vulnerability instance).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    pub rule_id: String,
    pub level: &'static str,
    pub message: SarifMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<SarifLocation>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub code_flows: Vec<SarifCodeFlow>,
}

/// A physical location (file + region).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    pub physical_location: SarifPhysicalLocation,
}

/// File + line.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysicalLocation {
    pub artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

#[derive(Debug, Serialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    pub start_line: u32,
}

/// A SARIF codeFlow (one per taint path).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifCodeFlow {
    pub thread_flows: Vec<SarifThreadFlow>,
}

/// A SARIF threadFlow (ordered call-site steps).
#[derive(Debug, Serialize)]
pub struct SarifThreadFlow {
    pub locations: Vec<SarifThreadFlowLocation>,
}

/// One step in a threadFlow.
#[derive(Debug, Serialize)]
pub struct SarifThreadFlowLocation {
    pub location: SarifLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<SarifMessage>,
}

/// Extract `(repo_relative_uri, start_line)` from strings formatted like
/// `"Symbol (path/to/File.java:84)"` or `"path/to/File.java:84"` when valid.
fn parse_file_line_uri(desc: &str) -> Option<(String, u32)> {
    let candidate = if let Some((_, inside)) = desc.rsplit_once('(') {
        inside.trim_end_matches(')').trim()
    } else {
        desc.trim()
    };
    let (file_part, line_part) = candidate.rsplit_once(':')?;
    let line: u32 = line_part.trim().parse().ok()?;
    let uri = file_part.trim();
    if uri.is_empty() || uri.contains(' ') || uri.contains('[') {
        return None;
    }
    Some((uri.to_string(), line))
}

/// Convert a [`VulnAnalyzeResult`] to a SARIF 2.1.0 [`SarifLog`].
///
/// Findings at `exploitable` or `under_investigation` severity are emitted as results.
/// Each `sink_taint.paths[]` entry maps to a `codeFlows[].threadFlows[]` list.
/// Call sites with file + line appear in `results[].locations`.
pub fn analyze_result_to_sarif(result: &VulnAnalyzeResult) -> SarifLog {
    analyze_results_to_sarif(std::slice::from_ref(result))
}

/// Convert multiple [`VulnAnalyzeResult`] entries (from `vuln scan` or directory `vuln analyze`)
/// into a single consolidated SARIF 2.1.0 [`SarifLog`].
pub fn analyze_results_to_sarif(results: &[VulnAnalyzeResult]) -> SarifLog {
    let mut rules = Vec::with_capacity(results.len());
    let mut seen_rules = std::collections::HashSet::new();
    let mut sarif_results = Vec::with_capacity(results.len());

    for result in results {
        let (rule, sarif_result) = single_result_to_sarif_parts(result);
        if seen_rules.insert(rule.id.clone()) {
            rules.push(rule);
        }
        sarif_results.push(sarif_result);
    }

    SarifLog {
        schema: SARIF_SCHEMA,
        version: "2.1.0",
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "rgctl",
                    semantic_version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://rgctl.io",
                    rules,
                },
            },
            results: sarif_results,
        }],
    }
}

fn single_result_to_sarif_parts(result: &VulnAnalyzeResult) -> (SarifRule, SarifResult) {
    let level = match result.exploitability {
        ExploitabilityVerdict::Exploitable => "error",
        ExploitabilityVerdict::UnderInvestigation => "warning",
        ExploitabilityVerdict::NotExploitable | ExploitabilityVerdict::NotAffected => "none",
    };

    let rule = SarifRule {
        id: result.osv_id.clone(),
        name: None,
        short_description: SarifMessage {
            text: result
                .notes
                .clone()
                .unwrap_or_else(|| format!("OSV vulnerability: {}", result.osv_id)),
        },
        full_description: None,
        help: SarifMessage {
            text: format!(
                "See https://osv.dev/vulnerability/{} for details.",
                result.osv_id
            ),
        },
        properties: serde_json::json!({
            "precision": "medium",
            "problem.severity": match result.exploitability {
                ExploitabilityVerdict::Exploitable => "error",
                ExploitabilityVerdict::UnderInvestigation => "warning",
                _ => "note",
            },
        }),
    };

    // Build locations from call_sites if present.
    let locations = result
        .sink_taint
        .as_ref()
        .map(|st| {
            st.call_sites
                .iter()
                .map(|site| SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation {
                            uri: site.file.clone(),
                        },
                        region: Some(SarifRegion {
                            start_line: site.line,
                        }),
                    },
                })
                .collect::<Vec<_>>()
        })
        .filter(|v: &Vec<_>| !v.is_empty());

    // Build codeFlows from call sites + optional upstream caller chains (with valid file URIs only).
    let code_flows: Vec<SarifCodeFlow> = result
        .sink_taint
        .as_ref()
        .map(|st| {
            if !st.call_sites.is_empty() {
                st.call_sites
                    .iter()
                    .map(|site| {
                        let mut steps = Vec::new();
                        // Attach upstream caller chain step if its `from` resolves to `file:line`.
                        for chain in st.caller_chains.iter().chain(st.paths.iter()) {
                            if chain.to.contains(&format!("{}:{}", site.file, site.line))
                                && let Some((up_uri, up_line)) = parse_file_line_uri(&chain.from)
                            {
                                steps.push(SarifThreadFlowLocation {
                                    location: SarifLocation {
                                        physical_location: SarifPhysicalLocation {
                                            artifact_location: SarifArtifactLocation {
                                                uri: up_uri,
                                            },
                                            region: Some(SarifRegion {
                                                start_line: up_line,
                                            }),
                                        },
                                    },
                                    message: Some(SarifMessage {
                                        text: chain.from.clone(),
                                    }),
                                });
                                break;
                            }
                        }
                        steps.push(SarifThreadFlowLocation {
                            location: SarifLocation {
                                physical_location: SarifPhysicalLocation {
                                    artifact_location: SarifArtifactLocation {
                                        uri: site.file.clone(),
                                    },
                                    region: Some(SarifRegion {
                                        start_line: site.line,
                                    }),
                                },
                            },
                            message: Some(SarifMessage {
                                text: format!("{} -> {}", site.caller, site.target_method),
                            }),
                        });
                        SarifCodeFlow {
                            thread_flows: vec![SarifThreadFlow { locations: steps }],
                        }
                    })
                    .collect()
            } else {
                st.paths
                    .iter()
                    .filter_map(|path| {
                        let mut steps = Vec::new();
                        if let Some((from_uri, from_line)) = parse_file_line_uri(&path.from) {
                            steps.push(SarifThreadFlowLocation {
                                location: SarifLocation {
                                    physical_location: SarifPhysicalLocation {
                                        artifact_location: SarifArtifactLocation { uri: from_uri },
                                        region: Some(SarifRegion {
                                            start_line: from_line,
                                        }),
                                    },
                                },
                                message: Some(SarifMessage {
                                    text: path.from.clone(),
                                }),
                            });
                        }
                        if let Some((to_uri, to_line)) = parse_file_line_uri(&path.to) {
                            steps.push(SarifThreadFlowLocation {
                                location: SarifLocation {
                                    physical_location: SarifPhysicalLocation {
                                        artifact_location: SarifArtifactLocation { uri: to_uri },
                                        region: Some(SarifRegion {
                                            start_line: to_line,
                                        }),
                                    },
                                },
                                message: Some(SarifMessage {
                                    text: path.to.clone(),
                                }),
                            });
                        }
                        (!steps.is_empty()).then_some(SarifCodeFlow {
                            thread_flows: vec![SarifThreadFlow { locations: steps }],
                        })
                    })
                    .collect()
            }
        })
        .unwrap_or_default();

    let sarif_result = SarifResult {
        rule_id: result.osv_id.clone(),
        level,
        message: SarifMessage {
            text: format!(
                "{:?}: {} (deps_verdict={})",
                result.exploitability, result.osv_id, result.deps_verdict
            ),
        },
        locations,
        code_flows,
    };

    (rule, sarif_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vuln_analyze::{
        ExploitabilityVerdict, VULN_ANALYZE_SCHEMA_VERSION, VulnAnalyzeResult,
    };
    use openvex::{Metadata, OpenVex, Statement, Status};

    fn minimal_result(osv_id: &str, exploitability: ExploitabilityVerdict) -> VulnAnalyzeResult {
        VulnAnalyzeResult {
            schema_version: VULN_ANALYZE_SCHEMA_VERSION.into(),
            osv_id: osv_id.to_string(),
            deps_verdict: "affected_candidate".into(),
            exploitability,
            phases: vec!["triage".into()],
            package: None,
            bundled_presence: None,
            sink_taint: None,
            openvex: OpenVex {
                metadata: Metadata {
                    context: "https://openvex.dev/ns/v0.2.0".into(),
                    id: format!("vex:{osv_id}"),
                    author: "rgctl".into(),
                    role: "document creator".into(),
                    timestamp: None,
                    version: "1".into(),
                    tooling: None,
                    supplier: None,
                },
                statements: vec![Statement {
                    vulnerability: Some(osv_id.to_string()),
                    vuln_description: None,
                    timestamp: None,
                    products: vec![format!("pkg:maven/com.example/lib@1.0.0")],
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
        }
    }

    #[test]
    fn sarif_exploitable_emits_error_level() {
        let r = minimal_result("CVE-TEST-001", ExploitabilityVerdict::Exploitable);
        let sarif = analyze_result_to_sarif(&r);
        assert_eq!(sarif.version, "2.1.0");
        assert_eq!(sarif.runs[0].results[0].level, "error");
        assert_eq!(sarif.runs[0].results[0].rule_id, "CVE-TEST-001");
    }

    #[test]
    fn sarif_under_investigation_emits_warning() {
        let r = minimal_result("CVE-TEST-002", ExploitabilityVerdict::UnderInvestigation);
        let sarif = analyze_result_to_sarif(&r);
        assert_eq!(sarif.runs[0].results[0].level, "warning");
    }

    #[test]
    fn sarif_not_exploitable_emits_none_level() {
        let r = minimal_result("CVE-TEST-003", ExploitabilityVerdict::NotExploitable);
        let sarif = analyze_result_to_sarif(&r);
        assert_eq!(sarif.runs[0].results[0].level, "none");
    }

    #[test]
    fn sarif_roundtrips_to_valid_json() {
        let r = minimal_result("CVE-TEST-004", ExploitabilityVerdict::Exploitable);
        let sarif = analyze_result_to_sarif(&r);
        let json = serde_json::to_string_pretty(&sarif).unwrap();
        // Must parse back without error.
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["version"].as_str().unwrap(), "2.1.0");
        assert!(v["runs"][0]["results"][0]["ruleId"].as_str().is_some());
    }
}
