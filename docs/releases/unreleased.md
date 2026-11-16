# Unreleased (post v0.4.27)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Highlights

### 24-language lossless write-back & byte-span parity (#199)

- **All 24 Tier-1 languages emit symbol, identifier, and call-site byte spans:** expanded declaration (`start_byte`, `end_byte`), relative identifier (`name_rel_offset`), and call-site (`call_site_start_byte`, `call_site_byte_len`) extraction—and full `discover --lossless` / `rgctl writeback` (`verify`, `export`, `patch`) BLAKE3 round-trip coverage—from 16 to all **24** Tier-1 plugins (`c`, `cpp`, `csharp`, `go`, `groovy`, `java`, `javascript`, `kotlin`, `markdown`, `php`, `puppet`, `erb`, `python`, `ruby`, `rust`, `typescript`, `swift`, `scala`, `dart`, `elixir`, `lua`, `bash`, `sql`, `hcl`).
- **42 Tier-2 `tags.scm` & 3 Mainframe plugins also record byte spans:** `QueryLanguagePlugin` (`rgctl-lang-runtime`) and the COBOL, PL/I, and JCL extractors now populate symbol and call-site byte spans during worker extraction; `ConfigKey` nodes (`yaml`, `json`, `toml`, `properties`, `xml`) preserve source line spans (`start_line`..`end_line`).

### Structured CLI, incremental update & graph analytics improvements (#187, #199)

- **`find -f json` projects `signature`:** entity rows now include the declaration `signature` alongside `qualified_name` and byte spans, allowing callers to disambiguate overloaded methods directly from `find`.
- **`blast-radius --line <N>` disambiguation:** filters duplicate-named or overloaded symbol candidates by declaration line instead of picking the first candidate.
- **Container `callers` / `blast-radius` constructor & file-filter handling:** applies file filters before callable collision checks and recognizes same-name constructors across C#, C++, Java, Kotlin, Scala, and Dart so type-level rollups resolve cleanly.
- **`inspect <sym> ast|cfg|pdg|dom` concrete-body priority:** prefers concrete method definitions over bodiless interface, trait, or protocol declarations sharing the same short name.
- **`security taint -f json` structured `paths`:** JSON taint output now populates `source_symbol`, `sink_symbol`, `path_length`, and ordered `path` step arrays (`symbol`, `file`, `line`, `code_snippet`, `kind`) in addition to summary counts.
- **Deterministic `rgctl update` symbol resolution:** `IncrementalUpdater` now rejects ambiguous cross-file bare names instead of selecting an arbitrary `HashMap` entry, matching cold `discover` so incremental updates equal a fresh cold index.
- **Active-pivot Brandes betweenness & deterministic `MemoryBackend` ordering (#187):** `SampledBetweenness::compute_flat` now samples pivots exclusively from vertices with non-zero out-degree using Horvitz–Thompson normalization (`(|V_active| / k) / ((n - 1)(n - 2))`) and mixes PRNG state through `splitmix64` to eliminate power-of-two LCG low-bit periodicity, preventing zero-out-degree sink/`Import`/`File` nodes (~80% of polyglot graphs) from diluting pivot selection. `MemoryBackend::all_node_ids()` also sorts returned UUIDs so `CodeGraphCsr` and `PetGraphView` assign deterministic dense indices across processes and platforms.
- **Cold `discover` startup, routing & memory optimizations:** `QueryLanguagePlugin` now compiles `tags.scm` `tree_sitter::Query` automatons lazily via `OnceLock` (cutting idle CLI RSS by ~117 MB and startup by ~340 ms), `LanguageRegistry` routes file extensions in O(1) without per-file heap allocations, `discover` reuses its initial `FileDiscoverer` file list instead of walking the repository twice, `save_parked_relations` streams via `bincode::serialize_into`, and cold `FactsCache` skips initial `facts.bin` population above 5k files (Linux cold Gate A down to **45.0 s** vs 75.0 s baseline).

### Language extraction, MRO, CFG, PDG & taint accuracy (#199)

- **Overloaded method outbound `Calls` attribution:** outbound calls from overloads `#2..#N` now resolve `relation.from` by enclosing declaration span/line rather than collapsing onto overload `#1`.
- **Annotated / decorated symbol names & Rust trait defaults:** `extract_name_from_node` skips leading annotation, decorator, and modifier wrappers so declarations such as `@override int auditWeight()` never record `@override` as the method name; Rust trait methods with default bodies no longer emit duplicate `Function` nodes.
- **OOP / trait MRO & receiver call resolution (16 languages):**
  - **Swift:** separates superclass `Extends` from protocol `Implements` and resolves `self.<method>()` to `<Type>.<method>`.
  - **Dart:** extracts `extends`, `with` mixins, and `implements` edges plus local typed-receiver call hints.
  - **Scala:** separates primary `extends` superclass (`Extends`) from `with` traits (`Implements`) and resolves `new T()` local receiver calls.
  - **Elixir:** emits `Implements` for `@behaviour` and `Extends` for `use`, and resolves intra-module calls with arguments.
- **Control-flow graph (CFG) lowering:**
  - **Scala:** fixed a mutual-recursion stack overflow on `match` / `case_block` (`case_clause` `body` field), adding per-arm `case_clause` + `guard` branching and `try` / `catch` / `finally` `Exception` edges.
  - **Kotlin:** lowers `try_expression` (`try`/`catch`/`finally`), single-expression `control_structure_body` arms (`if`/`while`/`for`), and `jump_expression` (`return`, `break`, `continue`).
  - **Ruby:** lowers `case`/`when` arm chains, `begin`/`rescue`/`ensure` exception edges and unwind blocks, and `while`/`until`/`for` `do` loop bodies.
  - **Swift & Dart:** lowers `switch_statement` across `switch_entry` / `switch_block` cases.
  - **Bash:** preserves `declaration_command`, `variable_assignment`, `unset_command`, `test_command`, and `command` as atomic CFG statements (`return` wires exit edges).
- **Def-use, PDG & interprocedural slicing:**
  - Added variable and parameter def-use extraction in `rgctl-analysis` for **C#, Kotlin, Puppet, Dart, Lua, and Bash**.
  - Fixed backward `InterproceduralSlicer` traversal to walk reverse PDG `data_deps` and `control_deps` edges and propagate across formal parameter boundaries.
- **Tier-2 `tags.scm` & Mainframe (`cobol`, `pli`, `jcl`) call resolution:**
  - `QueryLanguagePlugin` populates intra-file container-qualified call hints (`solidity`, `fortran`, `vhdl`, `apex`, `pascal`, `ada`); **Verilog** `tags.scm` captures nested `function_identifier` and `task_identifier` declarations.
  - **COBOL:** emits a callable `PROCEDURE DIVISION` `<entry>` (`Function -> Function`) so cross-program `CALL 'PROG'`, CICS `LINK PROGRAM(...)`, and JCL `EXEC PGM=...` resolve cleanly while preserving callable-only `Calls` endpoints.
  - **JCL:** `PEND` restores the outer `JOB` scope so steps after an inline `PROC ... PEND` qualify under the enclosing job (`JOB::STEP`).

### 71-plugin × 17-dimension correctness matrix & precision invariants (#199)

- **Precision `INV-8` & `INV-9` (`rgctl-graph`):** enforces byte-span bounds, name-within-symbol containment, proper container inequality (out-of-line safe for Go, Rust, C++, Ruby, and C#), and non-collapsing sibling `Function` spans across snapshot tests.
- **6-law in-process metamorphic gate (`tests/metamorphic_correctness_qe.rs`):** verifies Mode Monotonicity (`default` vs `--with-fields` vs `--with-cfg`), Sibling Edit Isolation, Cold vs. Incremental Equivalence, Lazy vs. `--with-cfg` CPG Equivalence, `diff_snapshots` Classification, `content_digest` Thread Determinism, and multi-repo `merge` stitching across all 24 Tier-1 languages in Tier 2 preflight.
- **`expected-facts.json` v2 17-dimension oracle (`tests/graph_correctness.rs`):** `TempDir`-sandboxed and migrated completely off deprecated `rgctl gql` onto canonical structured JSON CLI verbs across all 24 Tier-1 languages, verifying identity/overloads, call graphs, MRO, CFG/DOM/PDG, `slice`, `cpg flows`, `cpg mutations`, `security taint`, `clones`, `semantic`, `review paths`, and graph analytics (`harmonic` + `HyperBall`, `pagerank`, `betweenness`, `migration_plan.json`, Fast-Leiden community separation & connectedness).
- **Full 71-plugin QE suites:** un-stubbed Groovy/Kotlin/Ruby/Puppet CFG and 20-language intra/inter-procedural + sanitizer taint suites, 16-language MRO/DI suite, `tests/mainframe_plugins_correctness.rs` (COBOL, PL/I, JCL), and `tests/tier2_query_plugins_correctness.rs` (all 42 `tags.scm` plugins, `.m` ObjC/MATLAB sniffing, and all 5 config plugins).

## Breaking changes

| Changed | Migration |
|---------|-----------|

## Issues / PRs

| Ref | Title |
|-----|-------|
| [#187](https://github.com/rgctl/rgctl/issues/187) | Sampled Brandes betweenness active-source Horvitz–Thompson pivot sampling, `splitmix64` PRNG mixing, and deterministic `MemoryBackend` node ordering |
| [#199](https://github.com/rgctl/rgctl/issues/199) | Multidimensional 71-plugin × 17-dimension correctness matrix, 24-language lossless spans, and extraction/CFG/PDG/taint fixes |
