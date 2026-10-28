//! CLI coverage for discover `--with-universe` / `--export-migration-hints` (#31).

mod dashboard_harness;

use dashboard_harness::{copy_dir_all, rgctl_bin};
use std::path::Path;
use std::process::Command;

fn run_discover(repo: &Path, extra: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(rgctl_bin());
    cmd.current_dir(repo).args([
        "-r",
        repo.to_str().unwrap(),
        "discover",
        ".",
        "--languages",
        "java,rust",
    ]);
    cmd.args(extra);
    cmd.output().expect("spawn rgctl discover")
}

fn assert_ok(output: &std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn materialize() -> (tempfile::TempDir, std::path::PathBuf) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(&fixture, &repo).expect("copy fixture");
    // Fixture may ship a stale `.rgctl/`; start clean so default-off is observable.
    let _ = std::fs::remove_dir_all(repo.join(".rgctl"));
    (tmp, repo)
}

#[test]
fn discover_default_skips_dashboard_dir() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &[]);
    assert_ok(&output, "discover default");

    assert!(
        repo.join(".rgctl/graph.snapshot.bin").is_file(),
        "graph snapshot still required"
    );
    assert!(
        !repo.join(".rgctl/dashboard").exists(),
        "default discover must not write .rgctl/dashboard (#31)"
    );
}

#[test]
fn with_universe_writes_the_universe_bundle() {
    if !rgctl_dashboard::dist_embedded() {
        eprintln!("skip: dashboard bundle not embedded");
        return;
    }

    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-universe"]);
    assert_ok(&output, "discover --with-universe");

    let dash = repo.join(".rgctl/dashboard");
    assert!(dash.join("index.html").is_file(), "missing index.html");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dash.join("manifest.json")).expect("manifest.json"))
            .expect("manifest parses");
    assert_eq!(manifest["mode"], "universe");
}

#[test]
fn with_dashboard_flag_is_rejected() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-dashboard"]);
    assert!(
        !output.status.success(),
        "--with-dashboard must fail (removed; use --with-universe)"
    );
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        err.contains("--with-dashboard") || err.contains("unexpected argument"),
        "error should reject --with-dashboard: {err}"
    );
}

#[test]
fn export_migration_hints_writes_plan_without_dashboard() {
    let (_tmp, repo) = materialize();
    let plan_path = repo.join("hints.json");
    let output = run_discover(
        &repo,
        &[
            "--export-migration-hints",
            "-o",
            plan_path.to_str().unwrap(),
        ],
    );
    assert_ok(&output, "discover --export-migration-hints");

    assert!(plan_path.is_file(), "migration hints file not written");
    let plan: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(plan["schema_version"], 2);
    assert!(
        !repo.join(".rgctl/dashboard").exists(),
        "migration hints must not imply dashboard export"
    );
}

#[test]
fn export_migration_plan_alias_still_works() {
    let (_tmp, repo) = materialize();
    let plan_path = repo.join("legacy_alias.json");
    let output = run_discover(
        &repo,
        &["--export-migration-plan", "-o", plan_path.to_str().unwrap()],
    );
    assert_ok(&output, "discover --export-migration-plan alias");
    assert!(plan_path.is_file(), "alias must still write plan JSON");
}

#[test]
fn discover_help_documents_universe_not_dashboard() {
    let output = Command::new(rgctl_bin())
        .args(["discover", "--help"])
        .output()
        .expect("spawn help");
    assert_ok(&output, "discover --help");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("--with-universe"),
        "missing --with-universe"
    );
    assert!(
        !help.contains("--with-dashboard"),
        "--with-dashboard must be gone from help"
    );
    assert!(
        help.contains("--export-migration-hints"),
        "missing --export-migration-hints"
    );
}

#[test]
fn serve_help_has_no_dashboard_only_or_with_dashboard() {
    let output = Command::new(rgctl_bin())
        .args(["serve", "--help"])
        .output()
        .expect("spawn help");
    assert_ok(&output, "serve --help");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        !help.contains("--with-dashboard"),
        "serve must not document --with-dashboard"
    );
    assert!(
        !help.contains("--dashboard-only"),
        "serve must not document --dashboard-only"
    );
    assert!(help.contains("--open"), "serve should document --open");
}

/// Unknown hyphen flags used to fall into Discover's trailing `extra` catch-all and be ignored.
#[test]
fn discover_rejects_unknown_hyphen_flags() {
    let (_tmp, repo) = materialize();
    let output = run_discover(&repo, &["--with-not-a-real-flag"]);
    assert!(
        !output.status.success(),
        "unknown flag must fail, not be swallowed"
    );
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        err.contains("--with-not-a-real-flag") || err.contains("unexpected argument"),
        "error should name the bad flag: {err}"
    );
}
