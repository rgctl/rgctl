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
pub mod dep_stubs;
pub mod deps_check;
pub mod error;
pub mod fetch;
pub mod osv;
pub mod package_resolve;
pub mod sarif;
pub mod sink_taint;
pub mod threat_model;
pub mod version;
pub mod vuln_analyze;
pub mod vuln_rules;

pub use adapters::{
    BundledScanOpts, DepHit, scan_jars, scan_lockfiles, scan_manifests, scan_node_modules,
    scan_repo_dependencies,
};
pub use analyzer::{SecurityAnalyzer, SecurityVulnerability};
pub use cve_patterns::{CwePattern, default_cwe_patterns};
pub use dep_stubs::{
    DEP_STUBS_FILE, DepCallStub, DepStubsIndex, TransitiveStubHit, build_dep_stubs,
    find_transitive_stub_paths, load_or_build_dep_stubs, parse_jvm_class_bytes, read_dep_stubs,
};
pub use deps_check::{
    DepsCheckOpts, DepsCheckResult, DepsMatch, DepsVerdict, deps_check,
    deps_check_against_inventory, deps_check_for_triage,
};
pub use error::SecurityError;
pub use fetch::{
    AdvisorySourceOpts, DEFAULT_OSV_API_URL, LoadedAdvisory, MAX_QUERYBATCH_SIZE, OSV_API_URL_ENV,
    OSV_CACHE_REL, OsvFetchOpts, OsvFetchSummary, collect_full_inventory, fetch_osv_for_inventory,
    fetch_osv_for_repo, load_local_osv_path, parse_sbom_file, parse_scanner_report,
    resolve_advisories_and_inventory, to_osv_api_ecosystem,
};
pub use osv::{
    OsvPackageTriage, OsvTriage, SymbolCoverage, VersionInterval, VersionUpperBound,
    triage_from_vulnerability, triage_osv_bytes, triage_osv_path,
};
pub use package_resolve::{
    PackageResolution, PackageResolveError, resolve_package, resolve_package_with_ecosystem,
    structural_only_language_honesty,
};
pub use sarif::{SarifLog, analyze_result_to_sarif, analyze_results_to_sarif};
pub use sink_taint::{
    SinkCallSite, SinkFirstTaintResult, SinkPathSummary, SinkResolution, build_sink_first_result,
    build_sink_first_result_with_sites, missing_cfg_error_message, rules_with_osv_overlays,
};
pub use threat_model::{
    AssetEvidence, AssetKind, ConfidenceLevel, DEFAULT_MAX_LAZY_PDG_FUNCTIONS, DiscoveredPdgSink,
    EntryPointEvidence, EntryPointKind, EvidenceProvenance, EvidenceRecord, FunctionPdgReport,
    FunctionPdgRequest, LimitationNote, PdgSinkStatus, ReachableVulnEvidence, ReviewStatus,
    SinkEvidence, StrideCategory, THREAT_MODEL_SCHEMA_VERSION, ThreatCandidate, ThreatDataFlow,
    ThreatModelBuildOpts, ThreatModelReport, ThreatModelSummary, TrustBoundaryCandidate,
    TrustBoundaryKind, build_threat_model, classify_callee_sanitizer, classify_callee_sink,
    render_threat_model_markdown, render_threat_model_mermaid,
};
pub use version::{VersionEngine, engine_for_ecosystem};
pub use vuln_analyze::{
    BundledPresence, ExploitabilityVerdict, VULN_ANALYZE_SCHEMA_VERSION, VulnAnalyzeOpts,
    VulnAnalyzeResult, VulnScanCounts, VulnScanResult, affected_methods_from_osv_json, build_purl,
    build_vuln_scan_result, vuln_analyze,
};
pub use vuln_rules::{
    MissingTriggerPolicy, VULN_RULES_DIR, VulnTriggerRule, apply_trigger_rules,
    load_vuln_trigger_rules,
};
