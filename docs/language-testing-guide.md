# Language testing guide

Practical guide for contributors **adding or extending a programming language** in rgctl: where tests live, what new files to add, and what each suite must prove.

**Not this doc:** capability bar (Layers A–F) — see [tier-1-language-support.md](tier-1-language-support.md). Scaffold / crate layout — see that doc §3–4. PR workflow hub — [contributor-checklist.md](contributor-checklist.md).

---

## 1. Mental model

Language quality is proven at **four distances** from the plugin:

| Distance | Proves | Typical home |
|----------|--------|--------------|
| **Unit** | Grammar walkers emit the right symbols/relations for tiny snippets | `crates/rgctl-lang-{id}/` |
| **Fixture** | Structured query / CFG / taint on curated sources | `tests/fixtures/{id}/`, `tests/{id}_*.rs` |
| **Project** | Realistic multi-file graph + dashboard + expected-facts | `rgctl-tests/ecommerce-{id}/` |
| **Scale / QE** | Determinism, stable keys, invariants, cold discover | Shared QE binaries + Gate B corpus |

Always start with unit + a small fixture. Do not open a Tier 1 PR with only “discover succeeds on one file.”

---

## 2. Where to put tests (map)

```text
crates/rgctl-lang-{id}/
  src/…                    # plugin + unit tests (#[cfg(test)])
  {id}-ast-coverage.json   # grammar kind → extraction intent (build-gated)

tests/fixtures/{id}/
  langfeatures/…           # small sources for langfeatures / CFG probes
  …                        # optional taint / edge-case snippets

tests/
  {id}_langfeatures.rs     # GQL / structured-query gates on ecommerce or fixtures
  {id}_cfg_analysis.rs     # CFG/PDG after discover --with-cfg
  {id}_taint.rs            # source→sink (or declarative pack) gate
  dashboard_ecommerce_{id}.rs

rgctl-tests/ecommerce-{id}/
  src/…                    # multi-file sample app
  correctness/
    expected-facts.json    # graph_correctness harness (required for CI coverage)

Cargo.toml                 # [[test]] entries for each new integration binary
.github/workflows/ci.yml   # only if you add a named always-on QE step
```

**Copy from a neighbor language** (same family helps):

| You’re adding… | Best templates |
|----------------|----------------|
| JVM-like (Kotlin, Groovy, Scala) | `java_*`, `ecommerce-java`, `java_langfeatures` |
| C-family | `c_*` / `cpp_*`, `ecommerce-c` / `ecommerce-cpp` |
| Dynamic (Ruby, PHP, JS) | `ruby_*` / `php_*` / `javascript_*` |
| Systems (Rust, Go) | `rust_*` / `go_*` |

---

## 3. What to add for a **new** language

Minimum set depends on tier. Checkboxes are “add these artifacts,” not “paste empty stubs.”

### Tier 3 (regex) — minimum

- [ ] Unit tests in `rgctl-lang-{id}` for pattern matches / non-matches
- [ ] Optional: one file under `tests/fixtures/{id}/` + a tiny integration test if discover wiring is non-obvious
- [ ] **No** CFG / taint / dashboard / Gate B requirements

### Tier 2 (generic tree-sitter) — minimum

- [ ] Plugin / config unit tests (`extract_symbols` on fixtures)
- [ ] `{id}-ast-coverage.json` + `ast_coverage` unit test (same drift rule as Tier 1)
- [ ] Optional fixture under `tests/fixtures/{id}/`
- [ ] CFG/taint/dashboard only if you claim those capabilities

### Tier 1 (custom plugin) — required test matrix

Add **all** of the following before calling the language “fully supported.” Ship incrementally if needed, but the PR that claims Tier 1 must close the gaps.

| # | Artifact | What it must prove |
|---|----------|--------------------|
| T1 | `cargo test -p rgctl-lang-{id}` | Symbols (functions/types), `Calls` (and inheritance if applicable), honesty limits documented in tests or plugin comments |
| T2 | AST coverage JSON + unit test | Every named grammar kind classified; bump grammar ⇒ update JSON |
| T3 | `tests/fixtures/{id}/langfeatures/` (+ optional) | Snippets for features you claim (methods, generics, embeds, …) |
| T4 | `tests/{id}_langfeatures.rs` + `[[test]]` | After `discover -l {id}`, GQL/structured query finds harness symbols and expected `Calls` |
| T5 | `tests/{id}_cfg_analysis.rs` | `discover --with-cfg`: CFG block/edge counts or kinds for a branching + looping function |
| T6 | `tests/{id}_taint.rs` + `rules/taint/{id}.yaml` | At least one source→sink path (or pack hit) with `--with-cfg --with-taint` |
| T7 | `rgctl-tests/ecommerce-{id}/` | Multi-file sample with real calls across files |
| T8 | `…/correctness/expected-facts.json` | Registered in `tests/graph_correctness.rs` `PROJECTS`; `min_functions` / `min_calls_edges` (prefer exact harness symbols when stable) |
| T9 | `tests/dashboard_ecommerce_{id}.rs` | Release discover with dashboard harness flags; CFG index available |
| T10 | Field-write / Layer F golden (if CPG claimed) | `{id}_cfg_captures_field_write_and_query` pattern in `field_write.rs` — see Tier 1 Layer F |
| T11 | Gate B cold discover (scale) | ~10k-file corpus entry in [AGENTS.md](../AGENTS.md) Gate B table + baseline in `tests/cold_profile_gates.rs` once numbers exist |

**Shared graph QE (no new binary required, but your fixtures must pass):**

| Suite | Command | Your language is covered when… |
|-------|---------|--------------------------------|
| Graph correctness + invariants | `cargo test --test graph_correctness` | `ecommerce-{id}` is in `PROJECTS` and has `expected-facts.json` (`assert_graph_invariants` runs on every project) |
| Stable-key line-shift | `cargo test --test stable_key_invariance_qe` | Language id listed in `LANGS` in that file (prepend 15 comments/blanks → `diff_snapshots` adds/removes **0** nodes/edges). **Do not** put line numbers in `qualified_name` |
| Extraction determinism | `cargo test --test graph_determinism_qe` | Prefer a multi-file fixture; counts (and ideally `content_digest`) stable across file order / thread count |
| Cross-feature QE | `cargo test --test cross_feature_qe` | Collision / CALLS↔blast↔CFG agreement on shared fixtures (extend if you introduce ambiguity) |

---

## 4. What to assert (by suite)

### Plugin unit tests (`rgctl-lang-{id}`)

Assert on **extracted structures**, not CLI:

- Function/method **name**, **file**, **start_line** (line may move; identity must not depend on line — use `qualified_name` without `@L{n}`)
- At least one **`Calls`** edge between in-fixture symbols
- Inheritance / implements edges if the language has them
- Negative: do not invent calls into string literals / comments

### `{id}_langfeatures.rs`

Typical pattern (see `tests/ruby_langfeatures.rs`, `tests/go_langfeatures.rs`):

1. Cold `discover . -l {id}` on `rgctl-tests/ecommerce-{id}` or `tests/fixtures/{id}/langfeatures`
2. `rgctl -f json gql '…'` or structured `find` / `callers`
3. Assert symbol presence, callee/caller sets, and language property when set

### `{id}_cfg_analysis.rs`

- Discover with `--with-cfg` (and `-l {id}`)
- Inspect CFG via CLI (`inspect` / dashboard index / analysis APIs used by sibling tests)
- Require branching (`if`) and a loop when the language has them
- Prefer exact block/edge counts on a frozen harness function

### `{id}_taint.rs`

- Declarative pack under `crates/rgctl-analysis/rules/taint/{id}.yaml` (preferred over hard-coded Rust detectors)
- Fixture with a clear source and sink
- Assert a finding path exists (JSON schema + non-empty path)

### `expected-facts.json` (`graph_correctness`)

Schema: [rgctl-tests/correctness/SCHEMA.md](../rgctl-tests/correctness/SCHEMA.md). Policy: [QE.md](../rgctl-tests/correctness/QE.md) (required failures stay red).

| Field | Guidance |
|-------|----------|
| `graph.min_functions` / `min_calls_edges` | Floor so empty extract fails CI |
| `graph.exact_*` / `forbidden_edges` | Use when the harness is stable |
| `symbols.*.exact_callees` / `exact_callers` | Prefer over soft mins for correctness harness methods |
| `symbols.*.identity.canonical_fqn` | Must match what blast/inspect emit **today** |
| `invariants.B*` | Enable B1/B2/B5… only when CFG/blast are wired for the language |

After discover, `graph_correctness` also runs **`assert_graph_invariants`** (sorted unique nodes/edges, Calls endpoint legality, stable-key consistency, index bijection, digest, external stubs). If this fails, fix extraction or stub policy — do not weaken the checker for convenience.

### Dashboard ecommerce

- Follow `tests/dashboard_harness.rs` + an existing `dashboard_ecommerce_*.rs`
- Release binary; assert discover metrics and CFG/taint index availability as siblings do

### Stable keys (mandatory for Tier 1)

`StableNodeKey` facets are **path + name + node_type + qualified_name** — **not** `start_line`.

- Prepending blank/comment lines must not add/remove nodes or edges in `diff_snapshots`
- If your plugin puts `@L{line}` (or similar) in `qualified_name`, line-shift QE will fail — fix the plugin

### Cold / Gate B

- Small fixtures never replace Gate B
- When ready: fetch corpus (`./scripts/fetch-profile-repos.sh`), record `wall_secs` / nodes from `[profile] discover summary`, add ignored gate + baseline (+10%) in `tests/cold_profile_gates.rs`
- Document corpus path and env override in [AGENTS.md](../AGENTS.md) Gate B table

---

## 5. Wiring a new integration test binary

1. Add `tests/{name}.rs`.
2. Register in root `Cargo.toml`:

```toml
[[test]]
name = "{id}_langfeatures"
path = "tests/{id}_langfeatures.rs"
```

3. Run with:

```bash
cargo test --test {id}_langfeatures -- --nocapture
```

4. CI already runs `cargo test --workspace --lib --bins --tests`, so new `[[test]]` binaries are included. Add a **named** step in `.github/workflows/ci.yml` only for slow release gates (dashboard, scale) that should stay visible in the Actions UI.

---

## 6. Commands to run before a language PR

Replace `{id}` with your language id (`ruby`, `kotlin`, …).

```bash
# Format / lint (touched crates)
cargo fmt --all -- --check
cargo clippy -p rgctl-lang-{id} -p rgctl-languages -p rgctl-analysis -- -D warnings

# AST coverage drift
cargo check -p rgctl-languages
# optional strict:
# RGCTL_AST_COVERAGE_STRICT=1 cargo check -p rgctl-languages

# Plugin units
cargo test -p rgctl-lang-{id}

# Language integration binaries you added
cargo test --test {id}_langfeatures -- --nocapture
cargo test --test {id}_cfg_analysis -- --nocapture
cargo test --test {id}_taint -- --nocapture

# Graph correctness (includes your ecommerce-{id} once registered)
cargo test --test graph_correctness -- --nocapture

# Shared QE (stable keys + determinism)
cargo test --test stable_key_invariance_qe -- --nocapture
cargo test --test graph_determinism_qe -- --nocapture

# Dashboard (release)
cargo build --release --bin rgctl
cargo test --release --test dashboard_ecommerce_{id} -- --nocapture

# Full contributor workflow (when ready)
# see contributor-checklist.md §5
```

Paste the commands you ran in the PR description.

---

## 7. Checklist (copy into PR)

```markdown
### Language testing ({id})

- [ ] Tier chosen (1 / 2 / 3) and honesty limits noted
- [ ] `rgctl-lang-{id}` unit tests green
- [ ] AST coverage JSON updated / registered
- [ ] `tests/{id}_langfeatures.rs` (Tier 1)
- [ ] `tests/{id}_cfg_analysis.rs` (Tier 1)
- [ ] `tests/{id}_taint.rs` + `rules/taint/{id}.yaml` (Tier 1)
- [ ] `rgctl-tests/ecommerce-{id}/` + `correctness/expected-facts.json`
- [ ] `PROJECTS` entry in `tests/graph_correctness.rs`
- [ ] `dashboard_ecommerce_{id}` (Tier 1)
- [ ] Listed in `stable_key_invariance_qe` `LANGS` (no line-coupled QN)
- [ ] Gate B corpus planned or baselined (Tier 1 scale)
- [ ] Docs: languages coverage page regenerates from JSON; checklist / this guide updated if you invent a new suite pattern
```

---

## 8. Related docs

| Doc | Role |
|-----|------|
| [tier-1-language-support.md](tier-1-language-support.md) | Capability Layers A–F + scaffold steps |
| [contributor-checklist.md](contributor-checklist.md) | End-to-end PR hub |
| [languages/README.md](languages/README.md) | AST coverage SSOT / website |
| [rgctl-tests/correctness/SCHEMA.md](../rgctl-tests/correctness/SCHEMA.md) | `expected-facts.json` fields |
| [rgctl-tests/correctness/QE.md](../rgctl-tests/correctness/QE.md) | Required-red policy |
| [AGENTS.md](../AGENTS.md) | Gate A/B cold profiles, ingest rules |
| [internal/integration-tests.md](internal/integration-tests.md) | Broader Tier A/B/C integration matrix |
