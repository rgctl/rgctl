//! Integration tests for `discover --full` and serve-only HTTP (`serve` does not auto-pipeline).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rgctl_bin() -> PathBuf {
    if let Some(bin) = std::env::var_os("CARGO_BIN_EXE_rgctl") {
        return PathBuf::from(bin);
    }
    if let Some(bin) = std::env::var_os("CARGO_BIN_EXE_rgctl") {
        return PathBuf::from(bin);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl")
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.as_ref().join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(from, to)?;
        } else {
            fs::copy(from, to)?;
        }
    }
    Ok(())
}

fn materialize() -> (tempfile::TempDir, PathBuf) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny_polyglot_repo");
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    copy_dir_all(&fixture, &repo).expect("copy fixture");
    let _ = fs::remove_dir_all(repo.join(".rgctl"));
    let _ = fs::remove_dir_all(repo.join(".rbuilder"));
    fs::write(
        repo.join("java/com/example/Cart.java"),
        "package com.example;\npublic class Cart { private int total; }\n",
    )
    .expect("write field fixture");
    (tmp, repo)
}

fn run_in(repo: &Path, args: &[&str]) -> Output {
    Command::new(rgctl_bin())
        .args(args)
        .current_dir(repo)
        .output()
        .expect("spawn rgctl")
}

fn assert_ok(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed status={:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn discover_help_mentions_full() {
    let output = Command::new(rgctl_bin())
        .args(["discover", "--help"])
        .output()
        .expect("help");
    assert_ok(&output, "discover --help");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("--full"),
        "help should mention --full:\n{text}"
    );
}

#[test]
fn discover_full_writes_snapshot_dashboard_semantic_and_status() {
    let (_tmp, repo) = materialize();
    let repo_s = repo.to_str().unwrap();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo_s,
            "discover",
            ".",
            "--full",
            "--languages",
            "java,rust",
        ],
    );
    assert_ok(&output, "discover --full");

    assert!(repo.join(".rgctl/graph.snapshot.bin").is_file());
    assert!(repo.join(".rgctl/dashboard/index.html").is_file());
    assert!(repo.join(".rgctl/semantic_index.bin").is_file());
    let status: Value = serde_json::from_slice(
        &fs::read(repo.join(".rgctl/pipeline_status.json")).expect("status file"),
    )
    .expect("status json");
    let ids: Vec<&str> = status["plan"]
        .as_array()
        .expect("plan")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert_eq!(ids, ["basic_discover", "deep_pass", "semantic_index"]);
}

#[test]
fn discover_full_json_is_one_object_with_plan() {
    let (_tmp, repo) = materialize();
    let repo_s = repo.to_str().unwrap();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo_s,
            "-f",
            "json",
            "discover",
            ".",
            "--full",
            "--languages",
            "java,rust",
        ],
    );
    assert_ok(&output, "discover --full json");
    let doc: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout was not a single JSON object ({err}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(doc["schema_version"], 2);
    assert_eq!(doc["command"], "discover");
    assert_eq!(doc["full"], true);
    assert_eq!(doc["plan"].as_array().expect("plan").len(), 3);
}

#[test]
fn discover_full_does_not_require_taint() {
    let (_tmp, repo) = materialize();
    let repo_s = repo.to_str().unwrap();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo_s,
            "discover",
            ".",
            "--full",
            "--languages",
            "java,rust",
        ],
    );
    assert_ok(&output, "discover --full without taint");
    let taint = repo.join(".rgctl/dashboard/taint_index.json");
    if taint.is_file() {
        let doc: Value = serde_json::from_slice(&fs::read(taint).unwrap()).unwrap();
        assert_ne!(doc["available"].as_bool(), Some(true));
    }
}

#[test]
fn discover_full_second_run_and_missing_dashboard() {
    let (_tmp, repo) = materialize();
    let repo_s = repo.to_str().unwrap();
    let args = [
        "-r",
        repo_s,
        "-f",
        "json",
        "discover",
        ".",
        "--full",
        "--languages",
        "java,rust",
    ];
    assert_ok(&run_in(&repo, &args), "first --full");
    let second = run_in(&repo, &args);
    assert_ok(&second, "second --full");
    let doc: Value = serde_json::from_slice(&second.stdout).expect("json");
    let status = doc["plan"].as_array().unwrap()[0]["status"]
        .as_str()
        .unwrap_or("");
    assert!(status == "skipped" || status == "complete");

    fs::remove_file(repo.join(".rgctl/dashboard/index.html")).ok();
    let third = run_in(&repo, &args);
    assert_ok(&third, "third --full after deleting dashboard");
    let doc: Value = serde_json::from_slice(&third.stdout).expect("json");
    assert_ne!(
        doc["plan"].as_array().unwrap()[1]["status"].as_str(),
        Some("skipped")
    );
    assert!(repo.join(".rgctl/dashboard/index.html").is_file());
}

#[test]
fn discover_full_overlapping_lock_fails() {
    let (_tmp, repo) = materialize();
    fs::create_dir_all(repo.join(".rgctl")).unwrap();
    fs::write(repo.join(".rgctl/pipeline.lock"), b"99999\n").unwrap();
    let repo_s = repo.to_str().unwrap();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo_s,
            "discover",
            ".",
            "--full",
            "--languages",
            "java,rust",
        ],
    );
    assert!(!output.status.success());
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.to_ascii_lowercase().contains("already running") || err.contains("lock"),
        "{err}"
    );
}

#[test]
fn serve_without_dashboard_exits() {
    let (_tmp, repo) = materialize();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo.to_str().unwrap(),
            "serve",
            "--port",
            "18080",
        ],
    );
    assert!(
        !output.status.success(),
        "serve must fail fast without a dashboard (no auto-pipeline)"
    );
    let err = String::from_utf8_lossy(&output.stderr);
    assert!(
        err.contains("dashboard") || err.contains("discover") || err.contains("universe"),
        "{err}"
    );
}

#[test]
fn serve_deprecated_no_pipeline_flag_still_accepted() {
    let (_tmp, repo) = materialize();
    let output = run_in(
        &repo,
        &[
            "-r",
            repo.to_str().unwrap(),
            "serve",
            "--no-pipeline",
            "--port",
            "18081",
        ],
    );
    // Same fail-fast as default; flag is a hidden no-op for old scripts.
    assert!(!output.status.success());
}

#[test]
fn serve_help_lists_http_flags() {
    let help = Command::new(rgctl_bin())
        .args(["serve", "--help"])
        .output()
        .expect("help");
    assert_ok(&help, "serve --help");
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("--port"), "{text}");
    assert!(
        text.to_ascii_lowercase().contains("does not")
            || text.contains("serve-only")
            || text.contains("discover"),
        "help should say serve does not index: {text}"
    );
    assert!(
        !text.contains("--no-pipeline"),
        "--no-pipeline is hidden/deprecated: {text}"
    );
}

#[test]
fn service_crate_does_not_depend_on_package_rgctl() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let toml = fs::read_to_string(root.join("crates/rgctl-service/Cargo.toml"))
        .expect("read rgctl-service Cargo.toml");
    assert!(
        !toml.contains("\nrgctl =") && !toml.contains("\nrgctl={"),
        "rgctl-service must not depend on package rgctl:\n{toml}"
    );
}
