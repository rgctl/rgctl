//! `rgctl dead-code` — graph-based dead-code detection.

use super::context::CliContext;
use super::dead_code_output;
use anyhow::{Result, bail};
use rgctl_analysis::dead_code::{DeadCodeAnalyzer, DeadCodeConfidence};

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

    // Push the tier filter into the analyzer: excluded tiers never allocate
    // findings (and on columnar v2+ snapshots never decode cold extensions).
    if let Some(levels) = &confidence_filter {
        let tiers = levels
            .iter()
            .filter_map(|&l| match l {
                "high" => Some(DeadCodeConfidence::High),
                "medium" => Some(DeadCodeConfidence::Medium),
                "low" => Some(DeadCodeConfidence::Low),
                _ => None,
            })
            .collect();
        analyzer = analyzer.with_confidence_filter(tiers);
    }

    let mut report = analyzer.analyze()?;

    if let Some(limit) = args.limit {
        report.findings.truncate(limit);
    }

    // `dead_symbols` keeps the full (confidence-filtered) dead count;
    // `returned` reflects what is actually in the payload after `--limit`.
    report.summary.returned = report.findings.len();

    match ctx.format {
        super::OutputFormat::Json => {
            // Serialize the report directly — no intermediate
            // `serde_json::Value` tree (kernel-scale reports stay ~GB smaller).
            ctx.emit_serializable(&report)?;
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
