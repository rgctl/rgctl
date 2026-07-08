//! Dashboard gate — **ecommerce-cpp** test project (CFG/PDG/taint on C++).
//!
//!   cargo test --release --test dashboard_ecommerce_cpp
//!
//! Repo path: `/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-cpp`
//! (override: `RBUILDER_CPP_REPO`).

mod dashboard_harness;

use dashboard_harness::{
    assert_dashboard_bundle_all_analysis, ecommerce_cpp_repo_path, run_discover_all,
};
use rbuilder_dashboard::dist_embedded;

const CPP_MIN_NODES: u64 = 30;
const CPP_MIN_FUNCTIONS: u64 = 15;
const CPP_MIN_METANODES: u64 = 1;

#[test]
fn discover_all_writes_cpp_cfg_dashboard_bundle() {
    if !dist_embedded() {
        panic!(
            "dashboard/dist not embedded — run ./scripts/build-dashboard.sh && cargo build --release"
        );
    }

    let repo = ecommerce_cpp_repo_path();
    if !repo.is_dir() {
        eprintln!(
            "skip: C++ test repo not found at {} (set RBUILDER_CPP_REPO)",
            repo.display()
        );
        return;
    }

    let output = run_discover_all(&repo, Some("cpp"));
    assert!(
        output.status.success(),
        "discover --all on ecommerce-cpp failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle_all_analysis(&repo, CPP_MIN_NODES, CPP_MIN_METANODES);

    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/manifest.json")).unwrap(),
    )
    .unwrap();
    let functions = manifest["metrics"]["function_count"].as_u64().unwrap_or(0);
    assert!(
        functions >= CPP_MIN_FUNCTIONS,
        "expected >= {CPP_MIN_FUNCTIONS} functions, got {functions}"
    );

    let cfg_index: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".rbuilder/dashboard/cfg_index.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cfg_index["available"], true);
    assert!(
        cfg_index["function_count"].as_u64().unwrap_or(0) > 0,
        "cfg_index should list analyzed C++ functions"
    );

    eprintln!(
        "ecommerce-cpp OK: {} nodes, {} functions, {} cfg functions",
        manifest["graph"]["node_count"],
        functions,
        cfg_index["function_count"]
    );
}
