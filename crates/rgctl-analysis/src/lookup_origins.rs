//! Conservative member lookup and canonical constructor values from native facts.
use super::*;

/// Resolver-supplied canonical class/type value usable as a constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstructorCandidate {
    /// Exact native reference to the class/type value.
    pub value: ValueRef,
    /// Canonical possible constructed type.
    pub type_id: Uuid,
    /// Canonical explicit constructor body, when present and available.
    pub callee: Option<Uuid>,
}
/// A possible constructor carried by an opaque callable origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstructorValue {
    /// Query-local constructor-value origin.
    pub origin: OriginId,
    /// Canonical possible constructed type.
    pub type_id: Uuid,
    /// Explicit constructor body if supplied.
    pub callee: Option<Uuid>,
}
/// Explanation linking a result to its possible constructor-value origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstructionEvidence {
    /// Native constructed result value.
    pub value: ValueRef,
    /// Origin carrying the possible constructor or prototype type.
    pub origin: OriginId,
    /// Canonical possible instance type.
    pub type_id: Uuid,
}
/// Native member-value reference for canonical target resolution.
#[derive(Debug, Clone, Copy)]
pub struct CallableMemberValue {
    /// Function-local expression producing the possible member value.
    pub value: ExpressionId,
    /// Exact native receiver expression.
    pub receiver: ExpressionId,
    /// Named selector, or unknown/computed selector retaining all alternatives.
    pub member: Option<OperandNameId>,
    /// Exact native computed selector, if present; no constant folding is implied.
    pub selector: Option<ExpressionId>,
}
/// Enumerate member/index values and the conservative Python getattr convention.
/// This inspects persisted native facts, not source or a second AST traversal.
pub fn callable_member_values(
    facts: &FunctionOperands,
) -> impl Iterator<Item = CallableMemberValue> + '_ {
    facts
        .expressions
        .iter()
        .enumerate()
        .filter_map(|(index, _)| native_callable_member_value(facts, ExpressionId(index as u32)))
}
fn native_callable_member_value(
    facts: &FunctionOperands,
    value: ExpressionId,
) -> Option<CallableMemberValue> {
    let expression = facts.expressions.get(value.0 as usize)?;
    let inputs = expression.inputs.slice(&facts.inputs)?;
    let (receiver, member, selector) = match expression.kind {
        ExpressionKind::Member(name) => (*inputs.first()?, Some(name), None),
        ExpressionKind::Element if inputs.len() > 1 => {
            (*inputs.first()?, None, inputs.get(1).copied())
        }
        ExpressionKind::CallResult(site) => {
            let call = facts.calls.get(site.0 as usize)?;
            if facts.language != OperandLanguage::Python
                || call.receiver.is_some()
                || !matches!(facts.expressions.get(call.callee.0 as usize)?.kind,
                    ExpressionKind::Name(n) if facts.operand_name(n) == Some("getattr"))
            {
                return None;
            }
            let arguments = call.arguments.slice(&facts.arguments)?;
            (
                arguments.first()?.value,
                None,
                Some(arguments.get(1)?.value),
            )
        }
        _ => return None,
    };
    Some(CallableMemberValue {
        value,
        receiver,
        member,
        selector,
    })
}

#[derive(Default)]
pub(super) struct LookupState {
    constructors: HashMap<OriginId, HashSet<ConstructorValue>>,
    invocations: HashMap<ValueRef, Vec<CallRef>>,
    instances: HashMap<ValueRef, Vec<ValueRef>>,
    evidence: HashSet<ConstructionEvidence>,
    count: usize,
}
impl<C: InterproceduralCfgAccess + ?Sized> Query<'_, C> {
    pub(super) fn diagnose_lookup_selector(&mut self, value: ValueRef) {
        if self
            .icfg
            .get_cfg(value.function)
            .and_then(|cfg| native_callable_member_value(&cfg.operands, value.expression))
            .is_some_and(|member| member.member.is_none())
        {
            self.diagnostic(OriginDiagnostic::UnknownMemberSelector(value));
        }
    }

    pub(super) fn constructor_candidate(&mut self, candidate: ConstructorCandidate) {
        self.load(candidate.value.function);
        if !self.valid(candidate.value) {
            self.diagnostic(OriginDiagnostic::InvalidValue(candidate.value));
            return;
        }
        self.diagnose_lookup_selector(candidate.value);
        self.seed(OriginSeed {
            value: candidate.value,
            type_id: None,
        });
        let Some(&origin) = self.seed_ids.get(&OriginSeed {
            value: candidate.value,
            type_id: None,
        }) else {
            return;
        };
        let value = ConstructorValue {
            origin,
            type_id: candidate.type_id,
            callee: candidate.callee,
        };
        if self
            .lookup
            .constructors
            .get(&origin)
            .is_some_and(|s| s.contains(&value))
        {
            return;
        }
        if self.lookup.count >= self.limits.max_facts {
            self.limit();
            return;
        }
        self.lookup
            .constructors
            .entry(origin)
            .or_default()
            .insert(value);
        self.lookup.count += 1;
        self.output.constructor_values.push(value);
        self.replay_callable_origin(origin);
    }
    pub(super) fn register_lookups(&mut self, function: Uuid) {
        let Some(cfg) = self.icfg.get_cfg(function) else {
            return;
        };
        for (i, site) in cfg.operands.calls.iter().enumerate() {
            if !self.step() {
                break;
            }
            self.lookup
                .invocations
                .entry(ValueRef {
                    function,
                    expression: site.callee,
                })
                .or_default()
                .push(CallRef {
                    caller: function,
                    site: CallSiteId(i as u32),
                });
            // Class-object invocation conventions have receiver roles of their
            // own; unrelated methods on the class must not construct instances.
            let name = match cfg.operands.expressions[site.callee.0 as usize].kind {
                ExpressionKind::Name(n) | ExpressionKind::Member(n) => cfg.operands.operand_name(n),
                _ => None,
            };
            if matches!(
                (cfg.operands.language, name),
                (OperandLanguage::Ruby, Some("new")) | (OperandLanguage::Swift, Some("init"))
            ) && let Some(receiver) = site.receiver
            {
                self.lookup
                    .invocations
                    .entry(ValueRef {
                        function,
                        expression: receiver,
                    })
                    .or_default()
                    .push(CallRef {
                        caller: function,
                        site: CallSiteId(i as u32),
                    });
            }
            if !matches!(
                cfg.operands.language,
                OperandLanguage::JavaScript | OperandLanguage::TypeScript
            ) {
                continue;
            }
            if !matches!(cfg.operands.expressions[site.callee.0 as usize].kind,ExpressionKind::Member(n) if cfg.operands.operand_name(n)==Some("create"))
            {
                continue;
            }
            if !site.receiver.is_some_and(|r|matches!(cfg.operands.expressions[r.0 as usize].kind,ExpressionKind::Name(n) if cfg.operands.operand_name(n)==Some("Object"))){continue}
            let Some(argument) = site
                .arguments
                .slice(&cfg.operands.arguments)
                .and_then(|a| a.first())
            else {
                continue;
            };
            let expression = &cfg.operands.expressions[argument.value.0 as usize];
            if !matches!(expression.kind,ExpressionKind::Member(n) if cfg.operands.operand_name(n)==Some("prototype"))
            {
                continue;
            }
            if let Some(&base) = expression
                .inputs
                .slice(&cfg.operands.inputs)
                .and_then(|a| a.first())
            {
                self.lookup
                    .instances
                    .entry(ValueRef {
                        function,
                        expression: base,
                    })
                    .or_default()
                    .push(ValueRef {
                        function,
                        expression: site.result,
                    });
            }
        }
        // Go/Rust composite and struct literals construct without an invocation
        // body. The native type operand still supplies the possible instance.
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            if !self.step() {
                break;
            }
            if let ExpressionKind::Construction(source) = expression.kind {
                self.lookup
                    .instances
                    .entry(ValueRef {
                        function,
                        expression: source,
                    })
                    .or_default()
                    .push(ValueRef {
                        function,
                        expression: ExpressionId(index as u32),
                    });
            }
        }
    }
    pub(super) fn lookup_fact(&mut self, value: ValueRef, origin: OriginId) {
        let mut constructors: Vec<_> = self
            .lookup
            .constructors
            .get(&origin)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        constructors.sort_by_key(|c| (c.type_id, c.callee));
        for constructor in constructors {
            if !self.step() {
                return;
            }
            for call in self
                .lookup
                .invocations
                .get(&value)
                .cloned()
                .unwrap_or_default()
            {
                if !self.step() {
                    return;
                }
                if let Some(cfg) = self.icfg.get_cfg(call.caller)
                    && let Some(site) = cfg.operands.calls.get(call.site.0 as usize)
                {
                    self.construction(
                        ValueRef {
                            function: call.caller,
                            expression: site.result,
                        },
                        constructor,
                    );
                    if let Some(callee) = constructor.callee {
                        self.callable_target(
                            call,
                            CallableValue {
                                origin,
                                callee,
                                receiver: ReceiverSupply::Constructed,
                            },
                            origin,
                            ReceiverSupply::Constructed,
                        );
                    }
                }
            }
            for result in self
                .lookup
                .instances
                .get(&value)
                .cloned()
                .unwrap_or_default()
            {
                if !self.step() {
                    return;
                }
                self.construction(result, constructor);
            }
        }
    }
    fn construction(&mut self, value: ValueRef, constructor: ConstructorValue) {
        if self.limited {
            return;
        }
        let evidence = ConstructionEvidence {
            value,
            origin: constructor.origin,
            type_id: constructor.type_id,
        };
        if self.lookup.evidence.contains(&evidence) {
            return;
        }
        if self.lookup.evidence.len() >= self.limits.max_facts {
            self.limit();
            return;
        }
        self.lookup.evidence.insert(evidence);
        self.output.construction_evidence.push(evidence);
        self.seed(OriginSeed {
            value,
            type_id: Some(constructor.type_id),
        });
    }
}
