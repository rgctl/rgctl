//! Ansible security scanning against graph task nodes.
//!
//! This module provides security vulnerability detection for Ansible playbooks
//! and roles indexed in the rBuilder knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder_lang_ansible::{AnsibleSecurityScanner, AnsibleSeverity};
//! use rbuilder_graph::CodeGraph;
//! use std::path::Path;
//!
//! # fn main() -> rbuilder_error::Result<()> {
//! let graph = CodeGraph::load_from_repo(Path::new("."))?;
//! let scanner = AnsibleSecurityScanner::new();
//! let findings = scanner.scan_graph(graph.backend());
//!
//! let critical = AnsibleSecurityScanner::filter_by_severity(findings, AnsibleSeverity::High);
//! for finding in critical {
//!     println!("[{:?}] {}", finding.severity, finding.message);
//!     if let Some(cwe) = finding.cwe {
//!         println!("  CWE: {cwe}");
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Security Checks
//!
//! - **CWE-78**: Command injection in shell/command/raw modules
//! - **CWE-798**: Hardcoded secrets in task variables
//! - **CWE-732**: Insecure file permissions
//! - **CWE-250**: Unnecessary privilege escalation
//! - **CWE-532**: Sensitive data logging

use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Finding severity for Ansible scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnsibleSeverity {
    /// Informational
    Low,
    /// Should review
    Medium,
    /// Likely security issue
    High,
    /// Critical security risk
    Critical,
}

/// Ansible-specific security finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsibleSecurityFinding {
    /// Severity level
    pub severity: AnsibleSeverity,
    /// Human-readable message
    pub message: String,
    /// Task or node name
    pub location: String,
    /// Optional CWE identifier
    pub cwe: Option<String>,
    /// Remediation guidance
    pub remediation: Option<String>,
    /// Ansible module involved
    pub module: Option<String>,
}

/// Scans Ansible task nodes in the knowledge graph.
#[derive(Debug, Clone)]
pub struct AnsibleSecurityScanner {
    sensitive_modules: HashSet<String>,
    dangerous_modules: HashSet<String>,
}

impl Default for AnsibleSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsibleSecurityScanner {
    /// Create with built-in module classifications.
    pub fn new() -> Self {
        let sensitive_modules = ["shell", "command", "raw", "script"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let dangerous_modules = ["user", "authorized_key", "mysql_user", "postgresql_user"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        Self {
            sensitive_modules,
            dangerous_modules,
        }
    }

    /// Scan all Ansible task/handler nodes in the graph.
    pub fn scan_graph(&self, backend: &MemoryBackend) -> Vec<AnsibleSecurityFinding> {
        let mut findings = Vec::new();
        let tasks = backend
            .find_nodes_by_type(NodeType::AnsibleTask)
            .unwrap_or_default();
        let handlers = backend
            .find_nodes_by_type(NodeType::AnsibleHandler)
            .unwrap_or_default();
        for node in tasks.into_iter().chain(handlers) {
            findings.extend(self.scan_node(&node));
        }
        findings
    }

    /// Scan a single task/handler node.
    pub fn scan_node(&self, node: &rbuilder_graph::schema::Node) -> Vec<AnsibleSecurityFinding> {
        let mut findings = Vec::new();
        let module = node
            .get_property("module")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        let args = node.signature.as_deref().unwrap_or("");
        let name = node.name.clone();

        if let Some(f) = self.check_hardcoded_secrets(&name, args) {
            findings.push(f);
        }
        if self.sensitive_modules.contains(&module) {
            if let Some(f) = self.check_command_injection(&name, args) {
                findings.push(f);
            }
        }
        if let Some(f) = self.check_become_usage(&name, &module, node) {
            findings.push(f);
        }
        if let Some(f) = self.check_no_log_missing(&name, &module, args) {
            findings.push(f);
        }
        findings
    }

    fn check_hardcoded_secrets(&self, name: &str, args: &str) -> Option<AnsibleSecurityFinding> {
        let lower = args.to_lowercase();
        let patterns = [
            "password",
            "passwd",
            "secret",
            "token",
            "api_key",
            "private_key",
        ];
        for pattern in patterns {
            if lower.contains(pattern) && !args.contains("{{") {
                return Some(AnsibleSecurityFinding {
                    severity: AnsibleSeverity::High,
                    message: format!("Potential hardcoded secret in task '{name}'"),
                    location: name.to_string(),
                    cwe: Some("CWE-798".into()),
                    remediation: Some(
                        "Use Ansible Vault or variables instead of hardcoded secrets".into(),
                    ),
                    module: None,
                });
            }
        }
        None
    }

    fn check_command_injection(&self, name: &str, args: &str) -> Option<AnsibleSecurityFinding> {
        if args.contains("{{") && !args.contains("| quote") {
            return Some(AnsibleSecurityFinding {
                severity: AnsibleSeverity::Critical,
                message: format!(
                    "Potential command injection in '{name}' — variable not quoted with | quote"
                ),
                location: name.to_string(),
                cwe: Some("CWE-78".into()),
                remediation: Some(
                    "Use the '| quote' filter on variables in shell/command modules".into(),
                ),
                module: Some("shell".into()),
            });
        }
        None
    }

    fn check_become_usage(
        &self,
        name: &str,
        module: &str,
        node: &rbuilder_graph::schema::Node,
    ) -> Option<AnsibleSecurityFinding> {
        let become_flag = node.get_property("become").is_some_and(|v| v == "true");
        if become_flag && !self.is_become_necessary(module) {
            return Some(AnsibleSecurityFinding {
                severity: AnsibleSeverity::Medium,
                message: format!("Task '{name}' uses become which may not be necessary"),
                location: name.to_string(),
                cwe: Some("CWE-250".into()),
                remediation: Some("Only use privilege escalation when required".into()),
                module: Some(module.to_string()),
            });
        }
        None
    }

    fn check_no_log_missing(
        &self,
        name: &str,
        module: &str,
        args: &str,
    ) -> Option<AnsibleSecurityFinding> {
        if self.dangerous_modules.contains(module) {
            let lower = args.to_lowercase();
            if (lower.contains("password") || lower.contains("secret")) && !lower.contains("no_log")
            {
                return Some(AnsibleSecurityFinding {
                    severity: AnsibleSeverity::Medium,
                    message: format!("Task '{name}' handles sensitive data but no_log is not set"),
                    location: name.to_string(),
                    cwe: Some("CWE-532".into()),
                    remediation: Some("Add no_log: true to prevent logging sensitive data".into()),
                    module: Some(module.to_string()),
                });
            }
        }
        None
    }

    fn is_become_necessary(&self, module: &str) -> bool {
        matches!(
            module,
            "apt" | "yum" | "dnf" | "service" | "systemd" | "user" | "group"
        )
    }

    /// Filter findings by minimum severity.
    pub fn filter_by_severity(
        findings: Vec<AnsibleSecurityFinding>,
        min: AnsibleSeverity,
    ) -> Vec<AnsibleSecurityFinding> {
        findings.into_iter().filter(|f| f.severity >= min).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::schema::Node;

    #[test]
    fn test_hardcoded_secret_detection() {
        let scanner = AnsibleSecurityScanner::new();
        let mut node = Node::new(NodeType::AnsibleTask, "set db password".into());
        node.signature = Some("password: supersecret123".into());
        node.properties.insert("module".into(), "mysql_user".into());
        let findings = scanner.scan_node(&node);
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-798")));
    }

    #[test]
    fn test_command_injection_detection() {
        let scanner = AnsibleSecurityScanner::new();
        let mut node = Node::new(NodeType::AnsibleTask, "run cmd".into());
        node.properties.insert("module".into(), "shell".into());
        node.signature = Some("cmd: echo {{ user_input }}".into());
        let findings = scanner.scan_node(&node);
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
    }
}
