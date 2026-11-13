//! Bounded possible-value origins through native local and invocation transfers.
//! Resolution inputs use canonical IDs; the solver never parses or matches names.
use crate::call_bindings::{
    BindingDiagnostic, BindingSource, CallBindings, CallRef, InvocationTarget, TransferKind,
    ValueRef,
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

/// Dense query-local origin index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginId(pub u32);
/// A provider-supplied possible type at an exact native value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginSeed {
    /// Native producer (allocation or another explicitly modeled source).
    pub value: ValueRef,
    /// Canonical possible type UUID, supplied by graph/type resolution.
    pub type_id: Uuid,
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
        let mut query = Query::new(icfg, limits);
        if seeds.len() > limits.max_facts
            || targets.len() > limits.max_targets
            || members.len() > limits.max_member_candidates
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
            let Ok(index) = u32::try_from(query.output.seeds.len()) else {
                query.limit();
                break;
            };
            query.output.seeds.push(seed);
            query.fact(seed.value, OriginId(index));
        }
        query.run();
        query.finish()
    }
}
fn target_key(target: InvocationTarget) -> (Uuid, u32, Uuid, u8) {
    (
        target.call.caller,
        target.call.site.0,
        target.callee,
        target.receiver as u8,
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
    receiver_evidence: HashSet<ReceiverTarget>,
    adjacency: HashMap<ValueRef, Vec<ValueRef>>,
    edge_pairs: HashSet<(ValueRef, ValueRef)>,
    facts: HashMap<ValueRef, HashSet<OriginId>>,
    queue: VecDeque<(ValueRef, OriginId)>,
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
                transfers: vec![],
                diagnostics: vec![],
                context_policy: OriginContextPolicy::MergedPossibleOrigins,
            },
            loaded: HashSet::new(),
            targets: HashSet::new(),
            bound_calls: HashSet::new(),
            members: HashMap::new(),
            receivers: HashMap::new(),
            receiver_evidence: HashSet::new(),
            adjacency: HashMap::new(),
            edge_pairs: HashSet::new(),
            facts: HashMap::new(),
            queue: VecDeque::new(),
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
        if self.limited {
            return;
        }
        let Ok(binding) = self.bindings.bind(target) else {
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
        let scalar_return = self
            .icfg
            .get_cfg(target.callee)
            .is_some_and(|cfg| cfg.operands.returns.iter().all(|r| r.values.len <= 1));
        for transfer in transfers {
            if !self.step() {
                break;
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
            let calls = self.receivers.get(&value).cloned().unwrap_or_default();
            let type_id = self.output.seeds[origin.0 as usize].type_id;
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
