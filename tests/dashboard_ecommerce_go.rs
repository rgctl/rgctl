//! Dashboard gate — **ecommerce-go** test project (CFG/PDG/taint on Go).
//!
//!   cargo test --release --test dashboard_ecommerce_go
//!
//! Repo path: `/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-go`
//! (override: `RBUILDER_GO_REPO`).

mod dashboard_harness;

use dashboard_harness::{
    assert_dashboard_bundle_all_analysis, ecommerce_go_repo_path, run_discover_all,
};
use rbuilder_dashboard::dist_embedded;

const GO_MIN_NODES: u64 = 20;
const GO_MIN_FUNCTIONS: u64 = 10;
const GO_MIN_METANODES: u64 = 1;

#[test]
fn discover_all_writes_go_cfg_dashboard_bundle() {
    if !dist_embedded() {
        panic!(
            "dashboard/dist not embedded — run ./scripts/build-dashboard.sh && cargo build --release"
        );
    }

    let repo = ecommerce_go_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: go test repo not found at {} (set RBUILDER_GO_REPO)",
            repo.display()
        );
        return;
    }

    let output = run_discover_all(&repo, Some("go"));
    assert!(
        output.status.success(),
        "discover --all on ecommerce-go failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle_all_analysis(&repo, GO_MIN_NODES, GO_MIN_METANODES);

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/manifest.json")).unwrap(),
    )
    .unwrap();
    let functions = manifest["metrics"]["function_count"].as_u64().unwrap_or(0);
    assert!(
        functions >= GO_MIN_FUNCTIONS,
        "expected >= {GO_MIN_FUNCTIONS} functions, got {functions}"
    );

    let cfg_index: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/cfg_index.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cfg_index["available"], true);
    assert!(
        cfg_index["function_count"].as_u64().unwrap_or(0) > 0,
        "cfg_index should list analyzed Go functions"
    );

    eprintln!(
        "ecommerce-go OK: {} nodes, {} functions, {} cfg functions",
        manifest["graph"]["node_count"],
        functions,
        cfg_index["function_count"]
    );
}
