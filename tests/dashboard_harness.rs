//! Dashboard bundle validation — shared by fixture + golden-repo tests.

#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Default golden repo for dashboard phase gates (override with env).
pub const DEFAULT_GOLDEN_REPO: &str = "/Users/sshaaf/git/java/gbuilder";

/// Default Go ecommerce test repo (override with env).
pub const DEFAULT_GO_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-go";

/// Default C# ecommerce test repo (override with env).
pub const DEFAULT_CSHARP_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-csharp";

/// Default C ecommerce test repo (override with env).
pub const DEFAULT_C_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-c";

/// Default C++ ecommerce test repo (override with env).
pub const DEFAULT_CPP_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-cpp";

/// Default Python ecommerce test repo (override with env).
pub const DEFAULT_PYTHON_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-python";

/// Default Rust ecommerce test repo (override with env).
pub const DEFAULT_RUST_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-rust";

/// Default JavaScript ecommerce test repo (override with env).
pub const DEFAULT_JAVASCRIPT_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-javascript";

/// Default TypeScript ecommerce test repo (override with env).
pub const DEFAULT_TYPESCRIPT_REPO: &str = "/Users/sshaaf/git/rust/rbuilder-tests/ecommerce-typescript";

pub fn golden_repo_path() -> PathBuf {
    std::env::var("RBUILDER_DASHBOARD_GOLDEN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_GOLDEN_REPO))
}

pub fn ecommerce_go_repo_path() -> PathBuf {
    std::env::var("RBUILDER_GO_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_GO_REPO))
}

pub fn ecommerce_csharp_repo_path() -> PathBuf {
    std::env::var("RBUILDER_CSHARP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CSHARP_REPO))
}

pub fn ecommerce_c_repo_path() -> PathBuf {
    std::env::var("RBUILDER_C_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_C_REPO))
}

pub fn ecommerce_cpp_repo_path() -> PathBuf {
    std::env::var("RBUILDER_CPP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CPP_REPO))
}

pub fn ecommerce_python_repo_path() -> PathBuf {
    std::env::var("RBUILDER_PYTHON_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_PYTHON_REPO))
}

pub fn ecommerce_rust_repo_path() -> PathBuf {
    std::env::var("RBUILDER_RUST_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RUST_REPO))
}

pub fn ecommerce_javascript_repo_path() -> PathBuf {
    std::env::var("RBUILDER_JAVASCRIPT_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_JAVASCRIPT_REPO))
}

pub fn ecommerce_typescript_repo_path() -> PathBuf {
    std::env::var("RBUILDER_TYPESCRIPT_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_TYPESCRIPT_REPO))
}

pub fn rbuilder_bin() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_rbuilder") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/rbuilder")
}

pub fn run_discover(repo: &Path, languages: &str) -> Output {
    run_discover_with_flags(repo, Some(languages), false)
}

/// Run `discover . --all` (CFG/PDG, security scan, full dashboard analysis exports).
pub fn run_discover_all(repo: &Path, languages: Option<&str>) -> Output {
    run_discover_with_flags(repo, languages, true)
}

fn run_discover_with_flags(repo: &Path, languages: Option<&str>, all: bool) -> Output {
    let bin = rbuilder_bin();
    assert!(
        bin.is_file(),
        "rbuilder binary not found at {} — run cargo build --release",
        bin.display()
    );
    let mut cmd = Command::new(&bin);
    cmd.args(["-r", repo.to_str().unwrap(), "discover", "."]);
    if all {
        cmd.arg("--all");
    }
    if let Some(langs) = languages {
        cmd.args(["--languages", langs]);
    }
    cmd.output().expect("spawn rbuilder discover")
}

/// Default metasfresh example checkout (override with env).
pub const DEFAULT_METASFRESH_REPO: &str = "example/metasfresh-4.9.8b";

pub fn metasfresh_repo_path() -> PathBuf {
    std::env::var("RBUILDER_METASFRESH_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_METASFRESH_REPO))
}

/// Assert Phase 0–2 bundle contract under `{repo}/.rbuilder/dashboard/`.
pub fn assert_dashboard_bundle(repo: &Path, min_nodes: u64) {
    assert_dashboard_bundle_with_meta(repo, min_nodes, 1);
}

pub fn assert_dashboard_bundle_with_meta(repo: &Path, min_nodes: u64, min_metanodes: u64) {
    let dash = repo.join(".rbuilder/dashboard");

    assert!(dash.join("index.html").is_file(), "missing index.html");
    assert!(
        dash.join("manifest.json").is_file(),
        "missing manifest.json"
    );
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
        if slice_available {
            "complete"
        } else {
            "pending"
        }
    );
    assert_eq!(manifest["phases"]["6"], "complete");

    let dataflow_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("dataflow_index.json")).unwrap()).unwrap();
    let dataflow_available = dataflow_index["available"].as_bool().unwrap_or(false);
    assert_eq!(
        manifest["phases"]["7"],
        if dataflow_available {
            "complete"
        } else {
            "pending"
        }
    );
    assert!(dash.join("dataflow_index.json").is_file());
    assert_eq!(dataflow_index["schema_version"], 1);
    if !cfg_available {
        assert_eq!(dataflow_available, false);
    }

    assert!(
        dash.join("blast_index.json").is_file(),
        "missing blast_index.json (Phase 6)"
    );
    let blast_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("blast_index.json")).unwrap()).unwrap();
    let blast_schema = blast_index["schema_version"].as_u64().unwrap_or(0);
    assert!(
        blast_schema == 1 || blast_schema == 2,
        "unexpected blast_index schema_version: {blast_schema}"
    );
    assert_eq!(blast_index["available"], true);

    let analysis = &manifest["analysis"];
    assert_eq!(analysis["blast_available"], true);
    assert_eq!(analysis["blast_index_path"], "blast_index.json");
    assert_eq!(analysis["dataflow_available"], dataflow_available);
    assert_eq!(analysis["dataflow_index_path"], "dataflow_index.json");

    let taint_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("taint_index.json")).unwrap()).unwrap();
    let taint_available = taint_index["available"].as_bool().unwrap_or(false);
    assert_eq!(
        manifest["phases"]["8"],
        if taint_available {
            "complete"
        } else {
            "pending"
        }
    );
    assert!(dash.join("taint_index.json").is_file());
    assert_eq!(taint_index["schema_version"], 1);
    assert_eq!(analysis["taint_available"], taint_available);
    assert_eq!(analysis["taint_index_path"], "taint_index.json");

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

    assert!(
        dash.join("metagraph.json").is_file(),
        "missing metagraph.json"
    );
    let meta: Value =
        serde_json::from_slice(&std::fs::read(dash.join("metagraph.json")).unwrap()).unwrap();
    assert_eq!(meta["schema_version"], 3);
    assert!(
        meta["nodes"].as_array().map(|a| a.len()).unwrap_or(0) as u64 >= min_metanodes,
        "metagraph nodes below minimum"
    );
    assert!(
        dash.join("communities.json").is_file(),
        "missing communities.json"
    );
    let communities: Value =
        serde_json::from_slice(&std::fs::read(dash.join("communities.json")).unwrap()).unwrap();
    assert_eq!(communities["schema_version"], 1);
    assert!(
        communities["communities"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "communities.json must list at least one community"
    );
    assert_eq!(view["communities_path"], "communities.json");

    assert!(
        dash.join("migration_graph.json").is_file(),
        "missing migration_graph.json"
    );
    let migration_graph: Value =
        serde_json::from_slice(&std::fs::read(dash.join("migration_graph.json")).unwrap()).unwrap();
    assert_eq!(migration_graph["schema_version"], 2);
    assert_eq!(migration_graph["mode"], "package_macro");
    assert!(
        migration_graph["communities"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "migration_graph.json must list communities"
    );
    assert!(
        dash.join("migration_plan.json").is_file(),
        "missing migration_plan.json"
    );
    assert_eq!(analysis["migration_available"], true);
    assert_eq!(analysis["migration_graph_path"], "migration_graph.json");
    assert_eq!(analysis["migration_plan_path"], "migration_plan.json");
    let migration_plan: Value =
        serde_json::from_slice(&std::fs::read(dash.join("migration_plan.json")).unwrap()).unwrap();
    assert_eq!(migration_plan["schema_version"], 2);
    assert_eq!(migration_plan["order_mode"], "scheduled");
    assert!(
        migration_plan["steps"][0]["schedule_step"].as_u64().is_some()
            && migration_plan["steps"][0]["priority_rank"].as_u64().is_some(),
        "migration plan steps must include schedule_step and priority_rank"
    );

    let has_community_id = meta["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().any(|n| n.get("community_id").is_some()))
        .unwrap_or(false);
    assert!(has_community_id, "metanodes should carry community_id");

    let has_members = meta["nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().any(|n| {
                n["member_indices"]
                    .as_array()
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    assert!(
        has_members,
        "metagraph metanodes must include member_indices for LOD"
    );

    let node_count = manifest["graph"]["node_count"].as_u64().unwrap_or(0);
    let edge_count = manifest["graph"]["edge_count"].as_u64().unwrap_or(0);
    assert!(
        node_count >= min_nodes,
        "expected at least {min_nodes} nodes, got {node_count}"
    );
    assert!(edge_count > 0, "edge_count must be > 0");

    let payload = std::fs::read(dash.join("graph_payload.bin")).unwrap();
    assert_eq!(
        &payload[0..4],
        b"RBGR",
        "payload must be columnar v2 RBGR magic"
    );

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
    assert!(
        has_js,
        "assets/ must contain at least one .js or .wasm file"
    );

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

/// Assert dashboard bundle after `discover --all` (CFG, slice, dataflow must be present).
pub fn assert_dashboard_bundle_all_analysis(repo: &Path, min_nodes: u64, min_metanodes: u64) {
    assert_dashboard_bundle_with_meta(repo, min_nodes, min_metanodes);

    let dash = repo.join(".rbuilder/dashboard");
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(dash.join("manifest.json")).unwrap()).unwrap();
    let analysis = &manifest["analysis"];

    assert_eq!(
        manifest["phases"]["4"], "complete",
        "CFG phase should be complete after --all"
    );
    assert_eq!(
        manifest["phases"]["5"], "complete",
        "slice phase should be complete after --all"
    );
    assert_eq!(
        manifest["phases"]["7"], "complete",
        "dataflow phase should be complete after --all"
    );
    assert_eq!(analysis["cfg_available"], true);
    assert_eq!(analysis["slice_available"], true);
    assert_eq!(analysis["dataflow_available"], true);

    assert!(
        dash.join("cfg_pdg.archive.bin").is_file(),
        "cfg_pdg.archive.bin expected in dashboard bundle after discover --all"
    );

    let cfg_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("cfg_index.json")).unwrap()).unwrap();
    assert_eq!(cfg_index["available"], true);
    assert!(
        cfg_index["function_count"].as_u64().unwrap_or(0) > 0,
        "cfg_index should list analyzed functions"
    );

    let slice_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("slice_index.json")).unwrap()).unwrap();
    assert_eq!(slice_index["available"], true);
    assert!(
        slice_index["function_count"].as_u64().unwrap_or(0) > 0,
        "slice_index should list PDG bundles"
    );

    let taint_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("taint_index.json")).unwrap()).unwrap();
    if taint_index["available"].as_bool() == Some(true) {
        assert_eq!(manifest["phases"]["8"], "complete");
        assert_eq!(analysis["taint_available"], true);
        assert!(
            taint_index["total_flows"].as_u64().unwrap_or(0) > 0,
            "taint_index available but reports zero flows"
        );
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
