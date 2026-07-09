//! CLI integration tests for migration plan export.

mod dashboard_harness;

use dashboard_harness::{copy_dir_all, rbuilder_bin};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn run_discover_migration(repo: &Path, extra_args: &[&str]) -> std::process::Output {
    let bin = rbuilder_bin();
    let mut cmd = Command::new(&bin);
    cmd.args(["-r", repo.to_str().unwrap(), "discover", ".", "--languages", "java,rust"]);
    cmd.args(extra_args);
    cmd.output().expect("spawn rbuilder discover")
}

#[test]
fn export_migration_plan_writes_json_file() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let plan_path = repo.join("migration_plan.json");
    let output = run_discover_migration(
        &repo,
        &[
            "--export-migration-plan",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "discover failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(plan_path.is_file(), "migration plan file not written");
    let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["schema_version"], 2);
    assert_eq!(plan["order_mode"], "scheduled");
    assert!(plan["steps"].as_array().map(|s| !s.is_empty()).unwrap_or(false));
    assert_eq!(plan["preset"], "hybrid_default");
    assert!(plan["weights"]["alpha"].as_f64().is_some());
}

#[test]
fn export_migration_plan_json_stdout() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let output = run_discover_migration(
        &repo,
        &["--export-migration-plan", "-f", "json"],
    );
    assert!(
        output.status.success(),
        "discover failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["schema_version"], 2);
    assert_eq!(plan["order_mode"], "scheduled");
    assert!(plan["steps"].as_array().map(|s| !s.is_empty()).unwrap_or(false));
}

#[test]
fn migration_preset_foundational_first() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let plan_path = repo.join("foundational.json");
    let output = run_discover_migration(
        &repo,
        &[
            "--export-migration-plan",
            "--migration-preset",
            "foundational_first",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "discover failed");

    let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["preset"], "foundational_first");
    assert_eq!(plan["preset_label"], "Foundational First");
}

#[test]
fn export_migration_plan_priority_order() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(fixture, &repo).expect("copy fixture");

    let plan_path = repo.join("priority_plan.json");
    let output = run_discover_migration(
        &repo,
        &[
            "--export-migration-plan",
            "--migration-order",
            "priority",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "discover failed");

    let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["order_mode"], "priority");
    assert!(plan["steps"][0]["priority_rank"].as_u64().unwrap() >= 1);
    assert!(plan["steps"][0]["schedule_step"].as_u64().is_some());
}
