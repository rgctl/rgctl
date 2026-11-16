//! OSV triage — parse with the stable [`osv`](https://crates.io/crates/osv) schema crate,
//! then project an agent-facing JSON envelope.
//!
//! Policy: [AGENTS.md](../../../../AGENTS.md) · starting context:
//! [openspec/changes/_shared/starting-context.md](../../../../openspec/changes/_shared/starting-context.md).
//! This module does **not** run on the discover hot path.
//!
//! When emitting OpenVEX later, prefer the [`openvex`](https://crates.io/crates/openvex) types crate
//! rather than hand-rolled VEX JSON.

use osv::schema::{Event, Vulnerability};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::error::SecurityError;

/// Whether an advisory provides symbol-level sink metadata.
///
/// Used by `decide_verdict` to avoid false-negative `NotExploitable` on advisories
/// that simply lack `affected_methods` / `imports[].symbols`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "coverage")]
pub enum SymbolCoverage {
    /// Advisory names at least one specific sink symbol (method / function / class).
    Explicit {
        /// All extracted sink symbol names across all ecosystem conventions.
        symbols: Vec<String>,
    },
    /// Advisory has no symbol-level metadata; verdict must not be `not_exploitable`
    /// unless the package is wholly absent from the dependency graph.
    Unspecified,
}

impl SymbolCoverage {
    /// Return the extracted symbols if explicit, or an empty slice.
    pub fn symbols(&self) -> &[String] {
        match self {
            SymbolCoverage::Explicit { symbols } => symbols,
            SymbolCoverage::Unspecified => &[],
        }
    }

    pub fn is_explicit(&self) -> bool {
        matches!(self, SymbolCoverage::Explicit { .. })
    }
}

/// One vulnerable version interval from an OSV `ranges[]` entry.
///
/// OSV supports both exclusive `fixed` upper bounds and inclusive `last_affected` bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionInterval {
    /// Lower bound (inclusive). `None` means "from the beginning".
    pub introduced: Option<String>,
    /// Upper bound type and value.
    pub upper: VersionUpperBound,
}

/// Upper bound of a vulnerable version interval.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "version")]
pub enum VersionUpperBound {
    /// Exclusive fixed upper bound: `version < fixed` is vulnerable.
    Fixed(String),
    /// Inclusive last_affected upper bound: `version <= last_affected` is vulnerable.
    LastAffected(String),
    /// No upper bound (open-ended vulnerable range).
    Open,
}

/// Normalized OSV triage payload (agent JSON contract).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OsvTriage {
    pub schema_version: u32,
    pub command: &'static str,
    pub id: String,
    pub aliases: Vec<String>,
    pub summary: Option<String>,
    pub packages: Vec<OsvPackageTriage>,
    /// Honesty: OSV `versions[]` is not authoritative when `ranges` exist (backport series).
    pub versions_array_not_authoritative: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OsvPackageTriage {
    pub ecosystem: String,
    pub name: String,
    pub purl: Option<String>,
    /// All vulnerable version intervals (from `affected[].ranges[]`).
    /// Use this for multi-branch range checks via `VersionEngine::in_any_vulnerable_range`.
    pub ranges: Vec<VersionInterval>,
    /// Exact listed versions (fallback when `ranges` is empty).
    pub listed_versions: Vec<String>,
    /// Symbol coverage metadata (Explicit or Unspecified).
    pub symbol_coverage: SymbolCoverage,
    /// All affected method/function names (extracted from `symbol_coverage`).
    pub affected_methods: Vec<String>,
    /// All affected class names.
    pub affected_classes: Vec<String>,

    // --- Backward-compatible single-range accessors (derived from first range) ---
    /// First range's introduced bound (backward compat; prefer `ranges`).
    pub introduced: Option<String>,
    /// First range's fixed bound (backward compat; prefer `ranges`).
    pub fixed: Option<String>,
}

/// Parse OSV JSON bytes via `osv::schema::Vulnerability` (with extended ecosystem fallback for
/// Tier-1 ecosystems added after `osv` 0.3.0 such as `SwiftURL`, `Conan`/`ConanCenter`, `vcpkg`,
/// `Puppet`, and `LuaRocks`), then normalize for agents.
pub fn triage_osv_bytes(bytes: &[u8]) -> Result<OsvTriage, SecurityError> {
    match serde_json::from_slice::<Vulnerability>(bytes) {
        Ok(doc) => Ok(triage_from_vulnerability(doc)),
        Err(primary_err) => {
            let msg = primary_err.to_string();
            if msg.contains("Ecosystem")
                && let Ok(val) = serde_json::from_slice::<Value>(bytes)
            {
                return triage_osv_value(&val);
            }
            Err(SecurityError::OsvParse(msg))
        }
    }
}

fn triage_osv_value(doc: &Value) -> Result<OsvTriage, SecurityError> {
    let id = doc
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| SecurityError::OsvParse("missing `id` in OSV JSON".into()))?
        .to_string();
    let _modified = doc
        .get("modified")
        .and_then(|v| v.as_str())
        .ok_or_else(|| SecurityError::OsvParse("missing `modified` in OSV JSON".into()))?;
    let aliases: Vec<String> = doc
        .get("aliases")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let summary = doc
        .get("summary")
        .and_then(|s| s.as_str())
        .map(str::to_string);

    let mut packages = Vec::new();
    let mut any_versions = false;
    let mut any_ranges = false;

    for aff in doc
        .get("affected")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
    {
        let listed: Vec<String> = aff
            .get("versions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if !listed.is_empty() {
            any_versions = true;
        }

        let mut intervals = Vec::new();
        for range in aff
            .get("ranges")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            any_ranges = true;
            let mut pending_intro: Option<Option<String>> = None;
            for ev in range
                .get("events")
                .and_then(|e| e.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(v) = ev.get("introduced").and_then(|s| s.as_str()) {
                    if let Some(prev_intro) = pending_intro.take() {
                        intervals.push(VersionInterval {
                            introduced: prev_intro,
                            upper: VersionUpperBound::Open,
                        });
                    }
                    pending_intro = Some(Some(v.to_string()));
                } else if let Some(v) = ev.get("fixed").and_then(|s| s.as_str()) {
                    let intro = pending_intro.take().flatten();
                    intervals.push(VersionInterval {
                        introduced: intro,
                        upper: VersionUpperBound::Fixed(v.to_string()),
                    });
                } else if let Some(v) = ev.get("last_affected").and_then(|s| s.as_str()) {
                    let intro = pending_intro.take().flatten();
                    intervals.push(VersionInterval {
                        introduced: intro,
                        upper: VersionUpperBound::LastAffected(v.to_string()),
                    });
                }
            }
            if let Some(intro) = pending_intro.take() {
                intervals.push(VersionInterval {
                    introduced: intro,
                    upper: VersionUpperBound::Open,
                });
            }
        }

        let (bc_introduced, bc_fixed) = if let Some(first) = intervals.first() {
            let fix = match &first.upper {
                VersionUpperBound::Fixed(v) => Some(v.clone()),
                _ => None,
            };
            (first.introduced.clone(), fix)
        } else {
            (None, None)
        };

        let (affected_methods, affected_classes, symbol_coverage) =
            extract_ecosystem_specific(aff.get("ecosystem_specific"));

        let (ecosystem, name, purl) = match aff.get("package") {
            Some(pkg) => (
                pkg.get("ecosystem")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                pkg.get("name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                pkg.get("purl").and_then(|s| s.as_str()).map(str::to_string),
            ),
            None => ("unknown".into(), String::new(), None),
        };

        packages.push(OsvPackageTriage {
            ecosystem,
            name,
            purl,
            ranges: intervals,
            listed_versions: listed,
            symbol_coverage,
            affected_methods,
            affected_classes,
            introduced: bc_introduced,
            fixed: bc_fixed,
        });
    }

    Ok(OsvTriage {
        schema_version: 1,
        command: "vuln triage",
        id,
        aliases,
        summary,
        packages,
        versions_array_not_authoritative: any_versions && any_ranges,
    })
}

/// Load and triage an OSV JSON file.
pub fn triage_osv_path(path: &Path) -> Result<OsvTriage, SecurityError> {
    let bytes = fs::read(path).map_err(|e| SecurityError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    triage_osv_bytes(&bytes)
}

/// Project a parsed [`Vulnerability`] into the agent triage envelope.
pub fn triage_from_vulnerability(doc: Vulnerability) -> OsvTriage {
    let mut packages = Vec::new();
    let mut any_versions = false;
    let mut any_ranges = false;

    for aff in doc.affected.unwrap_or_default() {
        let listed = aff.versions.clone().unwrap_or_default();
        if !listed.is_empty() {
            any_versions = true;
        }

        // Collect all version intervals (multi-range support, task 1.2).
        let mut intervals: Vec<VersionInterval> = Vec::new();
        for range in aff.ranges.unwrap_or_default() {
            any_ranges = true;
            let mut pending_intro: Option<Option<String>> = None;

            for ev in &range.events {
                match ev {
                    Event::Introduced(v) => {
                        // Flush any previous open-ended introduced event before starting a new one.
                        if let Some(prev_intro) = pending_intro.take() {
                            intervals.push(VersionInterval {
                                introduced: prev_intro,
                                upper: VersionUpperBound::Open,
                            });
                        }
                        pending_intro = Some(Some(v.clone()));
                    }
                    Event::Fixed(v) => {
                        let intro = pending_intro.take().flatten();
                        intervals.push(VersionInterval {
                            introduced: intro,
                            upper: VersionUpperBound::Fixed(v.clone()),
                        });
                    }
                    Event::LastAffected(v) => {
                        let intro = pending_intro.take().flatten();
                        intervals.push(VersionInterval {
                            introduced: intro,
                            upper: VersionUpperBound::LastAffected(v.clone()),
                        });
                    }
                    _ => {}
                }
            }
            // Flush trailing open-ended introduced interval (e.g. unpatched CVE).
            if let Some(intro) = pending_intro.take() {
                intervals.push(VersionInterval {
                    introduced: intro,
                    upper: VersionUpperBound::Open,
                });
            }
        }

        // Backward-compat single pair derived from first interval (task 1.2).
        let (bc_introduced, bc_fixed) = if let Some(first) = intervals.first() {
            let intro = first.introduced.clone();
            let fix = match &first.upper {
                VersionUpperBound::Fixed(v) => Some(v.clone()),
                _ => None,
            };
            (intro, fix)
        } else {
            (None, None)
        };

        // Extract sink symbols across all ecosystem conventions (task 1.1).
        let (affected_methods, affected_classes, symbol_coverage) =
            extract_ecosystem_specific(aff.ecosystem_specific.as_ref());

        let (ecosystem, name, purl) = match aff.package {
            Some(pkg) => (ecosystem_label(&pkg.ecosystem), pkg.name, pkg.purl),
            None => ("unknown".into(), String::new(), None),
        };
        packages.push(OsvPackageTriage {
            ecosystem,
            name,
            purl,
            ranges: intervals,
            listed_versions: listed,
            symbol_coverage,
            affected_methods,
            affected_classes,
            introduced: bc_introduced,
            fixed: bc_fixed,
        });
    }

    OsvTriage {
        schema_version: 1,
        command: "vuln triage",
        id: doc.id,
        aliases: doc.aliases.unwrap_or_default(),
        summary: doc.summary,
        packages,
        versions_array_not_authoritative: any_versions && any_ranges,
    }
}

/// Extract sink symbols from `ecosystem_specific` across all standard OSV conventions.
///
/// Conventions handled (task 1.1):
/// - **JVM/GHSA**: `affected_methods`, `affected_classes`, `symbols`
/// - **Go**: `imports[].symbols` (each import entry is `{path, symbols: []}`)
/// - **RustSec**: `affects.functions`
///
/// Returns `(methods, classes, SymbolCoverage)`.
fn extract_ecosystem_specific(v: Option<&Value>) -> (Vec<String>, Vec<String>, SymbolCoverage) {
    let Some(obj) = v.and_then(|x| x.as_object()) else {
        return (Vec::new(), Vec::new(), SymbolCoverage::Unspecified);
    };

    let str_arr = |val: &Value| -> Vec<String> {
        val.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    // JVM/GHSA: `affected_methods`, `symbols`
    let mut methods: Vec<String> = obj.get("affected_methods").map(str_arr).unwrap_or_default();

    // JVM/GHSA: `symbols` (some advisories use this key instead of / in addition to `affected_methods`)
    for s in obj.get("symbols").map(str_arr).unwrap_or_default() {
        if !methods.contains(&s) {
            methods.push(s);
        }
    }

    // JVM/GHSA: `affected_classes`
    let classes: Vec<String> = obj.get("affected_classes").map(str_arr).unwrap_or_default();

    // Go convention: `imports[].symbols`
    if let Some(imports) = obj.get("imports").and_then(|x| x.as_array()) {
        for import in imports {
            for sym in import
                .get("symbols")
                .and_then(|s| s.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(s) = sym.as_str() {
                    let s = s.to_string();
                    if !methods.contains(&s) {
                        methods.push(s);
                    }
                }
            }
        }
    }

    // RustSec convention: `affects.functions`
    if let Some(fns) = obj
        .get("affects")
        .and_then(|a| a.get("functions"))
        .and_then(|f| f.as_array())
    {
        for f in fns {
            if let Some(s) = f.as_str() {
                let s = s.to_string();
                if !methods.contains(&s) {
                    methods.push(s);
                }
            }
        }
    }

    let all_symbols: Vec<String> = methods.iter().chain(classes.iter()).cloned().collect();

    let coverage = if all_symbols.is_empty() {
        SymbolCoverage::Unspecified
    } else {
        SymbolCoverage::Explicit {
            symbols: all_symbols,
        }
    };

    (methods, classes, coverage)
}

fn ecosystem_label(eco: &osv::schema::Ecosystem) -> String {
    serde_json::to_value(eco)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{eco:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triage_jackson_fixture() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/osv/jackson-CVE-2022-42004.json");
        let t = triage_osv_path(&p).expect("triage");
        assert!(t.aliases.iter().any(|a| a.contains("CVE-2022-42004")));
        assert_eq!(t.packages.len(), 1);
        let pkg = &t.packages[0];
        assert_eq!(pkg.name, "com.fasterxml.jackson.core:jackson-databind");
        // Backward-compat single-range fields still populated.
        assert_eq!(pkg.introduced.as_deref(), Some("2.0.0"));
        assert_eq!(pkg.fixed.as_deref(), Some("2.14.0"));
        // Multi-range vector also populated.
        assert!(!pkg.ranges.is_empty());
        assert!(pkg.affected_methods.iter().any(|m| m.contains("readValue")));
        // Explicit symbol coverage since jackson OSV has affected_methods.
        assert!(pkg.symbol_coverage.is_explicit());
        assert!(!t.versions_array_not_authoritative);
    }

    #[test]
    fn triage_xalan_backport_honesty() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/osv/xalan-CVE-2022-34169.json");
        let t = triage_osv_path(&p).expect("triage");
        assert!(t.versions_array_not_authoritative);
        let pkg = &t.packages[0];
        // Backward-compat fixed accessor.
        assert_eq!(pkg.fixed.as_deref(), Some("2.7.2.Final-rhlw-00004"));
        assert!(
            pkg.listed_versions
                .contains(&"2.7.2.Final-rhlw-00004".into())
        );
    }

    // Helper: add the required `modified` timestamp to synthetic OSV JSON.
    fn with_modified(json: &str) -> String {
        // Insert `"modified": "2024-01-01T00:00:00Z"` before the closing brace.
        let trimmed = json.trim_end().trim_end_matches('}');
        format!(r#"{trimmed}, "modified": "2024-01-01T00:00:00Z"}}"#)
    }

    #[test]
    fn symbol_coverage_go_imports() {
        // Synthetic Go-convention OSV with imports[].symbols
        let json = with_modified(
            r#"{
            "id": "GO-TEST-0001",
            "affected": [{
                "package": {"ecosystem": "Go", "name": "golang.org/x/net"},
                "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "0.1.1"}]}],
                "ecosystem_specific": {
                    "imports": [{"path": "golang.org/x/net/http2", "symbols": ["Server.ServeConn"]}]
                }
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        assert!(pkg.symbol_coverage.is_explicit());
        assert!(
            pkg.affected_methods
                .contains(&"Server.ServeConn".to_string())
        );
        assert_eq!(pkg.ranges[0].introduced.as_deref(), Some("0"));
        assert!(matches!(&pkg.ranges[0].upper, VersionUpperBound::Fixed(v) if v == "0.1.1"));
    }

    #[test]
    fn symbol_coverage_rustsec_affects_functions() {
        // Synthetic RustSec-convention OSV with affects.functions
        let json = with_modified(
            r#"{
            "id": "RUSTSEC-TEST-0001",
            "affected": [{
                "package": {"ecosystem": "crates.io", "name": "hyper"},
                "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "0.14.26"}]}],
                "ecosystem_specific": {
                    "affects": {"functions": ["hyper::proto::h1::Conn::read_head"]}
                }
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        assert!(pkg.symbol_coverage.is_explicit());
        assert!(
            pkg.affected_methods
                .contains(&"hyper::proto::h1::Conn::read_head".to_string())
        );
    }

    #[test]
    fn symbol_coverage_unspecified_when_no_methods() {
        let json = with_modified(
            r#"{
            "id": "GHSA-TEST-0001",
            "affected": [{
                "package": {"ecosystem": "npm", "name": "express"},
                "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "4.19.0"}]}]
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        assert_eq!(pkg.symbol_coverage, SymbolCoverage::Unspecified);
        assert!(pkg.affected_methods.is_empty());
    }

    #[test]
    fn multi_range_intervals_preserved() {
        // Two ranges: introduced 1.0.0 fixed 1.2.5; introduced 2.0.0 fixed 2.1.3
        let json = with_modified(
            r#"{
            "id": "CVE-TEST-MULTI",
            "affected": [{
                "package": {"ecosystem": "Maven", "name": "com.example:lib"},
                "ranges": [
                    {"type": "ECOSYSTEM", "events": [
                        {"introduced": "1.0.0"}, {"fixed": "1.2.5"},
                        {"introduced": "2.0.0"}, {"fixed": "2.1.3"}
                    ]}
                ]
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        // Both intervals captured.
        assert_eq!(pkg.ranges.len(), 2);
        assert_eq!(pkg.ranges[0].introduced.as_deref(), Some("1.0.0"));
        assert!(matches!(&pkg.ranges[0].upper, VersionUpperBound::Fixed(v) if v == "1.2.5"));
        assert_eq!(pkg.ranges[1].introduced.as_deref(), Some("2.0.0"));
        assert!(matches!(&pkg.ranges[1].upper, VersionUpperBound::Fixed(v) if v == "2.1.3"));
        // Backward-compat: first range.
        assert_eq!(pkg.introduced.as_deref(), Some("1.0.0"));
        assert_eq!(pkg.fixed.as_deref(), Some("1.2.5"));
    }

    #[test]
    fn last_affected_interval() {
        let json = with_modified(
            r#"{
            "id": "CVE-TEST-LAST",
            "affected": [{
                "package": {"ecosystem": "npm", "name": "some-pkg"},
                "ranges": [
                    {"type": "SEMVER", "events": [{"introduced": "1.0.0"}, {"last_affected": "1.4.2"}]}
                ]
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        assert_eq!(pkg.ranges.len(), 1);
        assert!(matches!(&pkg.ranges[0].upper, VersionUpperBound::LastAffected(v) if v == "1.4.2"));
        // Backward-compat fixed is None for last_affected ranges.
        assert!(pkg.fixed.is_none());
    }

    #[test]
    fn fixed_without_introduced_event_is_preserved() {
        let json = with_modified(
            r#"{
            "id": "CVE-TEST-IMPLICIT-ZERO",
            "affected": [{
                "package": {"ecosystem": "npm", "name": "some-pkg"},
                "ranges": [
                    {"type": "SEMVER", "events": [{"fixed": "1.2.5"}]}
                ]
            }]
        }"#,
        );
        let t = triage_osv_bytes(json.as_bytes()).expect("parse");
        let pkg = &t.packages[0];
        assert_eq!(pkg.ranges.len(), 1);
        assert!(pkg.ranges[0].introduced.is_none());
        assert!(matches!(&pkg.ranges[0].upper, VersionUpperBound::Fixed(v) if v == "1.2.5"));
        assert_eq!(pkg.fixed.as_deref(), Some("1.2.5"));
    }
}
