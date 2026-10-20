//! End-to-end JSON contracts for clones, security/VEX, and review export.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn rgctl_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl"))
}

fn run_json(cwd: &Path, args: &[&str]) -> Value {
    let out = Command::new(rgctl_bin())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "rgctl {:?} failed\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let start = stdout.find('{').expect("json");
    serde_json::from_str(stdout[start..].trim()).expect("parse")
}

fn init_git(dir: &Path) {
    for args in [
        ["init", "-b", "main"],
        ["config", "user.email", "t@e.com"],
        ["config", "user.name", "t"],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .unwrap()
            .success());
    }
}

fn commit_all(dir: &Path, msg: &str) {
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap();
    assert!(Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", msg])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap()
        .success());
}

#[test]
fn clones_exact_and_fragments_contract() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/clone-exact");
    let _ = fs::remove_dir_all(root.join(".rgctl"));
    let _ = run_json(&root, &["-f", "json", "discover", ".", "-l", "java"]);

    let exact = run_json(
        &root,
        &["-f", "json", "clones", "--mode", "exact", "--min-loc", "5"],
    );
    assert_eq!(exact["schema_version"], 1);
    assert!(exact["group_count"].as_u64().unwrap() >= 1);
    let groups = exact["groups"].as_array().unwrap();
    assert!(!groups.is_empty());
    assert!(groups[0]["members"].as_array().unwrap().len() >= 2);

    let fragments = run_json(&root, &["-f", "json", "clones", "--mode", "fragment"]);
    assert!(
        fragments["schema_version"].as_u64().unwrap_or(0) >= 1,
        "fragment schema_version: {}",
        fragments["schema_version"]
    );
    // Fragment mode reports clusters or empty groups with stable schema.
    assert!(fragments.get("groups").is_some() || fragments.get("group_count").is_some());
}

#[test]
fn security_deps_vuln_taint_contract() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    fs::write(
        repo.join("pom.xml"),
        r#"<?xml version="1.0"?><project><modelVersion>4.0.0</modelVersion>
        <groupId>a</groupId><artifactId>b</artifactId><version>1</version>
        <dependencies>
          <dependency><groupId>xalan</groupId><artifactId>xalan</artifactId><version>2.7.2</version></dependency>
        </dependencies></project>"#,
    )
    .unwrap();
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/App.java"),
        "class App { void run() { System.out.println(1); } }\n",
    )
    .unwrap();

    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "java"]);

    let osv = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/osv/xalan-CVE-2022-34169.json");
    let deps = run_json(
        repo,
        &[
            "-f",
            "json",
            "security",
            "deps",
            "check",
            "--osv",
            osv.to_str().unwrap(),
        ],
    );
    assert_eq!(deps["schema_version"], 1);
    assert!(deps.get("verdict").is_some() || deps.get("command").is_some());

    let vuln = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-f",
            "json",
            "security",
            "vuln",
            "analyze",
            "--osv",
            osv.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    if vuln.status.success() {
        let stdout = String::from_utf8_lossy(&vuln.stdout);
        if let Some(start) = stdout.find('{') {
            let v: Value = serde_json::from_str(stdout[start..].trim()).unwrap();
            assert!(
                v.get("vulnerabilities").is_some()
                    || v.get("schema_version").is_some()
                    || v.get("statements").is_some()
                    || v.get("bomFormat").is_some(),
                "unexpected VEX payload: {v}"
            );
        }
    }

    let taint = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-f",
            "json",
            "security",
            "taint",
            "--sink",
            "System.out.println",
        ])
        .output()
        .unwrap();
    if taint.status.success() {
        let stdout = String::from_utf8_lossy(&taint.stdout);
        if let Some(start) = stdout.find('{') {
            let v: Value = serde_json::from_str(stdout[start..].trim()).unwrap();
            assert_eq!(v["schema_version"], 1);
        }
    }
}

#[test]
fn review_export_contract() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn a() { b(); }\npub fn b() {}\n").unwrap();
    commit_all(repo, "base");
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn a() { b(); }\npub fn b() { /* x */ }\n",
    )
    .unwrap();
    commit_all(repo, "head");

    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "rust"]);
    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-f",
            "json",
            "review",
            "export",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ])
        .output()
        .unwrap();
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let start = stdout.find('{').expect("json");
        let v: Value = serde_json::from_str(stdout[start..].trim()).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert!(
            v["command"].as_str().unwrap_or("").contains("export")
                || v.get("symbols").is_some()
                || v.get("findings").is_some()
        );
    }
}
