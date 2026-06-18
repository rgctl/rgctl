//! Chef security scanning against graph resource nodes.

use crate::graph::backend::MemoryBackend;
use crate::graph::schema::NodeType;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Finding severity for Chef scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChefSeverity {
    /// Informational
    Low,
    /// Should review
    Medium,
    /// Likely security issue
    High,
    /// Critical security risk
    Critical,
}

/// Chef-specific security finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChefSecurityFinding {
    /// Severity level
    pub severity: ChefSeverity,
    /// Human-readable message
    pub message: String,
    /// Resource or node name
    pub location: String,
    /// Optional CWE identifier
    pub cwe: Option<String>,
    /// Remediation guidance
    pub remediation: Option<String>,
    /// Chef resource type
    pub resource_type: Option<String>,
}

/// Scans Chef resource nodes in the knowledge graph.
#[derive(Debug, Clone)]
pub struct ChefSecurityScanner {
    dangerous_resources: HashSet<String>,
}

impl Default for ChefSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ChefSecurityScanner {
    /// Create with built-in resource classifications.
    pub fn new() -> Self {
        let dangerous_resources = ["execute", "bash", "script", "ruby_block"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            dangerous_resources,
        }
    }

    /// Scan all Chef resource nodes in the graph.
    pub fn scan_graph(&self, backend: &MemoryBackend) -> Vec<ChefSecurityFinding> {
        let resources = backend
            .find_nodes_by_type(NodeType::ChefResource)
            .unwrap_or_default();
        resources
            .iter()
            .flat_map(|node| self.scan_node(node))
            .collect()
    }

    /// Scan a single resource node.
    pub fn scan_node(&self, node: &crate::graph::schema::Node) -> Vec<ChefSecurityFinding> {
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
        if resource_type == "file" || resource_type == "template" {
            if let Some(f) = self.check_file_permissions(&name, &resource_type, props) {
                findings.push(f);
            }
        }
        findings
    }

    fn check_hardcoded_secrets(&self, name: &str, props: &str) -> Option<ChefSecurityFinding> {
        let lower = props.to_lowercase();
        for pattern in ["password", "secret", "token", "api_key", "private_key"] {
            if lower.contains(pattern) && !props.contains("node[") && !props.contains("#{") {
                return Some(ChefSecurityFinding {
                    severity: ChefSeverity::High,
                    message: format!("Potential hardcoded secret in resource '{name}'"),
                    location: name.to_string(),
                    cwe: Some("CWE-798".into()),
                    remediation: Some(
                        "Use Chef encrypted data bags or node attributes".into(),
                    ),
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
    ) -> Option<ChefSecurityFinding> {
        if props.contains("#{") && !props.contains("Shellwords.escape") {
            return Some(ChefSecurityFinding {
                severity: ChefSeverity::Critical,
                message: format!(
                    "Potential command injection in {resource_type} resource '{name}'"
                ),
                location: name.to_string(),
                cwe: Some("CWE-78".into()),
                remediation: Some(
                    "Use Shellwords.escape for variable interpolation in commands".into(),
                ),
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
    ) -> Option<ChefSecurityFinding> {
        let lower = props.to_lowercase();
        if lower.contains("0666") || lower.contains("0777") || lower.contains("mode 666") {
            return Some(ChefSecurityFinding {
                severity: ChefSeverity::Medium,
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
        findings: Vec<ChefSecurityFinding>,
        min: ChefSeverity,
    ) -> Vec<ChefSecurityFinding> {
        findings
            .into_iter()
            .filter(|f| f.severity >= min)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::Node;

    #[test]
    fn test_command_injection_detection() {
        let scanner = ChefSecurityScanner::new();
        let mut node = Node::new(NodeType::ChefResource, "run cmd".into());
        node.properties
            .insert("resource_type".into(), "execute".into());
        node.signature = Some("command #{user_input}".into());
        let findings = scanner.scan_node(&node);
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
    }
}
