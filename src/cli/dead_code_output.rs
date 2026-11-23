//! JSON and text output formatting for `rgctl dead-code`.
//!
//! JSON output serializes [`DeadCodeReport`] directly (the struct already
//! derives `Serialize` with the documented snake_case field names), so
//! kernel-scale reports never build an intermediate `serde_json::Value`
//! tree.

use rgctl_analysis::dead_code::{
    DeadCodeConfidence, DeadCodeFinding, DeadCodeKind, DeadCodeReport,
};

pub fn emit_text(report: &DeadCodeReport) -> String {
    let mut out = String::new();

    let truncated = report.summary.dead_symbols != report.summary.returned;
    let summary_line = if truncated {
        format!(
            "{} total symbols, {} reachable, {} dead ({} entry points), {} returned",
            report.summary.total_symbols,
            report.summary.reachable_symbols,
            report.summary.dead_symbols,
            report.summary.entry_points,
            report.summary.returned,
        )
    } else {
        format!(
            "{} total symbols, {} reachable, {} dead ({} entry points)",
            report.summary.total_symbols,
            report.summary.reachable_symbols,
            report.summary.dead_symbols,
            report.summary.entry_points,
        )
    };
    out.push_str(&format!("Dead-code analysis: {summary_line}\n\n"));

    if report.findings.is_empty() {
        out.push_str("No dead code detected.\n");
        return out;
    }

    let high: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.confidence == DeadCodeConfidence::High)
        .collect();
    let medium: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.confidence == DeadCodeConfidence::Medium)
        .collect();
    let low: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.confidence == DeadCodeConfidence::Low)
        .collect();

    if !high.is_empty() {
        out.push_str(&format!("--- High confidence ({}) ---\n", high.len()));
        for f in &high {
            format_finding(&mut out, f);
        }
        out.push('\n');
    }

    if !medium.is_empty() {
        out.push_str(&format!("--- Medium confidence ({}) ---\n", medium.len()));
        for f in &medium {
            format_finding(&mut out, f);
        }
        out.push('\n');
    }

    if !low.is_empty() {
        out.push_str(&format!(
            "--- Low confidence ({}) — may be intentional API seams ---\n",
            low.len()
        ));
        for f in &low {
            format_finding(&mut out, f);
        }
        out.push('\n');
    }

    out
}

fn format_finding(out: &mut String, f: &DeadCodeFinding) {
    let location = match (&f.file_path, f.start_line) {
        (Some(path), Some(line)) => format!("{path}:{line}"),
        (Some(path), None) => path.clone(),
        _ => "unknown".to_string(),
    };
    out.push_str(&format!(
        "  {}  {}  ({})  {}\n",
        kind_label(&f.kind),
        f.symbol_name,
        location,
        f.reason,
    ));
}

/// Human-readable snake_case label for a finding kind (avoids Rust `{:?}`
/// `Debug` spellings like `UnreferencedFunction` in text output).
fn kind_label(kind: &DeadCodeKind) -> &'static str {
    match kind {
        DeadCodeKind::UnusedImport => "unused_import",
        DeadCodeKind::UnreferencedFunction => "unreferenced_function",
        DeadCodeKind::UnreferencedClass => "unreferenced_class",
        DeadCodeKind::UnreferencedVariable => "unreferenced_variable",
        DeadCodeKind::UnreferencedModule => "unreferenced_module",
        DeadCodeKind::UnreferencedType => "unreferenced_type",
    }
}
