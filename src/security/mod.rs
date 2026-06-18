//! Security analysis: CWE patterns and vulnerability reporting (Phase 13.5).

pub mod analyzer;
pub mod ansible;
pub mod chef;
pub mod cve_patterns;

pub use analyzer::{SecurityAnalyzer, SecurityVulnerability};
pub use ansible::{AnsibleSecurityFinding, AnsibleSecurityScanner, AnsibleSeverity};
pub use chef::{ChefSecurityFinding, ChefSecurityScanner, ChefSeverity};
pub use cve_patterns::{default_cwe_patterns, CwePattern};
