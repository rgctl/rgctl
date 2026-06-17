//! Security vulnerability reporting (Phase 13.5).

use crate::analysis::pdg::ProgramDependenceGraph;
use crate::analysis::taint::{TaintFlow, TaintSink, TaintSource};
use crate::security::cve_patterns::{default_cwe_patterns, CwePattern};
use regex::Regex;

/// A reported security vulnerability.
#[derive(Debug, Clone)]
pub struct SecurityVulnerability {
    /// CWE identifier.
    pub cwe_id: String,
    /// CWE name.
    pub cwe_name: String,
    /// Severity 1–10.
    pub severity: u8,
    /// Underlying taint flow.
    pub taint_flow: TaintFlow,
    /// Source line (1-based).
    pub source_line: usize,
    /// Sink line (1-based).
    pub sink_line: usize,
    /// Remediation guidance.
    pub recommendation: String,
}

/// Matches taint flows against CWE patterns.
#[derive(Debug, Clone)]
pub struct SecurityAnalyzer {
    cwe_patterns: Vec<CwePattern>,
}

impl SecurityAnalyzer {
    /// Create with built-in CWE patterns.
    pub fn new() -> Self {
        Self {
            cwe_patterns: default_cwe_patterns(),
        }
    }

    /// Analyze vulnerable taint flows.
    pub fn analyze(
        &self,
        flows: Vec<TaintFlow>,
        pdg: &ProgramDependenceGraph,
        source: &str,
    ) -> Vec<SecurityVulnerability> {
        let mut vulnerabilities = Vec::new();
        for flow in flows {
            if !flow.is_vulnerable() {
                continue;
            }
            for pattern in &self.cwe_patterns {
                if self.matches_cwe(&flow, pattern, source) {
                    let source_line = pdg
                        .nodes
                        .get(&flow.source)
                        .map(|n| n.statement.line)
                        .unwrap_or(0);
                    let sink_line = pdg
                        .nodes
                        .get(&flow.sink)
                        .map(|n| n.statement.line)
                        .unwrap_or(0);
                    vulnerabilities.push(SecurityVulnerability {
                        cwe_id: pattern.cwe_id.clone(),
                        cwe_name: pattern.name.clone(),
                        severity: pattern.severity,
                        taint_flow: flow.clone(),
                        source_line,
                        sink_line,
                        recommendation: self.generate_recommendation(pattern),
                    });
                    break;
                }
            }
        }
        vulnerabilities
    }

    fn matches_cwe(&self, flow: &TaintFlow, pattern: &CwePattern, source: &str) -> bool {
        if pattern.cwe_id == "CWE-798" {
            return pattern.source_patterns.iter().any(|p| {
                Regex::new(p)
                    .map(|re| re.is_match(source))
                    .unwrap_or(false)
            });
        }

        let source_match = pattern.source_patterns.is_empty()
            || self.flow_matches_source(flow, source)
            || pattern.source_patterns.iter().any(|p| {
                Regex::new(p)
                    .map(|re| re.is_match(source))
                    .unwrap_or(false)
            });

        let sink_match = pattern.sink_patterns.is_empty()
            || pattern.sink_patterns.iter().any(|p| {
                Regex::new(p)
                    .map(|re| re.is_match(source))
                    .unwrap_or(false)
            })
            || self.flow_matches_sink(flow);

        source_match && sink_match
    }

    fn flow_matches_source(&self, flow: &TaintFlow, source: &str) -> bool {
        match flow.source_type {
            TaintSource::HttpParameter => {
                source.contains("request.") || source.contains("req.")
            }
            TaintSource::CommandLineArg => {
                source.contains("sys.argv") || source.contains("process.argv")
            }
            TaintSource::EnvironmentVar => {
                source.contains("environ") || source.contains("process.env")
            }
            TaintSource::FileInput => source.contains("open(") || source.contains("readFile"),
            _ => true,
        }
    }

    fn flow_matches_sink(&self, flow: &TaintFlow) -> bool {
        matches!(
            flow.sink_type,
            TaintSink::SqlQuery
                | TaintSink::ShellCommand
                | TaintSink::HtmlRender
                | TaintSink::CodeEval
        )
    }

    fn generate_recommendation(&self, pattern: &CwePattern) -> String {
        match pattern.cwe_id.as_str() {
            "CWE-89" => {
                "Use parameterized queries or prepared statements instead of string concatenation."
                    .into()
            }
            "CWE-79" => "Escape HTML entities before rendering user input.".into(),
            "CWE-78" => {
                "Use shell escape functions or avoid shell execution entirely.".into()
            }
            "CWE-22" => "Validate file paths and restrict to allowed directories.".into(),
            "CWE-798" => "Load secrets from environment variables or a secret manager.".into(),
            _ => "Review and sanitize input before use.".into(),
        }
    }
}

impl Default for SecurityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg_builder::build_cfg_for_function;
    use crate::analysis::pdg::ProgramDependenceGraph;
    use crate::analysis::taint::TaintAnalyzer;

    #[cfg(feature = "lang-python")]
    #[test]
    fn test_security_analyzer_sql_injection() {
        let code = r#"
def handle(request):
    u = request.GET['user']
    cursor.execute(f"SELECT * FROM t WHERE u='{u}'")
"#;
        let cfg = build_cfg_for_function("python", code, "handle").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut taint = TaintAnalyzer::new(&pdg, &cfg);
        taint.detect_patterns("python");
        let flows = taint.vulnerable_flows();
        let vulns = SecurityAnalyzer::new().analyze(flows, &pdg, code);
        assert!(vulns.iter().any(|v| v.cwe_id == "CWE-89"));
    }
}
