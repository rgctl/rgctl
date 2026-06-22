//! Puppet security scanning against graph resource nodes.
//!
//! This module provides security vulnerability detection for Puppet modules
//! and manifests indexed in the rBuilder knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder_lang_puppet::{PuppetSecurityScanner, PuppetSeverity};
//! use rbuilder_graph::CodeGraph;
//! use std::path::Path;
//!
//! # fn main() -> rbuilder_error::Result<()> {
//! let graph = CodeGraph::load_from_repo(Path::new("."))?;
//! let scanner = PuppetSecurityScanner::new();
//! let findings = scanner.scan_graph(graph.backend());
//!
//! let high = PuppetSecurityScanner::filter_by_severity(findings, PuppetSeverity::High);
//! for finding in high {
//!     println!("[{:?}] {}", finding.severity, finding.message);
//! }
//! # Ok(())
//! # }
//! ```

use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Finding severity for Puppet scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PuppetSeverity {
    /// Informational
    Low,
    /// Should review
    Medium,
    /// Likely security issue
    High,
    /// Critical security risk
    Critical,
}

/// Puppet-specific security finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetSecurityFinding {
    /// Severity level
    pub severity: PuppetSeverity,
    /// Human-readable message
    pub message: String,
    /// Resource or node name
    pub location: String,
    /// Optional CWE identifier
    pub cwe: Option<String>,
    /// Remediation guidance
    pub remediation: Option<String>,
    /// Puppet resource type
    pub resource_type: Option<String>,
}

/// Scans Puppet resource nodes in the knowledge graph.
#[derive(Debug, Clone)]
pub struct PuppetSecurityScanner {
    dangerous_resources: HashSet<String>,
}

impl Default for PuppetSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl PuppetSecurityScanner {
    /// Create with built-in resource classifications.
    pub fn new() -> Self {
        let dangerous_resources = ["exec"].iter().map(|s| s.to_string()).collect();
        Self {
            dangerous_resources,
        }
    }

    /// Scan all Puppet resource nodes in the graph.
    pub fn scan_graph(&self, backend: &MemoryBackend) -> Vec<PuppetSecurityFinding> {
        let resources = backend
            .find_nodes_by_type(NodeType::PuppetResource)
            .unwrap_or_default();
        resources
            .iter()
            .flat_map(|node| self.scan_node(node))
            .collect()
    }

    /// Scan a single resource node.
    pub fn scan_node(&self, node: &rbuilder_graph::schema::Node) -> Vec<PuppetSecurityFinding> {
        let mut findings = Vec::new();
        let resource_type = node
            .get_property("resource_type")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let props = node.signature.as_deref().unwrap_or("");
        let name = node.name.clone();

        if let Some(f) = self.check_hardcoded_secrets(&name, props) {
            findings.push(f);
        }
        if self.dangerous_resources.contains(&resource_type) {
            if let Some(f) = self.check_command_injection(&name, &resource_type, props) {
                findings.push(f);
            }
        }
        if resource_type == "file" {
            if let Some(f) = self.check_file_permissions(&name, &resource_type, props) {
                findings.push(f);
            }
        }
        findings
    }

    fn check_hardcoded_secrets(&self, name: &str, props: &str) -> Option<PuppetSecurityFinding> {
        let lower = props.to_lowercase();
        for pattern in ["password", "secret", "token", "api_key", "private_key"] {
            if lower.contains(pattern) && !props.contains("lookup(") && !props.contains("$") {
                return Some(PuppetSecurityFinding {
                    severity: PuppetSeverity::High,
                    message: format!("Potential hardcoded secret in resource '{name}'"),
                    location: name.to_string(),
                    cwe: Some("CWE-798".into()),
                    remediation: Some("Use Hiera lookup() or encrypted data instead".into()),
                    resource_type: None,
                });
            }
        }
        None
    }

    fn check_command_injection(
        &self,
        name: &str,
        resource_type: &str,
        props: &str,
    ) -> Option<PuppetSecurityFinding> {
        if props.contains('$') && !props.contains("shellquote") {
            return Some(PuppetSecurityFinding {
                severity: PuppetSeverity::Critical,
                message: format!(
                    "Potential command injection in {resource_type} resource '{name}'"
                ),
                location: name.to_string(),
                cwe: Some("CWE-78".into()),
                remediation: Some("Use shellquote() for variable interpolation in commands".into()),
                resource_type: Some(resource_type.to_string()),
            });
        }
        None
    }

    fn check_file_permissions(
        &self,
        name: &str,
        resource_type: &str,
        props: &str,
    ) -> Option<PuppetSecurityFinding> {
        let lower = props.to_lowercase();
        if lower.contains("0666") || lower.contains("0777") || lower.contains("mode=666") {
            return Some(PuppetSecurityFinding {
                severity: PuppetSeverity::Medium,
                message: format!("Insecure file permissions in {resource_type} resource '{name}'"),
                location: name.to_string(),
                cwe: Some("CWE-732".into()),
                remediation: Some("Use restrictive file modes (e.g. 0644 or 0600)".into()),
                resource_type: Some(resource_type.to_string()),
            });
        }
        None
    }

    /// Filter findings by minimum severity.
    pub fn filter_by_severity(
        findings: Vec<PuppetSecurityFinding>,
        min: PuppetSeverity,
    ) -> Vec<PuppetSecurityFinding> {
        findings.into_iter().filter(|f| f.severity >= min).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::Node;

    #[test]
    fn test_command_injection_detection() {
        let scanner = PuppetSecurityScanner::new();
        let mut node = Node::new(NodeType::PuppetResource, "run cmd".into());
        node.properties
            .insert("resource_type".into(), "exec".into());
        node.signature = Some("command=/bin/sh -c echo $hostname".into());
        let findings = scanner.scan_node(&node);
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
    }
}
