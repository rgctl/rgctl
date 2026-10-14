//! Dashboard bundle validation — shared by fixture + golden-repo tests.

#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};

fn env_rg(suffix: &str) -> Result<String, std::env::VarError> {
    std::env::var(format!("RGCTL_{suffix}"))
        .or_else(|_| std::env::var(format!("RGCTL_{suffix}")))
}

use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Default golden repo for dashboard phase gates (override with env).
/// Golden repo, **relative to the checkout**.
///
/// This was `/Users/sshaaf/git/java/gbuilder` — an absolute path on one developer's machine, and the only
/// corpus helper here that was not resolved in-tree (`default_go_repo` and friends use `in_tree_ecommerce`).
/// The effect on any other checkout was that the golden gate hit its "repo not found" branch and **passed
/// without testing anything**, while reporting success.
///
/// `example/` is where `scripts/fetch-profile-repos.sh` puts the corpora the Gate B table in AGENTS.md
/// documents, so the default now points there and the env override still wins.
pub const DEFAULT_GOLDEN_REPO: &str = "example/metasfresh-4.9.8b";

fn in_tree_ecommerce(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("rgctl-tests")
        .join(name)
}

/// Default Go ecommerce test repo (override with env).
pub fn default_go_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-go")
}

/// Default C# ecommerce test repo (override with env).
pub fn default_csharp_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-csharp")
}

/// Default C ecommerce test repo (override with env).
pub fn default_c_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-c")
}

/// Default C++ ecommerce test repo (override with env).
pub fn default_cpp_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-cpp")
}

/// Default Python ecommerce test repo (override with env).
pub fn default_python_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-python")
}

/// Default Rust ecommerce test repo (override with env).
pub fn default_rust_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-rust")
}

/// Default JavaScript ecommerce test repo (override with env).
pub fn default_javascript_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-javascript")
}

/// Default TypeScript ecommerce test repo (override with env).
pub fn default_typescript_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-typescript")
}

/// Default PHP ecommerce test repo (override with env).
pub fn default_php_repo() -> PathBuf {
    in_tree_ecommerce("ecommerce-php")
}

/// Default Ruby ecommerce test repo (override with env).
pub fn default_ruby_repo() -> PathBuf {
    env_rg("ECOMMERCE_RUBY_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| in_tree_ecommerce("ecommerce-ruby"))
}

/// Default Puppet ecommerce test repo (override with env).
pub fn default_puppet_repo() -> PathBuf {
    env_rg("ECOMMERCE_PUPPET_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| in_tree_ecommerce("ecommerce-puppet"))
}

/// Default Kotlin ecommerce test repo (override with env).
pub fn default_kotlin_repo() -> PathBuf {
    env_rg("ECOMMERCE_KOTLIN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| in_tree_ecommerce("ecommerce-kotlin"))
}

/// Default Groovy ecommerce test repo (override with env).
pub fn default_groovy_repo() -> PathBuf {
    env_rg("ECOMMERCE_GROOVY_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| in_tree_ecommerce("ecommerce-groovy"))
}

pub fn golden_repo_path() -> PathBuf {
    env_rg("DASHBOARD_GOLDEN_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_GOLDEN_REPO))
}

pub fn ecommerce_go_repo_path() -> PathBuf {
    env_rg("GO_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_go_repo())
}

pub fn ecommerce_csharp_repo_path() -> PathBuf {
    env_rg("CSHARP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_csharp_repo())
}

pub fn ecommerce_c_repo_path() -> PathBuf {
    env_rg("C_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_c_repo())
}

pub fn ecommerce_cpp_repo_path() -> PathBuf {
    env_rg("CPP_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_cpp_repo())
}

pub fn ecommerce_python_repo_path() -> PathBuf {
    env_rg("PYTHON_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_python_repo())
}

pub fn ecommerce_rust_repo_path() -> PathBuf {
    env_rg("RUST_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_rust_repo())
}

pub fn ecommerce_javascript_repo_path() -> PathBuf {
    env_rg("JAVASCRIPT_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_javascript_repo())
}

pub fn ecommerce_typescript_repo_path() -> PathBuf {
    env_rg("TYPESCRIPT_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_typescript_repo())
}

pub fn rgctl_bin() -> PathBuf {
    if let Ok(p) =
        std::env::var("CARGO_BIN_EXE_rgctl")
    {
        return PathBuf::from(p);
    }
    panic!(
        "CARGO_BIN_EXE_rgctl is not set for dashboard tests; run via `cargo test` so the test harness uses the current binary"
    );
}

pub fn run_discover(repo: &Path, languages: &str) -> Output {
    run_discover_with_flags(repo, Some(languages), false)
}

/// Run discover with CFG + security + taint + dashboard (former `--all` depth + dashboard).
pub fn run_discover_all(repo: &Path, languages: Option<&str>) -> Output {
    run_discover_with_flags(repo, languages, true)
}

/// Run deep discover and return wall-clock duration alongside process output.
pub fn run_discover_all_timed(repo: &Path, languages: Option<&str>) -> (Output, Duration) {
    let start = Instant::now();
    let output = run_discover_all(repo, languages);
    (output, start.elapsed())
}

fn run_discover_with_flags(repo: &Path, languages: Option<&str>, deep: bool) -> Output {
    let bin = rgctl_bin();
    assert!(
        bin.is_file(),
        "rgctl binary not found at {} — run cargo build --release",
        bin.display()
    );
    let rgctl_dir = repo.join(".rgctl");
    if rgctl_dir.exists() {
        std::fs::remove_dir_all(&rgctl_dir).expect("remove stale .rgctl before discover");
    }
    let mut cmd = Command::new(&bin);
    cmd.current_dir(repo);
    // Dashboard tests always opt in (#31 — bare discover skips dashboard).
    cmd.args([
        "-r",
        repo.to_str().unwrap(),
        "discover",
        ".",
        "--with-dashboard",
    ]);
    if deep {
        // Former `--all` ≡ security + cfg + taint (#34).
        cmd.args(["--with-cfg", "--with-security", "--with-taint"]);
    }
    if let Some(langs) = languages {
        cmd.args(["--languages", langs]);
    }
    cmd.output().expect("spawn rgctl discover")
}

/// Default metasfresh example checkout (override with env).
pub const DEFAULT_METASFRESH_REPO: &str = "example/metasfresh-4.9.8b";

pub fn metasfresh_repo_path() -> PathBuf {
    env_rg("METASFRESH_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_METASFRESH_REPO))
}

/// Assert Phase 0–2 bundle contract under `{repo}/.rgctl/dashboard/`.
pub fn assert_dashboard_bundle(repo: &Path, min_nodes: u64) {
    assert_dashboard_bundle_with_meta(repo, min_nodes, 1);
}

pub fn assert_dashboard_bundle_with_meta(repo: &Path, min_nodes: u64, min_metanodes: u64) {
    let dash = repo.join(".rgctl/dashboard");

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

    assert!(
        dash.join("mutations_index.json").is_file(),
        "missing mutations_index.json (CPG field writes)"
    );
    let mutations_index: Value =
        serde_json::from_slice(&std::fs::read(dash.join("mutations_index.json")).unwrap()).unwrap();
    assert_eq!(mutations_index["schema_version"], 1);
    let mutations_available = mutations_index["available"].as_bool().unwrap_or(false);
    assert_eq!(analysis["mutations_available"], mutations_available);
    assert_eq!(analysis["mutations_index_path"], "mutations_index.json");
    if cfg_available {
        // discover --with-cfg builds field_write.index.bin → mutations export
        assert_eq!(
            mutations_available, true,
            "expected mutations_index when CFG/PDG archive exists"
        );
    } else {
        assert_eq!(mutations_available, false);
    }

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
    let sv = slice_index["schema_version"].as_u64().unwrap_or(0);
    assert!(sv >= 1, "slice_index schema_version must be >= 1, got {sv}");
    assert_eq!(slice_index["available"], cfg_available);
    assert_eq!(analysis["slice_available"], slice_available);
    assert_eq!(analysis["slice_index_path"], "slice_index.json");

    assert!(
        dash.join("cfg_index.json").is_file(),
        "missing cfg_index.json (Phase 4)"
    );
    let cfg_sv = cfg_index["schema_version"].as_u64().unwrap_or(0);
    assert!(
        cfg_sv >= 1,
        "cfg_index schema_version must be >= 1, got {cfg_sv}"
    );
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
        communities["communities"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
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
        migration_plan["steps"][0]["schedule_step"]
            .as_u64()
            .is_some()
            && migration_plan["steps"][0]["priority_rank"]
                .as_u64()
                .is_some(),
        "migration plan steps must include schedule_step and priority_rank"
    );

    let has_community_id = meta["nodes"]
        .as_array()
        .map(|nodes| nodes.iter().any(|n| n.get("community_id").is_some()))
        .unwrap_or(false);
    assert!(has_community_id, "metanodes should carry community_id");

    let community_only = meta["community_only"].as_bool().unwrap_or(false);
    if !community_only {
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
            "metagraph metanodes must include member_indices for LOD when community_only is false"
        );
    }

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
        html.contains("rgctl-manifest"),
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
        !repo.join(".rgctl/dashboard.html").exists(),
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

/// Assert dashboard bundle after `discover --with-cfg --with-security --with-taint` (CFG, slice, dataflow must be present).
pub fn assert_dashboard_bundle_all_analysis(repo: &Path, min_nodes: u64, min_metanodes: u64) {
    assert_dashboard_bundle_with_meta(repo, min_nodes, min_metanodes);

    let dash = repo.join(".rgctl/dashboard");
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
        "cfg_pdg.archive.bin expected in dashboard bundle after discover --with-cfg --with-security --with-taint"
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
