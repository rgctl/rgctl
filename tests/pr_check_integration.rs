//! Integration tests for `rgctl pr-check` (temporal policy + snapshot pair).

use rgctl_graph::backend::GraphBackend;
use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::write_columnar_from_nodes_edges;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn rgctl_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl")
        })
}

fn write_target_snapshot(path: &Path, caller_count: usize) {
    let mut backend = rgctl_graph::backend::MemoryBackend::new();
    let target = Node::new(NodeType::Function, "target_fn").with_file_path("src/pkg.rs");
    let target_id = target.id;
    backend.insert_node(target).unwrap();
    for i in 0..caller_count {
        let caller =
            Node::new(NodeType::Function, format!("caller{i}")).with_file_path("src/pkg.rs");
        let caller_id = caller.id;
        backend.insert_node(caller).unwrap();
        backend
            .insert_edge(Edge::new(caller_id, target_id, EdgeType::Calls))
            .unwrap();
    }
    write_columnar_from_nodes_edges(
        backend.all_nodes().unwrap(),
        backend.all_edges().unwrap(),
        path,
    )
    .unwrap();
}

fn init_git_repo(dir: &Path) {
    for args in [
        ["init", "-b", "main"],
        ["config", "user.email", "test@example.com"],
        ["config", "user.name", "test"],
    ] {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?} failed", args);
    }
}

fn git_commit_all(dir: &Path, message: &str) {
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    let out = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", message])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn setup_pr_repo(base_leaves: usize, head_leaves: usize) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/pkg.rs"), "fn target_fn() {}\n").unwrap();
    fs::create_dir_all(repo.join(".rgctl")).unwrap();
    write_target_snapshot(&repo.join(".rgctl/graph.snapshot.bin"), base_leaves);
    git_commit_all(repo, "base");

    fs::write(repo.join("src/pkg.rs"), "fn target_fn() { /* changed */ }\n").unwrap();
    write_target_snapshot(&repo.join(".rgctl/graph.snapshot.bin"), head_leaves);
    git_commit_all(repo, "head");

    fs::create_dir_all(repo.join(".rgctl-base/.rgctl")).unwrap();
    write_target_snapshot(
        &repo.join(".rgctl-base/.rgctl/graph.snapshot.bin"),
        base_leaves,
    );

    tmp
}

fn run_pr_check(repo: &Path, policy: &Path, extra_args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(rgctl_bin());
    cmd.current_dir(repo)
        .arg("-r")
        .arg(repo)
        .arg("-f")
        .arg("json")
        .arg("pr-check")
        .arg("--policy-file")
        .arg(policy)
        .arg("--full-snapshots");
    cmd.args(extra_args);
    cmd.output().expect("spawn rgctl pr-check")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_all(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn pr_check_passes_when_only_existing_violations() {
    let tmp = setup_pr_repo(6, 6);
    let repo = tmp.path();
    let policy = repo.join("policy.json");
    fs::write(
        &policy,
        r#"{
          "max_impact_nodes": 5,
          "scope": { "new_violations_only": true }
        }"#,
    )
    .unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD~1", "--head-ref", "HEAD"],
    );
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert_eq!(doc["schema_version"].as_str(), Some("2"));
    assert_eq!(doc["passed"].as_bool(), Some(true));
    assert!(doc["violations_summary"].is_object());
    assert!(doc["graph_diff"].is_object());
    assert!(doc["scope"]["files"].as_u64().unwrap() >= 1);
}

#[test]
fn pr_check_fails_on_new_temporal_violation() {
    let tmp = setup_pr_repo(0, 6);
    let repo = tmp.path();
    let policy = repo.join("policy.json");
    fs::write(
        &policy,
        r#"{
          "max_impact_nodes": 5,
          "scope": { "new_violations_only": true }
        }"#,
    )
    .unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD~1", "--head-ref", "HEAD"],
    );
    assert!(!out.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert_eq!(doc["passed"].as_bool(), Some(false));
    let violations = doc["violations"].as_array().expect("violations");
    assert!(
        violations
            .iter()
            .any(|v| v["classification"].as_str() == Some("new"))
    );
}

#[test]
fn pr_check_delta_head_synthesizes_from_base_only() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\npub fn target() {}\n").unwrap();
    git_commit_all(repo, "base");

    let discover = Command::new(rgctl_bin())
        .args(["-r", repo.to_str().unwrap(), "discover", "."])
        .current_dir(repo)
        .output()
        .expect("discover");
    assert!(
        discover.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&discover.stderr)
    );

    fs::create_dir_all(repo.join(".rgctl-base")).unwrap();
    copy_dir_all(&repo.join(".rgctl"), &repo.join(".rgctl-base/.rgctl"));

    fs::write(
        repo.join("src/lib.rs"),
        "pub fn helper() {}\npub fn target() { let _ = 1; }\n",
    )
    .unwrap();
    git_commit_all(repo, "head");
    fs::remove_file(repo.join(".rgctl/graph.snapshot.bin")).unwrap();

    let policy = repo.join("policy.json");
    fs::write(&policy, r#"{"max_impact_nodes": 500}"#).unwrap();

    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "-f",
            "json",
            "pr-check",
            "--policy-file",
            policy.to_str().unwrap(),
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ])
        .output()
        .expect("pr-check delta");
    assert!(
        out.status.success(),
        "delta pr-check failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert_eq!(doc["schema_version"].as_str(), Some("2"));
    assert!(doc["scope"]["files"].as_u64().unwrap() >= 1);
    assert!(
        !repo.join(".rgctl/graph.snapshot.bin").is_file(),
        "delta synth must not recreate live session snapshot"
    );
    assert!(
        repo.join(".rgctl/synth-head/graph.snapshot.bin").is_file(),
        "synthesized head must land under .rgctl/synth-head/"
    );
}

#[test]
fn pr_check_regression_from_violation_ledger() {
    let tmp = setup_pr_repo(0, 6);
    let repo = tmp.path();
    let policy = repo.join("policy.json");
    fs::write(
        &policy,
        r#"{
          "max_impact_nodes": 5,
          "scope": { "new_violations_only": true, "fail_on_regression": true }
        }"#,
    )
    .unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD~1", "--head-ref", "HEAD"],
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    let stable_key = doc["violations"][0]["stable_key"].as_u64().expect("stable_key");

    let ledger_line = format!(
        r#"{{"stable_key":{},"rule":"max_impact_nodes","class":"resolved","commit":"abc","ts":"1","symbol":"target_fn"}}"#,
        stable_key
    );
    fs::write(repo.join(".rgctl/violation_ledger.jsonl"), ledger_line).unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD~1", "--head-ref", "HEAD"],
    );
    assert!(!out.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert!(
        doc["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["classification"].as_str() == Some("regression"))
    );
    assert_eq!(doc["violations_summary"]["regression"].as_u64(), Some(1));
}

#[test]
fn pr_check_bisect_pins_introducing_commit() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/pkg.rs"), "fn target_fn() {}\n").unwrap();
    git_commit_all(repo, "c1");

    let discover = Command::new(rgctl_bin())
        .args(["-r", repo.to_str().unwrap(), "discover", "."])
        .current_dir(repo)
        .output()
        .expect("discover c1");
    assert!(discover.status.success());
    fs::create_dir_all(repo.join(".rgctl-base")).unwrap();
    copy_dir_all(&repo.join(".rgctl"), &repo.join(".rgctl-base/.rgctl"));

    fs::write(repo.join("src/pkg.rs"), "fn target_fn() { /* noop */ }\n").unwrap();
    git_commit_all(repo, "c2");

    fs::write(
        repo.join("src/pkg.rs"),
        "fn caller0() { target_fn(); }\nfn caller1() { target_fn(); }\nfn caller2() { target_fn(); }\nfn caller3() { target_fn(); }\nfn caller4() { target_fn(); }\nfn caller5() { target_fn(); }\nfn target_fn() {}\n",
    )
    .unwrap();
    git_commit_all(repo, "c3");

    fs::write(
        repo.join("src/pkg.rs"),
        "fn caller0() { target_fn(); }\nfn caller1() { target_fn(); }\nfn caller2() { target_fn(); }\nfn caller3() { target_fn(); }\nfn caller4() { target_fn(); }\nfn caller5() { target_fn(); }\nfn target_fn() { /* still */ }\n",
    )
    .unwrap();
    git_commit_all(repo, "c4");

    let policy = repo.join("policy.json");
    fs::write(
        &policy,
        r#"{"max_impact_nodes": 5, "scope": { "new_violations_only": true }}"#,
    )
    .unwrap();

    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "-f",
            "json",
            "pr-check",
            "--policy-file",
            policy.to_str().unwrap(),
            "--base-ref",
            "HEAD~3",
            "--head-ref",
            "HEAD",
            "--bisect",
        ])
        .output()
        .expect("bisect pr-check");
    assert!(
        !out.status.success(),
        "expected failure:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    let stable_key = doc["violations"][0]["stable_key"].as_u64().expect("stable_key");
    let introduced = doc["violations"][0]["introduced_in_commit"]
        .as_str()
        .expect("introduced_in_commit");
    let expected =
        first_commit_with_new_violation(repo, &policy, "HEAD~3", "HEAD", stable_key);
    assert_eq!(introduced, expected);
}

fn first_commit_with_new_violation(
    repo: &Path,
    policy: &Path,
    base_ref: &str,
    head_ref: &str,
    stable_key: u64,
) -> String {
    let revs = Command::new("git")
        .args(["rev-list", "--reverse", &format!("{base_ref}..{head_ref}")])
        .current_dir(repo)
        .output()
        .expect("rev-list");
    assert!(revs.status.success());
    for line in String::from_utf8_lossy(&revs.stdout).lines() {
        let commit = line.trim();
        if commit.is_empty() {
            continue;
        }
        let out = Command::new(rgctl_bin())
            .current_dir(repo)
            .args([
                "-r",
                repo.to_str().unwrap(),
                "-f",
                "json",
                "pr-check",
                "--policy-file",
                policy.to_str().unwrap(),
                "--base-ref",
                base_ref,
                "--head-ref",
                commit,
            ])
            .output()
            .expect("probe pr-check");
        if !out.status.success() {
            let doc: serde_json::Value =
                serde_json::from_slice(&out.stdout).expect("probe json");
            if doc["violations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| {
                    v["stable_key"].as_u64() == Some(stable_key)
                        && v["classification"].as_str() == Some("new")
                })
            {
                return commit.to_string();
            }
        }
    }
    panic!("no introducing commit found in {base_ref}..{head_ref}");
}

#[test]
fn pr_check_calendar_grace_warns_by_default() {
    let tmp = setup_pr_repo(6, 6);
    let repo = tmp.path();
    let policy = repo.join("policy.json");
    fs::write(
        &policy,
        r#"{
          "max_impact_nodes": 5,
          "scope": { "new_violations_only": false },
          "temporal": {
            "effective_from": "2026-09-01",
            "grace_period_days": 90,
            "severity_during_grace": "warn"
          }
        }"#,
    )
    .unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD~1", "--head-ref", "HEAD"],
    );
    assert!(
        out.status.success(),
        "grace should pass:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert_eq!(
        doc["violations"][0]["severity"].as_str(),
        Some("warn")
    );

    let strict = run_pr_check(
        repo,
        &policy,
        &[
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
            "--strict-calendar",
        ],
    );
    assert!(!strict.status.success());
}

#[test]
fn pr_check_worktree_synthetic_head() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/pkg.rs"), "fn target_fn() {}\n").unwrap();
    git_commit_all(repo, "base");

    let discover = Command::new(rgctl_bin())
        .args(["-r", repo.to_str().unwrap(), "discover", "."])
        .current_dir(repo)
        .output()
        .expect("discover");
    assert!(discover.status.success());

    // No `.rgctl-base/`: live session snapshot is the base (scratch head isolation).
    let live = repo.join(".rgctl/graph.snapshot.bin");
    let live_before = fs::read(&live).expect("live snapshot");

    fs::write(
        repo.join("src/pkg.rs"),
        "fn target_fn() {}\nfn extra_fn() {}\n",
    )
    .unwrap();

    let policy = repo.join("policy.json");
    fs::write(&policy, r#"{"max_impact_nodes": 500}"#).unwrap();

    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "-f",
            "json",
            "pr-check",
            "--policy-file",
            policy.to_str().unwrap(),
            "--synthetic-head",
            "worktree",
        ])
        .output()
        .expect("worktree pr-check");
    assert!(
        out.status.success(),
        "worktree pr-check failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout json");
    assert!(doc["scope"]["files"].as_u64().unwrap() >= 1);
    assert_eq!(
        fs::read(&live).expect("live after"),
        live_before,
        "worktree synth must not clobber live session snapshot"
    );
    assert!(repo.join(".rgctl/synth-head/graph.snapshot.bin").is_file());
}

#[test]
fn pr_check_strict_fails_on_empty_commit_diff() {
    let tmp = setup_pr_repo(0, 0);
    let repo = tmp.path();
    let policy = repo.join("policy.json");
    fs::write(&policy, r#"{"max_impact_nodes": 50}"#).unwrap();

    let out = run_pr_check(
        repo,
        &policy,
        &["--base-ref", "HEAD", "--head-ref", "HEAD", "--strict"],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("strict")
            || String::from_utf8_lossy(&out.stdout).contains("strict")
    );
}
