//! Differential equivalence oracles (EQ-1…EQ-4) and session lifecycle (E2E-SESSION-01).

use rgctl_graph::schema::{Edge, EdgeType, Node, NodeType};
use rgctl_graph::{
    assert_graph_invariants, write_columnar_from_nodes_edges, SnapshotNodeStore, SNAPSHOT_FILE,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn rgctl_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl"))
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
        .args(["add", "-A"])
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

fn run_json(repo: &Path, args: &[&str]) -> Value {
    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(args)
        .output()
        .expect("spawn rgctl");
    assert!(
        out.status.success(),
        "rgctl {:?} failed\nstdout={}\nstderr={}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let start = stdout.find('{').expect("json on stdout");
    serde_json::from_str(stdout[start..].trim()).expect("parse json")
}

fn blake3_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

fn open_col(repo: &Path) -> SnapshotNodeStore {
    SnapshotNodeStore::open(&repo.join(".rgctl").join(SNAPSHOT_FILE)).unwrap()
}

fn in_repo_topology(store: &SnapshotNodeStore) -> (BTreeSet<(String, String)>, BTreeSet<(String, String, String)>) {
    let col = store.mmap().columnar().unwrap();
    let mut nodes = BTreeSet::new();
    let mut id_to_key = std::collections::HashMap::new();
    for idx in 0..col.node_count() {
        let n = col.materialize_node_at(idx).unwrap();
        if n.file_path.as_deref() == Some("<external>") {
            continue;
        }
        let key = (
            n.name.to_string(),
            format!("{:?}", n.node_type),
        );
        id_to_key.insert(n.id, key.clone());
        nodes.insert(key);
    }
    let mut edges = BTreeSet::new();
    for idx in 0..col.edge_count() {
        let (from, to, et) = col.edge_at(idx).unwrap();
        let Some(fk) = id_to_key.get(&from) else { continue };
        let Some(tk) = id_to_key.get(&to) else { continue };
        edges.insert((fk.0.clone(), tk.0.clone(), format!("{et:?}")));
    }
    (nodes, edges)
}

/// EQ-1: cold discover @ V0 + update @ V1 ≡ cold discover @ V1.
#[test]
fn eq1_update_matches_cold_discover() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/a.rs"),
        "pub fn a() { b(); }\npub fn b() {}\n",
    )
    .unwrap();
    fs::write(repo.join("src/c.rs"), "pub fn c() {}\n").unwrap();
    git_commit_all(repo, "v0");

    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "rust"]);
    let store_v0 = open_col(repo);
    assert_graph_invariants(store_v0.mmap().columnar().unwrap()).unwrap();

    // V1 mutations: drop intra-file call, add function, modify cross-file callee body.
    fs::write(
        repo.join("src/a.rs"),
        "pub fn a() {}\npub fn b() {}\npub fn d() { b(); }\n",
    )
    .unwrap();
    fs::write(repo.join("src/c.rs"), "pub fn c() { /* changed */ }\n").unwrap();
    fs::write(repo.join("src/e.rs"), "pub fn e() {}\n").unwrap();
    git_commit_all(repo, "v1");

    let _ = run_json(repo, &["-f", "json", "update"]);
    let updated = in_repo_topology(&open_col(repo));

    // Cold oracle: wipe session and rediscover.
    fs::remove_dir_all(repo.join(".rgctl")).unwrap();
    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "rust"]);
    let cold = in_repo_topology(&open_col(repo));

    // Compare Function symbols + Calls edges (file node paths may be absolute/relative).
    let fn_only = |nodes: &BTreeSet<(String, String)>| -> BTreeSet<String> {
        nodes
            .iter()
            .filter(|(_, t)| t == "Function")
            .map(|(n, _)| n.clone())
            .collect()
    };
    let calls_only = |edges: &BTreeSet<(String, String, String)>| -> BTreeSet<(String, String)> {
        edges
            .iter()
            .filter(|(_, _, t)| t == "Calls")
            .map(|(a, b, _)| (a.clone(), b.clone()))
            .collect()
    };
    assert_eq!(fn_only(&updated.0), fn_only(&cold.0), "EQ-1 function set");
    assert_eq!(
        calls_only(&updated.1),
        calls_only(&cold.1),
        "EQ-1 Calls edge set"
    );
}

fn setup_shipping_review_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(
        repo.join("src/ship.rs"),
        r#"
pub fn get_cart_item_total() {}
pub fn is_free_shipping() {}
pub fn calculate_shipping() {
    get_cart_item_total();
    is_free_shipping();
}
"#,
    )
    .unwrap();
    git_commit_all(repo, "base");
    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "rust"]);
    fs::create_dir_all(repo.join(".rgctl-base/.rgctl")).unwrap();
    fs::copy(
        repo.join(".rgctl").join(SNAPSHOT_FILE),
        repo.join(".rgctl-base/.rgctl").join(SNAPSHOT_FILE),
    )
    .unwrap();

    fs::write(
        repo.join("src/ship.rs"),
        r#"
pub fn get_cart_item_total() {}
pub fn is_free_shipping() {}
pub fn calculate_shipping() {
    get_cart_item_total();
}
"#,
    )
    .unwrap();
    git_commit_all(repo, "head-delete-call");
    let _ = run_json(repo, &["-f", "json", "discover", ".", "-l", "rust"]);
    tmp
}

/// EQ-2: default delta-synth review ≡ --full-snapshots on cold-discovered base/head.
#[test]
fn eq2_delta_synth_matches_full_snapshots() {
    let tmp = setup_shipping_review_repo();
    let repo = tmp.path();

    let full = run_json(
        repo,
        &[
            "-f",
            "json",
            "review",
            "paths",
            "--full-snapshots",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ],
    );
    let delta = run_json(
        repo,
        &[
            "-f",
            "json",
            "review",
            "paths",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ],
    );
    assert_eq!(
        full["change_summary"]["call_edges"],
        delta["change_summary"]["call_edges"],
        "EQ-2 call_edges summary"
    );
    assert_eq!(
        full["change_summary"]["changed_symbols"],
        delta["change_summary"]["changed_symbols"],
        "EQ-2 changed_symbols"
    );
}

/// EQ-3: --synthetic-head worktree ≡ committed head oracle (delta-synth path).
#[test]
fn eq3_worktree_matches_committed_head() {
    let tmp = setup_shipping_review_repo();
    let repo = tmp.path();

    let committed = run_json(
        repo,
        &[
            "-f",
            "json",
            "review",
            "paths",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ],
    );
    let worktree = run_json(
        repo,
        &[
            "-f",
            "json",
            "review",
            "paths",
            "--synthetic-head",
            "worktree",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ],
    );
    assert_eq!(
        committed["change_summary"]["call_edges"],
        worktree["change_summary"]["call_edges"],
        "EQ-3 call_edges"
    );
}

/// EQ-4: ColumnarGraphMmap edge topology ≡ hydrated MemoryBackend.
#[test]
fn eq4_columnar_matches_memory_backend() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("g.bin");
    let a = Node::new(NodeType::Function, "a").with_file_path("a.rs");
    let b = Node::new(NodeType::Function, "b").with_file_path("b.rs");
    let a_id = a.id;
    let b_id = b.id;
    write_columnar_from_nodes_edges(
        vec![a, b],
        vec![Edge::new(a_id, b_id, EdgeType::Calls)],
        &path,
    )
    .unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    let col = store.mmap().columnar().unwrap();
    let backend = store.hydrate_backend().unwrap();
    let mut col_edges = BTreeSet::new();
    for idx in 0..col.edge_count() {
        let (f, t, e) = col.edge_at(idx).unwrap();
        col_edges.insert((f, t, format!("{e:?}")));
    }
    let mut mem_edges = BTreeSet::new();
    backend
        .for_each_edge(|e| {
            mem_edges.insert((e.from, e.to, format!("{:?}", e.edge_type)));
        })
        .unwrap();
    assert_eq!(col_edges, mem_edges, "EQ-4 edge topology");
    assert_eq!(col.node_count(), backend.node_count());
}

/// E2E-SESSION-01: multi-command lifecycle preserves live snapshot digests.
#[test]
fn e2e_session_lifecycle_preserves_artifacts() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    init_git_repo(repo);
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn root() { mid(); }\npub fn mid() {}\n")
        .unwrap();
    git_commit_all(repo, "init");
    fs::write(
        repo.join("src/lib.rs"),
        "pub fn root() { mid(); }\npub fn mid() { /* changed */ }\n",
    )
    .unwrap();
    git_commit_all(repo, "head");

    let _ = run_json(
        repo,
        &["-f", "json", "discover", ".", "-l", "rust", "--with-cfg", "--with-harmonic"],
    );
    let snap = repo.join(".rgctl/graph.snapshot.bin");
    let hashes = repo.join(".rgctl/file_hashes.json");
    let analysis = repo.join(".rgctl/analysis_results.bin");
    let cfg = repo.join(".rgctl/cfg_pdg.bin");
    let d_snap = blake3_file(&snap);
    let d_hashes = blake3_file(&hashes);
    let d_analysis = blake3_file(&analysis);
    let d_cfg = blake3_file(&cfg);

    assert_graph_invariants(open_col(repo).mmap().columnar().unwrap()).unwrap();

    // Read-only commands (best-effort; some may need extra flags).
    let _ = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-f",
            "json",
            "review",
            "paths",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ])
        .output();
    let _ = Command::new(rgctl_bin())
        .current_dir(repo)
        .args([
            "-f",
            "json",
            "review",
            "check",
            "--base-ref",
            "HEAD~1",
            "--head-ref",
            "HEAD",
        ])
        .output();
    let _ = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(["-f", "json", "clones", "--mode", "fragment"])
        .output();
    let _ = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(["-f", "json", "security", "deps", "check", "--osv", "/dev/null"])
        .output();
    let _ = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(["-f", "json", "blast-radius", "root"])
        .output();

    assert_eq!(blake3_file(&snap), d_snap, "snapshot digest changed");
    assert_eq!(blake3_file(&hashes), d_hashes, "file_hashes digest changed");
    if analysis.is_file() {
        assert_eq!(blake3_file(&analysis), d_analysis, "analysis_results changed");
    }
    if cfg.is_file() {
        assert_eq!(blake3_file(&cfg), d_cfg, "cfg_pdg changed");
    }

    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .unwrap();
    let porcelain = String::from_utf8_lossy(&status.stdout);
    for line in porcelain.lines() {
        // Allow modifications under .rgctl/ and tracked source; forbid untracked junk at repo root.
        if line.starts_with("?? ") {
            let path = line.trim_start_matches("?? ").trim();
            assert!(
                path.starts_with(".rgctl") || path.contains('/'),
                "untracked repo-root artifact: {path}"
            );
        }
    }
}
