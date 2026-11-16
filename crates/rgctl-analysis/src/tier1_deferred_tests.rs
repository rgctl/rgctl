//! Deferred fixtures use the same native facts and public queries as consumers.
use crate::call_bindings::{CallRef, InvocationTarget, ReceiverSupply, ValueRef};
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use crate::interprocedural_cfg::InterproceduralCFG;
use crate::operands::*;
use crate::value_origins::{OriginAnalysis, OriginLimits, OriginSeed};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use uuid::Uuid;

fn deferred_cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_deferred_cases.json"
    ))
    .unwrap()
}
fn deferred_source(case: &serde_json::Value) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(case["file"].as_str().unwrap())).unwrap()
}
struct DeferredFixture {
    icfg: InterproceduralCFG,
    ids: Vec<Uuid>,
}
impl DeferredFixture {
    fn captured(language: &str, source: &str, names: &[&str]) -> Self {
        let mut backend = MemoryBackend::new();
        let mut cfgs = HashMap::new();
        let ids: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let mut node = Node::new(NodeType::Function, *name);
                node.id = Uuid::from_u128(100 + i as u128);
                backend.insert_node(node.clone()).unwrap();
                let cfg = build_cfg_for_function(language, source, name)
                    .unwrap_or_else(|e| panic!("{language} {name}: {e}"));
                assert!(
                    cfg.operands.valid_references(),
                    "{language} {name}: invalid operands"
                );
                cfgs.insert(node.id, cfg);
                node.id
            })
            .collect();
        let icfg = InterproceduralCFG::from_cfg_archive(&backend, cfgs).unwrap();
        Self { icfg, ids }
    }
    fn deferred_cfg(&self, index: usize) -> &ControlFlowGraph {
        self.icfg.get_cfg(self.ids[index]).unwrap()
    }
    fn deferred_target(&self, caller: usize, name: &str, callee: usize) -> InvocationTarget {
        let cfg = self.deferred_cfg(caller);
        let site = cfg
            .operands
            .calls
            .iter()
            .position(|c| {
                matches!(
                    cfg.operands.expressions[c.callee.0 as usize].kind,
                    ExpressionKind::Name(n) | ExpressionKind::Member(n)
                        if cfg.operands.operand_name(n) == Some(name)
                )
            })
            .unwrap_or_else(|| panic!("missing {name}: {:#?}", cfg.operands));
        InvocationTarget {
            call: CallRef {
                caller: self.ids[caller],
                site: CallSiteId(site as u32),
            },
            callee: self.ids[callee],
            receiver: ReceiverSupply::None,
        }
    }
    fn deferred_seed(&self, index: usize, formal: usize, tag: u128) -> OriginSeed {
        OriginSeed {
            value: ValueRef {
                function: self.ids[index],
                expression: self.deferred_cfg(index).operands.formals[formal].value,
            },
            type_id: Some(Uuid::from_u128(tag)),
        }
    }
    fn returned_types(&self, result: &OriginAnalysis, index: usize) -> BTreeSet<Option<Uuid>> {
        self.deferred_cfg(index)
            .operands
            .returns
            .iter()
            .flat_map(|r| {
                r.values
                    .slice(&self.deferred_cfg(index).operands.inputs)
                    .unwrap()
            })
            .flat_map(|&expression| {
                result.origins(ValueRef {
                    function: self.ids[index],
                    expression,
                })
            })
            .map(|o| result.seeds[o.0 as usize].type_id)
            .collect()
    }
}
#[test]
fn deferred_inventory_covers_every_procedural_tier1_language() {
    let ordinary: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_operand_cases.json"
    ))
    .unwrap();
    assert_eq!(
        deferred_cases()
            .iter()
            .map(|c| c["language"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>(),
        ordinary
            .iter()
            .map(|c| c["language"].as_str().unwrap().to_owned())
            .collect()
    );
}
#[test]
fn native_awaits_and_suspend_calls_keep_factory_origins_across_tier1() {
    let mut failures = Vec::new();
    for case in deferred_cases() {
        if case["await_mode"] == "not_native" {
            continue;
        }
        let language = case["language"].as_str().unwrap();
        let source = deferred_source(&case);
        let f = DeferredFixture::captured(
            language,
            &source,
            &["operand_await", "operand_deferred_source"],
        );
        let seed = f.deferred_seed(0, 0, 1000);
        let target = f.deferred_target(0, "operand_deferred_source", 1);
        let result =
            OriginAnalysis::build(&f.icfg, &[seed], &[target], &[], OriginLimits::default());
        let awaits = f
            .deferred_cfg(0)
            .operands
            .expressions
            .iter()
            .filter(|e| e.kind == ExpressionKind::AwaitedValue)
            .count();
        if f.returned_types(&result, 0) != [seed.type_id].into_iter().collect()
            || (case["await_mode"] == "native" && awaits != 1)
        {
            let tree = crate::language_profile::parse_source(language, source.as_bytes()).unwrap();
            failures.push(format!(
                "{language}: await count {awaits}; {result:#?}\n{:#?}\n{}",
                f.deferred_cfg(0).operands,
                tree.root_node().to_sexp()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn native_generator_readers_keep_containers_yields_and_terminal_returns_distinct() {
    let mut failures = Vec::new();
    for case in deferred_cases() {
        if case["generator_mode"] == "not_modeled" {
            continue;
        }
        let language = case["language"].as_str().unwrap();
        let source = deferred_source(&case);
        let f =
            DeferredFixture::captured(language, &source, &["operand_generator", "operand_yield"]);
        let seeds = [f.deferred_seed(0, 0, 1000), f.deferred_seed(0, 1, 2000)];
        let target = f.deferred_target(0, "operand_yield", 1);
        let result =
            OriginAnalysis::build(&f.icfg, &seeds, &[target], &[], OriginLimits::default());
        let call_result = f.deferred_cfg(0).operands.calls[target.call.site.0 as usize].result;
        let factory_types: BTreeSet<_> = result
            .origins(ValueRef {
                function: f.ids[0],
                expression: call_result,
            })
            .iter()
            .map(|o| result.seeds[o.0 as usize].type_id)
            .collect();
        let expected: BTreeSet<_> = if case["generator_mode"] == "opaque_only" {
            [None].into_iter().collect()
        } else if matches!(language, "javascript" | "typescript") {
            [seeds[0].type_id, seeds[1].type_id].into_iter().collect()
        } else {
            [seeds[0].type_id].into_iter().collect()
        };
        if factory_types != [None].into_iter().collect()
            || !f.deferred_cfg(1).operands.generator
            || f.deferred_cfg(1).operands.yields.len() != 1
            || f.returned_types(&result, 0) != expected
        {
            let tree = crate::language_profile::parse_source(language, source.as_bytes()).unwrap();
            failures.push(format!("{language}: factory {factory_types:?}, expected {expected:?}; {result:#?}\n{:#?}\n{}",f.deferred_cfg(1).operands,tree.root_node().to_sexp()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn go_channel_handoff_keeps_comma_ok_boolean_separate() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "go")
        .unwrap();
    let source = deferred_source(&case);
    let f = DeferredFixture::captured("go", &source, &["operand_channel", "operand_channel_send"]);
    let seed = f.deferred_seed(0, 0, 1000);
    let target = f.deferred_target(0, "operand_channel_send", 1);
    let result = OriginAnalysis::build(&f.icfg, &[seed], &[target], &[], OriginLimits::default());
    assert_eq!(
        f.returned_types(&result, 0),
        [seed.type_id].into_iter().collect(),
        "{result:#?}"
    );
    let cfg = f.deferred_cfg(0);
    for (i, e) in cfg.operands.expressions.iter().enumerate() {
        if e.kind == (ExpressionKind::Projection { position: 1 }) {
            assert!(
                result
                    .origins(ValueRef {
                        function: f.ids[0],
                        expression: ExpressionId(i as u32)
                    })
                    .is_empty(),
                "boolean acquired element: {result:#?}"
            );
        }
    }
}

#[test]
fn python_runners_async_generators_and_delegated_boundaries_are_explicit() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "python")
        .unwrap();
    let source = deferred_source(&case);
    for (caller, callee) in [
        ("operand_deferred_runner", "operand_deferred_source"),
        ("operand_async_generator", "operand_async_yield"),
    ] {
        let f = DeferredFixture::captured("python", &source, &[caller, callee]);
        let seed = f.deferred_seed(0, 0, 1000);
        let target = f.deferred_target(0, callee, 1);
        let result =
            OriginAnalysis::build(&f.icfg, &[seed], &[target], &[], OriginLimits::default());
        assert_eq!(
            f.returned_types(&result, 0),
            [seed.type_id].into_iter().collect(),
            "{caller}: {result:#?}"
        );
    }
    let f = DeferredFixture::captured(
        "python",
        &source,
        &["operand_delegated_factory", "operand_delegated"],
    );
    let seeds = [f.deferred_seed(0, 0, 1000), f.deferred_seed(0, 1, 2000)];
    let target = f.deferred_target(0, "operand_delegated", 1);
    let result = OriginAnalysis::build(&f.icfg, &seeds, &[target], &[], OriginLimits::default());
    assert!(f.deferred_cfg(1).operands.generator);
    assert!(
        f.deferred_cfg(1)
            .operands
            .diagnostics
            .iter()
            .any(|d| d.kind == OperandDiagnosticKind::ReturnConvention)
    );
    assert_eq!(
        f.returned_types(&result, 0),
        [None].into_iter().collect(),
        "terminal return escaped delegated generator: {result:#?}"
    );
    let f = DeferredFixture::captured("python", &source, &["operand_not_generator"]);
    let seed = f.deferred_seed(0, 0, 1000);
    let result = OriginAnalysis::build(&f.icfg, &[seed], &[], &[], OriginLimits::default());
    assert!(
        f.returned_types(&result, 0).is_empty(),
        "ordinary list elements must not alias generator yielded slots: {result:#?}"
    );
}

#[test]
fn php_terminal_values_use_get_return_without_leaking_into_current() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "php")
        .unwrap();
    let source = deferred_source(&case);
    let f = DeferredFixture::captured(
        "php",
        &source,
        &["operand_generator_return", "operand_yield"],
    );
    let seeds = [f.deferred_seed(0, 0, 1000), f.deferred_seed(0, 1, 2000)];
    let target = f.deferred_target(0, "operand_yield", 1);
    let result = OriginAnalysis::build(&f.icfg, &seeds, &[target], &[], OriginLimits::default());
    assert_eq!(
        f.returned_types(&result, 0),
        [seeds[1].type_id].into_iter().collect(),
        "{result:#?}"
    );
}

#[test]
fn go_distinct_channel_allocations_do_not_merge_elements() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "go")
        .unwrap();
    let source = deferred_source(&case);
    let f = DeferredFixture::captured("go", &source, &["operand_channel_isolation"]);
    let seeds = [f.deferred_seed(0, 0, 1000), f.deferred_seed(0, 1, 2000)];
    let result = OriginAnalysis::build(&f.icfg, &seeds, &[], &[], OriginLimits::default());
    assert_eq!(
        f.returned_types(&result, 0),
        [seeds[0].type_id].into_iter().collect(),
        "{result:#?}"
    );
    for call in &f.deferred_cfg(0).operands.calls {
        assert!(
            result
                .origins(ValueRef {
                    function: f.ids[0],
                    expression: call.result
                })
                .iter()
                .all(|o| result.seeds[o.0 as usize].type_id.is_none()),
            "channel acquired element type: {result:#?}"
        );
    }
}

#[test]
fn empty_dart_generators_retain_identity_and_yield_references_round_trip() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "dart")
        .unwrap();
    let source = deferred_source(&case);
    let f = DeferredFixture::captured(
        "dart",
        &source,
        &["operand_empty_factory", "operand_empty_generator"],
    );
    let seed = f.deferred_seed(0, 0, 1000);
    let target = f.deferred_target(0, "operand_empty_generator", 1);
    let result = OriginAnalysis::build(&f.icfg, &[seed], &[target], &[], OriginLimits::default());
    assert!(
        f.deferred_cfg(1).operands.generator,
        "{:#?}",
        f.deferred_cfg(1).operands
    );
    assert_eq!(
        f.returned_types(&result, 0),
        [None].into_iter().collect(),
        "{result:#?}"
    );
    let f = DeferredFixture::captured("dart", &source, &["operand_yield"]);
    let mut cfg: ControlFlowGraph =
        bincode::deserialize(&bincode::serialize(f.deferred_cfg(0)).unwrap()).unwrap();
    assert_eq!(cfg.operands, f.deferred_cfg(0).operands);
    cfg.operands.yields[0] = ExpressionId(u32::MAX);
    assert!(!cfg.operands.valid_references());
}

#[test]
fn ecma_empty_and_delegated_generators_keep_container_identity_and_query_limits() {
    let case = deferred_cases()
        .into_iter()
        .find(|c| c["language"] == "javascript")
        .unwrap();
    let source = deferred_source(&case);
    let f = DeferredFixture::captured(
        "javascript",
        &source,
        &["operand_empty_factory", "operand_empty_generator"],
    );
    let seed = f.deferred_seed(0, 0, 2000);
    let target = f.deferred_target(0, "operand_empty_generator", 1);
    let result = OriginAnalysis::build(&f.icfg, &[seed], &[target], &[], OriginLimits::default());
    assert!(f.deferred_cfg(1).operands.generator);
    assert_eq!(
        f.returned_types(&result, 0),
        [None].into_iter().collect(),
        "{result:#?}"
    );
    let f = DeferredFixture::captured(
        "javascript",
        &source,
        &["operand_delegated_factory", "operand_delegated"],
    );
    let seeds = [f.deferred_seed(0, 0, 1000), f.deferred_seed(0, 1, 2000)];
    let target = f.deferred_target(0, "operand_delegated", 1);
    let result = OriginAnalysis::build(&f.icfg, &seeds, &[target], &[], OriginLimits::default());
    assert!(
        f.deferred_cfg(1)
            .operands
            .diagnostics
            .iter()
            .any(|d| d.kind == OperandDiagnosticKind::ReturnConvention)
    );
    assert_eq!(
        f.returned_types(&result, 0),
        [None].into_iter().collect(),
        "{result:#?}"
    );
    let limits = OriginLimits {
        max_steps: 1,
        ..OriginLimits::default()
    };
    let limited = OriginAnalysis::build(&f.icfg, &seeds, &[target], &[], limits);
    assert!(
        limited
            .diagnostics
            .iter()
            .any(|d| matches!(d, crate::value_origins::OriginDiagnostic::LimitReached)),
        "{limited:#?}"
    );
}
