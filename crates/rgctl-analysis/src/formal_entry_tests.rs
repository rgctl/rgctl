//! Formal entry definitions must participate in the existing local PDG.
use crate::cfg::{ControlFlowGraph, StatementKind};
use crate::cfg_builder::build_cfg_for_function;
use crate::operands::{FormalId, FormalKind};
use crate::pdg::{DataDepType, ProgramDependenceGraph};
use std::collections::HashSet;

fn build(language: &str, source: &str) -> (ControlFlowGraph, ProgramDependenceGraph) {
    let cfg = build_cfg_for_function(language, source, "consume").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    (cfg, pdg)
}

#[test]
fn six_language_formals_feed_local_aliases_and_returns() {
    for (language, source) in [
        (
            "java",
            "class Demo { static int consume(int value) { int alias = value; return alias; } }",
        ),
        (
            "javascript",
            "function consume(value) { const alias = value; return alias; }",
        ),
        (
            "typescript",
            "function consume(value: number) { const alias = value; return alias; }",
        ),
        (
            "python",
            "def consume(value):\n alias = value\n return alias\n",
        ),
        (
            "go",
            "package demo\nfunc consume(value int) int { alias := value; return alias }",
        ),
        (
            "rust",
            "fn consume(value: i32) -> i32 { let alias = value; return alias; }",
        ),
    ] {
        let (cfg, pdg) = build(language, source);
        let definitions: Vec<_> = pdg.formal_definitions().collect();
        assert_eq!(definitions.len(), 1, "{language}");
        let (formal, entry) = definitions[0];
        assert_eq!(formal, FormalId(0));
        assert!(entry.used_vars.is_empty());
        assert!(entry.statement.defines("value"));
        assert_eq!(
            cfg.operands.formals[formal.0 as usize].kind,
            FormalKind::Positional(0)
        );
        let alias = pdg
            .nodes
            .values()
            .find(|n| n.statement.defines("alias"))
            .unwrap();
        let returned = pdg
            .nodes
            .values()
            .find(|n| n.statement.kind == StatementKind::Return)
            .unwrap();
        assert!(
            pdg.data_deps
                .iter()
                .any(|d| d.dep_type == DataDepType::Flow && d.from == entry.id && d.to == alias.id),
            "{language}: entry -> alias"
        );
        assert!(
            pdg.data_deps.iter().any(|d| d.dep_type == DataDepType::Flow
                && d.from == alias.id
                && d.to == returned.id),
            "{language}: alias -> return"
        );
    }
}

#[test]
fn shadowing_parameter_preserves_distinct_inner_and_outer_definitions() {
    for (language, source) in [
        (
            "javascript",
            "function consume(value) { { let value = 2; use(value); } return value; }",
        ),
        (
            "typescript",
            "function consume(value: number) { { let value = 2; use(value); } return value; }",
        ),
        (
            "rust",
            "fn consume(value: i32) -> i32 { { let value = 2; use_value(value); } return value; }",
        ),
    ] {
        let (_, pdg) = build(language, source);
        let (_, entry) = pdg.formal_definitions().next().unwrap();
        let returned = pdg
            .nodes
            .values()
            .find(|n| n.statement.kind == StatementKind::Return)
            .unwrap();
        let inner = pdg
            .nodes
            .values()
            .find(|n| n.statement.kind == StatementKind::Declaration)
            .unwrap();
        assert_ne!(entry.defined_vars, inner.defined_vars, "{language}");
        assert!(
            pdg.data_deps.iter().any(|d| d.dep_type == DataDepType::Flow
                && d.from == entry.id
                && d.to == returned.id),
            "{language}: return uses entry"
        );
        assert!(
            !pdg.data_deps.iter().any(|d| d.dep_type == DataDepType::Flow
                && d.from == inner.id
                && d.to == returned.id),
            "{language}: inner must not escape"
        );
    }
}

#[test]
fn reassignments_and_branch_merges_keep_required_possible_origins() {
    let (_, overwritten) = build(
        "javascript",
        "function consume(value) { value = 2; return value; }",
    );
    let returned = overwritten
        .nodes
        .values()
        .find(|n| n.statement.kind == StatementKind::Return)
        .unwrap();
    let assigned = overwritten
        .nodes
        .values()
        .find(|n| {
            n.statement.kind != StatementKind::Parameter(FormalId(0))
                && n.statement.defines("value")
        })
        .unwrap();
    assert!(
        overwritten
            .data_deps
            .iter()
            .any(|d| d.dep_type == DataDepType::Flow
                && d.from == assigned.id
                && d.to == returned.id)
    );
    // The existing statement PDG conservatively retains earlier same-block
    // definitions as well. This commit does not qualify dead-definition exclusion.
    let (_, merged) = build(
        "javascript",
        "function consume(value, enabled) { if (enabled) { value = 2; } return value; }",
    );
    let entry = merged
        .formal_definitions()
        .find(|(id, _)| *id == FormalId(0))
        .unwrap()
        .1;
    let returned = merged
        .nodes
        .values()
        .find(|n| n.statement.kind == StatementKind::Return)
        .unwrap();
    let incoming: HashSet<_> = merged
        .data_deps
        .iter()
        .filter(|d| d.dep_type == DataDepType::Flow && d.to == returned.id && d.variable == "value")
        .map(|d| d.from)
        .collect();
    assert!(incoming.contains(&entry.id));
    assert_eq!(incoming.len(), 2);
}

#[test]
fn receiver_formals_and_defaults_are_entry_definitions_not_assignments() {
    for (language, source) in [
        ("java", "class Demo { Object consume() { return this; } }"),
        ("javascript", "class Demo { consume() { return this; } }"),
        ("typescript", "class Demo { consume() { return this; } }"),
        (
            "python",
            "class Demo:\n def consume(self):\n  return self\n",
        ),
        (
            "go",
            "package demo\ntype Demo struct {}\nfunc (self Demo) consume() Demo { return self }",
        ),
        (
            "rust",
            "impl Demo { fn consume(&self) -> &Self { return self; } }",
        ),
    ] {
        let (cfg, pdg) = build(language, source);
        let (id, entry) = pdg.formal_definitions().next().unwrap();
        assert_eq!(id, FormalId(0), "{language}");
        assert_eq!(cfg.operands.formals.len(), 1, "{language}");
        let returned = pdg
            .nodes
            .values()
            .find(|n| n.statement.kind == StatementKind::Return)
            .unwrap();
        assert!(
            pdg.data_deps.iter().any(|d| d.dep_type == DataDepType::Flow
                && d.from == entry.id
                && d.to == returned.id),
            "{language}: receiver read"
        );
    }
    let (_, pdg) = build(
        "python",
        "def consume(value=make_default()):\n return value\n",
    );
    assert!(
        pdg.formal_definitions()
            .all(|(_, n)| n.used_vars.is_empty())
    );
}

#[test]
fn formal_ids_survive_cfg_and_pdg_archive_round_trip() {
    let (cfg, pdg) = build(
        "javascript",
        "function consume(first, second) { const alias = second; return alias; }",
    );
    let cfg: ControlFlowGraph = bincode::deserialize(&bincode::serialize(&cfg).unwrap()).unwrap();
    let restored: ProgramDependenceGraph =
        bincode::deserialize(&bincode::serialize(&pdg).unwrap()).unwrap();
    let rebuilt = ProgramDependenceGraph::build(&cfg, b"").unwrap();
    for graph in [&restored, &rebuilt] {
        let defs: Vec<_> = graph.formal_definitions().collect();
        assert_eq!(defs.len(), cfg.operands.formals.len());
        for (id, node) in defs {
            let formal = &cfg.operands.formals[id.0 as usize];
            assert_eq!(
                node.statement.text,
                cfg.operands.operand_name(formal.name).unwrap()
            );
            assert!(pdg.nodes.contains_key(&node.id));
        }
        assert_eq!(graph.data_deps.len(), pdg.data_deps.len());
    }
}
