# Query-local invocation bindings

`InterproceduralCFG::call_bindings()` and the archive-backed view's matching
method create a shared `CallBindings` arena. Given a particular invocation and a
resolver-selected canonical target, it produces typed actual/formal, receiver
and return/result transfers from the persisted function operand records.

```rust
let mut query = icfg.call_bindings();
let binding = query.bind_invocation(InvocationTarget {
    call: CallRef { caller: caller_uuid, site: CallSiteId(0) },
    callee: callee_uuid,
    receiver: ReceiverSupply::None,
})?;
let transfers = binding.transfers.slice(query.transfers());
let diagnostics = binding.diagnostics.slice(query.diagnostics());
```

Import these types from `rgctl_analysis::call_bindings` and `CallSiteId` from
`rgctl_analysis::operands`. A function-local expression reference is qualified
by its enclosing callable UUID. Mapping ranges belong to their query arena and
source graph; they must not be mixed across snapshots/revisions.

## Target selection is a separate input

The binder does not select callees from names and does not assume a callable
Calls edge proves which of several source invocations it describes. The caller
must supply the selected invocation/target pair and receiver convention, including
whether a Python method is bound or a Go method expression passes its receiver
as the first explicit actual. This also permits binding a newly discovered
possible target before its derived Calls edge has been added.

Binding does not prove execution, establish target selection, validate the whole
program or certify an upgrade. Existing statement slicing is not silently
replaced by this API. Typed [formal entry definitions](formal-entry-definitions.md) now connect formal
IDs to local statement-level PDG nodes. The [local value-flow query](local-value-flow.md)
connects supported expression reads/writes using native scoped keys and reaching
definitions. Matched-context propagation, target discovery and taint consumption
remain subsequent integration work.

## Transfer semantics

- Ordinary actuals bind explicit formals by position. Python named actuals bind
  supported keyword formals by their declared labels. Labels are not callable
  identities; no global name matching takes place.
- Java/JavaScript implicit receivers and Go/Rust declared receivers are separate
  from explicit positional indices. Bound Python descriptors supply the first
  declared formal; a rest-only Python signature receives it as element zero.
- An allocated constructor result supplies its `this`/instance receiver. A void
  constructor return does not overwrite the allocated result.
- Defaults distinguish Python definition-time evaluation from invocation-time
  JavaScript/TypeScript evaluation. Unknown ECMA actuals retain the possibility
  of being undefined and therefore selecting the default; known literals and
  explicit construction do not introduce that alternative.
- Rest and keyword-rest transfers describe container membership, not whole-value
  aliasing. Positional rest elements retain indices and named rest elements
  retain the caller's interned key. Unknown spreads retain compatible candidates
  with explicit uncertainty rather than selecting one arbitrary formal.
- Each callee return value supplies the selected invocation's result. Go multiple
  returns retain slot positions. Python/ECMA bare and normal fallthrough returns
  produce language-defined implicit values. Typed fallthrough conventions without
  sufficient return information remain diagnosed rather than assumed.

Return edges remain associated with the `CallBinding` containing them. A future
interprocedural solver must match the invocation context on entry and return;
flattening every caller's transfers into one context-free value graph can mix
infeasible call/return paths.

## Coverage and bounded work

Missing bodies/operands, invalid references, unsupported forms, language boundaries,
arity errors, class-object receiver resolution and unknown spreads have typed
diagnostics. The already captured supported transfers are retained. A partial
mapping or an empty transfer range must not be interpreted as a successful
negative. Python classmethod receiver values currently require additional class
resolution; their explicit business arguments still bind normally.

Transfers and diagnostics are pooled per query and exposed through read-only slices,
so callers cannot invalidate cached mapping ranges. Repeated identical requests reuse
their mapping, diagnostics are deduplicated within a binding, and keyword spread
ordering follows formal declaration order. The default per-invocation transfer
budget is 4,096, configurable through `BindingLimits`. Exhaustion preserves the
prefix and adds `LimitReached`; it is not a complete mapping. A larger solver
must separately bound its worklist, contexts and total query memory.

The arena borrows a fixed procedural record set and configuration. Start a new
arena after changing source availability or graph/analysis revisions. No source
parsing, compiler process, persisted schema change or whole-graph clone is needed
to query existing operand records.

Native tests cover six-language positional/receiver transfers, named/default/rest
conventions, unknown spreads, repeated-call result isolation, constructors,
multiple returns, missing/invalid inputs, limits, memoization and archive-backed
equivalence. They test the binding contract separately from target resolution and
the end-to-end dependency-impact qualification suite.

## Possible-origin integration

The [bounded possible-origin query](value-origins.md) now composes these native
local and invocation facts and activates supplied canonical member candidates.
It merges caller alternatives, reports coverage gaps, and preserves transfer
provenance. Matched-context exclusions and interprocedural taint verdicts remain
separate work; the query cannot certify absence of impact.

Shell positional bindings retain numeric slots even when only later parameters are used. Unsupported declaration patterns reserve their source argument position and keep partial-coverage diagnostics.
