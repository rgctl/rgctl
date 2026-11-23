# Source-native function operand records

`ControlFlowGraph::operands` exposes compact per-callable syntax facts for the 20 procedural Tier 1 languages. Lowering reuses the native parsed tree
selected by the CFG builder, including exact byte-range callable selection. It
does not invoke a compiler, external indexer or an adapter-side source parser.

## Records

- `FormalFact` records a named entry value, explicit argument convention and
  default/optional/rest information. Java/JavaScript implicit receivers and
  Go/Rust explicit receivers are distinct from ordinary argument positions.
  Python descriptor conventions preserve the declaration's explicit positions;
  a later language-aware binder decides how a bound invocation supplies them.
- `CallSiteFact` records the callee expression, receiver, actual argument range,
  result expression, invocation kind and exact byte span. Nested and repeated
  same-line invocations have separate local IDs. Python class calls remain calls
  until symbol resolution establishes construction; `new` syntax is explicit.
- `ReturnFact` identifies explicit returns and supported implicit arrow/Rust-tail
  results. Go multiple return values remain ordered. Aggregate construction and
  interpolated expressions preserve their child operands.
- `ExpressionFact` distinguishes parameter entry values, unbound name occurrences,
  member selection, invocation results, construction, constants and computations.
  A computation's child is not automatically an object alias.

Names are interned once per function; argument and expression-reference arrays
are pooled. IDs use checked `u32` indices scoped to the callable and its source/
semantics revision. Transient tree-sitter node IDs are only a lowering memo and
are never persisted. Canonical callable UUIDs remain in the enclosing archive
record. An `OperandNameId` identifies spelling, not a lexical symbol binding.

```rust
let facts = &cfg.operands;
let call = &facts.calls[call_id.0 as usize];
let actuals = call.arguments.slice(&facts.arguments);
let receiver = call.receiver;
let result = call.result;
```

These are syntax-role facts, not an assertion that every captured call executes.
The shared [invocation binder](call-bindings.md) consumes them for actual/formal,
receiver and return transfers. Typed [formal entry definitions](formal-entry-definitions.md)
bridge incoming formal IDs to local statement PDG nodes. [Local value transfers](local-value-flow.md) now link supported body reads and
RHS values to exact scoped CFG positions. Inferred Calls edges and shared
interprocedural taint remain subsequent work.
Consumers must use shared native binding/dataflow analysis, not match names or
reparse statement text independently.

## Coverage

`OperandStatus` distinguishes a manually constructed/unbuilt CFG, an unsupported
language, and captured native syntax. Diagnostics mark unsupported patterns,
nested callable boundaries, malformed syntax and depth limits. A nested callable's
body is not attributed to its enclosing function. Captured or empty records are
not upgrade-safety or semantic-completeness verdicts. Destructuring and named Go
returns currently require further modeling. Receiver-origin propagation, heap
effects, callable capture and framework semantics remain separate work.

## Persistence and cache reuse

Operand pools persist with the CFG in **RBCP v7**, retaining the existing indexed
footer layout. CFG semantics revision is **4**; earlier records and index keys
must be rebuilt. Deserialization validates operand indices and ranges. Native
tests exercise fresh trees, bincode round trips, mmap archive reads and malformed
references.

Incremental analysis indices now retain the canonical declaration's byte range
and start line. Fast hits, UUID remaps and the whole-batch shortcut all reject
moved or unknown positions even when the body hash is unchanged. This prevents
returning absolute source spans from before a function was relocated. Unchanged
source positions retain the normal fast path. The first file/name/hash fields
of index keys remain available to existing dashboard metadata consumers.

The body walker reuses its traversal stack; name lookup borrows source strings
and owns only the persistent interned copy. Operand collection currently adds
a bounded native AST traversal per CFG build. Default discovery without procedural
analysis does not build these records. Whole-repository time, disk and allocator
RSS still require measurement; record layout alone is not a performance claim.

## Tier 1 applicability

Canonical correctness fixtures cover native parameters, local aliases, invocation
arguments and returned values across all 20 procedural Tier 1 languages. Shell
positional parameters and arguments are captured, but exit status and output
streams are not ordinary returned object values. SQL dollar-quoted routine bodies
are opaque to the bundled grammar; HCL has no project-defined callable parameter
bindings, and ERB/Markdown do not emit native procedural callable bodies. These
boundaries must not be interpreted as complete negative flow results.

Native lowering reports unsupported patterns, depth limits and malformed syntax.
Expression descent stops at 64 frames to protect worker stacks, including C/C++
method chains. Routine child lists use inline storage before spilling to the heap.
