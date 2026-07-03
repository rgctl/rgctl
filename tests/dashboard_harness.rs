//! Dashboard bundle validation — shared by fixture + golden-repo tests.

#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Default golden repo for dashboard phase gates (override with env).
pub const DEFAULT_GOLDEN_REPO: &str = "/Users/sshaaf/git/java/gbuilder";

pub fn golden_repo_path() -> PathBuf {
    std::env::var("RBUILDER_DASHBOARD_GOLDEN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_GOLDEN_REPO))
}

pub fn rbuilder_bin() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_rbuilder") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/rbuilder")
}

pub fn run_discover(repo: &Path, languages: &str) -> Output {
    let bin = rbuilder_bin();
    assert!(
        bin.is_file(),
        "rbuilder binary not found at {} — run cargo build --release",
        bin.display()
    );
    Command::new(&bin)
        .args([
            "-r",
            repo.to_str().unwrap(),
            "discover",
            ".",
            "--languages",
            languages,
        ])
        .output()
        .expect("spawn rbuilder discover")
}

/// Assert Phase 0–2 bundle contract under `{repo}/.rbuilder/dashboard/`.
pub fn assert_dashboard_bundle(repo: &Path, min_nodes: u64) {
    assert_dashboard_bundle_with_meta(repo, min_nodes, 1);
}

pub fn assert_dashboard_bundle_with_meta(
    repo: &Path,
    min_nodes: u64,
    min_metanodes: u64,
) {
    let dash = repo.join(".rbuilder/dashboard");

    assert!(dash.join("index.html").is_file(), "missing index.html");
    assert!(dash.join("manifest.json").is_file(), "missing manifest.json");
    assert!(
        dash.join("graph_payload.bin").is_file(),
        "missing graph_payload.bin"
    );

    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dash.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["graph"]["payload_format"], "columnar_v2");
    assert_eq!(manifest["phases"]["0"], "complete");
    assert_eq!(manifest["phases"]["1"], "complete");
    assert_eq!(manifest["phases"]["2"], "complete");
    assert_eq!(manifest["phases"]["3"], "complete");

    let cfg_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("cfg_index.json")).unwrap()).unwrap();
    let slice_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("slice_index.json")).unwrap()).unwrap();
    let cfg_available = cfg_index["available"].as_bool().unwrap_or(false);
    let slice_available = slice_index["available"].as_bool().unwrap_or(false);

    assert_eq!(
        manifest["phases"]["4"],
        if cfg_available { "complete" } else { "pending" }
    );
    assert_eq!(
        manifest["phases"]["5"],
        if slice_available { "complete" } else { "pending" }
    );
    assert_eq!(manifest["phases"]["6"], "complete");

    assert!(
        dash.join("blast_index.json").is_file(),
        "missing blast_index.json (Phase 6)"
    );
    let blast_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("blast_index.json")).unwrap()).unwrap();
    assert_eq!(blast_index["schema_version"], 1);
    assert_eq!(blast_index["available"], true);

    let analysis = &manifest["analysis"];
    assert_eq!(analysis["blast_available"], true);
    assert_eq!(analysis["blast_index_path"], "blast_index.json");

    assert!(
        dash.join("slice_index.json").is_file(),
        "missing slice_index.json (Phase 5)"
    );
    assert_eq!(slice_index["schema_version"], 1);
    assert_eq!(slice_index["available"], cfg_available);
    assert_eq!(analysis["slice_available"], slice_available);
    assert_eq!(analysis["slice_index_path"], "slice_index.json");

    assert!(
        dash.join("cfg_index.json").is_file(),
        "missing cfg_index.json (Phase 4)"
    );
    assert_eq!(cfg_index["schema_version"], 1);
    if !cfg_available {
        assert_eq!(
            cfg_index["available"], false,
            "default discover should export empty cfg index when no archive"
        );
    }

    assert_eq!(analysis["cfg_available"], cfg_available);
    assert_eq!(analysis["cfg_index_path"], "cfg_index.json");

    let view = &manifest["view"];
    assert_eq!(view["metagraph_path"], "metagraph.json");
    assert!(
        view["metanode_count"].as_u64().unwrap_or(0) >= min_metanodes,
        "expected >= {min_metanodes} metanodes"
    );

    assert!(dash.join("metagraph.json").is_file(), "missing metagraph.json");
    let meta: Value =
        serde_json::from_slice(&std::fs::read(dash.join("metagraph.json")).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 2);
    assert!(
        meta["nodes"].as_array().map(|a| a.len()).unwrap_or(0) as u64 >= min_metanodes,
        "metagraph nodes below minimum"
    );
    let has_members = meta["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().any(|n| n["member_indices"].as_array().map(|a| !a.is_empty()).unwrap_or(false)))
        .unwrap_or(false);
    assert!(has_members, "metagraph metanodes must include member_indices for LOD");

    let node_count = manifest["graph"]["node_count"].as_u64().unwrap_or(0);
    let edge_count = manifest["graph"]["edge_count"].as_u64().unwrap_or(0);
    assert!(
        node_count >= min_nodes,
        "expected at least {min_nodes} nodes, got {node_count}"
    );
    assert!(edge_count > 0, "edge_count must be > 0");

    let payload = std::fs::read(dash.join("graph_payload.bin")).unwrap();
    assert_eq!(&payload[0..4], b"RBGR", "payload must be columnar v2 RBGR magic");

    let header_nodes = u64::from_le_bytes(payload[12..20].try_into().unwrap());
    let header_edges = u64::from_le_bytes(payload[20..28].try_into().unwrap());
    assert_eq!(
        header_nodes, node_count,
        "manifest node_count must match payload header"
    );
    assert_eq!(
        header_edges, edge_count,
        "manifest edge_count must match payload header"
    );

    let html = std::fs::read_to_string(dash.join("index.html")).unwrap();
    assert!(
        html.contains("rbuilder-manifest"),
        "index.html must have injected manifest bootstrap"
    );
    assert!(
        html.contains("./assets/") || html.contains("assets/"),
        "index.html must reference bundled assets (not CDN)"
    );
    assert!(
        !html.contains("cdn.jsdelivr.net") && !html.contains("d3js.org"),
        "legacy CDN dashboard must not be exported"
    );

    assert!(
        !repo.join(".rbuilder/dashboard.html").exists(),
        "legacy monolithic dashboard.html must not be written"
    );

    let assets = dash.join("assets");
    assert!(assets.is_dir(), "missing assets/ directory in bundle");
    let has_js = std::fs::read_dir(&assets)
        .ok()
        .map(|rd| {
            rd.flatten().any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x == "js" || x == "wasm")
            })
        })
        .unwrap_or(false);
    assert!(has_js, "assets/ must contain at least one .js or .wasm file");

    // No double-nested asset paths from embed extract bug.
    if let Ok(entries) = std::fs::read_dir(dash.join("assets")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            assert_ne!(
                name, "assets",
                "assets/assets/ double nesting detected in bundle"
            );
        }
    }
}

pub fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.as_ref().join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(from, to)?;
        } else {
            std::fs::copy(from, to)?;
        }
    }
    Ok(())
}
