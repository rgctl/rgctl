use super::*;
use crate::cfg_builder::build_cfg_for_function;
use std::collections::{HashSet, VecDeque};

fn build(
    language: &str,
    source: &str,
) -> (ControlFlowGraph, ProgramDependenceGraph, LocalValueFlow) {
    let cfg = build_cfg_for_function(language, source, "consume").unwrap();
    assert!(cfg.operands.valid_references());
    assert!(cfg.valid_local_operand_positions());
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    let flow = LocalValueFlow::build(&cfg, &pdg, LocalFlowLimits::default());
    (cfg, pdg, flow)
}
fn reaches(flow: &LocalValueFlow, from: ExpressionId, to: ExpressionId) -> bool {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    while let Some(value) = queue.pop_front() {
        if value == to {
            return true;
        }
        if seen.insert(value) {
            queue.extend(
                flow.transfers
                    .iter()
                    .filter(|e| e.from == value)
                    .map(|e| e.to),
            );
        }
    }
    false
}
fn construction(cfg: &ControlFlowGraph, name: &str) -> ExpressionId {
    cfg.operands.calls.iter().find(|c| {
        matches!(cfg.operands.expressions[c.callee.0 as usize].kind,ExpressionKind::Name(n) if cfg.operands.operand_name(n)==Some(name))
    }).unwrap().result
}
#[test]
fn native_formals_reach_exact_receiver_occurrences_through_local_aliases_in_six_languages() {
    for (language, source) in [
        (
            "java",
            "class Demo { static int consume(X value) { X first = value; X second = (first); return second.execute(); } }",
        ),
        (
            "javascript",
            "function consume(value) { const first = value; const second = (first); return second.execute(); }",
        ),
        (
            "typescript",
            "function consume(value: X) { const first = value; const second = (first); return second.execute(); }",
        ),
        (
            "python",
            "def consume(value):\n first = value\n second = (first)\n return second.execute()\n",
        ),
        (
            "go",
            "package demo\nfunc consume(value X) int { first := value; second := (first); return second.execute() }",
        ),
        (
            "rust",
            "fn consume(value: X) -> i32 { let first = value; let second = (first); return second.execute(); }",
        ),
    ] {
        let (cfg, _, flow) = build(language, source);
        let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
        assert!(
            reaches(&flow, cfg.operands.formals[0].value, receiver),
            "{language}: {flow:?}"
        );
        assert!(
            !flow
                .diagnostics
                .contains(&LocalFlowDiagnostic::InvalidReference)
        );
    }
}
#[test]
fn same_line_reassignment_uses_the_latest_reaching_value() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume(value) { value.execute(); value = new W(); return value.execute(); }",
    );
    let receivers: Vec<_> = cfg
        .operands
        .calls
        .iter()
        .filter_map(|c| c.receiver)
        .collect();
    assert_eq!(receivers.len(), 2);
    let formal = cfg.operands.formals[0].value;
    let replacement = construction(&cfg, "W");
    assert!(reaches(&flow, formal, receivers[0]));
    assert!(!reaches(&flow, formal, receivers[1]));
    assert!(reaches(&flow, replacement, receivers[1]));
    assert!(!reaches(&flow, replacement, receivers[0]));
}
#[test]
fn branches_and_loops_keep_all_reaching_alternatives() {
    for statement in [
        "if (flag) { value = new W(); }",
        "while (flag) { value = new W(); }",
    ] {
        let source = format!(
            "function consume(flag) {{ let value = new X(); {statement} return value.execute(); }}"
        );
        let (cfg, _, flow) = build("javascript", &source);
        let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
        for name in ["X", "W"] {
            assert!(
                reaches(&flow, construction(&cfg, name), receiver),
                "{name}: {flow:?}"
            );
        }
    }
}
#[test]
fn shadowed_slot_does_not_replace_outer_formal() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume(value) { { let value = new W(); use(value); } return value.execute(); }",
    );
    let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
    assert!(reaches(&flow, cfg.operands.formals[0].value, receiver));
    assert!(!reaches(&flow, construction(&cfg, "W"), receiver));
    assert!(
        cfg.operands
            .local_slots
            .iter()
            .any(|&name| cfg.operands.operand_name(name) == Some("value#1"))
    );
}
#[test]
fn multiple_declarators_keep_possible_initializers_with_explicit_uncertainty() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume() { let x = new X(), y = x; return y.execute(); }",
    );
    let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
    assert!(
        reaches(&flow, construction(&cfg, "X"), receiver),
        "{flow:?}"
    );
    assert!(
        flow.diagnostics
            .iter()
            .any(|d| matches!(d, LocalFlowDiagnostic::IntraStatementWrites(_)))
    );
}
#[test]
fn complex_patterns_do_not_alias_an_entire_container_to_its_element() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume(value) { const {x} = value; return x.execute(); }",
    );
    assert!(
        cfg.operands
            .diagnostics
            .iter()
            .any(|d| d.kind == OperandDiagnosticKind::LocalWriteShape)
    );
    assert!(
        flow.diagnostics
            .contains(&LocalFlowDiagnostic::PartialOperands)
    );
    assert!(
        flow.diagnostics
            .iter()
            .any(|d| matches!(d, LocalFlowDiagnostic::UnmodeledWrite(_)))
    );
    let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
    assert!(!reaches(&flow, cfg.operands.formals[0].value, receiver));
}
#[test]
fn archive_round_trip_and_auxiliary_index_reordering_preserve_value_transfers() {
    let (cfg, pdg, flow) = build(
        "javascript",
        "function consume(value) { let first = value; first = new W(); return first.execute(); }",
    );
    let cfg: ControlFlowGraph = bincode::deserialize(&bincode::serialize(&cfg).unwrap()).unwrap();
    let mut pdg: ProgramDependenceGraph =
        bincode::deserialize(&bincode::serialize(&pdg).unwrap()).unwrap();
    pdg.restore_derived_indexes();
    // Auxiliary order must not be authoritative, including older archived
    // indexes rebuilt from unordered node maps.
    let mut stored = serde_json::to_value(&pdg).unwrap();
    for ids in stored["block_nodes"].as_object_mut().unwrap().values_mut() {
        ids.as_array_mut().unwrap().reverse();
    }
    let pdg: ProgramDependenceGraph = serde_json::from_value(stored).unwrap();
    let restored = LocalValueFlow::build(&cfg, &pdg, LocalFlowLimits::default());
    assert_eq!(restored.transfers, flow.transfers);
    assert_eq!(restored.diagnostics, flow.diagnostics);
}
#[test]
fn invalid_positions_incompatible_pdgs_and_limits_are_not_successful_negatives() {
    let (mut cfg, pdg, _) = build(
        "javascript",
        "function consume(value) { return value.execute(); }",
    );
    for limits in [
        LocalFlowLimits {
            max_transfers: 0,
            ..Default::default()
        },
        LocalFlowLimits {
            max_definitions: 0,
            ..Default::default()
        },
        LocalFlowLimits {
            max_candidate_checks: 0,
            ..Default::default()
        },
    ] {
        assert!(
            LocalValueFlow::build(&cfg, &pdg, limits)
                .diagnostics
                .contains(&LocalFlowDiagnostic::LimitReached)
        );
    }
    assert!(
        LocalValueFlow::build(&cfg, &ProgramDependenceGraph::default(), Default::default())
            .diagnostics
            .contains(&LocalFlowDiagnostic::IncompatiblePdg)
    );
    cfg.operands.local_uses[0].statement.index = u32::MAX;
    assert!(
        LocalValueFlow::build(&cfg, &pdg, Default::default())
            .diagnostics
            .contains(&LocalFlowDiagnostic::InvalidReference)
    );
    assert!(bincode::deserialize::<ControlFlowGraph>(&bincode::serialize(&cfg).unwrap()).is_err());
}
#[test]
fn pruned_unreachable_statements_do_not_leave_dangling_local_anchors() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume(value) { return value; let unused = new W(); }",
    );
    assert!(cfg.valid_local_operand_positions());
    assert!(
        !flow
            .diagnostics
            .contains(&LocalFlowDiagnostic::InvalidReference)
    );
}

#[test]
fn go_rhs_names_are_reads_and_mixed_short_declarations_reuse_existing_slots() {
    let (cfg, _, flow) = build(
        "go",
        "package demo\nfunc consume(value X) int { first := value; _ = first; value, other := W{}, 1; _ = other; return value.execute() }",
    );
    let first = cfg
        .blocks
        .values()
        .flat_map(|b| &b.statements)
        .find(|s| s.text.starts_with("first :="))
        .unwrap();
    assert!(first.defines("first"));
    assert!(!first.defines("value"));
    assert!(
        !cfg.operands
            .local_slots
            .iter()
            .any(|&n| cfg.operands.operand_name(n) == Some("value#1"))
    );
    let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
    assert!(!reaches(&flow, cfg.operands.formals[0].value, receiver));
    let value = cfg
        .operands
        .expressions
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            matches!(e.kind, ExpressionKind::Construction(_)).then_some(ExpressionId(i as u32))
        })
        .unwrap();
    assert!(reaches(&flow, value, receiver), "{flow:?}");
}

#[test]
fn selected_invocation_bindings_compose_with_exact_local_reads_and_returns() {
    use crate::call_bindings::{
        BindingSource, CallRef, InvocationTarget, ReceiverSupply, ValueRef,
    };
    use crate::interprocedural_cfg::InterproceduralCFG;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Node, NodeType};
    let caller =
        "function caller(first, second) { const actual = second; return consume(actual); }";
    let callee = "function consume(value) { const alias = value; return alias; }";
    let mut backend = MemoryBackend::new();
    let mut configs = HashMap::new();
    let mut locals = Vec::new();
    let mut ids = Vec::new();
    for (name, source) in [("caller", caller), ("consume", callee)] {
        let node = Node::new(NodeType::Function, name);
        let id = node.id;
        backend.insert_node(node).unwrap();
        ids.push(id);
        let cfg = build_cfg_for_function("javascript", source, name).unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
        let flow = LocalValueFlow::build(&cfg, &pdg, Default::default());
        locals.extend(flow.transfers.into_iter().map(|e| {
            (
                ValueRef {
                    function: id,
                    expression: e.from,
                },
                ValueRef {
                    function: id,
                    expression: e.to,
                },
            )
        }));
        configs.insert(id, cfg);
    }
    let icfg = InterproceduralCFG::from_cfg_archive(&backend, configs).unwrap();
    let mut query = icfg.call_bindings();
    let binding = query
        .bind_invocation(InvocationTarget {
            call: CallRef {
                caller: ids[0],
                site: CallSiteId(0),
            },
            callee: ids[1],
            receiver: ReceiverSupply::None,
        })
        .unwrap();
    locals.extend(
        binding
            .transfers
            .slice(query.transfers())
            .unwrap()
            .iter()
            .filter_map(|e| match e.from {
                BindingSource::Expression(from) => Some((from, e.to)),
                _ => None,
            }),
    );
    let cfg = icfg.get_cfg(ids[0]).unwrap();
    let target = ValueRef {
        function: ids[0],
        expression: cfg.operands.calls[0].result,
    };
    let can_reach = |from: ValueRef| {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(value) = stack.pop() {
            if value == target {
                return true;
            }
            if seen.insert(value) {
                stack.extend(locals.iter().filter(|(a, _)| *a == value).map(|(_, b)| *b));
            }
        }
        false
    };
    assert!(can_reach(ValueRef {
        function: ids[0],
        expression: cfg.operands.formals[1].value
    }));
    assert!(!can_reach(ValueRef {
        function: ids[0],
        expression: cfg.operands.formals[0].value
    }));
}

#[test]
fn parameter_handoff_prefers_its_first_body_read_over_the_entry_definition() {
    use crate::blast_slice_handoff::criterion_for_parameter;
    use crate::interprocedural_cfg::InterproceduralCFG;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Node, NodeType};
    let source = "function consume(first, second) {\n const alias = second;\n return alias;\n}";
    let cfg = build_cfg_for_function("javascript", source, "consume").unwrap();
    let mut backend = MemoryBackend::new();
    let node = Node::new(NodeType::Function, "consume");
    let id = node.id;
    backend.insert_node(node).unwrap();
    let icfg = InterproceduralCFG::from_cfg_archive(&backend, HashMap::from([(id, cfg)])).unwrap();
    for _ in 0..16 {
        assert_eq!(
            criterion_for_parameter(&backend, &icfg, &HashMap::new(), id, "second")
                .unwrap()
                .line,
            2
        );
        assert_eq!(
            criterion_for_parameter(&backend, &icfg, &HashMap::new(), id, "first")
                .unwrap()
                .line,
            1
        );
    }
}

#[test]
fn conditional_factory_return_values_reach_the_callers_receiver() {
    use crate::call_bindings::{
        BindingSource, CallRef, InvocationTarget, ReceiverSupply, ValueRef,
    };
    use crate::interprocedural_cfg::InterproceduralCFG;
    use rgctl_graph::backend::{GraphBackend, MemoryBackend};
    use rgctl_graph::schema::{Node, NodeType};
    let sources = [
        (
            "caller",
            "function caller(flag) { return make(flag).execute(); }",
        ),
        (
            "make",
            "function make(flag) { if (flag) { return new X(); } return new W(); }",
        ),
    ];
    let mut backend = MemoryBackend::new();
    let mut configs = HashMap::new();
    let mut edges = Vec::new();
    let mut ids = Vec::new();
    for (name, source) in sources {
        let node = Node::new(NodeType::Function, name);
        let id = node.id;
        ids.push(id);
        backend.insert_node(node).unwrap();
        let cfg = build_cfg_for_function("javascript", source, name).unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
        let local = LocalValueFlow::build(&cfg, &pdg, Default::default());
        edges.extend(local.transfers.into_iter().map(|e| {
            (
                ValueRef {
                    function: id,
                    expression: e.from,
                },
                ValueRef {
                    function: id,
                    expression: e.to,
                },
            )
        }));
        configs.insert(id, cfg);
    }
    let icfg = InterproceduralCFG::from_cfg_archive(&backend, configs).unwrap();
    let caller = icfg.get_cfg(ids[0]).unwrap();
    let site=caller.operands.calls.iter().position(|call|matches!(caller.operands.expressions[call.callee.0 as usize].kind,ExpressionKind::Name(n) if caller.operands.operand_name(n)==Some("make"))).unwrap();
    let mut bindings = icfg.call_bindings();
    let binding = bindings
        .bind_invocation(InvocationTarget {
            call: CallRef {
                caller: ids[0],
                site: CallSiteId(site as u32),
            },
            callee: ids[1],
            receiver: ReceiverSupply::None,
        })
        .unwrap();
    edges.extend(
        binding
            .transfers
            .slice(bindings.transfers())
            .unwrap()
            .iter()
            .filter_map(|e| match e.from {
                BindingSource::Expression(from) => Some((from, e.to)),
                _ => None,
            }),
    );
    let receiver = ValueRef {
        function: ids[0],
        expression: caller
            .operands
            .calls
            .iter()
            .find_map(|c| c.receiver)
            .unwrap(),
    };
    let reaches_receiver = |from| {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(value) = stack.pop() {
            if value == receiver {
                return true;
            }
            if seen.insert(value) {
                stack.extend(edges.iter().filter(|(a, _)| *a == value).map(|(_, b)| *b));
            }
        }
        false
    };
    let factory = icfg.get_cfg(ids[1]).unwrap();
    for name in ["X", "W"] {
        assert!(reaches_receiver(ValueRef {
            function: ids[1],
            expression: construction(factory, name)
        }));
    }
    // Control dependence on the selector does not make its value a receiver alias.
    assert!(!reaches_receiver(ValueRef {
        function: ids[0],
        expression: caller.operands.formals[0].value
    }));
}

#[test]
fn conditional_value_branches_flow_but_the_condition_does_not() {
    for (language, source) in [
        (
            "javascript",
            "function consume(flag) { return flag ? new X() : new W(); }",
        ),
        (
            "typescript",
            "function consume(flag:boolean):Y { return flag ? new X() : new W(); }",
        ),
        (
            "java",
            "class Demo { static Y consume(boolean flag) { return flag ? new X() : new W(); } }",
        ),
        (
            "python",
            "def consume(flag):\n return X() if flag else W()\n",
        ),
    ] {
        let (cfg, _, flow) = build(language, source);
        let result = cfg.operands.returns[0]
            .values
            .slice(&cfg.operands.inputs)
            .unwrap()[0];
        for ty in ["X", "W"] {
            assert!(
                reaches(&flow, construction(&cfg, ty), result),
                "{language} {ty}: {flow:?}"
            );
        }
        assert!(
            !reaches(&flow, cfg.operands.formals[0].value, result),
            "condition is not the selected value: {language}"
        );
    }
}

#[test]
fn successful_type_assertions_and_casts_preserve_only_the_operand_value() {
    for (language, source) in [
        (
            "typescript",
            "function consume(value:unknown):number { return (value as Y).execute(); }",
        ),
        (
            "typescript",
            "function consume(value:unknown):number { return (<Y>value).execute(); }",
        ),
        (
            "typescript",
            "function consume(value:Y|null):number { return value!.execute(); }",
        ),
        (
            "java",
            "class Demo { static int consume(Object value) { return ((Y)value).execute(); } }",
        ),
        (
            "go",
            "package demo\nfunc consume(value any) int {return value.(Y).Execute()}",
        ),
    ] {
        let (cfg, _, flow) = build(language, source);
        let receiver = cfg.operands.calls.iter().find_map(|c| c.receiver).unwrap();
        assert!(
            reaches(&flow, cfg.operands.formals[0].value, receiver),
            "{language}: {source}: {flow:?}"
        );
    }
}

#[test]
fn ordinary_computation_does_not_become_a_value_alias() {
    let (cfg, _, flow) = build(
        "javascript",
        "function consume(value) { return value + 1; }",
    );
    let result = cfg.operands.returns[0]
        .values
        .slice(&cfg.operands.inputs)
        .unwrap()[0];
    assert!(!reaches(&flow, cfg.operands.formals[0].value, result));
}

#[test]
fn conditional_operands_round_trip_and_validate_control_references() {
    let (cfg, _, _) = build(
        "javascript",
        "function consume(flag) { return flag ? new X() : new W(); }",
    );
    let encoded = bincode::serialize(&cfg).unwrap();
    let mut restored: ControlFlowGraph = bincode::deserialize(&encoded).unwrap();
    assert_eq!(restored.operands, cfg.operands);
    let branch = restored
        .operands
        .expressions
        .iter_mut()
        .find(|e| matches!(e.kind, ExpressionKind::Conditional { .. }))
        .unwrap();
    branch.kind = ExpressionKind::Conditional {
        condition: ExpressionId(u32::MAX),
    };
    assert!(!restored.operands.valid_references());
}
