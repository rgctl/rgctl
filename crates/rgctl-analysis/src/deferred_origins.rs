//! Bounded possible-value models for explicit deferred handoffs.
use super::*;

#[derive(Default)]
pub(super) struct DeferredState {
    readers: HashMap<ValueRef, Vec<ValueRef>>,
    replayed: HashSet<(ValueRef, OriginId)>,
}

fn deferred_callee_name<'a>(facts: &'a FunctionOperands, site: &CallSiteFact) -> Option<&'a str> {
    match facts.expressions.get(site.callee.0 as usize)?.kind {
        ExpressionKind::Name(name) | ExpressionKind::Member(name) => facts.operand_name(name),
        _ => None,
    }
}

impl<C: InterproceduralCfgAccess + ?Sized> Query<'_, C> {
    pub(super) fn register_deferred(&mut self, function: Uuid) {
        let Some(cfg) = self.icfg.get_cfg(function) else {
            return;
        };
        let language = cfg.operands.language;
        if !matches!(
            language,
            OperandLanguage::Go
                | OperandLanguage::Python
                | OperandLanguage::JavaScript
                | OperandLanguage::TypeScript
                | OperandLanguage::Php
                | OperandLanguage::CSharp
                | OperandLanguage::Dart
        ) {
            return;
        }
        for (index, site) in cfg.operands.calls.iter().enumerate() {
            if !self.step() {
                break;
            }
            let call = CallRef {
                caller: function,
                site: CallSiteId(index as u32),
            };
            let result = ValueRef {
                function,
                expression: site.result,
            };
            let arguments = site.arguments.slice(&cfg.operands.arguments).unwrap_or(&[]);
            let name = deferred_callee_name(&cfg.operands, site);
            let receiver = site.receiver.map(|expression| ValueRef {
                function,
                expression,
            });
            if language == OperandLanguage::Python {
                let source = if site.receiver.is_none() && matches!(name, Some("next" | "anext")) {
                    arguments.first().map(|a| ValueRef {
                        function,
                        expression: a.value,
                    })
                } else if matches!(name, Some("__next__" | "__anext__")) {
                    receiver
                } else {
                    None
                };
                if let Some(source) = source {
                    self.memory_access(source, MemoryMember::Yielded, result, false);
                }
                let asyncio_runner = name == Some("run")
                    && site.receiver.is_some_and(|receiver| {
                        matches!(
                            cfg.operands.expressions[receiver.0 as usize].kind,
                            ExpressionKind::Name(n)
                                if cfg.operands.operand_name(n) == Some("asyncio")
                        )
                    });
                if asyncio_runner && let Some(first) = arguments.first() {
                    self.edge(
                        ValueRef {
                            function,
                            expression: first.value,
                        },
                        result,
                        OriginTransferCause::DeferredCall { call },
                    );
                }
            }
            if language == OperandLanguage::Go
                && name == Some("make")
                && site.receiver.is_none()
                && arguments.first().is_some_and(|a| {
                    cfg.operands.expressions[a.value.0 as usize].kind == ExpressionKind::ChannelType
                })
            {
                self.seed(OriginSeed {
                    value: result,
                    type_id: None,
                });
            }
            if language == OperandLanguage::Php
                && let Some(receiver) = receiver
            {
                let member = match name {
                    Some("current") => Some(MemoryMember::Yielded),
                    Some("getReturn") => Some(MemoryMember::GeneratorReturn),
                    _ => None,
                };
                if let Some(member) = member {
                    self.memory_access(receiver, member, result, false);
                }
            }
            // Iterator-result/enumerator objects retain opaque identity. Their
            // value/current read later links to the originating generator slot.
            if receiver.is_some()
                && matches!(
                    (language, name),
                    (
                        OperandLanguage::JavaScript | OperandLanguage::TypeScript,
                        Some("next")
                    ) | (
                        OperandLanguage::CSharp,
                        Some("GetEnumerator" | "GetAsyncEnumerator")
                    )
                )
            {
                self.seed(OriginSeed {
                    value: result,
                    type_id: None,
                });
            }
        }
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            if !self.step() {
                break;
            }
            let ExpressionKind::Member(name) = expression.kind else {
                continue;
            };
            let Some(&receiver) = expression
                .inputs
                .slice(&cfg.operands.inputs)
                .and_then(|s| s.first())
            else {
                continue;
            };
            let value = ValueRef {
                function,
                expression: ExpressionId(index as u32),
            };
            let name = cfg.operands.operand_name(name);
            if language == OperandLanguage::Dart && name == Some("iterator") {
                self.seed(OriginSeed {
                    value,
                    type_id: None,
                });
            }
            if matches!(
                (language, name),
                (
                    OperandLanguage::JavaScript | OperandLanguage::TypeScript,
                    Some("value")
                ) | (OperandLanguage::CSharp, Some("Current"))
                    | (OperandLanguage::Dart, Some("current"))
            ) {
                if self.field_access_count >= self.limits.max_transfers {
                    self.limit();
                    break;
                }
                self.field_access_count += 1;
                self.deferred
                    .readers
                    .entry(ValueRef {
                        function,
                        expression: receiver,
                    })
                    .or_default()
                    .push(value);
            }
        }
    }

    pub(super) fn deferred_fact(&mut self, value: ValueRef, origin: OriginId) {
        let Some(readers) = self.deferred.readers.get(&value) else {
            return;
        };
        if !self.deferred.replayed.insert((value, origin)) {
            return;
        }
        let readers = readers.clone();
        let seed = self.output.seeds[origin.0 as usize];
        if seed.type_id.is_some() {
            return;
        }
        let Some(cfg) = self.icfg.get_cfg(seed.value.function) else {
            return;
        };
        let facts = &cfg.operands;
        let mut terminal = false;
        let source = match facts.expressions[seed.value.expression.0 as usize].kind {
            ExpressionKind::CallResult(site) => {
                let site = &facts.calls[site.0 as usize];
                let name = deferred_callee_name(facts, site);
                if matches!(
                    (facts.language, name),
                    (
                        OperandLanguage::JavaScript | OperandLanguage::TypeScript,
                        Some("next")
                    ) | (
                        OperandLanguage::CSharp,
                        Some("GetEnumerator" | "GetAsyncEnumerator")
                    )
                ) {
                    terminal = matches!(
                        facts.language,
                        OperandLanguage::JavaScript | OperandLanguage::TypeScript
                    );
                    site.receiver
                } else {
                    None
                }
            }
            ExpressionKind::Member(name)
                if facts.language == OperandLanguage::Dart
                    && facts.operand_name(name) == Some("iterator") =>
            {
                facts.expressions[seed.value.expression.0 as usize]
                    .inputs
                    .slice(&facts.inputs)
                    .and_then(|s| s.first())
                    .copied()
            }
            _ => None,
        };
        let Some(source) = source else {
            return;
        };
        let source = ValueRef {
            function: seed.value.function,
            expression: source,
        };
        for reader in readers {
            if !self.step() {
                break;
            }
            self.memory_access(source, MemoryMember::Yielded, reader, false);
            if terminal {
                self.memory_access(source, MemoryMember::GeneratorReturn, reader, false);
            }
        }
    }

    pub(super) fn bind_yields(&mut self, target: InvocationTarget) {
        let Some(callee) = self.icfg.get_cfg(target.callee) else {
            return;
        };
        if !callee.operands.generator {
            return;
        }
        let Some(caller) = self.icfg.get_cfg(target.call.caller) else {
            return;
        };
        let Some(site) = caller.operands.calls.get(target.call.site.0 as usize) else {
            return;
        };
        let result = ValueRef {
            function: target.call.caller,
            expression: site.result,
        };
        self.seed(OriginSeed {
            value: result,
            type_id: None,
        });
        for &value in &callee.operands.yields {
            if !self.step() {
                return;
            }
            self.memory_access(
                result,
                MemoryMember::Yielded,
                ValueRef {
                    function: target.callee,
                    expression: value,
                },
                true,
            );
        }
        for returned in &callee.operands.returns {
            for &value in returned
                .values
                .slice(&callee.operands.inputs)
                .unwrap_or(&[])
            {
                if !self.step() {
                    return;
                }
                self.memory_access(
                    result,
                    MemoryMember::GeneratorReturn,
                    ValueRef {
                        function: target.callee,
                        expression: value,
                    },
                    true,
                );
            }
        }
    }
}
