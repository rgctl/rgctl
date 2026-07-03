//! Dashboard gate on **metasfresh** with `discover . --all` (CFG/PDG + taint exports).
//!
//! Manual / optional — full analysis on ~128k functions takes a long time:
//!   ./scripts/test-dashboard-metasfresh.sh
//!   cargo test --release --test dashboard_metasfresh -- --ignored --nocapture
//!
//! Repo: `example/metasfresh-4.9.8b` (override: `RBUILDER_METASFRESH_REPO`).

mod dashboard_harness;

use dashboard_harness::{
    assert_dashboard_bundle_all_analysis, metasfresh_repo_path, run_discover_all,
};
use rbuilder_dashboard::dist_embedded;

/// metasfresh reference graph (~128k functions per performance doc).
const METASFRESH_MIN_NODES: u64 = 100_000;
const METASFRESH_MIN_FUNCTIONS: u64 = 100_000;
const METASFRESH_MIN_METANODES: u64 = 20;

#[test]
#[ignore = "manual: metasfresh discover --all is slow (CFG/PDG on ~128k functions)"]
fn discover_all_writes_dashboard_bundle_on_metasfresh() {
    if !dist_embedded() {
        panic!(
            "dashboard/dist not embedded — run ./scripts/build-dashboard.sh && cargo build --release"
        );
    }

    let repo = metasfresh_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: metasfresh example not found at {} (set RBUILDER_METASFRESH_REPO)",
            repo.display()
        );
        return;
    }

    let output = run_discover_all(&repo, None);
    assert!(
        output.status.success(),
        "discover --all on metasfresh failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle_all_analysis(&repo, METASFRESH_MIN_NODES, METASFRESH_MIN_METANODES);

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/manifest.json")).unwrap(),
    )
    .unwrap();
    let functions = manifest["metrics"]["function_count"].as_u64().unwrap_or(0);
    assert!(
        functions >= METASFRESH_MIN_FUNCTIONS,
        "expected >= {METASFRESH_MIN_FUNCTIONS} functions, got {functions}"
    );

    eprintln!(
        "metasfresh golden OK: {} nodes, {} edges, {} functions, {} metanodes, taint={}",
        manifest["graph"]["node_count"],
        manifest["graph"]["edge_count"],
        functions,
        manifest["view"]["metanode_count"],
        manifest["analysis"]["taint_available"]
    );
}

/// Validate an existing metasfresh cache without re-running discover (fast path after manual run).
#[test]
fn metasfresh_dashboard_bundle_when_cache_present() {
    if !dist_embedded() {
        eprintln!("skip: dashboard/dist not embedded");
        return;
    }

    let repo = metasfresh_repo_path();
    let dash = repo.join(".rbuilder/dashboard/manifest.json");
    let archive = repo.join(".rbuilder/analysis/cfg_pdg.archive.bin");
    if !repo.is_dir() || !dash.is_file() || !archive.is_file() {
        eprintln!(
            "skip: no metasfresh --all cache at {} (run ./scripts/test-dashboard-metasfresh.sh)",
            repo.display()
        );
        return;
    }

    assert_dashboard_bundle_all_analysis(&repo, METASFRESH_MIN_NODES, METASFRESH_MIN_METANODES);
}
