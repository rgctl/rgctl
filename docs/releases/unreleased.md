# Unreleased (post v0.4.24)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Highlights

### 98%–100% (`A+`) Tier-1 AST & Deep-Analysis Coverage Across All 24 Languages (`rgctl-ast-coverage`, `rgctl-analysis`, `rgctl-lang-*`)

Replaced permissive `"Skip"`-based AST coverage checks with a **5-pillar compile-time and CI enforcement architecture** and upgraded **all 24 Tier-1 tree-sitter languages** from `1.3%–75.9%` (`C`/`B`/`A`) to **$\ge 98.0\%$ (`A+`, with 21 of 24 at `100.0%`)**, **`0` raw `"Skip"` entries**, universal `mask.is_passive()` dispatch gating, full 20-language Layer A–F parity, and unconditional `tracker.assert_all_exercised()` depth test suites:

1. **Pillar 1 — Structured Passive Classification & Complete Removal of Raw `"Skip"` (`crates/rgctl-ast-coverage`, `crates/rgctl-plugin-api`)**:
   - Replaced raw `"Skip"` in `HandlerMask::from_coverage_label` and `ALLOWED_HANDLERS` with explicit passive categories: `SkipWrapper` (structural syntax containers transparent to tree walking), `SkipTypeToken` (leaf primitive/modifier/keyword tokens consumed by parent handlers), and `Unimplemented` (tracked backlog requiring a non-empty reason in `skip_justifications`). Raw `"Skip"` is now rejected at compile/test time across all 24 manifests.
   - Added `is_semantic_node_kind(kind)` denylist enforcement (`*_declaration`, `*_definition`, `*_statement`, `*_expression`, `*_invocation`, `*_call*`, `*_creation*`, `*_clause`, `*_pattern`, `*_directive`, `lambda*`, `closure*`) so semantic AST kinds cannot be silently marked passive without an explicit `skip_justifications` entry.
   - Removed the `(often Skip)` suggestion from `check_manifest` error diagnostics when a grammar pin introduces new named AST kinds.
2. **Pillar 2 — Quantitative Semantic AST Coverage Ratio & Monotonic Ratchet (`crates/rgctl-ast-coverage`)**:
   - Added `CoverageGrade` (`A+` $\ge 98\%$, `A` $\ge 75\%$, `B` $\ge 50\%$, `C` $< 50\%$), `SemanticCoverageStats`, and `compute_semantic_coverage` over the semantic denominator (`handled + unimplemented`, excluding `SkipWrapper` and `SkipTypeToken`).
   - Locked `target_grade: CoverageGrade::APlus` and monotonic `min_handled_kinds` ratchet floors for all 24 Tier-1 languages in `bundled_specs()` (`crates/rgctl-ast-coverage/src/lib.rs`).
3. **Pillar 3 — Universal `mask.is_passive()` Dispatch Gating & Three-Way Manifest Sync (`crates/rgctl-lang-*`, `crates/rgctl-analysis`, `crates/rgctl-ast-coverage`)**:
   - Gated all 24 Tier-1 `crates/rgctl-lang-*/src/single_pass.rs` walkers on `if mask.is_passive() { rgctl_ast_coverage::record_skip_hit(kind_id); ... continue; }` so passive AST kinds can never execute hidden extraction logic.
   - Exposed `cfg_handled_kinds(language_id)` in `crates/rgctl-analysis/src/cfg_builder.rs` and `def_use_handled_kinds(language_id)` in `crates/rgctl-analysis/src/def_use.rs`, and enforced 3-way agreement (`{id}-ast-coverage.json` $\leftrightarrow$ `plugin.declared_ast_handlers()` $\leftrightarrow$ `cfg_builder` / `def_use`) via `check_plugin_and_analysis_manifest_sync`.
4. **Pillar 4 — 20-Language Cross-Layer Behavioral Parity (`crates/rgctl-analysis/tests/tier1_parity_gate.rs`, `field_write_locals.rs`)**:
   - Implemented Layer F (`crates/rgctl-analysis/src/field_write_locals.rs`) local/parameter type recovery visitors (`visit_lua`, `visit_elixir`, `visit_bash`, `visit_scala`, `visit_swift`, `visit_dart`) so all **20 general-purpose Tier-1 languages** now support all 4 deep-analysis layers (`A` Plugin + `B` CFG/PDG + `C` Taint + `F` Field-Write/OOP).
   - Added `crates/rgctl-analysis/rules/taint/tier1-extended.yaml` (`swift`, `scala`, `dart`, `elixir`, `lua`, `bash`), sanitizer rules in `groovy.yaml` and `puppet.yaml`, and Spring/JPA/JNDI/SpEL/deserialization rules in `java.yaml`.
   - Added the 6-test cross-layer parity suite in `crates/rgctl-analysis/tests/tier1_parity_gate.rs` verifying manifest ratchets, 3-way handler sync, Layer A symbols/relations, Layer B CFG/PDG, Layer C taint rules, Layer F `field_write_locals`, and corpus unhandled AST hotspot auditing.
5. **Pillar 5 — Unconditional `assert_all_exercised()` Across All 24 Languages & Audit Tooling (`crates/rgctl-lang-*/tests/*_depth_extraction.rs`, `scripts/check-tier1-coverage.py`)**:
   - Upgraded all 12 existing `tests/{lang}_depth_extraction.rs` suites (`java`, `rust`, `python`, `typescript`, `javascript`, `go`, `csharp`, `cpp`, `c`, `php`, `ruby`, `kotlin`) and created 12 new `tests/{lang}_depth_extraction.rs` suites (`puppet`, `groovy`, `lua`, `elixir`, `bash`, `scala`, `swift`, `dart`, `erb`, `markdown`, `hcl`, `sql`) so all **24 Tier-1 languages** enforce unconditional `tracker.assert_all_exercised().expect(...)` with zero unhit active plugin kinds.
   - Added [`scripts/check-tier1-coverage.py`](../../scripts/check-tier1-coverage.py) (`--json`, `--language <id>`, `--run-tests`) to audit all **69 languages** across all 5 pillars.

#### Six-Wave Language Upgrade Summary (649+ Promoted Semantic AST Kinds)

- **Wave 1 (`puppet`, `ruby`, `groovy`, `python`)**: Promoted Puppet `search_expression`, `field_expression`, `binary_expression`, `unary_expression`; Ruby `alias`, `conditional`, `in_clause`, `lambda_parameters`, `destructured_left_assignment`, and Ruby 3 pattern-matching nodes; Groovy `lambda_expression`, `record_declaration`, `annotation_type_element_declaration`, `local_variable_declaration`, `field_access`, `array_creation_expression`, `ternary_expression`, `try_with_resources_statement`, `synchronized_statement`, `instanceof_expression`, and JPMS directives; Python `conditional_expression`, comprehension clauses (`for_in_clause`, `if_clause`), `with_clause`, `lambda_parameters`, `default_parameter`, `exec_statement`, `global_statement`, `nonlocal_statement`, and Python 3.10+ `match` patterns.
- **Wave 2 (`javascript`, `typescript`, `kotlin`, `php`)**: Promoted JS/TS `lexical_declaration`, `variable_declaration`, `using_declaration`, `member_expression`, `subscript_expression`, `export_clause`, `namespace_export`, `class_heritage`, `extends_type_clause`, `instantiation_expression`, `accessibility_modifier`, `binary_expression`, `labeled_statement`, and destructuring patterns; Kotlin `lambda_literal`, `anonymous_function`, `callable_reference`, `constructor_delegation_call`, `delegation_specifiers`, `explicit_delegation`, `annotation`, `file_annotation`, `class_modifier`, `function_modifier`, `visibility_modifier`, `navigation_expression`, `multi_variable_declaration`, `lambda_parameters`; PHP `object_creation_expression`, `include_expression`/`require_expression` variants, `member_access_expression`, `nullsafe_member_access_expression`, `scoped_property_access_expression`, `attribute_group`, `shell_command_expression`, `echo_statement`, `else_if_clause`, `conditional_expression`, `match_conditional_expression`.
- **Wave 3 (`go`, `rust`, `c`, `cpp`)**: Promoted Go `short_var_declaration`, `parameter_declaration`, `variadic_parameter_declaration`, `range_clause`, `selector_expression`, `index_expression`, `slice_expression`, `go_statement`, `defer_statement`, `send_statement`, `receive_statement`, `expression_switch_statement`, `type_switch_statement`, `for_clause`, `package_clause`, `type_parameter_declaration`; Rust `struct_expression`, `field_expression`, `index_expression`, `reference_expression`, `await_expression`, `binary_expression`, `closure_parameters`, pattern nodes, `where_clause`, `use_as_clause`; C/C++ `declaration`, `parameter_declaration`, `field_expression`, `pointer_expression`, `subscript_expression`, `update_expression`, `conditional_expression`, `binary_expression`, `labeled_statement`, plus C++ `lambda_expression`, `template_declaration`, `type_parameter_declaration`, `requires_clause`, `requires_expression`, `friend_declaration`, `namespace_alias_definition`, `delete_expression`, `condition_clause`, `init_statement` (`crates/rgctl-lang-cpp/src/depth.rs`).
- **Wave 4 (`csharp`)**: Promoted 88 C# semantic kinds including `file_scoped_namespace_declaration`, `object_creation_expression`, `implicit_object_creation_expression`, `array_creation_expression`, `anonymous_object_creation_expression`, `lambda_expression`, `anonymous_method_expression`, `indexer_declaration`, `operator_declaration`, `conversion_operator_declaration`, `destructor_declaration`, `accessor_declaration`, `attribute`, `local_declaration_statement`, `variable_declaration`, `member_access_expression`, `conditional_access_expression`, `element_access_expression`, `await_expression`, `using_statement`, `lock_statement`, C# pattern nodes, and LINQ query clauses.
- **Wave 5 (`lua`, `elixir`, `bash`, `scala`, `swift`, `dart`)**: Implemented Layer F visitors in `field_write_locals.rs` and promoted all skipped declarations, calls, member/navigation expressions, patterns, and control-flow clauses across all 6 languages to `100.0%` (`A+`).
- **Wave 6 (`erb`, `markdown`, `hcl`, `sql`)**: Classified SQL's 399 `keyword_*` / data-type tokens (`SkipTypeToken`) and 61 syntax wrappers (`SkipWrapper`), and promoted 63 active SQL DDL/DML/query constructs (`column_definition`, `function_declaration`, `create_index`, `create_trigger`, `create_type`, `create_schema`, `alter_table`, `cte`, `join`, `select_expression`, `when_clause`, `window_clause`, etc.), HCL resource/variable/expression nodes (`14/14 = 100.0%`), Markdown (`7/7 = 100.0%`), and ERB (`6/6 = 100.0%`).

#### Before vs. After Tier-1 AST Coverage Matrix (`scripts/check-tier1-coverage.py`)

| Language | Tier Classification | Before (`Handled / Sem`) | Before Cov % (Grade) | After (`Handled / Sem`) | After Cov % (Grade) | Passive (`Wrap / Tok / Unimp`) | Raw `Skip` | Dispatch Gate | Layers | Test Gate |
|---|---|---:|---:|---:|---:|---:|---:|:---:|:---:|:---:|
| **`bash`** | Tier-1 Gold | `7 / 59` | 11.9% (`C`) | **`46 / 46`** | **100.0% (`A+`)** | `11 / 2 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`c`** | Tier-1 Gold | `37 / 125` | 29.6% (`C`) | **`74 / 74`** | **100.0% (`A+`)** | `43 / 8 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`cpp`** | Tier-1 Gold | `58 / 206` | 28.2% (`C`) | **`108 / 108`** | **100.0% (`A+`)** | `69 / 29 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`dart`** | Tier-1 Gold | `19 / 221` | 8.6% (`C`) | **`139 / 139`** | **100.0% (`A+`)** | `61 / 21 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`elixir`** | Tier-1 Gold | `6 / 45` | 13.3% (`C`) | **`22 / 22`** | **100.0% (`A+`)** | `17 / 6 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`erb`** | Tier-1 DSL | `4 / 8` | 50.0% (`B`) | **`6 / 6`** | **100.0% (`A+`)** | `2 / 0 / 0` | 0 | YES | `A---` | `hard_assert` |
| **`go`** | Tier-1 Gold | `42 / 107` | 39.2% (`C`) | **`76 / 76`** | **100.0% (`A+`)** | `17 / 14 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`hcl`** | Tier-1 DSL | `3 / 64` | 4.7% (`C`) | **`14 / 14`** | **100.0% (`A+`)** | `30 / 20 / 0` | 0 | YES | `AB--` | `hard_assert` |
| **`javascript`** | Tier-1 Gold | `54 / 114` | 47.4% (`C`) | **`99 / 99`** | **100.0% (`A+`)** | `15 / 0 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`kotlin`** | Tier-1 Gold | `43 / 114` | 37.7% (`C`) | **`87 / 87`** | **100.0% (`A+`)** | `19 / 8 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`lua`** | Tier-1 Gold | `10 / 47` | 21.3% (`C`) | **`38 / 38`** | **100.0% (`A+`)** | `9 / 0 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`markdown`** | Tier-1 DSL | `6 / 51` | 11.8% (`C`) | **`7 / 7`** | **100.0% (`A+`)** | `17 / 27 / 0` | 0 | YES | `A---` | `hard_assert` |
| **`php`** | Tier-1 Gold | `55 / 157` | 35.0% (`C`) | **`100 / 100`** | **100.0% (`A+`)** | `48 / 9 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`puppet`** | Tier-1 Gold | `44 / 58` | 75.9% (`A`) | **`48 / 48`** | **100.0% (`A+`)** | `7 / 3 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`python`** | Tier-1 Gold | `55 / 123` | 44.7% (`C`) | **`95 / 95`** | **100.0% (`A+`)** | `22 / 6 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`ruby`** | Tier-1 Gold | `87 / 134` | 64.9% (`B`) | **`115 / 115`** | **100.0% (`A+`)** | `17 / 2 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`rust`** | Tier-1 Gold | `56 / 163` | 34.4% (`C`) | **`90 / 90`** | **100.0% (`A+`)** | `44 / 29 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`scala`** | Tier-1 Gold | `19 / 171` | 11.1% (`C`) | **`94 / 94`** | **100.0% (`A+`)** | `55 / 22 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`sql`** | Tier-1 DSL | `7 / 523` | 1.3% (`C`) | **`63 / 63`** | **100.0% (`A+`)** | `61 / 399 / 0` | 0 | YES | `AB--` | `hard_assert` |
| **`swift`** | Tier-1 Gold | `19 / 188` | 10.1% (`C`) | **`88 / 88`** | **100.0% (`A+`)** | `82 / 18 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`typescript`** | Tier-1 Gold | `70 / 177` | 39.6% (`C`) | **`106 / 106`** | **100.0% (`A+`)** | `29 / 42 / 0` | 0 | YES | `ABCF` | `hard_assert` |
| **`csharp`** | Tier-1 Gold | `55 / 215` | 25.6% (`C`) | **`166 / 167`** | **99.4% (`A+`)** | `31 / 17 / 1` | 0 | YES | `ABCF` | `hard_assert` |
| **`groovy`** | Tier-1 Gold | `70 / 150` | 46.7% (`C`) | **`106 / 108`** | **98.2% (`A+`)** | `33 / 9 / 2` | 0 | YES | `ABCF` | `hard_assert` |
| **`java`** | Tier-1 Gold | `99 / 142` | 69.7% (`B`) | **`100 / 102`** | **98.0% (`A+`)** | `31 / 9 / 2` | 0 | YES | `ABCF` | `hard_assert` |

#### Cold Profile Baselines (`tests/cold_profile_gates.rs` & `docs/internal/profile.md`)

- **Gate A (Linux Kernel `example/linux`):** Passed within the **75 s** reference baseline (`c` and `bash` at `100.0%` `A+` with zero hot-path allocation regression).
- **Gate B (Language-Scale Corpora):** `rust` (**25 s**), `llvm-project/clang` C++ (**12 s**), `vscode/src` TypeScript (**17 s**), `node/test` JavaScript (**4 s**; `--with-cfg` **5 s**), `home-assistant` Python (**15 s**), and updated denser-graph baselines for `discourse` Ruby (**8 s**, ~5.4 s wall / ~7.0 s process), `groovy` Gradle (**8 s**, ~6.9 s wall, ~139k nodes), `kotlin` JetBrains (**15 s**, ~13.7 s process, ~238k nodes), and `roslyn/src` C# (**22 s**, ~20.2 s process).

---

### Agent Pack Skills Split, Slash Commands & Guide Alignment (`skills/`, `agent-pack/`)

Restructured the embedded agent pack from a single monolithic skill into a **core structural router skill (`rgctl`) plus three on-demand slash-command workflow skills (`rgctl-review`, `rgctl-vuln`, `rgctl-migrate`)**, aligned with the CLI guide walkthroughs and fact-checked against live CLI JSON outputs:

- **`rgctl` (Core Structural Router — `skills/rgctl/SKILL.md` + `skills/rgctl/references/workflows.md`)**:
  - Always-available auto-router (`emit = ["skill"]`) for structural codebase questions: `find`, `callers`, `callees`, `relations`, `blast-radius`, `slice --with-alias`, `cpg flows`, `clones`, `metrics --pagerank|--centrality`, `search`, `diff`, `context-pack`, and Markdown knowledge graph queries (`discover -l markdown`).
  - Added **Closed-Loop Verification (After Code Edits)**: teaches agents to run `rgctl update` after modifying source files and verify transformations with structural graph assertions (`find --type import`, `callers`/`callees`, and `review paths --synthetic-head worktree -f json`) rather than stopping after text edits.
  - Added lazy-loaded playbooks in `skills/rgctl/references/workflows.md` (Workflows 1–9, including Markdown runbook navigation and post-edit verification loops).
- **`/rgctl:review` (`skills/rgctl-review/SKILL.md` — `emit = ["skill", "command"]`)**:
  - Dedicated PR, branch, and uncommitted worktree call-path review workflow (`rgctl -f json review paths` / `review check`).
  - Documents real JSON projection (`{change_summary, symbols: [.symbols[:15][] | {name, file, path_delta}]}`), `--synthetic-head worktree|staged`, `--max-symbols`, optional blast-radius escalation, and the `REVIEW_REPORT.md` four-section verdict rubric.
- **`/rgctl:vuln` (`skills/rgctl-vuln/SKILL.md` — `emit = ["skill", "command"]`)**:
  - Dedicated 3-step OSV/CVE reachability workflow (`vuln triage` $\rightarrow$ `deps check` $\rightarrow$ `vuln analyze --include-jars`), `.sink_taint.call_sites` / `.sink_taint.call_paths` verification, and OpenVEX status mapping (`affected`, `not_affected`, `under_investigation`).
- **`/rgctl:migrate` (`skills/rgctl-migrate/SKILL.md` — `emit = ["skill", "command"]`)**:
  - Dedicated 5-step Strangler-Fig migration workflow: `discover --export-migration-hints`, Fast-Leiden `communities` with `jq` projection (`select(.member_count > 1)`), `migration_plan.json` v2 step consumption, boundary `blast-radius`, and Step 5 post-edit verification (`rgctl update` + import/caller/review checks).
- **CLI Flag & JSON Schema Fact-Check**:
  - Audited every documented command against `example/coolstore` and `tests/fixtures/markdown-context`, removing unsupported `--limit` flags from `blast-radius`, `communities`, `slice`, and `inventory`, disambiguating multi-match Markdown sections (`docs/guide.md#Checkout Flow`), and syncing all dogfood skill trees (`.cursor/`, `.claude/`, `.agents/`, `.agent/`).

---

### Skill Benchmark & ScarfBench-Inspired Use-Case Evaluation Suite (`scripts/eval-skills.py`)

Added [`scripts/eval-skills.py`](../../scripts/eval-skills.py) (`--mode offline|agent`, `--suite queries|usecases|all`, `--usecase U1..U6`) to benchmark skill prompt budgets, diagnostic query trajectories (`with_skill` vs `no_skill`), and end-to-end code transformation accuracy using deterministic post-edit graph oracles in isolated temporary workspaces.

#### 1. Static Skill Token Budgets

| Skill File | Bytes | Est. Tokens (`bytes / 4`) | Token Budget | Byte Cap | Status |
|------------|------:|--------------------------:|-------------:|---------:|:------:|
| `skills/rgctl/SKILL.md` | 19,593 | **4,898** | 5,500 | 22,528 | ✓ |
| `skills/rgctl-review/SKILL.md` | 8,633 | **2,158** | 2,200 | 9,216 | ✓ |
| `skills/rgctl-vuln/SKILL.md` | 4,166 | **1,041** | 2,200 | 9,216 | ✓ |
| `skills/rgctl-migrate/SKILL.md` | 3,803 | **950** | 2,200 | 9,216 | ✓ |

*(Splitting `/rgctl:review`, `/rgctl:vuln`, and `/rgctl:migrate` into dedicated command skills saves **4,149 tokens** of always-on skill context when not invoked, and cuts single-command cold prompt overhead by **56%–81%** when invoked directly.)*

#### 2. Level 1 — Diagnostic Query Trajectories (`example/coolstore` & `example/kafka`)

Compares `with_skill` (`rgctl -f json` workflows) against `no_skill` (`rg` + source file reads). `Prompt 5-turn cached` models a 5-turn session with 1 cold turn + 4 prompt-cached turns (10% cached read weight); `Net 5-turn tok = Stdout tok + Prompt 5-turn cached`.

- **Small Java EE corpus (`example/coolstore` — 33 files):** `with_skill` reduces stdout tokens by **55%–95%** across all 6 workflows and reduces net 5-turn tokens by **27%–84%** on 5 of 6 workflows.
- **Large polyglot corpus (`example/kafka` — 7,521 files):** `with_skill` reduces net 5-turn tokens by **77%–97%** across all 6 workflows (up to **33.6× fewer net tokens** on migration planning and **31.9× fewer net tokens** on uncommitted review).

| Task | Corpus | Arm | OK | Wall ms | Stdout tok | Turns | Prompt 1-turn | Prompt 5-turn cached | Net 5-turn tok | Net Savings (`with_skill`) |
|------|--------|-----|:--:|--------:|-----------:|------:|--------------:|---------------------:|---------------:|---------------------------:|
| **Blast Radius** | `example/coolstore` | `with_skill` | ✓ | 105 | 918 | 1 | 4,898 | 1,371 | 2,289 | -10% *(56% less stdout)* |
| **Blast Radius** | `example/coolstore` | `no_skill` | ✓ | 27 | 2,076 | 3 | 0 | 0 | 2,076 | — |
| **OSV Reachability (`/rgctl:vuln`)** | `example/coolstore` | `with_skill` | ✓ | 113 | 814 | 3 | 1,041 | 291 | **1,105** | **-47% (1.9×)** |
| **OSV Reachability (`/rgctl:vuln`)** | `example/coolstore` | `no_skill` | ✓ | 21 | 2,078 | 4 | 0 | 0 | 2,078 | — |
| **Uncommitted Review (`/rgctl:review`)** | `example/coolstore` | `with_skill` | ✓ | 471 | 102 | 1 | 2,158 | 604 | **706** | **-66% (2.9×)** |
| **Uncommitted Review (`/rgctl:review`)** | `example/coolstore` | `no_skill` | ✓ | 22 | 2,076 | 3 | 0 | 0 | 2,076 | — |
| **Program Slicing** | `example/coolstore` | `with_skill` | ✓ | 21 | 262 | 1 | 4,898 | 1,371 | **1,633** | **-63% (2.7×)** |
| **Program Slicing** | `example/coolstore` | `no_skill` | ✓ | 21 | 4,370 | 3 | 0 | 0 | 4,370 | — |
| **Migration Planning (`/rgctl:migrate`)** | `example/coolstore` | `with_skill` | ✓ | 339 | 383 | 2 | 950 | 266 | **649** | **-84% (6.2×)** |
| **Migration Planning (`/rgctl:migrate`)** | `example/coolstore` | `no_skill` | ✓ | 32 | 4,026 | 9 | 0 | 0 | 4,026 | — |
| **Cross-File Callers / Hotspots** | `example/coolstore` | `with_skill` | ✓ | 121 | 1,628 | 1 | 4,898 | 1,371 | **2,999** | **-26% (1.4×)** |
| **Cross-File Callers / Hotspots** | `example/coolstore` | `no_skill` | ✓ | 38 | 4,076 | 9 | 0 | 0 | 4,076 | — |
| **Blast Radius** | `example/kafka` | `with_skill` | ✓ | 873 | 2,926 | 1 | 4,898 | 1,371 | **4,297** | **-80% (5.0×)** |
| **Blast Radius** | `example/kafka` | `no_skill` | ✓ | 246 | 21,518 | 7 | 0 | 0 | 21,518 | — |
| **OSV Reachability (`/rgctl:vuln`)** | `example/kafka` | `with_skill` | ✓ | 319 | 638 | 3 | 1,041 | 291 | **929** | **-97% (28.7×)** |
| **OSV Reachability (`/rgctl:vuln`)** | `example/kafka` | `no_skill` | ✓ | 163 | 26,700 | 6 | 0 | 0 | 26,700 | — |
| **Uncommitted Review (`/rgctl:review`)** | `example/kafka` | `with_skill` | ✓ | 1,198 | 102 | 1 | 2,158 | 604 | **706** | **-97% (31.9×)** |
| **Uncommitted Review (`/rgctl:review`)** | `example/kafka` | `no_skill` | ✓ | 250 | 22,546 | 5 | 0 | 0 | 22,546 | — |
| **Program Slicing** | `example/kafka` | `with_skill` | ✓ | 42 | 2,125 | 1 | 4,898 | 1,371 | **3,496** | **-91% (11.1×)** |
| **Program Slicing** | `example/kafka` | `no_skill` | ✓ | 170 | 38,966 | 3 | 0 | 0 | 38,966 | — |
| **Migration Planning (`/rgctl:migrate`)** | `example/kafka` | `with_skill` | ✓ | 2,745 | 334 | 2 | 950 | 266 | **600** | **-97% (33.6×)** |
| **Migration Planning (`/rgctl:migrate`)** | `example/kafka` | `no_skill` | ✓ | 239 | 20,171 | 9 | 0 | 0 | 20,171 | — |
| **Cross-File Callers / Hotspots** | `example/kafka` | `with_skill` | ✓ | 987 | 2,042 | 1 | 4,898 | 1,371 | **3,413** | **-77% (4.4×)** |
| **Cross-File Callers / Hotspots** | `example/kafka` | `no_skill` | ✓ | 223 | 15,092 | 9 | 0 | 0 | 15,092 | — |

#### 3. Level 2 — ScarfBench-Inspired Transformation Evals (`U1–U6`)

Evaluates end-to-end closed-loop transformations (`Pre-Query → Edit → rgctl update → Post-Edit Graph Oracle`) inside isolated temporary workspaces (`prepare_temp_workspace`):

| Use Case | Corpus | Arm | OK | Oracle | Score | Wall ms | Stdout tok | Turns | Net 5-turn tok | Post-Edit Graph Oracle Checks |
|----------|--------|-----|:--:|:------:|------:|--------:|-----------:|------:|---------------:|-------------------------------|
| **U1: Migrate `javax.ejb` $\rightarrow$ `jakarta.ejb`** | `example/coolstore` | `with_skill` | ✓ | ✓ | **1.00** | 446 | **1,905** | **4** | **3,542** | `javax.ejb imports == 0 (actual: 0)`; `jakarta.ejb imports == 13 (actual: 13)` |
| **U1: Migrate `javax.ejb` $\rightarrow$ `jakarta.ejb`** | `example/coolstore` | `no_skill` | ✓ | ✓ | 1.00 | 45 | 3,655 | 11 | 3,655 | `rg javax.ejb leftovers == 0 (actual: 0)` |
| **U2: Migrate `javax.json` $\rightarrow$ Jackson in `Transformers.java`** | `example/coolstore` | `with_skill` | ✓ | ✓ | **1.00** | 455 | **639** | **5** | **2,010** | `javax.json imports == 0 (actual: 0)`; `callers jsonToOrder == 1 (actual: 1)` |
| **U2: Migrate `javax.json` $\rightarrow$ Jackson in `Transformers.java`** | `example/coolstore` | `no_skill` | ✓ | ✓ | 1.00 | 46 | 3,216 | 6 | 3,216 | `rg javax.json leftovers == 0 (actual: 0)` |
| **U3: Refactor `priceShoppingCart` $\rightarrow$ `recalculateCartPricing`** | `example/coolstore` | `with_skill` | ✓ | ✓ | **1.00** | 890 | **1,865** | **5** | 3,841 | `find priceShoppingCart == 0`; `callers recalculateCartPricing == 4`; `review delta (changed=1, files=2, syms=1)` |
| **U3: Refactor `priceShoppingCart` $\rightarrow$ `recalculateCartPricing`** | `example/coolstore` | `no_skill` | ✓ | ✓ | 1.00 | 44 | 3,781 | 5 | 3,781 | `rg priceShoppingCart leftovers == 0 (actual: 0)` |
| **U4: Extract `ShippingService` behind `ShippingServiceRemote`** | `example/coolstore` | `with_skill` | ✓ | ✓ | **1.00** | 760 | **322** | **5** | **1,959** | `index_current/ready (True)`; `ShippingServiceRemote ss; present`; `implements ShippingServiceRemote (returned=14)` |
| **U4: Extract `ShippingService` behind `ShippingServiceRemote`** | `example/coolstore` | `no_skill` | ✓ | ✓ | 1.00 | 20 | 2,430 | 6 | 2,430 | `ShippingServiceRemote ss; present after edit` |
| **U5: Deduplicate `normalizePayload` exact clones** | `rgctl-tests/clone-exact` | `with_skill` | ✓ | ✓ | **1.00** | 401 | **593** | **4** | 1,964 | `clones group_count == 0 (actual: 0)`; `CloneB→CloneA callers/delegation` |
| **U5: Deduplicate `normalizePayload` exact clones** | `rgctl-tests/clone-exact` | `no_skill` | ✓ | ✓ | 1.00 | 1 | 543 | 5 | 543 | `CloneB delegates to CloneA` |
| **U6: Remediate `CVE-2022-42004` sink & re-verify VEX** | `example/coolstore` | `with_skill` | ✓ | ✓ | **1.00** | 424 | **1,026** | **3** | **1,317** | `post call_sites == 0 (actual: 0)`; `exploitability != exploitable (actual: not_exploitable)` |
| **U6: Remediate `CVE-2022-42004` sink & re-verify VEX** | `example/coolstore` | `no_skill` | ✓ | ✓ | 1.00 | 22 | 2,950 | 6 | 2,950 | `ObjectMapper removed from Transformers.java` |
