//! Soft regression gates vs `docs/performance-baselines.md` (+10% tolerance).
//!
//! Run manually after golden-repo discover:
//!   cargo test --release --test discover_perf_baselines -- --ignored --nocapture

mod dashboard_harness;

use dashboard_harness::{golden_repo_path, metasfresh_repo_path, run_discover_all_timed};
use std::time::Duration;

/// metasfresh reference: ~8.9 min discover --all (see performance-baselines.md, 2026-07-10).
const METASFRESH_DISCOVER_ALL_BASELINE_SECS: f64 = 531.0;
/// gbuilder reference when env not set (see performance-baselines.md).
const GBUILDER_DISCOVER_ALL_BASELINE_SECS: f64 = 5.5;
const METASFRESH_TOLERANCE: f64 = 1.10;

fn assert_within_baseline(label: &str, elapsed: Duration, baseline_secs: f64) {
    let limit = baseline_secs * METASFRESH_TOLERANCE;
    assert!(
        elapsed.as_secs_f64() <= limit,
        "{label}: {:.1}s exceeds baseline {:.1}s (+10% = {:.1}s)",
        elapsed.as_secs_f64(),
        baseline_secs,
        limit
    );
}

#[test]
#[ignore = "manual: requires metasfresh checkout and long discover --all run"]
fn metasfresh_discover_all_within_baseline() {
    let repo = metasfresh_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: metasfresh not at {}", repo.display());
        return;
    }

    let (output, elapsed) = run_discover_all_timed(&repo, None);
    assert!(output.status.success(), "discover --all failed");
    eprintln!(
        "metasfresh discover --all: {:.1}s (baseline {:.0}s)",
        elapsed.as_secs_f64(),
        METASFRESH_DISCOVER_ALL_BASELINE_SECS
    );
    assert_within_baseline(
        "metasfresh discover --all",
        elapsed,
        METASFRESH_DISCOVER_ALL_BASELINE_SECS,
    );
}

#[test]
#[ignore = "manual: gbuilder discover --all (~5.5s baseline)"]
fn gbuilder_discover_all_within_baseline() {
    let baseline_secs = std::env::var("RBUILDER_GBUILDER_DISCOVER_ALL_BASELINE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(GBUILDER_DISCOVER_ALL_BASELINE_SECS);

    let repo = golden_repo_path();
    if !repo.is_dir() {
        eprintln!("skip: gbuilder not at {}", repo.display());
        return;
    }

    let (output, elapsed) = run_discover_all_timed(&repo, Some("java"));
    assert!(output.status.success(), "discover --all failed");
    eprintln!(
        "gbuilder discover --all: {:.1}s (baseline {:.1}s)",
        elapsed.as_secs_f64(),
        baseline_secs
    );
    assert_within_baseline("gbuilder discover --all", elapsed, baseline_secs);
}
