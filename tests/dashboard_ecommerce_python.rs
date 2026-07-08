//! Dashboard gate — **ecommerce-python** test project (CFG/PDG/taint on Python).
//!
//!   cargo test --release --test dashboard_ecommerce_python
//!
//! Repo path: `/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-python`
//! (override: `RBUILDER_PYTHON_REPO`).

mod dashboard_harness;

use dashboard_harness::{
    assert_dashboard_bundle_all_analysis, ecommerce_python_repo_path, run_discover_all,
};
use rbuilder_dashboard::dist_embedded;

const PYTHON_MIN_NODES: u64 = 40;
const PYTHON_MIN_FUNCTIONS: u64 = 20;
const PYTHON_MIN_METANODES: u64 = 1;

#[test]
fn discover_all_writes_python_cfg_dashboard_bundle() {
    if !dist_embedded() {
        panic!(
            "dashboard/dist not embedded — run ./scripts/build-dashboard.sh && cargo build --release"
        );
    }

    let repo = ecommerce_python_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: python test repo not found at {} (set RBUILDER_PYTHON_REPO)",
            repo.display()
        );
        return;
    }

    let output = run_discover_all(&repo, Some("python"));
    assert!(
        output.status.success(),
        "discover --all on ecommerce-python failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle_all_analysis(&repo, PYTHON_MIN_NODES, PYTHON_MIN_METANODES);

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/manifest.json")).unwrap(),
    )
    .unwrap();
    let functions = manifest["metrics"]["function_count"].as_u64().unwrap_or(0);
    assert!(
        functions >= PYTHON_MIN_FUNCTIONS,
        "expected >= {PYTHON_MIN_FUNCTIONS} functions, got {functions}"
    );

    let cfg_index: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/cfg_index.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cfg_index["available"], true);
    assert!(
        cfg_index["function_count"].as_u64().unwrap_or(0) > 0,
        "cfg_index should list analyzed Python functions"
    );

    eprintln!(
        "ecommerce-python OK: {} nodes, {} functions, {} cfg functions",
        manifest["graph"]["node_count"],
        functions,
        cfg_index["function_count"]
    );
}
