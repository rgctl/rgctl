//! CLI subprocess golden-path tests — Layer 2 (narrow end-to-end regressions).
//!
//! Spawns `CARGO_BIN_EXE_rbuilder` against a temp copy of
//! `tests/fixtures/tiny_polyglot_repo`. Uses default graph storage under
//! `{repo}/.rbuilder/` (unlike `all_commands_sanity.rs`, which forces `-d sandbox_graph.db`).
//!
//! | Test | Regression guarded |
//! |------|-------------------|
//! | `discover_json_emits_telemetry_on_stdout` | JSON discover = one stdout object, no progress text |
//! | `discover_initializes_tiny_polyglot_repo` | Text discover materializes graph artifacts |
//! | `blast_radius_json_exit_zero_after_discover` | Java v2 target metadata after ingest |
//! | `blast_radius_policy_violation_fails_closed_with_exit_one` | Policy breach → exit 1 + valid JSON |
//! | `blast_radius_fast_path_under_150ms` | T0 SQLite fast path on warm `publishEvent` cache |
//! | `blast_radius_with_slices_populates_handoffs` | `--with-slices` emits non-empty `gatekeeping.handoffs` |
//! | `blast_radius_with_slices_under_30s_after_cfg_discover` | T3 slice path with CFG archive (`br.slice.total_ms`) |
//!
//! Full command matrix: `all_commands_sanity.rs` + `docs/cli-io-sanity-audit.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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
fn discover_json_emits_telemetry_on_stdout() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let output = run_rbuilder(
        repo,
        &["-f", "json", "discover", ".", "--languages", "java,rust"],
    );

    assert!(
        output.status.success(),
        "discover -f json failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[✓] Indexed"),
        "human progress should not appear on stdout in json mode"
    );

    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("discover stdout must be valid JSON");
    assert_eq!(doc.get("schema_version").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(doc.get("command").and_then(|v| v.as_str()), Some("discover"));

    let metrics = doc.get("metrics").unwrap().as_object().unwrap();
    assert!(metrics.get("files_discovered").and_then(|v| v.as_u64()).unwrap() >= 4);
    assert!(metrics.get("files_indexed").and_then(|v| v.as_u64()).unwrap() >= 4);
    assert!(metrics.get("nodes_generated").and_then(|v| v.as_u64()).unwrap() > 0);
    assert!(metrics.get("edges_generated").and_then(|v| v.as_u64()).unwrap() > 0);
    assert!(metrics.get("duration_ms").and_then(|v| v.as_u64()).is_some());
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

    assert_eq!(doc.get("schema_version").and_then(|v| v.as_u64()), Some(2));
    for key in ["target", "metrics", "topology", "gatekeeping"] {
        assert!(doc.get(key).is_some(), "blast-radius JSON missing '{key}'");
    }

    let target = doc.get("target").unwrap().as_object().unwrap();
    assert_eq!(target.get("language").and_then(|v| v.as_str()), Some("java"));
    assert_eq!(
        target.get("canonical_fqn").and_then(|v| v.as_str()),
        Some("OrderService::process")
    );
    assert!(
        target.get("signature").and_then(|v| v.as_str()).is_some(),
        "Java overload should include signature text"
    );
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

#[test]
fn blast_radius_with_slices_populates_handoffs() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(repo, &["discover", ".", "--languages", "java,rust"]);
    assert!(discover.status.success(), "discover setup failed");

    let output = run_rbuilder(
        repo,
        &["-f", "json", "blast-radius", "publishEvent", "--with-slices"],
    );

    assert!(
        output.status.success(),
        "blast-radius --with-slices failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("blast-radius stdout must be valid JSON");
    let handoffs = doc["gatekeeping"]["handoffs"]
        .as_array()
        .expect("handoffs array");
    assert!(!handoffs.is_empty());
    assert_eq!(handoffs[0]["callee"].as_str(), Some("publishEvent"));
}

#[test]
fn blast_radius_with_slices_under_30s_after_cfg_discover() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(
        repo,
        &["discover", ".", "--languages", "java,rust", "--cfg"],
    );
    assert!(
        discover.status.success(),
        "discover --cfg failed: stderr={}",
        String::from_utf8_lossy(&discover.stderr)
    );
    assert!(
        repo.join(".rbuilder/analysis/cfg_pdg.archive.bin").exists(),
        "discover --cfg should write cfg_pdg archive"
    );

    let start = Instant::now();
    let output = run_rbuilder(
        repo,
        &["-f", "json", "blast-radius", "publishEvent", "--with-slices"],
    );
    let latency = start.elapsed();

    assert!(
        output.status.success(),
        "blast-radius --with-slices failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        latency < Duration::from_secs(30),
        "br.slice.total_ms regression: {latency:?} >= 30s"
    );

    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("blast-radius stdout must be valid JSON");
    let handoffs = doc["gatekeeping"]["handoffs"]
        .as_array()
        .expect("handoffs array");
    assert!(!handoffs.is_empty());
}

#[test]
fn blast_radius_fast_path_under_150ms() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(repo, &["discover", ".", "--languages", "java,rust"]);
    assert!(discover.status.success(), "discover setup failed");

    let start = Instant::now();
    let output = run_rbuilder(
        repo,
        &["-f", "json", "blast-radius", "publishEvent"],
    );
    let latency = start.elapsed();

    assert!(
        output.status.success(),
        "blast-radius fast path failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        latency < Duration::from_millis(150),
        "br.query.fast_path_ms regression: {latency:?} >= 150ms"
    );

    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim())
            .expect("blast-radius stdout must be valid JSON");
    assert_eq!(
        doc["target"]["symbol"].as_str(),
        Some("publishEvent")
    );
    assert!(doc.get("topology").is_some());
}

#[test]
fn check_policy_violation_fails_closed_with_exit_one() {
    let dir = materialize_fixture();
    let repo = dir.path();

    let discover = run_rbuilder(repo, &["discover", ".", "--languages", "java,rust"]);
    assert!(discover.status.success(), "discover setup failed");

    let policy_path = repo.join("strict_policy.json");
    fs::write(&policy_path, r#"{"max_impact_nodes": 0}"#).expect("write policy file");

    let output = run_rbuilder(
        repo,
        &[
            "-f",
            "json",
            "check",
            "--policy-file",
            policy_path.to_str().expect("policy path utf8"),
        ],
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "check policy breach must fail closed with exit code 1; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("violated check stdout must be valid JSON");
    assert_eq!(doc.get("passed").and_then(|v| v.as_bool()), Some(false));
    let violations = doc["violations"].as_array().expect("violations array");
    assert!(!violations.is_empty());
    assert!(
        violations.iter().any(|v| {
            v.get("symbol")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s == "publishEvent")
        }),
        "expected publishEvent violation, got {violations:?}"
    );
}
