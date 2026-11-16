//! Groovy CFG, dominator-tree, and PDG structural integration tests.

use rgctl::analysis::{CfgEdgeType, ProgramDependenceGraph, build_cfg_for_function};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("mkdir");
    for entry in std::fs::read_dir(src).expect("readdir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if name == ".rgctl" {
            continue;
        }
        let ty = entry.file_type().expect("filetype");
        let dst_path = dst.join(name);
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), dst_path).expect("copy");
        }
    }
}

fn rgctl_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl"))
}

fn inspect_json(bin: &Path, cwd: &Path, sym: &str, layer: &str) -> Value {
    let out = Command::new(bin)
        .args(["-f", "json", "inspect", sym, layer])
        .current_dir(cwd)
        .output()
        .expect("inspect");
    assert!(
        out.status.success(),
        "inspect {sym} {layer} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("valid inspect json")
}

#[test]
fn groovy_discover_inspect_cfg_dom_pdg_sandboxed() {
    let src_repo =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/groovy/langfeatures");
    let tmp = tempfile::tempdir().expect("tempdir");
    let sandbox = tmp.path().join("repo");
    copy_dir_recursive(&src_repo, &sandbox);

    let bin = rgctl_bin();
    let out = Command::new(&bin)
        .args(["discover", ".", "-l", "groovy", "--with-cfg"])
        .current_dir(&sandbox)
        .output()
        .expect("discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !src_repo.join(".rgctl").exists(),
        "tracked fixture must not be polluted with .rgctl"
    );

    let cfg = inspect_json(&bin, &sandbox, "findAll", "cfg");
    let dom = inspect_json(&bin, &sandbox, "findAll", "dom");
    let pdg = inspect_json(&bin, &sandbox, "findAll", "pdg");

    let cfg_nodes = cfg["nodes"].as_array().expect("cfg nodes");
    assert!(
        cfg_nodes.len() >= 3,
        "expected branching CFG for findAll, got {} blocks",
        cfg_nodes.len()
    );

    let cfg_blocks: Vec<i64> = {
        let mut v: Vec<i64> = cfg_nodes
            .iter()
            .filter_map(|n| n["block_index"].as_i64())
            .collect();
        v.sort();
        v
    };
    let dom_blocks: Vec<i64> = {
        let mut v: Vec<i64> = dom["nodes"]
            .as_array()
            .expect("dom nodes")
            .iter()
            .filter_map(|n| n["block_index"].as_i64())
            .collect();
        v.sort();
        v
    };
    assert_eq!(cfg_blocks, dom_blocks, "dom blocks must match cfg blocks");

    let mut cfg_lines = BTreeSet::new();
    for node in cfg_nodes {
        if let Some(stmts) = node["statements"].as_array() {
            for s in stmts {
                if let Some(l) = s["line"].as_i64() {
                    cfg_lines.insert(l);
                }
            }
        }
    }
    let pdg_nodes = pdg["nodes"].as_array().expect("pdg nodes");
    assert!(!pdg_nodes.is_empty(), "expected non-empty PDG for findAll");
    for n in pdg_nodes {
        if let Some(l) = n["line"].as_i64() {
            assert!(
                cfg_lines.contains(&l),
                "PDG line {l} must belong to CFG statement lines {cfg_lines:?}"
            );
        }
    }
}

#[test]
fn groovy_cfg_loop_and_try_catch_structures() {
    let code = r#"
class OrderProcessor {
    int retryBatch(int limit) {
        int total = 0
        while (total < limit) {
            try {
                total = total + 1
            } catch (Exception ex) {
                return -1
            }
        }
        return total
    }
}
"#;
    let cfg = build_cfg_for_function("groovy", code, "retryBatch").expect("groovy CFG");
    assert!(cfg.has_cycle(), "while loop must form a back-edge cycle");
    assert!(
        cfg.edges
            .iter()
            .any(|e| matches!(e.edge_type, CfgEdgeType::Exception)),
        "try/catch must emit Exception edge"
    );
    let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).expect("groovy PDG");
    assert!(
        !pdg.data_deps.is_empty(),
        "expected data dependencies on total"
    );
}
