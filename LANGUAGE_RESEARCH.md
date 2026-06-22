# Phase 11.1.1: Tree-sitter Grammar Research

Research deliverable for adding 22 Tier 2 languages to rBuilder via TOML configs.
Completed: 2026-06-17.

## Summary

All 22 target languages have viable tree-sitter grammars. **20** publish Rust crates on
crates.io or in-repo `Cargo.toml`; **Clojure** and **Assembly** require git/path
dependencies or alternate packaging. **Elixir**, **Scheme**, and **Clojure** use
identifier-driven `call`/`list` nodes rather than dedicated `def_*` AST nodes — the
generic `function_kinds` / `class_kinds` walker in `src/languages/extraction/tree_sitter.rs`
will under-extract unless Phase 11.1.2 adds custom handlers or query-based extraction.

### Recommended Top 10 for Phase 11.1.2 (HIGH priority)

| Rank | Language | Rationale |
|------|----------|-----------|
| 1 | Swift | Mature grammar, crates.io crate, strong mobile demand |
| 2 | Scala | Official tree-sitter org, JVM/big-data demand |
| 3 | Elixir | Official lang-org grammar, production use at GitHub |
| 4 | Erlang | WhatsApp-maintained, BEAM ecosystem |
| 5 | Dart | Active grammar, Flutter demand |
| 6 | Lua | tree-sitter-grammars org, gaming/embedded |
| 7 | Haskell | Official tree-sitter org, FP community |
| 8 | Julia | Official tree-sitter org, scientific computing |
| 9 | R | r-lib maintained, data-science demand |
| 10 | Nim | Good grammar + Rust crate, systems scripting |

---

## Master Table

| Language | Tree-sitter Repo | Rust Crate | Quality | Function Kinds | Class Kinds | Priority | Est. Effort | Notes |
|----------|------------------|------------|---------|----------------|-------------|----------|-------------|-------|
| Swift | [alex-pinkus/tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift) | `tree-sitter-swift` v0.7.3 | **High** | `function_declaration`, `init_declaration`, `deinit_declaration` | `class_declaration`, `protocol_declaration`, `enum_declaration`, `struct_declaration` | **HIGH** | 1–2 days | Community grammar (not tree-sitter org); actively maintained, 200+ stars, used in production editors |
| Scala | [tree-sitter/tree-sitter-scala](https://github.com/tree-sitter/tree-sitter-scala) | `tree-sitter-scala` v0.26.0 | **High** | `function_definition`, `function_declaration` | `class_definition`, `object_definition`, `trait_definition`, `enum_definition` | **HIGH** | 1–2 days | Official org; Scala 2 + 3; `def` is token not node kind |
| Lua | [tree-sitter-grammars/tree-sitter-lua](https://github.com/tree-sitter-grammars/tree-sitter-lua) | `tree-sitter-lua` v0.5.0 | **High** | `function_declaration`, `function_definition` | _(none — use `table_constructor` for tables only)_ | **HIGH** | 1 day | Official tree-sitter-grammars org; Lua 5.1–5.4 |
| Elixir | [elixir-lang/tree-sitter-elixir](https://github.com/elixir-lang/tree-sitter-elixir) | `tree-sitter-elixir` v0.3.5 | **High** | `call` + `identifier` (`def`, `defp`, `defn`, `defnp`) — see note | `call` + `identifier` (`defmodule`, `defprotocol`, `defstruct`) | **HIGH** | 3–5 days | Official lang-org; **needs custom handler** — defs are `call` nodes, not `def_*` AST nodes |
| Erlang | [WhatsApp/tree-sitter-erlang](https://github.com/WhatsApp/tree-sitter-erlang) | `tree-sitter-erlang` v0.19.0 | **High** | `function_clause`, `fun_decl`, `internal_fun` | `module` | **HIGH** | 2–3 days | WhatsApp-maintained; atoms as names; `function_clause` is primary def node |
| Haskell | [tree-sitter/tree-sitter-haskell](https://github.com/tree-sitter/tree-sitter-haskell) | `tree-sitter-haskell` v0.23.1 | **Medium-High** | `function` (in `decl/function`), `signature` | `data_type`, `class_decl`, `instance` | **HIGH** | 3–4 days | Official org; layout scanner; node kind is `function` inside decl wrapper |
| OCaml | [tree-sitter/tree-sitter-ocaml](https://github.com/tree-sitter/tree-sitter-ocaml) | `tree-sitter-ocaml` v0.25.0 | **High** | `value_definition`, `method_definition`, `external`, `let_binding` | `module_definition`, `class_definition`, `type_definition` | **MEDIUM** | 2–3 days | Three grammars: `ocaml`, `ocaml_interface`, `ocaml_type`; use `grammars/ocaml` |
| Dart | [UserNobody14/tree-sitter-dart](https://github.com/UserNobody14/tree-sitter-dart) | `tree-sitter-dart` v0.0.1 | **Medium-High** | `function_signature`, `method_signature`, `getter_signature`, `setter_signature` | `class_definition`, `enum_declaration`, `mixin_declaration`, `extension_declaration` | **HIGH** | 2–3 days | Community; actively updated; signatures not `function_declaration` |
| R | [r-lib/tree-sitter-r](https://github.com/r-lib/tree-sitter-r) | `tree-sitter-r` v1.2.0 | **High** | `function_definition` | _(none — S3 classes not AST-modeled)_ | **HIGH** | 1–2 days | r-lib / R Foundation ecosystem; simple grammar |
| Julia | [tree-sitter/tree-sitter-julia](https://github.com/tree-sitter/tree-sitter-julia) | `tree-sitter-julia` v0.25.0 | **High** | `function_definition` | `struct_definition`, `module_definition`, `macro_definition`, `abstract_definition` | **HIGH** | 2 days | Official org; `baremodule` for modules |
| Perl | [tree-sitter-perl/tree-sitter-perl](https://github.com/tree-sitter-perl/tree-sitter-perl) | `ts-parser-perl` v1.1.2 | **Medium-High** | `subroutine_declaration_statement`, `method_declaration_statement` | `package_statement` | **MEDIUM** | 2–3 days | Crate name differs: `ts-parser-perl`; large grammar with external scanner |
| Fortran | [stadelmanma/tree-sitter-fortran](https://github.com/stadelmanma/tree-sitter-fortran) | `tree-sitter-fortran` v0.5.1 | **Medium** | `function_statement`, `subroutine_statement`, `module_procedure_statement` | `derived_type_definition`, `module_statement`, `interface` | **LOW** | 2–3 days | Scientific legacy; `function`/`subroutine` wrapper nodes |
| Assembly | [naclsn/tree-sitter-nasm](https://github.com/naclsn/tree-sitter-nasm) | `tree-sitter-nasm` v0.0.1 | **Low** | `instruction`, `preproc_function_def` | _(N/A — no OOP constructs)_ | **LOW** | 3–5 days | NASM-focused; alternatives: `bearcove/tree-sitter-x86asm`, `rush-rs/tree-sitter-asm`; no function symbols in traditional sense |
| Verilog | [tree-sitter/tree-sitter-verilog](https://github.com/tree-sitter/tree-sitter-verilog) | `tree-sitter-verilog` v1.0.3 | **Medium** | `function_declaration`, `task_declaration` | `module_declaration`, `class_declaration`, `interface_declaration` | **LOW** | 3–4 days | Official org (SystemVerilog); 900+ node types; HDL niche |
| VHDL | [jpt13653903/tree-sitter-vhdl](https://github.com/jpt13653903/tree-sitter-vhdl) | `tree-sitter-vhdl` v1.5.0 | **Medium** | `function_declaration`, `procedure_declaration` | `entity_declaration`, `architecture_declaration`, `package_declaration` | **LOW** | 3–4 days | Syntax-highlighting focus; 469 node types |
| COBOL | [yutaro-sakamoto/tree-sitter-cobol](https://github.com/yutaro-sakamoto/tree-sitter-cobol) | `tree-sitter-COBOL` v0.0.1 | **Medium** | `PARAGRAPH`, `PROCEDURE` | `RECORD` (data records, not OOP) | **LOW** | 3–5 days | COBOL85; NIST test suite; crate name is `tree-sitter-COBOL` |
| Pascal | [Isopod/tree-sitter-pascal](https://github.com/Isopod/tree-sitter-pascal) | `tree-sitter-pascal` v0.10.2 | **Medium** | `declProc` (filter `kFunction`, `kProcedure`) | `declClass`, `declEnum`, `declTypes` | **LOW** | 2–3 days | Delphi/FreePascal dialects; node names are generic (`declProc`) |
| Lisp/Scheme | [6cdh/tree-sitter-scheme](https://github.com/6cdh/tree-sitter-scheme) | `tree-sitter-scheme` v0.24.7-1 | **Low-Medium** | `list` + `symbol` (`define`, `define-syntax`) | _(none)_ | **LOW** | 4–5 days | S-expression grammar; **needs custom handler**; alt: `theHamsta/tree-sitter-commonlisp` for Common Lisp |
| Clojure | [sogaiu/tree-sitter-clojure](https://github.com/sogaiu/tree-sitter-clojure) | _(no Rust crate)_ | **Low-Medium** | `list_lit` + `sym_lit` (`defn`, `defmacro`) | _(none)_ | **LOW** | 5+ days | No Cargo.toml; npm-only; **needs custom handler** + git dep |
| F# | [ionide/tree-sitter-fsharp](https://github.com/ionide/tree-sitter-fsharp) | `tree-sitter-fsharp` | **Medium** | `function_or_value_defn`, `member_definition` | `class`, `enum_type_defn`, `delegate_type_defn`, `anon_type_defn` | **MEDIUM** | 3–4 days | Ionide/F# community; dual grammars (`fsharp`, `fsharp_signature`); WIP per README |
| Zig | [maxxnino/tree-sitter-zig](https://github.com/maxxnino/tree-sitter-zig) | `tree-sitter-zig` v0.0.1 | **Medium** | `fn` | `struct`, `enum`, `union` | **MEDIUM** | 2 days | Community; alt: `tree-sitter-grammars/tree-sitter-zig` (no crate yet); node kind is literally `fn` |
| Nim | [alaviss/tree-sitter-nim](https://github.com/alaviss/tree-sitter-nim) | `tree-sitter-nim` v0.6.2 | **Medium-High** | `proc_declaration`, `func_declaration`, `method_declaration` | `type_declaration`, `enum_declaration`, `object_declaration` | **MEDIUM** | 2 days | Good tags.scm; converter/template/macro decls available |
| Crystal | [crystal-lang-tools/tree-sitter-crystal](https://github.com/crystal-lang-tools/tree-sitter-crystal) | `tree-sitter-crystal` v0.0.1 | **Medium** | `def`, `fun_def`, `method_def`, `macro_def` | `class_def`, `module_def`, `struct_def`, `enum_def`, `lib_def` | **MEDIUM** | 2 days | Lang-tools org; Ruby-like syntax; no tags.scm yet |

---

## Priority Ranking

### HIGH — Mature grammar + high demand (Phase 11.1.2 first batch)

| Language | Demand Driver | Grammar Confidence |
|----------|---------------|-------------------|
| Swift | iOS/macOS/server (Vapor) | ★★★★★ |
| Scala | JVM, Spark, Akka | ★★★★★ |
| Elixir | Phoenix, LiveView, BEAM | ★★★★★ |
| Erlang | OTP, telecom, RabbitMQ | ★★★★☆ |
| Dart | Flutter, server-side | ★★★★☆ |
| Lua | Gaming (Roblox/Love2D), Neovim, Redis | ★★★★★ |
| Haskell | FP, academia, blockchain | ★★★★☆ |
| Julia | Scientific computing, ML | ★★★★★ |
| R | Statistics, bioinformatics | ★★★★★ |

### MEDIUM — Good grammar, moderate demand or integration friction

| Language | Demand Driver | Grammar Confidence |
|----------|---------------|-------------------|
| Nim | Systems scripting, Python-alternative | ★★★★☆ |
| Crystal | Ruby-like compiled language | ★★★☆☆ |
| Zig | Systems programming (growing) | ★★★☆☆ |
| OCaml | ML family, finance, compilers | ★★★★★ |
| F# | .NET functional, Ionide ecosystem | ★★★☆☆ |
| Perl | Legacy ops, bioinformatics | ★★★★☆ |

### LOW — Experimental, niche, or poor generic-extraction fit

| Language | Blocker |
|----------|---------|
| Fortran | Legacy scientific; moderate grammar quality |
| COBOL | Enterprise legacy; unusual paragraph model |
| Assembly | No functions/classes; label/instruction model |
| Verilog/VHDL | HDL niche; very large grammars |
| Pascal | Declining usage; generic node names |
| Lisp/Scheme | S-expression `define` forms need custom extraction |
| Clojure | No Rust crate; s-expression forms |

---

## Cargo Feature Flags (proposed for Phase 11.1.2)

Following existing `lang-{name}` convention in `Cargo.toml`:

```
lang-swift      → tree-sitter-swift
lang-scala      → tree-sitter-scala
lang-lua        → tree-sitter-lua
lang-elixir     → tree-sitter-elixir
lang-erlang     → tree-sitter-erlang
lang-haskell    → tree-sitter-haskell
lang-ocaml      → tree-sitter-ocaml
lang-dart       → tree-sitter-dart
lang-r          → tree-sitter-r
lang-julia      → tree-sitter-julia
lang-perl       → ts-parser-perl        # note: different crate name
lang-fortran    → tree-sitter-fortran
lang-assembly   → tree-sitter-nasm      # or tree-sitter-x86asm
lang-verilog    → tree-sitter-verilog
lang-vhdl       → tree-sitter-vhdl
lang-cobol      → tree-sitter-COBOL     # note: capital COBOL in crate name
lang-pascal     → tree-sitter-pascal
lang-scheme     → tree-sitter-scheme
lang-clojure    → (git dependency)      # no published crate
lang-fsharp     → tree-sitter-fsharp
lang-zig        → tree-sitter-zig
lang-nim        → tree-sitter-nim
lang-crystal    → tree-sitter-crystal
```

Proposed bundle update for Phase 11.1.3:

```toml
[bundles.tier2-high]
description = "High-priority Tier 2 languages (Phase 11)"
languages = ["swift", "scala", "elixir", "erlang", "dart", "lua", "haskell", "julia", "r"]
```

---

## Per-Language Integration Notes

### Swift (`alex-pinkus/tree-sitter-swift`)

- **Extensions**: `.swift`
- **TOML function_kinds**: `function_declaration`, `init_declaration`
- **TOML class_kinds**: `class_declaration`, `protocol_declaration`, `enum_declaration`, `struct_declaration`
- **Import kinds**: `import_declaration`
- **Validation**: `cargo add tree-sitter-swift --optional` then `cargo build --features lang-swift`

### Scala (`tree-sitter/tree-sitter-scala`)

- **Extensions**: `.scala`, `.sc`
- **TOML function_kinds**: `function_definition`, `function_declaration`
- **TOML class_kinds**: `class_definition`, `object_definition`, `trait_definition`, `enum_definition`
- **Notes**: `object_definition` holds singleton objects; `given_definition` for Scala 3 implicits

### Lua (`tree-sitter-grammars/tree-sitter-lua`)

- **Extensions**: `.lua`
- **TOML function_kinds**: `function_declaration`, `function_definition`
- **TOML class_kinds**: _(empty — Lua has no classes)_
- **Notes**: Method syntax `obj:method()` appears as `function_definition` with `method_index_expression`

### Elixir (`elixir-lang/tree-sitter-elixir`) ⚠️ Custom handler recommended

- **Extensions**: `.ex`, `.exs`
- **AST challenge**: `def foo` parses as `(call target: (identifier) @ignore arguments: ...)`. The node kind is `call`, not `def`.
- **tags.scm approach**: Matches `call` where target identifier is `def`/`defp`/`defn`/etc.
- **Phase 11.1.2 options**:
  1. Add `ElixirPlugin` custom handler (recommended)
  2. Or extend generic extractor with query-file support
- **Fallback generic kinds**: `anonymous_function` (captures `fn ->` only)

### Erlang (`WhatsApp/tree-sitter-erlang`)

- **Extensions**: `.erl`, `.hrl`
- **TOML function_kinds**: `function_clause`, `fun_decl`
- **TOML class_kinds**: `module` (module is closest to compilation unit)
- **Notes**: Function names are atoms; `-module(foo).` creates module context

### Haskell (`tree-sitter/tree-sitter-haskell`)

- **Extensions**: `.hs`, `.lhs`
- **TOML function_kinds**: `function` (child of `decl` rules)
- **TOML class_kinds**: `data_type`, `class_decl`, `newtype`, `type_family`
- **Notes**: External scanner for layout/off-side rule; names in `variable` children

### OCaml (`tree-sitter/tree-sitter-ocaml`)

- **Extensions**: `.ml`, `.mli`
- **TOML function_kinds**: `value_definition`, `method_definition`, `external`
- **TOML class_kinds**: `module_definition`, `class_definition`, `type_definition`
- **Notes**: Requires `tree_sitter_ocaml::LANGUAGE` from ocaml sub-grammar; separate interface grammar for `.mli`

### Dart (`UserNobody14/tree-sitter-dart`)

- **Extensions**: `.dart`
- **TOML function_kinds**: `function_signature`, `method_signature`
- **TOML class_kinds**: `class_definition`, `enum_declaration`, `extension_declaration`, `mixin_declaration`
- **Notes**: Constructor nodes: `constructor_signature`, `factory_constructor_signature`

### R (`r-lib/tree-sitter-r`)

- **Extensions**: `.r`, `.R`
- **TOML function_kinds**: `function_definition`
- **TOML class_kinds**: _(empty)_
- **Notes**: Simplest integration of the 22 languages

### Julia (`tree-sitter/tree-sitter-julia`)

- **Extensions**: `.jl`
- **TOML function_kinds**: `function_definition`
- **TOML class_kinds**: `struct_definition`, `module_definition`, `macro_definition`, `abstract_definition`
- **Notes**: `baremodule` for anonymous modules

### Perl (`tree-sitter-perl/tree-sitter-perl`)

- **Extensions**: `.pl`, `.pm`, `.t`
- **TOML function_kinds**: `subroutine_declaration_statement`, `method_declaration_statement`
- **TOML class_kinds**: `package_statement`
- **Notes**: Crate published as `ts-parser-perl`, not `tree-sitter-perl`

### Fortran (`stadelmanma/tree-sitter-fortran`)

- **Extensions**: `.f`, `.f90`, `.f03`, `.f08`
- **TOML function_kinds**: `function_statement`, `subroutine_statement`
- **TOML class_kinds**: `derived_type_definition`, `module_statement`
- **Notes**: Fixed-form and free-form supported

### Assembly (`naclsn/tree-sitter-nasm`)

- **Extensions**: `.asm`, `.s`, `.nasm`
- **Recommended repo**: `naclsn/tree-sitter-nasm` (NASM syntax, 142 node types)
- **Alternatives**: `bearcove/tree-sitter-x86asm` (GAS), `rush-rs/tree-sitter-asm` (Rust asm)
- **TOML function_kinds**: _(none useful)_ — consider `label` + `instruction` for symbol map
- **Notes**: Not a good fit for function/class extraction; regex fallback may be better

### Verilog / SystemVerilog (`tree-sitter/tree-sitter-verilog`)

- **Extensions**: `.v`, `.sv`, `.vh`
- **TOML function_kinds**: `function_declaration`, `task_declaration`
- **TOML class_kinds**: `module_declaration`, `class_declaration`
- **Notes**: Very large grammar (988 node types); consider SystemVerilog-only scope

### VHDL (`jpt13653903/tree-sitter-vhdl`)

- **Extensions**: `.vhd`, `.vhdl`
- **TOML function_kinds**: `function_declaration`, `procedure_declaration`
- **TOML class_kinds**: `entity_declaration`, `package_declaration`
- **Notes**: No tags.scm; node kinds verified from `src/node-types.json`

### COBOL (`yutaro-sakamoto/tree-sitter-cobol`)

- **Extensions**: `.cob`, `.cbl`, `.cpy`
- **TOML function_kinds**: `PARAGRAPH`, `PROCEDURE`
- **TOML class_kinds**: _(empty — `RECORD` is data, not OOP)_
- **Notes**: Paragraph names are the closest analog to functions

### Pascal (`Isopod/tree-sitter-pascal`)

- **Extensions**: `.pas`, `.pp`, `.dpr`
- **TOML function_kinds**: `declProc` (contains `kFunction`, `kProcedure` children)
- **TOML class_kinds**: `declClass`, `declEnum`, `declTypes`
- **Notes**: May need custom name extraction from `declProc` children

### Lisp / Scheme (`6cdh/tree-sitter-scheme`)

- **Extensions**: `.scm`, `.ss`, `.sls`
- **Alternative**: `theHamsta/tree-sitter-commonlisp` for Common Lisp (`.lisp`, `.cl`)
- **TOML function_kinds**: _(requires custom handler for `define`/`defun` forms)_
- **Notes**: Generic walker will not extract `define` bindings

### Clojure (`sogaiu/tree-sitter-clojure`)

- **Extensions**: `.clj`, `.cljs`, `.cljc`, `.edn`
- **TOML function_kinds**: _(requires custom handler for `defn`/`defmacro` in `list_lit`)_
- **Notes**: No Rust crate; must use git dependency or skip until packaging exists

### F# (`ionide/tree-sitter-fsharp`)

- **Extensions**: `.fs`, `.fsi`, `.fsx`
- **TOML function_kinds**: `function_or_value_defn`, `member_definition`
- **TOML class_kinds**: `class`, `enum_type_defn`, `delegate_type_defn`
- **Notes**: README says WIP; use `fsharp` sub-directory grammar; `.fsi` needs `fsharp_signature` grammar

### Zig (`maxxnino/tree-sitter-zig`)

- **Extensions**: `.zig`, `.zon`
- **TOML function_kinds**: `fn`
- **TOML class_kinds**: `struct`, `enum`, `union`
- **Notes**: Node kind is literally `fn`; test name extraction from child `IDENTIFIER`

### Nim (`alaviss/tree-sitter-nim`)

- **Extensions**: `.nim`, `.nims`
- **TOML function_kinds**: `proc_declaration`, `func_declaration`, `method_declaration`
- **TOML class_kinds**: `type_declaration`, `enum_declaration`, `object_declaration`
- **Notes**: Also has `converter_declaration`, `template_declaration`, `macro_declaration`

### Crystal (`crystal-lang-tools/tree-sitter-crystal`)

- **Extensions**: `.cr`
- **TOML function_kinds**: `def`, `fun_def`, `method_def`
- **TOML class_kinds**: `class_def`, `module_def`, `struct_def`, `enum_def`, `lib_def`
- **Notes**: Ruby-like; `lib_def` for C bindings

---

## Grammar Validation Checklist

For each language before merging TOML config:

```bash
# Example for Swift
cargo add tree-sitter-swift --optional
# Add to Cargo.toml: lang-swift = ["tree-sitter-swift"]
cargo build --features lang-swift
cargo test --features lang-swift
```

| Language | Repo reachable | Rust crate | node-types.json | tags.scm | crates.io |
|----------|---------------|------------|-----------------|----------|-----------|
| Swift | ✅ | ✅ | ✅ | ✅ | ✅ |
| Scala | ✅ | ✅ | ✅ | ✅ | ✅ |
| Lua | ✅ | ✅ | ✅ | ✅ | ✅ |
| Elixir | ✅ | ✅ | ✅ | ✅ | ✅ |
| Erlang | ✅ | ✅ | ✅ | ✅ | ✅ |
| Haskell | ✅ | ✅ | ✅ | ✅ (highlights) | ✅ |
| OCaml | ✅ | ✅ | ✅ | ✅ | ✅ |
| Dart | ✅ | ✅ | ✅ | ✅ | ✅ |
| R | ✅ | ✅ | ✅ | ✅ | ✅ |
| Julia | ✅ | ✅ | ✅ | ✅ (highlights) | ✅ |
| Perl | ✅ | ✅ | ⚠️ git only | ✅ (highlights) | ✅ |
| Fortran | ✅ | ✅ | ✅ | ✅ | ✅ |
| Assembly | ✅ | ✅ | ✅ | ⚠️ | ✅ |
| Verilog | ✅ | ✅ | ✅ | ❌ | ✅ |
| VHDL | ✅ | ✅ | ✅ | ❌ | ✅ |
| COBOL | ✅ | ✅ | ✅ | ❌ | ⚠️ |
| Pascal | ✅ | ✅ | ✅ | ✅ (highlights) | ✅ |
| Scheme | ✅ | ✅ | ✅ | ✅ (highlights) | ✅ |
| Clojure | ✅ | ❌ | ✅ | ❌ | ❌ |
| F# | ✅ | ✅ | ✅ | ❌ | ✅ |
| Zig | ✅ | ✅ | ✅ | ✅ (highlights) | ⚠️ |
| Nim | ✅ | ✅ | ✅ | ✅ (highlights) | ✅ |
| Crystal | ✅ | ✅ | ✅ | ❌ | ⚠️ |

---

## Acceptance Criteria Status

- ✅ All 22 languages researched
- ✅ Grammar repos identified and validated (GitHub API, 2026-06-17)
- ✅ Node kinds documented (from `node-types.json`, `grammar.js`, and `queries/tags.scm`)
- ✅ Priority ranking with rationale
- ✅ Cargo feature flag names proposed
- ✅ Ready for Phase 11.1.2 (TOML config creation)

## Next Steps (Phase 11.1.2)

1. Add TOML entries in `languages.toml` for top 10 HIGH priority languages
2. Add optional `tree-sitter-*` dependencies and `lang-*` features to `Cargo.toml`
3. Write `#[cfg(feature = "lang-*")]` integration tests per language
4. Plan custom handlers for Elixir, Scheme, and Clojure (generic extraction insufficient)
5. Update `bundles.tier2-high` in `languages.toml` (Phase 11.1.3)
