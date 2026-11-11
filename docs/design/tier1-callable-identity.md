# Tier 1 callable identity and overrides

## Problem

Using only a callable's qualified name as its graph identity collapses overloads.
Calls, code hashes, inherited overrides, and cached CFGs can then refer to another
body. Source ranges alone cannot repair the graph once two declarations share a
node ID.

Dependency discovery is separate work. This change operates on project source;
it does not add package readers, class-file readers, or dependency traversal.

## Implementation

The existing stable node key accepts an optional callable discriminator. Tier 1
extractors, including Java, attach language-specific identity metadata at their
existing CST construction sites through
`rgctl_plugin_helpers::callable_identity`. Java uses the fallible `try_symbol`
entry point to preserve parse errors; the Java policy owns constructor, array,
varargs, receiver-parameter, and enclosing-lambda identity rules. Shared
`finish` and call-site helpers attach relation identities. The Java plugin
supplies its existing local-variable type evidence and retains import/receiver
resolution. Extraction remains on the existing
parallel workers, without a second parse or whole-graph hydration.

Typed overload languages retain parameter types and relevant dispatch syntax:
C++ receiver qualifiers and declarators, C# parameter modifiers, Swift labels,
Kotlin extension receivers, Scala parameter groups, and generic arity. Parameter
names and bodies do not form identity. SQL retains schema ownership and argument
types. Rust distinguishes implementations belonging to different traits.

TypeScript overload declarations retain individual signatures while the runtime
implementation has its own identity. Declaration identity and override dispatch
identity are separate, allowing inherited signatures to describe one runtime
method without merging graph nodes. Dynamic languages use runtime name binding;
repeated definitions retain separate bodies and mark superseded definitions.
Elixir includes arity, clause patterns, and guards. Languages without callable
overloads retain their existing name/owner semantics. Markdown, HCL, and ERB are
explicitly non-applicable overload cases.

Call relations preserve the caller's discriminator. Literal or known formal
argument types can establish a target; unknown expressions do not establish an
overload. Argument evidence is separate from canonical identity, so an argument
tuple cannot silently choose between const/non-const or generic variants. Exact
evidence survives parked relations and cross-repository merge, with no fallback
to another overload when that evidence fails.

MRO uses dispatch identity and implementation ownership, excludes superseded
definitions, and prefers runtime implementations over paired declarations.
CFG selection uses exact source bytes; ambiguous name-only lookups fail. Archive
reuse verifies source hashes and does not remap graph IDs through ambiguous names.

## Migration and evidence

Facts-cache format 3 and extraction semantics version 4 invalidate old extraction
results. Missing or stale `.rgctl/extraction_semantics.json` forces full source
extraction, including when `discover --files` was requested. The semantics marker
is written after successful snapshot persistence. Existing snapshots remain
readable for queries.

`.rgctl/callable_resolution.json` counts unresolved canonical calls and retains at
most 1,024 original relation samples with reasons. This is evidence of incomplete
source resolution, not a claim of compiler-level type checking. Type aliases,
implicit conversions, dynamic dispatch, and arbitrary expressions can require
more evidence than the source extractor provides.

## Validation

`tests/fixtures/tier1_callable_identity/inventory.json` is checked against the
registered Tier 1 coverage inventory. Every language has a source example and an
explicit applicability contract. `tests/tier1_callable_identity.rs` verifies body
isolation, stable IDs across layout and parameter renames, snapshot round trips,
overload calls, inherited dispatch, Rust trait ownership, generic arity,
declarator details, and dynamic rebinding. It also exercises actual CLI rebuilds
from simulated collapsed snapshots with missing and obsolete semantics markers.

Merge and CFG unit tests cover refusal to select another overload. The Tier 1
parity gate verifies extraction and AST coverage manifests. Required validation:

```sh
cargo +1.99.0 test --test tier1_callable_identity
cargo +1.99.0 test --workspace --lib
cargo +1.99.0 test -p rgctl-analysis --test tier1_parity_gate
./scripts/preflight-pr.sh --quick
```

No cold-ingest performance claim is made by these correctness tests. Before a
performance comparison, use release builds and fresh artifacts with the
language-filtered corpora and commands in `AGENTS.md` (for example Roslyn with
`discover src -l csharp`, LLVM clang with `-l cpp`, and VS Code src with
`-l typescript`). Record wall time, node/function counts, and `index_graph_build`.
