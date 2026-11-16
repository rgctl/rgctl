//! End-to-end Tier-1 vulnerability reachability correctness suite (`-f json` / `-f sarif` only).
//!
//! Policy (`AGENTS.md`):
//! - Zero `rgctl gql` invocations (`gql` is deprecated).
//! - Every fixture runs in an isolated `TempDir` sandbox.
//! - Covers all 20 taint-enabled Tier-1 languages plus `vuln fetch`, `vuln scan`,
//!   local overrides (`--osv`, `--from-report`, `--sbom`, `--offline`),
//!   `--with-dep-stubs` JVM `.class` stitching, and `.rgctl/vuln-rules.d/*.yaml`.

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    routing::{get, post},
};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn rgctl_bin() -> &'static str {
    env!("CARGO_BIN_EXE_rgctl")
}

fn run_rgctl_json(repo: &Path, args: &[&str]) -> Value {
    assert!(
        !args.contains(&"gql"),
        "policy violation: deprecated `rgctl gql` is forbidden in correctness tests"
    );
    let mut full = vec!["-r", repo.to_str().expect("utf8 repo"), "-f", "json"];
    full.extend_from_slice(args);
    let out = Command::new(rgctl_bin())
        .args(&full)
        .output()
        .expect("spawn rgctl");
    assert!(
        out.status.success(),
        "rgctl {:?} failed:\nstdout={}\nstderr={}",
        full,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "invalid JSON from rgctl {:?}: {e}\nstdout={}",
            full,
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

fn run_rgctl_sarif(repo: &Path, args: &[&str]) -> Value {
    let mut full = vec!["-r", repo.to_str().expect("utf8 repo"), "-f", "sarif"];
    full.extend_from_slice(args);
    let out = Command::new(rgctl_bin())
        .args(&full)
        .output()
        .expect("spawn rgctl");
    assert!(
        out.status.success(),
        "rgctl -f sarif {:?} failed:\nstdout={}\nstderr={}",
        full,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("valid SARIF JSON")
}

struct Tier1VulnLangCase {
    lang: &'static str,
    manifest_file: &'static str,
    manifest_content: &'static str,
    use_sbom_flag: bool,
    ecosystem: &'static str,
    pkg_name: &'static str,
    osv_method: &'static str,
    source_file: &'static str,
    exploitable_code: &'static str,
    mitigated_code: &'static str,
    uncalled_code: &'static str,
}

fn all_20_tier1_cases() -> Vec<Tier1VulnLangCase> {
    vec![
        Tier1VulnLangCase {
            lang: "rust",
            manifest_file: "Cargo.lock",
            manifest_content: "[[package]]\nname = \"vuln_crate\"\nversion = \"1.2.0\"\n",
            use_sbom_flag: false,
            ecosystem: "crates.io",
            pkg_name: "vuln_crate",
            osv_method: "vuln_crate::Client::unsafe_sink",
            source_file: "src/lib.rs",
            exploitable_code: "use vuln_crate::Client;\npub fn entry() {\n    let q = std::env::var(\"Q\").unwrap();\n    Client::unsafe_sink(q);\n}\n",
            mitigated_code: "use vuln_crate::Client;\npub fn entry() {\n    Client::unsafe_sink(\"safe_const\");\n}\n",
            uncalled_code: "use vuln_crate::Client;\npub fn entry() {\n    Client::safe_helper(\"ok\");\n}\n",
        },
        Tier1VulnLangCase {
            lang: "java",
            manifest_file: "gradle.lockfile",
            manifest_content: "com.acme.vuln:vuln-lib:1.2.0=compileClasspath\n",
            use_sbom_flag: false,
            ecosystem: "Maven",
            pkg_name: "com.acme.vuln:vuln-lib",
            osv_method: "com.acme.vuln.VulnClient.unsafeSink",
            source_file: "src/App.java",
            exploitable_code: "import com.acme.vuln.VulnClient;\npublic class App {\n  public void entry(HttpServletRequest request, VulnClient client) {\n    String q = request.getParameter(\"q\");\n    client.unsafeSink(q);\n  }\n}\n",
            mitigated_code: "import com.acme.vuln.VulnClient;\npublic class App {\n  public void entry(HttpServletRequest request, VulnClient client) {\n    String q = request.getParameter(\"q\");\n    q = HtmlUtils.htmlEscape(q);\n    client.unsafeSink(q);\n  }\n}\n",
            uncalled_code: "import com.acme.vuln.VulnClient;\npublic class App {\n  public void entry(VulnClient client) {\n    client.safeHelper(\"ok\");\n  }\n}\n",
        },
        Tier1VulnLangCase {
            lang: "python",
            manifest_file: "requirements.txt",
            manifest_content: "vuln-py==1.2.0\n",
            use_sbom_flag: false,
            ecosystem: "PyPI",
            pkg_name: "vuln-py",
            osv_method: "vuln_py.Client.unsafe_sink",
            source_file: "app.py",
            exploitable_code: "import vuln_py\ndef entry(request, client):\n    q = request.GET['q']\n    client.unsafe_sink(q)\n",
            mitigated_code: "import vuln_py\ndef entry(request, client):\n    q = request.GET['q']\n    q = int(q)\n    client.unsafe_sink(q)\n",
            uncalled_code: "import vuln_py\ndef entry(client):\n    client.safe_helper('ok')\n",
        },
        Tier1VulnLangCase {
            lang: "go",
            manifest_file: "go.sum",
            manifest_content: "github.com/acme/vulngo v1.2.0 h1:abc=\n",
            use_sbom_flag: false,
            ecosystem: "Go",
            pkg_name: "github.com/acme/vulngo",
            osv_method: "vulngo.UnsafeSink",
            source_file: "main.go",
            exploitable_code: "package main\nimport \"github.com/acme/vulngo\"\nfunc entry() {\n    q := os.Getenv(\"Q\")\n    vulngo.UnsafeSink(q)\n}\n",
            mitigated_code: "package main\nimport \"github.com/acme/vulngo\"\nfunc entry() {\n    q := os.Getenv(\"Q\")\n    q = html.EscapeString(q)\n    vulngo.UnsafeSink(q)\n}\n",
            uncalled_code: "package main\nimport \"github.com/acme/vulngo\"\nfunc entry() {\n    vulngo.SafeHelper(\"ok\")\n}\n",
        },
        Tier1VulnLangCase {
            lang: "csharp",
            manifest_file: "packages.lock.json",
            manifest_content: "{\"version\":1,\"dependencies\":{\"net8.0\":{\"Acme.VulnNet\":{\"type\":\"Direct\",\"resolved\":\"1.2.0\"}}}}",
            use_sbom_flag: false,
            ecosystem: "NuGet",
            pkg_name: "Acme.VulnNet",
            osv_method: "Acme.VulnNet.VulnClient.UnsafeSink",
            source_file: "App.cs",
            exploitable_code: "using Acme.VulnNet;\nclass App {\n  void Entry() {\n    string q = Environment.GetEnvironmentVariable(\"Q\");\n    VulnClient.UnsafeSink(q);\n  }\n}\n",
            mitigated_code: "using Acme.VulnNet;\nclass App {\n  void Entry() {\n    string q = Environment.GetEnvironmentVariable(\"Q\");\n    q = WebUtility.HtmlEncode(q);\n    VulnClient.UnsafeSink(q);\n  }\n}\n",
            uncalled_code: "using Acme.VulnNet;\nclass App {\n  void Entry() {\n    VulnClient.SafeHelper(\"ok\");\n  }\n}\n",
        },
        Tier1VulnLangCase {
            lang: "typescript",
            manifest_file: "package-lock.json",
            manifest_content: "{\"name\":\"app\",\"lockfileVersion\":3,\"packages\":{\"node_modules/vuln-ts\":{\"version\":\"1.2.0\"}}}",
            use_sbom_flag: false,
            ecosystem: "npm",
            pkg_name: "vuln-ts",
            osv_method: "VulnClient.unsafeSink",
            source_file: "src/app.ts",
            exploitable_code: "import { VulnClient } from 'vuln-ts';\nexport function entry(req: any): void {\n  let q = req.query.id;\n  VulnClient.unsafeSink(q);\n}\n",
            mitigated_code: "import { VulnClient } from 'vuln-ts';\nexport function entry(req: any): void {\n  let q = req.query.id;\n  q = escapeHtml(q);\n  VulnClient.unsafeSink(q);\n}\n",
            uncalled_code: "import { VulnClient } from 'vuln-ts';\nexport function entry(): void {\n  VulnClient.safeHelper('ok');\n}\n",
        },
        Tier1VulnLangCase {
            lang: "javascript",
            manifest_file: "package-lock.json",
            manifest_content: "{\"name\":\"app\",\"lockfileVersion\":3,\"packages\":{\"node_modules/vuln-js\":{\"version\":\"1.2.0\"}}}",
            use_sbom_flag: false,
            ecosystem: "npm",
            pkg_name: "vuln-js",
            osv_method: "VulnClient.unsafeSink",
            source_file: "src/app.js",
            exploitable_code: "import { VulnClient } from 'vuln-js';\nexport function entry(req) {\n  let q = req.query.id;\n  VulnClient.unsafeSink(q);\n}\n",
            mitigated_code: "import { VulnClient } from 'vuln-js';\nexport function entry(req) {\n  let q = req.query.id;\n  q = escapeHtml(q);\n  VulnClient.unsafeSink(q);\n}\n",
            uncalled_code: "import { VulnClient } from 'vuln-js';\nexport function entry() {\n  VulnClient.safeHelper('ok');\n}\n",
        },
        Tier1VulnLangCase {
            lang: "c",
            manifest_file: "vcpkg.json",
            manifest_content: "{\"name\":\"app\",\"overrides\":[{\"name\":\"vulnc\",\"version\":\"1.2.0\"}]}",
            use_sbom_flag: false,
            ecosystem: "vcpkg",
            pkg_name: "vulnc",
            osv_method: "vulnc::vulnc_unsafe_sink",
            source_file: "src/app.c",
            exploitable_code: "#include \"vulnc/api.h\"\nvoid entry(void) {\n  char *q = getenv(\"Q\");\n  vulnc_unsafe_sink(q);\n}\n",
            mitigated_code: "#include \"vulnc/api.h\"\nvoid entry(void) {\n  vulnc_unsafe_sink(\"safe_const\");\n}\n",
            uncalled_code: "#include \"vulnc/api.h\"\nvoid entry(void) {\n  vulnc_safe_helper(\"ok\");\n}\n",
        },
        Tier1VulnLangCase {
            lang: "cpp",
            manifest_file: "conan.lock",
            manifest_content: "{\"graph_lock\":{\"nodes\":{\"1\":{\"ref\":\"vulncpp/1.2.0@acme/stable\"}}}}",
            use_sbom_flag: false,
            ecosystem: "Conan",
            pkg_name: "vulncpp",
            osv_method: "vulncpp::unsafe_sink",
            source_file: "src/app.cpp",
            exploitable_code: "#include <vulncpp/client.hpp>\nvoid entry() {\n  const char* q = std::getenv(\"Q\");\n  vulncpp::unsafe_sink(q);\n}\n",
            mitigated_code: "#include <vulncpp/client.hpp>\nvoid entry() {\n  vulncpp::unsafe_sink(\"safe_const\");\n}\n",
            uncalled_code: "#include <vulncpp/client.hpp>\nvoid entry() {\n  vulncpp::safe_helper(\"ok\");\n}\n",
        },
        Tier1VulnLangCase {
            lang: "php",
            manifest_file: "composer.lock",
            manifest_content: "{\"packages\":[{\"name\":\"acme/vuln-php\",\"version\":\"v1.2.0\"}]}",
            use_sbom_flag: false,
            ecosystem: "Packagist",
            pkg_name: "acme/vuln-php",
            osv_method: "Acme\\VulnPhp\\Client::unsafeSink",
            source_file: "src/app.php",
            exploitable_code: "<?php\nuse Acme\\VulnPhp\\Client;\nfunction entry() {\n  $q = $_GET['q'];\n  Client::unsafeSink($q);\n}\n",
            mitigated_code: "<?php\nuse Acme\\VulnPhp\\Client;\nfunction entry() {\n  $q = $_GET['q'];\n  $q = htmlspecialchars($q);\n  Client::unsafeSink($q);\n}\n",
            uncalled_code: "<?php\nuse Acme\\VulnPhp\\Client;\nfunction entry() {\n  Client::safeHelper('ok');\n}\n",
        },
        Tier1VulnLangCase {
            lang: "ruby",
            manifest_file: "Gemfile.lock",
            manifest_content: "GEM\n  specs:\n    vuln_gem (1.2.0)\n",
            use_sbom_flag: false,
            ecosystem: "RubyGems",
            pkg_name: "vuln_gem",
            osv_method: "VulnGem.unsafe_sink",
            source_file: "app.rb",
            exploitable_code: "require 'vuln_gem'\ndef entry(params)\n  q = params[:q]\n  VulnGem.unsafe_sink(q)\nend\n",
            mitigated_code: "require 'vuln_gem'\ndef entry(params)\n  q = params[:q]\n  q = CGI.escapeHTML(q)\n  VulnGem.unsafe_sink(q)\nend\n",
            uncalled_code: "require 'vuln_gem'\ndef entry\n  VulnGem.safe_helper('ok')\nend\n",
        },
        Tier1VulnLangCase {
            lang: "kotlin",
            manifest_file: "gradle.lockfile",
            manifest_content: "com.acme.vuln:vuln-kt:1.2.0=compileClasspath\n",
            use_sbom_flag: false,
            ecosystem: "Maven",
            pkg_name: "com.acme.vuln:vuln-kt",
            osv_method: "com.acme.vuln.VulnKt.unsafeSink",
            source_file: "src/App.kt",
            exploitable_code: "import com.acme.vuln.VulnKt\nclass App {\n  fun entry(request: Req, client: VulnKt) {\n    val q = request.getParameter(\"q\")\n    client.unsafeSink(q)\n  }\n}\n",
            mitigated_code: "import com.acme.vuln.VulnKt\nclass App {\n  fun entry(request: Req, client: VulnKt) {\n    var q = request.getParameter(\"q\")\n    q = HtmlUtils.htmlEscape(q)\n    client.unsafeSink(q)\n  }\n}\n",
            uncalled_code: "import com.acme.vuln.VulnKt\nclass App {\n  fun entry(client: VulnKt) {\n    client.safeHelper(\"ok\")\n  }\n}\n",
        },
        Tier1VulnLangCase {
            lang: "groovy",
            manifest_file: "gradle.lockfile",
            manifest_content: "com.acme.vuln:vuln-groovy:1.2.0=compileClasspath\n",
            use_sbom_flag: false,
            ecosystem: "Maven",
            pkg_name: "com.acme.vuln:vuln-groovy",
            osv_method: "com.acme.vuln.VulnGroovy.unsafeSink",
            source_file: "src/App.groovy",
            exploitable_code: "import com.acme.vuln.VulnGroovy\nclass App {\n  void entry(def request, def client) {\n    def q = request.getParameter(\"q\")\n    client.unsafeSink(q)\n  }\n}\n",
            mitigated_code: "import com.acme.vuln.VulnGroovy\nclass App {\n  void entry(def request, def client) {\n    def q = request.getParameter(\"q\")\n    q = StringEscapeUtils.escapeHtml(q)\n    client.unsafeSink(q)\n  }\n}\n",
            uncalled_code: "import com.acme.vuln.VulnGroovy\nclass App {\n  void entry(def client) {\n    client.safeHelper('ok')\n  }\n}\n",
        },
        Tier1VulnLangCase {
            lang: "puppet",
            manifest_file: "Puppetfile.lock",
            manifest_content: "FORGE\n  remote: https://forgeapi.puppet.com\n  specs:\n    acme-vulnmod (1.2.0)\n",
            use_sbom_flag: false,
            ecosystem: "Puppet",
            pkg_name: "acme/vulnmod",
            osv_method: "vulnmod::unsafe_sink",
            source_file: "manifests/init.pp",
            exploitable_code: "define profile::entry() {\n  include vulnmod\n  $q = lookup('q')\n  vulnmod::unsafe_sink($q)\n}\n",
            mitigated_code: "define profile::entry() {\n  include vulnmod\n  vulnmod::unsafe_sink('safe_const')\n}\n",
            uncalled_code: "define profile::entry() {\n  include vulnmod\n  vulnmod::safe_helper('ok')\n}\n",
        },
        Tier1VulnLangCase {
            lang: "swift",
            manifest_file: "Package.resolved",
            manifest_content: "{\"version\":2,\"pins\":[{\"identity\":\"vuln-swift\",\"location\":\"https://github.com/acme/VulnSwift.git\",\"state\":{\"version\":\"1.2.0\"}}]}",
            use_sbom_flag: false,
            ecosystem: "SwiftURL",
            pkg_name: "github.com/acme/VulnSwift",
            osv_method: "VulnSwift.unsafeSink",
            source_file: "Sources/App.swift",
            exploitable_code: "import VulnSwift\nfunc entry() {\n    let q = readLine() ?? \"\"\n    VulnSwift.unsafeSink(q)\n}\n",
            mitigated_code: "import VulnSwift\nfunc entry() {\n    VulnSwift.unsafeSink(\"safe_const\")\n}\n",
            uncalled_code: "import VulnSwift\nfunc entry() {\n    VulnSwift.safeHelper(\"ok\")\n}\n",
        },
        Tier1VulnLangCase {
            lang: "scala",
            manifest_file: "gradle.lockfile",
            manifest_content: "com.acme.vuln:vuln-scala_2.13:1.2.0=compileClasspath\n",
            use_sbom_flag: false,
            ecosystem: "Maven",
            pkg_name: "com.acme.vuln:vuln-scala_2.13",
            osv_method: "com.acme.vuln.VulnScala.unsafeSink",
            source_file: "src/App.scala",
            exploitable_code: "import com.acme.vuln.VulnScala\nobject App {\n  def entry(): Unit = {\n    val q = StdIn.readLine()\n    VulnScala.unsafeSink(q)\n  }\n}\n",
            mitigated_code: "import com.acme.vuln.VulnScala\nobject App {\n  def entry(): Unit = {\n    VulnScala.unsafeSink(\"safe_const\")\n  }\n}\n",
            uncalled_code: "import com.acme.vuln.VulnScala\nobject App {\n  def entry(): Unit = {\n    VulnScala.safeHelper(\"ok\")\n  }\n}\n",
        },
        Tier1VulnLangCase {
            lang: "dart",
            manifest_file: "pubspec.lock",
            manifest_content: "packages:\n  vuln_pub:\n    dependency: \"direct main\"\n    version: \"1.2.0\"\n",
            use_sbom_flag: false,
            ecosystem: "Pub",
            pkg_name: "vuln_pub",
            osv_method: "VulnClient.unsafeSink",
            source_file: "lib/app.dart",
            exploitable_code: "import 'package:vuln_pub/vuln_pub.dart';\nvoid entry(dynamic client) {\n  var q = stdin.readLineSync();\n  client.unsafeSink(q);\n}\n",
            mitigated_code: "import 'package:vuln_pub/vuln_pub.dart';\nvoid entry(dynamic client) {\n  var q = stdin.readLineSync();\n  q = HtmlEscape().convert(q);\n  client.unsafeSink(q);\n}\n",
            uncalled_code: "import 'package:vuln_pub/vuln_pub.dart';\nvoid entry(dynamic client) {\n  client.safeHelper('ok');\n}\n",
        },
        Tier1VulnLangCase {
            lang: "elixir",
            manifest_file: "mix.lock",
            manifest_content: "%{\n  \"vuln_hex\": {:hex, :vuln_hex, \"1.2.0\", \"hash\"}\n}\n",
            use_sbom_flag: false,
            ecosystem: "Hex",
            pkg_name: "vuln_hex",
            osv_method: "VulnHex.unsafe_sink",
            source_file: "lib/app.ex",
            exploitable_code: "defmodule App do\n  alias VulnHex\n  def entry() do\n    q = System.get_env(\"Q\")\n    VulnHex.unsafe_sink(q)\n  end\nend\n",
            mitigated_code: "defmodule App do\n  alias VulnHex\n  def entry() do\n    q = System.get_env(\"Q\")\n    q = Plug.HTML.html_escape(q)\n    VulnHex.unsafe_sink(q)\n  end\nend\n",
            uncalled_code: "defmodule App do\n  alias VulnHex\n  def entry() do\n    VulnHex.safe_helper(\"ok\")\n  end\nend\n",
        },
        Tier1VulnLangCase {
            lang: "lua",
            manifest_file: "app-1.0-1.rockspec",
            manifest_content: "package = \"app\"\nversion = \"1.0-1\"\ndependencies = {\n  \"vulnlua == 1.2.0\"\n}\n",
            use_sbom_flag: false,
            ecosystem: "LuaRocks",
            pkg_name: "vulnlua",
            osv_method: "vulnlua.unsafe_sink",
            source_file: "app.lua",
            exploitable_code: "local vulnlua = require('vulnlua')\nfunction entry()\n  local q = io.read()\n  vulnlua.unsafe_sink(q)\nend\n",
            mitigated_code: "local vulnlua = require('vulnlua')\nfunction entry()\n  local q = io.read()\n  q = shellescape(q)\n  vulnlua.unsafe_sink(q)\nend\n",
            uncalled_code: "local vulnlua = require('vulnlua')\nfunction entry()\n  vulnlua.safe_helper('ok')\nend\n",
        },
        Tier1VulnLangCase {
            lang: "bash",
            manifest_file: "bom.cdx.json",
            manifest_content: "{\"bomFormat\":\"CycloneDX\",\"specVersion\":\"1.5\",\"components\":[{\"name\":\"vuln-cli\",\"version\":\"1.2.0\",\"purl\":\"pkg:npm/vuln-cli@1.2.0\"}]}",
            use_sbom_flag: true,
            ecosystem: "npm",
            pkg_name: "vuln-cli",
            osv_method: "vuln_cli_sink",
            source_file: "deploy.sh",
            exploitable_code: "#!/usr/bin/env bash\nentry() {\n  read -r q\n  vuln_cli_sink \"$q\"\n}\n",
            mitigated_code: "#!/usr/bin/env bash\nentry() {\n  vuln_cli_sink 'safe_const'\n}\n",
            uncalled_code: "#!/usr/bin/env bash\nentry() {\n  vuln_cli_safe 'ok'\n}\n",
        },
    ]
}

#[test]
fn all_20_tier1_taint_languages_exploitable_mitigated_and_uncalled_via_json_cli() {
    let cases = all_20_tier1_cases();
    assert_eq!(
        cases.len(),
        20,
        "must cover all 20 taint-enabled Tier-1 languages"
    );

    for case in &cases {
        for (variant, code, expected_verdict, expected_status, expected_just) in [
            (
                "exploitable",
                case.exploitable_code,
                "exploitable",
                "affected",
                None,
            ),
            (
                "mitigated",
                case.mitigated_code,
                "not_exploitable",
                "not_affected",
                Some("inline_mitigations_already_exist"),
            ),
            (
                "uncalled",
                case.uncalled_code,
                "not_exploitable",
                "not_affected",
                Some("vulnerable_code_not_in_execute_path"),
            ),
        ] {
            let tmp = TempDir::new().expect("sandbox");
            let root = tmp.path();

            let manifest_path = root.join(case.manifest_file);
            if let Some(p) = manifest_path.parent() {
                fs::create_dir_all(p).unwrap();
            }
            fs::write(&manifest_path, case.manifest_content).unwrap();

            let src_path = root.join(case.source_file);
            if let Some(p) = src_path.parent() {
                fs::create_dir_all(p).unwrap();
            }
            fs::write(&src_path, code).unwrap();

            let osv_path = root.join("advisory.osv.json");
            let osv_doc = json!({
                "id": format!("GHSA-TIER1-{}-{variant}", case.lang.to_ascii_uppercase()),
                "modified": "2025-06-01T00:00:00Z",
                "affected": [{
                    "package": {
                        "ecosystem": case.ecosystem,
                        "name": case.pkg_name
                    },
                    "ranges": [{
                        "type": "ECOSYSTEM",
                        "events": [{"introduced": "1.0.0"}, {"fixed": "2.0.0"}]
                    }],
                    "ecosystem_specific": {
                        "affected_methods": [case.osv_method]
                    }
                }]
            });
            fs::write(&osv_path, serde_json::to_vec_pretty(&osv_doc).unwrap()).unwrap();

            // 1. Default `discover .` WITHOUT `--with-cfg` (proves lazy per-call-site CPG hydration!).
            let _ = run_rgctl_json(root, &["discover", "."]);

            // 2. Canonical `-f json find` sanity check (zero `rgctl gql`).
            let found = run_rgctl_json(root, &["find", "--type", "function"]);
            assert!(
                found["total"].as_u64().unwrap_or(0) >= 1,
                "[{} / {variant}] expected >= 1 indexed function, got {found}",
                case.lang
            );

            // 3. Run `vuln analyze` via `-f json`.
            let osv_str = osv_path.to_str().unwrap();
            let sbom_str = manifest_path.to_str().unwrap();
            let mut analyze_args = vec!["vuln", "analyze", "--osv", osv_str];
            if case.use_sbom_flag {
                analyze_args.extend_from_slice(&["--sbom", sbom_str]);
            }
            let res = run_rgctl_json(root, &analyze_args);

            assert_eq!(
                res["deps_verdict"].as_str(),
                Some("affected_candidate"),
                "[{} / {variant}] deps_verdict mismatch: {res}",
                case.lang
            );
            assert_eq!(
                res["exploitability"].as_str(),
                Some(expected_verdict),
                "[{} / {variant}] exploitability mismatch: {res}",
                case.lang
            );
            assert_eq!(
                res["openvex"]["statements"][0]["status"].as_str(),
                Some(expected_status),
                "[{} / {variant}] OpenVEX status mismatch: {res}",
                case.lang
            );
            let prod = res["openvex"]["statements"][0]["products"][0]
                .as_str()
                .unwrap_or("");
            assert!(
                prod.starts_with("pkg:"),
                "[{} / {variant}] expected canonical PURL in OpenVEX products, got {prod:?}",
                case.lang
            );
            if let Some(just) = expected_just {
                assert_eq!(
                    res["openvex"]["statements"][0]["justification"].as_str(),
                    Some(just),
                    "[{} / {variant}] OpenVEX justification mismatch: {res}",
                    case.lang
                );
            }

            // Verify SARIF 2.1.0 output on exploitable findings.
            if variant == "exploitable" {
                let sarif = run_rgctl_sarif(root, &analyze_args);
                assert_eq!(sarif["version"].as_str(), Some("2.1.0"));
                assert_eq!(
                    sarif["runs"][0]["results"][0]["level"].as_str(),
                    Some("error"),
                    "[{}] SARIF level mismatch: {sarif}",
                    case.lang
                );
            }
        }
    }
}

/// Synthesize a minimal valid JVM `.class` binary where `this_internal.from_method` invokes
/// `target_internal.target_method` via `invokevirtual` (`0xB6`).
fn build_synthetic_jvm_wrapper_class(
    this_internal: &str,
    from_method: &str,
    target_internal: &str,
    target_method: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0xCAFE_BABEu32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&52u16.to_be_bytes());
    out.extend_from_slice(&13u16.to_be_bytes());
    let push_utf8 = |buf: &mut Vec<u8>, s: &str| {
        buf.push(1);
        buf.extend_from_slice(&(s.len() as u16).to_be_bytes());
        buf.extend_from_slice(s.as_bytes());
    };
    push_utf8(&mut out, this_internal);
    out.extend_from_slice(&[7, 0, 1]);
    push_utf8(&mut out, "java/lang/Object");
    out.extend_from_slice(&[7, 0, 3]);
    push_utf8(&mut out, from_method);
    push_utf8(&mut out, "(Ljava/lang/String;)V");
    push_utf8(&mut out, "Code");
    push_utf8(&mut out, target_internal);
    out.extend_from_slice(&[7, 0, 8]);
    push_utf8(&mut out, target_method);
    out.extend_from_slice(&[12, 0, 10, 0, 6]);
    out.extend_from_slice(&[10, 0, 9, 0, 11]);

    out.extend_from_slice(&0x0021u16.to_be_bytes());
    out.extend_from_slice(&2u16.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());

    out.extend_from_slice(&0x0001u16.to_be_bytes());
    out.extend_from_slice(&5u16.to_be_bytes());
    out.extend_from_slice(&6u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());

    let bytecode: [u8; 4] = [0xB6, 0x00, 12, 0xB1];
    let mut code_attr = Vec::new();
    code_attr.extend_from_slice(&2u16.to_be_bytes());
    code_attr.extend_from_slice(&2u16.to_be_bytes());
    code_attr.extend_from_slice(&(bytecode.len() as u32).to_be_bytes());
    code_attr.extend_from_slice(&bytecode);
    code_attr.extend_from_slice(&0u16.to_be_bytes());
    code_attr.extend_from_slice(&0u16.to_be_bytes());

    out.extend_from_slice(&7u16.to_be_bytes());
    out.extend_from_slice(&(code_attr.len() as u32).to_be_bytes());
    out.extend_from_slice(&code_attr);
    out.extend_from_slice(&0u16.to_be_bytes());
    out
}

#[derive(Clone)]
struct MockServerState {
    hits: Arc<AtomicUsize>,
}

async fn mock_querybatch(
    State(st): State<MockServerState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    st.hits.fetch_add(1, Ordering::SeqCst);
    let queries = body["queries"].as_array().cloned().unwrap_or_default();
    let results: Vec<Value> = queries
        .iter()
        .map(|q| {
            let name = q["package"]["name"].as_str().unwrap_or("");
            if name.contains("jackson-databind") {
                json!({ "vulns": [{ "id": "GHSA-batch-jackson-01" }] })
            } else if name.contains("slf4j") {
                json!({ "vulns": [{ "id": "GHSA-batch-slf4j-02" }] })
            } else {
                json!({ "vulns": [] })
            }
        })
        .collect();
    Json(json!({ "results": results }))
}

async fn mock_get_vuln(
    State(st): State<MockServerState>,
    AxumPath(id): AxumPath<String>,
) -> Json<Value> {
    st.hits.fetch_add(1, Ordering::SeqCst);
    if id == "GHSA-batch-jackson-01" {
        Json(json!({
            "id": id,
            "modified": "2025-01-01T00:00:00Z",
            "affected": [{
                "package": {
                    "ecosystem": "Maven",
                    "name": "com.fasterxml.jackson.core:jackson-databind"
                },
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [{"introduced": "2.0.0"}, {"fixed": "2.14.0"}]
                }],
                "ecosystem_specific": {
                    "affected_methods": ["com.fasterxml.jackson.databind.ObjectMapper.readValue"]
                }
            }]
        }))
    } else {
        Json(json!({
            "id": id,
            "modified": "2025-01-01T00:00:00Z",
            "affected": [{
                "package": {
                    "ecosystem": "Maven",
                    "name": "org.slf4j:slf4j-api"
                },
                "ranges": [{
                    "type": "ECOSYSTEM",
                    "events": [{"introduced": "1.0.0"}, {"fixed": "2.1.0"}]
                }],
                "ecosystem_specific": {
                    "affected_methods": ["org.slf4j.LoggerFactory.unusedDanger"]
                }
            }]
        }))
    }
}

#[test]
fn network_fetch_batch_scan_overrides_dep_stubs_and_trigger_rules_end_to_end() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/v1/querybatch", post(mock_querybatch))
        .route("/v1/vulns/{id}", get(mock_get_vuln))
        .with_state(MockServerState { hits: hits.clone() });
    let (addr, fut) = rt.block_on(async {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        (a, async move {
            let _ = axum::serve(l, app).await;
        })
    });
    rt.spawn(fut);
    let api_url = format!("http://{addr}");

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Repo with shaded/Gradle wrapper JAR (`lib/audit-client.jar`, NO META-INF/maven/pom.xml!)
    // plus `gradle.lockfile` for batch `vuln fetch` / `vuln scan`.
    fs::write(
        root.join("gradle.lockfile"),
        "com.fasterxml.jackson.core:jackson-databind:2.13.4=compileClasspath\norg.slf4j:slf4j-api:2.0.5=compileClasspath\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/OrderController.java"),
        r#"import com.enterprise.audit.AuditClient;
public class OrderController {
    public void handle(HttpServletRequest request, AuditClient auditClient) {
        String body = request.getParameter("payload");
        auditClient.logEvent(body);
    }
}
"#,
    )
    .unwrap();

    fs::create_dir_all(root.join("lib")).unwrap();
    {
        let f = fs::File::create(root.join("lib/audit-client.jar")).unwrap();
        let mut zip = ZipWriter::new(f);
        zip.start_file(
            "com/enterprise/audit/AuditClient.class",
            SimpleFileOptions::default(),
        )
        .unwrap();
        let cls = build_synthetic_jvm_wrapper_class(
            "com/enterprise/audit/AuditClient",
            "logEvent",
            "com/fasterxml/jackson/databind/ObjectMapper",
            "readValue",
        );
        zip.write_all(&cls).unwrap();
        zip.finish().unwrap();
    }

    let _ = run_rgctl_json(root, &["discover", "."]);

    // 1. `rgctl -f json vuln fetch` against mock OSV server.
    let fetch_res = run_rgctl_json(root, &["vuln", "fetch", "--osv-api-url", &api_url]);
    assert_eq!(fetch_res["packages_queried"], 2);
    assert_eq!(fetch_res["advisories_matched"], 2);
    assert!(hits.load(Ordering::SeqCst) >= 3);

    // 2. Offline local override (`--offline`) + `--with-dep-stubs` stitches
    //    `OrderController.handle -> AuditClient.logEvent -> ObjectMapper.readValue` with ZERO HTTP hits!
    hits.store(0, Ordering::SeqCst);
    let scan_res = run_rgctl_json(
        root,
        &[
            "vuln",
            "scan",
            "--offline",
            "--osv-api-url",
            &api_url,
            "--include-jars",
            "lib",
            "--with-dep-stubs",
        ],
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "--offline must make zero HTTP requests"
    );
    assert_eq!(scan_res["counts"]["total"], 2);
    assert_eq!(scan_res["counts"]["exploitable"], 1);
    assert_eq!(scan_res["counts"]["not_exploitable"], 1);
    // Deterministic sorting by advisory ID (`GHSA-batch-jackson-01` before `GHSA-batch-slf4j-02`).
    assert_eq!(scan_res["results"][0]["osv_id"], "GHSA-batch-jackson-01");
    assert_eq!(scan_res["results"][0]["exploitability"], "exploitable");
    let stitched_to = scan_res["results"][0]["sink_taint"]["paths"][0]["to"]
        .as_str()
        .unwrap_or("");
    assert!(
        stitched_to.contains("AuditClient.logEvent")
            && stitched_to.contains("ObjectMapper.readValue"),
        "expected stitched transitive chain in sink_taint.paths[0].to, got {stitched_to:?}"
    );
    assert_eq!(scan_res["results"][1]["osv_id"], "GHSA-batch-slf4j-02");
    assert_eq!(scan_res["results"][1]["exploitability"], "not_exploitable");

    // 3. Batch SARIF 2.1.0 output (`-f sarif vuln scan --offline ...`).
    let batch_sarif = run_rgctl_sarif(
        root,
        &[
            "vuln",
            "scan",
            "--offline",
            "--include-jars",
            "lib",
            "--with-dep-stubs",
        ],
    );
    assert_eq!(batch_sarif["version"], "2.1.0");
    assert_eq!(
        batch_sarif["runs"][0]["results"].as_array().unwrap().len(),
        2
    );

    // 4. Declarative trigger-condition rule (`.rgctl/vuln-rules.d/jackson.yaml`) downgrades
    //    conditional CVE when `UNWRAP_SINGLE_VALUE_ARRAYS` is absent!
    let rules_dir = root.join(".rgctl/vuln-rules.d");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(
        rules_dir.join("jackson.yaml"),
        r#"rules:
  - id: jackson-unwrap-flag
    advisories: ["GHSA-batch-jackson-01"]
    required_patterns: ["UNWRAP_SINGLE_VALUE_ARRAYS"]
    missing_verdict: not_exploitable
    reason: "Requires UNWRAP_SINGLE_VALUE_ARRAYS enabled"
"#,
    )
    .unwrap();

    let osv_dir = root.join(".rgctl/osv");
    let gated_res = run_rgctl_json(
        root,
        &[
            "vuln",
            "analyze",
            "--osv",
            osv_dir.to_str().unwrap(),
            "--include-jars",
            "lib",
            "--with-dep-stubs",
        ],
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "--osv <dir> must make zero HTTP requests"
    );
    assert_eq!(
        gated_res["results"][0]["exploitability"], "not_exploitable",
        "declarative rule should downgrade when UNWRAP_SINGLE_VALUE_ARRAYS is absent: {gated_res}"
    );
}
