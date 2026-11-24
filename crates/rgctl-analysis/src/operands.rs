//! Compact, source-native operand roles for one callable's procedural record.
//!
//! IDs are local to a source/semantics revision, not cross-snapshot identities.
//! Name IDs retain spelling; local slot/use/write pools carry scoped bindings.

use serde::{Deserialize, Serialize};

#[path = "operand_lowering.rs"]
mod lowering;
pub(crate) use lowering::{local_write_sources, lower_function_operands};

#[cfg(test)]
#[path = "operand_tests.rs"]
mod tests;

/// Function-local expression index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExpressionId(pub u32);
/// Function-local invocation index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallSiteId(pub u32);
/// Function-local formal index, including explicit receiver records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FormalId(pub u32);
/// Index into the function's interned names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperandNameId(pub u32);

/// Checked, exclusive source-file byte span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperandSpan {
    /// Inclusive start byte.
    pub start: u32,
    /// Exclusive end byte.
    pub end: u32,
}

/// Contiguous range in a shared function-local operand pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperandRange {
    /// First index.
    pub start: u32,
    /// Element count.
    pub len: u32,
}

impl OperandRange {
    /// Resolve a range without trusting serialized indices.
    pub fn slice<T>(self, pool: &[T]) -> Option<&[T]> {
        let end = self.start.checked_add(self.len)?;
        pool.get(self.start as usize..end as usize)
    }
}

/// Native language binding rules to apply in a subsequent binder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperandLanguage {
    /// No supported language was lowered.
    #[default]
    Unsupported,
    /// Java.
    Java,
    /// JavaScript.
    JavaScript,
    /// TypeScript/TSX.
    TypeScript,
    /// Python.
    Python,
    /// Go.
    Go,
    /// Rust.
    Rust,
    /// C native operands.
    C,
    /// Cpp native operands.
    Cpp,
    /// CSharp native operands.
    CSharp,
    /// Groovy native operands.
    Groovy,
    /// Kotlin native operands.
    Kotlin,
    /// Php native operands.
    Php,
    /// Puppet native operands.
    Puppet,
    /// Ruby native operands.
    Ruby,
    /// Swift native operands.
    Swift,
    /// Scala native operands.
    Scala,
    /// Dart native operands.
    Dart,
    /// Elixir native operands.
    Elixir,
    /// Lua native operands.
    Lua,
    /// Bash native operands.
    Bash,
}

/// Scope of the syntactic operand capture, not a safety verdict.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperandStatus {
    /// Manually constructed CFG without operand lowering.
    #[default]
    NotBuilt,
    /// Language outside the currently supported operand lowerers.
    UnsupportedLanguage,
    /// Native syntax captured; diagnostics identify unsupported subforms.
    Captured,
}

/// Role of a native expression. Computation inputs are not necessarily aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpressionKind {
    /// Name occurrence; local_uses carries captured scoped read bindings.
    Name(OperandNameId),
    /// Member selection; inputs contain the receiver expression.
    Member(OperandNameId),
    /// Explicit parameter entry value.
    Formal(FormalId),
    /// Result of one particular invocation.
    CallResult(CallSiteId),
    /// Non-call aggregate construction, referring to its type expression.
    Construction(ExpressionId),
    /// A literal constant.
    Literal,
    /// Parentheses preserving an inner expression.
    Parenthesized,
    /// Conditional selection; inputs are value alternatives, not the condition.
    Conditional {
        /// Boolean/truth-tested control expression; never a selected-value alias.
        condition: ExpressionId,
    },
    /// Assertion or reference cast preserving the operand on a successful path.
    AssertedValue,
    /// Assignment from a tuple-valued expression; first input is the result.
    Projection {
        /// Declared result position; consumers may conservatively merge slots.
        position: u32,
    },
    /// Other computation with ordered syntactic children.
    Computation,
    /// Unsupported, malformed or depth-limited expression.
    Unknown,
}

/// One expression with a source identity and pooled ordered child operands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionFact {
    /// Exact source span; synthetic receiver formals have no token span.
    pub span: Option<OperandSpan>,
    /// Expression role.
    pub kind: ExpressionKind,
    /// Range into [`FunctionOperands::inputs`].
    pub inputs: OperandRange,
}

/// Parameter role without confusing explicit indices with implicit receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalKind {
    /// Implicit Java receiver or separate Go/Rust receiver.
    Receiver,
    /// Ordinary explicit positional parameter.
    Positional(u32),
    /// Python parameter preceding `/`.
    PositionalOnly(u32),
    /// Python keyword-only parameter.
    KeywordOnly,
    /// Positional rest/variadic parameter.
    VariadicPositional,
    /// Python keyword rest parameter.
    VariadicKeyword,
}

/// Receiver convention retained for language-aware call binding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverConvention {
    /// No declared receiver convention.
    #[default]
    None,
    /// Synthetic `this` formal, separate from explicit argument positions.
    ImplicitThis,
    /// Python descriptor binding can supply the first explicit positional formal.
    PythonDescriptor,
    /// Python `classmethod` descriptor supplies the class as the first formal.
    PythonClassDescriptor,
    /// Go/Rust declare a receiver separately from ordinary arguments.
    Explicit,
}

/// One formal definition shared by invocation and local value transfers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormalFact {
    /// Interned name.
    pub name: OperandNameId,
    /// Entry value definition.
    pub value: ExpressionId,
    /// Binding convention.
    pub kind: FormalKind,
    /// Source default expression, if any; its execution timing is language-specific.
    pub default_value: Option<ExpressionId>,
    /// Missing explicit actual is allowed by a default, optional or rest form.
    pub optional: bool,
}

/// Syntax role of an actual argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArgumentKind {
    /// Explicit positional argument.
    Positional,
    /// Named argument with a pooled name.
    Named(OperandNameId),
    /// Positional spread/rest expansion.
    SpreadPositional,
    /// Keyword dictionary expansion.
    SpreadKeywords,
}

/// One actual argument in source evaluation order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgumentFact {
    /// Actual expression.
    pub value: ExpressionId,
    /// Language binding role.
    pub kind: ArgumentKind,
}

/// Syntactic invocation kind; Python class calls require later symbol resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationKind {
    /// Ordinary call expression.
    Call,
    /// Explicit `new`/object-creation syntax.
    Construction,
}

/// One invocation, including nested and repeated calls on the same source line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSiteFact {
    /// Exact invocation span.
    pub span: OperandSpan,
    /// Expression selecting the callable.
    pub callee: ExpressionId,
    /// Receiver expression for member invocations.
    pub receiver: Option<ExpressionId>,
    /// Range into [`FunctionOperands::arguments`].
    pub arguments: OperandRange,
    /// Result expression identifying this invocation.
    pub result: ExpressionId,
    /// Syntactic kind.
    pub kind: InvocationKind,
}

/// Explicit or language-defined implicit return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnFact {
    /// Return syntax or implicit result span.
    pub span: OperandSpan,
    /// Range into [`FunctionOperands::inputs`]; Go can return multiple values.
    pub values: OperandRange,
    /// True for an expression-bodied arrow or Rust tail expression.
    pub implicit: bool,
}

/// Reason that syntactic operand capture cannot claim full coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperandDiagnosticKind {
    /// Unsupported parameter pattern, such as destructuring.
    ParameterPattern,
    /// Missing or unsupported invocation operand.
    InvocationShape,
    /// Unmodeled return convention, such as named Go returns.
    ReturnConvention,
    /// Nested callable body deliberately belongs to a separate procedural record.
    NestedCallable,
    /// Malformed native syntax.
    MalformedSyntax,
    /// Local write RHS/pattern has no supported whole-value mapping.
    LocalWriteShape,
    /// Expression lowering reached its bounded recursion depth.
    DepthLimit,
}

/// Compact coverage evidence; source text is not duplicated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperandDiagnostic {
    /// Affected syntax.
    pub span: OperandSpan,
    /// Missing capability.
    pub kind: OperandDiagnosticKind,
}

/// Synthetic projected value defining one native assignment target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentValueFact {
    /// Exact declared/assigned identifier span.
    pub target: OperandSpan,
    /// Projected expression supplying this write.
    pub value: ExpressionId,
}

/// Pooled per-function native syntax facts, persisted with the CFG.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionOperands {
    /// Language defining invocation/parameter conventions.
    pub language: OperandLanguage,
    /// Capture status, independent of whether a callable has any calls.
    pub status: OperandStatus,
    /// How a bound invocation supplies a receiver.
    pub receiver_convention: ReceiverConvention,
    /// Enclosing callable source span.
    pub span: Option<OperandSpan>,
    /// Enclosing callable one-based source line, for cache relocation checks.
    pub source_line: Option<u32>,
    /// Names interned once per function.
    pub names: Vec<String>,
    /// Expression records indexed by [`ExpressionId`].
    pub expressions: Vec<ExpressionFact>,
    /// Pooled ordered expression references.
    pub inputs: Vec<ExpressionId>,
    /// Formal definitions in declaration order.
    pub formals: Vec<FormalFact>,
    /// Invocation records indexed by [`CallSiteId`].
    pub calls: Vec<CallSiteFact>,
    /// Pooled actual argument records.
    pub arguments: Vec<ArgumentFact>,
    /// Return sites and their value operands.
    pub returns: Vec<ReturnFact>,
    /// Projected RHS values for native multi-value assignments.
    pub assignment_values: Vec<AssignmentValueFact>,
    /// Scoped native local keys, indexed by LocalSlotId.
    pub local_slots: Vec<OperandNameId>,
    /// Exact local read occurrences and CFG positions.
    pub local_uses: Vec<LocalUseFact>,
    /// Exact local definitions, including incoming formals.
    pub local_writes: Vec<LocalWriteFact>,
    /// Explicit unsupported/malformed forms.
    pub diagnostics: Vec<OperandDiagnostic>,
}

impl FunctionOperands {
    /// Resolve a name without treating its spelling as symbol identity.
    pub fn operand_name(&self, id: OperandNameId) -> Option<&str> {
        self.names.get(id.0 as usize).map(String::as_str)
    }

    /// Check ranges and references before accepting persisted operand records.
    pub fn valid_references(&self) -> bool {
        let expr = |id: ExpressionId| (id.0 as usize) < self.expressions.len();
        let slot = |id: LocalSlotId| (id.0 as usize) < self.local_slots.len();
        let name = |id: OperandNameId| (id.0 as usize) < self.names.len();
        self.expressions.iter().all(|e| {
            e.inputs.slice(&self.inputs).is_some()
                && match e.kind {
                    ExpressionKind::Name(id) | ExpressionKind::Member(id) => name(id),
                    ExpressionKind::Formal(id) => (id.0 as usize) < self.formals.len(),
                    ExpressionKind::CallResult(id) => (id.0 as usize) < self.calls.len(),
                    ExpressionKind::Construction(id) => expr(id),
                    ExpressionKind::Conditional { condition } => expr(condition),
                    _ => true,
                }
        }) && self
            .assignment_values
            .iter()
            .all(|a| a.target.start < a.target.end && expr(a.value))
            && self.local_slots.iter().all(|&id| name(id))
            && self
                .local_uses
                .iter()
                .all(|u| expr(u.value) && slot(u.slot))
            && self
                .local_writes
                .iter()
                .all(|w| slot(w.slot) && w.value.is_none_or(expr))
            && self.inputs.iter().all(|&id| expr(id))
            && self
                .formals
                .iter()
                .all(|f| name(f.name) && expr(f.value) && f.default_value.is_none_or(expr))
            && self.arguments.iter().all(|a| {
                expr(a.value)
                    && match a.kind {
                        ArgumentKind::Named(id) => name(id),
                        _ => true,
                    }
            })
            && self.calls.iter().all(|c| {
                expr(c.callee)
                    && c.receiver.is_none_or(expr)
                    && expr(c.result)
                    && c.arguments.slice(&self.arguments).is_some()
            })
            && self
                .returns
                .iter()
                .all(|r| r.values.slice(&self.inputs).is_some())
    }
}

/// Dense function-local lexical slot selected by the native scope resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocalSlotId(pub u32);

/// Exact CFG position; source lines are not statement identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatementRef {
    /// Containing basic block.
    pub block: crate::cfg::BlockId,
    /// Statement index within that block, after native lowering.
    pub index: u32,
}

/// Scoped local read of a particular native expression occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalUseFact {
    /// Expression whose value is read.
    pub value: ExpressionId,
    /// Native lexical slot; spelling alone is not the binding.
    pub slot: LocalSlotId,
    /// Exact consuming CFG statement.
    pub statement: StatementRef,
}

/// One local definition and its native RHS, when modeled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalWriteFact {
    /// Native lexical slot being defined.
    pub slot: LocalSlotId,
    /// Formal entry or initializer/RHS value; absent for unmodeled writes.
    pub value: Option<ExpressionId>,
    /// Exact defining CFG statement.
    pub statement: StatementRef,
}
