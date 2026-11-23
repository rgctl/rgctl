//! Native predicate lowering must retain scoped def/use facts for the PDG.

use super::*;
use crate::pdg::ProgramDependenceGraph;
use crate::slicing::{BackwardSlicer, SliceCriterion};

const PROCEDURAL_CASES: &[(&str, &str, usize)] = &[
    (
        "rust",
        "fn example() {\n let enabled = true;\n if enabled { consume(); }\n}\n",
        2,
    ),
    (
        "javascript",
        "function example() {\n const enabled = true;\n if (enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "typescript",
        "function example() {\n const enabled: boolean = true;\n if (enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "python",
        "def example():\n enabled = True\n if enabled:\n  consume()\n",
        2,
    ),
    (
        "go",
        "package demo\nfunc example() {\n enabled := true\n if enabled { consume() }\n}\n",
        3,
    ),
    (
        "java",
        "class Demo {\n void example() {\n boolean enabled = true;\n if (enabled) { consume(); }\n }\n}\n",
        3,
    ),
    (
        "c",
        "void example() {\n int enabled = 1;\n if (enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "cpp",
        "void example() {\n bool enabled = true;\n if (enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "csharp",
        "class Demo { void example() {\n bool enabled = true;\n if (enabled) { consume(); }\n} }\n",
        2,
    ),
    (
        "groovy",
        "class Demo { void example() {\n def enabled = true;\n if (enabled) { consume(); }\n} }\n",
        2,
    ),
    (
        "kotlin",
        "fun example() {\n val enabled = true\n if (enabled) { consume() }\n}\n",
        2,
    ),
    (
        "php",
        "<?php function example() {\n $enabled = true;\n if ($enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "puppet",
        "function example() {\n $enabled = true\n if $enabled { notice('ok') }\n}\n",
        2,
    ),
    (
        "ruby",
        "def example\n enabled = true\n if enabled\n  consume()\n end\nend\n",
        2,
    ),
    (
        "swift",
        "func example() {\n var enabled = false\n enabled = true\n if enabled { consume() }\n}\n",
        3,
    ),
    (
        "scala",
        "object Demo { def example(): Unit = {\n var enabled = false\n enabled = true\n if (enabled) { consume() }\n} }\n",
        3,
    ),
    (
        "dart",
        "void example() {\n var enabled = true;\n if (enabled) { consume(); }\n}\n",
        2,
    ),
    (
        "elixir",
        "defmodule Demo do\n def example() do\n  enabled = true\n  if enabled do\n   consume()\n  end\n end\nend\n",
        3,
    ),
    (
        "lua",
        "function example()\n local enabled = true\n if enabled then consume() end\nend\n",
        2,
    ),
    (
        "bash",
        "example() {\n local enabled=1\n if [ \"$enabled\" = 1 ]; then echo ok; fi\n}\n",
        2,
    ),
];

fn assert_predicate_case((language, source, definition_line): (&str, &str, usize)) {
    let tree = crate::language_profile::parse_source(language, source.as_bytes()).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "{language}: invalid predicate fixture"
    );
    let function = "example";
    let cfg = build_cfg_for_function(language, source, function)
        .unwrap_or_else(|e| panic!("{language}: {e}"));
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    let predicate = pdg
        .nodes
        .values()
        .find(|n| {
            n.statement.kind == StatementKind::Branch
                && n.used_vars
                    .iter()
                    .any(|v| v.trim_start_matches('$').split('#').next() == Some("enabled"))
        })
        .unwrap_or_else(|| panic!("{language}: missing predicate input: {:?}", cfg.blocks));
    let variable = predicate
        .used_vars
        .iter()
        .find(|v| v.trim_start_matches('$').split('#').next() == Some("enabled"))
        .unwrap();
    assert!(
        predicate.used_vars.contains(variable),
        "{language}: predicate discarded its input"
    );
    assert!(
        pdg.data_deps.iter().any(|d| d.to == predicate.id
            && d.variable == *variable
            && pdg.nodes[&d.from].statement.line == definition_line),
        "{language}: missing definition → predicate edge: {:?}",
        cfg.blocks
    );
    let slice = BackwardSlicer::new(&pdg, &cfg)
        .slice(SliceCriterion {
            variable: variable.clone(),
            line: predicate.statement.line,
        })
        .unwrap();
    assert!(
        slice.lines.contains(&definition_line),
        "{language}: predicate slice omitted its definition"
    );
}

macro_rules! predicate_case {
    ($name:ident, $index:expr) => {
        #[test]
        fn $name() {
            assert_predicate_case(PROCEDURAL_CASES[$index]);
        }
    };
}
predicate_case!(predicate_rust, 0);
predicate_case!(predicate_javascript, 1);
predicate_case!(predicate_typescript, 2);
predicate_case!(predicate_python, 3);
predicate_case!(predicate_go, 4);
predicate_case!(predicate_java, 5);
predicate_case!(predicate_c, 6);
predicate_case!(predicate_cpp, 7);
predicate_case!(predicate_csharp, 8);
predicate_case!(predicate_groovy, 9);
predicate_case!(predicate_kotlin, 10);
predicate_case!(predicate_php, 11);
predicate_case!(predicate_puppet, 12);
predicate_case!(predicate_ruby, 13);
predicate_case!(predicate_swift, 14);
predicate_case!(predicate_scala, 15);
predicate_case!(predicate_dart, 16);
predicate_case!(predicate_elixir, 17);
predicate_case!(predicate_lua, 18);
predicate_case!(predicate_bash, 19);

#[test]
fn tier1_inventory_has_explicit_predicate_applicability() {
    let mut covered: HashSet<_> = PROCEDURAL_CASES.iter().map(|c| c.0).collect();
    assert_eq!(covered.len(), PROCEDURAL_CASES.len());
    for language in ["hcl", "sql", "erb", "markdown"] {
        assert!(covered.insert(language));
    }
    let bundled: HashSet<_> = rgctl_ast_coverage::bundled_specs()
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(covered, bundled, "classify every bundled Tier 1 language");
    for language in ["erb", "markdown"] {
        assert!(crate::language_profile::profile_for_language(language).is_none());
    }
}

#[test]
fn hcl_external_predicate_input_survives_cfg_pdg_and_serialization() {
    // Terraform attributes are declarative; `var.enabled` is an external input,
    // not a local variable defined by another attribute in this block.
    let source = "resource \"null_resource\" \"example\" {\n count = var.enabled ? 1 : 0\n}\n";
    let cfg = build_cfg_for_function("hcl", source, "resource.null_resource.example").unwrap();
    let restored: crate::cfg::ControlFlowGraph =
        bincode::deserialize(&bincode::serialize(&cfg).unwrap()).unwrap();
    let pdg = ProgramDependenceGraph::build(&restored, source.as_bytes()).unwrap();
    let predicate = pdg
        .nodes
        .values()
        .find(|n| n.statement.kind == StatementKind::Branch)
        .unwrap();
    assert!(predicate.statement.text.contains("var.enabled"));
    assert!(
        predicate
            .used_vars
            .iter()
            .any(|v| v == "var.enabled" || v == "enabled"),
        "{:?}",
        predicate.used_vars
    );
}

#[test]
fn sql_opaque_function_body_round_trips_without_invented_predicates() {
    // The SQL grammar leaves PL/pgSQL inside the quoted string opaque.
    // The function still participates in the procedural serialization revision.
    let source = "CREATE FUNCTION example() RETURNS INT LANGUAGE plpgsql AS 'BEGIN IF enabled THEN RETURN 1; END IF; RETURN 0; END;';";
    let tree = crate::language_profile::parse_source("sql", source.as_bytes()).unwrap();
    assert!(!tree.root_node().has_error());
    let cfg = build_cfg_for_function("sql", source, "example").unwrap();
    let restored: crate::cfg::ControlFlowGraph =
        bincode::deserialize(&bincode::serialize(&cfg).unwrap()).unwrap();
    assert_eq!(cfg.blocks.len(), restored.blocks.len());
    assert!(
        !restored
            .blocks
            .values()
            .flat_map(|b| &b.statements)
            .any(|s| s.kind == StatementKind::Branch)
    );
}

#[test]
fn short_circuit_operands_keep_separate_def_use_facts() {
    let source = "fn example() {\n let left = true;\n let right = true;\n if left && right { consume(); }\n}\n";
    let cfg = build_cfg_for_function("rust", source, "example").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    for (name, definition_line) in [("left", 2), ("right", 3)] {
        let predicate = pdg
            .nodes
            .values()
            .find(|n| n.statement.kind == StatementKind::Branch && n.statement.text == name)
            .unwrap();
        assert_eq!(predicate.used_vars, HashSet::from([name.to_string()]));
        assert!(pdg.data_deps.iter().any(|d| d.to == predicate.id
            && d.variable == name
            && pdg.nodes[&d.from].statement.line == definition_line));
    }
}

#[test]
fn shadowed_predicate_uses_the_inner_scoped_definition() {
    let source = "fn example() {\n let enabled = false;\n {\n let enabled = true;\n if enabled { consume(); }\n }\n}\n";
    let cfg = build_cfg_for_function("rust", source, "example").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    let predicate = pdg
        .nodes
        .values()
        .find(|n| n.statement.kind == StatementKind::Branch && n.statement.text == "enabled")
        .unwrap();
    assert!(
        predicate
            .used_vars
            .iter()
            .any(|v| v.starts_with("enabled#")),
        "predicate did not resolve lexical shadowing: {:?}",
        predicate.used_vars
    );
    let definitions: Vec<_> = pdg
        .data_deps
        .iter()
        .filter(|d| d.to == predicate.id && d.variable.starts_with("enabled"))
        .map(|d| pdg.nodes[&d.from].statement.line)
        .collect();
    assert_eq!(definitions, [4]);
}

#[test]
fn assignment_predicate_keeps_its_definition_and_rhs_use() {
    let source = "void example() {\n int enabled = 0;\n int next = 1;\n if ((enabled = next)) { consume(enabled); }\n}\n";
    let cfg = build_cfg_for_function("c", source, "example").unwrap();
    let predicate = cfg
        .blocks
        .values()
        .flat_map(|b| &b.statements)
        .find(|s| s.kind == StatementKind::Branch)
        .unwrap();
    assert!(
        predicate
            .defined_vars
            .iter()
            .any(|v| v.defines_name("enabled"))
    );
    assert!(predicate.used_vars.iter().any(|v| v == "next"));
}
