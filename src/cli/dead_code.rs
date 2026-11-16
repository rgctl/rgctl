//! `rgctl dead-code` — graph-based dead-code detection.

use super::context::CliContext;
use super::dead_code_output;
use anyhow::{Result, bail};
use rgctl_analysis::dead_code::{DeadCodeAnalyzer, DeadCodeConfidence};

fn confidence_str(c: DeadCodeConfidence) -> &'static str {
    match c {
        DeadCodeConfidence::High => "high",
        DeadCodeConfidence::Medium => "medium",
        DeadCodeConfidence::Low => "low",
    }
}

/// Parse a comma-separated `--confidence` spec into canonical level names.
///
/// Unknown level names are rejected so a typo (e.g. `--confidence hig`)
/// fails loudly instead of silently filtering out every finding.
fn parse_confidence_levels(spec: &str) -> Result<Vec<&'static str>> {
    let mut levels = Vec::new();
    for token in spec.split(',').map(str::trim) {
        let lower = token.to_ascii_lowercase();
        let level = match lower.as_str() {
            "high" => "high",
            "medium" => "medium",
            "low" => "low",
            _ => bail!(
                "invalid --confidence level `{token}`: expected `high`, `medium`, or `low` (comma-separated)"
            ),
        };
        if !levels.contains(&level) {
            levels.push(level);
        }
    }
    if levels.is_empty() {
        bail!("--confidence requires at least one of: high, medium, low");
    }
    Ok(levels)
}

pub struct DeadCodeArgs {
    pub confidence: Option<String>,
    pub entry_points: Vec<String>,
    pub exclude: Vec<String>,
    pub include_tests: bool,
    pub limit: Option<usize>,
}

pub fn run(ctx: &CliContext, args: DeadCodeArgs) -> Result<()> {
    // Validate `--confidence` before the (potentially expensive) analysis so
    // typos fail fast instead of silently producing an empty report.
    let confidence_filter = args
        .confidence
        .as_deref()
        .map(parse_confidence_levels)
        .transpose()?;

    let store = ctx.open_snapshot_store()?;
    let Some(store) = store else {
        bail!("Graph not found (run `rgctl discover` first)");
    };

    let mut analyzer = DeadCodeAnalyzer::new(&store)?
        .with_exclude_patterns(args.exclude)
        .with_include_tests(args.include_tests);

    if !args.entry_points.is_empty() {
        analyzer = analyzer.with_entry_points(args.entry_points);
    }

    let mut report = analyzer.analyze()?;

    if let Some(keep) = &confidence_filter {
        report
            .findings
            .retain(|f| keep.contains(&confidence_str(f.confidence)));
        report.summary.dead_symbols = report.findings.len();
    }

    if let Some(limit) = args.limit {
        report.findings.truncate(limit);
    }

    match ctx.format {
        super::OutputFormat::Json => {
            let json = dead_code_output::report_to_json(&report);
            ctx.emit_json_value(&json)?;
        }
        _ => {
            let text = dead_code_output::emit_text(&report);
            ctx.emit(&text)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comma_separated_levels() {
        assert_eq!(parse_confidence_levels("high").unwrap(), vec!["high"],);
        assert_eq!(
            parse_confidence_levels("high, low ,MEDIUM").unwrap(),
            vec!["high", "low", "medium"],
        );
    }

    #[test]
    fn rejects_unknown_confidence_levels() {
        assert!(parse_confidence_levels("hig").is_err());
        assert!(parse_confidence_levels("high,nope").is_err());
        assert!(parse_confidence_levels("").is_err());
        assert!(parse_confidence_levels("   ").is_err());
        let msg = parse_confidence_levels("hig").unwrap_err().to_string();
        assert!(
            msg.contains("hig") && msg.contains("medium"),
            "error should name the bad token and valid levels: {msg}"
        );
    }
}
