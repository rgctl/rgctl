//! JSON output for `rgctl pr-check`.

use rgctl_analysis::{CalendarDisposition, CalendarSeverity, PolicyDelta, TemporalClass};
use rgctl_graph::snapshot_diff::DiffStats;
use serde::Serialize;
use std::collections::HashMap;

pub const PR_CHECK_SCHEMA_VERSION: &str = "2";

#[derive(Debug, Serialize)]
pub struct PrCheckResponse {
    pub schema_version: &'static str,
    pub passed: bool,
    pub violations: Vec<PrCheckViolation>,
    pub violations_summary: ViolationsSummary,
    pub graph_diff: DiffStats,
    pub scope: PrCheckScopeSummary,
}

#[derive(Debug, Serialize, Default)]
pub struct ViolationsSummary {
    pub new: usize,
    pub existing: usize,
    pub resolved: usize,
    pub regression: usize,
}

#[derive(Debug, Serialize)]
pub struct PrCheckViolation {
    pub symbol: String,
    pub classification: TemporalClass,
    pub violation: rgctl_analysis::PolicyViolation,
    pub stable_key: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introduced_in_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct PrCheckScopeSummary {
    pub files: usize,
    pub entities: usize,
}

pub fn build_pr_check_response(
    deltas: &[PolicyDelta],
    graph_diff: DiffStats,
    scoped_files: usize,
    scoped_entities: usize,
    new_violations_only: bool,
    fail_on_regression: bool,
    introduced_in_commit: &HashMap<u64, String>,
    calendar: &HashMap<u64, CalendarDisposition>,
    strict_calendar: bool,
) -> PrCheckResponse {
    let violations: Vec<PrCheckViolation> = deltas
        .iter()
        .map(|d| {
            let stable_key = d.key.as_u64();
            let cal = calendar.get(&stable_key);
            PrCheckViolation {
                symbol: d.symbol.clone(),
                classification: d.classification,
                violation: d.violation.clone(),
                stable_key,
                introduced_in_commit: introduced_in_commit.get(&stable_key).cloned(),
                severity: cal.and_then(|c| c.severity.map(CalendarSeverity::as_str)),
            }
        })
        .collect();

    let violations_summary = summarize_violations(&violations);
    let passed = !violations.iter().any(|v| {
        is_blocking(
            v.classification,
            new_violations_only,
            fail_on_regression,
            calendar.get(&v.stable_key),
            strict_calendar,
        )
    });

    PrCheckResponse {
        schema_version: PR_CHECK_SCHEMA_VERSION,
        passed,
        violations,
        violations_summary,
        graph_diff,
        scope: PrCheckScopeSummary {
            files: scoped_files,
            entities: scoped_entities,
        },
    }
}

fn summarize_violations(violations: &[PrCheckViolation]) -> ViolationsSummary {
    let mut summary = ViolationsSummary::default();
    for v in violations {
        match v.classification {
            TemporalClass::New => summary.new += 1,
            TemporalClass::Existing => summary.existing += 1,
            TemporalClass::Resolved => summary.resolved += 1,
            TemporalClass::Regression => summary.regression += 1,
        }
    }
    summary
}

fn is_blocking(
    classification: TemporalClass,
    new_violations_only: bool,
    fail_on_regression: bool,
    calendar: Option<&CalendarDisposition>,
    strict_calendar: bool,
) -> bool {
    if classification == TemporalClass::Resolved {
        return false;
    }
    if let Some(cal) = calendar {
        if cal.override_blocking == Some(true) {
            return true;
        }
        if cal.override_blocking == Some(false) {
            return strict_calendar;
        }
    }
    match classification {
        TemporalClass::Resolved => false,
        TemporalClass::New => true,
        TemporalClass::Regression => fail_on_regression,
        TemporalClass::Existing => !new_violations_only,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_analysis::{PolicyDelta, PolicyViolation, TemporalClass};
    use rgctl_graph::snapshot_diff::DiffStats;
    use rgctl_graph::stable_key::stable_key_from_facets;

    fn sample_delta(classification: TemporalClass) -> PolicyDelta {
        PolicyDelta {
            key: stable_key_from_facets(
                Some("src/a.rs"),
                "foo",
                rgctl_graph::schema::NodeType::Function,
                None,
            ),
            symbol: "foo".into(),
            classification,
            violation: PolicyViolation::ScaleFailure { count: 6, max: 5 },
        }
    }

    #[test]
    fn new_violations_only_fails_on_new_class() {
        let stats = DiffStats::default();
        let pass = build_pr_check_response(
            &[sample_delta(TemporalClass::Existing)],
            stats,
            1,
            1,
            true,
            true,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(pass.passed);

        let fail = build_pr_check_response(
            &[sample_delta(TemporalClass::New)],
            stats,
            1,
            1,
            true,
            true,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(!fail.passed);
    }

    #[test]
    fn resolved_never_blocks_gate() {
        let stats = DiffStats::default();
        let pass = build_pr_check_response(
            &[sample_delta(TemporalClass::Resolved)],
            stats,
            1,
            1,
            false,
            true,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(pass.passed);
        assert_eq!(pass.violations_summary.resolved, 1);
    }

    #[test]
    fn calendar_grace_warn_suppresses_until_strict() {
        let stats = DiffStats::default();
        let key = sample_delta(TemporalClass::Existing).key.as_u64();
        let calendar = HashMap::from([(
            key,
            CalendarDisposition {
                severity: Some(CalendarSeverity::Warn),
                override_blocking: Some(false),
            },
        )]);
        let pass = build_pr_check_response(
            &[sample_delta(TemporalClass::Existing)],
            stats,
            1,
            1,
            false,
            true,
            &HashMap::new(),
            &calendar,
            false,
        );
        assert!(pass.passed);

        let fail = build_pr_check_response(
            &[sample_delta(TemporalClass::Existing)],
            stats,
            1,
            1,
            false,
            true,
            &HashMap::new(),
            &calendar,
            true,
        );
        assert!(!fail.passed);
    }

    #[test]
    fn regression_blocks_when_enabled() {
        let stats = DiffStats::default();
        let fail = build_pr_check_response(
            &[sample_delta(TemporalClass::Regression)],
            stats,
            1,
            1,
            true,
            true,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(!fail.passed);

        let pass = build_pr_check_response(
            &[sample_delta(TemporalClass::Regression)],
            stats,
            1,
            1,
            true,
            false,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(pass.passed);
    }

    #[test]
    fn all_violations_fail_when_not_new_only() {
        let stats = DiffStats::default();
        let fail = build_pr_check_response(
            &[sample_delta(TemporalClass::Existing)],
            stats,
            1,
            1,
            false,
            true,
            &HashMap::new(),
            &HashMap::new(),
            false,
        );
        assert!(!fail.passed);
    }
}
