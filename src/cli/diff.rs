//! `rgctl diff` — cold/open columnar snapshot pair diff (profiling).

use super::args::OutputFormat;
use super::context::CliContext;
use anyhow::{Context, Result};
use rgctl_graph::snapshot_diff::{NoopDiffSink, SnapshotPair, diff_snapshots};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::info;

pub struct DiffArgs {
    pub base: PathBuf,
    pub head: PathBuf,
}

#[derive(Debug, Serialize)]
struct DiffResponse {
    schema_version: u32,
    base: String,
    head: String,
    digest_equal: bool,
    nodes_added: usize,
    nodes_removed: usize,
    nodes_changed: usize,
    edges_added: usize,
    edges_removed: usize,
    timings: DiffTimings,
}

#[derive(Debug, Serialize)]
struct DiffTimings {
    wall_secs: f64,
    open_secs: f64,
    digest_secs: f64,
    diff_secs: f64,
}

pub fn run(ctx: &CliContext, args: DiffArgs) -> Result<()> {
    let base = resolve_snapshot_path(&args.base)?;
    let head = resolve_snapshot_path(&args.head)?;

    let wall_start = Instant::now();

    let open_start = Instant::now();
    let pair = SnapshotPair::open(&base, &head).with_context(|| {
        format!(
            "open snapshot pair {} vs {}",
            base.display(),
            head.display()
        )
    })?;
    let open_secs = open_start.elapsed().as_secs_f64();

    let digest_start = Instant::now();
    let digest_equal = pair.digest_equal().context("compare content digests")?;
    let digest_secs = digest_start.elapsed().as_secs_f64();

    let diff_start = Instant::now();
    let mut sink = NoopDiffSink;
    let stats = diff_snapshots(&pair.base, &pair.head, &mut sink).context("diff_snapshots")?;
    let diff_secs = diff_start.elapsed().as_secs_f64();

    let wall_secs = wall_start.elapsed().as_secs_f64();

    info!(
        target: "profile",
        wall_secs,
        open_secs,
        digest_secs,
        diff_secs,
        digest_equal,
        nodes_added = stats.nodes_added,
        nodes_removed = stats.nodes_removed,
        nodes_changed = stats.nodes_changed,
        edges_added = stats.edges_added,
        edges_removed = stats.edges_removed,
        "[profile] diff summary"
    );

    let response = DiffResponse {
        schema_version: 1,
        base: base.display().to_string(),
        head: head.display().to_string(),
        digest_equal,
        nodes_added: stats.nodes_added,
        nodes_removed: stats.nodes_removed,
        nodes_changed: stats.nodes_changed,
        edges_added: stats.edges_added,
        edges_removed: stats.edges_removed,
        timings: DiffTimings {
            wall_secs,
            open_secs,
            digest_secs,
            diff_secs,
        },
    };

    match ctx.format {
        OutputFormat::Json => ctx.emit_json_value(&serde_json::to_value(&response)?)?,
        OutputFormat::Text
        | OutputFormat::Graphviz
        | OutputFormat::Mermaid
        | OutputFormat::Sarif => {
            println!(
                "diff: digest_equal={} changed={} added={} removed={} (wall={:.3}s)",
                response.digest_equal,
                response.nodes_changed,
                response.nodes_added,
                response.nodes_removed,
                response.timings.wall_secs
            );
        }
    }
    Ok(())
}

fn resolve_snapshot_path(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let candidate = path.join("graph.snapshot.bin");
    if candidate.is_file() {
        return Ok(candidate);
    }
    let candidate = path.join(".rgctl").join("graph.snapshot.bin");
    if candidate.is_file() {
        return Ok(candidate);
    }
    anyhow::bail!(
        "snapshot not found at {} (expected file or dir with graph.snapshot.bin)",
        path.display()
    );
}
