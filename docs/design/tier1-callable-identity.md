# Tier 1 callable identity and overrides

## 1. System Overview & End-to-End Data Flow

Using only a callable's qualified name as its graph identity collapses overloads.
Calls, code hashes, inherited overrides, and cached CFGs can then refer to another
body. Source ranges cannot repair the graph once declarations share a node ID.

An optional canonical callable discriminator distinguishes declarations while
preserving readable names. Field materialization separately records supported
members and their source spans. Dependency discovery, package readers, class-file
readers, and dependency traversal are outside this source-graph design.

```text
Source files
  -> existing parallel CST extraction workers
  -> shared callable identity and field-span helpers
  -> Symbol facts and relation identity/argument evidence
  -> GraphBuilder: typed insertion, call resolution, inherited overrides
  -> columnar snapshot: stable identity, spans, properties
  -> queries / snapshot diff / merge / optional CFG analysis
```

Extraction uses the existing workers and CST construction sites. It does not
introduce a second identity parse or whole-graph hydration pass. Fields can be
materialized independently of CFG through `discover --with-fields`.

## 2. Theoretical Foundations Mapped to `rgctl`

Callable identity follows declaration equivalence: parameter names, bodies,
whitespace, annotations, and source coordinates do not distinguish overloads.
The language policy retains syntax that affects declaration or dispatch identity.

- Typed overload languages retain canonical parameter types and relevant syntax:
  C++ receiver qualifiers and declarators, C# parameter modifiers, Swift labels,
  Kotlin extension receivers, Scala parameter groups, and generic arity.
- SQL retains schema ownership and argument types. Rust distinguishes
  implementations belonging to different traits.
- TypeScript overload declarations retain individual signatures, with a separate
  runtime implementation identity. Declaration and override-dispatch identities
  can therefore differ without merging graph nodes.
- Dynamic languages follow runtime name binding. Repeated definitions retain
  separate bodies and mark superseded definitions. Elixir also includes arity,
  clause patterns, and guards.
- Languages without callable overloads retain their existing name/owner semantics.
  Markdown, HCL, and ERB are explicitly non-applicable overload cases.

`rgctl_plugin_helpers::callable_identity` implements the shared extraction
contract, including Java. Java uses the fallible `try_symbol` entry point to
preserve parse errors; its policy owns constructor, array, varargs,
receiver-parameter, and enclosing-lambda rules. The plugin supplies existing local
variable evidence and import/receiver resolution to the shared call helpers.

Canonical identity and argument evidence serve different purposes. A literal or
known formal argument type may establish a call target; an unknown expression
cannot prove an overload. Argument tuples cannot silently choose between
const/non-const or generic variants. Explicit identity evidence survives parked
relations and merge, without falling back to a different overload when it fails.

Method resolution order (MRO) uses dispatch identity and implementation ownership,
excludes superseded definitions, and prefers runtime implementations over paired
declarations. A field and a same-named method differ by node kind; callable
discriminators distinguish callable declarations independently of field positions.

## 3. Binary Layout, Data Structures & Memory Discipline

Stable node keys and deterministic UUID construction include the optional
callable discriminator alongside the existing path, name, node kind, and
qualified name. The columnar extension keeps a borrowed identity prefix so mmap
identity lookup does not decode parameters, properties, or labels.

| Representation | Role |
| --- | --- |
| `Symbol` identity metadata | Canonical declaration identity and separate dispatch/argument evidence during extraction and graph construction. |
| `NodeExtension` identity prefix | Canonical identity for borrowed mmap lookup and stable-key generation. |
| `callable_identity` node property | Canonical identity for the in-memory node accessor and UUID generation. |
| `override_identity` node property | Stored when different from canonical identity, including explicit empty values that suppress fallback. |
| Typed `FieldSourceSpan` | Declaration and identifier positions carried by `Symbol.fields` into the shared materializer. |
| Fixed 14-byte Green span tail | Declaration length (`u32`), overflow identifier offset/length (`u32`, `u32`), and identifier token length (`u16`), relative to the row's start byte. |

The extraction-only `identity_discriminator` metadata key is not copied into node
properties. Equal non-empty dispatch identities use the canonical-property
fallback. Distinct and explicitly empty dispatch identities retain their meaning.
This storage cleanup changes neither UUID construction nor the snapshot layout.

Helper-marked callable body hashing and repeated-definition ordering read scalar
source-span fields. Fresh facts do not duplicate those offsets in JSON metadata;
unmarked callable extraction retains its existing body-selection fallback.
Absolute extraction offsets stay out of node properties, so blank-line shifts do
not change function extension digests. CFG selection prefers dedicated byte spans
and accepts legacy offset properties when reading older nodes. The extension
retains its discriminator prefix and relative span tail; older format-2 span
extensions remain readable through the compatibility decoder.

Facts-cache format 8 invalidates older extracted payloads. Extraction semantics
version 12 also invalidates graphs requiring the current materialization rules.
A missing or stale `.rgctl/extraction_semantics.json` forces source-graph rebuilding,
including when `discover --files` was requested. The marker is written after
successful snapshot persistence. Existing supported snapshots remain readable.

Older redundant identity properties remain readable. Comparing old and new
representations can report property-only node changes, with no added/removed
nodes or edges when source and identities are unchanged. This targeted migration
contract does not provide cross-version semantic normalization.

The lightweight analysis index keys entries by versioned exact node UUID plus
body hash. The UUID already carries callable ownership, discriminator and node
kind, so identical method bodies in different owners remain separate cache
entries. Lookup, batch shortcuts, persistence, index reconstruction and stale-key
cleanup use the same key. Partial rebuilds stream retained pack records as opaque
bytes so cache-hit payloads survive without CFG/PDG deserialization. Actual UUID
changes rebuild analysis; file/name/body
matches cannot establish equivalent CFG/PDG ownership.

The sidecar keeps its existing bincode entry layout. Loading legacy keys upgrades
them using their recorded UUIDs and preserves flow counts. Entries previously lost
to a name-key collision rebuild once on a miss. CFG/PDG archive aliases retain
their legacy name-based keys and existing uniqueness checks; snapshot and archive
formats do not change. This index repair does not broaden cache compatibility
across analysis options or algorithm versions.

## 4. CLI Contract, JSON Schema & Implementation Map

```sh
rgctl -f json discover . --with-fields
rgctl -f json discover . --with-cfg
```

`--with-fields` materializes supported field nodes and source positions without
CFG, dominators, PDG, or taint. CFG and `--full` also request fields. Default cold
discovery leaves optional field materialization off; native property/column
symbols can already exist without this option.

Field capability is recorded against the snapshot digest. Requesting fields
upgrades a warm field-less graph across all files, even with `--files`.
Incremental updates preserve an existing field-enabled mode. Structured queries
expose existing declaration/name byte offsets. This uses shared byte-span storage
in place of the earlier UTF-16 range-property approach; no UTF-16 projection API
is introduced.

`.rgctl/callable_resolution.json` records unresolved-call counts and at most 1,024
original relation samples with reasons. It is bounded resolution evidence, not
proof of compiler-level type checking. Materialization capability is tracked by
`.rgctl/materialized_fields.digest` and does not imply CFG archive coverage.

| Component | Implementation |
| --- | --- |
| Shared declaration, finishing, and call-site helpers | [callable_identity.rs](../../crates/rgctl-plugin-helpers/src/callable_identity.rs) and its Java policy module |
| Shared field and identifier spans | [spans.rs](../../crates/rgctl-plugin-helpers/src/spans.rs) |
| Worker preparation and declaration-body hashing | [extractor.rs](../../crates/rgctl-extraction/src/extractor.rs) |
| Typed insertion, identity storage, call resolution, and inherited overrides | [graph_builder.rs](../../crates/rgctl-extraction/src/graph_builder.rs) |
| Stable keys and columnar identity/span encoding | [stable_key.rs](../../crates/rgctl-graph/src/stable_key.rs), [columnar_snapshot.rs](../../crates/rgctl-graph/src/columnar_snapshot.rs) |
| Incremental field preservation and identity-aware relation repair | [updater.rs](../../crates/rgctl-incremental/src/updater.rs) |
| Discover field/CFG controls and capability reuse | [discover.rs](../../src/cli/discover.rs), [discover_impl.rs](../../src/cli/discover_impl.rs) |
| MRO and CFG declaration selection/archive handling | [mro.rs](../../crates/rgctl-analysis/src/mro.rs), [cfg_builder.rs](../../crates/rgctl-analysis/src/cfg_builder.rs), [cfg_pdg_archive.rs](../../crates/rgctl-analysis/src/cfg_pdg_archive.rs) |

Callable helpers receive declaration CST nodes, including individual Go interface
methods. An unrelated member edit therefore cannot contaminate another callable's
body hash or discriminator. Shared `finish` and call-site helpers attach the
caller's identity and distinguish inferred lexical hints from explicit receivers.
Signature-aware resolution searches inheritance before an inferred free-function
fallback, excludes unrelated type members, and rejects incompatible or ambiguous
families. Java inherited overloads remain available when a child declares another
signature. Incremental repair enforces identity/argument evidence across files and
preserves class-over-interface precedence.

Field positions cover field-producing Tier 1 plugins through the shared
materializer; native properties/columns use shared symbol spans. Lua named table
members and Elixir struct fields are emitted directly. Insertion indexes include
node kind, qualified name, and optional discriminator so fields cannot suppress
same-named methods. See the [field/callable collision audit](field-callable-collision.md).

## 5. Performance Guardrails & Verification Checklist

Use the configured Cargo toolchain meeting the workspace minimum of Rust 1.99.
Run commands from the repository root:

```sh
cargo test --test tier1_callable_identity
cargo test --test tier1_field_review_regressions
cargo test --test tier1_identity_storage
cargo test --workspace --lib
cargo test -p rgctl-analysis --test tier1_parity_gate
./scripts/preflight-pr.sh --quick
```

The callable fixture inventory is checked against all registered Tier 1 languages.
It covers body isolation, stable IDs after layout edits and parameter renames,
snapshot round trips, overload calls, inherited dispatch, Rust trait ownership,
generic arity, declarator details, and dynamic rebinding. CLI regressions rebuild
collapsed snapshots with missing/stale markers and preserve cross-file calls
through incremental edits. Merge and CFG tests reject wrong-overload selection.

The field inventory checks all 24 registered Tier 1 languages: 20 source
member/property/column cases and four explicit non-applicable contracts (Bash,
HCL, Markdown, ERB). Cases cover exact identifiers, type/name collisions, sigils,
grouped declarations, decorators/annotations, initializer references, quoted
struct atoms, lexical table ownership, snapshot round trips, and line-shift
identity/diff invariance. Identity-storage migration checks preserve canonical
IDs and edges while identifying property-only representation changes.

Correctness tests do not establish ingest performance. Use immutable release
binaries, fresh artifacts, the language-filtered corpora, and commands in
[AGENTS.md](../../AGENTS.md) and the [profiling guide](../internal/profile.md).
Record wall time, peak memory, graph/function counts, and `index_graph_build`.
Compare repeated before/after observations without concurrent compilation, and
report cache state and measurement variability. Keep benchmark artifacts outside
the source changes.

## 6. Known Limitations & By-Design Boundaries

- Source resolution remains conservative. Type aliases, implicit conversions,
  dynamic dispatch, arbitrary expressions, and inferred external types can require
  evidence unavailable to the extractor. This design does not ingest dependencies.
- Dynamic computed member keys and external field types are not compiler-checked.
  Field nodes and source locations do not guarantee complete field-reference or
  read/write tracking. CFG alone does not establish that coverage either.
- Ambiguous name-only CFG selection fails. Warm name-only archive hits validate
  the unique source declaration with a CST lookup, without constructing another
  CFG. A single cached record cannot prove uniqueness when archive coverage is
  partial. Source parsing remains necessary for this name-only evidence; exact
  graph-ID/span cache paths retain their existing behavior. Legacy initializer
  ordinal requests retain their full-build compatibility path rather than
  assuming owner-qualified uniqueness. Archive reuse validates source hashes;
  graph IDs are not remapped through ambiguous callable names. Cache optimization
  must preserve those checks, including when archive coverage is partial.
- Position-only edits preserve callable identity. Changing a language identity
  contract can legitimately change IDs and requires cache/semantics invalidation.
- Backward readability is supported through explicit format readers. Cross-version
  comparisons can include extraction/metadata representation changes; there is no
  universal normalization layer that guarantees source-only diff results.
