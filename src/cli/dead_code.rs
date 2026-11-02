//! `rgctl dead-code` — graph-based dead-code detection.

use super::context::CliContext;
use super::dead_code_output;
use anyhow::{Result, bail};
use rgctl_analysis::dead_code::DeadCodeAnalyzer;

pub struct DeadCodeArgs {
    pub confidence: Option<String>,
    pub entry_points: Vec<String>,
    pub exclude: Vec<String>,
    pub include_tests: bool,
    pub limit: Option<usize>,
}

pub fn run(ctx: &CliContext, args: DeadCodeArgs) -> Result<()> {
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

    if let Some(conf) = &args.confidence {
        let keep: Vec<_> = conf.split(',').map(|s| s.trim().to_lowercase()).collect();
        report.findings.retain(|f| {
            let f_conf = format!("{:?}", f.confidence).to_lowercase();
            keep.iter().any(|k| k == &f_conf)
        });
        report.dead_symbols = report.findings.len();
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
