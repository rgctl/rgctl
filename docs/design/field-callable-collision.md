# Field/callable collision and independent materialization

## Confirmed failure

Enabling fields previously reduced function counts in four profiling corpora.
A replay of extractor output through the old insertion rule, with CFG entirely
absent, reproduced every lost declaration:

| Corpus | Functions suppressed by fields | Representative declaration |
| --- | ---: | --- |
| Node.js | 1 | `RandomReadStream._process` (bound method assigned to `this._process`) |
| Metasfresh | 476 | Lombok boolean getters such as `SAPGLJournalLine.isFieldsReadOnlyInUI` |
| Home Assistant | 234 | Lambda-backed class attributes such as `AccuWeatherSensorDescription.attr_fn` |
| Discourse | 23 | Singleton methods such as `Category.subcategory_ids` |

These counts match the default-versus-CFG function-count differences in the
2026-11-11 profile at commit `bbe5f35d`. The replay counts source declarations;
the CLI additionally counts external stubs, so absolute totals are different.
Extraction failures and skipped files are separate existing corpus limitations.

## Cause

Stable node IDs already distinguish node kind. The builder's insertion index
instead used only `file::qualified_name` plus an optional callable discriminator.
Materialized fields entered that index before later callable symbols. A callable
without a discriminator could therefore reuse the field entry and be discarded.
Java's generated Lombok getter is one example; the same mechanism affects other
languages. This is not a hash collision, an overload-signature collision, or a
CFG algorithm removing functions.

Callable discriminators distinguish callable declarations from other callable
declarations. Fields still need declaration/name spans for editing and graph
queries. Neither feature replaces the other.

## Fix

The insertion index includes `NodeType` alongside its existing logical key.
UUID construction and qualified names remain unchanged. Complexity lookup,
external-stub reuse, and relation resolution respect the kind distinction.
Calls resolve to functions; generic references with multiple kinds remain
ambiguous rather than selecting an arbitrary declaration. Incremental relation
rebuilding preserves candidates and explicit identity discriminators instead of
overwriting them in a single-value name map.

Extraction semantics version 11 forces graphs built with the old insertion rule
to rebuild. Facts-cache format 8 invalidates extraction records affected by the declaration and field-span correctness fixes.

## Separate execution controls

```sh
rgctl discover . --with-fields
```

This runs field materialization without CFG, dominators, PDG, or taint. The
internal pipeline already supported this independently; the CLI now exposes it.
`--with-cfg` still includes fields because that existing mode needs them.
`--full` also includes them. Default cold discovery does not add materialized
fields, although some plugins already emit native property symbols by default.

A digest marker records field capability. Requesting fields against an existing
field-less snapshot rebuilds it, including when `--files` was supplied, so the
upgrade covers every file. Incremental updates retain field capability and
refresh its marker. A plain discover may reuse a richer existing snapshot; a
fresh default rebuild removes the field-capability marker.

## Verification

Regression coverage checks same-named fields and callables in Java, Python,
Ruby, and JavaScript, stable function IDs and hashes with fields on/off, call
edges pointing to methods rather than fields, and incremental ambiguity handling.
A CLI regression covers a warm field-less snapshot, a whole-graph capability
upgrade requested with `--files`, repeated warm discovery, and subsequent source
updates. It asserts that no CFG/PDG archive is created. The existing Tier 1 field
inventory covers every applicable Tier 1 language.

Profiling artifacts and benchmark scripts are kept outside the source tree.

## Field correctness review regressions

The shared span helper excludes annotations/decorators and initializer/default
expressions when selecting declaration identifiers; record searches are limited
to components. Quoted Elixir struct atoms retain identifier-content spans.
Lua table members follow the matching left-hand assignment slot, keep nested
owners, and distinguish local bindings in separate functions/blocks without
using source coordinates as identity. Tables passed as arguments, returned from
functions, or stored in array positions do not inherit unrelated assignment
owners. Focused regressions live in `tests/tier1_field_review_regressions.rs` and
the Lua single-pass unit tests, alongside the full Tier 1 field inventory.
