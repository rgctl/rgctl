//! Fixture-scale gate for `rgctl vuln threat-model` (#229) on the dogfood
//! `rgctl-tests/ecommerce-{java,python,rust}` services.
//!
//! `tests/vuln_threat_model.rs` covers small inline archetypes; this file pins the
//! behaviour the design doc (§5) claims on the real multi-file fixtures:
//!
//! - epistemic invariants (every candidate is `graph_rule` + `needs_review`, cites
//!   resolvable evidence, states assumptions; data flows reference real EP/SNK ids);
//! - per-archetype floors (route counts, CLI `main`, SQL sinks, auth-suppressed Spoofing);
//! - correctness caps are never silent (`CALL_DEPTH_LIMITED`, `PDG_BUDGET_EXHAUSTED`);
//! - byte-identical JSON across runs.

use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create fixture copy dir");
    for entry in fs::read_dir(src).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let target = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            let name = entry.file_name();
            if matches!(
                name.to_string_lossy().as_ref(),
                ".rgctl" | "target" | "kantra_cache" | "node_modules"
            ) {
                continue;
            }
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

/// Copy `rgctl-tests/<name>` into a fresh temp dir and run a cold `discover .`.
fn discovered_fixture(name: &str) -> tempfile::TempDir {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("rgctl-tests")
        .join(name);
    assert!(src.is_dir(), "fixture {} missing", src.display());
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir(&src, tmp.path());
    let out = Command::new(env!("CARGO_BIN_EXE_rgctl"))
        .current_dir(tmp.path())
        .args(["discover", "."])
        .output()
        .expect("spawn rgctl discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    tmp
}

fn threat_model_raw(repo: &Path, extra: &[&str]) -> Vec<u8> {
    let out = Command::new(env!("CARGO_BIN_EXE_rgctl"))
        .current_dir(repo)
        .args(["-f", "json", "vuln", "threat-model", "--offline"])
        .args(extra)
        .output()
        .expect("spawn rgctl vuln threat-model");
    assert!(
        out.status.success(),
        "threat-model {extra:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn threat_model(repo: &Path, extra: &[&str]) -> Value {
    let raw = threat_model_raw(repo, extra);
    serde_json::from_slice(&raw).unwrap_or_else(|e| {
        panic!(
            "stdout must be pure JSON ({e}): {}",
            String::from_utf8_lossy(&raw)
        )
    })
}

fn arr<'a>(doc: &'a Value, field: &str) -> &'a Vec<Value> {
    doc[field]
        .as_array()
        .unwrap_or_else(|| panic!("`{field}` must be an array"))
}

fn limitation_codes(doc: &Value) -> HashSet<String> {
    arr(doc, "limitations_and_gaps")
        .iter()
        .map(|l| l["code"].as_str().expect("limitation code").to_string())
        .collect()
}

fn count_kind(doc: &Value, field: &str, key: &str, value: &str) -> usize {
    arr(doc, field)
        .iter()
        .filter(|r| r[key].as_str() == Some(value))
        .count()
}

fn assert_contract(doc: &Value) {
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["command"], "vuln threat-model");
    let index = doc["evidence_index"]
        .as_object()
        .expect("evidence_index object");

    let mut ids = HashSet::new();
    for (field, prefix) in [
        ("entry_points", "EP-"),
        ("assets", "AST-"),
        ("sinks", "SNK-"),
        ("trust_boundaries", "TB-"),
        ("data_flows", "DF-"),
        ("threat_candidates", "THR-"),
    ] {
        for rec in arr(doc, field) {
            let id = rec["id"].as_str().expect("record id");
            assert!(id.starts_with(prefix), "{field} id {id} lacks {prefix}");
            assert!(ids.insert(id.to_string()), "duplicate id {id}");
        }
    }

    let ep_ids: HashSet<&str> = arr(doc, "entry_points")
        .iter()
        .filter_map(|e| e["id"].as_str())
        .collect();
    let sink_ids: HashSet<&str> = arr(doc, "sinks")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    for df in arr(doc, "data_flows") {
        let ep = df["entry_point_id"].as_str().expect("entry_point_id");
        let sink = df["sink_id"].as_str().expect("sink_id");
        assert!(ep_ids.contains(ep), "data flow cites unknown entry {ep}");
        assert!(
            sink_ids.contains(sink),
            "data flow cites unknown sink {sink}"
        );
    }

    for tc in arr(doc, "threat_candidates") {
        let id = tc["id"].as_str().unwrap_or("<missing>");
        assert_eq!(tc["provenance"], "graph_rule", "{id} provenance");
        assert_eq!(tc["review_status"], "needs_review", "{id} review_status");
        let refs = tc["evidence_refs"].as_array().expect("evidence_refs");
        assert!(!refs.is_empty(), "{id} must cite evidence");
        for r in refs {
            let r = r.as_str().expect("ref id");
            assert!(index.contains_key(r), "{id} cites dangling ref {r}");
        }
        assert!(
            !tc["assumptions"]
                .as_array()
                .expect("assumptions")
                .is_empty(),
            "{id} must state assumptions"
        );
    }

    let s = &doc["summary"];
    assert_eq!(s["entry_points"], arr(doc, "entry_points").len());
    assert_eq!(s["sinks"], arr(doc, "sinks").len());
    assert_eq!(s["threat_candidates"], arr(doc, "threat_candidates").len());
    assert!(limitation_codes(doc).contains("OUT_OF_REPO_CONTROLS"));
}

#[test]
fn ecommerce_java_routes_spoofing_and_pdg_budget_is_reported() {
    let repo = discovered_fixture("ecommerce-java");
    let doc = threat_model(repo.path(), &[]);
    assert_contract(&doc);

    let routes = count_kind(&doc, "entry_points", "kind", "http_route");
    assert!(routes >= 30, "expected >=30 Spring routes, got {routes}");
    assert_eq!(
        count_kind(&doc, "entry_points", "kind", "cli_entrypoint"),
        1
    );
    let spoofing = count_kind(&doc, "threat_candidates", "stride", "spoofing");
    assert!(
        spoofing > 0 && spoofing <= routes,
        "spoofing candidates ({spoofing}) must be bounded by routes ({routes})"
    );

    // A tiny budget must be surfaced, never silently truncate taint coverage.
    let tiny = threat_model(repo.path(), &["--max-pdg-functions", "1"]);
    assert!(
        limitation_codes(&tiny).contains("PDG_BUDGET_EXHAUSTED"),
        "PDG budget shortfall must be reported"
    );
    // A budget covering every reachable function reports no shortfall.
    let full = threat_model(repo.path(), &["--max-pdg-functions", "100000"]);
    assert!(!limitation_codes(&full).contains("PDG_BUDGET_EXHAUSTED"));
    assert_contract(&full);

    // `--no-lazy-cfg` inspects nothing, so the whole reachable set is reported as skipped.
    let off = threat_model(repo.path(), &["--no-lazy-cfg"]);
    assert!(limitation_codes(&off).contains("PDG_BUDGET_EXHAUSTED"));
}

#[test]
fn ecommerce_python_auth_guarded_routes_suppress_spoofing() {
    let repo = discovered_fixture("ecommerce-python");
    let doc = threat_model(repo.path(), &[]);
    assert_contract(&doc);

    let routes = count_kind(&doc, "entry_points", "kind", "http_route");
    assert!(routes >= 25, "expected >=25 FastAPI routes, got {routes}");
    let guarded = arr(&doc, "entry_points")
        .iter()
        .filter(|e| {
            e["observed_auth_controls"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        })
        .count();
    assert!(
        guarded > 0,
        "Depends(get_current_user) routes must be detected"
    );
    let spoofing = count_kind(&doc, "threat_candidates", "stride", "spoofing");
    assert!(
        spoofing <= routes - guarded,
        "auth-guarded routes must not yield Spoofing ({spoofing} candidates, {guarded}/{routes} guarded)"
    );
}

#[test]
fn ecommerce_rust_cli_sinks_depth_limit_and_determinism() {
    let repo = discovered_fixture("ecommerce-rust");
    let raw_a = threat_model_raw(repo.path(), &[]);
    let raw_b = threat_model_raw(repo.path(), &[]);
    assert_eq!(raw_a, raw_b, "threat-model JSON must be byte-identical");
    let doc: Value = serde_json::from_slice(&raw_a).expect("json");
    assert_contract(&doc);

    assert!(
        arr(&doc, "entry_points")
            .iter()
            .any(|e| e["kind"] == "cli_entrypoint"
                && e["name"].as_str().is_some_and(|n| n.contains("main"))),
        "Rust `main` must be the CLI entry point"
    );
    assert_eq!(
        count_kind(&doc, "threat_candidates", "stride", "spoofing"),
        0,
        "a local CLI main must not produce HTTP Spoofing noise"
    );
    let sql = count_kind(&doc, "sinks", "sink_category", "sql_query");
    assert!(sql > 0, "sqlx query sinks must be discovered");
    assert!(!arr(&doc, "data_flows").is_empty(), "main must reach sinks");

    // Shrinking reachability to one hop must be reported, not silently drop deep sinks.
    let shallow = threat_model(repo.path(), &["--max-depth", "1"]);
    assert_contract(&shallow);
    assert!(
        limitation_codes(&shallow).contains("CALL_DEPTH_LIMITED"),
        "depth-limited traversal must be reported"
    );
    assert!(arr(&shallow, "data_flows").len() <= arr(&doc, "data_flows").len());
}
