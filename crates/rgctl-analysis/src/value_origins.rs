//! Bounded possible-value origins through native local and invocation transfers.
//! Resolution inputs use canonical IDs; the solver never parses or matches names.
use crate::call_bindings::{
    BindingDiagnostic, BindingSource, CallBindings, CallRef, InvocationTarget, ReceiverSupply,
    TransferKind, ValueRef,
};
use crate::interprocedural_cfg::InterproceduralCfgAccess;
use crate::local_value_flow::{
    LocalFlowDiagnostic, LocalFlowLimits, LocalTransferKind, LocalValueFlow,
};
use crate::operands::*;
use crate::pdg::ProgramDependenceGraph;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[cfg(test)]
#[path = "value_origin_tests.rs"]
mod tests;

#[path = "callable_origins.rs"]
mod callable_origins;
#[path = "deferred_origins.rs"]
mod deferred_origins;
use callable_origins::CallableState;
pub use callable_origins::{CallableCandidate, CallableValue, OriginRequirement};

#[path = "lookup_origins.rs"]
mod lookup_origins;
use lookup_origins::LookupState;
pub use lookup_origins::{
    CallableMemberValue, ConstructionEvidence, ConstructorCandidate, ConstructorValue,
    callable_member_values,
};

/// Dense query-local origin index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginId(pub u32);
/// A provider-supplied possible type at an exact native value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginSeed {
    /// Native producer (allocation or another explicitly modeled source).
    pub value: ValueRef,
    /// Canonical possible type from graph resolution, or no nominal type for an
    /// opaque aggregate identity. Missing type never selects a member target.
    pub type_id: Option<Uuid>,
}
/// Type-compatible canonical member target; becomes a call only when its receiver carries that type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemberCandidate {
    /// Exact invocation/target and established receiver supply convention.
    pub target: InvocationTarget,
    /// Canonical receiver type accepted by the resolver for this candidate.
    pub receiver_type: Uuid,
}
/// Origin set for one exact expression, qualified by callable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueOrigins {
    /// Native value.
    pub value: ValueRef,
    /// Query-local origin indices.
    pub origins: Vec<OriginId>,
}
/// A receiver-supported possible invocation, with origin provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiverTarget {
    /// Selected canonical possible target.
    pub target: InvocationTarget,
    /// Origin reaching this exact receiver.
    pub origin: OriginId,
}
/// Provenance of a value-preserving edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginTransferCause {
    /// Native scoped local transfer.
    Local(LocalTransferKind),
    /// A modeled deferred runner exposes the possible result of its argument.
    DeferredCall {
        /// Exact native runner invocation; builtin source spelling is conservative.
        call: CallRef,
    },
    /// A supported iterator reader exposes a yielded or terminal value.
    DeferredYield {
        /// Opaque source generator identity, distinct from yielded values.
        container: OriginId,
        /// Exact native reader expression.
        read: ValueRef,
        /// This value came from terminal return, rather than an explicit yield.
        terminal: bool,
    },
    /// A possible stored element reaches an indexed read on the same aggregate.
    Element {
        /// Opaque aggregate or object origin, distinct from the stored value.
        container: OriginId,
        /// Exact indexed read.
        read: ValueRef,
    },
    /// A stored value reaches a named member read on the same possible object.
    Field {
        /// Possible object allocation/provider origin used for the store/load join.
        object: OriginId,
        /// Exact member read; its native operand supplies the member identity.
        read: ValueRef,
    },
    /// Native selected-invocation binding.
    Invocation {
        /// Invocation identity and target.
        target: InvocationTarget,
        /// Argument, receiver, default or return convention.
        kind: TransferKind,
    },
}
/// One explainable value-preserving transfer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OriginTransfer {
    /// Producer.
    pub from: ValueRef,
    /// Consumer.
    pub to: ValueRef,
    /// Native transfer provenance.
    pub cause: OriginTransferCause,
}
/// Explicit context policy; this query retains all possible caller alternatives.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OriginContextPolicy {
    /// Shared formal values merge callers; this can over-warn and cannot qualify context exclusions.
    MergedPossibleOrigins,
}
/// Missing information or bounded-query coverage evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OriginDiagnostic {
    /// Missing procedural record.
    MissingFunction(Uuid),
    /// Invalid source-local expression or invocation reference.
    InvalidValue(ValueRef),
    /// Invalid invocation reference supplied by candidate resolution.
    InvalidCall(CallRef),
    /// Local query coverage evidence.
    Local {
        /// Canonical callable owning the local query.
        function: Uuid,
        /// Native local coverage evidence.
        diagnostic: LocalFlowDiagnostic,
    },
    /// Selected invocation binding coverage evidence.
    Binding {
        /// Selected invocation and callee.
        target: InvocationTarget,
        /// Native binding coverage evidence.
        diagnostic: BindingDiagnostic,
    },
    /// A binder error; no fabricated mapping is produced.
    BindingError(InvocationTarget),
    /// Membership, unknown spread or multi-return projection is not a whole-value alias.
    UnsupportedTransfer {
        /// Selected invocation and callee.
        target: InvocationTarget,
        /// Membership/projection convention requiring another model.
        kind: TransferKind,
    },
    /// An implicit argument/return value has no modeled object origin.
    ImplicitValue {
        /// Selected invocation and callee.
        target: InvocationTarget,
        /// Convention introducing the implicit value.
        kind: TransferKind,
    },
    /// No supplied/discovered canonical target for an invocation in a visited function.
    UnresolvedCall(CallRef),
    /// A computed selector is not narrowed by this native model. All compatible
    /// canonical candidates remain possible, even when the selector is literal.
    UnknownMemberSelector(ValueRef),
    /// Target reachability is retained, but apply-array or pre-bound arguments
    /// need an expanded-actual model before argument-specific flow is complete.
    UnsupportedCallableArguments(CallRef),
    /// No resolver-provided member candidate accepts an arriving receiver type.
    UnresolvedReceiver {
        /// Exact invocation.
        call: CallRef,
        /// Arriving canonical possible type.
        type_id: Uuid,
    },
    /// Global or delegated local budget was exhausted; result is only a prefix.
    LimitReached,
}
/// Bounds on one origin query, not a process RSS or wall-time guarantee.
#[derive(Debug, Clone, Copy)]
pub struct OriginLimits {
    /// Maximum distinct loaded callable records.
    pub max_functions: usize,
    /// Maximum selected canonical invocation/target pairs.
    pub max_targets: usize,
    /// Maximum input member candidates.
    pub max_member_candidates: usize,
    /// Maximum origin facts plus receiver evidence; also bounds seed/diagnostic pools.
    pub max_facts: usize,
    /// Maximum emitted value transfer records.
    pub max_transfers: usize,
    /// Maximum worklist/edge/candidate processing steps.
    pub max_steps: usize,
    /// Limits delegated to a function-local query.
    pub local: LocalFlowLimits,
}
impl Default for OriginLimits {
    fn default() -> Self {
        Self {
            max_functions: 1_024,
            max_targets: 20_000,
            max_member_candidates: 200_000,
            max_facts: 500_000,
            max_transfers: 250_000,
            max_steps: 1_000_000,
            local: LocalFlowLimits::default(),
        }
    }
}
/// Query-local possible origins and derived receiver targets. Never a safety verdict.
#[derive(Debug, Serialize, Deserialize)]
pub struct OriginAnalysis {
    /// Seed origins, indexed by OriginId.
    pub seeds: Vec<OriginSeed>,
    /// Nonempty origin sets, sorted by canonical callable and expression ID.
    pub values: Vec<ValueOrigins>,
    /// Receiver-supported possible calls, including origin provenance.
    pub receiver_targets: Vec<ReceiverTarget>,
    /// Possible functions carried through native value transfers.
    pub callable_values: Vec<CallableValue>,
    /// Canonical invocations activated by reaching callable values.
    pub callable_targets: Vec<ReceiverTarget>,
    /// Canonical possible constructor values carried through native transfers.
    pub constructor_values: Vec<ConstructorValue>,
    /// Native results supported by constructor/prototype-value origins.
    pub construction_evidence: Vec<ConstructionEvidence>,
    /// Native value-transfer explanations.
    pub transfers: Vec<OriginTransfer>,
    /// Partial/unknown/limit evidence.
    pub diagnostics: Vec<OriginDiagnostic>,
    /// Caller contexts are explicitly merged in this first conservative solver.
    pub context_policy: OriginContextPolicy,
}
impl OriginAnalysis {
    /// Find origins without exposing an invalidatable memoized range.
    pub fn origins(&self, value: ValueRef) -> &[OriginId] {
        self.values
            .binary_search_by_key(&(value.function, value.expression.0), |v| {
                (v.value.function, v.value.expression.0)
            })
            .ok()
            .map(|i| self.values[i].origins.as_slice())
            .unwrap_or(&[])
    }
    /// Propagate exact value origins and activate resolver-supplied member candidates.
    /// Inputs must belong to the same immutable graph/source revision as `icfg`.
    pub fn build<C: InterproceduralCfgAccess + ?Sized>(
        icfg: &C,
        seeds: &[OriginSeed],
        targets: &[InvocationTarget],
        members: &[MemberCandidate],
        limits: OriginLimits,
    ) -> Self {
        Self::build_with_callables(icfg, seeds, targets, members, &[], limits)
    }
    /// Include canonical callable-value candidates alongside instance candidates.
    pub fn build_with_callables<C: InterproceduralCfgAccess + ?Sized>(
        icfg: &C,
        seeds: &[OriginSeed],
        targets: &[InvocationTarget],
        members: &[MemberCandidate],
        callables: &[CallableCandidate],
        limits: OriginLimits,
    ) -> Self {
        Self::build_with_values(icfg, seeds, targets, members, callables, &[], limits)
    }
    /// Include canonical constructor values as well as function and instance values.
    pub fn build_with_values<C: InterproceduralCfgAccess + ?Sized>(
        icfg: &C,
        seeds: &[OriginSeed],
        targets: &[InvocationTarget],
        members: &[MemberCandidate],
        callables: &[CallableCandidate],
        constructors: &[ConstructorCandidate],
        limits: OriginLimits,
    ) -> Self {
        let mut query = Query::new(icfg, limits);
        if seeds.len() > limits.max_facts
            || targets.len() > limits.max_targets
            || members.len() > limits.max_member_candidates
            || callables.len() > limits.max_member_candidates
            || constructors.len() > limits.max_member_candidates
        {
            query.limit();
            return query.finish();
        }
        let mut ordered_members = members.to_vec();
        ordered_members.sort_by_key(|m| (target_key(m.target), m.receiver_type));
        for member in ordered_members {
            let call = member.target.call;
            if !icfg
                .get_cfg(call.caller)
                .and_then(|cfg| cfg.operands.calls.get(call.site.0 as usize))
                .is_some_and(|site| site.receiver.is_some())
            {
                query.diagnostic(OriginDiagnostic::InvalidCall(call));
                continue;
            }
            query.members.entry(call).or_default().push(member);
        }
        let mut ordered_constructors = constructors.to_vec();
        ordered_constructors
            .sort_by_key(|c| (c.value.function, c.value.expression.0, c.type_id, c.callee));
        for candidate in ordered_constructors {
            query.constructor_candidate(candidate);
            if query.limited {
                break;
            }
        }
        let mut ordered_callables = callables.to_vec();
        ordered_callables.sort_by_key(|c| {
            (
                c.value.function,
                c.value.expression.0,
                c.callee,
                receiver_key(c.receiver),
            )
        });
        for candidate in ordered_callables {
            query.callable_candidate(candidate);
            if query.limited {
                break;
            }
        }
        let mut ordered_targets = targets.to_vec();
        ordered_targets.sort_by_key(|t| target_key(*t));
        for target in ordered_targets {
            query.target(target);
            if query.limited {
                break;
            }
        }
        let mut ordered_seeds = seeds.to_vec();
        ordered_seeds.sort_by_key(|s| (s.value.function, s.value.expression.0, s.type_id));
        ordered_seeds.dedup();
        for seed in ordered_seeds {
            if query.limited {
                break;
            }
            query.load(seed.value.function);
            if !query.valid(seed.value) {
                query.diagnostic(OriginDiagnostic::InvalidValue(seed.value));
                continue;
            }
            query.seed(seed);
        }
        query.run();
        query.finish()
    }
}
fn receiver_key(receiver: ReceiverSupply) -> (u8, Uuid, u32) {
    match receiver {
        ReceiverSupply::None => (0, Uuid::nil(), 0),
        ReceiverSupply::Member => (1, Uuid::nil(), 0),
        ReceiverSupply::FirstArgument => (2, Uuid::nil(), 0),
        ReceiverSupply::Constructed => (3, Uuid::nil(), 0),
        ReceiverSupply::BoundValue(value) => (4, value.function, value.expression.0),
    }
}
fn target_key(target: InvocationTarget) -> (Uuid, u32, Uuid, (u8, Uuid, u32)) {
    (
        target.call.caller,
        target.call.site.0,
        target.callee,
        receiver_key(target.receiver),
    )
}
struct Query<'a, C: InterproceduralCfgAccess + ?Sized> {
    icfg: &'a C,
    bindings: CallBindings<'a, C>,
    limits: OriginLimits,
    output: OriginAnalysis,
    loaded: HashSet<Uuid>,
    targets: HashSet<InvocationTarget>,
    bound_calls: HashSet<CallRef>,
    members: HashMap<CallRef, Vec<MemberCandidate>>,
    receivers: HashMap<ValueRef, Vec<CallRef>>,
    projections: HashMap<(ValueRef, u32), Vec<ValueRef>>,
    receiver_evidence: HashSet<ReceiverTarget>,
    adjacency: HashMap<ValueRef, Vec<ValueRef>>,
    edge_pairs: HashSet<(ValueRef, ValueRef)>,
    facts: HashMap<ValueRef, HashSet<OriginId>>,
    queue: VecDeque<(ValueRef, OriginId)>,
    seed_ids: HashMap<OriginSeed, OriginId>,
    callables: CallableState,
    deferred: deferred_origins::DeferredState,
    lookup: LookupState,
    field_names: HashMap<String, u32>,
    field_accesses: HashMap<ValueRef, Vec<FieldAccess>>,
    field_stores: HashMap<(OriginId, MemoryMember), HashSet<ValueRef>>,
    field_loads: HashMap<(OriginId, MemoryMember), HashSet<ValueRef>>,
    field_access_count: usize,
    field_slot_count: usize,
    fact_count: usize,
    steps: usize,
    limited: bool,
}
impl<'a, C: InterproceduralCfgAccess + ?Sized> Query<'a, C> {
    fn new(icfg: &'a C, limits: OriginLimits) -> Self {
        Self {
            icfg,
            bindings: CallBindings::new(icfg),
            limits,
            output: OriginAnalysis {
                seeds: vec![],
                values: vec![],
                receiver_targets: vec![],
                callable_values: vec![],
                callable_targets: vec![],
                constructor_values: vec![],
                construction_evidence: vec![],
                transfers: vec![],
                diagnostics: vec![],
                context_policy: OriginContextPolicy::MergedPossibleOrigins,
            },
            loaded: HashSet::new(),
            targets: HashSet::new(),
            bound_calls: HashSet::new(),
            members: HashMap::new(),
            receivers: HashMap::new(),
            projections: HashMap::new(),
            receiver_evidence: HashSet::new(),
            adjacency: HashMap::new(),
            edge_pairs: HashSet::new(),
            facts: HashMap::new(),
            queue: VecDeque::new(),
            seed_ids: HashMap::new(),
            callables: CallableState::default(),
            deferred: deferred_origins::DeferredState::default(),
            lookup: LookupState::default(),
            field_names: HashMap::new(),
            field_accesses: HashMap::new(),
            field_stores: HashMap::new(),
            field_loads: HashMap::new(),
            field_access_count: 0,
            field_slot_count: 0,
            fact_count: 0,
            steps: 0,
            limited: false,
        }
    }
    fn limit(&mut self) {
        if !self.limited {
            self.limited = true;
            self.output.diagnostics.push(OriginDiagnostic::LimitReached);
        }
    }
    fn diagnostic(&mut self, diagnostic: OriginDiagnostic) {
        if self.output.diagnostics.len() >= self.limits.max_facts {
            self.limit()
        } else {
            self.output.diagnostics.push(diagnostic)
        }
    }
    fn step(&mut self) -> bool {
        if self.limited {
            return false;
        }
        self.steps = self.steps.saturating_add(1);
        if self.steps > self.limits.max_steps {
            self.limit();
            false
        } else {
            true
        }
    }
    fn valid(&self, value: ValueRef) -> bool {
        self.icfg
            .get_cfg(value.function)
            .is_some_and(|cfg| (value.expression.0 as usize) < cfg.operands.expressions.len())
    }
    fn load(&mut self, function: Uuid) {
        if self.loaded.contains(&function) || self.limited {
            return;
        }
        if self.loaded.len() >= self.limits.max_functions {
            self.limit();
            return;
        }
        self.loaded.insert(function);
        let Some(cfg) = self.icfg.get_cfg(function) else {
            self.diagnostic(OriginDiagnostic::MissingFunction(function));
            return;
        };
        let statements = cfg
            .blocks
            .values()
            .map(|b| b.statements.len())
            .sum::<usize>();
        let definitions = cfg
            .blocks
            .values()
            .flat_map(|b| &b.statements)
            .map(|s| s.defined_vars.len())
            .sum::<usize>();
        if statements > self.limits.local.max_statements
            || definitions > self.limits.local.max_definitions
            || cfg.operands.calls.len() > self.limits.max_facts
        {
            self.diagnostic(OriginDiagnostic::Local {
                function,
                diagnostic: LocalFlowDiagnostic::LimitReached,
            });
            return;
        }
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            if !self.step() {
                return;
            }
            if let ExpressionKind::Projection { position } = expression.kind
                && let Some(&source) = expression
                    .inputs
                    .slice(&cfg.operands.inputs)
                    .and_then(|s| s.first())
            {
                self.projections
                    .entry((
                        ValueRef {
                            function,
                            expression: source,
                        },
                        position,
                    ))
                    .or_default()
                    .push(ValueRef {
                        function,
                        expression: ExpressionId(index as u32),
                    });
            }
        }
        for (index, call) in cfg.operands.calls.iter().enumerate() {
            if let Some(receiver) = call.receiver {
                self.receivers
                    .entry(ValueRef {
                        function,
                        expression: receiver,
                    })
                    .or_default()
                    .push(CallRef {
                        caller: function,
                        site: CallSiteId(index as u32),
                    });
            }
        }
        self.register_callable_calls(function);
        self.register_lookups(function);
        self.register_fields(function);
        self.register_deferred(function);
        if self.limited {
            return;
        }
        let Ok(pdg) = ProgramDependenceGraph::build(cfg, b"") else {
            self.diagnostic(OriginDiagnostic::MissingFunction(function));
            return;
        };
        let flow = LocalValueFlow::build(cfg, &pdg, self.limits.local);
        for diagnostic in flow.diagnostics {
            self.diagnostic(OriginDiagnostic::Local {
                function,
                diagnostic,
            });
        }
        for edge in flow.transfers {
            self.edge(
                ValueRef {
                    function,
                    expression: edge.from,
                },
                ValueRef {
                    function,
                    expression: edge.to,
                },
                OriginTransferCause::Local(edge.kind),
            );
            if self.limited {
                break;
            }
        }
    }
    fn seed(&mut self, seed: OriginSeed) {
        if self.limited {
            return;
        }
        if let Some(&origin) = self.seed_ids.get(&seed) {
            self.fact(seed.value, origin);
            return;
        }
        if self.output.seeds.len() >= self.limits.max_facts {
            self.limit();
            return;
        }
        let Ok(index) = u32::try_from(self.output.seeds.len()) else {
            self.limit();
            return;
        };
        let origin = OriginId(index);
        self.seed_ids.insert(seed, origin);
        self.output.seeds.push(seed);
        self.fact(seed.value, origin);
    }
    fn fact(&mut self, value: ValueRef, origin: OriginId) {
        if self.limited || self.facts.get(&value).is_some_and(|s| s.contains(&origin)) {
            return;
        }
        if self.fact_count.saturating_add(self.receiver_evidence.len()) >= self.limits.max_facts {
            self.limit();
            return;
        }
        self.facts.entry(value).or_default().insert(origin);
        self.fact_count += 1;
        self.index_callable_origin(value, origin);
        self.queue.push_back((value, origin));
    }
    fn edge(&mut self, from: ValueRef, to: ValueRef, cause: OriginTransferCause) {
        if self.limited {
            return;
        }
        if self.output.transfers.len() >= self.limits.max_transfers {
            self.limit();
            return;
        }
        if !self.valid(from) || !self.valid(to) {
            self.diagnostic(OriginDiagnostic::InvalidValue(if !self.valid(from) {
                from
            } else {
                to
            }));
            return;
        }
        self.output
            .transfers
            .push(OriginTransfer { from, to, cause });
        if self.edge_pairs.insert((from, to)) {
            self.adjacency.entry(from).or_default().push(to);
            let mut origins: Vec<_> = self
                .facts
                .get(&from)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            origins.sort_by_key(|o| o.0);
            for origin in origins {
                if !self.step() {
                    break;
                }
                self.fact(to, origin);
            }
        }
    }
    fn target(&mut self, target: InvocationTarget) {
        if self.targets.contains(&target) || self.limited {
            return;
        }
        if self.targets.len() >= self.limits.max_targets {
            self.limit();
            return;
        }
        self.targets.insert(target);
        self.bound_calls.insert(target.call);
        self.load(target.call.caller);
        self.load(target.callee);
        if let ReceiverSupply::BoundValue(value) = target.receiver {
            self.load(value.function);
        }
        if self.limited {
            return;
        }
        self.bind_yields(target);
        let Ok(binding) = self.bindings.bind_invocation(target) else {
            self.diagnostic(OriginDiagnostic::BindingError(target));
            return;
        };
        let diagnostics = binding
            .diagnostics
            .slice(self.bindings.diagnostics())
            .unwrap_or(&[])
            .to_vec();
        let transfers = binding
            .transfers
            .slice(self.bindings.transfers())
            .unwrap_or(&[])
            .to_vec();
        for diagnostic in diagnostics {
            self.diagnostic(OriginDiagnostic::Binding { target, diagnostic });
        }
        let generator = self
            .icfg
            .get_cfg(target.callee)
            .is_some_and(|cfg| cfg.operands.generator);
        let scalar_return = self
            .icfg
            .get_cfg(target.callee)
            .is_some_and(|cfg| cfg.operands.returns.iter().all(|r| r.values.len <= 1));
        for transfer in transfers {
            if !self.step() {
                break;
            }
            if generator && matches!(transfer.kind, TransferKind::ReturnToResult { .. }) {
                continue;
            }
            if let TransferKind::ReturnToResult { position } = transfer.kind
                && !scalar_return
            {
                let projections = self
                    .projections
                    .get(&(transfer.to, position))
                    .cloned()
                    .unwrap_or_default();
                if projections.is_empty() {
                    self.diagnostic(OriginDiagnostic::UnsupportedTransfer {
                        target,
                        kind: transfer.kind,
                    });
                }
                for to in projections {
                    if !self.step() {
                        break;
                    }
                    match transfer.from {
                        BindingSource::Expression(from) => self.edge(
                            from,
                            to,
                            OriginTransferCause::Invocation {
                                target,
                                kind: transfer.kind,
                            },
                        ),
                        BindingSource::Implicit(_) => {
                            self.diagnostic(OriginDiagnostic::ImplicitValue {
                                target,
                                kind: transfer.kind,
                            })
                        }
                    }
                }
                continue;
            }
            if matches!(transfer.kind, TransferKind::RestElement { .. }) {
                self.seed(OriginSeed {
                    value: transfer.to,
                    type_id: None,
                });
                if let BindingSource::Expression(from) = transfer.from {
                    self.memory_access(transfer.to, MemoryMember::Element, from, true);
                } else {
                    self.diagnostic(OriginDiagnostic::ImplicitValue {
                        target,
                        kind: transfer.kind,
                    });
                }
                continue;
            }
            let supported =
                matches!(
                    transfer.kind,
                    TransferKind::ActualToFormal
                        | TransferKind::ReceiverToFormal
                        | TransferKind::DefaultToFormal { .. }
                ) || (matches!(transfer.kind, TransferKind::ReturnToResult { position: 0 })
                    && scalar_return);
            if !supported {
                self.diagnostic(OriginDiagnostic::UnsupportedTransfer {
                    target,
                    kind: transfer.kind,
                });
                continue;
            }
            match transfer.from {
                BindingSource::Expression(from) => self.edge(
                    from,
                    transfer.to,
                    OriginTransferCause::Invocation {
                        target,
                        kind: transfer.kind,
                    },
                ),
                BindingSource::Implicit(_) => self.diagnostic(OriginDiagnostic::ImplicitValue {
                    target,
                    kind: transfer.kind,
                }),
            }
        }
    }
    fn run(&mut self) {
        while let Some((value, origin)) = self.queue.pop_front() {
            if !self.step() {
                break;
            }
            let outgoing = self.adjacency.get(&value).cloned().unwrap_or_default();
            for to in outgoing {
                if !self.step() {
                    break;
                }
                self.fact(to, origin);
            }
            self.deferred_fact(value, origin);
            self.callable_fact(value, origin);
            self.lookup_fact(value, origin);
            let accesses = self.field_accesses.get(&value).cloned().unwrap_or_default();
            for access in accesses {
                self.bind_field(access, origin)
            }
            let calls = self.receivers.get(&value).cloned().unwrap_or_default();
            let Some(type_id) = self.output.seeds[origin.0 as usize].type_id else {
                continue;
            };
            for call in calls {
                let candidates = self.members.get(&call).cloned().unwrap_or_default();
                let mut matched = false;
                for member in candidates {
                    if !self.step() {
                        break;
                    }
                    if member.receiver_type != type_id {
                        continue;
                    }
                    matched = true;
                    let evidence = ReceiverTarget {
                        target: member.target,
                        origin,
                    };
                    if self.receiver_evidence.contains(&evidence) {
                        continue;
                    }
                    if self.fact_count.saturating_add(self.receiver_evidence.len())
                        >= self.limits.max_facts
                    {
                        self.limit();
                        break;
                    }
                    self.receiver_evidence.insert(evidence);
                    self.output.receiver_targets.push(evidence);
                    self.target(member.target);
                }
                if !matched && !self.limited {
                    self.diagnostic(OriginDiagnostic::UnresolvedReceiver { call, type_id });
                }
            }
        }
    }
    fn finish(mut self) -> OriginAnalysis {
        if !self.limited {
            let mut functions: Vec<_> = self.loaded.iter().copied().collect();
            functions.sort();
            for function in functions {
                if let Some(cfg) = self.icfg.get_cfg(function) {
                    for index in 0..cfg.operands.calls.len() {
                        if !self.step() {
                            break;
                        }
                        let call = CallRef {
                            caller: function,
                            site: CallSiteId(index as u32),
                        };
                        if !self.bound_calls.contains(&call) {
                            self.diagnostic(OriginDiagnostic::UnresolvedCall(call));
                        }
                    }
                }
            }
        }
        self.output.values = self
            .facts
            .into_iter()
            .map(|(value, origins)| {
                let mut origins: Vec<_> = origins.into_iter().collect();
                origins.sort_by_key(|o| o.0);
                ValueOrigins { value, origins }
            })
            .collect();
        self.output
            .values
            .sort_by_key(|v| (v.value.function, v.value.expression.0));
        self.output
            .receiver_targets
            .sort_by_key(|r| (target_key(r.target), r.origin.0));
        self.output
            .callable_values
            .sort_by_key(|b| (b.origin.0, b.callee, receiver_key(b.receiver)));
        self.output
            .callable_targets
            .sort_by_key(|r| (target_key(r.target), r.origin.0));
        self.output
            .constructor_values
            .sort_by_key(|c| (c.origin.0, c.type_id, c.callee));
        self.output.construction_evidence.sort_by_key(|e| {
            (
                e.value.function,
                e.value.expression.0,
                e.origin.0,
                e.type_id,
            )
        });
        self.output.transfers.sort_by_key(|e| {
            (
                e.from.function,
                e.from.expression.0,
                e.to.function,
                e.to.expression.0,
            )
        });
        self.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MemoryMember {
    Field(u32),
    Element,
    Yielded,
    GeneratorReturn,
}
#[derive(Debug, Clone, Copy)]
struct FieldAccess {
    member: MemoryMember,
    value: ValueRef,
    store: bool,
}
impl<C: InterproceduralCfgAccess + ?Sized> Query<'_, C> {
    fn register_fields(&mut self, function: Uuid) {
        let Some(cfg) = self.icfg.get_cfg(function) else {
            return;
        };
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            if self.limited {
                break;
            }
            if let ExpressionKind::Member(member) = expression.kind
                && let Some(&receiver) = expression
                    .inputs
                    .slice(&cfg.operands.inputs)
                    .and_then(|s| s.first())
                && let Some(name) = cfg.operands.operand_name(member)
            {
                self.field_access(
                    ValueRef {
                        function,
                        expression: receiver,
                    },
                    name,
                    ValueRef {
                        function,
                        expression: ExpressionId(index as u32),
                    },
                    false,
                );
            }
        }
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            if self.limited {
                break;
            }
            let value = ValueRef {
                function,
                expression: ExpressionId(index as u32),
            };
            match expression.kind {
                ExpressionKind::Collection => {
                    self.seed(OriginSeed {
                        value,
                        type_id: None,
                    });
                    for &element in expression.inputs.slice(&cfg.operands.inputs).unwrap_or(&[]) {
                        self.memory_access(
                            value,
                            MemoryMember::Element,
                            ValueRef {
                                function,
                                expression: element,
                            },
                            true,
                        );
                        if self.limited {
                            break;
                        }
                    }
                }
                ExpressionKind::Element => {
                    if let Some(&base) = expression
                        .inputs
                        .slice(&cfg.operands.inputs)
                        .and_then(|s| s.first())
                    {
                        self.memory_access(
                            ValueRef {
                                function,
                                expression: base,
                            },
                            MemoryMember::Element,
                            value,
                            false,
                        );
                    }
                }
                _ => {}
            }
        }
        for write in &cfg.operands.element_writes {
            if self.limited {
                break;
            }
            self.memory_access(
                ValueRef {
                    function,
                    expression: write.container,
                },
                MemoryMember::Element,
                ValueRef {
                    function,
                    expression: write.value,
                },
                true,
            );
        }
        for write in &cfg.operands.field_writes {
            if self.limited {
                break;
            }
            if let Some(name) = cfg.operands.operand_name(write.member) {
                self.field_access(
                    ValueRef {
                        function,
                        expression: write.receiver,
                    },
                    name,
                    ValueRef {
                        function,
                        expression: write.value,
                    },
                    true,
                );
            }
        }
    }
    fn field_access(&mut self, base: ValueRef, name: &str, value: ValueRef, store: bool) {
        if self.limited {
            return;
        }
        if self.field_access_count >= self.limits.max_transfers {
            self.limit();
            return;
        }
        let next = self.field_names.len() as u32;
        let member = *self.field_names.entry(name.to_owned()).or_insert(next);
        self.memory_access(base, MemoryMember::Field(member), value, store);
    }
    fn memory_access(
        &mut self,
        base: ValueRef,
        member: MemoryMember,
        value: ValueRef,
        store: bool,
    ) {
        if self.limited {
            return;
        }
        if self.field_access_count >= self.limits.max_transfers {
            self.limit();
            return;
        }
        let access = FieldAccess {
            member,
            value,
            store,
        };
        self.field_access_count += 1;
        self.field_accesses.entry(base).or_default().push(access);
        let mut origins: Vec<_> = self
            .facts
            .get(&base)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        origins.sort_by_key(|o| o.0);
        for origin in origins {
            self.bind_field(access, origin)
        }
    }
    fn bind_field(&mut self, access: FieldAccess, object: OriginId) {
        if !self.step() {
            return;
        }
        let key = (object, access.member);
        if access.store {
            if self.field_slot_count >= self.limits.max_transfers {
                self.limit();
                return;
            }
            if !self
                .field_stores
                .entry(key)
                .or_default()
                .insert(access.value)
            {
                return;
            }
            self.field_slot_count += 1;
            let mut reads: Vec<_> = self
                .field_loads
                .get(&key)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            reads.sort_by_key(|v| (v.function, v.expression.0));
            for read in reads {
                if !self.step() {
                    break;
                }
                self.edge(
                    access.value,
                    read,
                    match access.member {
                        MemoryMember::Yielded | MemoryMember::GeneratorReturn => {
                            OriginTransferCause::DeferredYield {
                                container: object,
                                read,
                                terminal: access.member == MemoryMember::GeneratorReturn,
                            }
                        }
                        MemoryMember::Field(_) => OriginTransferCause::Field { object, read },
                        MemoryMember::Element => OriginTransferCause::Element {
                            container: object,
                            read,
                        },
                    },
                );
            }
        } else {
            if self.field_slot_count >= self.limits.max_transfers {
                self.limit();
                return;
            }
            if !self
                .field_loads
                .entry(key)
                .or_default()
                .insert(access.value)
            {
                return;
            }
            self.field_slot_count += 1;
            let mut writes: Vec<_> = self
                .field_stores
                .get(&key)
                .into_iter()
                .flatten()
                .copied()
                .collect();
            writes.sort_by_key(|v| (v.function, v.expression.0));
            for write in writes {
                if !self.step() {
                    break;
                }
                self.edge(
                    write,
                    access.value,
                    match access.member {
                        MemoryMember::Yielded | MemoryMember::GeneratorReturn => {
                            OriginTransferCause::DeferredYield {
                                container: object,
                                read: access.value,
                                terminal: access.member == MemoryMember::GeneratorReturn,
                            }
                        }
                        MemoryMember::Field(_) => OriginTransferCause::Field {
                            object,
                            read: access.value,
                        },
                        MemoryMember::Element => OriginTransferCause::Element {
                            container: object,
                            read: access.value,
                        },
                    },
                );
            }
        }
    }
}
