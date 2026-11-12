use super::*;
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;

fn name(facts: &FunctionOperands, value: ExpressionId) -> &str {
    match facts.expressions[value.0 as usize].kind {
        ExpressionKind::Name(id) | ExpressionKind::Member(id) => facts.name(id).unwrap(),
        kind => panic!("expected named operand, got {kind:?}"),
    }
}

fn arguments(facts: &FunctionOperands, call: usize) -> &[ArgumentFact] {
    facts.calls[call].arguments.slice(&facts.arguments).unwrap()
}

#[test]
fn receiver_argument_formal_and_return_roles_round_trip_in_six_languages() {
    for (language, source) in [
        (
            "java",
            "class Demo { int consume(X value, X other) { return value.execute(other); } }",
        ),
        (
            "javascript",
            "function consume(value, other) { return value.execute(other); }",
        ),
        (
            "typescript",
            "function consume(value: X, other: X) { return value.execute(other); }",
        ),
        (
            "python",
            "def consume(value, other):\n return value.execute(other)\n",
        ),
        (
            "go",
            "package demo\nfunc consume(value X, other X) int { return value.execute(other) }",
        ),
        (
            "rust",
            "fn consume(value: X, other: X) -> i32 { return value.execute(other); }",
        ),
    ] {
        let cfg = build_cfg_for_function(language, source, "consume").unwrap();
        let facts = &cfg.operands;
        assert_eq!(facts.status, OperandStatus::Captured, "{language}");
        assert!(
            facts.diagnostics.is_empty(),
            "{language}: {:?}",
            facts.diagnostics
        );
        assert!(facts.valid_references(), "{language}");
        let explicit: Vec<_> = facts
            .formals
            .iter()
            .filter_map(|f| match f.kind {
                FormalKind::Positional(index) => Some((index, facts.name(f.name).unwrap())),
                _ => None,
            })
            .collect();
        assert_eq!(explicit, [(0, "value"), (1, "other")], "{language}");
        assert_eq!(facts.calls.len(), 1, "{language}");
        let call = &facts.calls[0];
        assert_eq!(
            &source[call.span.start as usize..call.span.end as usize],
            "value.execute(other)",
            "{language}"
        );
        assert_eq!(name(facts, call.callee), "execute", "{language}");
        assert_eq!(name(facts, call.receiver.unwrap()), "value", "{language}");
        assert_eq!(arguments(facts, 0).len(), 1, "{language}");
        assert_eq!(
            name(facts, arguments(facts, 0)[0].value),
            "other",
            "{language}"
        );
        assert_eq!(facts.returns.len(), 1, "{language}");
        assert_eq!(
            facts.returns[0].values.slice(&facts.inputs).unwrap(),
            [call.result],
            "{language}"
        );
        assert_eq!(
            facts.expressions[call.result.0 as usize].kind,
            ExpressionKind::CallResult(CallSiteId(0))
        );
        // Fresh native trees have different transient node IDs, but exported
        // local identities and archive facts must remain deterministic.
        assert_eq!(
            facts,
            &build_cfg_for_function(language, source, "consume")
                .unwrap()
                .operands
        );
        let restored: ControlFlowGraph =
            bincode::deserialize(&bincode::serialize(&cfg).unwrap()).unwrap();
        assert_eq!(facts, &restored.operands);
        let tmp = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4();
        let mut archive = crate::cfg_pdg_archive::CfgPdgArchive::default();
        archive.insert(crate::cfg_pdg_archive::CfgPdgRecord {
            function_id: id,
            code_hash: rgctl_graph::code_index::hash_code(source),
            function_name: "consume".into(),
            file_path: None,
            cfg: std::sync::Arc::new(cfg.clone()),
            pdg: std::sync::Arc::new(
                crate::pdg::ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap(),
            ),
        });
        let path = crate::cfg_pdg_archive::CfgPdgArchive::default_path(tmp.path());
        archive.write_to_path(&path).unwrap();
        let view = crate::cfg_pdg_archive::CfgPdgArchiveMmap::open(&path).unwrap();
        assert_eq!(facts, &view.load_record(id).unwrap().unwrap().cfg.operands);
        assert_eq!(
            facts.names.iter().filter(|n| n.as_str() == "value").count(),
            1
        );
    }
}

#[test]
fn nested_and_repeated_same_line_invocations_keep_distinct_roles() {
    let source = "function example(x,y) { return consume(make(x), x.execute(y)) + consume(x,y); }";
    let cfg = build_cfg_for_function("javascript", source, "example").unwrap();
    let facts = &cfg.operands;
    assert_eq!(facts.calls.len(), 4);
    assert_eq!(
        facts
            .calls
            .iter()
            .map(|c| name(facts, c.callee))
            .collect::<Vec<_>>(),
        ["consume", "make", "execute", "consume"]
    );
    let outer = arguments(facts, 0);
    assert_eq!(outer.len(), 2);
    assert_eq!(
        facts.expressions[outer[0].value.0 as usize].kind,
        ExpressionKind::CallResult(CallSiteId(1))
    );
    assert_eq!(
        facts.expressions[outer[1].value.0 as usize].kind,
        ExpressionKind::CallResult(CallSiteId(2))
    );
    assert_eq!(name(facts, arguments(facts, 1)[0].value), "x");
    assert_eq!(name(facts, facts.calls[2].receiver.unwrap()), "x");
    assert_eq!(name(facts, arguments(facts, 2)[0].value), "y");
    assert_eq!(
        arguments(facts, 3)
            .iter()
            .map(|a| name(facts, a.value))
            .collect::<Vec<_>>(),
        ["x", "y"]
    );
    assert_ne!(facts.calls[0].span, facts.calls[3].span);
    assert_ne!(facts.calls[0].result, facts.calls[3].result);
    assert!(facts.valid_references());
}

#[test]
fn construction_is_the_actual_value_of_the_outer_invocation() {
    for (language, source) in [
        (
            "java",
            "class Demo { X example() { return consume(new X()); } }",
        ),
        (
            "javascript",
            "function example() { return consume(new X()); }",
        ),
        (
            "typescript",
            "function example() { return consume(new X()); }",
        ),
    ] {
        let cfg = build_cfg_for_function(language, source, "example").unwrap();
        let facts = &cfg.operands;
        assert_eq!(facts.calls.len(), 2, "{language}");
        assert_eq!(
            facts.calls[1].kind,
            InvocationKind::Construction,
            "{language}"
        );
        assert_eq!(name(facts, facts.calls[1].callee), "X", "{language}");
        assert_eq!(
            arguments(facts, 0)[0].value,
            facts.calls[1].result,
            "{language}"
        );
        assert_eq!(
            facts.returns[0].values.slice(&facts.inputs).unwrap(),
            [facts.calls[0].result],
            "{language}"
        );
    }
}

#[test]
fn python_parameter_and_actual_binding_conventions_are_retained() {
    let source = "def consume(value, /, other=1, *, named=2, **kw):\n return value.execute(named=other, *value, **kw)\n";
    let cfg = build_cfg_for_function("python", source, "consume").unwrap();
    let facts = &cfg.operands;
    assert_eq!(
        facts.formals.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            FormalKind::PositionalOnly(0),
            FormalKind::Positional(1),
            FormalKind::KeywordOnly,
            FormalKind::VariadicKeyword
        ]
    );
    assert!(facts.formals[1].default_value.is_some());
    assert!(facts.formals[2].default_value.is_some());
    let args = arguments(facts, 0);
    assert!(matches!(args[0].kind, ArgumentKind::Named(id) if facts.name(id) == Some("named")));
    assert_eq!(name(facts, args[0].value), "other");
    assert_eq!(args[1].kind, ArgumentKind::SpreadPositional);
    assert_eq!(args[2].kind, ArgumentKind::SpreadKeywords);
    assert!(facts.valid_references());
}

#[test]
fn go_receiver_grouped_parameters_variadics_and_multiple_returns() {
    let source = "package demo\nfunc (s *Service) consume(first, second X, rest ...X) (X, error) { return first, nil }";
    let cfg = build_cfg_for_function("go", source, "consume").unwrap();
    let facts = &cfg.operands;
    assert_eq!(facts.receiver_convention, ReceiverConvention::Explicit);
    assert_eq!(
        facts
            .formals
            .iter()
            .map(|f| (facts.name(f.name).unwrap(), f.kind))
            .collect::<Vec<_>>(),
        [
            ("s", FormalKind::Receiver),
            ("first", FormalKind::Positional(0)),
            ("second", FormalKind::Positional(1)),
            ("rest", FormalKind::VariadicPositional)
        ]
    );
    assert_eq!(
        facts.returns[0].values.slice(&facts.inputs).unwrap().len(),
        2
    );
    assert!(facts.diagnostics.is_empty(), "{:?}", facts.diagnostics);
}

#[test]
fn javascript_typescript_defaults_optional_and_rest_parameters() {
    for (language, source) in [
        (
            "javascript",
            "function consume(value, other=1, ...rest) { return value.execute(other, ...rest); }",
        ),
        (
            "typescript",
            "function consume(value: X, other?: number, ...rest: X[]) { return value.execute(other, ...rest); }",
        ),
    ] {
        let cfg = build_cfg_for_function(language, source, "consume").unwrap();
        let facts = &cfg.operands;
        assert_eq!(facts.formals.len(), 3, "{language}");
        assert_eq!(
            facts.formals[2].kind,
            FormalKind::VariadicPositional,
            "{language}"
        );
        assert!(facts.formals[1].optional, "{language}");
        assert_eq!(
            arguments(facts, 0)[1].kind,
            ArgumentKind::SpreadPositional,
            "{language}"
        );
    }
}

#[test]
fn arrow_expression_and_rust_tail_returns_have_explicit_result_roles() {
    for (language, source) in [
        ("javascript", "const example = value => value.execute();"),
        ("rust", "fn example(value: X) -> i32 { value.execute() }"),
    ] {
        let cfg = build_cfg_for_function(language, source, "example").unwrap();
        let facts = &cfg.operands;
        assert_eq!(facts.returns.len(), 1, "{language}");
        assert!(facts.returns[0].implicit, "{language}");
        assert_eq!(
            facts.returns[0].values.slice(&facts.inputs).unwrap(),
            [facts.calls[0].result],
            "{language}"
        );
    }
}

#[test]
fn nested_callable_body_does_not_leak_invocations_into_its_parent() {
    for (language, source) in [
        (
            "javascript",
            "function example(value) { const nested = other => other.hidden(); return value.execute(); }",
        ),
        (
            "java",
            "class Demo { int example(X value) { Runnable nested = () -> value.hidden(); return value.execute(); } }",
        ),
        (
            "python",
            "def example(value):\n nested = lambda other: other.hidden()\n return value.execute()\n",
        ),
        (
            "rust",
            "fn example(value: X) -> i32 { let nested = || value.hidden(); value.execute() }",
        ),
        (
            "go",
            "package demo\nfunc example(value X) int { nested := func(other X) int { return other.hidden() }; return value.execute() }",
        ),
    ] {
        let cfg = build_cfg_for_function(language, source, "example").unwrap();
        let facts = &cfg.operands;
        assert_eq!(
            facts.calls.len(),
            1,
            "{language}: nested body leaked into caller"
        );
        assert_eq!(name(facts, facts.calls[0].callee), "execute", "{language}");
        assert_eq!(
            facts.returns.len(),
            1,
            "{language}: nested return leaked into caller"
        );
        assert!(
            facts
                .diagnostics
                .iter()
                .any(|d| d.kind == OperandDiagnosticKind::NestedCallable),
            "{language}"
        );
    }
}

#[test]
fn unsupported_destructuring_is_diagnosed_not_invented_as_a_formal_name() {
    let cfg = build_cfg_for_function(
        "javascript",
        "function example({value}) { return value.execute(); }",
        "example",
    )
    .unwrap();
    assert!(
        cfg.operands
            .diagnostics
            .iter()
            .any(|d| d.kind == OperandDiagnosticKind::ParameterPattern)
    );
    assert!(cfg.operands.formals.is_empty());
}

#[test]
fn python_descriptor_conventions_distinguish_static_and_class_methods() {
    for (decorator, expected) in [
        ("", ReceiverConvention::PythonDescriptor),
        (" @staticmethod\n", ReceiverConvention::None),
        (" @classmethod\n", ReceiverConvention::PythonClassDescriptor),
    ] {
        let source = format!(
            "class Demo:\n{decorator} def example(self, value):\n  return value.execute()\n"
        );
        let cfg = build_cfg_for_function("python", &source, "example").unwrap();
        assert_eq!(cfg.operands.receiver_convention, expected, "{decorator}");
        // Python's explicit first formal remains at its declaration index.
        assert_eq!(cfg.operands.formals[0].kind, FormalKind::Positional(0));
    }
}

#[test]
fn persisted_invalid_operand_indices_are_rejected() {
    let mut cfg = build_cfg_for_function(
        "javascript",
        "function example(x) { return x.execute(); }",
        "example",
    )
    .unwrap();
    cfg.operands.calls[0].callee = ExpressionId(u32::MAX);
    assert!(bincode::deserialize::<ControlFlowGraph>(&bincode::serialize(&cfg).unwrap()).is_err());
    assert!(
        OperandRange {
            start: u32::MAX,
            len: 2
        }
        .slice(&[1, 2])
        .is_none()
    );
}

#[test]
fn exact_callable_byte_range_selects_the_matching_operand_record() {
    let first = "void example(int value) { value.first(); }";
    let second = "void example(String value) { value.second(); }";
    let source = format!("class Demo {{ {first} {second} }}");
    for (body, expected) in [(first, "first"), (second, "second")] {
        let start = source.find(body).unwrap();
        let cfg = crate::cfg_builder::build_cfg_for_byte_range(
            "java",
            &source,
            "example",
            start,
            start + body.len(),
        )
        .unwrap();
        assert_eq!(
            cfg.operands.span,
            Some(OperandSpan {
                start: start as u32,
                end: (start + body.len()) as u32
            })
        );
        assert_eq!(cfg.operands.calls.len(), 1);
        assert_eq!(name(&cfg.operands, cfg.operands.calls[0].callee), expected);
    }
}

#[test]
fn unsupported_and_unbuilt_operands_are_distinct_from_an_empty_capture() {
    assert_eq!(
        ControlFlowGraph::new().operands.status,
        OperandStatus::NotBuilt
    );
    let unsupported = build_cfg_for_function("c", "void example() {}", "example").unwrap();
    assert_eq!(
        unsupported.operands.status,
        OperandStatus::UnsupportedLanguage
    );
    let empty = build_cfg_for_function("javascript", "function example() {}", "example").unwrap();
    assert_eq!(empty.operands.status, OperandStatus::Captured);
    assert!(empty.operands.calls.is_empty());
}

#[test]
fn aggregate_construction_and_interpolated_calls_are_not_literal_constants() {
    let source = "package demo\nfunc example() X { return X{value: makeValue()} }";
    let cfg = build_cfg_for_function("go", source, "example").unwrap();
    let facts = &cfg.operands;
    let returned = facts.returns[0].values.slice(&facts.inputs).unwrap()[0];
    let ExpressionKind::Construction(type_value) = facts.expressions[returned.0 as usize].kind
    else {
        panic!("aggregate lost its construction role");
    };
    assert_eq!(name(facts, type_value), "X");
    assert_eq!(facts.calls.len(), 1);
    assert_eq!(name(facts, facts.calls[0].callee), "makeValue");

    let cfg = build_cfg_for_function("python", "def example():\n return f'{make()}'\n", "example")
        .unwrap();
    let facts = &cfg.operands;
    assert_eq!(facts.calls.len(), 1);
    let mut pending = facts.returns[0]
        .values
        .slice(&facts.inputs)
        .unwrap()
        .to_vec();
    let mut found_call = false;
    while let Some(id) = pending.pop() {
        let expression = &facts.expressions[id.0 as usize];
        found_call |= matches!(expression.kind, ExpressionKind::CallResult(_));
        pending.extend_from_slice(expression.inputs.slice(&facts.inputs).unwrap());
    }
    assert!(
        found_call,
        "interpolated call disconnected from return expression"
    );
}
