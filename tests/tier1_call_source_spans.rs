//! Call ownership must respect declaration columns across the complete Tier 1 inventory.
use rgctl_extraction::{Extractor, GraphBuilder};
use rgctl_graph::schema::{EdgeType, NodeType};
use rgctl_plugin_api::{RelationType, SymbolType, containing_function};
use std::{collections::BTreeSet, path::Path, sync::Arc};

struct Case {
    language: &'static str,
    file: &'static str,
    source: &'static str,
    /// SQL dollar-quoted bodies are opaque to its native grammar, so exercise
    /// declaration containment without claiming procedural call extraction.
    calls: bool,
}

const CASES: &[Case] = &[
    Case {
        language: "c",
        file: "span.c",
        source: "int left(void) { return right(); } int right(void) { return left(); }",
        calls: true,
    },
    Case {
        language: "cpp",
        file: "span.cpp",
        source: "int left() { return right(); } int right() { return left(); }",
        calls: true,
    },
    Case {
        language: "csharp",
        file: "Span.cs",
        source: "class Span { int left() { return right(); } int right() { return left(); } }",
        calls: true,
    },
    Case {
        language: "go",
        file: "span.go",
        source: "package span\nfunc left() int { return right() }; func right() int { return left() }",
        calls: true,
    },
    Case {
        language: "groovy",
        file: "span.groovy",
        source: "class Span { def left() { return right(); } def right() { return left(); } }",
        calls: true,
    },
    Case {
        language: "java",
        file: "Span.java",
        source: "class Span { int left() { return right(); } int right() { return left(); } }",
        calls: true,
    },
    Case {
        language: "javascript",
        file: "span.js",
        source: "function left() { return right(); } function right() { return left(); }",
        calls: true,
    },
    Case {
        language: "kotlin",
        file: "span.kt",
        source: "fun left(): Int { return right() }; fun right(): Int { return left() }",
        calls: true,
    },
    Case {
        language: "php",
        file: "span.php",
        source: "<?php function left() { return right(); } function right() { return left(); }",
        calls: true,
    },
    Case {
        language: "puppet",
        file: "span.pp",
        source: "function left() { right() } function right() { left() }",
        calls: true,
    },
    Case {
        language: "python",
        file: "span.py",
        source: "def left(): return right()\ndef right(): return left()\n",
        calls: true,
    },
    Case {
        language: "ruby",
        file: "span.rb",
        source: "def left(); right(); end; def right(); left(); end",
        calls: true,
    },
    Case {
        language: "rust",
        file: "span.rs",
        source: "fn left() { right(); } fn right() { left(); }",
        calls: true,
    },
    Case {
        language: "typescript",
        file: "span.ts",
        source: "function left(): number { return right(); } function right(): number { return left(); }",
        calls: true,
    },
    Case {
        language: "swift",
        file: "span.swift",
        source: "func left() -> Int { return right() }; func right() -> Int { return left() }",
        calls: true,
    },
    Case {
        language: "scala",
        file: "span.scala",
        source: "object Span { def left(): Int = { right() }; def right(): Int = { left() } }",
        calls: true,
    },
    Case {
        language: "dart",
        file: "span.dart",
        source: "int left() { return right(); } int right() { return left(); }",
        calls: true,
    },
    Case {
        language: "elixir",
        file: "span.ex",
        source: "defmodule Span do; def left(), do: right(); def right(), do: left(); end",
        calls: true,
    },
    Case {
        language: "lua",
        file: "span.lua",
        source: "function left() return right() end function right() return left() end",
        calls: true,
    },
    Case {
        language: "bash",
        file: "span.sh",
        source: "left() { right; }; right() { left; }",
        calls: true,
    },
    Case {
        language: "sql",
        file: "span.sql",
        source: "CREATE FUNCTION left() RETURNS INT AS $$ SELECT 401; $$ LANGUAGE sql; CREATE FUNCTION right() RETURNS INT AS $$ SELECT 402; $$ LANGUAGE sql;",
        calls: false,
    },
    Case {
        language: "hcl",
        file: "span.tf",
        source: "variable \"left\" { default = 401 }\nvariable \"right\" { default = 402 }",
        calls: false,
    },
    Case {
        language: "erb",
        file: "span.erb",
        source: "<%= left(401) %><%= right(402) %>",
        calls: false,
    },
    Case {
        language: "markdown",
        file: "span.md",
        source: "# Left\n\n401\n\n# Right\n\n402\n",
        calls: false,
    },
];

#[test]
fn inventory_covers_every_tier1_language() {
    let actual: BTreeSet<_> = CASES.iter().map(|c| c.language).collect();
    let expected: BTreeSet<_> = rgctl_ast_coverage::bundled_specs()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), CASES.len(), "duplicate language case");
}

#[test]
fn nested_same_boundary_lines_and_outside_nodes_are_order_independent() {
    let registry = rgctl_languages::default_registry();
    let plugin = registry.get_language_plugin("rust").unwrap();
    let source = "fn outer() { fn inner() {\n    ping(); } }\nfn sibling() { pong(); }\n";
    let all = plugin
        .extract_all(Path::new("nested.rs"), source.as_bytes())
        .unwrap();
    let functions: Vec<_> = all
        .symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .collect();
    assert_eq!(functions.len(), 3);
    let tree =
        rgctl_plugin_api::parse_with_pool("rust", plugin.grammar().unwrap(), source.as_bytes())
            .unwrap();
    for (probe, owner) in [("ping()", "inner"), ("pong()", "sibling")] {
        let start = source.find(probe).unwrap();
        let node = tree
            .root_node()
            .descendant_for_byte_range(start, start + probe.len())
            .unwrap();
        for reversed in [false, true] {
            let mut candidates = functions.clone();
            if reversed {
                candidates.reverse();
            }
            assert_eq!(containing_function(node, &candidates).unwrap().name, owner);
        }
    }
    // The root starts at the same point as outer but extends beyond its end.
    assert!(containing_function(tree.root_node(), &functions).is_none());
}

#[test]
fn native_declaration_spans_select_the_correct_owner() {
    let registry = rgctl_languages::default_registry();
    let mut failures = Vec::new();
    for case in CASES {
        let plugin = registry.get_language_plugin(case.language).unwrap();
        let all = plugin
            .extract_all(Path::new(case.file), case.source.as_bytes())
            .unwrap();
        let functions: Vec<_> = all
            .symbols
            .iter()
            .filter(|s| matches!(s.symbol_type, SymbolType::Function | SymbolType::Method))
            .collect();
        if matches!(case.language, "hcl" | "erb" | "markdown") {
            assert!(
                functions.is_empty(),
                "{} has no source-written callable declarations",
                case.language
            );
            continue;
        }
        assert_eq!(functions.len(), 2, "{}: {functions:?}", case.language);
        let tree = rgctl_plugin_api::parse_with_pool(
            case.language,
            plugin.grammar().unwrap(),
            case.source.as_bytes(),
        )
        .unwrap();
        assert!(
            !tree.root_node().has_error(),
            "{} fixture must parse: {}",
            case.language,
            tree.root_node().to_sexp()
        );
        let probes = if case.language == "sql" {
            [("401", "left"), ("402", "right")]
        } else if case.language == "bash" {
            [("right;", "left"), ("left;", "right")]
        } else {
            [("right()", "left"), ("left()", "right")]
        };
        for (index, (probe, owner)) in probes.into_iter().enumerate() {
            let start = if index == 0 {
                case.source.find(probe)
            } else {
                case.source.rfind(probe)
            }
            .unwrap();
            let node = tree
                .root_node()
                .descendant_for_byte_range(start, start + probe.len())
                .unwrap();
            // Symbol order must not decide which same-line declaration owns a node.
            for reversed in [false, true] {
                let mut candidates = functions.clone();
                if reversed {
                    candidates.reverse();
                }
                let actual = containing_function(node, &candidates).map(|s| s.name.as_str());
                if actual != Some(owner) {
                    failures.push(format!(
                        "{} {probe}: expected {owner}, got {actual:?}; reversed={reversed}",
                        case.language
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn native_calls_and_materialized_graphs_keep_sibling_owners_separate() {
    let registry = Arc::new(rgctl_languages::default_registry());
    let extractor = Extractor::new(registry.clone());
    let mut failures = Vec::new();
    for case in CASES.iter().filter(|c| c.calls) {
        let plugin = registry.get_language_plugin(case.language).unwrap();
        let all = plugin
            .extract_all(Path::new(case.file), case.source.as_bytes())
            .unwrap();
        // The older plugins also expose the split API. Newer plugins explicitly
        // implement only extract_all; do not call their unimplemented methods.
        let split = if matches!(
            case.language,
            "swift" | "scala" | "dart" | "elixir" | "lua" | "bash"
        ) {
            None
        } else {
            let symbols = plugin
                .extract_symbols(Path::new(case.file), case.source.as_bytes())
                .unwrap();
            let relations = plugin
                .extract_relations(Path::new(case.file), case.source.as_bytes(), &symbols)
                .unwrap();
            Some((symbols, relations))
        };
        let mut extractions = vec![(&all.symbols, &all.relations)];
        if let Some((symbols, relations)) = &split {
            extractions.push((symbols, relations));
        }
        for (symbols, relations) in extractions {
            for (owner, target) in [("left", "right"), ("right", "left")] {
                let symbol = symbols.iter().find(|s| s.name == owner).unwrap();
                let target_symbol = symbols.iter().find(|s| s.name == target).unwrap();
                let matches = |endpoint: &str, symbol: &rgctl_plugin_api::Symbol| {
                    endpoint == symbol.name || symbol.qualified_name.as_deref() == Some(endpoint)
                };
                if !relations.iter().any(|r| {
                    r.relation_type == RelationType::Calls
                        && matches(&r.from, symbol)
                        && matches(&r.to, target_symbol)
                }) {
                    failures.push(format!(
                        "{}: missing native {owner} -> {target}; {:?}",
                        case.language, relations
                    ));
                }
                if relations.iter().any(|r| {
                    r.relation_type == RelationType::Calls
                        && matches(&r.from, symbol)
                        && matches(&r.to, symbol)
                }) {
                    failures.push(format!("{}: wrong self-call on {owner}", case.language));
                }
            }
        }
        let file = extractor
            .extract_file_with_source(Path::new(case.file), case.source.as_bytes().to_vec())
            .unwrap();
        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&[file], &mut builder).unwrap();
        let (nodes, edges) = builder.into_graph();
        let id = |name: &str| {
            nodes
                .iter()
                .find(|n| n.node_type == NodeType::Function && n.name == name)
                .unwrap()
                .id
        };
        for (owner, target) in [("left", "right"), ("right", "left")] {
            if !edges.iter().any(|e| {
                e.edge_type == EdgeType::Calls && e.from == id(owner) && e.to == id(target)
            }) {
                failures.push(format!(
                    "{}: missing graph {owner} -> {target}",
                    case.language
                ));
            }
            if edges
                .iter()
                .any(|e| e.edge_type == EdgeType::Calls && e.from == id(owner) && e.to == id(owner))
            {
                failures.push(format!(
                    "{}: wrong graph self-call on {owner}",
                    case.language
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
