//! Dashboard bundle sanity — tiny polyglot fixture (fast CI path).
//!
//! Golden-repo gate: `cargo test --release --test dashboard_gbuilder`
//! Requires `scripts/build-dashboard.sh` before `cargo build`.

mod dashboard_harness;

use dashboard_harness::{assert_dashboard_bundle, copy_dir_all, run_discover};
use rgctl_dashboard::dist_embedded;
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

/// The review artifact travels in the bundle, and a re-export notices when only it changed.
///
/// The artifact is not part of the graph fingerprint that the reuse gate compares. Without the third
/// condition, `rgctl review export` followed by a re-export would match the fingerprint, return early,
/// and leave the previously copied artifact in place — a reviewer reading a review of a change nobody is
/// looking at any more. This test fails on that skip, not on the copy.
#[test]
fn review_artifact_is_carried_and_re_exported_when_only_it_changes() {
    if !dist_embedded() {
        eprintln!("skip: dashboard/dist not embedded");
        return;
    }

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let bundle = repo.join(".rgctl/dashboard");

    // First export: no artifact. The manifest records that, rather than omitting the section, so a
    // consumer can tell "there is none" from "this manifest predates the field".
    let output = run_discover(&repo, "java,rust");
    assert!(
        output.status.success(),
        "discover failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = read_manifest(&bundle);
    assert_eq!(
        manifest["workflows"]["available"], false,
        "a bundle exported with no review artifact must say so"
    );
    assert!(
        manifest["workflows"].get("review_path").is_none(),
        "absent artifact must not serialise a path that reads as a value"
    );
    assert!(!bundle.join("workflows/review.json").exists());

    // Provide one. The graph is untouched, so the export fingerprint is unchanged — this is precisely
    // the case a two-condition gate skips.
    let artifact_body = r#"{"schema_version":"1","generated_at":"1792080101","provenance":{"graph":{"base_digest":"aa","head_digest":"bb"}}}"#;
    let artifact = repo.join(".rgctl/workflows/review.json");
    std::fs::create_dir_all(artifact.parent().unwrap()).expect("create workflows dir");
    std::fs::write(&artifact, artifact_body).expect("write artifact");

    let output = rediscover(&repo);
    assert!(
        output.status.success(),
        "second discover failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let carried = bundle.join("workflows/review.json");
    assert!(
        carried.is_file(),
        "the new artifact was not carried into the bundle — the reuse gate skipped the export"
    );
    assert_eq!(
        std::fs::read_to_string(&carried).expect("read carried artifact"),
        artifact_body,
        "the copied artifact must be byte-identical to the source"
    );

    let manifest = read_manifest(&bundle);
    assert_eq!(manifest["workflows"]["available"], true);
    assert_eq!(manifest["workflows"]["review_path"], "workflows/review.json");
    assert_eq!(
        manifest["workflows"]["review_generated_at"], "1792080101",
        "the artifact's own generation time is recorded"
    );
    assert_eq!(
        manifest["workflows"]["review_head_digest"], "bb",
        "the graph revision the artifact describes is recorded, so a consumer can compare it with \
         graph.digest instead of assuming the artifact is current"
    );
}

fn read_manifest(bundle: &Path) -> serde_json::Value {
    let raw = std::fs::read_to_string(bundle.join("manifest.json")).expect("manifest.json");
    serde_json::from_str(&raw).expect("manifest parses")
}

/// Re-run discover **without** wiping `.rgctl`, unlike `run_discover`.
///
/// The harness removes `.rgctl` before every run so each test starts cold, but the workflow artifact
/// lives inside `.rgctl`, so a wiped run could never exercise the reuse gate. Same flags as
/// `run_discover`, minus the removal.
fn rediscover(repo: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_rgctl"))
        .current_dir(repo)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "discover",
            ".",
            "--with-dashboard",
            "--languages",
            "java,rust",
        ])
        .output()
        .expect("spawn rgctl discover")
}
