# Procedural semantics revisions

CFG statement def/use facts feed local PDGs, slicing, field-write extraction and
taint. A source hash alone does not validate these facts after lowering changes.

`cfg::CFG_SEMANTICS_VERSION` now identifies the lowering semantics. Serialized
CFGs begin with an eight-byte `RGCF` tag and little-endian revision, validated
before their blocks are decoded. Derived adjacency and `local_types` remain
transient. Older unversioned CFGs and incompatible revisions must be rebuilt;
they are not silently accepted as current analysis.

The procedural archive is **RBCP v6**. It retains the v3 indexed footer layout
(`TOC3`, 32-byte entries) while its record payloads contain versioned CFGs and
[function operand pools](function-operands.md). CFG semantics revision is 3. Typed formal entry definitions seed local reaching
definitions and lexical scope; artifacts without them must be rebuilt.
Preencoded incremental writes preserve records only from the current archive
format; old records cannot be copied under a new header without rebuilding.
On-demand `ensure_cfg_pdg_for_source` rebuilds old-format records from source.

Incremental analysis keys retain the exact callable UUID/body-hash identity and
append a `cfgN` semantics component. The lightweight index filters incompatible keys
before fast reuse and stale-key purging. Procedural stable-alias UUIDs also
include the revision. These are procedural cache identities, not changes to
canonical callable identities in the repository graph.

After upgrading from unversioned records, run `rgctl discover --with-cfg` to
rebuild eager analysis. On-demand flow/slice queries rebuild requested records.
Reading incompatible persisted results directly can return a rebuild error.

## Predicate lowering repair

`wire_condition` previously emitted nonconstant branch statements with empty
def/use sets. It now uses the ordinary native statement extractor and lexical
scope rewriting. Consequently a condition such as `if enabled` depends on the
reaching definition of `enabled`, short-circuit operands retain their individual
uses, and assignment conditions retain both the assignment and RHS use.
Constant-condition pruning is unchanged.

Ruby `begin` expressions without `rescue` or `ensure` descend into their child
statements directly. Redispatching the same `begin` node through generic block
handling previously caused infinite recursion in eager CFG discovery and lazy
inspection or slicing. Empty, assigned and nested blocks retain local-variable
dependencies and return termination without changing generic block scoping.

Regressions exercise reaching definitions and backward slices in all 20
procedural Tier 1 languages, HCL external conditional inputs, and SQL opaque-body
serialization. The inventory test explicitly classifies ERB and Markdown as
without CFG profiles. Swift and Scala use assignment definitions because their
local declaration lowering needs a separate repair. Dart unfielded `if`
predicates are selected before ordinary lowering. Additional tests cover scoped
shadowing, short-circuiting, C assignment conditions, cache-index migration and
on-demand archive rebuilding. This repairs input facts; it does
not implement the separate argument/formal/return binding work in issue #198.
