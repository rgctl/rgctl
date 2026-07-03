//! Subprocess golden-path tests against the tiny polyglot fixture repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn rbuilder_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rbuilder"))
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo")
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn materialize_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_all(&fixture_root(), dir.path()).expect("copy tiny_polyglot_repo fixture");
    dir
}

fn run_rbuilder(repo: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(rbuilder_bin());
    cmd.arg("-r").arg(repo);
    cmd.args(args);
    cmd.output().expect("spawn rbuilder")
}

#[test]
fn discover_initializes_tiny_polyglot_repo() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let output = run_rbuilder(
        repo,
        &["discover", ".", "--languages", "java,rust"],
    );

    assert!(
        output.status.success(),
        "discover failed: status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let graph_db = repo.join(".rbuilder/graph.db");
    let snapshot = repo.join(".rbuilder/graph.snapshot.bin");
    assert!(
        graph_db.exists() || snapshot.exists(),
        "discover should materialize graph artifacts under .rbuilder/"
    );
}

#[test]
fn blast_radius_json_exit_zero_after_discover() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(repo, &["discover", ".", "--languages", "java,rust"]);
    assert!(discover.status.success(), "discover setup failed");

    let output = run_rbuilder(
        repo,
        &[
            "-f",
            "json",
            "blast-radius",
            "process",
            "--class",
            "OrderService",
        ],
    );

    assert!(
        output.status.success(),
        "blast-radius failed: status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("blast-radius stdout must be valid JSON");

    assert_eq!(doc.get("schema_version").and_then(|v| v.as_u64()), Some(1));
    for key in ["target", "metrics", "topology", "gatekeeping"] {
        assert!(doc.get(key).is_some(), "blast-radius JSON missing '{key}'");
    }
}

#[test]
fn blast_radius_policy_violation_fails_closed_with_exit_one() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(repo, &["discover", ".", "--languages", "java,rust"]);
    assert!(discover.status.success(), "discover setup failed");

    let policy_path = repo.join("strict_policy.json");
    fs::write(
        &policy_path,
        r#"{"max_impact_nodes": 0}"#,
    )
    .expect("write policy file");

    let output = run_rbuilder(
        repo,
        &[
            "-f",
            "json",
            "blast-radius",
            "process",
            "--class",
            "OrderService",
            "--policy-file",
            policy_path.to_str().expect("policy path utf8"),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "policy breach must fail closed with exit code 1; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("violated blast-radius stdout must be valid JSON");
    assert_eq!(
        doc["gatekeeping"]["policy_status"].as_str(),
        Some("VIOLATED")
    );
}
