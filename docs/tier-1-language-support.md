# Tier 1 language support — contributor requirements

This document defines what **fully supported** means for a programming language in rBuilder, and the concrete steps to add or promote a language to that level.

**Audience:** contributors adding a new Tier 1 language or bringing Rust / Python / C / C# / etc. to parity with Java and Go.

**Related docs:** [Code_structure.md](Code_structure.md) (crate layout), [dashboard-design.md](dashboard-design.md) (dashboard bundle), [cli-getting-started.md](cli-getting-started.md) (discover / serve workflow).

---

## 1. Language tiers (quick reference)

rBuilder uses a **hybrid tiering** model:

| Tier | Handler | Crate pattern | Indexing | CFG / PDG / taint | Call graph |
|------|---------|---------------|----------|-------------------|------------|
| **Tier 1** | `custom` — dedicated `LanguagePlugin` | `rbuilder-lang-{id}/` | Rich symbols + relations | **Required** for “fully supported” | **Required** (`Calls` at minimum) |
| **Tier 2** | Generic tree-sitter | `rbuilder-lang-{id}/` + `config.rs` | Kinds from `LanguageConfig` | Optional | Usually none |
| **Tier 3** | Regex | `rbuilder-lang-{id}/` + regex patterns | Pattern-based symbols | No | No |

**Tier 1 custom plugins today (indexing):** Rust, Python, TypeScript, JavaScript, Go, Java — see `languages.toml` (`handler = "custom"`).

**Fully supported (analysis parity)** is a **subset** of Tier 1: languages that pass the [capability checklist](#2-capability-checklist--fully-supported) below. Today that bar is met most completely by **Java** and **Go**; Rust and Python have CFG + taint but shallower taint / fewer golden tests.

---

## 2. Capability checklist — “fully supported”

A language is **fully supported** when all rows are ✅ and backed by automated tests.

### Layer A — Graph plugin (indexing)

| # | Requirement | Where |
|---|-------------|--------|
| A1 | Custom `LanguagePlugin` (not generic Tier 2 only) | `crates/rbuilder-lang-{id}/src/plugin.rs` |
| A2 | Tree-sitter grammar wired (`grammar()` + parse) | Plugin + `tree-sitter-{id}` crate dep |
| A3 | **Symbols:** functions/methods with name, location, signature, parameters | `extract_symbols()` |
| A4 | **Symbols:** types (class/struct/interface/enum as appropriate) | `extract_symbols()` |
| A5 | **Relations:** `Calls` between functions | `extract_relations()` — use `rbuilder_plugin_api::walk_calls` or language-specific walker |
| A6 | **Relations (OOP):** `Extends` / `Implements` where the language has inheritance | Java-style plugins; optional for Go/Rust |
| A7 | Cyclomatic / cognitive complexity for functions | `calculate_complexity()` |
| A8 | Entry in `languages.toml` with `handler = "custom"` | Repo root |
| A9 | Registered in `rbuilder-languages` | `crates/rbuilder-languages/src/lib.rs` |

### Layer B — Analysis profile (CFG pipeline)

| # | Requirement | Where |
|---|-------------|--------|
| B1 | `LanguageAnalysisProfile` with `cfg_enabled: true` | `crates/rbuilder-analysis/src/language_profile.rs` |
| B2 | `tree-sitter-{id}` dependency on **`rbuilder-analysis`** | `crates/rbuilder-analysis/Cargo.toml` |
| B3 | `function_kinds` match tree-sitter node kinds used in CFG lookup | `language_profile.rs` + `cfg_builder.rs` |
| B4 | CFG builders for control flow: `if`, loops, `return`, `break`/`continue` | `crates/rbuilder-analysis/src/cfg_builder.rs` |
| B5 | CFG builders for language-specific control flow (e.g. `switch`, `select`, `match`, `try`) | `cfg_builder.rs` |
| B6 | Definition-use extraction for assignments / declarations | `crates/rbuilder-analysis/src/def_use.rs` |
| B7 | PDG builds from CFG + source (automatic once CFG + def/use work) | `crates/rbuilder-analysis/src/pdg.rs` |
| B8 | `discover --cfg` / `discover --all` includes `.ext` files | Automatic via `cfg_language_id_from_path` in `discover_impl.rs` |

### Layer C — Security & interprocedural

| # | Requirement | Where |
|---|-------------|--------|
| C1 | `taint_enabled: true` on profile | `language_profile.rs` |
| C2 | `detect_{lang}_patterns()` — sources, sinks, sanitizers | `crates/rbuilder-analysis/src/taint.rs` |
| C3 | Taint routed via `canonical_language_id()` | `TaintAnalyzer::detect_patterns` |
| C4 | Interprocedural CFG uses correct language (not wrong grammar) | `interprocedural_cfg.rs` → `language_id_from_path` |
| C5 | Slice CLI resolves language from file path | `src/cli/context.rs` → `language_from_path` |

### Layer D — Dashboard & UX

| # | Requirement | Where |
|---|-------------|--------|
| D1 | `discover --all` writes `.rbuilder/dashboard/` with CFG index populated | `cfg_index.json` `available: true` |
| D2 | Per-function CFG + dominance render in dashboard | Manual smoke or Playwright |
| D3 | Dataflow / taint tabs show data when flows exist | PDG + taint archive export |
| D4 | Blast radius lists functions with non-zero scores when call graph exists | `manifest.json` `calls_count` > 0 |

### Layer E — Tests (required for merge)

| # | Requirement | Where |
|---|-------------|--------|
| E1 | Plugin unit tests: symbols + at least one `Calls` relation | `crates/rbuilder-lang-{id}/src/plugin.rs` `#[cfg(test)]` |
| E2 | CFG unit tests: branching function + loop cycle | `crates/rbuilder-analysis/src/cfg_builder.rs` tests |
| E3 | Taint unit/integration test: at least one source→sink path | `tests/phase13_taint.rs` or `tests/{lang}_phase13.rs` |
| E4 | Fixture integration test on a small real repo | e.g. `tests/go_cfg_analysis.rs` |
| E5 | Dashboard golden gate: `discover --all` + bundle assertions | e.g. `tests/dashboard_ecommerce_go.rs` + `tests/dashboard_harness.rs` |
| E6 | `cargo test` + `cargo clippy` clean for touched crates | CI |

---

## 3. Repository layout

```
languages.toml                          # Metadata source of truth (handler, extensions, kinds)
crates/
  rbuilder-plugin-api/                  # LanguagePlugin trait, Symbol, Relation, call_extraction
  rbuilder-lang-runtime/                # Generic Tier 2 TreeSitterLanguagePlugin
  rbuilder-lang-{id}/                   # One crate per language (YOU ADD THIS)
    Cargo.toml
    src/
      lib.rs                            # pub fn register(registry: &mut LanguageRegistry)
      plugin.rs                         # Tier 1: custom LanguagePlugin impl
      config.rs                         # Tier 2 only: static LanguageConfig
  rbuilder-analysis/
    src/
      language_profile.rs               # CFG/taint gating registry
      cfg_builder.rs                    # Per-language CFG visitors
      def_use.rs                        # Per-language def/use AST cases
      taint.rs                          # Per-language taint patterns
  rbuilder-languages/                   # Wire register() into default binary
tests/
  {lang}_cfg_analysis.rs                # Fixture CFG tests
  {lang}_phase13.rs                     # Taint + calls integration
  dashboard_{fixture}.rs                # discover --all dashboard gate
```

### Crate naming rules

| Language | Crate name | Package name on crates.io path |
|----------|------------|--------------------------------|
| Go | `rbuilder-lang-go` | `rbuilder-lang-go` |
| Java | `rbuilder-lang-java` | `rbuilder-lang-java` |
| TypeScript | `rbuilder-lang-typescript` | hyphens, not underscores |

- **Directory:** `crates/rbuilder-lang-{id}/`
- **`language_id()`:** lowercase, no spaces (`"go"`, `"csharp"`, `"javascript"`)
- **Tree-sitter dep:** `tree-sitter-{grammar}` (version pin in crate `Cargo.toml`)

---

## 4. Step-by-step: add a new Tier 1 language

Use **Kotlin → Tier 1** or **C# → Tier 1** as a mental template; use **Java** / **Go** as code references.

### Step 1 — Scaffold the plugin crate

1. Copy an existing custom plugin crate (e.g. `rbuilder-lang-go` or `rbuilder-lang-java`).
2. Rename to `crates/rbuilder-lang-{id}/`.
3. Update `Cargo.toml`:

```toml
[package]
name = "rbuilder-lang-{id}"
description = "rBuilder language plugin: {id}"

[dependencies]
rbuilder-plugin-api = { workspace = true }
rbuilder-registry = { workspace = true }
rbuilder-plugin-helpers = { workspace = true }
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

5. Add to **workspace root** `Cargo.toml`:
   - `members` list
   - `[workspace.dependencies] rbuilder-lang-{id} = { path = "...", version = "0.1.0" }`
6. Register in `crates/rbuilder-languages/src/lib.rs`.

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

Required methods — see `crates/rbuilder-plugin-api/src/lib.rs`:

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
use rbuilder_plugin_api::{walk_calls, GO_CALL_KINDS}; // or define LANG_CALL_KINDS

walk_calls(tree.root_node(), source, file_path, symbols, CALL_KINDS, "mylang", &mut relations);
```

Add language-specific call node kinds to `call_extraction.rs` if needed (e.g. `method_invocation` for Java uses a custom walker today).

**Reference implementations:**

| Feature | Look at |
|---------|---------|
| Calls + inheritance | `crates/rbuilder-lang-java/src/plugin.rs` |
| Structs + methods | `crates/rbuilder-lang-go/src/plugin.rs` |
| Classes + type inference | `crates/rbuilder-lang-python/src/plugin.rs` |
| Traits + functions | `crates/rbuilder-lang-rust/src/plugin.rs` |

### Step 4 — Wire the analysis profile

Edit `crates/rbuilder-analysis/src/language_profile.rs`:

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

Add `tree-sitter-mylang` to `crates/rbuilder-analysis/Cargo.toml`.

Export new helpers from `crates/rbuilder-analysis/src/lib.rs` if public API additions are needed.

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

In `taint.rs`:

1. Add `detect_mylang_patterns(&mut self)`.
2. Register in `detect_patterns` via `canonical_language_id` match arm.
3. Cover at minimum: HTTP/input sources, SQL/shell sinks, common sanitizers for that ecosystem.

Keep patterns **statement-text** based for now (consistent with existing code); type-aware taint is optional (`with_type_inference`).

### Step 8 — Tests

Minimum test matrix:

```
crates/rbuilder-lang-{id}/src/plugin.rs   # unit: symbols, calls
crates/rbuilder-analysis/src/cfg_builder.rs # unit: CFG shape
crates/rbuilder-analysis/src/language_profile.rs # unit: path → id
tests/{id}_cfg_analysis.rs                # integration: real fixture file
tests/{id}_phase13.rs                     # taint + calls smoke
tests/dashboard_{fixture}.rs              # discover --all + manifest/cfg_index
```

Reuse `tests/dashboard_harness.rs` helpers (`run_discover_all`, `assert_dashboard_bundle_all_analysis`).

Provide a **small fixture repo** under `rbuilder-tests/` or document `RBUILDER_{LANG}_REPO` env override.

### Step 9 — Manual validation

```bash
cargo build --release
./scripts/build-dashboard.sh && cargo build --release   # if dashboard dist changed

rbuilder discover --all -r /path/to/fixture-repo -l {id} -v
rbuilder serve -r /path/to/fixture-repo --host 127.0.0.1 --port 8080
# Open http://127.0.0.1:8080 — check Graph, CFG, Dataflow, Taint, Blast Radius tabs
```

Confirm `manifest.json` shows `calls_count > 0` and `cfg_index.json` has `"available": true`.

---

## 5. Promoting Tier 2 → Tier 1

Many languages exist as **generic tree-sitter** plugins (`TreeSitterLanguagePlugin::from_config`). To promote:

1. Replace generic plugin with **custom** `plugin.rs` (copy from Go/Java).
2. Change `languages.toml` `handler` from `"tree_sitter"` to `"custom"`.
3. Implement `extract_relations` (at least `Calls`).
4. Complete [Layer B–E](#layer-b--analysis-profile-cfg-pipeline) checklist.
5. Keep `config.rs` only if scripts still generate it; otherwise delete to avoid dual sources of truth.

**Do not** add CFG support only in `cfg_builder.rs` without a `language_profile` entry — `discover` will skip the language.

---

## 6. What not to do

| Anti-pattern | Why |
|--------------|-----|
| Parse language X in `rbuilder-graph` or `discover_impl.rs` | Belongs in `rbuilder-lang-*` + `rbuilder-analysis` |
| Hardcode `.ext` lists in CLI | Use `language_profile` / `languages.toml` |
| Tier 1 plugin without `Calls` relations | Blast radius and call graph stay empty |
| CFG enabled without tests | Dashboard shows blocks but regressions go unnoticed |
| Duplicate grammar only in plugin crate | `rbuilder-analysis` needs its own `tree-sitter-*` dep for CFG |
| Skip `rbuilder-languages` registration | Language won’t ship in default `rbuilder` binary |

---

## 7. PR submission checklist

Copy into your PR description:

- [ ] `crates/rbuilder-lang-{id}/` with `LanguagePlugin` + tests
- [ ] `languages.toml` updated (`handler = "custom"`)
- [ ] Workspace `Cargo.toml` + bundle registration
- [ ] `language_profile.rs` entry (`cfg_enabled`, `taint_enabled`, grammar)
- [ ] `cfg_builder.rs` + tests for control-flow constructs
- [ ] `def_use.rs` cases for declarations/assignments
- [ ] `taint.rs` `detect_{id}_patterns`
- [ ] `extract_relations` emits `Calls` (and inheritance if applicable)
- [ ] Integration test + dashboard gate (or documented fixture path)
- [ ] `discover --all` smoke on fixture repo documented in test
- [ ] No new CDN / online-only dashboard dependencies

---

## 8. Current parity snapshot (2026-07)

| Language | Tier | Calls | CFG | Taint | Dashboard gate |
|----------|------|-------|-----|-------|----------------|
| Java | 1 custom | ✅ + Extends/Implements | ✅ | ✅ rich | gbuilder golden |
| Go | 1 custom | ✅ | ✅ deep | ✅ rich | `dashboard_ecommerce_go` |
| Python | 1 custom | ✅ | ✅ | ✅ richest | — |
| Rust | 1 custom | ✅ | ✅ | ⚠️ basic | — |
| JS / TS | 1 custom | ❌ stub | ❌ | ✅ (JS patterns) | — |
| C / C++ | 2 generic | ❌ | ❌ | ❌ | — |
| C# | regex / partial | ❌ | ❌ | ❌ | — |

Use this table to prioritize gaps; updating it when a language completes Layer E is encouraged.

---

## 9. Getting help

- Open a **[Language Support Request](.github/ISSUE_TEMPLATE/language_request.md)** issue before large work.
- Point questions at `rbuilder-plugin-api::LanguagePlugin` and the reference crates above.
- For dashboard contract details, see `tests/dashboard_harness.rs` and `docs/dashboard-design.md`.
