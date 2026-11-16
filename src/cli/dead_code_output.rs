//! JSON and text output formatting for `rgctl dead-code`.

use rgctl_analysis::dead_code::{DeadCodeConfidence, DeadCodeFinding, DeadCodeReport};
use serde_json::json;

pub fn report_to_json(report: &DeadCodeReport) -> serde_json::Value {
    json!({
        "schema_version": report.schema_version,
        "summary": {
            "total_symbols": report.summary.total_symbols,
            "reachable_symbols": report.summary.reachable_symbols,
            "dead_symbols": report.summary.dead_symbols,
            "entry_points": report.summary.entry_points,
        },
        "findings": report.findings.iter().map(finding_to_json).collect::<Vec<_>>(),
    })
}

fn finding_to_json(f: &DeadCodeFinding) -> serde_json::Value {
    json!({
        "symbol_name": f.symbol_name,
        "qualified_name": f.qualified_name,
        "file_path": f.file_path,
        "start_line": f.start_line,
        "end_line": f.end_line,
        "node_type": f.node_type,
        "kind": f.kind,
        "confidence": f.confidence,
        "reason": f.reason,
        "node_id": f.node_id.to_string(),
    })
}

pub fn emit_text(report: &DeadCodeReport) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Dead-code analysis: {} total symbols, {} reachable, {} dead ({} entry points)\n\n",
        report.summary.total_symbols,
        report.summary.reachable_symbols,
        report.summary.dead_symbols,
        report.summary.entry_points,
    ));

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
        "  {:?}  {}  ({})  {}\n",
        f.kind, f.symbol_name, location, f.reason,
    ));
}
