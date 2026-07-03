//! Dashboard golden-repo gate — **gbuilder** Java project.
//!
//! Run after every dashboard phase:
//!   ./scripts/test-dashboard-golden.sh
//!   cargo test --release --test dashboard_gbuilder
//!
//! Repo path: `/Users/sshaaf/git/java/gbuilder` (override: `RBUILDER_DASHBOARD_GOLDEN_REPO`).

mod dashboard_harness;

use dashboard_harness::{assert_dashboard_bundle_with_meta, golden_repo_path, run_discover};
use rbuilder_dashboard::dist_embedded;

/// gbuilder is a real multi-module Java graph (~2k nodes). Minimum counts guard against regressions.
const GBUILDER_MIN_NODES: u64 = 500;
const GBUILDER_MIN_FUNCTIONS: u64 = 400;
const GBUILDER_MIN_METANODES: u64 = 5;

#[test]
fn discover_writes_dashboard_bundle_on_gbuilder_golden_repo() {
    if !dist_embedded() {
        panic!(
            "dashboard/dist not embedded — run ./scripts/build-dashboard.sh && cargo build --release"
        );
    }

    let repo = golden_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: golden repo not found at {} (set RBUILDER_DASHBOARD_GOLDEN_REPO)",
            repo.display()
        );
        return;
    }

    let output = run_discover(&repo, "java");
    assert!(
        output.status.success(),
        "discover on gbuilder failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle_with_meta(&repo, GBUILDER_MIN_NODES, GBUILDER_MIN_METANODES);

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/manifest.json")).unwrap(),
    )
    .unwrap();
    let functions = manifest["metrics"]["function_count"].as_u64().unwrap_or(0);
    assert!(
        functions >= GBUILDER_MIN_FUNCTIONS,
        "expected >= {GBUILDER_MIN_FUNCTIONS} functions, got {functions}"
    );

    eprintln!(
        "gbuilder golden OK: {} nodes, {} edges, {} functions, {} metanodes",
        manifest["graph"]["node_count"],
        manifest["graph"]["edge_count"],
        functions,
        manifest["view"]["metanode_count"]
    );
}
