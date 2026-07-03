//! Dashboard bundle sanity — tiny polyglot fixture (fast CI path).
//!
//! Golden-repo gate: `cargo test --release --test dashboard_gbuilder`
//! Requires `scripts/build-dashboard.sh` before `cargo build`.

mod dashboard_harness;

use dashboard_harness::{assert_dashboard_bundle, copy_dir_all, run_discover};
use rbuilder_dashboard::dist_embedded;
use std::path::Path;

#[test]
fn dashboard_dist_embedded_at_compile_time() {
    assert!(
        dist_embedded(),
        "dashboard/dist missing — run: ./scripts/build-dashboard.sh && cargo build"
    );
}

#[test]
fn discover_writes_dashboard_bundle_on_tiny_fixture() {
    if !dist_embedded() {
        eprintln!("skip: dashboard/dist not embedded");
        return;
    }

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let output = run_discover(&repo, "java,rust");
    assert!(
        output.status.success(),
        "discover failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_dashboard_bundle(&repo, 1);
}
