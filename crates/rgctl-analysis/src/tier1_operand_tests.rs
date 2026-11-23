//! Canonical language correctness fixtures must compose local and cross-call flow.
use crate::call_bindings::{CallRef, InvocationTarget, ReceiverSupply, ValueRef};
use crate::cfg_builder::build_cfg_for_function;
use crate::interprocedural_cfg::InterproceduralCFG;
use crate::operands::{CallSiteId, ExpressionKind, FormalKind, OperandStatus};
use crate::value_origins::{OriginAnalysis, OriginLimits, OriginSeed};
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

fn check(language: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_operand_cases.json"
    ))
    .unwrap();
    let case = manifest
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["language"] == language)
        .unwrap();
    let source = std::fs::read_to_string(root.join(case["file"].as_str().unwrap())).unwrap();
    let caller_cfg = build_cfg_for_function(language, &source, "operand_relay").unwrap();
    let callee_cfg = build_cfg_for_function(language, &source, "operand_sink").unwrap();
    if !matches!(language, "c" | "bash" | "puppet" | "elixir") {
        let member = build_cfg_for_function(language, &source, "operand_member").unwrap();
        assert!(
            member.operands.calls.iter().any(|c| c.receiver.is_some()),
            "{language}: missing native member receiver: {:#?}",
            member.operands
        );
    }

    if source.contains("operand_instance") {
        let cfg = build_cfg_for_function(
            language,
            &source,
            if language == "lua" {
                "OperandSelf:operand_instance"
            } else {
                "operand_instance"
            },
        )
        .unwrap();
        let formal = cfg
            .operands
            .formals
            .iter()
            .find(|f| f.kind == FormalKind::Receiver)
            .unwrap_or_else(|| {
                panic!("{language}: missing implicit receiver: {:#?}", cfg.operands)
            });
        let receiver = cfg
            .operands
            .calls
            .iter()
            .find_map(|c| c.receiver)
            .unwrap_or_else(|| {
                panic!(
                    "{language}: missing self/this invocation receiver: {:#?}",
                    cfg.operands
                )
            });
        let value = formal.value;
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Function, "operand_instance");
        node.id = Uuid::from_u128(4);
        backend.insert_node(node.clone()).unwrap();
        let icfg = InterproceduralCFG::from_cfg_archive(&backend, HashMap::from([(node.id, cfg)]))
            .unwrap();
        let seed = OriginSeed {
            value: ValueRef {
                function: node.id,
                expression: value,
            },
            type_id: Uuid::from_u128(5),
        };
        let result = OriginAnalysis::build(&icfg, &[seed], &[], &[], OriginLimits::default());
        assert!(
            !result
                .origins(ValueRef {
                    function: node.id,
                    expression: receiver
                })
                .is_empty(),
            "{language}: receiver origin lost: {result:#?}"
        );
    }
    if source.contains("operand_choice") {
        let cfg = build_cfg_for_function(language, &source, "operand_choice").unwrap();
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Function, "operand_choice");
        node.id = Uuid::from_u128(6);
        backend.insert_node(node.clone()).unwrap();
        let seeds: Vec<_> = cfg
            .operands
            .formals
            .iter()
            .enumerate()
            .map(|(i, f)| OriginSeed {
                value: ValueRef {
                    function: node.id,
                    expression: f.value,
                },
                type_id: Uuid::from_u128(10 + i as u128),
            })
            .collect();
        assert_eq!(seeds.len(), 3, "{language}: choice formals");
        let returns: Vec<_> = cfg
            .operands
            .returns
            .iter()
            .flat_map(|r| r.values.slice(&cfg.operands.inputs).unwrap())
            .copied()
            .collect();
        let icfg = InterproceduralCFG::from_cfg_archive(&backend, HashMap::from([(node.id, cfg)]))
            .unwrap();
        let result = OriginAnalysis::build(&icfg, &seeds, &[], &[], OriginLimits::default());
        let types: std::collections::BTreeSet<_> = returns
            .into_iter()
            .flat_map(|expression| {
                result.origins(ValueRef {
                    function: node.id,
                    expression,
                })
            })
            .map(|origin| result.seeds[origin.0 as usize].type_id)
            .collect();
        assert_eq!(
            types,
            [Uuid::from_u128(11), Uuid::from_u128(12)]
                .into_iter()
                .collect(),
            "{language}: condition must not become a selected result: {result:#?}"
        );
    }
    let facts = &caller_cfg.operands;
    assert_eq!(facts.status, OperandStatus::Captured, "{language}");
    assert!(facts.valid_references());
    let formal = facts
        .formals
        .iter()
        .find(|f| f.kind == FormalKind::Positional(0))
        .unwrap_or_else(|| panic!("{language}: missing formal: {facts:#?}"));
    let site = facts.calls.iter().position(|c| matches!(facts.expressions[c.callee.0 as usize].kind, ExpressionKind::Name(n) if facts.name(n) == Some("operand_sink"))).unwrap_or_else(|| panic!("{language}: missing call: {facts:#?}"));
    let seed_expression = formal.value;
    let result_expression = facts.calls[site].result;
    let argument = facts.calls[site]
        .arguments
        .slice(&facts.arguments)
        .unwrap()
        .first()
        .unwrap()
        .value;
    let mut backend = MemoryBackend::new();
    let mut caller = Node::new(NodeType::Function, "operand_relay");
    caller.id = Uuid::from_u128(1);
    let mut callee = Node::new(NodeType::Function, "operand_sink");
    callee.id = Uuid::from_u128(2);
    backend.insert_node(caller.clone()).unwrap();
    backend.insert_node(callee.clone()).unwrap();
    let callee_formal = callee_cfg
        .operands
        .formals
        .iter()
        .find(|f| f.kind == FormalKind::Positional(0))
        .unwrap()
        .value;
    let icfg = InterproceduralCFG::from_cfg_archive(
        &backend,
        HashMap::from([(caller.id, caller_cfg), (callee.id, callee_cfg)]),
    )
    .unwrap();
    let target = InvocationTarget {
        call: CallRef {
            caller: caller.id,
            site: CallSiteId(site as u32),
        },
        callee: callee.id,
        receiver: ReceiverSupply::None,
    };
    let seed = OriginSeed {
        value: ValueRef {
            function: caller.id,
            expression: seed_expression,
        },
        type_id: Uuid::from_u128(3),
    };
    let output = OriginAnalysis::build(&icfg, &[seed], &[target], &[], OriginLimits::default());
    for (function, expression) in [(caller.id, argument), (callee.id, callee_formal)] {
        assert!(
            !output
                .origins(ValueRef {
                    function,
                    expression
                })
                .is_empty(),
            "{language}: missing alias/argument/formal flow: {output:#?}; cfg: {:#?}",
            icfg.get_cfg(caller.id).unwrap()
        );
    }
    if language != "bash" {
        assert!(
            !output
                .origins(ValueRef {
                    function: caller.id,
                    expression: result_expression
                })
                .is_empty(),
            "{language}: missing returned origin: {output:#?}"
        );
    }
}

macro_rules! cases { ($($name:ident => $language:literal),* $(,)?) => { $(#[test] fn $name() { check($language); })* }; }
cases! { c => "c", cpp => "cpp", csharp => "csharp", go => "go", groovy => "groovy", java => "java", javascript => "javascript", typescript => "typescript", kotlin => "kotlin", php => "php", puppet => "puppet", python => "python", ruby => "ruby", rust => "rust", swift => "swift", scala => "scala", dart => "dart", elixir => "elixir", lua => "lua", bash => "bash" }

#[test]
fn go_multi_return_slots_do_not_mix_and_type_switch_alias_keeps_its_origin() {
    let source = include_str!("../../../rgctl-tests/ecommerce-go/correctness/operand_flow.go");
    let mut backend = MemoryBackend::new();
    let mut cfgs = HashMap::new();
    let mut ids = Vec::new();
    for (i, name) in ["operand_projection", "operand_pair", "operand_switch"]
        .iter()
        .enumerate()
    {
        let mut node = Node::new(NodeType::Function, *name);
        node.id = Uuid::from_u128(i as u128 + 1);
        ids.push(node.id);
        backend.insert_node(node.clone()).unwrap();
        cfgs.insert(node.id, build_cfg_for_function("go", source, name).unwrap());
    }
    let caller = &cfgs[&ids[0]];
    let left = caller.operands.formals[0].value;
    let right = caller.operands.formals[1].value;
    let projected_return = caller.operands.returns[0]
        .values
        .slice(&caller.operands.inputs)
        .unwrap()[0];
    assert_eq!(caller.operands.assignment_values.len(), 2);
    let switch = &cfgs[&ids[2]];
    let switched = switch.operands.formals[0].value;
    let switch_returns = switch
        .operands
        .returns
        .iter()
        .flat_map(|r| r.values.slice(&switch.operands.inputs).unwrap())
        .copied()
        .collect::<Vec<_>>();
    let icfg = InterproceduralCFG::from_cfg_archive(&backend, cfgs).unwrap();
    let seeds = [
        (ids[0], left, 10),
        (ids[0], right, 11),
        (ids[2], switched, 12),
    ]
    .map(|(function, expression, tag)| OriginSeed {
        value: ValueRef {
            function,
            expression,
        },
        type_id: Uuid::from_u128(tag),
    });
    let target = InvocationTarget {
        call: CallRef {
            caller: ids[0],
            site: CallSiteId(0),
        },
        callee: ids[1],
        receiver: ReceiverSupply::None,
    };
    let result = OriginAnalysis::build(&icfg, &seeds, &[target], &[], OriginLimits::default());
    let origins = result.origins(ValueRef {
        function: ids[0],
        expression: projected_return,
    });
    assert_eq!(origins.len(), 1, "{result:#?}");
    assert_eq!(result.seeds[origins[0].0 as usize].value.expression, right);
    for expression in switch_returns {
        let origins = result.origins(ValueRef {
            function: ids[2],
            expression,
        });
        assert_eq!(origins.len(), 1, "{result:#?}");
        assert_eq!(
            result.seeds[origins[0].0 as usize].type_id,
            Uuid::from_u128(12)
        );
    }
}

#[test]
fn native_fixture_inventory_covers_all_applicable_tier1_languages() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_operand_cases.json"
    ))
    .unwrap();
    let tier1: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/tier1_call_targets/cases.json"
    ))
    .unwrap();
    let expected: std::collections::BTreeSet<_> = tier1
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["language"].as_str().unwrap())
        .collect();
    let mut actual: std::collections::BTreeSet<_> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["language"].as_str().unwrap())
        .collect();
    // SQL dollar-quoted bodies are opaque in the bundled grammar; HCL is
    // declarative, and ERB/Markdown do not emit procedural callable bodies.
    for language in ["sql", "hcl", "erb", "markdown"] {
        let case = tier1
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["language"] == language)
            .unwrap();
        assert!(!case["reason"].as_str().unwrap().is_empty());
        assert!(actual.insert(language));
    }
    assert_eq!(actual, expected);
}
