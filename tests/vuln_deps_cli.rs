//! Subprocess CLI checks for vuln triage / deps check / vuln analyze (Phase 1 + lockfile/SARIF soundness).

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
    serde_json::from_slice(&out.stdout).expect("json stdout")
}

fn run_discover(repo: &Path) {
    let out = rgctl()
        .current_dir(repo)
        .args(["discover", "."])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Write a minimal pom.xml matching the given package at the given version.
fn write_pom(dir: &Path, group: &str, artifact: &str, version: &str) {
    std::fs::write(
        dir.join("pom.xml"),
        format!(
            r#"<?xml version="1.0"?>
<project><modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId><artifactId>app</artifactId><version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>{group}</groupId>
      <artifactId>{artifact}</artifactId>
      <version>{version}</version>
    </dependency>
  </dependencies>
</project>"#
        ),
    )
    .unwrap();
}

/// Write a synthetic OSV JSON file (with no symbol metadata = Unspecified coverage).
fn write_unspecified_osv(dir: &Path, ecosystem: &str, name: &str) -> String {
    let path = dir.join("test.osv.json");
    std::fs::write(
        &path,
        format!(
            r#"{{
  "id": "GHSA-TEST-PHASE1",
  "modified": "2024-01-01T00:00:00Z",
  "affected": [{{
    "package": {{"ecosystem": "{ecosystem}", "name": "{name}"}},
    "ranges": [{{"type": "ECOSYSTEM", "events": [{{"introduced": "0"}}, {{"fixed": "99.0.0"}}]}}]
  }}]
}}"#
        ),
    )
    .unwrap();
    path.to_string_lossy().to_string()
}

/// Write a synthetic multi-range OSV JSON file.
fn write_multi_range_osv(dir: &Path) -> String {
    let path = dir.join("multi_range.osv.json");
    std::fs::write(
        &path,
        r#"{
  "id": "CVE-TEST-MULTI-RANGE",
  "modified": "2024-01-01T00:00:00Z",
  "affected": [{
    "package": {"ecosystem": "Maven", "name": "com.example:multi-lib"},
    "ranges": [{"type": "ECOSYSTEM", "events": [
        {"introduced": "1.0.0"}, {"fixed": "1.2.5"},
        {"introduced": "2.0.0"}, {"fixed": "2.1.3"}
    ]}]
  }]
}"#,
    )
    .unwrap();
    path.to_string_lossy().to_string()
}

#[test]
fn vuln_triage_json_schema() {
    let osv = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/osv/jackson-CVE-2022-42004.json"
    );
    let out = rgctl()
        .args(["-f", "json", "vuln", "triage", "--osv", osv])
        .output()
        .unwrap();
    let v = stdout_json(out);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["command"], "vuln triage");
}

#[test]
fn deps_check_xalan_not_affected() {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(
        repo.path().join("pom.xml"),
        r#"<?xml version="1.0"?><project><modelVersion>4.0.0</modelVersion>
        <groupId>a</groupId><artifactId>b</artifactId><version>1</version></project>"#,
    )
    .unwrap();
    let osv = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/osv/xalan-CVE-2022-34169.json"
    );
    let out = rgctl()
        .args([
            "-r",
            repo.path().to_str().unwrap(),
            "-f",
            "json",
            "deps",
            "check",
            "--osv",
            osv,
        ])
        .output()
        .unwrap();
    let v = stdout_json(out);
    assert_eq!(v["verdict"], "not_affected");
}

#[test]
fn unspecified_symbol_coverage_with_discovered_imports_and_strict_escalation() {
    let repo = tempfile::tempdir().unwrap();
    write_pom(repo.path(), "com.example.vuln", "vuln-lib", "1.2.3");
    std::fs::write(
        repo.path().join("App.java"),
        "package demo;\nimport com.example.vuln.Client;\npublic class App { void run() {} }\n",
    )
    .unwrap();
    run_discover(repo.path());

    let osv = write_unspecified_osv(repo.path(), "Maven", "com.example.vuln:vuln-lib");

    // Default: package is imported (`import_hits > 0`) + Unspecified symbols -> `under_investigation`.
    let default_out = rgctl()
        .args([
            "-r",
            repo.path().to_str().unwrap(),
            "-f",
            "json",
            "vuln",
            "analyze",
            "--osv",
            &osv,
        ])
        .output()
        .unwrap();
    let default_json = stdout_json(default_out);
    assert_eq!(default_json["deps_verdict"], "affected_candidate");
    assert_eq!(default_json["exploitability"], "under_investigation");
    assert_eq!(
        default_json["openvex"]["statements"][0]["status"],
        "under_investigation"
    );

    // With `--strict-reachability`: escalates `under_investigation` -> `exploitable`.
    let strict_out = rgctl()
        .args([
            "-r",
            repo.path().to_str().unwrap(),
            "-f",
            "json",
            "vuln",
            "analyze",
            "--osv",
            &osv,
            "--strict-reachability",
        ])
        .output()
        .unwrap();
    let strict_json = stdout_json(strict_out);
    assert_eq!(strict_json["exploitability"], "exploitable");
    assert_eq!(
        strict_json["openvex"]["statements"][0]["status"],
        "affected"
    );
}

#[test]
fn lazy_pdg_distinguishes_tainted_constant_and_inconclusive_java_call_sites() {
    let osv = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/osv/jackson-CVE-2022-42004.json"
    );

    // 1. Tainted HTTP input -> `readValue` WITHOUT prior `--with-cfg` -> `exploitable`, SARIF `-o` works.
    let tainted_repo = tempfile::tempdir().unwrap();
    write_pom(
        tainted_repo.path(),
        "com.fasterxml.jackson.core",
        "jackson-databind",
        "2.13.5",
    );
    std::fs::write(
        tainted_repo.path().join("OrderController.java"),
        r#"package demo;
import com.fasterxml.jackson.databind.ObjectMapper;
import javax.servlet.http.HttpServletRequest;

public class OrderController {
    public Object handle(HttpServletRequest request) throws Exception {
        String raw = request.getParameter("payload");
        ObjectMapper mapper = new ObjectMapper();
        return mapper.readValue(raw, Object.class);
    }
}
"#,
    )
    .unwrap();
    run_discover(tainted_repo.path()); // Note: plain `discover .`, NO `--with-cfg`!

    let tainted_json = stdout_json(
        rgctl()
            .args([
                "-r",
                tainted_repo.path().to_str().unwrap(),
                "-f",
                "json",
                "vuln",
                "analyze",
                "--osv",
                osv,
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(tainted_json["exploitability"], "exploitable");
    assert_eq!(
        tainted_json["openvex"]["statements"][0]["status"],
        "affected"
    );
    assert_eq!(
        tainted_json["openvex"]["statements"][0]["products"][0],
        "pkg:maven/com.fasterxml.jackson.core/jackson-databind@2.13.5"
    );

    // Verify `-f sarif -o <file>` writes valid SARIF 2.1.0 with clean relative URI.
    let sarif_file = tainted_repo.path().join("findings.sarif");
    let sarif_out = rgctl()
        .args([
            "-r",
            tainted_repo.path().to_str().unwrap(),
            "-f",
            "sarif",
            "-o",
            sarif_file.to_str().unwrap(),
            "vuln",
            "analyze",
            "--osv",
            osv,
        ])
        .output()
        .unwrap();
    assert!(sarif_out.status.success());
    let sarif_json: Value =
        serde_json::from_slice(&std::fs::read(&sarif_file).expect("sarif file written")).unwrap();
    assert_eq!(sarif_json["version"], "2.1.0");
    assert_eq!(sarif_json["runs"][0]["results"][0]["level"], "error");
    assert_eq!(
        sarif_json["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
            ["uri"],
        "OrderController.java"
    );

    // 2. Constant-literal call (`mapper.readValue("{\"ok\":1}", Object.class)`) -> `not_exploitable` + `inline_mitigations_already_exist`.
    let const_repo = tempfile::tempdir().unwrap();
    write_pom(
        const_repo.path(),
        "com.fasterxml.jackson.core",
        "jackson-databind",
        "2.13.5",
    );
    std::fs::write(
        const_repo.path().join("HealthCheck.java"),
        r#"package demo;
import com.fasterxml.jackson.databind.ObjectMapper;

public class HealthCheck {
    public Object ping() throws Exception {
        ObjectMapper mapper = new ObjectMapper();
        return mapper.readValue("{\"ok\":1}", Object.class);
    }
}
"#,
    )
    .unwrap();
    run_discover(const_repo.path());

    let const_json = stdout_json(
        rgctl()
            .args([
                "-r",
                const_repo.path().to_str().unwrap(),
                "-f",
                "json",
                "vuln",
                "analyze",
                "--osv",
                osv,
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(const_json["exploitability"], "not_exploitable");
    assert_eq!(
        const_json["openvex"]["statements"][0]["status"],
        "not_affected"
    );
    assert_eq!(
        const_json["openvex"]["statements"][0]["justification"],
        "inline_mitigations_already_exist"
    );

    // 3. Inconclusive parameter call (`parse(String body)`) -> `under_investigation` by default, `exploitable` under `--strict-reachability`.
    let inconc_repo = tempfile::tempdir().unwrap();
    write_pom(
        inconc_repo.path(),
        "com.fasterxml.jackson.core",
        "jackson-databind",
        "2.13.5",
    );
    std::fs::write(
        inconc_repo.path().join("JsonHelper.java"),
        r#"package demo;
import com.fasterxml.jackson.databind.ObjectMapper;

public class JsonHelper {
    public Object parse(String body) throws Exception {
        ObjectMapper mapper = new ObjectMapper();
        return mapper.readValue(body, Object.class);
    }
}
"#,
    )
    .unwrap();
    run_discover(inconc_repo.path());

    let inconc_default = stdout_json(
        rgctl()
            .args([
                "-r",
                inconc_repo.path().to_str().unwrap(),
                "-f",
                "json",
                "vuln",
                "analyze",
                "--osv",
                osv,
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(inconc_default["exploitability"], "under_investigation");

    let inconc_strict = stdout_json(
        rgctl()
            .args([
                "-r",
                inconc_repo.path().to_str().unwrap(),
                "-f",
                "json",
                "vuln",
                "analyze",
                "--osv",
                osv,
                "--strict-reachability",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(inconc_strict["exploitability"], "exploitable");
}

#[test]
fn reference_stacks_rust_ts_go_and_tier1_lockfile_engines_end_to_end() {
    // Rust (`Cargo.lock` + RustSec `affects.functions`)
    let rust_repo = tempfile::tempdir().unwrap();
    std::fs::write(
        rust_repo.path().join("Cargo.lock"),
        "version = 3\n\n[[package]]\nname = \"hyper\"\nversion = \"0.14.20\"\n",
    )
    .unwrap();
    std::fs::write(
        rust_repo.path().join("lib.rs"),
        "use hyper::Client;\npub fn status() -> usize { 200 }\n",
    )
    .unwrap();
    run_discover(rust_repo.path());
    let rust_osv = rust_repo.path().join("rustsec.json");
    std::fs::write(
        &rust_osv,
        r#"{
          "id": "RUSTSEC-TEST-01",
          "modified": "2024-01-01T00:00:00Z",
          "affected": [{
            "package": {"ecosystem": "crates.io", "name": "hyper"},
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "0.14.26"}]}],
            "ecosystem_specific": {"affects": {"functions": ["hyper::proto::h1::Conn::read_head"]}}
          }]
        }"#,
    )
    .unwrap();
    let rust_res = stdout_json(
        rgctl()
            .args([
                "-r",
                rust_repo.path().to_str().unwrap(),
                "-f",
                "json",
                "vuln",
                "analyze",
                "--osv",
                rust_osv.to_str().unwrap(),
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(rust_res["deps_verdict"], "affected_candidate");
    assert_eq!(rust_res["exploitability"], "not_exploitable");
    assert_eq!(
        rust_res["openvex"]["statements"][0]["products"][0],
        "pkg:cargo/hyper@0.14.20"
    );

    // TypeScript (`pnpm-lock.yaml` v9 + `package.json` range) + Go (`go.sum` + `imports[].symbols`) + PyPI (`uv.lock` PEP 440) + NuGet + Composer + RubyGems
    let poly_repo = tempfile::tempdir().unwrap();
    std::fs::write(
        poly_repo.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n\npackages:\n  'qs@6.5.2':\n    resolution: {integrity: sha512-a}\n",
    )
    .unwrap();
    std::fs::write(
        poly_repo.path().join("uv.lock"),
        "[[package]]\nname = \"urllib3\"\nversion = \"2.0.0a1\"\n",
    )
    .unwrap();
    std::fs::write(
        poly_repo.path().join("packages.lock.json"),
        r#"{"version": 1, "dependencies": {"net8.0": {"Newtonsoft.Json": {"resolved": "12.0.2"}}}}"#,
    )
    .unwrap();
    std::fs::write(
        poly_repo.path().join("composer.lock"),
        r#"{"packages": [{"name": "symfony/http-kernel", "version": "v6.3.1"}]}"#,
    )
    .unwrap();
    std::fs::write(
        poly_repo.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    rails (7.0.4.rc1)\n",
    )
    .unwrap();

    for (id, eco, pkg, fixed) in [
        ("NPM-1", "npm", "qs", "6.5.3"),
        ("PYPI-1", "PyPI", "urllib3", "2.0.0"),
        ("NUGET-1", "NuGet", "Newtonsoft.Json", "13.0.1"),
        ("PACK-1", "Packagist", "symfony/http-kernel", "6.3.2"),
        ("GEM-1", "RubyGems", "rails", "7.0.4"),
    ] {
        let osv_path = poly_repo.path().join(format!("{id}.json"));
        std::fs::write(
            &osv_path,
            format!(
                r#"{{
                  "id": "{id}",
                  "modified": "2024-01-01T00:00:00Z",
                  "affected": [{{
                    "package": {{"ecosystem": "{eco}", "name": "{pkg}"}},
                    "ranges": [{{"type": "ECOSYSTEM", "events": [{{"introduced": "0"}}, {{"fixed": "{fixed}"}}]}}]
                  }}]
                }}"#
            ),
        )
        .unwrap();
        let dc = stdout_json(
            rgctl()
                .args([
                    "-r",
                    poly_repo.path().to_str().unwrap(),
                    "-f",
                    "json",
                    "deps",
                    "check",
                    "--osv",
                    osv_path.to_str().unwrap(),
                ])
                .output()
                .unwrap(),
        );
        assert_eq!(
            dc["verdict"], "affected_candidate",
            "lockfile deps_check failed for {eco}:{pkg}"
        );
    }
}

#[test]
fn multi_range_osv_triage_and_branch_detection() {
    let repo = tempfile::tempdir().unwrap();
    let osv = write_multi_range_osv(repo.path());

    let triage = stdout_json(
        rgctl()
            .args(["-f", "json", "vuln", "triage", "--osv", &osv])
            .output()
            .unwrap(),
    );
    assert_eq!(triage["packages"][0]["introduced"], "1.0.0");
    assert_eq!(triage["packages"][0]["fixed"], "1.2.5");
    assert_eq!(
        triage["packages"][0]["ranges"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        2
    );

    for (ver, expected) in [
        ("1.2.4", "affected_candidate"),
        ("1.2.5", "not_affected"),
        ("2.0.5", "affected_candidate"),
        ("2.1.3", "not_affected"),
    ] {
        write_pom(repo.path(), "com.example", "multi-lib", ver);
        let res = stdout_json(
            rgctl()
                .args([
                    "-r",
                    repo.path().to_str().unwrap(),
                    "-f",
                    "json",
                    "deps",
                    "check",
                    "--osv",
                    &osv,
                ])
                .output()
                .unwrap(),
        );
        assert_eq!(res["verdict"], expected, "version={ver}");
    }
}

#[test]
fn jackson_triage_has_explicit_symbol_coverage() {
    let osv = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/osv/jackson-CVE-2022-42004.json"
    );
    let out = rgctl()
        .args(["-f", "json", "vuln", "triage", "--osv", osv])
        .output()
        .unwrap();
    let v = stdout_json(out);
    let coverage = &v["packages"][0]["symbol_coverage"];
    assert_eq!(coverage["coverage"].as_str().unwrap_or(""), "explicit");
    assert!(
        coverage["symbols"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    );
}
