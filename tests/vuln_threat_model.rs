//! End-to-end integration tests for `rgctl vuln threat-model` (alias `vuln surface`) (#229)
//! across three representative application archetypes:
//! 1. Web / FastAPI application (Python) — routes, auth vs unauth, lazy PDG taint, secrets, STRIDE, Mermaid
//! 2. Backend / Enterprise API service (Java + Maven OSV reachability) — REST controller, OSV CVE join
//! 3. CLI / Batch application (Rust) — `fn main()` entrypoint, file/shell sinks, indirect callback honesty

use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn rgctl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rgctl"))
}

fn stdout_json(out: std::process::Output) -> Value {
    assert!(
        out.status.success(),
        "stderr={}\nstdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("valid JSON on stdout")
}

fn run_discover_plain(repo: &Path) {
    let out = rgctl()
        .current_dir(repo)
        .args(["discover", "."])
        .output()
        .expect("spawn rgctl discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn assert_epistemic_invariants(report: &Value) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["command"], "vuln threat-model");
    let evidence_index = report["evidence_index"]
        .as_object()
        .expect("evidence_index object");
    let candidates = report["threat_candidates"]
        .as_array()
        .expect("threat_candidates array");

    for tc in candidates {
        let id = tc["id"].as_str().unwrap_or("<missing>");
        assert_eq!(
            tc["provenance"], "graph_rule",
            "candidate {id} must have provenance=graph_rule"
        );
        assert_eq!(
            tc["review_status"], "needs_review",
            "candidate {id} must have review_status=needs_review"
        );
        let refs = tc["evidence_refs"].as_array().expect("evidence_refs array");
        assert!(
            !refs.is_empty(),
            "candidate {id} must cite at least one evidence_ref"
        );
        for r in refs {
            let ref_id = r.as_str().expect("string ref id");
            assert!(
                evidence_index.contains_key(ref_id),
                "candidate {id} cites dangling evidence_ref {ref_id}"
            );
        }
        let assumptions = tc["assumptions"].as_array().expect("assumptions array");
        assert!(
            !assumptions.is_empty(),
            "candidate {id} must state explicit assumptions"
        );
    }
}

#[test]
fn archetype_1_python_web_app_routes_lazy_pdg_stride_and_determinism() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("app.py"),
        r#"import os
import subprocess
import logging
from fastapi import FastAPI, Request

app = FastAPI()
logger = logging.getLogger("audit")

STRIPE_KEY = os.getenv("STRIPE_SECRET_KEY")
DB_URL = os.getenv("DATABASE_URL")

def require_auth(token: str) -> bool:
    return token == "valid"

def sanitize_input(val: str) -> str:
    return val.strip()

@app.post("/exec")
def run_diag(request: Request):
    cmd = request.query_params.get("cmd")
    subprocess.run(cmd, shell=True)
    return {"ok": True}

@app.get("/orders")
def list_orders(token: str, q: str):
    require_auth(token)
    clean_q = sanitize_input(q)
    logger.info(clean_q)
    return {"orders": [], "key_loaded": bool(STRIPE_KEY), "db": bool(DB_URL)}
"#,
    )
    .unwrap();

    // Plain `discover .` WITHOUT `--with-cfg`: verifies zero discover bloat and lazy per-sink PDG!
    run_discover_plain(repo.path());

    let out1 = rgctl()
        .current_dir(repo.path())
        .args(["-f", "json", "vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    let out2 = rgctl()
        .current_dir(repo.path())
        .args(["-f", "json", "vuln", "surface", "--offline"])
        .output()
        .unwrap();
    assert_eq!(
        out1.stdout, out2.stdout,
        "consecutive runs (and `surface` alias) must be byte-identical"
    );

    let report = stdout_json(out1);
    assert_epistemic_invariants(&report);

    // Entry points: POST /exec (no auth) and GET /orders (has `require_auth` + `sanitize_input`).
    let eps = report["entry_points"].as_array().unwrap();
    assert!(eps.len() >= 2, "expected >=2 HTTP routes, got: {eps:?}");

    let exec_ep = eps
        .iter()
        .find(|e| e["name"].as_str().is_some_and(|n| n.contains("/exec")))
        .expect("POST /exec entry point");
    assert_eq!(exec_ep["kind"], "http_route");
    assert!(
        exec_ep["observed_auth_controls"]
            .as_array()
            .unwrap()
            .is_empty(),
        "POST /exec should have zero observed auth controls"
    );

    let orders_ep = eps
        .iter()
        .find(|e| e["name"].as_str().is_some_and(|n| n.contains("/orders")))
        .expect("GET /orders entry point");
    assert!(
        !orders_ep["observed_auth_controls"]
            .as_array()
            .unwrap()
            .is_empty(),
        "GET /orders should observe require_auth control: {orders_ep:?}"
    );

    // Sinks: `subprocess.run` classified as `shell_command` with lazy PDG taint proof.
    let sinks = report["sinks"].as_array().unwrap();
    let shell_sink = sinks
        .iter()
        .find(|s| s["sink_category"] == "shell_command")
        .expect("shell_command sink");
    assert_eq!(
        shell_sink["pdg_taint"]["status"], "proven_unsanitized_flow",
        "lazy on-demand CFG/PDG should prove unsanitized request -> subprocess.run flow: {shell_sink:?}"
    );
    assert_eq!(shell_sink["provenance"], "cpg_taint");

    // Assets: STRIPE_SECRET_KEY classified as secret, DATABASE_URL classified as connection config.
    let assets = report["assets"].as_array().unwrap();
    assert!(
        assets.iter().any(|a| {
            a["name"] == "STRIPE_SECRET_KEY" && a["kind"] == "secret_or_credential_config"
        }),
        "expected STRIPE_SECRET_KEY secret asset in: {assets:?}"
    );
    assert!(
        assets.iter().any(|a| {
            a["name"] == "DATABASE_URL" && a["kind"] == "connection_or_endpoint_config"
        }),
        "expected DATABASE_URL endpoint asset in: {assets:?}"
    );

    // STRIDE candidates: Spoofing on /exec (NOT on /orders), High-confidence Tampering & EoP on /exec, InfoDisclosure on STRIPE_SECRET_KEY.
    let candidates = report["threat_candidates"].as_array().unwrap();
    assert!(
        candidates
            .iter()
            .any(|c| c["stride"] == "spoofing" && c["title"].as_str().unwrap().contains("/exec")),
        "expected Spoofing candidate on /exec"
    );
    assert!(
        !candidates
            .iter()
            .any(|c| c["stride"] == "spoofing" && c["title"].as_str().unwrap().contains("/orders")),
        "authenticated /orders route must not trigger unauthenticated Spoofing candidate"
    );
    assert!(
        candidates.iter().any(|c| {
            c["stride"] == "tampering"
                && c["confidence"] == "high"
                && c["title"].as_str().unwrap().contains("/exec")
        }),
        "expected High-confidence Tampering candidate on unsanitized /exec -> subprocess.run"
    );
    assert!(
        candidates
            .iter()
            .any(|c| c["stride"] == "elevation_of_privilege"),
        "expected ElevationOfPrivilege candidate on /exec -> subprocess.run"
    );
    assert!(
        candidates.iter().any(|c| {
            c["stride"] == "information_disclosure"
                && c["title"].as_str().unwrap().contains("STRIPE_SECRET_KEY")
        }),
        "expected InformationDisclosure candidate on STRIPE_SECRET_KEY"
    );

    // Mermaid and Markdown formats succeed and are deterministic.
    let mmd = rgctl()
        .current_dir(repo.path())
        .args(["-f", "mermaid", "vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    assert!(mmd.status.success());
    let mmd_text = String::from_utf8(mmd.stdout).unwrap();
    assert!(mmd_text.starts_with("flowchart LR\n"));
    assert!(mmd_text.contains("-->|"));

    let md = rgctl()
        .current_dir(repo.path())
        .args(["vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    assert!(md.status.success());
    let md_text = String::from_utf8(md.stdout).unwrap();
    assert!(md_text.contains("# Threat Model Evidence & STRIDE Candidates"));
    assert!(md_text.contains("Epistemic Boundary"));
}

#[test]
fn archetype_2_java_enterprise_service_joins_osv_reachability_offline() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("pom.xml"),
        r#"<?xml version="1.0"?>
<project><modelVersion>4.0.0</modelVersion>
  <groupId>com.acme</groupId><artifactId>order-service</artifactId><version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>2.13.4</version>
    </dependency>
  </dependencies>
</project>"#,
    )
    .unwrap();

    let api_dir = repo.path().join("src/main/java/com/acme/api");
    let billing_dir = repo.path().join("src/main/java/com/acme/billing");
    std::fs::create_dir_all(&api_dir).unwrap();
    std::fs::create_dir_all(&billing_dir).unwrap();

    std::fs::write(
        api_dir.join("OrderController.java"),
        r#"package com.acme.api;

import com.acme.billing.PayloadDecoder;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class OrderController {
    private final PayloadDecoder decoder = new PayloadDecoder();

    @PostMapping("/api/orders")
    public String submitOrder(String rawBody) throws Exception {
        return decoder.decodeOrder(rawBody);
    }
}
"#,
    )
    .unwrap();

    std::fs::write(
        billing_dir.join("PayloadDecoder.java"),
        r#"package com.acme.billing;

import com.fasterxml.jackson.databind.ObjectMapper;

public class PayloadDecoder {
    public String decodeOrder(String jsonPayload) throws Exception {
        ObjectMapper mapper = new ObjectMapper();
        Object parsed = mapper.readValue(jsonPayload, Object.class);
        return parsed.toString();
    }
}
"#,
    )
    .unwrap();

    run_discover_plain(repo.path());

    let osv_fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/osv/jackson-CVE-2022-42004.json"
    );

    let out = rgctl()
        .current_dir(repo.path())
        .args([
            "-f",
            "json",
            "vuln",
            "threat-model",
            "--osv",
            osv_fixture,
            "--osv-api-url",
            "http://127.0.0.1:9", // unroutable port proves zero network calls
        ])
        .output()
        .unwrap();
    let report = stdout_json(out);
    assert_epistemic_invariants(&report);

    let eps = report["entry_points"].as_array().unwrap();
    assert!(
        !eps.is_empty(),
        "expected Spring @PostMapping entry point in Java service"
    );

    let rvs = report["reachable_vulnerabilities"].as_array().unwrap();
    assert_eq!(
        rvs.len(),
        1,
        "expected jackson CVE-2022-42004 in reachable_vulnerabilities: {rvs:?}"
    );
    assert_eq!(rvs[0]["provenance"], "osv_reachability");
    assert!(rvs[0]["purl"].as_str().is_some_and(|p| {
        p.starts_with("pkg:maven/com.fasterxml.jackson.core/jackson-databind@2.13.4")
    }));
    assert!(
        !rvs[0]["reaching_entry_points"]
            .as_array()
            .unwrap()
            .is_empty(),
        "expected OrderController.submitOrder entry point linked to reachable jackson call site: {rvs:?}"
    );

    let candidates = report["threat_candidates"].as_array().unwrap();
    assert!(
        candidates.iter().any(|c| {
            c["evidence_refs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r == "OSV-001")
        }),
        "expected STRIDE candidate linked to OSV-001"
    );
}

#[test]
fn archetype_3_rust_cli_batch_app_main_sinks_and_indirect_call_honesty() {
    let repo = tempfile::tempdir().unwrap();
    let src_dir = repo.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        repo.path().join("Cargo.toml"),
        r#"[package]
name = "deploy-tool"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::write(
        src_dir.join("main.rs"),
        r#"use std::env;
use std::fs;
use std::process::Command;

fn run_hook(hook: fn(&str), target: &str) {
    hook(target);
}

fn default_hook(_target: &str) {}

fn deploy_bundle(bundle_path: &str) {
    let _token = env::var("DEPLOY_API_TOKEN").unwrap_or_default();
    let _ = fs::write("/tmp/deploy.manifest", bundle_path);
    let _ = Command::new("sh").arg("-c").arg(bundle_path).status();
}

fn main() {
    let arg = env::args().nth(1).unwrap_or_default();
    run_hook(default_hook, &arg);
    deploy_bundle(&arg);
}
"#,
    )
    .unwrap();

    run_discover_plain(repo.path());

    let out = rgctl()
        .current_dir(repo.path())
        .args(["-f", "json", "vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    let report = stdout_json(out);
    assert_epistemic_invariants(&report);

    let eps = report["entry_points"].as_array().unwrap();
    assert_eq!(eps.len(), 1, "expected CLI main entrypoint: {eps:?}");
    assert_eq!(eps[0]["kind"], "cli_entrypoint");

    // CLI entrypoints should NOT emit HTTP/RPC Spoofing candidates.
    let candidates = report["threat_candidates"].as_array().unwrap();
    assert!(
        !candidates.iter().any(|c| c["stride"] == "spoofing"),
        "local CLI main should not emit external HTTP/RPC Spoofing candidate"
    );

    // Sinks & flows from `main` -> `deploy_bundle` -> `fs::write` & `Command::new`.
    let sinks = report["sinks"].as_array().unwrap();
    assert!(
        sinks.iter().any(|s| s["sink_category"] == "shell_command"),
        "expected Command::new shell_command sink in: {sinks:?}"
    );
    assert!(
        sinks.iter().any(|s| s["sink_category"] == "file_write"),
        "expected fs::write file_write sink in: {sinks:?}"
    );
    assert!(
        !report["data_flows"].as_array().unwrap().is_empty(),
        "expected call-graph data_flows from CLI main to deploy_bundle sinks"
    );

    // Indirect callback `hook(target)` in `run_hook` must be surfaced honestly in summary + limitations!
    assert!(
        report["summary"]["unresolved_or_indirect_calls"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "expected unresolved_or_indirect_calls >= 1 from callback parameter `hook(target)`"
    );
    let lims = report["limitations_and_gaps"].as_array().unwrap();
    assert!(
        lims.iter()
            .any(|l| l["code"] == "DYNAMIC_OR_INDIRECT_CALLS"),
        "expected DYNAMIC_OR_INDIRECT_CALLS limitation note in: {lims:?}"
    );
}

#[test]
fn missing_snapshot_emits_structured_graph_not_found_json_error() {
    let empty_repo = tempfile::tempdir().unwrap();
    let out = rgctl()
        .current_dir(empty_repo.path())
        .args(["-f", "json", "vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err_json: Value = serde_json::from_slice(&out.stdout).expect("structured JSON error");
    assert_eq!(err_json["ok"], false);
    assert_eq!(err_json["error_code"], "GRAPH_NOT_FOUND");
    assert_eq!(err_json["recovery_command"], "rgctl discover");
}

/// Regression: data-flow IDs used to be renumbered in place after a HashMap-ordered build, so
/// swapped IDs overwrote each other's `evidence_index` records (dangling `evidence_refs`, wrong
/// summaries) and output differed between runs. Also pins whole-token control matching:
/// `get_author` / `invalidate_cache` must not count as auth / validation controls.
#[test]
fn multi_route_flows_keep_evidence_index_consistent_and_controls_token_exact() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("app.py"),
        r#"import subprocess
import os
from fastapi import FastAPI

app = FastAPI()

def helper_a(x):
    subprocess.run(x, shell=True)

def helper_b(x):
    os.system(x)

def helper_c(x):
    eval(x)

def get_author(x):
    return x

def invalidate_cache(x):
    return x

@app.post("/a")
def route_a(cmd: str):
    get_author(cmd)
    helper_a(cmd)
    helper_c(cmd)

@app.post("/b")
def route_b(cmd: str):
    invalidate_cache(cmd)
    helper_b(cmd)
    helper_a(cmd)
    helper_c(cmd)

@app.get("/c")
def route_c(cmd: str):
    helper_c(cmd)
    helper_b(cmd)
"#,
    )
    .unwrap();
    run_discover_plain(repo.path());

    let run = || {
        rgctl()
            .current_dir(repo.path())
            .args(["-f", "json", "vuln", "threat-model", "--offline"])
            .output()
            .unwrap()
    };
    let first = run();
    for _ in 0..3 {
        assert_eq!(
            first.stdout,
            run().stdout,
            "threat-model JSON must be byte-identical"
        );
    }
    let report = stdout_json(first);
    assert_epistemic_invariants(&report);

    let index = report["evidence_index"].as_object().unwrap();
    let eps = report["entry_points"].as_array().unwrap();
    let sinks = report["sinks"].as_array().unwrap();
    let flows = report["data_flows"].as_array().unwrap();
    assert!(flows.len() >= 3, "expected several flows, got {flows:?}");
    for df in flows {
        let id = df["id"].as_str().unwrap();
        let rec = index
            .get(id)
            .unwrap_or_else(|| panic!("{id} missing from evidence_index"));
        assert_eq!(rec["id"], id);
        assert_eq!(rec["kind"], "data_flow");
        let ep = eps
            .iter()
            .find(|e| e["id"] == df["entry_point_id"])
            .unwrap();
        let sink = sinks.iter().find(|s| s["id"] == df["sink_id"]).unwrap();
        assert_eq!(
            rec["symbol_or_target"],
            format!(
                "{} -> {}",
                ep["name"].as_str().unwrap(),
                sink["callee"].as_str().unwrap()
            ),
            "{id} evidence record must describe its own flow"
        );
    }

    for ep in eps {
        assert!(
            ep["observed_auth_controls"].as_array().unwrap().is_empty(),
            "`get_author` must not be treated as auth: {ep}"
        );
        assert!(
            ep["observed_validation_controls"]
                .as_array()
                .unwrap()
                .is_empty(),
            "`invalidate_cache` must not be treated as validation: {ep}"
        );
    }
    let spoofing = report["threat_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["stride"] == "spoofing")
        .count();
    assert_eq!(
        spoofing,
        eps.len(),
        "every unauthenticated route yields Spoofing"
    );
}

/// `@Scheduled` jobs are internally triggered: classified as `scheduled_job`, never as
/// unauthenticated HTTP ingress (no Spoofing candidate).
#[test]
fn scheduled_jobs_are_not_http_ingress() {
    let repo = tempfile::tempdir().unwrap();
    let src = repo.path().join("src/main/java/com/acme");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("Jobs.java"),
        r#"package com.acme;

import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Component
public class Jobs {
    @Scheduled(cron = "0 0 * * * *")
    public void nightlyCleanup() {
        purge();
    }

    private void purge() {}
}
"#,
    )
    .unwrap();
    run_discover_plain(repo.path());
    let out = rgctl()
        .current_dir(repo.path())
        .args(["-f", "json", "vuln", "threat-model", "--offline"])
        .output()
        .unwrap();
    let report = stdout_json(out);
    assert_epistemic_invariants(&report);
    let eps = report["entry_points"].as_array().unwrap();
    let job = eps
        .iter()
        .find(|e| {
            e["handler_symbol"]
                .as_str()
                .is_some_and(|h| h.contains("nightlyCleanup"))
        })
        .unwrap_or_else(|| panic!("scheduled job entry point missing: {eps:?}"));
    assert_eq!(job["kind"], "scheduled_job");
    assert!(
        !report["threat_candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["stride"] == "spoofing"),
        "scheduled jobs must not produce Spoofing candidates"
    );
}
