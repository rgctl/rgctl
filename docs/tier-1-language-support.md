# Tier 1 language support — contributor requirements

This document defines what **fully supported** means for a programming language in rgctl, and the concrete steps to add or promote a language to that level.

**Audience:** contributors adding a new Tier 1 language or bringing a language to parity with the current bar (Java-shaped Layer F + Layers A–E).

**Related docs:** [language-testing-guide.md](language-testing-guide.md) (where to add tests / what to assert), [Code_structure.md](Code_structure.md) (crate layout), [dashboard-design.md](dashboard-design.md) (dashboard bundle), [user-guide.md](user-guide.md) (discover / serve), [hybrid-cpg-plan.md](design/hybrid-cpg-plan.md) (CPG).

---

## 1. Language tiers (quick reference)

rgctl uses a **hybrid tiering** model:

| Tier | Handler | Crate pattern | Indexing | CFG / PDG / taint | Call graph | Hybrid CPG (Layer F) |
|------|---------|---------------|----------|-------------------|------------|----------------------|
| **Tier 1** | `custom` — dedicated `LanguagePlugin` | `rgctl-lang-{id}/` | Rich symbols + relations | **Required** | **Required** (`Calls` at minimum) | **Required** — same bar as Java |
| **Tier 2** | Generic tree-sitter | `rgctl-lang-{id}/` + `config.rs` | Kinds from `LanguageConfig` | Optional | Usually none | Not required |
| **Tier 3** | Regex | `rgctl-lang-{id}/` + regex patterns | Pattern-based symbols | No | No | No |

**Tier 1 custom plugins today:** Rust, Python, Ruby, PHP, TypeScript, JavaScript, Go, Java, C#, C, C++, Puppet, Kotlin, Groovy — see `languages.toml` (`handler = "custom"`).

**Markdown** is a separate **custom markup plugin** (`rgctl-lang-markdown`): documentation context graph only — not Tier 1 and not generic Tier 2. See [markdown-context.md](markdown-context.md).

**Fully supported** means the language passes **all** of [Layers A–F](#2-capability-checklist--fully-supported) with automated tests. Layer F (hybrid CPG: fields, constructors, typed params, field-write mutations) is **not optional** for Tier 1 — Java is the reference implementation, not a special case.

Honesty limits still apply (no full points-to, no reflection, dynamic languages may emit more `Unresolved` receivers) — but plugins must ship the same *shapes* and golden mutation fixture as Java.

---

## 2. Capability checklist — “fully supported”

A language is **fully supported** when all rows are ✅ and backed by automated tests.

### Layer A — Graph plugin (indexing)

| # | Requirement | Where |
|---|-------------|--------|
| A1 | Custom `LanguagePlugin` (not generic Tier 2 only) | `crates/rgctl-lang-{id}/src/plugin.rs` |
| A2 | Tree-sitter grammar wired (`grammar()` + parse) | Plugin + `tree-sitter-{id}` crate dep |
| A3 | **Symbols:** functions/methods with name, location, signature, parameters | `extract_symbols()` |
| A4 | **Symbols:** types (class/struct/interface/enum as appropriate) | `extract_symbols()` |
| A5 | **Relations:** `Calls` between functions | `extract_relations()` — use `rgctl_plugin_api::walk_calls` or language-specific walker |
| A6 | **Relations (OOP / composition):** `Extends` / `Implements` (or language equivalent: Go struct embed → `Extends`, method-set satisfaction → `Implements`; Rust trait impls when extracted) | Required for Tier 1 — not optional. Document honesty limits (no full points-to) separately. |
| A7 | Cyclomatic / cognitive complexity for functions | `calculate_complexity()` |
| A8 | Entry in `languages.toml` with `handler = "custom"` | Repo root |
| A9 | Registered in `rgctl-languages` | `crates/rgctl-languages/src/lib.rs` |

### Layer B — Analysis profile (CFG pipeline)

| # | Requirement | Where |
|---|-------------|--------|
| B1 | `LanguageAnalysisProfile` with `cfg_enabled: true` | `crates/rgctl-analysis/src/language_profile.rs` |
| B2 | `tree-sitter-{id}` dependency on **`rgctl-analysis`** | `crates/rgctl-analysis/Cargo.toml` |
| B3 | `function_kinds` match tree-sitter node kinds used in CFG lookup | `language_profile.rs` + `cfg_builder.rs` |
| B4 | CFG builders for control flow: `if`, loops, `return`, `break`/`continue` | `crates/rgctl-analysis/src/cfg_builder.rs` |
| B5 | CFG builders for language-specific control flow (e.g. `switch`, `select`, `match`, `try`) | `cfg_builder.rs` |
| B6 | Definition-use extraction for assignments / declarations | `crates/rgctl-analysis/src/def_use.rs` |
| B7 | PDG builds from CFG + source (automatic once CFG + def/use work) | `crates/rgctl-analysis/src/pdg.rs` |
| B8 | `discover --with-cfg` / `discover --with-cfg --with-security --with-taint` includes `.ext` files | Automatic via `cfg_language_id_from_path` in `discover_impl.rs` |

### Layer C — Security & interprocedural

| # | Requirement | Where |
|---|-------------|--------|
| C1 | `taint_enabled: true` on profile | `language_profile.rs` |
| C2 | Declarative taint pack under `crates/rgctl-analysis/rules/taint/{lang}.yaml` (sources, sinks, sanitizers) | Schema + engine in `taint_rules.rs`; do **not** add `detect_{lang}_patterns()` in Rust |
| C3 | Taint routed via `canonical_language_id()` | `TaintAnalyzer::detect_patterns` / `TaintRuleSet::for_language` |
| C4 | Interprocedural CFG uses correct language (not wrong grammar) | `interprocedural_cfg.rs` → `language_id_from_path` |
| C5 | Slice CLI resolves language from file path | `src/cli/context.rs` → `language_from_path` |

### Layer D — Dashboard & UX

| # | Requirement | Where |
|---|-------------|--------|
| D1 | `discover --with-cfg --with-security --with-taint` writes `.rgctl/dashboard/` with CFG index populated | `cfg_index.json` `available: true` |
| D2 | Per-function CFG + dominance render in dashboard | Manual smoke or Playwright |
| D3 | Dataflow / taint tabs show data when flows exist | PDG + taint archive export |
| D4 | Blast radius lists functions with non-zero scores when call graph exists | `manifest.json` `calls_count` > 0 |

### Layer E — Tests (required for merge)

| # | Requirement | Where |
|---|-------------|--------|
| E1 | Plugin unit tests: symbols + at least one `Calls` relation | `crates/rgctl-lang-{id}/src/plugin.rs` `#[cfg(test)]` |
| E2 | CFG unit tests: branching function + loop cycle | `crates/rgctl-analysis/src/cfg_builder.rs` tests |
| E3 | Taint unit/integration test: at least one source→sink path | `tests/taint_analysis.rs` or `tests/{lang}_taint.rs` |
| E4 | Fixture integration test on a small real repo | e.g. `tests/go_cfg_analysis.rs` |
| E5 | Dashboard golden gate: `discover --with-cfg --with-security --with-taint` + bundle assertions | e.g. `tests/dashboard_ecommerce_go.rs` + `tests/dashboard_harness.rs` |
| E6 | `cargo test` + `cargo clippy` clean for touched crates | CI |

### Layer F — CPG readiness (hybrid CPG) — **required for Tier 1**

Required for high-quality `cpg mutations` / typed field writes. Reference: Java (`rgctl-lang-java`) + `field_write` golden tests. See [hybrid-cpg-plan.md](design/hybrid-cpg-plan.md).

| # | Requirement | Where | Acceptance |
|---|-------------|--------|------------|
| F1 | Type symbols populate **`fields[]`** (name + best-effort type string) | `extract_symbols()` | Plugin unit test lists ≥1 field with type when the grammar has types |
| F2 | **Constructors** (or language equivalent) extracted as functions; detectable as ctor | Plugin metadata | `metadata.is_constructor: true` **and** qualified name ending in `.<init>` or `::<init>` (Java/C#/TS/JS/Python/`New*` Go / Rust `new` / C++ same-name ctor). Languages without ctors (C): document limit; golden test may mark an init helper as ctor in the graph node |
| F3 | Methods expose **typed parameters** when the grammar has them | `parameters[].param_type` | Plugin unit test; dynamic langs may leave `None` but still extract names |
| F4 | CFG/`def_use`: **field-access LHS** recorded as member write (`obj.field`; `->` normalized to `.`) | `def_use.rs` + lang kinds | Covered by shared field-access kinds + lang decl kinds (`lexical_declaration`, etc.) |
| F5 | Best-effort local/param types for CFG functions | `field_write_locals.rs` | `merge_local_types("{id}", …)` recovers formals + typed locals (or copy-assign inference for JS) |
| F6 | Golden mutation fixture: type `T` with field write outside ctor → `cpg mutations` / index query hits it with `--exclude-ctors` | `field_write::tests::{id}_cfg_captures_field_write_and_query` | Exactly one non-ctor hit for the typed write |
| F7 | Document resolution limits (no reflection, no full inference) | This doc + hybrid plan | Honesty note in PR / language section |

**Graph extract** must populate `Symbol.fields` on type symbols (F1). **Graph materialization** of those fields as `Variable` nodes under the owning type (`Contains`) happens only when discover runs with **`--with-cfg`** (CPG path) — default discover keeps fields on symbols without emitting per-field graph nodes (`rgctl-extraction` graph builder). Empty `fields` on the symbol is not enough for Tier 1 plugins.

**Non-negotiable:** a new Tier 1 language that skips Layer F is **not mergeable** as Tier 1. Ship it as Tier 2 until F1–F6 land.

#### Layer F language notes (honesty)

| Language | Ctor convention | Type strength |
|----------|-----------------|---------------|
| Java / C# | Real constructors → `Type.<init>` | Strong |
| C++ | Name == enclosing class → `Type::<init>` | Strong |
| Go | `NewT` returning `T`/`*T` → `T.<init>` (heuristic) | Strong on structs |
| Rust | `fn new` in impl → `Type::<init>` | Strong on structs |
| TypeScript / JavaScript | `constructor` method → class `.<init>` | TS strong; JS weak (params may be untyped; graph param types / copy inference help) |
| Python | `__init__` → `Class.<init>`; harvest `self.x` fields | Annotations when present |
| C | No language ctors; struct fields + typed params required | Strong on structs; `file_stem::name` FQN; normalized `#include` Import graph |

**Java extract honesty (java-extract-gaps + java-grammar-remainder + remainder gates):**

- Annotation types are `:Annotation` nodes (not `:Interface`). Usages emit `AnnotatedWith`; no classpath/FQN resolution beyond imports/package best-effort.
- Records are `Class` with `metadata.is_record`; compact ctors and `<clinit>` / `<initblock>N` are CFG entry points.
- Annotation elements are Functions with `is_annotation_element`; interface `constant_declaration` becomes fields.
- Generics/`throws` are symbol metadata (`type_params`, `throws`); not TypeParameter nodes. Structured `find` JSON can surface allowlisted properties where indexed (`type_params`, `throws`, `is_lambda`, `is_external_stub`, …).
- Lambdas are synthetic Functions (`$lambda$N`, `is_lambda`); direct CFG `$lambda$N` lookup is file-global (prefer enclosing-method CFG).
- Anonymous classes use synthetic `Outer.$AnonymousN` owners.
- Expression refs: field reads → `References`; array `new` → `Instantiates`; `.class` → `References`. No full points-to.
- Type-use annotations (`annotated_type`) and declaration-site parameter annotations attach to the **owning method/constructor** (parameter encodings are not graph nodes). Field type-use attaches to the field symbol.
- Unresolved Instantiates / DependsOn / Uses / AnnotatedWith / References / Calls targets become deduplicated **external stub** nodes (`is_external_stub`, file `<external>`) so graph edges survive in `relations`; stubs are placeholders, not a JDK model.
- Pattern-matching (`record_pattern` / `type_pattern`) not first-class symbols.
- No full reflection / retention-policy analysis.
- Langfeature gates: `cargo test --test java_langfeatures` (fixture `tests/fixtures/java/langfeatures`).

**Rust extract honesty (rust-extraction-depth):**

- `use` / `mod` → `Import` / `Module` symbols; no `cargo` workspace path resolution beyond `crate::` text.
- `impl Trait for Type` → `Implements`; trait items as `Interface` symbols with `Trait::method` members.
- `#[derive(...)]` and attribute paths → `AnnotatedWith` (proc-macro bodies not expanded).
- Module-prefix FQNs from `src/` path (`services::order::Foo::bar`); not full `rustc` name resolution.
- `struct { ... }`, `Type::new()`, `Type::default()` → `Instantiates`; field reads → `References`.
- `dyn Trait`, macros, and opaque callees → `metadata.unresolved` on `Calls`.
- Langfeature gates: `cargo test --test rust_langfeatures` (fixture `rgctl-tests/ecommerce-rust`).

**C extract honesty (c-extraction-depth):**

- File-scoped FQN: `{file_stem}::{symbol}` on functions and structs; `foo.h` / `foo.c` pairs may share the same qualified name — filter by `file_path`.
- `#include` → normalized `Import` symbols (`metadata.kind: "include"`); path strings only, no filesystem resolution.
- Function-pointer and macro-generated calls → `metadata.unresolved` on `Calls` when callee cannot be resolved statically; `#define` indexing deferred.
- Langfeature gates: `cargo test --test c_langfeatures` (fixture `rgctl-tests/ecommerce-c`, CALLS ≥ 40).

---

## 3. Repository layout

```
languages.toml                          # Metadata source of truth (handler, extensions, kinds)
crates/
  rgctl-plugin-api/                  # LanguagePlugin trait, Symbol, Relation, call_extraction
  rgctl-lang-runtime/                # Generic Tier 2 TreeSitterLanguagePlugin
  rgctl-lang-{id}/                   # One crate per language (YOU ADD THIS)
    Cargo.toml
    src/
      lib.rs                            # pub fn register(registry: &mut LanguageRegistry)
      plugin.rs                         # Tier 1: custom LanguagePlugin impl
      config.rs                         # Tier 2 only: static LanguageConfig
  rgctl-analysis/
    src/
      language_profile.rs               # CFG/taint gating registry
      cfg_builder.rs                    # Per-language CFG visitors
      def_use.rs                        # Per-language def/use AST cases
      field_write.rs                    # Mutation index (Layer F)
      field_write_locals.rs             # Per-language local/param type recovery (F5)
      taint_rules.rs                    # Declarative packs + apply engine
      rules/taint/*.yaml                # Per-language source/sink/sanitizer packs
      taint.rs                          # Forward taint analysis (uses rule engine)
  rgctl-languages/                   # Wire register() into default binary
tests/
  {lang}_cfg_analysis.rs                # Fixture CFG tests
  {lang}_taint.rs                     # Taint + calls integration
  dashboard_{fixture}.rs                # discover --with-cfg --with-security --with-taint dashboard gate
```

### Crate naming rules

| Language | Crate name | Package name on crates.io path |
|----------|------------|--------------------------------|
| Go | `rgctl-lang-go` | `rgctl-lang-go` |
| Java | `rgctl-lang-java` | `rgctl-lang-java` |
| TypeScript | `rgctl-lang-typescript` | hyphens, not underscores |

- **Directory:** `crates/rgctl-lang-{id}/`
- **`language_id()`:** lowercase, no spaces (`"go"`, `"csharp"`, `"javascript"`)
- **Tree-sitter dep:** `tree-sitter-{grammar}` (version pin in crate `Cargo.toml`)

---

## 4. Step-by-step: add a new Tier 1 language

Use **Kotlin → Tier 1** or **C# → Tier 1** as a mental template; use **Java** / **Go** as code references.

### Step 1 — Scaffold the plugin crate

1. Copy an existing custom plugin crate (e.g. `rgctl-lang-go` or `rgctl-lang-java`).
2. Rename to `crates/rgctl-lang-{id}/`.
3. Update `Cargo.toml`:

```toml
[package]
name = "rgctl-lang-{id}"
description = "rgctl language plugin: {id}"

[dependencies]
rgctl-plugin-api = { workspace = true }
rgctl-registry = { workspace = true }
rgctl-plugin-helpers = { workspace = true }
tree-sitter = { workspace = true }
tree-sitter-{grammar} = "0.xx"
serde_json = "1"
```

4. Implement `lib.rs`:

```rust
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(MyPlugin::new().expect("init MyPlugin")));
}
```

5. Add `{id}-ast-coverage.json` (every named grammar kind → `Symbol` / `Relation` / `CfgStatement` / `AstSkeleton` / `Literal` / intentional `Skip`) plus `src/ast_coverage.rs` with `*_ast_coverage_manifest_matches_grammar` (see `rgctl-lang-ruby` / `rgctl-lang-java`). Update the JSON when bumping the grammar pin. Register the language in `rgctl-ast-coverage::bundled_specs` so `cargo check -p rgctl-languages` warns on drift (`RGCTL_AST_COVERAGE_STRICT=1` fails the build).

6. Add to **workspace root** `Cargo.toml`:
   - `members` list
   - `[workspace.dependencies] rgctl-lang-{id} = { path = "...", version = "0.1.0" }`
7. Register in `crates/rgctl-languages/src/lib.rs`.

### Step 2 — `languages.toml`

Add a `[languages.{id}]` section:

```toml
[languages.{id}]
handler = "custom"
plugin = "MyPlugin"
module = "crate::languages::builtin::{id}"   # legacy doc path; crate is standalone
crate = "tree-sitter-{grammar}"
extensions = ["ext"]
aliases = ["alias"]
function_kinds = ["function_declaration"]      # must match tree-sitter
class_kinds = ["class_declaration"]
import_kinds = ["import_declaration"]
enable_complexity = true
enable_type_inference = false                  # true only if plugin infers param types
```

Run `scripts/generate_lang_configs.py` if you maintain Tier 2 `config.rs` files in parallel (not needed for pure custom Tier 1).

### Step 3 — Implement `LanguagePlugin`

Required methods — see `crates/rgctl-plugin-api/src/lib.rs`:

| Method | Purpose |
|--------|---------|
| `language_id()` | Canonical id string |
| `file_extensions()` | `&["go"]`, `&["java"]`, etc. |
| `grammar()` | `Some(tree_sitter_*::LANGUAGE.into())` |
| `extract_symbols()` | Walk AST; emit `Symbol` list |
| `extract_relations()` | Emit `Relation` with `RelationType::Calls` (minimum) |
| `calculate_complexity()` | Optional but expected for Tier 1 |

**Calls extraction:** prefer shared helper:

```rust
use rgctl_plugin_api::{walk_calls, GO_CALL_KINDS}; // or define LANG_CALL_KINDS

walk_calls(tree.root_node(), source, file_path, symbols, CALL_KINDS, "mylang", &mut relations);
```

Add language-specific call node kinds to `call_extraction.rs` if needed (e.g. `method_invocation` for Java uses a custom walker today).

**Reference implementations:**

| Feature | Look at |
|---------|---------|
| Calls + inheritance | `crates/rgctl-lang-java/src/plugin.rs` |
| Structs + methods | `crates/rgctl-lang-go/src/plugin.rs` |
| Classes + type inference | `crates/rgctl-lang-python/src/plugin.rs` |
| Traits + functions | `crates/rgctl-lang-rust/src/plugin.rs` |

### Step 4 — Wire the analysis profile

Edit `crates/rgctl-analysis/src/language_profile.rs`:

```rust
LanguageAnalysisProfile {
    id: "mylang",
    aliases: &["ml"],
    extensions: &["ml"],
    function_kinds: &["function_declaration"],
    cfg_enabled: true,
    taint_enabled: true,
},
```

Add grammar loader in `grammar_for()`:

```rust
"mylang" => Ok(tree_sitter_mylang::LANGUAGE.into()),
```

Add `tree-sitter-mylang` to `crates/rgctl-analysis/Cargo.toml`.

Export new helpers from `crates/rgctl-analysis/src/lib.rs` if public API additions are needed.

### Step 5 — CFG builder

In `cfg_builder.rs`:

1. Confirm `build_cfg_for_function` parses via `language_profile::parse_source`.
2. Add `visit_*` handlers for language-specific statement node kinds.
3. Add `is_block_like()` kinds if the grammar uses nonstandard block nodes (Go uses `statement_list`).
4. Add unit tests `test_{lang}_if_cfg`, `test_{lang}_loop_has_cycle`, plus switch/try if applicable.

**Tip:** Dump the AST with a small `tree_sitter` script or `cfg_builder` test when mapping kinds — do not guess field names (`body` vs `consequence`).

### Step 6 — Def-use and PDG

In `def_use.rs`, add match arms for the language’s assignment/declaration node kinds (see Go `short_var_declaration`, `range_clause`).

PDG construction is shared; no separate file unless control-dependency edge cases need work.

### Step 7 — Taint patterns

Add a declarative pack at `crates/rgctl-analysis/rules/taint/{id}.yaml` (schema_version 1):

1. List `languages: [{id}]` and ordered `rules` with `role` (`source` / `sink` / `sanitizer`), `kind`, and `match` (`substring` / `any_substring` / `all_substrings` / `regex` / `all_of`).
2. Register the YAML in `BUNDLED_PACKS` inside `taint_rules.rs` (`include_str!`).
3. Cover at minimum: HTTP/input sources, SQL/shell sinks, common sanitizers for that ecosystem.

Do **not** add `detect_{id}_patterns()` in Rust. Keep patterns **statement-text** based for v1; type-aware taint remains optional (`with_type_inference`). Project overlays: `.rgctl/taint-rules.d/` or `--taint-rules PATH`.

### Step 8 — Tests

Full matrix (where files go, what to assert, commands, PR checklist): **[language-testing-guide.md](language-testing-guide.md)**.

Minimum artifacts:

```
crates/rgctl-lang-{id}/                 # unit: symbols, Calls, AST coverage
tests/fixtures/{id}/langfeatures/       # small snippets
tests/{id}_langfeatures.rs              # GQL / structured-query gates
tests/{id}_cfg_analysis.rs              # CFG after discover --with-cfg
tests/{id}_taint.rs                     # taint pack + source→sink
rgctl-tests/ecommerce-{id}/correctness/ # expected-facts.json + graph_correctness
tests/dashboard_ecommerce_{id}.rs       # dashboard harness (release)
```

Also register the language in `stable_key_invariance_qe` and plan Gate B once you have a ~10k-file corpus. Reuse `tests/dashboard_harness.rs` helpers (`run_discover_all`, `assert_dashboard_bundle_all_analysis`).

### Step 9 — Manual validation

```bash
cargo build --release
./scripts/build-dashboard.sh && cargo build --release   # if dashboard dist changed

rgctl discover --with-cfg --with-security --with-taint -r /path/to/fixture-repo -l {id} -v
rgctl serve -r /path/to/fixture-repo --host 127.0.0.1 --port 8080
# Open http://127.0.0.1:8080 — check Graph, CFG, Dataflow, Taint, Blast Radius tabs
```

Confirm `manifest.json` shows `calls_count > 0` and `cfg_index.json` has `"available": true`.

---

## 5. Promoting Tier 2 → Tier 1

Many languages exist as **generic tree-sitter** plugins (`TreeSitterLanguagePlugin::from_config`). To promote:

1. Replace generic plugin with **custom** `plugin.rs` (copy from Go/Java).
2. Change `languages.toml` `handler` from `"tree_sitter"` to `"custom"`.
3. Implement `extract_relations` (at least `Calls`).
4. Complete [Layers B–F](#2-capability-checklist--fully-supported) checklist (**including Layer F**).
5. Keep `config.rs` only if scripts still generate it; otherwise delete to avoid dual sources of truth.

**Do not** add CFG support only in `cfg_builder.rs` without a `language_profile` entry — `discover` will skip the language.

---

## 6. What not to do

| Anti-pattern | Why |
|--------------|-----|
| Parse language X in `rgctl-graph` or `discover_impl.rs` | Belongs in `rgctl-lang-*` + `rgctl-analysis` |
| Hardcode `.ext` lists in CLI | Use `language_profile` / `languages.toml` |
| Tier 1 plugin without `Calls` relations | Blast radius and call graph stay empty |
| Tier 1 without Layer F (fields / ctors / mutation golden) | `cpg mutations` is Java-only quality; **not** Tier 1 |
| CFG enabled without tests | Dashboard shows blocks but regressions go unnoticed |
| Duplicate grammar only in plugin crate | `rgctl-analysis` needs its own `tree-sitter-*` dep for CFG |
| Skip `rgctl-languages` registration | Language won’t ship in default `rgctl` binary |
| Full type checker inside the plugin | Out of scope — bound resolution only (decl / param / field) |

---

## 7. PR submission checklist

Copy into your PR description. For the **testing** half (paths, asserts, commands), also use [language-testing-guide §7](language-testing-guide.md#7-checklist-copy-into-pr).

- [ ] `crates/rgctl-lang-{id}/` with `LanguagePlugin` + tests
- [ ] `languages.toml` updated (`handler = "custom"`)
- [ ] Workspace `Cargo.toml` + bundle registration
- [ ] `language_profile.rs` entry (`cfg_enabled`, `taint_enabled`, grammar)
- [ ] `cfg_builder.rs` + tests for control-flow constructs
- [ ] `def_use.rs` cases for declarations/assignments **including field-access LHS**
- [ ] **Layer F:** `fields[]`, `is_constructor` + `.<init>`/`::<init>`, typed params
- [ ] **Layer F:** `merge_local_types` arm in `field_write_locals.rs` (or documented N/A)
- [ ] **Layer F:** golden `{id}_cfg_captures_field_write_and_query` in `field_write` tests
- [ ] `taint` pack `rules/taint/{id}.yaml` + `BUNDLED_PACKS` entry (not `detect_{id}_patterns`)
- [ ] `extract_relations` emits `Calls` (and inheritance if applicable)
- [ ] `{id}-ast-coverage.json` + `ast_coverage` test (`*_ast_coverage_manifest_matches_grammar`)
- [ ] `tests/{id}_langfeatures.rs` · `{id}_cfg_analysis.rs` · `{id}_taint.rs` + `[[test]]` entries
- [ ] `rgctl-tests/ecommerce-{id}/correctness/expected-facts.json` + `graph_correctness` `PROJECTS` entry
- [ ] `dashboard_ecommerce_{id}` release gate (or documented fixture path)
- [ ] Stable-key QE: language in `stable_key_invariance_qe` (no line-coupled `qualified_name`)
- [ ] `discover --with-cfg --with-security --with-taint` smoke on fixture repo documented in test
- [ ] No new CDN / online-only dashboard dependencies

---

## 8. Current parity snapshot (2026-07)

| Language | Tier | Calls | CFG | Taint | Dashboard gate | Layer F (CPG mutations) |
|----------|------|-------|-----|-------|----------------|-------------------------|
| Java | 1 custom | ✅ + Extends/Implements/AnnotatedWith/Permits/Instantiates | ✅ (+ compact ctor, `<clinit>`) | ✅ rich | gbuilder golden | ✅ F1–F6 |
| Go | 1 custom | ✅ | ✅ deep | ✅ rich | `dashboard_ecommerce_go` | ✅ F1–F6 |
| C# | 1 custom | ✅ + AnnotatedWith/FQN/Instantiates + call type hints | ✅ | ✅ | `dashboard_ecommerce_csharp`, `csharp_langfeatures` | ✅ F1–F6 |
| C | 1 custom | ✅ FQN + includes | ✅ | ✅ | `dashboard_ecommerce_c` | ✅ F1/F3–F6 (no native ctors) |
| C++ | 1 custom | ✅ + Extends/Instantiates + qualified call hints | ✅ | ✅ | `dashboard_ecommerce_cpp`, `cpp_langfeatures` | ✅ F1–F6 |
| Python | 1 custom | ✅ Import/Extends/AnnotatedWith/FQN/Instantiates; no `sys.path` resolution; `typing.Protocol` → Implements optional | ✅ | ✅ richest | `dashboard_ecommerce_python`, `python_langfeatures` | ✅ F1–F6 |
| Rust | 1 custom | ✅ | ✅ | ✅ rich | `dashboard_ecommerce_rust` | ✅ F1–F6 |
| JS / TS | 1 custom | ✅ Import/Extends/FQN/Instantiates (+ decorators TS); shared `rgctl-plugin-helpers::ecmascript` | ✅ | ✅ rich | `dashboard_ecommerce_javascript`, `javascript_langfeatures`, `typescript_langfeatures` | ✅ F1–F6 (JS weaker types) |
| PHP | 1 custom | ✅ + Uses (traits), Import, attributes, anonymous classes | ✅ | ✅ + `$_FILES`, `filter_input`, `prepare` | `dashboard_ecommerce_php` | ✅ F1–F6 |
| Ruby | 1 custom | ✅ Import, mixin Extends/Uses, Instantiates, unresolved dynamic calls | ✅ rescue/begin | ✅ Rack-ish patterns | `dashboard_ecommerce_ruby`, `ruby_langfeatures` | ✅ F1–F6 |
| Puppet | 1 custom | ✅ IncludesClass/InheritsClass/RequiresResource/DependsOnModule/Calls | ✅ if/unless/case | ✅ lookup/exec patterns | pending `dashboard_ecommerce_puppet` | ✅ F1/F3; F2 N/A; **F6 waived** (honesty) |
| Kotlin | 1 custom | ✅ Calls/Extends/Implements; `tree-sitter-kotlin-ng` | ✅ if/when/loops | ✅ JVM patterns | ✅ `dashboard_ecommerce_kotlin`, langfeatures, verify script | ✅ Layer F (`navigation_expression`) |
| Groovy | 1 custom | ✅ best-effort Calls; dynamic honesty | ✅ if/loops (Java-shaped AST) | ✅ script sinks | ✅ `dashboard_ecommerce_groovy`, langfeatures, verify script | ✅ Layer F (`field_access`) |

Layer F golden coverage lives in `crates/rgctl-analysis/src/field_write.rs` (`*_cfg_captures_field_write_and_query`). Update this table when promoting a language or when F tests regress.

---

## 9. Getting help

- Open a **[Language Support Request](.github/ISSUE_TEMPLATE/language_request.md)** issue before large work.
- Point questions at `rgctl-plugin-api::LanguagePlugin` and the reference crates above.
- For dashboard contract details, see `tests/dashboard_harness.rs` and `docs/dashboard-design.md`.
