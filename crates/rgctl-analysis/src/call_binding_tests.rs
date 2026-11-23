use super::*;
use crate::cfg_builder::build_cfg_for_function;
use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
use crate::interprocedural_cfg::InterproceduralCFG;
use crate::pdg::ProgramDependenceGraph;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};
use std::sync::Arc;

struct Fixture {
    icfg: InterproceduralCFG,
    backend: MemoryBackend,
    caller: Uuid,
    callee: Uuid,
    sources: HashMap<Uuid, String>,
}

impl Fixture {
    fn new(language: &str, caller: &str, callee: &str) -> Self {
        Self::named(language, caller, callee, "consume")
    }

    fn named(language: &str, caller: &str, callee: &str, callee_name: &str) -> Self {
        let caller_cfg = build_cfg_for_function(language, caller, "caller").unwrap();
        let callee_cfg = build_cfg_for_function(language, callee, callee_name).unwrap();
        let mut backend = MemoryBackend::new();
        let caller_node = Node::new(NodeType::Function, "caller");
        let callee_node = Node::new(NodeType::Function, callee_name);
        let caller_id = caller_node.id;
        let callee_id = callee_node.id;
        backend.insert_node(caller_node).unwrap();
        backend.insert_node(callee_node).unwrap();
        // Deliberately no Calls edge: target selection is supplied explicitly,
        // and binding must not require an unresolved edge to exist already.
        let icfg = InterproceduralCFG::from_cfg_archive(
            &backend,
            HashMap::from([(caller_id, caller_cfg), (callee_id, callee_cfg)]),
        )
        .unwrap();
        Self {
            icfg,
            backend,
            caller: caller_id,
            callee: callee_id,
            sources: HashMap::from([(caller_id, caller.into()), (callee_id, callee.into())]),
        }
    }

    fn target(&self, site: u32, receiver: ReceiverSupply) -> InvocationTarget {
        InvocationTarget {
            call: CallRef {
                caller: self.caller,
                site: CallSiteId(site),
            },
            callee: self.callee,
            receiver,
        }
    }

    fn formal_name(&self, value: ValueRef) -> &str {
        let facts = &self.icfg.get_cfg(value.function).unwrap().operands;
        let ExpressionKind::Formal(id) = facts.expressions[value.expression.0 as usize].kind else {
            panic!("not a formal target");
        };
        facts
            .operand_name(facts.formals[id.0 as usize].name)
            .unwrap()
    }

    fn value_name(&self, source: BindingSource) -> &str {
        let BindingSource::Expression(value) = source else {
            panic!("expected expression source");
        };
        let facts = &self.icfg.get_cfg(value.function).unwrap().operands;
        let ExpressionKind::Name(name) = facts.expressions[value.expression.0 as usize].kind else {
            panic!("not a name occurrence");
        };
        facts.operand_name(name).unwrap()
    }
}

#[test]
fn actual_and_return_edges_are_native_and_position_specific_in_six_languages() {
    for (language, caller, callee) in [
        (
            "java",
            "class App { static X caller(X a, X b) { return consume(a,b); } }",
            "class Dep { static X consume(X first, X second) { return second; } }",
        ),
        (
            "javascript",
            "function caller(a,b) { return consume(a,b); }",
            "function consume(first,second) { return second; }",
        ),
        (
            "typescript",
            "function caller(a:X,b:X) { return consume(a,b); }",
            "function consume(first:X,second:X) { return second; }",
        ),
        (
            "python",
            "def caller(a,b):\n return consume(a,b)\n",
            "def consume(first,second):\n return second\n",
        ),
        (
            "go",
            "package demo\nfunc caller(a X,b X) X {return consume(a,b)}",
            "package demo\nfunc consume(first X,second X) X {return second}",
        ),
        (
            "rust",
            "fn caller(a:X,b:X)->X {consume(a,b)}",
            "fn consume(first:X,second:X)->X {second}",
        ),
    ] {
        let f = Fixture::new(language, caller, callee);
        let mut query = CallBindings::new(&f.icfg);
        let binding = query
            .bind_invocation(f.target(0, ReceiverSupply::None))
            .unwrap();
        assert!(
            binding
                .diagnostics
                .slice(&query.diagnostics)
                .unwrap()
                .is_empty(),
            "{language}: {:?}",
            query.diagnostics
        );
        let edges = binding.transfers.slice(&query.transfers).unwrap();
        let actuals: Vec<_> = edges
            .iter()
            .filter(|e| e.kind == TransferKind::ActualToFormal)
            .map(|e| (f.value_name(e.from), f.formal_name(e.to)))
            .collect();
        assert_eq!(actuals, [("a", "first"), ("b", "second")], "{language}");
        let returned = edges
            .iter()
            .find(|e| matches!(e.kind, TransferKind::ReturnToResult { position: 0 }))
            .unwrap();
        assert_eq!(f.value_name(returned.from), "second", "{language}");
        assert_eq!(
            returned.to,
            ValueRef {
                function: f.caller,
                expression: f.icfg.get_cfg(f.caller).unwrap().operands.calls[0].result
            }
        );
    }
}

#[test]
fn bound_receivers_do_not_shift_ordinary_actuals_in_six_languages() {
    for (language, caller, callee, receiver_name) in [
        (
            "java",
            "class App { static int caller(Demo obj,int value) {return obj.consume(value);} }",
            "class Demo { int consume(int value) {return value;} }",
            "this",
        ),
        (
            "javascript",
            "function caller(obj,value) {return obj.consume(value);}",
            "class Demo {consume(value){return value;}}",
            "this",
        ),
        (
            "typescript",
            "function caller(obj:Demo,value:number){return obj.consume(value);}",
            "class Demo {consume(value:number){return value;}}",
            "this",
        ),
        (
            "python",
            "def caller(obj,value):\n return obj.consume(value)\n",
            "class Demo:\n def consume(self,value):\n  return value\n",
            "self",
        ),
        (
            "go",
            "package demo\nfunc caller(obj *Demo,value int) int{return obj.consume(value)}",
            "package demo\nfunc (s *Demo) consume(value int) int{return value}",
            "s",
        ),
        (
            "rust",
            "fn caller(obj:&Demo,value:i32)->i32{obj.consume(value)}",
            "impl Demo {fn consume(&self,value:i32)->i32{return value;}}",
            "self",
        ),
    ] {
        let f = Fixture::new(language, caller, callee);
        let mut query = CallBindings::new(&f.icfg);
        let binding = query
            .bind_invocation(f.target(0, ReceiverSupply::Member))
            .unwrap();
        assert!(
            binding
                .diagnostics
                .slice(&query.diagnostics)
                .unwrap()
                .is_empty(),
            "{language}: {:?}",
            query.diagnostics
        );
        let edges = binding.transfers.slice(&query.transfers).unwrap();
        let receiver = edges
            .iter()
            .find(|e| e.kind == TransferKind::ReceiverToFormal)
            .unwrap();
        assert_eq!(f.value_name(receiver.from), "obj");
        assert_eq!(f.formal_name(receiver.to), receiver_name);
        let actual = edges
            .iter()
            .find(|e| e.kind == TransferKind::ActualToFormal)
            .unwrap();
        assert_eq!(
            (f.value_name(actual.from), f.formal_name(actual.to)),
            ("value", "value")
        );
    }
}

#[test]
fn unbound_python_method_and_go_method_expression_keep_explicit_receivers() {
    let f = Fixture::new(
        "python",
        "def caller(obj,value):\n return Demo.consume(obj,value)\n",
        "class Demo:\n def consume(self,value):\n  return value\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    let actuals: Vec<_> = binding
        .transfers
        .slice(&query.transfers)
        .unwrap()
        .iter()
        .filter(|e| e.kind == TransferKind::ActualToFormal)
        .map(|e| (f.value_name(e.from), f.formal_name(e.to)))
        .collect();
    assert_eq!(actuals, [("obj", "self"), ("value", "value")]);

    let f = Fixture::new(
        "go",
        "package demo\nfunc caller(obj *Demo,value int) int{return (*Demo).consume(obj,value)}",
        "package demo\nfunc (s *Demo) consume(value int) int{return value}",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::FirstArgument))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .is_empty()
    );
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    let receiver = edges
        .iter()
        .find(|e| e.kind == TransferKind::ReceiverToFormal)
        .unwrap();
    assert_eq!(
        (f.value_name(receiver.from), f.formal_name(receiver.to)),
        ("obj", "s")
    );
    let actual = edges
        .iter()
        .find(|e| e.kind == TransferKind::ActualToFormal)
        .unwrap();
    assert_eq!(
        (f.value_name(actual.from), f.formal_name(actual.to)),
        ("value", "value")
    );
}

#[test]
fn python_named_defaults_and_positional_only_keyword_rest_are_distinct() {
    let f = Fixture::new(
        "python",
        "def caller(a,b):\n return consume(second=b,first=a)\n",
        "def consume(first,second):\n return second\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    let actuals: Vec<_> = binding
        .transfers
        .slice(&query.transfers)
        .unwrap()
        .iter()
        .filter(|e| e.kind == TransferKind::ActualToFormal)
        .map(|e| (f.value_name(e.from), f.formal_name(e.to)))
        .collect();
    assert_eq!(actuals, [("b", "second"), ("a", "first")]);

    let f = Fixture::new(
        "python",
        "def caller(a,b):\n return consume(a,first=b)\n",
        "def consume(first,/,second=1,**rest):\n return first\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .is_empty(),
        "{:?}",
        query.diagnostics
    );
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    assert!(
        edges
            .iter()
            .any(|e| matches!(e.kind, TransferKind::KeywordElement { .. })
                && f.formal_name(e.to) == "rest"
                && f.value_name(e.from) == "b")
    );
    assert!(edges.iter().any(|e| matches!(
        e.kind,
        TransferKind::DefaultToFormal {
            evaluation: DefaultEvaluation::Definition,
            conditional: false
        }
    ) && f.formal_name(e.to) == "second"));
}

#[test]
fn unknown_ecma_actual_retains_the_default_undefined_alternative() {
    for (actual, conditional) in [("", Some(false)), ("maybe", Some(true)), ("7", None)] {
        let f = Fixture::new(
            "javascript",
            &format!("function caller(maybe){{return consume({actual});}}"),
            "function consume(value=new X()){return value;}",
        );
        let mut query = CallBindings::new(&f.icfg);
        let binding = query
            .bind_invocation(f.target(0, ReceiverSupply::None))
            .unwrap();
        let default = binding
            .transfers
            .slice(&query.transfers)
            .unwrap()
            .iter()
            .find_map(|e| match e.kind {
                TransferKind::DefaultToFormal {
                    evaluation,
                    conditional,
                } => {
                    assert_eq!(evaluation, DefaultEvaluation::Invocation);
                    Some(conditional)
                }
                _ => None,
            });
        assert_eq!(default, conditional, "actual={actual}");
    }
}

#[test]
fn unknown_spreads_retain_candidate_formals_and_report_uncertainty() {
    for (actual, expected_diagnostic) in [
        ("*values", BindingDiagnostic::PositionalSpread),
        ("**values", BindingDiagnostic::KeywordSpread),
    ] {
        let f = Fixture::new(
            "python",
            &format!("def caller(values):\n return consume({actual})\n"),
            "def consume(first,second=1,**rest):\n return first\n",
        );
        let mut query = CallBindings::new(&f.icfg);
        let binding = query
            .bind_invocation(f.target(0, ReceiverSupply::None))
            .unwrap();
        let diagnostics = binding.diagnostics.slice(&query.diagnostics).unwrap();
        assert!(diagnostics.contains(&expected_diagnostic));
        assert!(
            !diagnostics.contains(&BindingDiagnostic::MissingArgument),
            "{actual}: {diagnostics:?}"
        );
        let edges = binding.transfers.slice(&query.transfers).unwrap();
        assert!(
            edges
                .iter()
                .any(|e| e.kind == TransferKind::PossibleSpreadToFormal
                    && f.formal_name(e.to) == "first")
        );
        assert!(edges.iter().any(|e| matches!(
            e.kind,
            TransferKind::DefaultToFormal {
                conditional: true,
                ..
            }
        ) && f.formal_name(e.to) == "second"));
    }
}

#[test]
fn rest_elements_are_membership_edges_not_whole_object_aliases() {
    let f = Fixture::new(
        "javascript",
        "function caller(a,b){return consume(a,b);}",
        "function consume(...values){return values;}",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    assert!(
        edges.iter().any(
            |e| e.kind == TransferKind::RestElement { index: 0 } && f.value_name(e.from) == "a"
        )
    );
    assert!(
        edges.iter().any(
            |e| e.kind == TransferKind::RestElement { index: 1 } && f.value_name(e.from) == "b"
        )
    );
    assert!(!edges.iter().any(|e| e.kind == TransferKind::ActualToFormal));
}

#[test]
fn return_slots_and_repeated_call_results_keep_their_invocation_context() {
    let f = Fixture::new(
        "go",
        "package demo\nfunc caller(a X,b X)(X,X){return consume(a,b)}",
        "package demo\nfunc consume(first X,second X)(X,X){return first,second}",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    let slots: Vec<_> = binding
        .transfers
        .slice(&query.transfers)
        .unwrap()
        .iter()
        .filter_map(|e| match e.kind {
            TransferKind::ReturnToResult { position } => Some((position, f.value_name(e.from))),
            _ => None,
        })
        .collect();
    assert_eq!(slots, [(0, "first"), (1, "second")]);

    let f = Fixture::new(
        "javascript",
        "function caller(a,b){return consume(a)+consume(b);}",
        "function consume(value){return value;}",
    );
    let mut query = CallBindings::new(&f.icfg);
    let first = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    let second = query
        .bind_invocation(f.target(1, ReceiverSupply::None))
        .unwrap();
    for (binding, expected) in [(first, "a"), (second, "b")] {
        let edges = binding.transfers.slice(&query.transfers).unwrap();
        assert!(edges.iter().any(|e|e.kind==TransferKind::ActualToFormal && f.value_name(e.from)==expected));
        let result = f.icfg.get_cfg(f.caller).unwrap().operands.calls
            [binding.target.call.site.0 as usize]
            .result;
        assert!(
            edges
                .iter()
                .filter(|e| matches!(e.kind, TransferKind::ReturnToResult { .. }))
                .all(|e| e.to.expression == result)
        );
    }
}

#[test]
fn constructor_result_supplies_this_without_void_return_overwriting_it() {
    let f = Fixture::named(
        "java",
        "class App {static Demo caller(int value){return new Demo(value);}}",
        "class Demo {Demo(int value){}}",
        "Demo",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::Constructed))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .is_empty()
    );
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    let receiver = edges
        .iter()
        .find(|e| e.kind == TransferKind::ReceiverToFormal)
        .unwrap();
    assert_eq!(
        receiver.from,
        BindingSource::Expression(ValueRef {
            function: f.caller,
            expression: f.icfg.get_cfg(f.caller).unwrap().operands.calls[0].result
        })
    );
    assert_eq!(f.formal_name(receiver.to), "this");
    assert!(!edges.iter().any(|e| matches!(
        e.kind,
        TransferKind::ReturnToResult { .. } | TransferKind::FallthroughToResult
    )));
}

#[test]
fn bare_and_fallthrough_returns_produce_language_defined_values() {
    for (language, caller, callee, expected) in [
        (
            "python",
            "def caller():\n return consume()\n",
            "def consume():\n return\n",
            ImplicitValue::None,
        ),
        (
            "javascript",
            "function caller(){return consume();}",
            "function consume(){}",
            ImplicitValue::Undefined,
        ),
    ] {
        let f = Fixture::new(language, caller, callee);
        let mut query = CallBindings::new(&f.icfg);
        let binding = query
            .bind_invocation(f.target(0, ReceiverSupply::None))
            .unwrap();
        assert!(
            binding
                .transfers
                .slice(&query.transfers)
                .unwrap()
                .iter()
                .any(|e| e.from == BindingSource::Implicit(expected)),
            "{language}: {:?}",
            query.transfers
        );
    }
}

#[test]
fn unsupported_formals_never_narrow_all_actuals_to_a_wrong_position() {
    let f = Fixture::new(
        "javascript",
        "function caller(a,b){return consume(a,b);}",
        "function consume({ignored},value){return value;}",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::PartialOperands)
    );
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    assert!(
        edges
            .iter()
            .any(|e| e.kind == TransferKind::PossibleSpreadToFormal
                && f.value_name(e.from) == "b"
                && f.formal_name(e.to) == "value")
    );
}

#[test]
fn missing_source_arity_errors_and_limits_cannot_be_successful_empty_mappings() {
    let f = Fixture::new(
        "python",
        "def caller(a):\n return consume(a)\n",
        "def consume(first,second):\n return second\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::MissingArgument)
    );
    assert!(
        query
            .bind_invocation(f.target(u32::MAX, ReceiverSupply::None))
            .is_err()
    );
    let mut missing = f.target(0, ReceiverSupply::None);
    missing.callee = Uuid::new_v4();
    let binding = query.bind_invocation(missing).unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::MissingCallee)
    );
    let mut limited = CallBindings::with_limits(
        &f.icfg,
        BindingLimits {
            max_transfers_per_call: 1,
        },
    );
    let binding = limited
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(binding.transfers.len <= 1);
    assert!(
        binding
            .diagnostics
            .slice(&limited.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::LimitReached)
    );
}

#[test]
fn archive_view_produces_the_same_transfer_mapping_without_source_access() {
    let f = Fixture::new(
        "javascript",
        "function caller(a,b){return consume(a,b);}",
        "function consume(first,second){return second;}",
    );
    let mut archive = CfgPdgArchive::default();
    for (&id, cfg) in &f.icfg.function_cfgs {
        let source = &f.sources[&id];
        archive.insert(CfgPdgRecord {
            function_id: id,
            code_hash: rgctl_graph::code_index::hash_code(source),
            function_name: if id == f.caller {
                "caller".into()
            } else {
                "consume".into()
            },
            file_path: None,
            cfg: Arc::new(cfg.clone()),
            pdg: Arc::new(ProgramDependenceGraph::build(cfg, source.as_bytes()).unwrap()),
        });
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = CfgPdgArchive::default_path(tmp.path());
    archive.write_to_path(&path).unwrap();
    let loaded = CfgPdgArchive::load_from_path(&path).unwrap();
    let view = loaded.interprocedural_cfg_view(&f.backend).unwrap();
    let mut fresh = f.icfg.call_bindings();
    let mut cached = view.call_bindings();
    let target = f.target(0, ReceiverSupply::None);
    assert_eq!(
        fresh.bind_invocation(target).unwrap(),
        cached.bind_invocation(target).unwrap()
    );
    assert_eq!(fresh.transfers, cached.transfers);
    assert_eq!(fresh.diagnostics, cached.diagnostics);
}

#[test]
fn duplicate_keywords_class_receiver_and_invalid_operands_remain_diagnosed() {
    let f = Fixture::new(
        "python",
        "def caller(a,b):\n return consume(a,first=b)\n",
        "def consume(first):\n return first\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::DuplicateArgument)
    );

    let f = Fixture::new(
        "python",
        "def caller(obj,value):\n return obj.consume(value)\n",
        "class Demo:\n @classmethod\n def consume(cls,value):\n  return value\n",
    );
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::Member))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::ClassReceiver)
    );
    assert!(
        binding
            .transfers
            .slice(&query.transfers)
            .unwrap()
            .iter()
            .any(|e| e.kind == TransferKind::ActualToFormal && f.formal_name(e.to) == "value")
    );

    let mut f = Fixture::new(
        "javascript",
        "function caller(a){return consume(a);}",
        "function consume(value){return value;}",
    );
    f.icfg
        .function_cfgs
        .get_mut(&f.caller)
        .unwrap()
        .operands
        .calls[0]
        .arguments
        .start = u32::MAX;
    let mut query = CallBindings::new(&f.icfg);
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::None))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .contains(&BindingDiagnostic::InvalidOperand)
    );
}

#[test]
fn bound_python_receiver_is_the_first_element_of_a_rest_only_signature() {
    let f = Fixture::new(
        "python",
        "def caller(obj,value):\n return obj.consume(value)\n",
        "class Demo:\n def consume(*values):\n  return values\n",
    );
    let mut query = f.icfg.call_bindings();
    let binding = query
        .bind_invocation(f.target(0, ReceiverSupply::Member))
        .unwrap();
    assert!(
        binding
            .diagnostics
            .slice(&query.diagnostics)
            .unwrap()
            .is_empty(),
        "{:?}",
        query.diagnostics
    );
    let edges = binding.transfers.slice(&query.transfers).unwrap();
    assert!(
        edges
            .iter()
            .any(|e| e.kind == TransferKind::RestElement { index: 0 }
                && f.value_name(e.from) == "obj")
    );
    assert!(edges.iter().any(
        |e| e.kind == TransferKind::RestElement { index: 1 } && f.value_name(e.from) == "value"
    ));
}

#[test]
fn repeated_binding_requests_reuse_pools_and_keep_deterministic_keyword_edges() {
    let f = Fixture::new(
        "python",
        "def caller(values):\n return consume(**values)\n",
        "def consume(first,second=1,*,third=2):\n return first\n",
    );
    let mut query = f.icfg.call_bindings();
    let target = f.target(0, ReceiverSupply::None);
    let first = query.bind_invocation(target).unwrap();
    let lengths = (query.transfers.len(), query.diagnostics.len());
    assert_eq!(first, query.bind_invocation(target).unwrap());
    assert_eq!(lengths, (query.transfers.len(), query.diagnostics.len()));
    let mut second = f.icfg.call_bindings();
    second.bind_invocation(target).unwrap();
    assert_eq!(query.transfers, second.transfers);
    assert_eq!(query.diagnostics, second.diagnostics);
}

#[test]
fn selected_binding_formal_id_joins_native_pdg_without_name_matching() {
    let fixture = Fixture::new(
        "javascript",
        "function caller(first, second) { return consume(second, first); }",
        "function consume(left, right) { const alias = right; return alias.execute(); }",
    );
    let mut query = fixture.icfg.call_bindings();
    let binding = query
        .bind_invocation(fixture.target(0, ReceiverSupply::None))
        .unwrap();
    let target = binding
        .transfers
        .slice(query.transfers())
        .unwrap()
        .iter()
        .find(|edge| {
            edge.kind == TransferKind::ActualToFormal && fixture.value_name(edge.from) == "first"
        })
        .unwrap()
        .to;
    let cfg = fixture.icfg.get_cfg(target.function).unwrap();
    let ExpressionKind::Formal(id) = cfg.operands.expressions[target.expression.0 as usize].kind
    else {
        panic!("binding did not end at a formal");
    };
    assert_eq!(id, FormalId(1));
    let pdg =
        ProgramDependenceGraph::build(cfg, fixture.sources[&fixture.callee].as_bytes()).unwrap();
    let entry = pdg
        .formal_definitions()
        .find(|(formal, _)| *formal == id)
        .unwrap()
        .1;
    let alias = pdg
        .nodes
        .values()
        .find(|n| n.statement.defines("alias"))
        .unwrap();
    let returned = pdg
        .nodes
        .values()
        .find(|n| n.statement.kind == crate::cfg::StatementKind::Return)
        .unwrap();
    assert!(pdg.data_deps.iter().any(|d| d.from == entry.id
        && d.to == alias.id
        && d.dep_type == crate::pdg::DataDepType::Flow));
    assert!(pdg.data_deps.iter().any(|d| d.from == alias.id
        && d.to == returned.id
        && d.dep_type == crate::pdg::DataDepType::Flow));
}
