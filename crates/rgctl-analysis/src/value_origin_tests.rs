use super::*;
use crate::call_bindings::ReceiverSupply;
use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use crate::interprocedural_cfg::InterproceduralCFG;
use rgctl_graph::backend::{GraphBackend, MemoryBackend};
use rgctl_graph::schema::{Node, NodeType};

struct Fixture {
    backend: MemoryBackend,
    icfg: InterproceduralCFG,
    ids: Vec<Uuid>,
}
impl Fixture {
    fn new(language: &str, functions: &[(&str, &str)]) -> Self {
        let mut backend = MemoryBackend::new();
        let mut cfgs = HashMap::new();
        let mut ids = Vec::new();
        for (index, (name, source)) in functions.iter().enumerate() {
            let mut node = Node::new(NodeType::Function, *name);
            node.id = Uuid::from_u128(index as u128 + 1);
            let id = node.id;
            backend.insert_node(node).unwrap();
            ids.push(id);
            cfgs.insert(id, build_cfg_for_function(language, source, name).unwrap());
        }
        let icfg = InterproceduralCFG::from_cfg_archive(&backend, cfgs).unwrap();
        Self { backend, icfg, ids }
    }
    fn cfg(&self, index: usize) -> &ControlFlowGraph {
        self.icfg.get_cfg(self.ids[index]).unwrap()
    }
    fn call(&self, index: usize, name: &str) -> CallRef {
        let cfg = self.cfg(index);
        let site = cfg
            .operands
            .calls
            .iter()
            .position(
                |c| match cfg.operands.expressions[c.callee.0 as usize].kind {
                    ExpressionKind::Name(n) | ExpressionKind::Member(n) => {
                        cfg.operands.name(n) == Some(name)
                    }
                    _ => false,
                },
            )
            .unwrap();
        CallRef {
            caller: self.ids[index],
            site: CallSiteId(site as u32),
        }
    }
    fn target(
        &self,
        index: usize,
        name: &str,
        callee: usize,
        receiver: ReceiverSupply,
    ) -> InvocationTarget {
        InvocationTarget {
            call: self.call(index, name),
            callee: self.ids[callee],
            receiver,
        }
    }
    fn allocation(&self, index: usize, name: &str) -> ValueRef {
        let cfg = self.cfg(index);
        if let Some((i,_))=cfg.operands.expressions.iter().enumerate().find(|(_,e)|matches!(e.kind,ExpressionKind::Construction(ty) if matches!(cfg.operands.expressions[ty.0 as usize].kind,ExpressionKind::Name(n) if cfg.operands.name(n)==Some(name)))) {
            return ValueRef{function:self.ids[index],expression:ExpressionId(i as u32)}
        }
        let site = self.call(index, name).site;
        ValueRef {
            function: self.ids[index],
            expression: cfg.operands.calls[site.0 as usize].result,
        }
    }
}
fn seed(value: ValueRef, type_id: u128) -> OriginSeed {
    OriginSeed {
        value,
        type_id: Uuid::from_u128(type_id),
    }
}
fn member(target: InvocationTarget, type_id: u128) -> MemberCandidate {
    MemberCandidate {
        target,
        receiver_type: Uuid::from_u128(type_id),
    }
}

#[test]
fn parameter_receivers_gain_targets_in_six_languages_without_existing_member_calls() {
    for (language, caller, callee, method) in [
        (
            "java",
            "class App { static int exercise(){return consume(new X());} }",
            "class App { static int consume(X value){return value.execute();} }",
            "class X { int execute(){return 100;} }",
        ),
        (
            "javascript",
            "function exercise(){return consume(new X());}",
            "function consume(value){return value.execute();}",
            "class X {execute(){return 100;}}",
        ),
        (
            "typescript",
            "function exercise():number{return consume(new X());}",
            "function consume(value:X):number{return value.execute();}",
            "class X {execute():number{return 100;}}",
        ),
        (
            "python",
            "def exercise():\n return consume(X())\n",
            "def consume(value):\n return value.execute()\n",
            "class X:\n def execute(self):\n  return 100\n",
        ),
        (
            "go",
            "package demo\nfunc exercise() int {return consume(X{})}",
            "package demo\nfunc consume(value X) int {return value.execute()}",
            "package demo\ntype X struct{}\nfunc (self X) execute() int {return 100}",
        ),
        (
            "rust",
            "fn exercise()->i32{return consume(X{});}",
            "fn consume(value:X)->i32{return value.execute();}",
            "impl X {fn execute(&self)->i32{return 100;}}",
        ),
    ] {
        let fixture = Fixture::new(
            language,
            &[
                ("exercise", caller),
                ("consume", callee),
                ("execute", method),
            ],
        );
        let target = fixture.target(1, "execute", 2, ReceiverSupply::Member);
        let output = OriginAnalysis::build(
            &fixture.icfg,
            &[seed(fixture.allocation(0, "X"), 100)],
            &[fixture.target(0, "consume", 1, ReceiverSupply::None)],
            &[member(target, 100)],
            Default::default(),
        );
        assert!(
            output.receiver_targets.iter().any(|r| r.target == target),
            "{language}: {:?}",
            output.diagnostics
        );
    }
}
#[test]
fn unused_allocation_is_not_a_receiver_candidate_in_the_single_call_context() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(){const unused=new X(); const actual=new W(); return consume(actual);}",
            ),
            (
                "consume",
                "function consume(value){const alias=value; return alias.execute();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
            ("execute", "class W{execute(){return 200;}}"),
        ],
    );
    let x = f.target(1, "execute", 2, ReceiverSupply::Member);
    let w = f.target(1, "execute", 3, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[
            seed(f.allocation(0, "X"), 100),
            seed(f.allocation(0, "W"), 101),
        ],
        &[f.target(0, "consume", 1, ReceiverSupply::None)],
        &[member(x, 100), member(w, 101)],
        Default::default(),
    );
    assert!(output.receiver_targets.iter().any(|r| r.target == w));
    assert!(!output.receiver_targets.iter().any(|r| r.target == x));
}
#[test]
fn conditional_factory_keeps_both_receiver_origins() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(flag){return make(flag).execute();}",
            ),
            (
                "make",
                "function make(flag){if(flag){return new X();}return new W();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
            ("execute", "class W{execute(){return 200;}}"),
        ],
    );
    let x = f.target(0, "execute", 2, ReceiverSupply::Member);
    let w = f.target(0, "execute", 3, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[
            seed(f.allocation(1, "X"), 100),
            seed(f.allocation(1, "W"), 101),
        ],
        &[f.target(0, "make", 1, ReceiverSupply::None)],
        &[member(x, 100), member(w, 101)],
        Default::default(),
    );
    assert!(output.receiver_targets.iter().any(|r| r.target == x));
    assert!(output.receiver_targets.iter().any(|r| r.target == w));
}
#[test]
fn discovered_call_bindings_replay_origins_that_arrived_before_the_edge() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(){return new Factory().make().execute();}",
            ),
            ("make", "class Factory{make(){return new X();}}"),
            ("execute", "class X{execute(){return 100;}}"),
        ],
    );
    let make = f.target(0, "make", 1, ReceiverSupply::Member);
    let execute = f.target(0, "execute", 2, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[
            seed(f.allocation(0, "Factory"), 102),
            seed(f.allocation(1, "X"), 100),
        ],
        &[],
        &[member(make, 102), member(execute, 100)],
        Default::default(),
    );
    assert!(output.receiver_targets.iter().any(|r| r.target == make));
    assert!(output.receiver_targets.iter().any(|r| r.target == execute));
}
#[test]
fn recursion_reaches_a_bounded_fixed_point() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(){return consume(new X()).execute();}",
            ),
            (
                "consume",
                "function consume(value){if(globalThis.again){return consume(value);}return value;}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
        ],
    );
    let execute = f.target(0, "execute", 2, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[seed(f.allocation(0, "X"), 100)],
        &[
            f.target(0, "consume", 1, ReceiverSupply::None),
            f.target(1, "consume", 1, ReceiverSupply::None),
        ],
        &[member(execute, 100)],
        OriginLimits {
            max_steps: 1_000,
            ..Default::default()
        },
    );
    assert!(output.receiver_targets.iter().any(|r| r.target == execute));
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|d| matches!(d, OriginDiagnostic::LimitReached))
    );
}
#[test]
fn rest_membership_is_diagnosed_instead_of_aliasing_container_and_element() {
    let f = Fixture::new(
        "javascript",
        &[
            ("exercise", "function exercise(){return consume(new X());}"),
            (
                "consume",
                "function consume(...values){return values.execute();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
        ],
    );
    let execute = f.target(1, "execute", 2, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[seed(f.allocation(0, "X"), 100)],
        &[f.target(0, "consume", 1, ReceiverSupply::None)],
        &[member(execute, 100)],
        Default::default(),
    );
    assert!(!output.receiver_targets.iter().any(|r| r.target == execute));
    assert!(output.diagnostics.iter().any(|d| matches!(
        d,
        OriginDiagnostic::UnsupportedTransfer {
            kind: TransferKind::RestElement { .. },
            ..
        }
    )));
}
#[test]
fn global_limits_and_invalid_invocations_are_explicit() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(value){return value.execute();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
        ],
    );
    let receiver = f.cfg(0).operands.formals[0].value;
    let seed = seed(
        ValueRef {
            function: f.ids[0],
            expression: receiver,
        },
        100,
    );
    let target = f.target(0, "execute", 1, ReceiverSupply::Member);
    for limits in [
        OriginLimits {
            max_functions: 0,
            ..Default::default()
        },
        OriginLimits {
            max_facts: 0,
            ..Default::default()
        },
        OriginLimits {
            max_transfers: 0,
            ..Default::default()
        },
        OriginLimits {
            max_steps: 0,
            ..Default::default()
        },
        OriginLimits {
            max_targets: 0,
            ..Default::default()
        },
    ] {
        let output = OriginAnalysis::build(&f.icfg, &[seed], &[], &[member(target, 100)], limits);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| matches!(d, OriginDiagnostic::LimitReached)),
            "{:?}",
            output.diagnostics
        );
    }
    let bad = InvocationTarget {
        call: CallRef {
            caller: f.ids[0],
            site: CallSiteId(999),
        },
        ..target
    };
    let output = OriginAnalysis::build(&f.icfg, &[], &[], &[member(bad, 100)], Default::default());
    assert!(
        output
            .diagnostics
            .iter()
            .any(|d| matches!(d, OriginDiagnostic::InvalidCall(_)))
    );
}
#[test]
fn borrowed_archive_view_matches_owned_records() {
    use crate::cfg_pdg_archive::{CfgPdgArchive, CfgPdgRecord};
    use std::sync::Arc;
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(value){return value.execute();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
        ],
    );
    let seeds = [seed(
        ValueRef {
            function: f.ids[0],
            expression: f.cfg(0).operands.formals[0].value,
        },
        100,
    )];
    let members = [member(
        f.target(0, "execute", 1, ReceiverSupply::Member),
        100,
    )];
    let owned = OriginAnalysis::build(&f.icfg, &seeds, &[], &members, Default::default());
    let mut archive = CfgPdgArchive::default();
    for (&id, cfg) in &f.icfg.function_cfgs {
        archive.insert(CfgPdgRecord {
            function_id: id,
            code_hash: "fixture".into(),
            function_name: "fixture".into(),
            file_path: None,
            cfg: Arc::new(cfg.clone()),
            pdg: Arc::new(ProgramDependenceGraph::build(cfg, b"").unwrap()),
        });
    }
    let archive: CfgPdgArchive =
        bincode::deserialize(&bincode::serialize(&archive).unwrap()).unwrap();
    let view = archive.interprocedural_cfg_view(&f.backend).unwrap();
    let borrowed = OriginAnalysis::build(&view, &seeds, &[], &members, Default::default());
    assert_eq!(
        serde_json::to_value(owned).unwrap(),
        serde_json::to_value(borrowed).unwrap()
    );
}

#[test]
fn known_defined_actual_does_not_receive_an_inactive_default_origin() {
    let f = Fixture::new(
        "javascript",
        &[
            ("exercise", "function exercise(){return consume(new W());}"),
            (
                "consume",
                "function consume(value=new X()){return value.execute();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
            ("execute", "class W{execute(){return 200;}}"),
        ],
    );
    let x = f.target(1, "execute", 2, ReceiverSupply::Member);
    let w = f.target(1, "execute", 3, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[
            seed(f.allocation(1, "X"), 100),
            seed(f.allocation(0, "W"), 101),
        ],
        &[f.target(0, "consume", 1, ReceiverSupply::None)],
        &[member(x, 100), member(w, 101)],
        Default::default(),
    );
    assert!(output.receiver_targets.iter().any(|r| r.target == w));
    assert!(!output.receiver_targets.iter().any(|r| r.target == x));
}
#[test]
fn merged_callers_remain_explicitly_conservative() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "first",
                "function first(){return consume(new X()).execute();}",
            ),
            (
                "second",
                "function second(){return consume(new W()).execute();}",
            ),
            ("consume", "function consume(value){return value;}"),
            ("execute", "class X{execute(){return 100;}}"),
            ("execute", "class W{execute(){return 200;}}"),
        ],
    );
    let x = f.target(1, "execute", 3, ReceiverSupply::Member);
    let w = f.target(1, "execute", 4, ReceiverSupply::Member);
    let output = OriginAnalysis::build(
        &f.icfg,
        &[
            seed(f.allocation(0, "X"), 100),
            seed(f.allocation(1, "W"), 101),
        ],
        &[
            f.target(0, "consume", 2, ReceiverSupply::None),
            f.target(1, "consume", 2, ReceiverSupply::None),
        ],
        &[member(x, 100), member(w, 101)],
        Default::default(),
    );
    assert!(matches!(
        output.context_policy,
        OriginContextPolicy::MergedPossibleOrigins
    ));
    assert!(output.receiver_targets.iter().any(|r| r.target == w));
    // This first solver intentionally over-warns across the shared formal.
    assert!(output.receiver_targets.iter().any(|r| r.target == x));
}
#[test]
fn input_order_does_not_change_serialized_origin_results() {
    let f = Fixture::new(
        "javascript",
        &[
            (
                "exercise",
                "function exercise(flag){return make(flag).execute();}",
            ),
            (
                "make",
                "function make(flag){if(flag){return new X();}return new W();}",
            ),
            ("execute", "class X{execute(){return 100;}}"),
            ("execute", "class W{execute(){return 200;}}"),
        ],
    );
    let mut seeds = vec![
        seed(f.allocation(1, "X"), 100),
        seed(f.allocation(1, "W"), 101),
    ];
    let targets = [f.target(0, "make", 1, ReceiverSupply::None)];
    let mut members = vec![
        member(f.target(0, "execute", 2, ReceiverSupply::Member), 100),
        member(f.target(0, "execute", 3, ReceiverSupply::Member), 101),
    ];
    let first = OriginAnalysis::build(&f.icfg, &seeds, &targets, &members, Default::default());
    seeds.reverse();
    members.reverse();
    let second = OriginAnalysis::build(&f.icfg, &seeds, &targets, &members, Default::default());
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
}
