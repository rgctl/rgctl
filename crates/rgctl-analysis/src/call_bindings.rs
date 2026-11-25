//! Shared, query-local argument/receiver/return transfers for selected targets.
//!
//! Target selection is an input: a callable Calls edge alone does not identify
//! which of several source invocations it describes. No name-only resolution
//! or operand parsing is performed here.

use crate::cfg::{CfgEdgeType, ControlFlowGraph};
use crate::interprocedural_cfg::InterproceduralCfgAccess;
use crate::operands::*;
use rgctl_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[cfg(test)]
#[path = "call_binding_tests.rs"]
mod tests;

/// One invocation in one selected procedural graph/revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallRef {
    /// Canonical enclosing callable UUID.
    pub caller: Uuid,
    /// Function-local invocation ID.
    pub site: CallSiteId,
}

/// An operand qualified by its canonical enclosing callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueRef {
    /// Enclosing callable UUID.
    pub function: Uuid,
    /// Function-local expression ID.
    pub expression: ExpressionId,
}

/// Receiver supply selected by the call resolver, not guessed from names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReceiverSupply {
    /// Ordinary/unbound call; explicit actuals retain declaration positions.
    None,
    /// Use the member invocation's receiver expression.
    Member,
    /// A method expression supplies the receiver as its first positional actual.
    FirstArgument,
    /// The allocated invocation result supplies a constructor's receiver.
    Constructed,
    /// A callable value retains a receiver expression from another invocation.
    BoundValue(ValueRef),
}

/// A resolver-selected target for a particular invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvocationTarget {
    /// Invocation identity.
    pub call: CallRef,
    /// Selected canonical possible callee UUID.
    pub callee: Uuid,
    /// Bound/unbound/constructor convention established by target resolution.
    pub receiver: ReceiverSupply,
}

/// Implicit values introduced by language-level binding rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImplicitValue {
    /// Missing JavaScript/TypeScript positional actual or normal fallthrough.
    Undefined,
    /// Python normal fallthrough.
    None,
    /// Empty positional rest container.
    EmptyRest,
    /// Empty keyword rest dictionary.
    EmptyKeywords,
    /// Value cannot yet be modeled; diagnostics must accompany this source.
    Unknown,
}

/// Source of a transfer, including values without a source expression token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingSource {
    /// Native expression value.
    Expression(ValueRef),
    /// Language-defined implicit value.
    Implicit(ImplicitValue),
}

/// When a default's expression is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultEvaluation {
    /// Python defaults are evaluated when the function is defined.
    Definition,
    /// JavaScript/TypeScript defaults are evaluated for the invocation.
    Invocation,
}

/// Typed transfer semantics; container membership is not whole-object aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferKind {
    /// An actual expression supplies one explicit formal.
    ActualToFormal,
    /// Receiver supplies the selected receiver formal.
    ReceiverToFormal,
    /// A default supplies the formal, possibly when an actual is undefined.
    DefaultToFormal {
        /// Default evaluation context.
        evaluation: DefaultEvaluation,
        /// True when the default is only an alternative for an unknown actual.
        conditional: bool,
    },
    /// One positional actual becomes an element of a rest container.
    RestElement {
        /// Index among explicitly known rest elements.
        index: u32,
    },
    /// Source elements contribute to a rest container without nesting it.
    RestSpread,
    /// A named actual becomes a keyword-rest element.
    KeywordElement {
        /// Key in the caller's interned name pool.
        name: OperandNameId,
    },
    /// Unknown keyword elements contribute to a keyword-rest dictionary.
    KeywordSpread,
    /// Unknown spread may supply this formal; mapping is conservative.
    PossibleSpreadToFormal,
    /// An implicit value supplies a missing/empty formal.
    ImplicitToFormal,
    /// Return value contributes to the invocation result at a position.
    ReturnToResult {
        /// Go multiple-return slot; ordinary scalar languages use zero.
        position: u32,
    },
    /// Normal function fallthrough supplies an implicit result.
    FallthroughToResult,
}

/// A value-transfer edge for one target binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueTransfer {
    /// Produced/passed value.
    pub from: BindingSource,
    /// Receiving formal or invocation result.
    pub to: ValueRef,
    /// Binding semantics.
    pub kind: TransferKind,
}

/// Reason the selected invocation cannot have a fully established mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingDiagnostic {
    /// Caller procedural source/operands are unavailable.
    MissingCaller,
    /// Callee procedural source/operands are unavailable.
    MissingCallee,
    /// Native operand capture reported unsupported/malformed subforms.
    PartialOperands,
    /// Invalid operand/range reference in a supplied in-memory record.
    InvalidOperand,
    /// Cross-language calling convention is not modeled.
    LanguageBoundary,
    /// Required receiver was not supplied or its role is not modeled.
    ReceiverConvention,
    /// Python class receiver requires class-object resolution.
    ClassReceiver,
    /// Required actual is missing.
    MissingArgument,
    /// Extra actual has no ordinary/rest formal or arguments-object model.
    ExtraArgument,
    /// Unknown named argument without keyword rest.
    UnexpectedKeyword,
    /// Two explicit actuals bind the same formal.
    DuplicateArgument,
    /// Unknown positional spread has unknown cardinality.
    PositionalSpread,
    /// Unknown keyword spread has unknown keys.
    KeywordSpread,
    /// Named/rest/fallthrough convention is not modeled for this language.
    UnsupportedConvention,
    /// The per-invocation transfer expansion budget was reached.
    LimitReached,
}

/// Ranges into one query's shared transfer/diagnostic pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallBinding {
    /// Selected source invocation and target.
    pub target: InvocationTarget,
    /// Range into [`CallBindings::transfers`].
    pub transfers: OperandRange,
    /// Range into [`CallBindings::diagnostics`]. Empty is not a safety verdict.
    pub diagnostics: OperandRange,
}

/// Per-invocation limits; a larger query must also bound its own worklist.
#[derive(Debug, Clone, Copy)]
pub struct BindingLimits {
    /// Maximum emitted transfers for one invocation/target pair.
    pub max_transfers_per_call: usize,
}

impl Default for BindingLimits {
    fn default() -> Self {
        Self {
            max_transfers_per_call: 4096,
        }
    }
}

/// Borrowed procedural graph plus query-local pooled call mappings.
pub struct CallBindings<'a, C: InterproceduralCfgAccess + ?Sized> {
    icfg: &'a C,
    limits: BindingLimits,
    bindings: HashMap<InvocationTarget, CallBinding>,
    /// Typed edges from selected invocation mappings.
    transfers: Vec<ValueTransfer>,
    /// Coverage/arity/convention evidence for those mappings.
    diagnostics: Vec<BindingDiagnostic>,
}

impl<'a, C: InterproceduralCfgAccess + ?Sized> CallBindings<'a, C> {
    /// Bind only against this graph's procedural records; snapshots are not mixed.
    pub fn new(icfg: &'a C) -> Self {
        Self::with_limits(icfg, BindingLimits::default())
    }

    /// Create an arena with an explicit per-invocation expansion budget.
    pub fn with_limits(icfg: &'a C, limits: BindingLimits) -> Self {
        Self {
            icfg,
            limits,
            bindings: HashMap::new(),
            transfers: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Read pooled transfer edges without invalidating cached mapping ranges.
    pub fn transfers(&self) -> &[ValueTransfer] {
        &self.transfers
    }

    /// Read pooled diagnostics without invalidating cached mapping ranges.
    pub fn diagnostics(&self) -> &[BindingDiagnostic] {
        &self.diagnostics
    }

    /// Map a resolver-selected invocation/target pair without selecting targets.
    ///
    /// This does not assert that `target.callee` was selected by extraction or
    /// that a structural callable edge establishes this particular call site.
    pub fn bind_invocation(&mut self, target: InvocationTarget) -> Result<CallBinding> {
        if let Some(&binding) = self.bindings.get(&target) {
            return Ok(binding);
        }
        let start = u32_index(self.transfers.len())?;
        let diagnostic_start = u32_index(self.diagnostics.len())?;
        let caller = self.icfg.get_cfg(target.call.caller);
        let callee = self.icfg.get_cfg(target.callee);
        if caller.is_none_or(|c| c.operands.status != OperandStatus::Captured) {
            self.diagnostics.push(BindingDiagnostic::MissingCaller);
        }
        if callee.is_none_or(|c| c.operands.status != OperandStatus::Captured) {
            self.diagnostics.push(BindingDiagnostic::MissingCallee);
        }
        if let (Some(caller), Some(callee)) = (caller, callee)
            && caller.operands.status == OperandStatus::Captured
            && callee.operands.status == OperandStatus::Captured
        {
            let caller_facts = &caller.operands;
            let callee_facts = &callee.operands;
            let call = caller_facts
                .calls
                .get(target.call.site.0 as usize)
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "call site {} in {}",
                        target.call.site.0, target.call.caller
                    ))
                })?;
            if !caller_facts.diagnostics.is_empty() || !callee_facts.diagnostics.is_empty() {
                self.diagnostics.push(BindingDiagnostic::PartialOperands);
            }
            if caller_facts.language != callee_facts.language
                && !(ecma(caller_facts.language) && ecma(callee_facts.language))
            {
                self.diagnostics.push(BindingDiagnostic::LanguageBoundary);
            } else {
                let bound_receiver = if let ReceiverSupply::BoundValue(value) = target.receiver {
                    if self.icfg.get_cfg(value.function).is_some_and(|cfg| {
                        (value.expression.0 as usize) < cfg.operands.expressions.len()
                    }) {
                        Some(value)
                    } else {
                        self.diagnostics.push(BindingDiagnostic::InvalidOperand);
                        None
                    }
                } else {
                    None
                };
                let mut binder = Binder {
                    target,
                    bound_receiver,
                    caller: caller_facts,
                    callee: callee_facts,
                    call,
                    transfers: &mut self.transfers,
                    diagnostics: &mut self.diagnostics,
                    start: start as usize,
                    diagnostic_start: diagnostic_start as usize,
                    limit: self.limits.max_transfers_per_call,
                    limited: false,
                };
                binder.bind_actuals()?;
                binder.bind_return_values(callee)?;
                if binder.limited {
                    binder.record_binding_diagnostic(BindingDiagnostic::LimitReached);
                }
            }
        }
        let binding = CallBinding {
            target,
            transfers: OperandRange {
                start,
                len: u32_index(self.transfers.len())? - start,
            },
            diagnostics: OperandRange {
                start: diagnostic_start,
                len: u32_index(self.diagnostics.len())? - diagnostic_start,
            },
        };
        self.bindings.insert(target, binding);
        Ok(binding)
    }
}

fn u32_index(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::SerdeError("call binding pool exceeds u32".into()))
}

fn ecma(language: OperandLanguage) -> bool {
    matches!(
        language,
        OperandLanguage::JavaScript | OperandLanguage::TypeScript
    )
}

struct Binder<'a> {
    target: InvocationTarget,
    bound_receiver: Option<ValueRef>,
    caller: &'a FunctionOperands,
    callee: &'a FunctionOperands,
    call: &'a CallSiteFact,
    transfers: &'a mut Vec<ValueTransfer>,
    diagnostics: &'a mut Vec<BindingDiagnostic>,
    start: usize,
    diagnostic_start: usize,
    limit: usize,
    limited: bool,
}

impl Binder<'_> {
    fn record_binding_diagnostic(&mut self, diagnostic: BindingDiagnostic) {
        if !self.diagnostics[self.diagnostic_start..].contains(&diagnostic) {
            self.diagnostics.push(diagnostic);
        }
    }

    fn append_binding_transfer(&mut self, transfer: ValueTransfer) {
        if self.transfers.len() - self.start >= self.limit {
            self.limited = true;
            return;
        }
        self.transfers.push(transfer);
    }

    fn emit(&mut self, source: BindingSource, formal: usize, kind: TransferKind) {
        let Some(f) = self.callee.formals.get(formal) else {
            self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
            return;
        };
        if self.callee.expressions.get(f.value.0 as usize).is_none() {
            self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
            return;
        }
        self.append_binding_transfer(ValueTransfer {
            from: source,
            to: ValueRef {
                function: self.target.callee,
                expression: f.value,
            },
            kind,
        });
    }

    fn actual(&self, value: ExpressionId) -> Option<BindingSource> {
        self.caller.expressions.get(value.0 as usize)?;
        Some(BindingSource::Expression(ValueRef {
            function: self.target.call.caller,
            expression: value,
        }))
    }

    fn default(&mut self, formal: usize, conditional: bool) {
        let f = &self.callee.formals[formal];
        if let Some(value) = f.default_value {
            if self.callee.expressions.get(value.0 as usize).is_none() {
                self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
                return;
            }
            let evaluation = if self.callee.language == OperandLanguage::Python {
                DefaultEvaluation::Definition
            } else {
                DefaultEvaluation::Invocation
            };
            self.emit(
                BindingSource::Expression(ValueRef {
                    function: self.target.callee,
                    expression: value,
                }),
                formal,
                TransferKind::DefaultToFormal {
                    evaluation,
                    conditional,
                },
            );
        }
    }

    fn bind_actuals(&mut self) -> Result<()> {
        let Some(arguments) = self.call.arguments.slice(&self.caller.arguments) else {
            self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
            return Ok(());
        };
        let mut assigned = vec![false; self.callee.formals.len()];
        let mut positional = Vec::new();
        let mut keywords = HashMap::new();
        let mut keyword_order = Vec::new();
        let mut rest = None;
        let mut keyword_rest = None;
        let mut receiver = None;
        for (index, formal) in self.callee.formals.iter().enumerate() {
            match formal.kind {
                FormalKind::Receiver => receiver = Some(index),
                FormalKind::Positional(position) | FormalKind::PositionalOnly(position) => {
                    positional.push((position, index))
                }
                FormalKind::VariadicPositional => rest = Some(index),
                FormalKind::VariadicKeyword => keyword_rest = Some(index),
                FormalKind::KeywordOnly => {}
            }
            if matches!(
                formal.kind,
                FormalKind::Positional(_) | FormalKind::KeywordOnly
            ) && let Some(name) = self.callee.operand_name(formal.name)
            {
                keywords.insert(name, index);
                keyword_order.push(index);
            }
        }
        positional.sort_by_key(|&(position, _)| position);
        let python_bound = matches!(
            self.callee.receiver_convention,
            ReceiverConvention::PythonDescriptor | ReceiverConvention::PythonClassDescriptor
        ) && matches!(
            self.target.receiver,
            ReceiverSupply::Member | ReceiverSupply::Constructed | ReceiverSupply::BoundValue(_)
        );
        let receiver_in_rest = python_bound && positional.is_empty() && rest.is_some();
        if python_bound {
            receiver = positional.first().map(|&(_, index)| index).or(rest);
            positional.retain(|&(_, index)| Some(index) != receiver);
        }
        let mut skip_first =
            self.target.receiver == ReceiverSupply::FirstArgument && !arguments.is_empty();
        if let Some(formal) = receiver {
            let kind = if receiver_in_rest {
                TransferKind::RestElement { index: 0 }
            } else {
                TransferKind::ReceiverToFormal
            };
            let source = match self.target.receiver {
                ReceiverSupply::None => None,
                ReceiverSupply::Member => self.call.receiver.and_then(|value| self.actual(value)),
                ReceiverSupply::FirstArgument => {
                    if let Some(argument) = arguments
                        .first()
                        .filter(|a| a.kind == ArgumentKind::Positional)
                    {
                        skip_first = true;
                        self.actual(argument.value)
                    } else {
                        None
                    }
                }
                ReceiverSupply::Constructed => self.actual(self.call.result),
                ReceiverSupply::BoundValue(_) => self.bound_receiver.map(BindingSource::Expression),
            };
            if self.callee.receiver_convention == ReceiverConvention::PythonClassDescriptor {
                self.record_binding_diagnostic(BindingDiagnostic::ClassReceiver);
                self.emit(
                    BindingSource::Implicit(ImplicitValue::Unknown),
                    formal,
                    kind,
                );
            } else if let Some(source) = source {
                self.emit(source, formal, kind);
            } else {
                self.record_binding_diagnostic(BindingDiagnostic::ReceiverConvention);
            }
            assigned[formal] = true;
        } else if self.target.receiver != ReceiverSupply::None {
            self.record_binding_diagnostic(BindingDiagnostic::ReceiverConvention);
        }
        let mut cursor = 0usize;
        let mut rest_index = u32::from(receiver_in_rest);
        let mut uncertain_positions = self
            .callee
            .diagnostics
            .iter()
            .any(|d| d.kind == OperandDiagnosticKind::ParameterPattern);
        let mut uncertain_keywords = false;
        for (argument_index, argument) in arguments.iter().enumerate() {
            if self.limited {
                break;
            }
            if skip_first && argument_index == 0 {
                continue;
            }
            let Some(source) = self.actual(argument.value) else {
                self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
                continue;
            };
            let remaining = if self.callee.language == OperandLanguage::Bash {
                positional.partition_point(|&(position, _)| (position as usize) < cursor)
            } else {
                cursor
            };
            match argument.kind {
                ArgumentKind::Positional
                    if self.callee.language == OperandLanguage::Bash && !uncertain_positions =>
                {
                    if let Ok(index) = positional
                        .binary_search_by_key(&u32_index(cursor)?, |&(position, _)| position)
                    {
                        let formal = positional[index].1;
                        self.emit(source, formal, TransferKind::ActualToFormal);
                        assigned[formal] = true;
                    }
                    // Shell functions accept unused arguments. Captured parameters
                    // can be sparse (`$2` without `$1`); never compact their slots.
                    cursor += 1;
                }
                ArgumentKind::Positional if uncertain_positions => {
                    for &(_, formal) in &positional[remaining..] {
                        if self.limited {
                            break;
                        }
                        self.emit(source, formal, TransferKind::PossibleSpreadToFormal);
                    }
                    if let Some(formal) = rest {
                        self.emit(
                            source,
                            formal,
                            TransferKind::RestElement { index: rest_index },
                        );
                        rest_index += 1;
                    }
                }
                ArgumentKind::Positional => {
                    if let Some(&(_, formal)) = positional.get(cursor) {
                        if assigned[formal] {
                            self.record_binding_diagnostic(BindingDiagnostic::DuplicateArgument);
                        }
                        self.emit(source, formal, TransferKind::ActualToFormal);
                        assigned[formal] = true;
                        if ecma(self.callee.language)
                            && !definitely_defined(self.caller, argument.value, 0)
                        {
                            self.default(formal, true);
                        }
                        cursor += 1;
                    } else if let Some(formal) = rest {
                        self.emit(
                            source,
                            formal,
                            TransferKind::RestElement { index: rest_index },
                        );
                        rest_index += 1;
                        assigned[formal] = true;
                    } else if !ecma(self.callee.language) {
                        self.record_binding_diagnostic(BindingDiagnostic::ExtraArgument);
                    } else {
                        self.record_binding_diagnostic(BindingDiagnostic::UnsupportedConvention);
                    }
                }
                ArgumentKind::Named(name_id) => {
                    if self.callee.language != OperandLanguage::Python {
                        self.record_binding_diagnostic(BindingDiagnostic::UnsupportedConvention);
                    }
                    let name = self.caller.operand_name(name_id);
                    if let Some(&formal) = name.and_then(|name| keywords.get(name)) {
                        if assigned[formal] {
                            self.record_binding_diagnostic(BindingDiagnostic::DuplicateArgument);
                        }
                        self.emit(source, formal, TransferKind::ActualToFormal);
                        assigned[formal] = true;
                    } else if let Some(formal) = keyword_rest {
                        self.emit(
                            source,
                            formal,
                            TransferKind::KeywordElement { name: name_id },
                        );
                        assigned[formal] = true;
                    } else {
                        self.record_binding_diagnostic(BindingDiagnostic::UnexpectedKeyword);
                    }
                }
                ArgumentKind::SpreadPositional => {
                    self.record_binding_diagnostic(BindingDiagnostic::PositionalSpread);
                    for &(_, formal) in &positional[remaining..] {
                        if self.limited {
                            break;
                        }
                        self.emit(source, formal, TransferKind::PossibleSpreadToFormal);
                    }
                    if let Some(formal) = rest {
                        self.emit(source, formal, TransferKind::RestSpread);
                    }
                    uncertain_positions = true;
                }
                ArgumentKind::SpreadKeywords => {
                    self.record_binding_diagnostic(BindingDiagnostic::KeywordSpread);
                    for &formal in &keyword_order {
                        if self.limited {
                            break;
                        }
                        if !assigned[formal] {
                            self.emit(source, formal, TransferKind::PossibleSpreadToFormal);
                        }
                    }
                    if let Some(formal) = keyword_rest {
                        self.emit(source, formal, TransferKind::KeywordSpread);
                    }
                    uncertain_keywords = true;
                }
            }
        }
        for (formal, is_assigned) in assigned.into_iter().enumerate() {
            if self.limited {
                break;
            }
            if is_assigned {
                continue;
            }
            let f = &self.callee.formals[formal];
            let maybe_supplied = match f.kind {
                FormalKind::Positional(_) => uncertain_positions || uncertain_keywords,
                FormalKind::PositionalOnly(_) => uncertain_positions,
                FormalKind::KeywordOnly => uncertain_keywords,
                FormalKind::VariadicPositional => uncertain_positions,
                FormalKind::VariadicKeyword => uncertain_keywords,
                FormalKind::Receiver => false,
            };
            if f.default_value.is_some() {
                self.default(formal, maybe_supplied);
            } else if matches!(
                f.kind,
                FormalKind::VariadicPositional | FormalKind::VariadicKeyword
            ) {
                let value = if f.kind == FormalKind::VariadicPositional {
                    ImplicitValue::EmptyRest
                } else {
                    ImplicitValue::EmptyKeywords
                };
                self.emit(
                    BindingSource::Implicit(value),
                    formal,
                    TransferKind::ImplicitToFormal,
                );
            } else if ecma(self.callee.language) {
                self.emit(
                    BindingSource::Implicit(ImplicitValue::Undefined),
                    formal,
                    TransferKind::ImplicitToFormal,
                );
            } else if !maybe_supplied {
                self.record_binding_diagnostic(BindingDiagnostic::MissingArgument);
            }
        }
        Ok(())
    }

    fn bind_return_values(&mut self, cfg: &ControlFlowGraph) -> Result<()> {
        if self.limited || self.target.receiver == ReceiverSupply::Constructed {
            return Ok(());
        }
        if self
            .caller
            .expressions
            .get(self.call.result.0 as usize)
            .is_none()
        {
            self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
            return Ok(());
        }
        let to = ValueRef {
            function: self.target.call.caller,
            expression: self.call.result,
        };
        for returned in &self.callee.returns {
            let Some(values) = returned.values.slice(&self.callee.inputs) else {
                self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
                continue;
            };
            if values.is_empty() {
                let value = match self.callee.language {
                    OperandLanguage::Python => Some(ImplicitValue::None),
                    language if ecma(language) => Some(ImplicitValue::Undefined),
                    OperandLanguage::Go
                        if self
                            .callee
                            .diagnostics
                            .iter()
                            .any(|d| d.kind == OperandDiagnosticKind::ReturnConvention) =>
                    {
                        Some(ImplicitValue::Unknown)
                    }
                    _ => None,
                };
                if let Some(value) = value {
                    self.append_binding_transfer(ValueTransfer {
                        from: BindingSource::Implicit(value),
                        to,
                        kind: TransferKind::ReturnToResult { position: 0 },
                    });
                }
            }
            for (position, &expression) in values.iter().enumerate() {
                if self.limited {
                    return Ok(());
                }
                if self.callee.expressions.get(expression.0 as usize).is_none() {
                    self.record_binding_diagnostic(BindingDiagnostic::InvalidOperand);
                    continue;
                }
                self.append_binding_transfer(ValueTransfer {
                    from: BindingSource::Expression(ValueRef {
                        function: self.target.callee,
                        expression,
                    }),
                    to,
                    kind: TransferKind::ReturnToResult {
                        position: u32_index(position)?,
                    },
                });
            }
        }
        let falls_through = !self.callee.returns.iter().any(|r| r.implicit)
            && (cfg.exits.contains(&cfg.entry)
                || cfg.edges.iter().any(|edge| {
                    cfg.exits.contains(&edge.to)
                        && !matches!(edge.edge_type, CfgEdgeType::Return | CfgEdgeType::Exception)
                }));
        if falls_through {
            let value = match self.callee.language {
                OperandLanguage::Python => ImplicitValue::None,
                language if ecma(language) => ImplicitValue::Undefined,
                _ => {
                    self.record_binding_diagnostic(BindingDiagnostic::UnsupportedConvention);
                    ImplicitValue::Unknown
                }
            };
            self.append_binding_transfer(ValueTransfer {
                from: BindingSource::Implicit(value),
                to,
                kind: TransferKind::FallthroughToResult,
            });
        }
        Ok(())
    }
}

fn definitely_defined(facts: &FunctionOperands, value: ExpressionId, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    let Some(expression) = facts.expressions.get(value.0 as usize) else {
        return false;
    };
    match expression.kind {
        ExpressionKind::Literal | ExpressionKind::Construction(_) => true,
        ExpressionKind::CallResult(id) => facts
            .calls
            .get(id.0 as usize)
            .is_some_and(|c| c.kind == InvocationKind::Construction),
        ExpressionKind::Parenthesized => expression
            .inputs
            .slice(&facts.inputs)
            .and_then(|inputs| inputs.first())
            .is_some_and(|&id| definitely_defined(facts, id, depth + 1)),
        _ => false,
    }
}
