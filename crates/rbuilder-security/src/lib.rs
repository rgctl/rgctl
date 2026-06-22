//! Security analysis for rBuilder

pub mod analyzer;
pub mod cve_patterns;

pub use analyzer::{SecurityAnalyzer, SecurityVulnerability};
pub use cve_patterns::{default_cwe_patterns, CwePattern};
