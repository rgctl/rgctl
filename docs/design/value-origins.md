# Bounded possible-value origins

`value_origins::OriginAnalysis::build` composes native local value transfers and
selected-invocation bindings, propagates possible origins, and activates canonical
member candidates when a matching origin reaches their exact receiver value.
This is a query-local monotone worklist, not a source parser or name resolver.

```rust
let result = OriginAnalysis::build(
    &icfg, &seeds, &initial_targets, &member_candidates, OriginLimits::default(),
);
```

## Resolver inputs and output

- `OriginSeed` identifies a producer `ValueRef` and a canonical possible type UUID.
  A seed can represent an allocation or another explicitly modeled provider.
  The producer identity is retained, allowing later instance-sensitive models.
- `InvocationTarget` supplies an exact source invocation and canonical selected
  callee with its receiver convention. Existing macro Calls alone do not identify
  which of several source invocations they represent; the provider must associate
  them from native facts rather than global method-name guessing.
- `MemberCandidate` supplies a canonical invocation/target and the receiver type
  accepted by graph/type resolution. It is inactive until that type reaches the
  invocation's native receiver expression.

The resolver is responsible for canonical type/target correctness and coverage.
The solver validates expression and invocation references but cannot prove that
an arbitrary provider's seed or type-compatibility premise is correct. An empty
candidate list does not establish that no runtime target exists.

Results contain query-local seed IDs, sorted expression origin sets, receiver
supported possible targets, typed local/invocation transfer provenance and
coverage diagnostics. JSON uses record arrays rather than structured map keys.
Callers can hydrate derived targets as `EdgeType::Calls` in a query overlay and
retain the origin/transfer explanation; persisted graph records are unchanged.

## Propagation

Each visited function loads its native CFG and builds a local PDG once for this
query without source parsing or I/O. The existing local query supplies scoped
read/write transfers. Actual/formal, receiver and default bindings preserve
values, and scalar returns feed the selected invocation result.

A newly activated member target adds its invocation bindings. Origins already
present at the source of a newly added transfer are replayed immediately; otherwise
factory results that arrived before the target would be lost. Facts are deduped
by `(value, origin)` and targets by canonical invocation/target/receiver convention.
Recursion terminates at a fixed point or an explicit query budget.

Rest/keyword membership and unknown spreads are diagnosed rather than equating
an entire container with an element. Native Go result projections retain their
return-slot positions, as described below.
Implicit values do not acquire fabricated object origins. Unsupported local,
binding and missing-body evidence is retained while known paths continue.

## Context and verdict limits

The explicit context policy is **MergedPossibleOrigins**. Different callers can
merge origins at shared formals and subsequently propagate them to each caller's
result. This preserves alternatives but can over-warn. Tests deliberately retain
that behavior; no context-specific exclusion is claimed.

Conditional factory returns retain both possible outputs. Known-defined actuals
still obey the native binder's default-selection rules. A single modeled call can
distinguish an unused X allocation from the W value actually passed into its
receiver, without claiming this establishes arbitrary-program absence of impact.

Fields, containers, captures, deferred execution, dynamic lookup, call effects,
implicit provider/lifecycle calls and missing source require additional models.
This API does not implement interprocedural taint verdicts or certify upgrade
safety. Consumers must inspect relevant uncertainty and retain unsupported paths
as unknown or conservative warnings.

## Scope and bounds

The query visits records requested by the supplied seeds/targets and newly activated
members. It does not implicitly scan or clone the whole repository graph. CFGs
are borrowed from owned or archive-backed access; no new persisted schema or
procedural revision is needed beyond the existing local operand records.

`OriginLimits` bounds functions, targets, member inputs, origin/evidence pools,
transfers and processing steps. Function-local limits are checked before PDG
construction and delegated local flow work. Exhaustion produces `LimitReached`
or a local limit diagnostic, preserving a partial prefix. These count limits are
not wall-time or allocator RSS guarantees; local PDG/reaching-definition costs
still require profiling before scale claims.

The solver serializes explicit missing-function, local/binding coverage,
unsupported-transfer, unresolved-call/receiver and limit diagnostics. Empty paths
with missing inputs or unsupported handoffs must never be treated as successful
negative results.

## Go result projections

A multi-value call result retains separate native assignment projections. The
solver indexes those projections once per loaded callable and routes each
`ReturnToResult { position }` transfer to its matching projection. It does not
alias the tuple with its elements or mix different return positions. Missing
projection consumers remain diagnosed. Type-switch aliases reuse the incoming
origin within the switch scope. Instance-field and general aggregate storage
remain separate extensions.

## Named instance-member state

The native operand lowerer now records explicit `FieldWriteFact` values without
confusing the stored RHS with the receiver object. Reads reuse native `Member`
operands. Supported writes include explicit assignments, TypeScript constructor
parameter properties, and named keyed Go struct construction. Constructor
invocation binding supplies the constructed result to the constructor receiver.

The origin solver joins stores and loads by query-local object origin plus
member name, then emits `OriginTransferCause::Field` with the object origin and
exact read. It registers newly loaded functions and replays existing receiver
origins, and registers new store/load edges regardless of processing order.
This is flow-insensitive weak-update state: all stores remain possible, including
stores from merged invocation contexts. It can over-warn and does not prove
object uniqueness or temporal validity. A stored X does not turn the container
itself into X, and differently named fields do not automatically alias.

Member-access and slot-entry counts share `max_transfers`, and all generated
store/load pair processing uses `max_steps`. Exhaustion returns the existing
limit diagnostic and partial prefix. No whole-repository heap is created.

Implicit/unqualified Java fields, generated dataclass constructors, module
initialization, descriptors, computed properties, collections and reflective
state still require their respective facts/models. These tests do not establish
complete heap, alias or lifecycle semantics. Stored field operands require CFG
semantics 8 and RBCP v11; previous records are rejected by revision guards.

## Aggregate elements and variadic packing

`OriginSeed.type_id` is optional: opaque aggregate identities have no nominal
type, while instance origins retain their canonical type UUID. An untyped
aggregate never selects a member candidate. Literal sequence/map values and
native variadic formal containers introduce bounded query-local opaque origins.
Their values flow through the same local and invocation transfers as other
origins, without equating the container with its stored values.

Native `Collection`, `Element` and `ElementWriteFact` operands supply explicit
literal members, indexed reads and stores. All keys/indices currently merge in
one element slot per possible container origin. This deliberately retains
alternatives and can over-warn across different keys or array positions. Map
keys are not treated as element values. Variadic `RestElement` transfers populate
the formal's element slot; they do not propagate an actual X into the entire
rest container. Unknown spreads remain diagnosed. Go multi-return projections continue to route
return values by their declared result position, separately from container slots.

`OriginTransferCause::Element` records the possible container and exact read.
Element and named-field slots are distinct. Both share the existing bounded
memory-access, slot and work budgets. Indexed assignment reads the container and
selector rather than defining a new container variable; native def/use lowering
now preserves that distinction for supported indexed forms.

Persisted aggregate operands require CFG semantics 9 and RBCP v12. Contexts and
keys merge; no precise index exclusion, complete container API semantics,
iteration, comprehension, or mutation-order proof is claimed.
