//! Contract correctness suite for all 42 `handler = "queries"` (`tags.scm`) Tier-2 plugins
//! and all 5 `rgctl-config-formats` plugins (`yaml`, `json`, `toml`, `properties`, `xml`).
//!
//! Enforces symbol extraction ($\ge 2$ symbols per snippet), valid byte spans, `EdgeType::Calls`
//! on call-capturing grammars, `.m` Objective-C vs. MATLAB content disambiguation, `ConfigKey` +
//! `EdgeType::UsesConfig` across all 5 config formats, and `INV-1..INV-9` on every snapshot.

use rgctl_graph::schema::{EdgeType, Node, NodeType};
use rgctl_graph::{SNAPSHOT_FILE, SnapshotNodeStore, assert_graph_invariants};
use rgctl_languages::default_registry;
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use rgctl_registry::LanguageRegistry;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tempfile::TempDir;

fn shared_default_registry() -> Arc<LanguageRegistry> {
    static REGISTRY: OnceLock<Arc<LanguageRegistry>> = OnceLock::new();
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(default_registry())))
}

fn write_file(repo: &Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn run_pipeline_and_check_invariants(
    registry: Arc<LanguageRegistry>,
    repo: &Path,
) -> (Vec<Node>, Vec<(uuid::Uuid, uuid::Uuid, EdgeType)>) {
    let data_dir = repo.join(".rgctl");
    fs::create_dir_all(&data_dir).unwrap();
    let snap = data_dir.join(SNAPSHOT_FILE);
    let pipeline = ProcessingPipeline::with_config(
        registry,
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            ..PipelineConfig::default()
        },
    );
    pipeline
        .process_repository_to_snapshot(repo, &snap, None)
        .expect("pipeline snapshot");
    let store = SnapshotNodeStore::open(&snap).expect("open snapshot");
    let col = store.mmap().columnar().expect("columnar");
    assert_graph_invariants(col).expect("INV-1..INV-9 must hold");

    let mut nodes = Vec::new();
    for idx in 0..col.node_count() {
        nodes.push(col.materialize_node_at(idx).expect("materialize"));
    }
    let mut edges = Vec::new();
    for idx in 0..col.edge_count() {
        edges.push(col.edge_at(idx).expect("edge_at"));
    }
    (nodes, edges)
}

struct Tier2Case {
    id: &'static str,
    rel: &'static str,
    expect_calls: bool,
    source: &'static str,
}

const DEFAULT_REGISTRY_TIER2_CASES: &[Tier2Case] = &[
    Tier2Case {
        id: "zig",
        rel: "zig/main.zig",
        expect_calls: true,
        source: "pub fn helper() i32 { return 1; }\npub fn caller() i32 { return helper(); }\n",
    },
    Tier2Case {
        id: "nix",
        rel: "nix/default.nix",
        expect_calls: true,
        source: "{\n  helper = x: x + 1;\n  caller = y: helper y;\n}\n",
    },
    Tier2Case {
        id: "proto",
        rel: "proto/order.proto",
        expect_calls: false,
        source: "syntax = \"proto3\";\nmessage OrderReq { string id = 1; }\nmessage OrderResp { bool ok = 1; }\nservice OrderSvc { rpc Create(OrderReq) returns (OrderResp); }\n",
    },
    Tier2Case {
        id: "solidity",
        rel: "solidity/Vault.sol",
        expect_calls: true,
        source: "pragma solidity ^0.8.0;\ncontract Vault {\n  function helper() public pure returns (uint256) { return 1; }\n  function caller() public pure returns (uint256) { return helper(); }\n}\n",
    },
    Tier2Case {
        id: "julia",
        rel: "julia/math.jl",
        expect_calls: true,
        source: "function helper(x)\n  return x + 1\nend\nfunction caller(y)\n  return helper(y)\nend\n",
    },
    Tier2Case {
        id: "r",
        rel: "r/stats.R",
        expect_calls: true,
        source: "helper <- function(x) {\n  x + 1\n}\ncaller <- function(y) {\n  helper(y)\n}\n",
    },
    Tier2Case {
        id: "haskell",
        rel: "haskell/Math.hs",
        expect_calls: true,
        source: "module Math where\nhelper x = x + 1\ncaller y = helper y\n",
    },
    Tier2Case {
        id: "gdscript",
        rel: "gdscript/player.gd",
        expect_calls: true,
        source: "class Inner:\n\tpass\nfunc helper(x):\n\treturn x + 1\nfunc caller(y):\n\treturn helper(y)\n",
    },
    Tier2Case {
        id: "fortran",
        rel: "fortran/calc.f90",
        expect_calls: true,
        source: "module calc_mod\ncontains\nsubroutine helper(x)\n  integer :: x\nend subroutine helper\nsubroutine caller(y)\n  integer :: y\n  call helper(y)\nend subroutine caller\nend module calc_mod\n",
    },
    Tier2Case {
        id: "verilog",
        rel: "verilog/alu.sv",
        expect_calls: true,
        source: "module alu;\n  function int helper(input int x);\n    return x + 1;\n  endfunction\n  function int caller(input int y);\n    return helper(y);\n  endfunction\nendmodule\n",
    },
    Tier2Case {
        id: "objc",
        rel: "objc/Service.mm",
        expect_calls: true,
        source: "int helper_fn(int x) { return x + 1; }\nint caller_fn(int y) { return helper_fn(y); }\n",
    },
    Tier2Case {
        id: "clojure",
        rel: "clojure/core.clj",
        expect_calls: true,
        source: "(ns demo.core)\n(defn helper [x] x)\n(defn caller [y] (helper y))\n",
    },
    Tier2Case {
        id: "ocaml",
        rel: "ocaml/math.ml",
        expect_calls: true,
        source: "let helper x = x + 1\nlet caller y = helper y\n",
    },
    Tier2Case {
        id: "elisp",
        rel: "elisp/init.el",
        expect_calls: true,
        source: "(defun helper (x) x)\n(defun caller (y) (helper y))\n",
    },
    Tier2Case {
        id: "html",
        rel: "html/index.html",
        expect_calls: false,
        source: "<html><body><main><section>ok</section></main><script>var x = 1;</script></body></html>\n",
    },
    Tier2Case {
        id: "css",
        rel: "css/app.css",
        expect_calls: false,
        source: ".header { color: red; }\n.footer { color: blue; }\n@keyframes fade { from { opacity: 0; } to { opacity: 1; } }\n",
    },
    Tier2Case {
        id: "svelte",
        rel: "svelte/App.svelte",
        expect_calls: false,
        source: "<script>\n  let count = 0;\n</script>\n<main><button>{count}</button></main>\n",
    },
    Tier2Case {
        id: "graphql",
        rel: "graphql/schema.graphql",
        expect_calls: false,
        source: "type Order { id: ID! }\nquery GetOrder { order { id } }\nfragment OrderFields on Order { id }\n",
    },
    Tier2Case {
        id: "prisma",
        rel: "prisma/schema.prisma",
        expect_calls: false,
        source: "model Order {\n  id Int @id\n}\nenum OrderStatus {\n  OPEN\n  PAID\n}\n",
    },
    Tier2Case {
        id: "elm",
        rel: "elm/Main.elm",
        expect_calls: true,
        source: "module Main exposing (helper, caller)\nhelper x =\n    x + 1\ncaller y =\n    helper y\n",
    },
    Tier2Case {
        id: "erlang",
        rel: "erlang/calc.erl",
        expect_calls: true,
        source: "-module(calc).\nhelper(X) -> X + 1.\ncaller(Y) -> helper(Y).\n",
    },
    Tier2Case {
        id: "gleam",
        rel: "gleam/calc.gleam",
        expect_calls: true,
        source: "pub fn helper(x: Int) -> Int {\n  x + 1\n}\npub fn caller(y: Int) -> Int {\n  helper(y)\n}\n",
    },
    Tier2Case {
        id: "fsharp",
        rel: "fsharp/Calc.fs",
        expect_calls: true,
        source: "module Calc\nlet helper x = x + 1\nlet caller y = helper y\n",
    },
    Tier2Case {
        id: "racket",
        rel: "racket/calc.rkt",
        expect_calls: true,
        source: "#lang racket\n(define (helper x) x)\n(define (caller y) (helper y))\n",
    },
    Tier2Case {
        id: "scheme",
        rel: "scheme/calc.scm",
        expect_calls: true,
        source: "(define (helper x) x)\n(define (caller y) (helper y))\n",
    },
    Tier2Case {
        id: "d",
        rel: "d/calc.d",
        expect_calls: true,
        source: "module calc;\nint helper(int x) { return x + 1; }\nint caller(int y) { return helper(y); }\n",
    },
    Tier2Case {
        id: "odin",
        rel: "odin/calc.odin",
        expect_calls: true,
        source: "package calc\nhelper :: proc(x: int) -> int { return x + 1 }\ncaller :: proc(y: int) -> int { return helper(y) }\n",
    },
    Tier2Case {
        id: "cuda",
        rel: "cuda/kernel.cu",
        expect_calls: true,
        source: "__device__ int helper(int x) { return x + 1; }\n__global__ void caller() { int v = helper(1); }\n",
    },
    Tier2Case {
        id: "glsl",
        rel: "glsl/shader.glsl",
        expect_calls: true,
        source: "float helper(float x) { return x + 1.0; }\nvoid main() { float y = helper(2.0); }\n",
    },
    Tier2Case {
        id: "vhdl",
        rel: "vhdl/alu.vhd",
        expect_calls: true,
        source: "entity alu is\nend entity alu;\narchitecture rtl of alu is\n  procedure helper is\n  begin\n  end procedure helper;\n  procedure caller is\n  begin\n    helper;\n  end procedure caller;\nbegin\nend architecture rtl;\n",
    },
    Tier2Case {
        id: "cmake",
        rel: "cmake/Build.cmake",
        expect_calls: true,
        source: "function(helper_fn arg)\nendfunction()\nfunction(caller_fn arg)\n  helper_fn(${arg})\nendfunction()\n",
    },
    Tier2Case {
        id: "make",
        rel: "make/rules.mk",
        expect_calls: false,
        source: "CC := gcc\nbuild:\n\t@echo build\nclean:\n\t@echo clean\n",
    },
    Tier2Case {
        id: "starlark",
        rel: "starlark/defs.bzl",
        expect_calls: true,
        source: "def helper(x):\n    return x + 1\ndef caller(y):\n    return helper(y)\n",
    },
    Tier2Case {
        id: "powershell",
        rel: "powershell/Build.ps1",
        expect_calls: true,
        source: "function Invoke-Helper {\n  param($x)\n  return $x\n}\nfunction Invoke-Caller {\n  Invoke-Helper 1\n}\n",
    },
    Tier2Case {
        id: "apex",
        rel: "apex/OrderService.cls",
        expect_calls: true,
        source: "public class OrderService {\n  public Integer helper(Integer x) { return x + 1; }\n  public Integer caller(Integer y) { return helper(y); }\n}\n",
    },
    Tier2Case {
        id: "pascal",
        rel: "pascal/Unit1.pas",
        expect_calls: true,
        source: "unit Unit1;\ninterface\nprocedure Helper;\nprocedure Caller;\nimplementation\nprocedure Helper;\nbegin\nend;\nprocedure Caller;\nbegin\n  Helper();\nend;\nend.\n",
    },
    Tier2Case {
        id: "perl",
        rel: "perl/order.pl",
        expect_calls: true,
        source: "package OrderPkg;\nsub helper { return 1; }\nsub caller { return helper(); }\n1;\n",
    },
    Tier2Case {
        id: "ada",
        rel: "ada/calc.adb",
        expect_calls: true,
        source: "package body Calc is\n  procedure Helper is\n  begin\n    null;\n  end Helper;\n  procedure Caller is\n  begin\n    Helper;\n  end Caller;\nend Calc;\n",
    },
    Tier2Case {
        id: "matlab",
        rel: "matlab/solver.matlab",
        expect_calls: true,
        source: "function y = caller(x)\n  y = helper(x);\nend\nfunction z = helper(a)\n  z = a + 1;\nend\n",
    },
];

#[test]
fn all_39_default_registry_tier2_query_plugins_extract_symbols_calls_and_pass_invariants() {
    assert_eq!(
        DEFAULT_REGISTRY_TIER2_CASES.len(),
        39,
        "39 default_registry() Tier-2 query plugins (+ 3 standalone json/yaml/toml = 42)"
    );

    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    for case in DEFAULT_REGISTRY_TIER2_CASES {
        write_file(repo, case.rel, case.source);
    }

    let (nodes, edges) = run_pipeline_and_check_invariants(shared_default_registry(), repo);

    let mut failures = Vec::new();
    for case in DEFAULT_REGISTRY_TIER2_CASES {
        let file_nodes: Vec<&Node> = nodes
            .iter()
            .filter(|n| {
                n.node_type != NodeType::File
                    && n.file_path
                        .as_deref()
                        .is_some_and(|p| p.replace('\\', "/").ends_with(case.rel))
            })
            .collect();
        if file_nodes.len() < 2 {
            failures.push(format!(
                "[{}] expected >= 2 extracted symbols in {}, got {}: {:?}",
                case.id,
                case.rel,
                file_nodes.len(),
                file_nodes
                    .iter()
                    .map(|n| (&n.node_type, n.name.as_str()))
                    .collect::<Vec<_>>()
            ));
            continue;
        }
        for sym in &file_nodes {
            if sym.start_byte.is_none() || sym.end_byte.is_none() {
                failures.push(format!(
                    "[{}] symbol {} missing byte span ({:?}..{:?})",
                    case.id, sym.name, sym.start_byte, sym.end_byte
                ));
            }
        }
        if case.expect_calls {
            let file_ids: BTreeSet<uuid::Uuid> = file_nodes.iter().map(|n| n.id).collect();
            let has_call = edges.iter().any(|(f, t, et)| {
                *et == EdgeType::Calls && file_ids.contains(f) && file_ids.contains(t)
            });
            if !has_call {
                failures.push(format!(
                    "[{}] expected intra-file EdgeType::Calls in {}, symbols={:?}",
                    case.id,
                    case.rel,
                    file_nodes
                        .iter()
                        .map(|n| n.name.as_str())
                        .collect::<Vec<_>>()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "Tier-2 query plugin contract failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn standalone_json_yaml_toml_query_plugins_extract_symbols_and_pass_invariants() {
    let mut standalone = LanguageRegistry::empty();
    rgctl_lang_json::register(&mut standalone);
    rgctl_lang_yaml::register(&mut standalone);
    rgctl_lang_toml::register(&mut standalone);
    let registry = Arc::new(standalone);

    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write_file(
        repo,
        "cfg/data.json",
        "{\n  \"alpha\": 1,\n  \"beta\": 2\n}\n",
    );
    write_file(repo, "cfg/data.yaml", "alpha: 1\nbeta: 2\n");
    write_file(
        repo,
        "cfg/data.toml",
        "[service]\nport = 8080\nhost = \"localhost\"\n",
    );

    let (nodes, _edges) = run_pipeline_and_check_invariants(registry, repo);

    for (id, rel) in [
        ("json", "cfg/data.json"),
        ("yaml", "cfg/data.yaml"),
        ("toml", "cfg/data.toml"),
    ] {
        let syms: Vec<&Node> = nodes
            .iter()
            .filter(|n| {
                n.node_type != NodeType::File
                    && n.file_path
                        .as_deref()
                        .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
            })
            .collect();
        assert!(
            syms.len() >= 2,
            "[{id}] standalone QueryLanguagePlugin expected >= 2 symbols in {rel}, got {syms:?}"
        );
    }
}

#[test]
fn dot_m_extension_disambiguates_objc_and_matlab_during_discover() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();

    write_file(
        repo,
        "ios/Widget.m",
        "#import <Foundation/Foundation.h>\n@implementation Widget\n- (void)helper {}\n- (void)caller { [self helper]; }\n@end\n",
    );
    write_file(
        repo,
        "dsp/filter_step.m",
        "function [y] = filter_step(x)\n  y = inner_gain(x);\nend\nfunction [g] = inner_gain(v)\n  g = v * 2;\nend\n",
    );

    let (nodes, edges) = run_pipeline_and_check_invariants(shared_default_registry(), repo);

    let objc_syms: Vec<&Node> = nodes
        .iter()
        .filter(|n| {
            n.node_type != NodeType::File
                && n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with("ios/Widget.m"))
        })
        .collect();
    assert!(
        objc_syms.iter().any(|n| n.name.contains("Widget"))
            && objc_syms.iter().any(|n| n.name.contains("helper")),
        "expected Objective-C symbols in ios/Widget.m, got {objc_syms:?}"
    );

    let matlab_syms: Vec<&Node> = nodes
        .iter()
        .filter(|n| {
            n.node_type != NodeType::File
                && n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with("dsp/filter_step.m"))
        })
        .collect();
    assert!(
        matlab_syms.iter().any(|n| n.name.as_str() == "filter_step")
            && matlab_syms.iter().any(|n| n.name.as_str() == "inner_gain"),
        "expected MATLAB functions in dsp/filter_step.m, got {matlab_syms:?}"
    );
    let matlab_ids: BTreeSet<uuid::Uuid> = matlab_syms.iter().map(|n| n.id).collect();
    assert!(
        edges.iter().any(|(f, t, et)| *et == EdgeType::Calls
            && matlab_ids.contains(f)
            && matlab_ids.contains(t)),
        "expected MATLAB Calls edge filter_step -> inner_gain"
    );
}

#[test]
fn all_5_config_format_plugins_emit_config_keys_uses_config_and_pass_invariants() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();

    write_file(
        repo,
        "config/application.yaml",
        "yaml:\n  server:\n    port: 8081\n",
    );
    write_file(
        repo,
        "config/appsettings.json",
        "{\n  \"json\": {\n    \"server\": {\n      \"port\": 8082\n    }\n  }\n}\n",
    );
    write_file(repo, "config/settings.toml", "[toml.server]\nport = 8083\n");
    write_file(
        repo,
        "config/application.properties",
        "props.server.port=8084\n",
    );
    write_file(
        repo,
        "config/config.xml",
        "<configuration><xml><server><port>8085</port></server></xml></configuration>\n",
    );
    write_file(
        repo,
        "src/AppConfigReader.java",
        r#"public class AppConfigReader {
    @Value("${yaml.server.port}")
    String yamlPort;
    @Value("${json.server.port}")
    String jsonPort;
    @Value("${toml.server.port}")
    String tomlPort;
    @Value("${props.server.port}")
    String propsPort;
    @Value("${configuration.xml.server.port}")
    String xmlPort;
}
"#,
    );

    let (nodes, edges) = run_pipeline_and_check_invariants(shared_default_registry(), repo);

    let config_keys: Vec<&Node> = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::ConfigKey)
        .collect();

    for (fmt, rel) in [
        ("yaml", "config/application.yaml"),
        ("json", "config/appsettings.json"),
        ("toml", "config/settings.toml"),
        ("properties", "config/application.properties"),
        ("xml", "config/config.xml"),
    ] {
        let fmt_keys: Vec<&&Node> = config_keys
            .iter()
            .filter(|n| {
                n.file_path
                    .as_deref()
                    .is_some_and(|p| p.replace('\\', "/").ends_with(rel))
            })
            .collect();
        assert!(
            !fmt_keys.is_empty(),
            "[{fmt}] expected ConfigKey nodes in {rel}, all config_keys={config_keys:?}"
        );
        assert!(
            fmt_keys.iter().all(|n| n.start_line.unwrap_or(0) >= 1),
            "[{fmt}] expected non-zero start_line on ConfigKey nodes in {rel}: {fmt_keys:?}"
        );
        let fmt_key_ids: BTreeSet<uuid::Uuid> = fmt_keys.iter().map(|n| n.id).collect();
        assert!(
            edges
                .iter()
                .any(|(_, to, et)| *et == EdgeType::UsesConfig && fmt_key_ids.contains(to)),
            "[{fmt}] expected EdgeType::UsesConfig targeting ConfigKey in {rel}; keys={fmt_keys:?}"
        );
    }
}
