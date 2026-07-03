//! Full-platform CLI subprocess I/O sanity audit (Layer 3).
//!
//! Single test [`test_all_cli_commands_json_schema_sanity`] spawns the real
//! `rbuilder` binary once per command and validates JSON contracts + exit codes.
//!
//! # Harness
//!
//! - **`Sandbox`** — copies [`tests/fixtures/tiny_polyglot_repo`] into a temp dir;
//!   passes `-r {repo}` and `-d {repo}/sandbox_graph.db` on every invocation.
//! - **Binary** — `CARGO_BIN_EXE_rbuilder` (built by `cargo test` for the active profile).
//! - **Helpers** — schema version, key presence/absence, nil-UUID scan, exit-code checks.
//!
//! # Coverage (see `docs/cli-io-sanity-audit.md` for the full matrix)
//!
//! | Command | What this file asserts |
//! |---------|------------------------|
//! | `discover` | Text + JSON v2; flags in [`test_discover_cli_flags`] |
//! | `blast-radius` | v2 sections, empty/default `handoffs`, `--depth` + `caller_depth_limit`, `--with-slices` populated handoffs, policy → exit 1 |
//! | `gql` | v1 rows; `--explain` sets `explain: true` |
//! | `metrics` | Standalone `--pagerank`, `--betweenness`, `--communities` key omission |
//! | `check` | Pass → exit 0; fail → exit 1 on `publishEvent` scale breach |
//! | `slice` | CFG + PDG topology; `--taint` flat schema |
//! | `inspect` | `cfg`, `pdg`, and `dom` layers with structured topology |
//!
//! Layer 1 (serializer fixtures): `cargo test --test cli_output`.
//! Layer 2 (narrow golden paths): `cargo test --test subprocess_golden_path`.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str;

const NIL_UUID: &str = "00000000-0000-0000-0000-000000000000";

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
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

struct Sandbox {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    db: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        copy_dir_all(&fixture_root(), dir.path()).expect("copy fixture");
        let db = dir.path().join("sandbox_graph.db");
        Self {
            repo: dir.path().to_path_buf(),
            db,
            _dir: dir,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(rbuilder_bin());
        cmd.arg("-r").arg(&self.repo).arg("-d").arg(&self.db);
        cmd.args(args);
        cmd.output().expect("spawn rbuilder")
    }

    fn parse_stdout_json(&self, output: &Output) -> Value {
        let stdout = str::from_utf8(&output.stdout).expect("stdout utf8");
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "invalid JSON on stdout: {e}\nstdout={stdout}\nstderr={}",
                str::from_utf8(&output.stderr).unwrap_or("")
            )
        })
    }
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed: status={} stderr={}",
        output.status,
        str::from_utf8(&output.stderr).unwrap_or("")
    );
}

fn assert_exit_code(output: &Output, code: i32, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{label}: expected exit {code}, stderr={}",
        str::from_utf8(&output.stderr).unwrap_or("")
    );
}

fn assert_schema_version(doc: &Value, expected: u64) {
    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(expected),
        "schema_version mismatch in {doc}"
    );
}

fn assert_keys_present(obj: &Value, keys: &[&str]) {
    let map = obj.as_object().expect("expected JSON object");
    for key in keys {
        assert!(map.contains_key(*key), "missing key '{key}' in {obj}");
    }
}

fn assert_keys_absent_in_str(json_str: &str, keys: &[&str]) {
    for key in keys {
        assert!(
            !json_str.contains(&format!("\"{key}\"")),
            "key '{key}' must be absent from payload"
        );
    }
}

fn assert_no_nil_uuids(json_str: &str) {
    assert!(
        !json_str.contains(NIL_UUID),
        "nil UUID must not appear in serialized output"
    );
}

fn assert_handoffs_empty_array(doc: &Value) {
    let handoffs = doc["gatekeeping"]["handoffs"]
        .as_array()
        .expect("gatekeeping.handoffs must be an array");
    assert!(handoffs.is_empty());
}

#[test]
fn test_all_cli_commands_json_schema_sanity() {
    let sandbox = Sandbox::new();
    let lib_rs = sandbox.repo.join("rust/src/lib.rs");
    assert!(lib_rs.exists(), "fixture rust source missing");

    // --- discover (text): stdout must not be JSON telemetry ---
    let discover_text = sandbox.run(&["discover", ".", "--languages", "java,rust"]);
    assert_success(&discover_text, "discover text mode");
    let text_stdout = str::from_utf8(&discover_text.stdout).unwrap_or("");
    assert!(
        !text_stdout.trim().starts_with('{'),
        "text-mode discover must not emit JSON on stdout"
    );

    // Re-ingest with JSON telemetry (sandbox db, isolated repo copy)
    let sandbox = Sandbox::new();
    let discover_json = sandbox.run(&[
        "-f",
        "json",
        "discover",
        ".",
        "--languages",
        "java,rust",
    ]);
    assert_success(&discover_json, "discover json mode");
    let discover_doc = sandbox.parse_stdout_json(&discover_json);
    assert_schema_version(&discover_doc, 2);
    assert_eq!(
        discover_doc.get("command").and_then(|v| v.as_str()),
        Some("discover")
    );
    assert_keys_present(
        discover_doc.get("metrics").expect("metrics"),
        &[
            "files_discovered",
            "files_indexed",
            "files_skipped",
            "nodes_generated",
            "edges_generated",
            "duration_ms",
        ],
    );

    // --- blast-radius v2 ---
    let blast = sandbox.run(&[
        "-f",
        "json",
        "blast-radius",
        "OrderService::process",
    ]);
    assert_success(&blast, "blast-radius");
    let blast_doc = sandbox.parse_stdout_json(&blast);
    let blast_str = str::from_utf8(&blast.stdout).unwrap();
    assert_schema_version(&blast_doc, 2);
    for key in ["target", "metrics", "topology", "gatekeeping"] {
        assert!(blast_doc.get(key).is_some(), "blast-radius missing '{key}'");
    }
    let target = blast_doc["target"].as_object().expect("target object");
    assert_keys_present(
        &Value::Object(target.clone()),
        &[
            "id",
            "symbol",
            "language",
            "canonical_fqn",
            "file_path",
        ],
    );
    assert_eq!(target["language"].as_str(), Some("java"));
    assert_eq!(
        target["canonical_fqn"].as_str(),
        Some("OrderService::process")
    );
    assert_handoffs_empty_array(&blast_doc);
    assert_no_nil_uuids(blast_str);

    // --- blast-radius --depth: hop cap reflected in JSON ---
    let blast_pe = sandbox.run(&["-f", "json", "blast-radius", "publishEvent"]);
    assert_success(&blast_pe, "blast-radius publishEvent");
    let pe_doc = sandbox.parse_stdout_json(&blast_pe);
    assert!(
        pe_doc["metrics"].get("caller_depth_limit").is_none(),
        "full closure must omit metrics.caller_depth_limit"
    );

    let blast_depth = sandbox.run(&[
        "-f",
        "json",
        "blast-radius",
        "publishEvent",
        "--depth",
        "1",
    ]);
    assert_success(&blast_depth, "blast-radius --depth 1");
    let depth_doc = sandbox.parse_stdout_json(&blast_depth);
    assert_eq!(
        depth_doc["metrics"]["caller_depth_limit"].as_u64(),
        Some(1)
    );
    let full_impact = pe_doc["metrics"]["impact_zone_size"]
        .as_u64()
        .unwrap_or(0);
    let depth_impact = depth_doc["metrics"]["impact_zone_size"]
        .as_u64()
        .unwrap_or(0);
    assert!(
        depth_impact <= full_impact,
        "depth-limited impact_zone_size ({depth_impact}) must be <= full ({full_impact})"
    );

    // --- blast-radius --with-slices: populated handoffs (unique callee `publishEvent`) ---
    let blast_slices = sandbox.run(&[
        "-f",
        "json",
        "blast-radius",
        "publishEvent",
        "--with-slices",
    ]);
    assert_success(&blast_slices, "blast-radius --with-slices");
    let slices_doc = sandbox.parse_stdout_json(&blast_slices);
    let handoffs = slices_doc["gatekeeping"]["handoffs"]
        .as_array()
        .expect("handoffs array");
    assert!(
        !handoffs.is_empty(),
        "expected populated handoffs for publishEvent"
    );
    assert_keys_present(&handoffs[0], &["callee", "param", "index"]);
    assert_eq!(handoffs[0]["callee"].as_str(), Some("publishEvent"));

    // --- gql v1 ---
    let gql = sandbox.run(&[
        "-f",
        "json",
        "gql",
        "MATCH (n:Function) RETURN n LIMIT 2",
    ]);
    assert_success(&gql, "gql");
    let gql_doc = sandbox.parse_stdout_json(&gql);
    assert_schema_version(&gql_doc, 1);
    assert_keys_present(&gql_doc, &["rows", "count", "explain"]);
    assert_eq!(gql_doc["explain"].as_bool(), Some(false));
    assert!(gql_doc["rows"].is_array());
    if let Some(row) = gql_doc["rows"].as_array().and_then(|rows| rows.first()) {
        if let Some(binding) = row.as_array().and_then(|r| r.first()) {
            assert_keys_present(binding, &["binding", "node", "type", "file"]);
        }
    }

    // --- gql --explain ---
    let gql_explain = sandbox.run(&[
        "-f",
        "json",
        "gql",
        "MATCH (n:Function) RETURN n LIMIT 1",
        "--explain",
    ]);
    assert_success(&gql_explain, "gql --explain");
    let explain_doc = sandbox.parse_stdout_json(&gql_explain);
    assert_eq!(explain_doc["explain"].as_bool(), Some(true));
    assert!(explain_doc["rows"].is_array());

    // --- metrics: pagerank-only omits other sections ---
    let metrics_pr = sandbox.run(&["-f", "json", "metrics", "--pagerank"]);
    assert_success(&metrics_pr, "metrics --pagerank");
    let metrics_str = str::from_utf8(&metrics_pr.stdout).unwrap();
    let metrics_doc = sandbox.parse_stdout_json(&metrics_pr);
    assert_schema_version(&metrics_doc, 1);
    assert!(metrics_doc.get("pagerank").is_some());
    assert_keys_absent_in_str(metrics_str, &["betweenness", "communities"]);
    let top = metrics_doc["pagerank"]["top"].as_array().expect("pagerank.top");
    assert!(top.len() <= 20);

    // --- metrics: betweenness-only omits other sections ---
    let metrics_bc = sandbox.run(&["-f", "json", "metrics", "--betweenness"]);
    assert_success(&metrics_bc, "metrics --betweenness");
    let metrics_bc_str = str::from_utf8(&metrics_bc.stdout).unwrap();
    let metrics_bc_doc = sandbox.parse_stdout_json(&metrics_bc);
    assert_schema_version(&metrics_bc_doc, 1);
    assert!(metrics_bc_doc.get("betweenness").and_then(|v| v.as_array()).is_some());
    assert_keys_absent_in_str(metrics_bc_str, &["pagerank", "communities"]);

    // --- metrics: communities-only omits other sections ---
    let metrics_cm = sandbox.run(&["-f", "json", "metrics", "--communities"]);
    assert_success(&metrics_cm, "metrics --communities");
    let metrics_cm_str = str::from_utf8(&metrics_cm.stdout).unwrap();
    let metrics_cm_doc = sandbox.parse_stdout_json(&metrics_cm);
    assert_schema_version(&metrics_cm_doc, 1);
    assert!(metrics_cm_doc.get("communities").unwrap().is_object());
    assert_keys_absent_in_str(metrics_cm_str, &["pagerank", "betweenness"]);

    // --- check: pass → exit 0 ---
    let permissive_policy = sandbox.repo.join("permissive_policy.json");
    fs::write(
        &permissive_policy,
        r#"{"max_impact_nodes": 1000000, "centrality_alert_threshold": 1e12}"#,
    )
    .expect("write policy");
    let check_pass = sandbox.run(&[
        "-f",
        "json",
        "check",
        "--policy-file",
        permissive_policy.to_str().unwrap(),
    ]);
    assert_exit_code(&check_pass, 0, "check pass");
    let check_doc = sandbox.parse_stdout_json(&check_pass);
    assert_schema_version(&check_doc, 1);
    assert_keys_present(&check_doc, &["policy", "violations", "passed"]);
    assert_eq!(check_doc["passed"].as_bool(), Some(true));
    assert!(check_doc["violations"].as_array().unwrap().is_empty());

    // --- check: fail → exit 1 (unique `publishEvent` has caller `checkout`) ---
    let strict_check_policy = sandbox.repo.join("strict_check_policy.json");
    fs::write(&strict_check_policy, r#"{"max_impact_nodes": 0}"#).expect("write policy");
    let check_fail = sandbox.run(&[
        "-f",
        "json",
        "check",
        "--policy-file",
        strict_check_policy.to_str().unwrap(),
    ]);
    assert_exit_code(&check_fail, 1, "check policy violation");
    let check_fail_doc = sandbox.parse_stdout_json(&check_fail);
    assert_eq!(check_fail_doc["passed"].as_bool(), Some(false));
    let violations = check_fail_doc["violations"]
        .as_array()
        .expect("violations array");
    assert!(!violations.is_empty());
    assert!(
        violations.iter().any(|v| {
            v.get("symbol")
                .and_then(|s| s.as_str())
                .is_some_and(|s| s == "publishEvent")
        }),
        "expected publishEvent scale violation, got {violations:?}"
    );

    // --- slice CFG topology ---
    let lib_rs = sandbox.repo.join("rust/src/lib.rs");
    let lib_rs_str = lib_rs.to_str().expect("lib path utf8");
    let slice_cfg = sandbox.run(&[
        "-f",
        "json",
        "slice",
        lib_rs_str,
        "--line",
        "6",
        "--variable",
        "order_id",
        "--function",
        "process_labeled",
        "--view",
        "cfg",
    ]);
    assert_success(&slice_cfg, "slice cfg");
    let slice_cfg_doc = sandbox.parse_stdout_json(&slice_cfg);
    assert_schema_version(&slice_cfg_doc, 1);
    assert_eq!(slice_cfg_doc["view"].as_str(), Some("cfg"));
    assert!(slice_cfg_doc["nodes"].is_array());
    assert!(slice_cfg_doc["edges"].is_array());
    assert!(slice_cfg_doc.get("blocks").is_none());

    // --- slice PDG topology ---
    let slice_pdg = sandbox.run(&[
        "-f",
        "json",
        "slice",
        lib_rs_str,
        "--line",
        "6",
        "--variable",
        "order_id",
        "--function",
        "process_labeled",
        "--view",
        "pdg",
    ]);
    assert_success(&slice_pdg, "slice pdg");
    let slice_pdg_doc = sandbox.parse_stdout_json(&slice_pdg);
    assert_schema_version(&slice_pdg_doc, 1);
    assert_eq!(slice_pdg_doc["view"].as_str(), Some("pdg"));
    assert!(slice_pdg_doc["nodes"].is_array());
    assert!(slice_pdg_doc["edges"].is_array());
    assert!(!slice_pdg_doc["nodes"].as_array().unwrap().is_empty());

    // --- slice taint flat schema (no topology edges) ---
    let slice_taint = sandbox.run(&[
        "-f",
        "json",
        "slice",
        lib_rs_str,
        "--line",
        "6",
        "--variable",
        "order_id",
        "--function",
        "process_labeled",
        "--taint",
    ]);
    assert_success(&slice_taint, "slice taint");
    let taint_doc = sandbox.parse_stdout_json(&slice_taint);
    assert_schema_version(&taint_doc, 1);
    assert_keys_present(&taint_doc, &["taint", "flows", "vulnerable"]);
    assert_eq!(taint_doc["taint"].as_bool(), Some(true));
    assert!(taint_doc.get("nodes").is_none());
    assert!(taint_doc.get("edges").is_none());

    // --- inspect dom: stable block_index integers ---
    let inspect_dom = sandbox.run(&["-f", "json", "inspect", "checkout", "dom"]);
    assert_success(&inspect_dom, "inspect dom");
    let dom_doc = sandbox.parse_stdout_json(&inspect_dom);
    assert_schema_version(&dom_doc, 1);
    assert_eq!(dom_doc["layer"].as_str(), Some("dom"));
    let dom_nodes = dom_doc["nodes"].as_array().expect("dom nodes");
    assert!(!dom_nodes.is_empty());
    for node in dom_nodes {
        assert!(node.get("block_index").and_then(|v| v.as_u64()).is_some());
        assert!(node.get("start_line").is_some());
    }
    if let Some(idom) = dom_doc["idom"].as_array() {
        for rel in idom {
            assert!(rel.get("block").and_then(|v| v.as_u64()).is_some());
            assert!(rel.get("immediate_dominator").and_then(|v| v.as_u64()).is_some());
        }
    }

    // --- inspect cfg: block_index topology ---
    let inspect_cfg = sandbox.run(&["-f", "json", "inspect", "checkout", "cfg"]);
    assert_success(&inspect_cfg, "inspect cfg");
    let cfg_doc = sandbox.parse_stdout_json(&inspect_cfg);
    assert_eq!(cfg_doc["layer"].as_str(), Some("cfg"));
    assert!(cfg_doc["nodes"].is_array());
    assert!(cfg_doc["edges"].is_array());
    for node in cfg_doc["nodes"].as_array().unwrap() {
        assert!(node.get("block_index").and_then(|v| v.as_u64()).is_some());
    }

    // --- inspect pdg: structured nodes/edges ---
    let inspect_pdg = sandbox.run(&["-f", "json", "inspect", "checkout", "pdg"]);
    assert_success(&inspect_pdg, "inspect pdg");
    let pdg_doc = sandbox.parse_stdout_json(&inspect_pdg);
    assert_eq!(pdg_doc["layer"].as_str(), Some("pdg"));
    assert!(pdg_doc["nodes"].is_array());
    assert!(pdg_doc["edges"].is_array());
    assert!(!pdg_doc["nodes"].as_array().unwrap().is_empty());

    // --- blast-radius policy violation → exit 1, JSON still valid ---
    let strict_policy = sandbox.repo.join("strict_policy.json");
    fs::write(&strict_policy, r#"{"max_impact_nodes": 0}"#).expect("write policy");
    let blast_violation = sandbox.run(&[
        "-f",
        "json",
        "blast-radius",
        "process",
        "--class",
        "OrderService",
        "--policy-file",
        strict_policy.to_str().unwrap(),
    ]);
    assert_exit_code(&blast_violation, 1, "blast-radius policy violation");
    let violation_doc = sandbox.parse_stdout_json(&blast_violation);
    assert_eq!(
        violation_doc["gatekeeping"]["policy_status"].as_str(),
        Some("VIOLATED")
    );
}

fn discover_json_metrics(sandbox: &Sandbox, extra: &[&str]) -> Value {
    let mut args = vec![
        "-f", "json", "discover", ".", "--languages", "java,rust",
    ];
    args.extend_from_slice(extra);
    let output = sandbox.run(&args);
    assert_success(&output, &format!("discover json {:?}", extra));
    sandbox.parse_stdout_json(&output)
}

#[test]
fn test_discover_cli_flags() {
    let full = Sandbox::new();
    let full_doc = discover_json_metrics(&full, &[]);
    let full_indexed = full_doc["metrics"]["files_indexed"]
        .as_u64()
        .expect("files_indexed");

    let excluded = Sandbox::new();
    let ex_doc = discover_json_metrics(&excluded, &["--exclude", "rust"]);
    assert!(
        ex_doc["metrics"]["files_indexed"].as_u64().unwrap() < full_indexed,
        "exclude rust should index fewer files than full polyglot discover"
    );

    let verbose = Sandbox::new();
    let verbose_out = verbose.run(&["discover", ".", "--languages", "java,rust", "-v"]);
    assert_success(&verbose_out, "discover --verbose text mode");
    let verbose_stdout = str::from_utf8(&verbose_out.stdout).unwrap_or("");
    assert!(
        !verbose_stdout.trim().starts_with('{'),
        "verbose text discover must not emit JSON telemetry on stdout"
    );

    for (label, flag) in [
        ("security", "--security"),
        ("cfg", "--cfg"),
        ("all", "--all"),
    ] {
        let sandbox = Sandbox::new();
        let doc = discover_json_metrics(&sandbox, &[flag]);
        assert_schema_version(&doc, 2);
        assert_eq!(doc["command"].as_str(), Some("discover"));
        assert!(
            doc["metrics"]["nodes_generated"].as_u64().unwrap() > 0,
            "discover {label} should produce nodes"
        );
    }
}
