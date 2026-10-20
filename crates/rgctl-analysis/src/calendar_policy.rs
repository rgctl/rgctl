//! Calendar-aware policy evaluation (grace periods, SLAs, sunset dates).

use crate::policy_diff::{PolicyDelta, TemporalClass};
use crate::violation_ledger::ViolationLedger;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Optional calendar fields on a policy file.
/// Calendar policy configuration from the policy file `temporal` section.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct PolicyTemporal {
    /// ISO date (`YYYY-MM-DD`) when the policy becomes effective.
    #[serde(default)]
    pub effective_from: Option<String>,
    /// Days after `effective_from` where violations may warn instead of fail.
    #[serde(default)]
    pub grace_period_days: Option<u32>,
    /// Fail `existing` violations older than this many days when `enforce_sla` is set.
    #[serde(default)]
    pub violation_sla_days: Option<u32>,
    /// ISO date when the rule sunsets (escalates to fail after this date).
    #[serde(default)]
    pub sunset_date: Option<String>,
    /// Days before `sunset_date` to emit `warn` severity.
    #[serde(default)]
    pub sunset_warn_days: Option<u32>,
    /// Behavior for violations during the grace window.
    #[serde(default)]
    pub severity_during_grace: GraceSeverity,
    /// After grace, treat `existing` violations as blocking.
    #[serde(default)]
    pub fail_existing_after_grace: bool,
    /// Enforce `violation_sla_days` using ledger `first_seen`.
    #[serde(default)]
    pub enforce_sla: bool,
}

/// Grace-window severity for calendar evaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GraceSeverity {
    /// Violations warn (non-blocking unless `--strict-calendar`).
    #[default]
    Warn,
    /// Violations fail during grace.
    Fail,
}

/// Calendar severity surfaced on violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSeverity {
    /// Non-blocking unless `--strict-calendar`.
    Warn,
    /// Blocking failure.
    Fail,
}

impl CalendarSeverity {
    /// JSON / CLI string label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// Per-violation calendar outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalendarDisposition {
    /// Optional warn/fail label for JSON output.
    pub severity: Option<CalendarSeverity>,
    /// When set, overrides base temporal blocking (`true` = fail, `false` = pass).
    pub override_blocking: Option<bool>,
}

/// Evaluate calendar rules for all policy deltas.
pub fn evaluate_calendar_for_deltas(
    deltas: &[PolicyDelta],
    config: &PolicyTemporal,
    ledger: &ViolationLedger,
    today_days: u64,
    now_secs: u64,
) -> HashMap<u64, CalendarDisposition> {
    deltas
        .iter()
        .map(|delta| {
            let disposition = evaluate_calendar_disposition(
                delta,
                config,
                ledger,
                today_days,
                now_secs,
            );
            (delta.key.as_u64(), disposition)
        })
        .collect()
}

/// Evaluate calendar disposition for one temporal delta.
pub fn evaluate_calendar_disposition(
    delta: &PolicyDelta,
    config: &PolicyTemporal,
    ledger: &ViolationLedger,
    today_days: u64,
    now_secs: u64,
) -> CalendarDisposition {
    if delta.classification == TemporalClass::Resolved {
        return CalendarDisposition::default();
    }

    if config.is_empty() {
        return CalendarDisposition::default();
    }

    if let Some(effective) = config.effective_from.as_deref().and_then(iso_date_to_day_number) {
        if today_days < effective {
            return CalendarDisposition {
                severity: None,
                override_blocking: Some(false),
            };
        }
        if let Some(grace_days) = config.grace_period_days {
            let grace_end = effective.saturating_add(grace_days as u64);
            if today_days < grace_end {
                let severity = match config.severity_during_grace {
                    GraceSeverity::Warn => CalendarSeverity::Warn,
                    GraceSeverity::Fail => CalendarSeverity::Fail,
                };
                return CalendarDisposition {
                    severity: Some(severity),
                    override_blocking: Some(config.severity_during_grace == GraceSeverity::Fail),
                };
            }
            if config.fail_existing_after_grace
                && delta.classification == TemporalClass::Existing
            {
                return CalendarDisposition {
                    severity: Some(CalendarSeverity::Fail),
                    override_blocking: Some(true),
                };
            }
        }
    }

    if config.enforce_sla {
        if let Some(sla_days) = config.violation_sla_days {
            let rule = delta.violation.rule_id();
            if let Some(age_days) = ledger.violation_age_days(delta.key.as_u64(), rule, now_secs) {
                if age_days > sla_days as u64 {
                    return CalendarDisposition {
                        severity: Some(CalendarSeverity::Fail),
                        override_blocking: Some(true),
                    };
                }
            }
        }
    }

    if let Some(sunset) = config.sunset_date.as_deref().and_then(iso_date_to_day_number) {
        if today_days >= sunset {
            return CalendarDisposition {
                severity: Some(CalendarSeverity::Fail),
                override_blocking: Some(true),
            };
        }
        if let Some(warn_days) = config.sunset_warn_days {
            let days_until = sunset.saturating_sub(today_days);
            if days_until <= warn_days as u64 {
                return CalendarDisposition {
                    severity: Some(CalendarSeverity::Warn),
                    override_blocking: Some(false),
                };
            }
        }
    }

    CalendarDisposition::default()
}

impl PolicyTemporal {
    fn is_empty(&self) -> bool {
        self.effective_from.is_none()
            && self.grace_period_days.is_none()
            && self.violation_sla_days.is_none()
            && self.sunset_date.is_none()
            && !self.fail_existing_after_grace
            && !self.enforce_sla
    }
}

/// UTC day number for the current system time (days since Unix epoch).
pub fn system_today_days() -> u64 {
    unix_secs_to_utc_days(system_unix_secs())
}

/// Current Unix timestamp in seconds.
pub fn system_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_secs_to_utc_days(secs: u64) -> u64 {
    secs / 86_400
}

/// Parse `YYYY-MM-DD` to days since Unix epoch (UTC).
pub fn iso_date_to_day_number(iso: &str) -> Option<u64> {
    let parts: Vec<&str> = iso.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<i64>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    Some(civil_date_to_day_number(year, month, day))
}

/// Days since epoch for an ISO date offset by `delta_days` (for tests).
pub fn iso_date_plus_days(iso: &str, delta_days: i64) -> Option<u64> {
    iso_date_to_day_number(iso).map(|base| {
        if delta_days >= 0 {
            base.saturating_add(delta_days as u64)
        } else {
            base.saturating_sub((-delta_days) as u64)
        }
    })
}

fn civil_date_to_day_number(year: i64, month: u32, day: u32) -> u64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as i64;
    (era * 146_097 + doe - 719_468) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyViolation;
    use crate::violation_ledger::ViolationLedger;
    use rgctl_graph::stable_key::stable_key_from_facets;
    use rgctl_graph::schema::NodeType;

    fn sample_delta(classification: TemporalClass) -> PolicyDelta {
        PolicyDelta {
            key: stable_key_from_facets(Some("src/a.rs"), "foo", NodeType::Function, None, None),
            symbol: "foo".into(),
            classification,
            violation: PolicyViolation::ScaleFailure { count: 6, max: 5 },
        }
    }

    #[test]
    fn grace_period_warns_only() {
        let today = iso_date_to_day_number("2026-09-10").unwrap();
        let config = PolicyTemporal {
            effective_from: Some("2026-08-01".into()),
            grace_period_days: Some(60),
            severity_during_grace: GraceSeverity::Warn,
            ..Default::default()
        };
        let delta = sample_delta(TemporalClass::Existing);
        let ledger = ViolationLedger::in_memory();
        let disposition = evaluate_calendar_disposition(&delta, &config, &ledger, today, 0);
        assert_eq!(disposition.severity, Some(CalendarSeverity::Warn));
        assert_eq!(disposition.override_blocking, Some(false));
    }

    #[test]
    fn post_grace_fails_existing() {
        let today = iso_date_to_day_number("2026-10-15").unwrap();
        let config = PolicyTemporal {
            effective_from: Some("2026-08-01".into()),
            grace_period_days: Some(30),
            fail_existing_after_grace: true,
            ..Default::default()
        };
        let delta = sample_delta(TemporalClass::Existing);
        let disposition = evaluate_calendar_disposition(
            &delta,
            &config,
            &ViolationLedger::in_memory(),
            today,
            0,
        );
        assert_eq!(disposition.override_blocking, Some(true));
    }

    #[test]
    fn sla_enforcement_uses_ledger_first_seen() {
        let now_secs = 4_000_000_000u64;
        let first_seen = now_secs - (45 * 86_400);
        let delta = sample_delta(TemporalClass::Existing);
        let mut ledger = ViolationLedger::in_memory();
        ledger
            .append(ledger_entry_with_ts(
                delta.key.as_u64(),
                "max_impact_nodes",
                TemporalClass::Existing,
                first_seen,
            ))
            .unwrap();
        let config = PolicyTemporal {
            violation_sla_days: Some(30),
            enforce_sla: true,
            ..Default::default()
        };
        let disposition = evaluate_calendar_disposition(
            &delta,
            &config,
            &ledger,
            system_today_days(),
            now_secs,
        );
        assert_eq!(disposition.override_blocking, Some(true));
    }

    fn ledger_entry_with_ts(
        stable_key: u64,
        rule: &str,
        class: TemporalClass,
        ts_secs: u64,
    ) -> crate::violation_ledger::ViolationLedgerEntry {
        crate::violation_ledger::ViolationLedgerEntry {
            stable_key,
            rule: rule.to_string(),
            class,
            commit: "abc".into(),
            ts: ts_secs.to_string(),
            symbol: Some("foo".into()),
        }
    }
}
