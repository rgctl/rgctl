//! Rich CLI output formatting with colors and emojis

use console::Style;
use rbuilder_analysis::dependency::ImpactResult;
use std::fmt::Write as FmtWrite;

/// Severity level for formatted output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Critical / high impact
    Critical,
    /// Warning / medium impact
    Warning,
    /// OK / low impact
    Ok,
    /// Informational
    Info,
}

impl Severity {
    fn emoji(&self) -> &'static str {
        match self {
            Self::Critical => "🔴",
            Self::Warning => "⚠️",
            Self::Ok => "✅",
            Self::Info => "🔍",
        }
    }

    fn style(&self) -> Style {
        match self {
            Self::Critical => Style::new().red().bold(),
            Self::Warning => Style::new().yellow().bold(),
            Self::Ok => Style::new().green(),
            Self::Info => Style::new().cyan(),
        }
    }
}

/// Format an impact analysis report for CLI display.
pub fn format_impact_report(
    symbol: &str,
    direct: &[String],
    indirect: &[String],
    severity: Severity,
) -> String {
    let total = direct.len() + indirect.len();
    let mut out = String::new();

    let _ = writeln!(
        out,
        "{} Analyzing impact of changing `{}`...\n",
        Severity::Info.emoji(),
        symbol
    );

    let header = match severity {
        Severity::Critical => format!(
            "{} HIGH IMPACT - affects {total} symbol(s)",
            Severity::Critical.emoji()
        ),
        Severity::Warning => format!(
            "{} MEDIUM IMPACT - affects {total} symbol(s)",
            Severity::Warning.emoji()
        ),
        Severity::Ok => format!(
            "{} LOW IMPACT - affects {total} symbol(s)",
            Severity::Ok.emoji()
        ),
        Severity::Info => format!("Impact affects {total} symbol(s)"),
    };
    let _ = writeln!(out, "{}", severity.style().apply_to(header));

    if !direct.is_empty() {
        let _ = writeln!(
            out,
            "\n{} DIRECT DEPENDENCIES ({}):",
            Severity::Critical.emoji(),
            direct.len()
        );
        for (i, name) in direct.iter().take(12).enumerate() {
            let _ = writeln!(out, "   {}. {}", i + 1, name);
        }
    }

    if !indirect.is_empty() {
        let _ = writeln!(
            out,
            "\n{} INDIRECT DEPENDENCIES ({}):",
            Severity::Warning.emoji(),
            indirect.len()
        );
        for (i, name) in indirect.iter().take(8).enumerate() {
            let _ = writeln!(out, "   {}. {}", i + 1, name);
        }
    }

    let recommendation = match severity {
        Severity::Critical => "High-risk change. Consider gradual rollout.",
        Severity::Warning => "Moderate impact. Review affected callers before changing.",
        Severity::Ok => "Low impact. Safe to proceed with caution.",
        Severity::Info => "Review affected symbols before proceeding.",
    };
    let _ = writeln!(out, "\n💡 RECOMMENDATION: {recommendation}");

    out
}

/// Format impact from DependencyAnalyzer result.
pub fn format_impact_result(result: &ImpactResult) -> String {
    let total = result.affected_names.len();
    let severity = if total > 20 {
        Severity::Critical
    } else if total > 5 {
        Severity::Warning
    } else {
        Severity::Ok
    };

    format_impact_report(&result.source_name, &[], &result.affected_names, severity)
}

/// Format a complexity level with emoji indicator.
pub fn format_complexity_level(name: &str, cyclomatic: usize) -> String {
    use rbuilder_analysis::complexity::classify_complexity;
    let level = classify_complexity(cyclomatic);
    let (emoji, label) = match level {
        rbuilder_analysis::complexity::ComplexityLevel::Low => ("✅", "LOW"),
        rbuilder_analysis::complexity::ComplexityLevel::Medium => ("⚠️", "MEDIUM"),
        rbuilder_analysis::complexity::ComplexityLevel::High => ("⚠️", "HIGH"),
        rbuilder_analysis::complexity::ComplexityLevel::Critical => ("🔴", "CRITICAL"),
    };
    format!("{emoji} {name} has cyclomatic complexity: {cyclomatic} ({label})")
}

/// Format query result count with emoji.
pub fn format_count(label: &str, count: usize) -> String {
    format!("{} Found {count} {label}", Severity::Ok.emoji())
}

/// Build an ASCII bar chart for a distribution.
pub fn ascii_bar_chart(title: &str, buckets: &[(String, usize)]) -> String {
    if buckets.is_empty() {
        return format!("{title}: (no data)");
    }
    let max = buckets.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1);
    let mut out = format!("📊 {title}\n");
    for (label, value) in buckets {
        let bar_len = (*value * 20 / max).max(if *value > 0 { 1 } else { 0 });
        let bar: String = "█".repeat(bar_len);
        let _ = writeln!(out, "   {label:<12} {bar} {value}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_impact_report() {
        let report = format_impact_report(
            "verify_token",
            &["authenticate_user".into()],
            &["login_handler".into()],
            Severity::Warning,
        );
        assert!(report.contains("verify_token"));
        assert!(report.contains("RECOMMENDATION"));
    }

    #[test]
    fn test_format_complexity() {
        let msg = format_complexity_level("AuthenticationService", 45);
        assert!(msg.contains("CRITICAL"));
    }
}
