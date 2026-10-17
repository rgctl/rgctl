//! `rgctl review` — temporal PR analysis family (`paths` + `check`).

use super::args::OutputFormat;
use super::context::CliContext;
use super::pr_check::{self, PrCheckArgs};
use super::temporal_prep::{
    SyntheticHeadMode, TemporalArtifactArgs, prepare_artifacts, resolve_hunk_index,
    resolve_scope_paths,
};
use crate::analysis::{ReviewPathsOptions, build_review_paths_report};
use anyhow::{Context, Result};
use rgctl_graph::snapshot_diff::{SnapshotPair, VecDiffSink, diff_snapshots};
use rgctl_incremental::EntityScope;

/// Shared clap fields for `review paths` (also mirrored on `review check` / `pr-check`).
pub struct ReviewPathsArgs {
    pub base_artifact: Option<String>,
    pub head_artifact: Option<String>,
    pub base_ref: String,
    pub head_ref: String,
    pub cascade_depth: usize,
    pub full_snapshots: bool,
    pub synthetic_head: Option<String>,
    pub upstream_depth: usize,
    pub downstream_depth: usize,
    pub symbol: Option<String>,
    pub max_symbols: usize,
    pub fanout: usize,
}

pub fn run_check(ctx: &CliContext, args: PrCheckArgs) -> Result<()> {
    pr_check::run(ctx, args)
}

/// A built review-paths report plus the provenance captured while building it.
///
/// `review export` consumes the report *and* the provenance, so the two are produced
/// together: re-deriving the digests or artifact paths afterwards would risk them
/// describing a different snapshot pair than the report does.
pub struct PathsReportBundle {
    pub report: crate::analysis::ReviewPathsReport,
    pub base_snapshot: std::path::PathBuf,
    pub head_snapshot: std::path::PathBuf,
    pub base_digest: String,
    pub head_digest: String,
}

/// Build the before/after call-path report for a PR. Shared by `review paths` and
/// `review export` so the two can never disagree about the same change.
pub fn build_paths_bundle(ctx: &CliContext, args: ReviewPathsArgs) -> Result<PathsReportBundle> {
    let synthetic_head = SyntheticHeadMode::parse(args.synthetic_head.as_deref())?;
    let temporal = TemporalArtifactArgs {
        base_artifact: args.base_artifact,
        head_artifact: args.head_artifact,
        base_ref: args.base_ref,
        head_ref: args.head_ref,
        cascade_depth: args.cascade_depth,
        full_snapshots: args.full_snapshots,
    };

    let artifacts = prepare_artifacts(ctx, &temporal, synthetic_head)?;
    let pair = SnapshotPair::open(&artifacts.base_snapshot, &artifacts.head_snapshot)
        .with_context(|| {
            format!(
                "open snapshot pair (base={}, head={})",
                artifacts.base_snapshot.display(),
                artifacts.head_snapshot.display()
            )
        })?;

    let base_digest = pair.base.content_digest()?.to_string();
    let head_digest = pair.head.content_digest()?.to_string();

    let mut sink = VecDiffSink::default();
    let _stats = diff_snapshots(&pair.base, &pair.head, &mut sink)?;

    let paths = resolve_scope_paths(ctx, &temporal, synthetic_head)?;
    let hunk_index = resolve_hunk_index(ctx, &temporal, synthetic_head)?;
    let scope = EntityScope::new(&pair.head, &paths, Some(&hunk_index));
    let hunk_seeds = scope.changed_entities()?;

    let files_in_scope: Vec<String> = paths.files().iter().map(|s| s.to_string()).collect();

    let options = ReviewPathsOptions {
        upstream_depth: args.upstream_depth,
        downstream_depth: args.downstream_depth,
        max_symbols: args.max_symbols.max(1),
        fanout_cap: args.fanout.max(1),
        max_neighbor_nodes: crate::analysis::DEFAULT_MAX_NEIGHBOR_NODES,
        symbol_filter: args.symbol,
    };

    let report = build_review_paths_report(
        &pair.base,
        &pair.head,
        &hunk_seeds,
        &sink.edges,
        &sink.nodes,
        &files_in_scope,
        &options,
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(PathsReportBundle {
        report,
        base_snapshot: artifacts.base_snapshot,
        head_snapshot: artifacts.head_snapshot,
        base_digest,
        head_digest,
    })
}

pub fn run_paths(ctx: &CliContext, args: ReviewPathsArgs) -> Result<()> {
    let report = build_paths_bundle(ctx, args)?.report;

    if !report.ambiguous.is_empty() && report.symbols.is_empty() {
        if ctx.format == OutputFormat::Json {
            ctx.emit_json_value(&serde_json::to_value(&report)?)?;
        } else {
            println!("Ambiguous --symbol; candidates:");
            for c in &report.ambiguous {
                match &c.file {
                    Some(f) => println!("  {} ({})", c.name, f),
                    None => println!("  {}", c.name),
                }
            }
        }
        std::process::exit(2);
    }

    if ctx.format == OutputFormat::Json {
        ctx.emit_json_value(&serde_json::to_value(&report)?)?;
    } else {
        print_text_summary(&report);
    }
    Ok(())
}

fn print_text_summary(report: &crate::analysis::ReviewPathsReport) {
    println!(
        "review paths: {} symbols, {} unscored files",
        report.change_summary.changed_symbols, report.change_summary.unscored_files
    );
    if report.truncation.symbols || report.truncation.fanout {
        println!(
            "truncation: symbols={} fanout={}",
            report.truncation.symbols, report.truncation.fanout
        );
    }
    for sym in &report.symbols {
        println!("\n{}:", sym.name);
        if sym.path_before == sym.path_after && sym.path_delta.is_empty() {
            println!("  path unchanged: {}", sym.path_before.join(" → "));
            continue;
        }
        if !sym.path_before.is_empty() {
            println!("  before: {}", sym.path_before.join(" → "));
        } else {
            println!("  before: (absent)");
        }
        if !sym.path_after.is_empty() {
            println!("  after:  {}", sym.path_after.join(" → "));
        } else {
            println!("  after:  (absent)");
        }
        for d in &sym.path_delta {
            match d {
                crate::analysis::PathDelta::Retargeted {
                    from,
                    to_before,
                    to_after,
                } => println!("  delta: {from}: {to_before} → {to_after} (retargeted)"),
                crate::analysis::PathDelta::Added { from, to, .. } => {
                    println!("  delta: + {from} → {to}")
                }
                crate::analysis::PathDelta::Removed { from, to, .. } => {
                    println!("  delta: - {from} → {to}")
                }
            }
        }
    }
    if !report.unscored_files.is_empty() {
        println!("\nunscored:");
        for f in &report.unscored_files {
            println!("  {} ({})", f.path, f.reason);
        }
    }
}
