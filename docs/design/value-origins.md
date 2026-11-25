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

Persisted aggregate operands require CFG semantics 11 and RBCP v14. Contexts and
keys merge; no precise index exclusion, complete container API semantics,
iteration, comprehension, or mutation-order proof is claimed.


### Tier 1 storage coverage

Empty JavaScript/TypeScript object literals have opaque object identity; ordinary
named literal properties register stores without turning their keys into values.
The canonical holder-field-to-array-to-receiver example therefore composes without
a resolver having to invent a nominal type for the holder.

Canonical `correctness/operand_flow.*` fixtures exercise storage with the native
CFG and query APIs, plus CLI inspection of the persisted operand pools:

- Named field writes/reads: C, C++, C#, Go, Groovy, Java, JavaScript, TypeScript,
  Kotlin, PHP, Python, Ruby instance variables, Rust, Swift, Scala, Dart, and Lua.
- Indexed writes/reads: C, C++, C#, Go, Groovy, Java, JavaScript, TypeScript,
  Kotlin, PHP, Python, Ruby, Rust, Swift, Dart, and Lua. Indices deliberately merge:
  storing at one index may reach a different index, without aliasing the container.
- Native literals and indexed reads: Elixir maps and Bash arrays. Puppet array and
  Scala tuple construction retain opaque aggregate identity, without treating the
  aggregate as an element's nominal type. The bundled Puppet grammar does not
  parse variable subscripts; Scala `apply`/`update` use invocation syntax and need
  a separate canonical collection-API model. Neither is claimed as native indexing.
- Named variadic containers and actual-to-element transfers: C#, Java, Groovy,
  Go, JavaScript, TypeScript, Kotlin, PHP, Python (including annotations), Ruby,
  and Swift. All actual origins remain possible elements; none becomes the
  nominal type of the container. Other rest/spread conventions retain existing
  partial binding/operand evidence rather than certifying missing flow.

SQL/HCL are declarative and ERB/Markdown do not provide procedural callable bodies
for this query. This capability matrix does not qualify arbitrary getters,
setters, overloaded indexing, reflective storage, library collections, or exact
mutation order. Ruby's fixture uses native `@slot` storage; a `box.slot` method
call is not assumed to be a field read.

## Callable values and bound receivers

`build_with_callables` accepts `CallableCandidate` inputs at exact native values,
with a canonical target, receiver supply and optional exact receiver/type premise.
The query creates distinct callable-value origins, propagates them through local,
argument, return and modeled memory transfers, and activates invocations whose
callee expression receives them. `callable_values` and `callable_targets` retain
the possible function and native invocation evidence. Newly activated function
alternatives replay already reached consumers. Budgets bound inputs, callable
bindings, evidence and work; the reverse value index is bounded by origin facts.

`ReceiverSupply::BoundValue` retains a function-qualified receiver expression
from another caller. The invocation binder validates it against native records
and binds the target receiver formal; it does not reinterpret the receiver as an
ordinary explicit argument. Python descriptor conventions consume the receiver
formal as with ordinary member calls. First-argument receiver conventions skip
that actual even when the selected function declares no receiver formal.

Native ECMA `bind`, `call` and `apply` models preserve possible target reachability.
Bound receiver alternatives merge, including the original receiver candidate.
The current model does not qualify partial bound argument lists or apply-array
expansion. `UnsupportedCallableArguments` explicitly marks those cases; existing
unresolved/binding evidence also remains relevant. Source
spelling of these library methods is a conservative modeling premise, not proof
of an unmodified builtin. Member reference binding is conservative across
languages; retaining an ECMA receiver may over-warn on an otherwise unbound use.

Resolvers own lexical/name and canonical-target coverage. Possible same-file
name candidates are not a scope-completeness proof. Fields, keys and callers
still merge as documented. Captures, decorators, arbitrary descriptors,
reflection, scheduling and interprocedural taint need separate models. This
query-local extension itself does not add persisted call-target edges. Native
function-pointer parameter, anonymous invocation and dynamic command lowering
repairs require CFG semantics 10 and RBCP v13 in this PR.


### Tier 1 callable coverage

Canonical `operand_callback` fixtures activate resolver-supplied function values
across all 20 procedural Tier 1 languages and check argument/return propagation
(shell commands retain their stream/exit-status boundary). Sixteen languages with
native method receiver conventions additionally keep a bound receiver distinct
from the ordinary argument. Seven field-call fixtures compose callable storage
with direct invocation; no intermediate scalar alias is required.

Native invocation forms include C/C++ function-pointer declarators, Elixir
anonymous invocation dots and expanded Bash command names. Ruby `call`, Java
`Function.apply`, and Puppet's leading-callable `call` convention are modeled
explicitly as conservative source-spelling premises. Arbitrary receiver members
such as Java `toString` or Ruby `object_id` do not invoke the stored callable.
Custom Java SAM names and other callable-object protocols need canonical
resolution/models beyond this named convention. Puppet's leading callable uses
the existing first-argument convention and retains binding uncertainty.

Callable/type prerequisites must actually reach their source value. Supplying an
unsatisfied prerequisite cannot activate a call. Direct callable loads from a
member are now memory reads even when that member is the invocation selector.
The existing merged contexts, keys and bounded-work diagnostics still apply.

## Explicit deferred values

Native `AwaitedValue` operands preserve possible resolved origins on a successful
await. Canonical argument/return bindings compose asynchronous factory returns
with the await reader; no source-text or global method-name joins are introduced.
This is a resolved-value abstraction: it does not establish precise coroutine or
promise object identity before awaiting, nor prove execution or scheduling.

Native generators carry a `generator` role and explicit `yields` values. Binding
a generator introduces an opaque invocation-result identity. Yielded elements and
terminal returns have separate query-local slots. Ordinary return transfers do
not type the generator itself as either value. `DeferredYield` provenance records
which slot supplied an exact read, including whether it was a terminal result.
Iterator-result and enumerator carriers also keep opaque identities. Existing
weak updates, merged contexts and bounded memory/work budgets still apply.

### Tier 1 coverage

Canonical `correctness/operand_flow.*` fixtures and
`tests/fixtures/native_deferred_cases.json` enumerate all 20 procedural languages:

- Native await/co-await: C++, C#, JavaScript, TypeScript, Python, Rust, Swift,
  and Dart. Dart's grouping node preserves its inner await. Kotlin suspend calls
  already use ordinary canonical argument/return bindings and have an equivalent
  compatibility fixture rather than an invented await keyword.
- Generator producer facts: C++, C#, JavaScript, TypeScript, Python, PHP, and
  Dart, including native generator declarations in the ECMA CFG inventory.
- Readers: Python `next`/`anext` and iterator methods; PHP `current`/`getReturn`;
  ECMA `next().value` including aliases of the iterator result; C# enumerator
  `Current`; Dart iterable `iterator.current`. ECMA iterator values conservatively
  include terminal returns when `done` may be true. Python/PHP current-element
  readers do not receive terminal returns. C++ custom promise/iterator reader
  protocols require their own models; this slice exposes co-yield facts and an
  opaque factory result without assuming a library ABI.
- Python `asyncio.run` exposes possible resolved origins. Go native channel
  construction, sends, receives, cross-function handoffs and comma-ok result
  position zero use element storage. The Boolean status never receives the
  element origin. Distinct channel allocation sites retain separate slots.

The manifest explicitly marks languages without native await/generator syntax or
without a modeled reader. Java/Groovy/Scala future libraries, Ruby block/Fiber
protocols, Lua coroutines, Elixir mailboxes, and shell stream/job behavior are not
inferred from unrelated method names. SQL/HCL and ERB/Markdown retain the existing
declarative/non-procedural boundary.

Library conventions are language/source-spelling premises, not proof of an
unshadowed or unmodified builtin; unresolved call/binding evidence remains visible.
Delegating yields retain a generator role and an explicit `ReturnConvention`
diagnostic, preventing terminal values from leaking into the opaque generator.
Resume arguments, arbitrary awaitable transformations, event loops, stream APIs,
channel iteration/close semantics, delivery order, termination and race freedom
need further models. Empty or partial results must not establish no impact.

New persisted roles and operands use CFG semantics 11 and RBCP v14. Capture
reuses the current native tree and worker pipeline; query extensions remain lazy
and bounded rather than constructing a whole-repository heap.

## Computed lookup and constructor values

`callable_member_values` streams exact native named/indexed member facts and the
conservative Python `getattr` convention. Each fact retains its value, receiver,
named member (if present), and exact computed selector expression (if present).
The enumeration uses persisted operands rather than source parsing or another AST
walk. Channel receives are not index selectors, Puppet resource references with a
static resource type are not collection reads, and arbitrary receiver methods
named `getattr` are not Python builtin lookup.

A resolver supplies compatible canonical `CallableCandidate` alternatives. The
existing receiver-origin requirement gates those candidates; unknown keys retain
all compatible methods. `UnknownMemberSelector` remains explicit for computed
candidates, including literal selectors: this layer does not perform constant
folding or claim a closed selector set. Scalar and aggregate callbacks use the
same bounded callable engine, without a global method-name join.

`build_with_values` accepts `ConstructorCandidate` inputs at exact native
class/type references. Opaque constructor origins flow through local, argument,
return, field and aggregate transfers. An invocation of a reached constructor
introduces its possible canonical instance type at the result. When a canonical
explicit body is supplied, its invocation binds a constructed receiver and
ordinary arguments separately. `ConstructorValue` and `ConstructionEvidence`
remain separate from function values and instance origins. A missing body does
not fabricate an implementation, and unresolved invocation evidence remains.

Go/Rust composite literals and Elixir structs construct from their native type
operands without inventing constructor calls. Ruby class-object `new` and Swift
metatype `init` conventions use their native receivers; unrelated class-object
methods do not construct instances. ECMA `Object.create(Type.prototype)` creates
a possible instance from a reached constructor reference without invoking its
constructor body. Source spelling and resolver-provided canonical identities
remain conservative premises, not proof against shadowing or monkey patching.

Ordinary generative constructor returns do not alias the instance. ECMA is an
exception: an explicit returned object may replace the instance. Its canonical
binder retains possible returned origins as well as the possible new instance,
with `ConservativeConstructorReturn` because the runtime object test is not
proven. Factory-function calls use the ordinary callable/return model. Custom
factory constructors, proxies, descriptors, reflective construction and all
initializer side effects are not established by this model.

### Tier 1 qualification

`tests/fixtures/native_lookup_cases.json` enumerates all 20 procedural languages
and points to canonical `correctness/operand_flow.*` sources. Public native API
tests exercise these same sources, and correctness JSON checks persisted facts.

- Native indexed/computed callable values are exercised in C, C++, C#, Java,
  Groovy, Go, JavaScript, TypeScript, Kotlin, PHP, Puppet, Ruby, Rust, Swift, Dart,
  Elixir, Lua and Bash. Python exercises `getattr`; Scala exercises named members.
  Scala indexing is an ordinary `apply`-call protocol in the native grammar, so
  this layer does not reinterpret arbitrary calls as collection/member reads.
- Native construction is exercised in C++, C#, Java, Groovy, Go, JavaScript,
  TypeScript, Kotlin, PHP, Python, Ruby, Rust, Swift, Scala, Dart and Elixir.
  C/Puppet/Lua/Bash have no corresponding native nominal constructor in this
  model; their supported functions/resources/tables/commands retain their
  existing contracts. SQL/HCL and ERB/Markdown keep the non-procedural boundary.
- Explicit constructor-body receiver/argument separation is exercised in C++,
  C#, Java, Groovy, JavaScript, TypeScript, PHP, Python, Ruby, Swift and Dart.
  Kotlin/Scala primary initialization and the Go/Rust/Elixir structural forms do
  not introduce a separate explicit constructor body in these fixtures.
- Aliases, constructor registries and ECMA prototypes retain canonical evidence.
  Negative controls cover incompatible receiver origins, unrelated class methods,
  invalid references, absent constructor candidates and exhausted work budgets.

Go's native grammar can parse `callbacks[key](value)` as a generic type conversion.
The possible indexed invocation and scoped value-head read are retained, with an
explicit `InvocationShape` diagnostic. Genuine generic type conversions are not
proven function calls, and unrelated type annotations do not become local reads.
C/C++ pointer-return declarators retain the nearest native function parameters;
Swift initializer lookup matches the plugin's canonical `init` name; ECMA
constructor CFG lookup/indexing matches the plugin's class name while retaining
the source `constructor` alias; Dart
constructor signatures retain their parameters.

State stays lazy and query-local, with bounded work, constructors, evidence and
transfers. Contexts and memory keys still merge. Empty or partial results cannot
establish no impact. Native receiver/lowering changes use CFG semantics 12 and
RBCP v15; existing per-file native parsing and streamed persistence are reused.
