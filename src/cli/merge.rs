//! `rgctl merge` — cross-repo park-and-resolve into one snapshot.

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::{Context, Result, bail};
use rgctl_extraction::{MergeReport, merge_repositories};
use std::path::PathBuf;

pub struct MergeArgs {
    pub repos: Vec<PathBuf>,
    pub output_dir: PathBuf,
}

pub fn run(ctx: &CliContext, args: MergeArgs) -> Result<()> {
    if args.repos.is_empty() {
        bail!("rgctl merge requires at least one repository path");
    }
    let report = merge_repositories(&args.repos, &args.output_dir).with_context(|| {
        format!(
            "merge {} repos into {}",
            args.repos.len(),
            args.output_dir.display()
        )
    })?;
    emit_report(ctx, &report)?;
    Ok(())
}

fn emit_report(ctx: &CliContext, report: &MergeReport) -> Result<()> {
    match ctx.format {
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "schema_version": 1,
                "command": "merge",
                "repos_merged": report.repos_merged,
                "total_nodes": report.total_nodes,
                "total_edges": report.total_edges,
                "parked_relations_evaluated": report.parked_relations_evaluated,
                "cross_repo_edges_resolved": report.cross_repo_edges_resolved,
                "stubs_rewired": report.stubs_rewired,
                "ambiguous_skipped": report.ambiguous_skipped,
            });
            ctx.emit_json_value(&payload)?;
        }
        OutputFormat::Text
        | OutputFormat::Graphviz
        | OutputFormat::Mermaid
        | OutputFormat::Sarif => {
            let text = format!(
                "Merged {} repos → {} nodes, {} edges\n\
                 parked evaluated: {}\n\
                 cross-repo resolved: {}\n\
                 stubs rewired: {}\n\
                 ambiguous skipped: {}\n",
                report.repos_merged,
                report.total_nodes,
                report.total_edges,
                report.parked_relations_evaluated,
                report.cross_repo_edges_resolved,
                report.stubs_rewired,
                report.ambiguous_skipped,
            );
            ctx.emit(&text)?;
        }
    }
    Ok(())
}
