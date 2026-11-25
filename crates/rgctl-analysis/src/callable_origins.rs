//! Callable values supplied by canonical resolution and propagated by native flow.
use super::*;

/// Canonical possible function value at an exact native expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallableCandidate {
    /// Expression producing this callable value.
    pub value: ValueRef,
    /// Canonical target callable.
    pub callee: Uuid,
    /// Bound receiver or first-argument convention; never a guessed invocation.
    pub receiver: ReceiverSupply,
    /// Optional exact receiver/type premise required before activating the value.
    pub requirement: Option<OriginRequirement>,
}
/// A callable member is available when this value carries the specified type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginRequirement {
    /// Native receiver value.
    pub value: ValueRef,
    /// Canonical possible type.
    pub type_id: Uuid,
}
/// Possible target carried by one callable-value origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallableValue {
    /// Query-local origin of the callable value.
    pub origin: OriginId,
    /// Canonical pointed-to callable.
    pub callee: Uuid,
    /// Established or conservative receiver convention.
    pub receiver: ReceiverSupply,
}
#[derive(Default)]
pub(super) struct CallableState {
    requirements: HashMap<ValueRef, Vec<CallableCandidate>>,
    values: HashMap<OriginId, HashSet<CallableValue>>,
    origin_values: HashMap<OriginId, Vec<ValueRef>>,
    invocations: HashMap<ValueRef, Vec<CallRef>>,
    binds: HashMap<ValueRef, Vec<CallRef>>,
    calls: HashMap<ValueRef, Vec<CallRef>>,
    evidence: HashSet<ReceiverTarget>,
    count: usize,
}
impl<C: InterproceduralCfgAccess + ?Sized> Query<'_, C> {
    pub(super) fn callable_candidate(&mut self, candidate: CallableCandidate) {
        self.load(candidate.value.function);
        if !self.valid(candidate.value) {
            self.diagnostic(OriginDiagnostic::InvalidValue(candidate.value));
            return;
        }
        if let Some(requirement) = candidate.requirement {
            self.load(requirement.value.function);
            if !self.valid(requirement.value) {
                self.diagnostic(OriginDiagnostic::InvalidValue(requirement.value));
                return;
            }
            self.callables
                .requirements
                .entry(requirement.value)
                .or_default()
                .push(candidate);
            if self
                .facts
                .get(&requirement.value)
                .into_iter()
                .flatten()
                .any(|o| self.output.seeds[o.0 as usize].type_id == Some(requirement.type_id))
            {
                self.callable_value(candidate.value, candidate.callee, candidate.receiver);
            }
        } else {
            self.callable_value(candidate.value, candidate.callee, candidate.receiver);
        }
    }
    fn callable_value(&mut self, value: ValueRef, callee: Uuid, receiver: ReceiverSupply) {
        if self.limited {
            return;
        }
        if !matches!(
            receiver,
            ReceiverSupply::None | ReceiverSupply::FirstArgument | ReceiverSupply::BoundValue(_)
        ) {
            self.diagnostic(OriginDiagnostic::InvalidValue(value));
            return;
        }
        self.seed(OriginSeed {
            value,
            type_id: None,
        });
        let Some(&origin) = self.seed_ids.get(&OriginSeed {
            value,
            type_id: None,
        }) else {
            return;
        };
        let binding = CallableValue {
            origin,
            callee,
            receiver,
        };
        if self
            .callables
            .values
            .get(&origin)
            .is_some_and(|s| s.contains(&binding))
        {
            return;
        }
        if self.callables.count >= self.limits.max_facts {
            self.limit();
            return;
        }
        self.callables
            .values
            .entry(origin)
            .or_default()
            .insert(binding);
        self.callables.count += 1;
        self.output.callable_values.push(binding);
        // A new possible function can arrive after its origin reached consumers.
        // Replay those consumers without inventing new value-alias edges.
        for at in self
            .callables
            .origin_values
            .get(&origin)
            .cloned()
            .unwrap_or_default()
        {
            if !self.step() {
                break;
            }
            self.queue.push_back((at, origin));
        }
    }
    pub(super) fn register_callable_calls(&mut self, function: Uuid) {
        let Some(cfg) = self.icfg.get_cfg(function) else {
            return;
        };
        for (i, call) in cfg.operands.calls.iter().enumerate() {
            if self.limited {
                break;
            }
            let site = CallRef {
                caller: function,
                site: CallSiteId(i as u32),
            };
            self.callables
                .invocations
                .entry(ValueRef {
                    function,
                    expression: call.callee,
                })
                .or_default()
                .push(site);
            // Function-object invocation conventions are language-specific.
            // A receiver carrying a callable does not make arbitrary members
            // (e.g. Java toString) invocations of that callable.
            let name = match cfg.operands.expressions[call.callee.0 as usize].kind {
                ExpressionKind::Name(name) | ExpressionKind::Member(name) => {
                    cfg.operands.operand_name(name)
                }
                _ => None,
            };
            let function_object_call = matches!(
                (cfg.operands.language, name),
                (OperandLanguage::Java, Some("apply")) | (OperandLanguage::Ruby, Some("call"))
            );
            if function_object_call && let Some(receiver) = call.receiver {
                self.callables
                    .invocations
                    .entry(ValueRef {
                        function,
                        expression: receiver,
                    })
                    .or_default()
                    .push(site);
            }
            if cfg.operands.language == OperandLanguage::Puppet
                && name == Some("call")
                && let Some(first) = call
                    .arguments
                    .slice(&cfg.operands.arguments)
                    .and_then(|args| args.first())
            {
                self.callables
                    .calls
                    .entry(ValueRef {
                        function,
                        expression: first.value,
                    })
                    .or_default()
                    .push(site);
            }
            if matches!(
                cfg.operands.language,
                OperandLanguage::JavaScript | OperandLanguage::TypeScript
            ) && let ExpressionKind::Member(name) =
                cfg.operands.expressions[call.callee.0 as usize].kind
                && let Some(receiver) = call.receiver
            {
                let value = ValueRef {
                    function,
                    expression: receiver,
                };
                match cfg.operands.operand_name(name) {
                    Some("bind") => self.callables.binds.entry(value).or_default().push(site),
                    Some("call" | "apply") => {
                        self.callables.calls.entry(value).or_default().push(site)
                    }
                    _ => {}
                }
            }
        }
    }
    pub(super) fn callable_fact(&mut self, value: ValueRef, origin: OriginId) {
        let requirements = self
            .callables
            .requirements
            .get(&value)
            .cloned()
            .unwrap_or_default();
        for candidate in requirements {
            if !self.step() {
                return;
            }
            if candidate
                .requirement
                .is_some_and(|r| self.output.seeds[origin.0 as usize].type_id == Some(r.type_id))
            {
                self.callable_value(candidate.value, candidate.callee, candidate.receiver);
            }
        }
        let mut bindings: Vec<_> = self
            .callables
            .values
            .get(&origin)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        bindings.sort_by_key(|b| (b.callee, receiver_key(b.receiver)));
        for binding in bindings {
            if !self.step() {
                return;
            }
            for call in self
                .callables
                .invocations
                .get(&value)
                .cloned()
                .unwrap_or_default()
            {
                self.callable_target(call, binding, origin, binding.receiver);
            }
            for call in self
                .callables
                .calls
                .get(&value)
                .cloned()
                .unwrap_or_default()
            {
                if let Some(cfg) = self.icfg.get_cfg(call.caller)
                    && matches!(
                        cfg.operands.language,
                        OperandLanguage::JavaScript | OperandLanguage::TypeScript
                    )
                    && let Some(site) = cfg.operands.calls.get(call.site.0 as usize)
                    && matches!(cfg.operands.expressions[site.callee.0 as usize].kind, ExpressionKind::Member(name) if cfg.operands.operand_name(name) == Some("apply"))
                {
                    self.diagnostic(OriginDiagnostic::UnsupportedCallableArguments(call));
                }
                // Target reachability is retained; expanded argument precision
                // is explicitly incomplete for apply-array conventions.
                self.callable_target(call, binding, origin, ReceiverSupply::FirstArgument);
            }
            for call in self
                .callables
                .binds
                .get(&value)
                .cloned()
                .unwrap_or_default()
            {
                if !self.step() {
                    return;
                }
                let Some(cfg) = self.icfg.get_cfg(call.caller) else {
                    continue;
                };
                let site = &cfg.operands.calls[call.site.0 as usize];
                if site.arguments.len > 1 {
                    self.diagnostic(OriginDiagnostic::UnsupportedCallableArguments(call));
                }
                let result = ValueRef {
                    function: call.caller,
                    expression: site.result,
                };
                self.callable_value(result, binding.callee, binding.receiver);
                if let Some(arg) = site
                    .arguments
                    .slice(&cfg.operands.arguments)
                    .and_then(|s| s.first())
                {
                    self.callable_value(
                        result,
                        binding.callee,
                        ReceiverSupply::BoundValue(ValueRef {
                            function: call.caller,
                            expression: arg.value,
                        }),
                    );
                }
            }
        }
    }
    fn callable_target(
        &mut self,
        call: CallRef,
        binding: CallableValue,
        origin: OriginId,
        receiver: ReceiverSupply,
    ) {
        if !self.step() {
            return;
        }
        let target = InvocationTarget {
            call,
            callee: binding.callee,
            receiver,
        };
        let evidence = ReceiverTarget { target, origin };
        if self.callables.evidence.contains(&evidence) {
            return;
        }
        if self.callables.evidence.len() >= self.limits.max_facts {
            self.limit();
            return;
        }
        self.callables.evidence.insert(evidence);
        self.output.callable_targets.push(evidence);
        self.target(target);
    }
    pub(super) fn index_callable_origin(&mut self, value: ValueRef, origin: OriginId) {
        self.callables
            .origin_values
            .entry(origin)
            .or_default()
            .push(value);
    }
}
