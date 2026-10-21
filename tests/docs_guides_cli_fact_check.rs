//! Fact-check core guide CLI commands + schema_version / JSON keys against
//! `rgctl-tests/ecommerce-java` (always) and `example/coolstore` (when present).
//!
//! ```bash
//! cargo test --test docs_guides_cli_fact_check
//! ```

mod rgctl_harness;

use rgctl_harness::{assert_ok, cli_json, copy_tree, rgctl, run_in_repo};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn ecommerce_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rgctl-tests/ecommerce-java")
}

fn coolstore_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example/coolstore")
}

fn materialize_ecommerce() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("ecommerce-java");
    copy_tree(&ecommerce_src(), &repo);
    let _ = fs::remove_dir_all(repo.join(".rgctl"));
    (tmp, repo)
}

/// Clean CoolStore via `git archive HEAD` when `.git` exists; otherwise copy excluding dirt.
fn materialize_coolstore() -> Option<(tempfile::TempDir, PathBuf)> {
    let src = coolstore_src();
    if !src.is_dir() {
        return None;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("coolstore");
    fs::create_dir_all(&repo).unwrap();

    let real = fs::canonicalize(&src).unwrap_or(src.clone());
    let git_dir = real.join(".git");
    if git_dir.exists() {
        let status = Command::new("git")
            .args(["-C", real.to_str().unwrap(), "archive", "HEAD"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                let tar = Command::new("tar")
                    .args(["-x", "-C", repo.to_str().unwrap()])
                    .stdin(child.stdout.take().unwrap())
                    .status()?;
                let _ = child.wait()?;
                Ok(tar)
            });
        match status {
            Ok(s) if s.success() => {}
            _ => {
                copy_tree(&real, &repo);
                scrub_coolstore_dirt(&repo);
            }
        }
    } else {
        copy_tree(&real, &repo);
        scrub_coolstore_dirt(&repo);
    }
    Some((tmp, repo))
}

fn scrub_coolstore_dirt(repo: &Path) {
    for name in [".rgctl", "rgctl-benchmark", ".claude", "docs"] {
        let _ = fs::remove_dir_all(repo.join(name));
    }
    let _ = fs::remove_file(repo.join("policy.json"));
}

fn discover_java_cfg(repo: &Path) {
    assert_ok(
        &run_in_repo(
            repo,
            &[
                "discover",
                ".",
                "-l",
                "java",
                "--with-cfg",
                "--with-harmonic",
            ],
        ),
        "discover -l java --with-cfg --with-harmonic",
    );
}

fn assert_keys(v: &Value, keys: &[&str]) {
    for k in keys {
        assert!(
            v.get(*k).is_some(),
            "missing key `{k}` in {}",
            v
        );
    }
}

fn assert_absent(v: &Value, keys: &[&str]) {
    for k in keys {
        assert!(
            v.get(*k).is_none(),
            "unexpected key `{k}` (stale guide field) in {}",
            v
        );
    }
}

#[test]
fn ecommerce_java_guide_commands_schema_and_keys() {
    let (_tmp, repo) = materialize_ecommerce();
    discover_java_cfg(&repo);

    let status = cli_json(&repo, &["status"]);
    assert_eq!(status["schema_version"], 3);
    assert_keys(&status, &["nodes", "edges", "command", "status", "capabilities"]);

    let find = cli_json(&repo, &["find", "--type", "function", "--count-only"]);
    assert_eq!(find["schema_version"], 2);
    assert!(find["total"].as_u64().unwrap_or(0) > 0);

    let inventory = cli_json(&repo, &["inventory"]);
    assert!(inventory["counts"].is_array());

    let callers = cli_json(
        &repo,
        &["callers", "clearCart", "--class", "CartService"],
    );
    assert!(callers["callers"].is_array());

    let callees = cli_json(
        &repo,
        &["callees", "clearCart", "--class", "CartService"],
    );
    assert!(callees["callees"].is_array());

    let blast = cli_json(
        &repo,
        &["blast-radius", "clearCart", "--class", "CartService"],
    );
    assert_eq!(blast["schema_version"], 2);
    assert_keys(&blast, &["metrics", "target", "topology"]);

    // Flags precede the layer subcommand: `inspect SYM --class … cfg`.
    let cfg = cli_json(
        &repo,
        &["inspect", "checkout", "--class", "OrderService", "cfg"],
    );
    assert_eq!(cfg["schema_version"], 1);
    assert_keys(&cfg, &["nodes", "edges", "layer", "symbol", "pruned"]);
    assert_absent(&cfg, &["node_count", "edge_count", "nodes_sample"]);

    let pdg = cli_json(
        &repo,
        &["inspect", "checkout", "--class", "OrderService", "pdg"],
    );
    assert_keys(&pdg, &["nodes", "edges", "data_deps", "control_deps", "layer"]);

    let dom = cli_json(
        &repo,
        &["inspect", "checkout", "--class", "OrderService", "dom"],
    );
    assert_keys(&dom, &["idom", "frontiers", "nodes", "layer"]);

    let cpg_status = cli_json(&repo, &["cpg", "status"]);
    assert_eq!(cpg_status["schema_version"], 1);
    assert_keys(
        &cpg_status,
        &[
            "archive_present",
            "field_write_count",
            "function_count",
            "graph_digest",
        ],
    );

    let cpg_calls = cli_json(
        &repo,
        &["cpg", "calls", "checkout", "--class", "OrderService"],
    );
    assert_eq!(cpg_calls["schema_version"], 1);
    assert_keys(
        &cpg_calls,
        &["edges", "function_id", "function_name", "schema_version"],
    );
    assert_absent(&cpg_calls, &["edges_sample", "returned"]);

    let metrics = cli_json(&repo, &["metrics", "--pagerank"]);
    assert!(metrics["pagerank"]["top"].is_array());

    let communities = cli_json(&repo, &["communities", "list"]);
    assert!(communities["communities"].is_array());

    let clones = cli_json(&repo, &["clones"]);
    assert_eq!(clones["schema_version"], 1);
    assert!(clones["groups"].is_array());

    // Plain discover -f json → DiscoverResponse telemetry
    let plain = cli_json(&repo, &["discover", ".", "-l", "java"]);
    assert_eq!(plain["schema_version"], 2);
    assert_eq!(plain["command"], "discover");
    assert!(plain["metrics"]["files_discovered"].as_u64().unwrap_or(0) >= 1);

    // --export-migration-hints → migration plan on stdout + artifact
    let mig = cli_json(
        &repo,
        &[
            "discover",
            ".",
            "-l",
            "java",
            "--export-migration-hints",
        ],
    );
    assert_eq!(mig["schema_version"], 2);
    assert!(mig["steps"].is_array());
    assert!(mig.get("preset").is_some());
    assert!(
        repo.join(".rgctl/migration_plan.json").is_file(),
        "expected .rgctl/migration_plan.json"
    );

    let install_list = cli_json(&repo, &["install", "--list-agents"]);
    assert!(install_list["agents"].is_array());

    // Actual install write report uses schema_version 3
    let pack_tmp = tempfile::tempdir().unwrap();
    let out = Command::new(rgctl())
        .current_dir(pack_tmp.path())
        .args([
            "-r",
            pack_tmp.path().to_str().unwrap(),
            "-f",
            "json",
            "install",
            "--skill",
            "--tools",
            "cursor",
        ])
        .output()
        .expect("install");
    assert_ok(&out, "install --skill");
    let install: Value = serde_json::from_slice(&out.stdout).expect("install json");
    assert_eq!(install["schema_version"], 3);
    assert_eq!(install["command"], "install");
}

#[test]
fn coolstore_guide_commands_when_present() {
    let Some((_tmp, repo)) = materialize_coolstore() else {
        eprintln!("skip: example/coolstore not present");
        return;
    };

    let java_count = walkdir_java_count(&repo);
    assert_eq!(
        java_count, 26,
        "clean CoolStore should have 26 Java sources; got {java_count}"
    );

    discover_java_cfg(&repo);

    let status = cli_json(&repo, &["status"]);
    assert_eq!(status["schema_version"], 3);
    assert_eq!(status["nodes"], 608);
    assert_eq!(status["edges"], 1709);

    let find = cli_json(&repo, &["find", "--type", "function", "--count-only"]);
    assert_eq!(find["total"], 152);

    let cpg_status = cli_json(&repo, &["cpg", "status"]);
    assert_eq!(cpg_status["field_write_count"], 41);

    let cfg = cli_json(&repo, &["inspect", "priceShoppingCart", "cfg"]);
    assert_keys(&cfg, &["nodes", "edges", "layer", "symbol"]);
    assert_absent(&cfg, &["node_count", "edge_count", "nodes_sample"]);
    assert_eq!(cfg["nodes"].as_array().unwrap().len(), 12);
    assert_eq!(cfg["edges"].as_array().unwrap().len(), 16);

    let pdg = cli_json(&repo, &["inspect", "priceShoppingCart", "pdg"]);
    assert_keys(&pdg, &["data_deps", "control_deps", "nodes", "edges"]);

    let dom = cli_json(&repo, &["inspect", "priceShoppingCart", "dom"]);
    assert_keys(&dom, &["idom", "frontiers", "nodes"]);

    let calls = cli_json(&repo, &["cpg", "calls", "priceShoppingCart"]);
    assert_keys(&calls, &["edges", "function_id", "function_name"]);
    assert_absent(&calls, &["edges_sample", "returned"]);
    assert_eq!(calls["function_name"], "priceShoppingCart");

    let export_out = repo.join("fn-export.json");
    let exp = run_in_repo(
        &repo,
        &[
            "export",
            "--export-format",
            "json",
            "--export-output",
            export_out.to_str().unwrap(),
            "--query",
            "type:Function",
        ],
    );
    assert_ok(&exp, "export type:Function");
    let msg = format!(
        "{}{}",
        String::from_utf8_lossy(&exp.stdout),
        String::from_utf8_lossy(&exp.stderr)
    );
    assert!(
        msg.contains("152 nodes, 130 edges"),
        "unexpected export Function counts: {msg}"
    );

    let export_all = repo.join("all-export.json");
    let exp_all = run_in_repo(
        &repo,
        &[
            "export",
            "--export-format",
            "json",
            "--export-output",
            export_all.to_str().unwrap(),
            "--query",
            "all",
        ],
    );
    assert_ok(&exp_all, "export all");
    let msg_all = format!(
        "{}{}",
        String::from_utf8_lossy(&exp_all.stdout),
        String::from_utf8_lossy(&exp_all.stderr)
    );
    assert!(
        msg_all.contains("608 nodes, 1709 edges"),
        "unexpected export all counts: {msg_all}"
    );
}

fn walkdir_java_count(repo: &Path) -> usize {
    fn walk(p: &Path, n: &mut usize) {
        let Ok(rd) = fs::read_dir(p) else { return };
        for ent in rd.flatten() {
            let path = ent.path();
            if path.is_dir() {
                let name = ent.file_name();
                if name == ".git" || name == "target" || name == ".rgctl" {
                    continue;
                }
                walk(&path, n);
            } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(repo, &mut n);
    n
}
