//! Security analysis for rgctl.
//!
//! - Existing: CWE pattern analyzer (to be unified with declarative taint rules — issue #16).
//! - OSV supply-chain: triage via the [`osv`](https://crates.io/crates/osv) schema crate.
//!   OpenVEX emission (later) should use [`openvex`](https://crates.io/crates/openvex).
//!
//! Policy: [AGENTS.md](../../../AGENTS.md) ·
//! [openspec/changes/_shared/starting-context.md](../../../openspec/changes/_shared/starting-context.md).
//! OSV/deps/bundled JAR scans MUST NOT run on default discover.

pub mod adapters;
pub mod analyzer;
pub mod cve_patterns;
pub mod deps_check;
pub mod error;
pub mod osv;
pub mod package_resolve;
pub mod sink_taint;
pub mod version;
pub mod vuln_analyze;

pub use adapters::{BundledScanOpts, DepHit, scan_jars, scan_manifests, scan_node_modules};
pub use analyzer::{SecurityAnalyzer, SecurityVulnerability};
pub use cve_patterns::{CwePattern, default_cwe_patterns};
pub use deps_check::{DepsCheckOpts, DepsCheckResult, DepsMatch, DepsVerdict, deps_check};
pub use error::SecurityError;
pub use osv::{
    OsvPackageTriage, OsvTriage, triage_from_vulnerability, triage_osv_bytes, triage_osv_path,
};
pub use package_resolve::{PackageResolution, PackageResolveError, resolve_package};
pub use sink_taint::{
    SinkCallSite, SinkFirstTaintResult, SinkPathSummary, SinkResolution, build_sink_first_result,
    build_sink_first_result_with_sites, missing_cfg_error_message, rules_with_osv_overlays,
};
pub use version::{VersionEngine, engine_for_ecosystem};
pub use vuln_analyze::{
    BundledPresence, ExploitabilityVerdict, VULN_ANALYZE_SCHEMA_VERSION, VulnAnalyzeOpts,
    VulnAnalyzeResult, affected_methods_from_osv_json, vuln_analyze,
};
