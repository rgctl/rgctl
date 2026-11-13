# Native local value transfers

`LocalValueFlow::build(&cfg, &pdg, limits)` derives function-local value transfers
from native expression/CFG anchors and the existing reaching-definition engine.
It does not parse source or infer targets from names, lines or statement text.

```rust
use rgctl_analysis::local_value_flow::{LocalValueFlow, LocalFlowLimits};
let flow = LocalValueFlow::build(&cfg, &pdg, LocalFlowLimits::default());
for transfer in &flow.transfers {
    // Qualify transfer.from/to with this callable's canonical UUID.
}
```

## Captured facts

`FunctionOperands` persists three additional pools:

- `local_slots`: interned native scoped keys, indexed by `LocalSlotId`.
- `local_uses`: each exact name expression, scoped slot and `StatementRef`.
- `local_writes`: each scoped definition, exact statement and its formal entry or
  initializer/RHS expression. Unsupported writes have no fabricated RHS value.

`StatementRef` is a block ID and statement index. Native lowering records reads
before binding new declarations, then records writes after scoped identities
have been assigned. The transient source-span index is built once per callable
from its existing expression records. The same parsed tree captures initializer,
assignment and predicate expressions; there is no additional parser.

Formal entry writes reuse `FormalFact::value`. These compose directly with
invocation actual/formal and return/result transfers. A native regression follows
one caller parameter through an actual alias, a selected invocation, the callee's
formal/local alias and its return to that invocation's result. The other caller
parameter remains disconnected in that modeled example.

## Query semantics

For each read, the query selects the latest matching preceding local definition
in its block, or the reaching incoming definitions when no preceding definition
exists. Branches and loops retain all reaching alternatives. Parentheses preserve
an inner value. Other computations retain their existing syntax inputs; those
inputs are not silently treated as whole-value aliases. Fields and container
projections need separate models.

Multiple local writes within one statement retain compatible same-statement
alternatives and an `IntraStatementWrites` diagnostic until finer evaluation
order is represented. Complex destructuring and unmatched multi-value writes
remain explicitly unsupported; a container is not equated with one element.
One expression emitted at several CFG positions retains alternatives with an
`AmbiguousOccurrence` diagnostic.

This expression-level query can distinguish same-line reads before and after
reassignment even though the older statement PDG retains conservative historical
same-block edges. That behavior does not qualify global exclusion or upgrade
safety. Hoisting, nonlocal/global binding, captures, exceptions, heap effects and
other language-specific forms require additional qualification.

## Native correctness details

Go RHS expression-list elements are reads rather than definitions. Selector
reads include their receiver operand. Transparent `statement_list` grammar
wrappers do not introduce a second lexical scope. Go mixed `:=` declarations
reuse an existing current-scope binding while nested blocks can still shadow it.

Reaching-definition catalog construction looks up PDG nodes by deterministic CFG
position/content identity. It does not trust an auxiliary block index rebuilt
in unordered map iteration order after archive loading.

## Compatibility, limits and consumers

CFG semantics revision is **4**, archive format **RBCP v7**. Serialized local
references are checked against the expression pools and actual CFG statements.
Unreachable statement anchors are discarded with their pruned CFG blocks. Old
source-identical procedural artifacts must be rebuilt through the existing
revision guards.

A query validates that the supplied PDG matches the selected CFG. Missing local
origins, unsupported operands/writes, ambiguous positions, incompatible records
and exhausted budgets are diagnostics, not successful negatives. Default limits
bound statements, definitions, emitted transfers and candidate scans; they are
not an allocator RSS or wall-time guarantee. Larger propagation worklists and
call contexts need their own limits.

The API consumes an existing per-function CFG/PDG, including archive-backed
records, and does not clone the repository graph. No performance claim is made
without profiling.

Target selection remains an input to invocation binding. This API supplies the
local part of that value-flow chain; receiver-origin propagation, dispatch target
discovery, matched-call traversal and adapter/taint integration remain separate
consumers. The existing adapter qualification score cannot be updated from these
unit tests alone.

## Legacy handoff compatibility

`criterion_for_parameter` now selects the earliest recorded body read of the
formal's scoped slot, falling back to its entry definition when unused. Legacy
records select a use before a definition with a deterministic ordering. This
removes the previous arbitrary PDG-map choice between an entry and a body use.
The new local query considers all recorded reads; it is not limited to that
legacy helper's single representative criterion.

## Conditional results and successful assertions

Conditional operands retain the condition separately and only their selected
value branches as `inputs`. Both branches supply possible origins to the result;
no path-condition evaluation or branch exclusion is claimed. JavaScript,
TypeScript and Java ternaries and Python conditional expressions share this role.
Successful Java reference casts, Go type assertions and TypeScript `as`, angle
assertions and non-null assertions preserve the operand origin. Assertion failure
is not an alternate object value; exception/panic effects remain separate work.
Ordinary arithmetic and other computations are not aliases of their children.

These persisted expression roles use CFG semantics 5 and RBCP v8; prior procedural
records are invalidated by the existing archive and source-revision guards.
