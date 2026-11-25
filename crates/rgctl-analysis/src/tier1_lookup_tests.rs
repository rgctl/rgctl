//! Canonical fixture qualification for native lookup and constructor-value APIs.
use crate::call_bindings::{ReceiverSupply, ValueRef};
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use crate::interprocedural_cfg::InterproceduralCFG;
use crate::operands::*;
use crate::value_origins::*;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use uuid::Uuid;

fn lookup_cases() -> Vec<serde_json::Value> {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_lookup_cases.json"
    ))
    .unwrap()
}
fn lookup_source(case: &serde_json::Value) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(case["file"].as_str().unwrap()),
    )
    .unwrap()
}
struct LookupFixture {
    icfg: InterproceduralCFG,
    ids: Vec<Uuid>,
}
impl LookupFixture {
    fn capture(language: &str, source: &str, names: &[&str]) -> Self {
        let mut backend = MemoryBackend::new();
        let mut cfgs = HashMap::new();
        let ids = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut node = Node::new(NodeType::Function, *name);
                node.id = Uuid::from_u128(index as u128 + 1);
                backend.insert_node(node.clone()).unwrap();
                let cfg = build_cfg_for_function(language, source, name)
                    .unwrap_or_else(|e| panic!("{language} {name}: {e}"));
                assert!(cfg.operands.valid_references());
                cfgs.insert(node.id, cfg);
                node.id
            })
            .collect();
        Self {
            icfg: InterproceduralCFG::from_cfg_archive(&backend, cfgs).unwrap(),
            ids,
        }
    }
    fn lookup_cfg(&self, index: usize) -> &ControlFlowGraph {
        self.icfg.get_cfg(self.ids[index]).unwrap()
    }
    fn value(&self, index: usize, expression: ExpressionId) -> ValueRef {
        ValueRef {
            function: self.ids[index],
            expression,
        }
    }
    fn formal(&self, index: usize, formal: usize) -> ValueRef {
        self.value(index, self.lookup_cfg(index).operands.formals[formal].value)
    }
    fn types(&self, output: &OriginAnalysis, value: ValueRef) -> BTreeSet<Option<Uuid>> {
        output
            .origins(value)
            .iter()
            .map(|id| output.seeds[id.0 as usize].type_id)
            .collect()
    }
    fn returns(&self, output: &OriginAnalysis, index: usize) -> BTreeSet<Option<Uuid>> {
        self.lookup_cfg(index)
            .operands
            .returns
            .iter()
            .flat_map(|r| {
                r.values
                    .slice(&self.lookup_cfg(index).operands.inputs)
                    .unwrap()
            })
            .flat_map(|&e| self.types(output, self.value(index, e)))
            .collect()
    }
}
#[test]
fn lookup_inventory_matches_every_procedural_tier1_language() {
    let ordinary: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native_operand_cases.json"
    ))
    .unwrap();
    assert_eq!(
        lookup_cases()
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
fn native_member_values_retain_receiver_selector_and_all_compatible_targets() {
    let mut failures = Vec::new();
    for case in lookup_cases() {
        let language = case["language"].as_str().unwrap();
        let source = lookup_source(&case);
        let f = LookupFixture::capture(
            language,
            &source,
            &["operand_computed", "operand_sink", "operand_relay"],
        );
        let facts = &f.lookup_cfg(0).operands;
        let lookup = callable_member_values(facts).find(|v| {
            if case["computed_mode"] == "named" {
                v.member.is_some()
            } else {
                v.member.is_none()
            }
        });
        let Some(lookup) = lookup else {
            failures.push(format!(
                "{language}: no native lookup; {facts:#?}\n{}",
                crate::language_profile::parse_source(language, source.as_bytes())
                    .unwrap()
                    .root_node()
                    .to_sexp()
            ));
            continue;
        };
        if case["computed_mode"] != "named" && lookup.selector.is_none() {
            failures.push(format!("{language}: missing selector"));
            continue;
        }
        let receiver = f.value(0, lookup.receiver);
        let instance_type = Uuid::from_u128(1000);
        let argument_type = Uuid::from_u128(2000);
        let seeds = [
            OriginSeed {
                value: if language == "bash" {
                    receiver
                } else {
                    f.formal(0, 0)
                },
                type_id: Some(instance_type),
            },
            OriginSeed {
                value: f.formal(0, facts.formals.len() - 1),
                type_id: Some(argument_type),
            },
        ];
        let callables: Vec<_> = [1, 2]
            .map(|index| CallableCandidate {
                value: f.value(0, lookup.value),
                callee: f.ids[index],
                receiver: ReceiverSupply::None,
                requirement: Some(OriginRequirement {
                    value: receiver,
                    type_id: instance_type,
                }),
            })
            .into_iter()
            .collect();
        let output = OriginAnalysis::build_with_values(
            &f.icfg,
            &seeds,
            &[],
            &[],
            &callables,
            &[],
            OriginLimits::default(),
        );
        let targets: BTreeSet<_> = output
            .callable_targets
            .iter()
            .map(|t| t.target.callee)
            .collect();
        if case["computed_mode"] != "named" {
            assert!(
                output.diagnostics.iter().any(|d| matches!(d,
                OriginDiagnostic::UnknownMemberSelector(value)
                    if *value == f.value(0, lookup.value))),
                "{language}: unresolved selector must remain explicit"
            );
        }
        let args_ok = [1, 2].into_iter().all(|i| {
            f.types(&output, f.formal(i, 0)) == [Some(argument_type)].into_iter().collect()
        });
        let returned = language == "bash"
            || f.returns(&output, 0) == [Some(argument_type)].into_iter().collect();
        if targets != [f.ids[1], f.ids[2]].into_iter().collect() || !args_ok || !returned {
            failures.push(format!(
                "{language}: targets/arguments/return {output:#?}\n{facts:#?}"
            ));
        }
        let blocked = [
            OriginSeed {
                value: seeds[0].value,
                type_id: Some(Uuid::from_u128(9999)),
            },
            seeds[1],
        ];
        let negative = OriginAnalysis::build_with_values(
            &f.icfg,
            &blocked,
            &[],
            &[],
            &callables,
            &[],
            OriginLimits::default(),
        );
        assert!(
            negative.callable_targets.is_empty(),
            "{language}: incompatible receiver activated canonical target"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn native_constructor_references_produce_canonical_instances_across_tier1() {
    let mut failures = Vec::new();
    for case in lookup_cases() {
        if case["constructor_mode"] == "not_native" {
            continue;
        }
        let language = case["language"].as_str().unwrap();
        let source = lookup_source(&case);
        let f = LookupFixture::capture(language, &source, &["operand_construct"]);
        let facts = &f.lookup_cfg(0).operands;
        let reference=facts.expressions.iter().position(|e|matches!(e.kind,ExpressionKind::Name(n) if facts.operand_name(n)==Some("OperandConstructed"))).map(|i|ExpressionId(i as u32)).or_else(|| facts.expressions.iter().find_map(|e|if let ExpressionKind::Construction(ty)=e.kind{Some(ty)}else{None}));
        let Some(reference) = reference else {
            failures.push(format!(
                "{language}: no constructor reference {facts:#?}\n{}",
                crate::language_profile::parse_source(language, source.as_bytes())
                    .unwrap()
                    .root_node()
                    .to_sexp()
            ));
            continue;
        };
        let constructor = ConstructorCandidate {
            value: f.value(0, reference),
            type_id: Uuid::from_u128(3000),
            callee: None,
        };
        let output = OriginAnalysis::build_with_values(
            &f.icfg,
            &[],
            &[],
            &[],
            &[],
            &[constructor],
            OriginLimits::default(),
        );
        if f.returns(&output, 0) != [Some(constructor.type_id)].into_iter().collect()
            || output.construction_evidence.is_empty()
            || f.types(&output, constructor.value) != [None].into_iter().collect()
        {
            failures.push(format!(
                "{language}: constructor/instance separation {output:#?}\n{facts:#?}\n{}",
                crate::language_profile::parse_source(language, source.as_bytes())
                    .unwrap()
                    .root_node()
                    .to_sexp()
            ));
        }
        assert!(
            output.callable_targets.is_empty(),
            "{language}: missing constructor body was invented"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn constructor_aliases_registries_and_prototypes_keep_canonical_evidence() {
    for case in lookup_cases() {
        let language = case["language"].as_str().unwrap();
        if !matches!(
            language,
            "javascript" | "typescript" | "python" | "php" | "ruby" | "swift"
        ) {
            continue;
        }
        let source = lookup_source(&case);
        for name in [
            "operand_constructor_alias",
            "operand_constructor_registry",
            "operand_prototype",
            "operand_not_constructor",
        ] {
            if !source.contains(name) {
                continue;
            }
            let names = if name == "operand_prototype" {
                vec![name, "constructor"]
            } else {
                vec![name]
            };
            let f = LookupFixture::capture(language, &source, &names);
            let facts = &f.lookup_cfg(0).operands;
            let expression = if name == "operand_prototype" {
                facts.expressions.iter().position(|e|matches!(e.kind,ExpressionKind::Name(n) if facts.operand_name(n)==Some("OperandConstructed"))).map(|i|ExpressionId(i as u32)).unwrap()
            } else {
                facts.formals[0].value
            };
            let constructor = ConstructorCandidate {
                value: f.value(0, expression),
                type_id: Uuid::from_u128(3000),
                callee: if name == "operand_prototype" {
                    Some(f.ids[1])
                } else {
                    None
                },
            };
            let output = OriginAnalysis::build_with_values(
                &f.icfg,
                &[],
                &[],
                &[],
                &[],
                &[constructor],
                OriginLimits::default(),
            );
            if name == "operand_prototype" {
                assert!(
                    output.callable_targets.is_empty(),
                    "{language}: Object.create invoked a constructor body"
                );
            }
            if name == "operand_not_constructor" {
                assert!(
                    output.construction_evidence.is_empty(),
                    "{language}: arbitrary class method constructed an instance: {output:#?}"
                );
            } else {
                assert_eq!(
                    f.returns(&output, 0),
                    [Some(constructor.type_id)].into_iter().collect(),
                    "{language} {name}: {output:#?}\n{facts:#?}"
                );
            }
        }
    }
}

#[test]
fn explicit_constructor_bodies_bind_instances_and_arguments_separately() {
    let mut failures = Vec::new();
    for case in lookup_cases() {
        let language = case["language"].as_str().unwrap();
        let Some(body) = case["constructor_body"].as_str() else {
            continue;
        };
        let source = lookup_source(&case);
        if let Err(error) = build_cfg_for_function(language, &source, body) {
            failures.push(format!("{language}: constructor body: {error}"));
            continue;
        }
        let f = LookupFixture::capture(language, &source, &["operand_construct", body]);
        let facts = &f.lookup_cfg(0).operands;
        let expression=facts.expressions.iter().position(|e|matches!(e.kind,ExpressionKind::Name(n) if facts.operand_name(n)==Some("OperandConstructed"))).unwrap();
        let constructor = ConstructorCandidate {
            value: f.value(0, ExpressionId(expression as u32)),
            type_id: Uuid::from_u128(3000),
            callee: Some(f.ids[1]),
        };
        assert!(
            !facts.formals.is_empty(),
            "{language}: missing constructor caller argument: {facts:#?}"
        );
        let seed = OriginSeed {
            value: f.formal(0, 0),
            type_id: Some(Uuid::from_u128(2000)),
        };
        let output = OriginAnalysis::build_with_values(
            &f.icfg,
            &[seed],
            &[],
            &[],
            &[],
            &[constructor],
            OriginLimits::default(),
        );
        let formals = &f.lookup_cfg(1).operands.formals;
        let receiver = formals
            .iter()
            .position(|p| p.kind == FormalKind::Receiver)
            .or_else(|| {
                if matches!(language, "python") {
                    Some(0)
                } else {
                    None
                }
            });
        let receiver_ok = receiver.is_some_and(|i| {
            f.types(&output, f.formal(1, i)) == [Some(constructor.type_id)].into_iter().collect()
        });
        let argument_ok = formals.iter().enumerate().any(|(i, _)| {
            Some(i) != receiver
                && f.types(&output, f.formal(1, i)) == [seed.type_id].into_iter().collect()
        });
        if !receiver_ok
            || !argument_ok
            || f.returns(&output, 0) != [Some(constructor.type_id)].into_iter().collect()
            || !output.callable_targets.iter().any(|t| {
                t.target.callee == f.ids[1] && t.target.receiver == ReceiverSupply::Constructed
            })
        {
            failures.push(format!(
                "{language}: constructor receiver/argument binding {output:#?}\n{:#?}",
                f.lookup_cfg(1).operands
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn constructor_reference_validation_and_budgets_remain_explicit() {
    let case = lookup_cases()
        .into_iter()
        .find(|c| c["language"] == "javascript")
        .unwrap();
    let source = lookup_source(&case);
    let f = LookupFixture::capture("javascript", &source, &["operand_constructor_alias"]);
    let constructor = ConstructorCandidate {
        value: f.formal(0, 0),
        type_id: Uuid::from_u128(3000),
        callee: None,
    };
    let limits = OriginLimits {
        max_member_candidates: 0,
        ..OriginLimits::default()
    };
    let limited =
        OriginAnalysis::build_with_values(&f.icfg, &[], &[], &[], &[], &[constructor], limits);
    assert!(
        limited
            .diagnostics
            .iter()
            .any(|d| matches!(d, OriginDiagnostic::LimitReached))
    );
    let invalid = ConstructorCandidate {
        value: f.value(0, ExpressionId(u32::MAX)),
        ..constructor
    };
    let output = OriginAnalysis::build_with_values(
        &f.icfg,
        &[],
        &[],
        &[],
        &[],
        &[],
        OriginLimits::default(),
    );
    assert!(output.constructor_values.is_empty() && output.construction_evidence.is_empty());
    let bad = OriginAnalysis::build_with_values(
        &f.icfg,
        &[],
        &[],
        &[],
        &[],
        &[invalid],
        OriginLimits::default(),
    );
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| matches!(d, OriginDiagnostic::InvalidValue(v) if *v == invalid.value))
    );
}

#[test]
fn ecma_constructor_object_returns_remain_possible_without_typing_the_constructor() {
    for case in lookup_cases()
        .into_iter()
        .filter(|c| matches!(c["language"].as_str(), Some("javascript" | "typescript")))
    {
        let language = case["language"].as_str().unwrap();
        let source = lookup_source(&case);
        let f = LookupFixture::capture(
            language,
            &source,
            &["operand_construct_return", "operand_returning_constructor"],
        );
        let facts = &f.lookup_cfg(0).operands;
        let expression=facts.expressions.iter().position(|e| matches!(e.kind,ExpressionKind::Name(n) if facts.operand_name(n)==Some("operand_returning_constructor"))).unwrap();
        let constructor = ConstructorCandidate {
            value: f.value(0, ExpressionId(expression as u32)),
            type_id: Uuid::from_u128(3000),
            callee: Some(f.ids[1]),
        };
        let seed = OriginSeed {
            value: f.formal(0, 0),
            type_id: Some(Uuid::from_u128(2000)),
        };
        let output = OriginAnalysis::build_with_values(
            &f.icfg,
            &[seed],
            &[],
            &[],
            &[],
            &[constructor],
            OriginLimits::default(),
        );
        assert_eq!(
            f.returns(&output, 0),
            [Some(constructor.type_id), seed.type_id]
                .into_iter()
                .collect(),
            "{language}: {output:#?}"
        );
        assert_eq!(
            f.types(&output, constructor.value),
            [None].into_iter().collect()
        );
        assert!(output.diagnostics.iter().any(|d| matches!(
            d,
            OriginDiagnostic::Binding {
                diagnostic: crate::call_bindings::BindingDiagnostic::ConservativeConstructorReturn,
                ..
            }
        )));
    }
}

#[test]
fn native_lookup_boundaries_do_not_confuse_receives_resources_or_arbitrary_getattr_methods() {
    for (language, name) in [
        ("go", "operand_channel"),
        ("puppet", "operand_resource_reference"),
        ("python", "operand_not_getattr"),
    ] {
        let case = lookup_cases()
            .into_iter()
            .find(|c| c["language"] == language)
            .unwrap();
        let source = lookup_source(&case);
        let f = LookupFixture::capture(language, &source, &[name]);
        assert!(
            callable_member_values(&f.lookup_cfg(0).operands).all(|value| value.member.is_some()),
            "{language}: unrelated native form became a computed-member read"
        );
    }
}

#[test]
fn ecma_constructor_locations_support_indexed_names_and_source_aliases() {
    for case in lookup_cases()
        .into_iter()
        .filter(|c| matches!(c["language"].as_str(), Some("javascript" | "typescript")))
    {
        let language = case["language"].as_str().unwrap();
        let source = lookup_source(&case);
        let indexed = build_cfg_for_function(language, &source, "OperandConstructed").unwrap();
        let legacy = build_cfg_for_function(language, &source, "constructor").unwrap();
        assert_eq!(indexed.operands, legacy.operands);
        let (_, locations) =
            crate::cfg_builder::index_function_locations(language, source.as_bytes()).unwrap();
        let canonical = locations.get("OperandConstructed").unwrap();
        let alias = locations.get("constructor").unwrap();
        assert_eq!(
            (canonical.start_byte, canonical.end_byte),
            (alias.start_byte, alias.end_byte)
        );
    }
}
