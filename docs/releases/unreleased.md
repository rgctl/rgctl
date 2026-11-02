# Unreleased (post v0.4.23)

<!-- Add bullets here during development; move to docs/releases/v0.4.x.md at tag time. -->

## Highlights

### Tier-1 AST & Deep-Analysis Coverage Enforcement (`rgctl-ast-coverage`, `rgctl-analysis`, `rgctl-lang-java`)

Replaced permissive `"Skip"`-based AST coverage checks with a **5-pillar compile-time and CI enforcement architecture** across all 24 Tier-1 tree-sitter languages, using **Java (`rgctl-lang-java`, 98.0% Grade `A+`)** as the gold reference implementation:

1. **Pillar 1 — Structured Passive Classification & Semantic Node Denylist (`crates/rgctl-ast-coverage`, `crates/rgctl-plugin-api`)**:
   - Extended `HandlerMask` and `ALLOWED_HANDLERS` with explicit passive categories: `SkipWrapper` (structural syntax containers transparent to tree walking), `SkipTypeToken` (leaf primitive/modifier tokens consumed by parent handlers), and `Unimplemented` (tracked backlog requiring a non-empty reason in `skip_justifications`).
   - Added `is_semantic_node_kind(kind)` denylist enforcement (`*_declaration`, `*_definition`, `*_statement`, `*_expression`, `*_invocation`, `*_call*`, `*_creation*`, `*_clause`, `*_pattern`, `*_directive`, `lambda*`, `closure*`) so semantic AST kinds cannot be silently dismissed without an explicit `skip_justifications` entry.
   - Removed the `(often Skip)` suggestion from `check_manifest` error diagnostics when a grammar pin introduces new named AST kinds.
2. **Pillar 2 — Quantitative Semantic AST Coverage Ratio & Monotonic Ratchet (`crates/rgctl-ast-coverage`)**:
   - Added `CoverageGrade` (`A+` $\ge 98\%$, `A` $\ge 75\%$, `B` $\ge 50\%$, `C` $< 50\%$), `SemanticCoverageStats`, and `compute_semantic_coverage` over the semantic denominator (`handled + unimplemented`, excluding `SkipWrapper` and `SkipTypeToken`). Raw `"Skip"` is banned from manifests.
   - Added `min_handled_kinds` and `target_grade` monotonic ratchet fields to `CoverageSpec` across all 24 Tier-1 languages in `bundled_specs()` so handled AST counts and coverage grades can never regress.
3. **Pillar 3 — Dispatch Table Gating & Three-Way Manifest Sync (`crates/rgctl-lang-java`, `crates/rgctl-analysis`, `crates/rgctl-ast-coverage`)**:
   - Gated `crates/rgctl-lang-java/src/single_pass.rs` on `mask.is_passive()` so AST kinds marked passive in `java-ast-coverage.json` cannot execute hidden extraction logic.
   - Exposed `cfg_handled_kinds(language_id)` in `crates/rgctl-analysis/src/cfg_builder.rs` and `def_use_handled_kinds(language_id)` in `crates/rgctl-analysis/src/def_use.rs`, and enforced 3-way agreement (`{id}-ast-coverage.json` $\leftrightarrow$ `plugin.declared_ast_handlers()` $\leftrightarrow$ `cfg_builder` / `def_use`) via `check_plugin_and_analysis_manifest_sync`.
4. **Pillar 4 — Cross-Layer Behavioral Parity Gate (`crates/rgctl-analysis/tests/tier1_parity_gate.rs`)**:
   - Added a 6-test cross-layer parity suite verifying all 24 Tier-1 languages across:
     - Manifest ratchets & 3-way handler sync (`tier1_all_24_languages_pass_manifest_ratchet_and_three_way_sync`),
     - **Layer A**: Symbol & relation extraction invariants (`tier1_layer_a_symbol_and_relation_invariants`),
     - **Layer B**: CFG block/edge construction & PDG def-use invariants (`tier1_layer_b_cfg_and_pdg_invariants`),
     - **Layer C**: Bundled taint rules ($\ge 1$ source, sink, and sanitizer across all 20 `taint_enabled` languages, adding `crates/rgctl-analysis/rules/taint/tier1-extended.yaml` for `swift`, `scala`, `dart`, `elixir`, `lua`, `bash` and sanitizer rules to `groovy.yaml` and `puppet.yaml`, plus Spring/JPA/JNDI/SpEL/deserialization rules in `java.yaml`),
     - **Layer F**: `field_write_locals::language_visit_fn` registration & OOP metadata invariants (`tier1_layer_f_field_write_locals_and_oop_metadata_invariants`),
     - **Corpus Hotspot Audit**: Unhandled AST node frequency ranking (`tier1_corpus_unhandled_ast_hotspot_audit`).
5. **Pillar 5 — Unconditional `assert_all_exercised()` & Hotspot Telemetry (`crates/rgctl-ast-coverage/src/tracker.rs`, `scripts/check-tier1-coverage.py`)**:
   - Added `record_skip_hit`, `skip_hit_count`, and `unhandled_hotspots(top_n)` to `AstCoverageTracker`.
   - Upgraded Java (`crates/rgctl-lang-java/java-ast-coverage.json`) to **100 active handled kinds (`100/102 = 98.0%`, Grade `A+`)** with `0` raw `Skip`s (`31 SkipWrapper`, `9 SkipTypeToken`, `2 Unimplemented`) and unconditional `tracker.assert_all_exercised()` in both `java_depth_extraction.rs` and `tests/java_langfeatures.rs` (all 83 plugin-dispatched Java AST kinds exercised).
   - Added [`scripts/check-tier1-coverage.py`](../../scripts/check-tier1-coverage.py) (`--json`, `--language <id>`, `--run-tests`) to audit all **69 languages** and classify them into honest sub-tiers (`1 Tier-1 Gold`, `13 Tier-1 Deep`, `6 Tier-1 Shallow`, `4 Tier-1 DSL`, `3 Mainframe Custom`, `42 Tier-2 tags.scm`).

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
