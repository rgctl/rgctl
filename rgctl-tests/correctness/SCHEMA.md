# Expected-facts schema (`expected-facts.json`)

Hand-labeled graph facts checked by `cargo test --test graph_correctness` (approach **A**) plus
cross-feature invariants (approach **B**). See [sshaaf/rgctl#26](https://github.com/rgctl/rgctl/issues/26)
and [QE.md](./QE.md) for required-red policy (option B).

## Location

```text
rgctl-tests/ecommerce-<lang>/correctness/expected-facts.json
tests/fixtures/correctness_matrix/<lang>/correctness/expected-facts.json
```

Projects without this file are skipped by the `graph_correctness` tests. Every project executes inside an isolated `TempDir` sandbox (`-d <temp_db>`) so parallel runs never write `.rgctl` caches into tracked directories, and every snapshot is checked with `assert_graph_invariants` (`INV-1`..`INV-9`, including byte-span & container inequality `INV-8` and sibling `Function` span distinctness `INV-9`).

## Top-level fields

| Field | Type | Meaning |
|-------|------|---------|
| `schema_version` | int | `2` for full 17-dimension harnesses (`1` remains supported) |
| `language` | string | Language id (`java`, `rust`, `swift`, `hcl`, `markdown`, …) |
| `project` | string | Directory name (`ecommerce-java`, `swift`, …) |
| `graph` | object | Repo-level minimums, exact counts, `forbidden_edges`, and `expected_relations` |
| `symbols` | object | Named expected-fact entries (keys are stable ids) |
| `invariants` | object | Cross-feature checks (`B1`..`B9`) |
| `analytics` | object | Optional graph-analytics laws (`harmonic`, `hyperball`, `pagerank`, `betweenness`, `migration_plan.json`, Fast-Leiden `separate` & connectedness) |

## Severity

| Value | CI behavior |
|-------|-------------|
| `required` | Failure → test fails |
| `best_effort` | Reported only |
| `unsupported` | Skipped |

## Symbol entry (thorough)

Prefer **exact** fields for harness symbols. Soft `min_*` / `*_any` remain for domain edges that are still noisy.

```json
{
  "match": { "name": "correctnessMid", "class": "CorrectnessHarness" },
  "severity": "required",
  "unique": true,
  "identity": {
    "exact_name": "correctnessMid",
    "canonical_fqn": "CorrectnessHarness::correctnessMid",
    "class_context": "CorrectnessHarness",
    "file_suffix": "CorrectnessHarness.java",
    "language": "java"
  },
  "exact_callees": ["correctnessLeaf"],
  "exact_callers": ["correctnessRoot"],
  "blast": {
    "exact_direct_callers": ["correctnessRoot"],
    "exact_impact_zone": ["correctnessRoot"],
    "min_score": 0.1
  },
  "ast": {
    "must_call": ["correctnessLeaf"],
    "statements_contain": ["return correctnessLeaf() + 1;"],
    "exact_statement_kinds": ["Return"]
  },
  "cfg": {
    "exact_block_count": 2,
    "exact_edge_count": 1,
    "exact_edge_kinds": ["return"]
  },
  "pdg": {
    "exact_data_deps": 0,
    "exact_control_deps": 0,
    "exact_node_count": 1,
    "data_edges": [{ "variable": "value" }],
    "node_labels_contain": ["correctnessLeaf()"]
  },
  "dom": {
    "exact_block_count": 2,
    "exact_idom": [
      { "block": 0, "immediate_dominator": 1 },
      { "block": 1, "immediate_dominator": 1 }
    ]
  }
}
```

### Layers

> **Important:** `rgctl gql` is **deprecated** and is not used by `graph_correctness`. All symbol and edge assertions run against canonical structured JSON verbs (`find`, `callers`, `callees`, `relations`, `inventory`, `blast-radius`, `inspect`, `slice`, `cpg`, `security`, `clones`, `semantic`, `review`) and typed `ColumnarGraphSnapshot` / `AnalysisResults` APIs.

| Section | Source of truth | What we assert |
|---------|-----------------|----------------|
| `identity` | `blast-radius` `target` + `inspect` `symbol` | Exact function name, FQN, class, file, language |
| `exact_callees` / `exact_callers` | Structured `relations --edge calls` / `callers` / `callees` | Exact short-name sets (no extras) |
| `blast` | `blast-radius` topology/metrics | Exact callers, impact zone, counts, score |
| `ast` | CFG statement texts/kinds | Call sites and statement text as lowered from AST |
| `cfg` | `inspect SYMBOL cfg` | Exact blocks, edges, edge kinds, statement text |
| `pdg` / `dataflow` | `inspect SYMBOL pdg` | Data/control deps, node kinds/labels, data-edge variables |
| `dom` / `dominance` | `inspect SYMBOL dom` | Block count, idom pairs |

### `match`

Used to resolve a unique function via `blast-radius` / `inspect`:

- Prefer `Class::name` when `class` is set
- Fall back to bare `name` (must be unique when `unique: true`)

## Invariants (B)

| Id | Check |
|----|--------|
| `B1_blast_vs_calls` | Blast `topology.direct_callers` names ≡ reverse CALLS neighbors |
| `B2_gql_calls_nonzero` | At least one CALLS edge exists after discover |
| `B5_inspect_cfg_present` | `inspect <symbol> cfg` returns nodes |
| `B6_cfg_calls_subset_of_calls_edges` | Expected callees appear in both CALLS graph and CFG/AST text |
| `B7_pdg_lines_subset_cfg` | Every PDG node line appears in CFG statements |
| `B8_dom_blocks_match_cfg` | DOM `block_index` set ≡ CFG `block_index` set |
| `B9_blast_target_name` | Blast `canonical_fqn` short name ≡ `match.name` |

## Harness conventions

Each `ecommerce-*` app should include a small intentional call chain:

`correctnessRoot` → `correctnessMid` → `correctnessLeaf`

Additionally, each language ships **CFG feature probes** (`CfgFeatures` / `cfg_features.*`) that exercise
control-flow lowering unique to that language (short-circuit, loops, switch, try/cleanup, await, …).
Those symbols are asserted via `cfg.min_blocks` + `cfg.statements_contain` in `expected-facts.json`.

(Language-idiomatic spellings: `correctness_root`, `CorrectnessRoot`, etc.)

Prefer **direct calls** with **no intermediate locals** assigned from call results **in C/C++**.
Some C/C++ extractors have mis-attributed `int value = mid();` as a CALLS edge from `value` → `mid`.
Java may keep `int value = mid();` when PDG data-deps on `value` are part of the expected-facts.

Keep harness definitions in **one translation unit** (avoid header prototypes that become duplicate Function nodes).

## Versioning

Bump `schema_version` when fields are removed or semantics change. Additive fields may keep `1`.
`schema_version` `2` marks a file that uses the v2 sections below. The runner accepts v1 files unchanged.

## v2 sections (additive)

Omitted sections are skipped. Set `severity` to `unsupported` to record a dimension the language does not implement.

| Section | CLI | What we assert |
|---------|-----|----------------|
| `graph.expected_relations` | `relations <from> --edge <edge> --direction outgoing` | Asserts typed non-`Calls` edges (`references`, `depends-on`, `uses`, …) for structural languages (`sql`, `hcl`, `markdown`, `erb`) as well as general projects |
| `match.entity_type` | `find --type <entity_type>` | Resolves non-function symbols (`table`, `class`, `module`, …) via structured `find` when `blast-radius`/`inspect` are function-scoped |
| `match.discriminator` / `match.line` | `find` + `blast-radius`/`inspect --line` | One overload: qualified name or signature contains the discriminator, and the definition line matches when `find` returns `line` |
| `identity.discriminator` | `blast-radius` target / `inspect` | The same fragment appears on the resolved node |
| `mro.extends` / `implements` / `overrides` | `relations --edge extends\|implements\|overrides` | Owner (class, or the symbol when no class) has each named `Extends`/`Implements` target; the symbol has each named `Overrides` target |
| `slice` | `rgctl -f json slice FILE --line N --variable V` | `must_contain` / `must_exclude` substrings. Default direction is `backward` |
| `flows` | `rgctl -f json cpg flows FILE --line N --variable V --with-alias` | Same text checks. Default direction is `forward`; `with_alias` defaults to true |
| `mutations` | `rgctl -f json cpg mutations --type NAME --exclude-ctors` | Text checks. `exclude_ctors` defaults to true. Optional `member` |
| `taint` | `rgctl -f json security taint --sink S --source external` | Text checks plus optional `min_flows` against the `flows`/`paths` array (intra- and inter-procedural) |
| `clones` | `rgctl -f json clones --mode exact\|bloom\|fragment --no-write` | `group_with` names share a group with this symbol; `not_grouped_with` names do not. Optional `seed`, `min_loc` (default 1) |
| `semantic` | `semantic index --embedder vocab` then `semantic query --limit 3 --expand neighbors` | `top3_contains` is in the top 3 hit names (optional `scope` path prefix via `--scope`). `neighbor_names` must appear in the neighbor expansion |
| `review` | `review paths --synthetic-head worktree` | Sandbox-only edit: replace `find` with `replace` in `file` inside a `TempDir` copy with local git init, then require `must_contain` names in the report. The tracked fixture is never modified |

## `analytics`

Present only when the harness should re-discover with
`discover . --with-cfg --with-taint --with-security --with-harmonic --export-migration-hints`
and check the call chain `root -> mid -> leaf`:

| Check | Law |
|-------|-----|
| `harmonic` | `harmonic(root) > harmonic(mid) > harmonic(leaf) == 0` |
| `hyperball` | Exact BFS harmonic and `HyperBallHarmonic` agree on that strict order |
| `pagerank` | `pagerank(leaf) > pagerank(mid) > pagerank(root)` |
| `betweenness` | `betweenness(mid) > 0`, and both endpoints are `0` |
| `migration_plan.json` | at least one finite `avg_harmonic` greater than 0 |
| `separate` | optional pair of names that must receive different community ids |
| connectedness | each non-infrastructure community's behavioral (`Calls` / `Uses` / `References`) participants form one connected subgraph |

```json
{
  "analytics": {
    "root": "correctnessRoot",
    "mid": "correctnessMid",
    "leaf": "correctnessLeaf",
    "separate": ["cliqueA", "cliqueB"]
  }
}
```

```json
{
  "match": { "name": "process", "class": "Order", "discriminator": "(int)", "line": 40 },
  "identity": { "discriminator": "(int)" },
  "mro": { "overrides": ["process"] },
  "slice": {
    "file": "src/Order.java",
    "line": 42,
    "variable": "y",
    "must_contain": ["input"],
    "must_exclude": ["99"]
  },
  "flows": {
    "file": "src/Order.java",
    "line": 42,
    "variable": "y",
    "must_contain": ["alias"]
  },
  "mutations": { "type": "Order", "must_exclude": ["<init>"] },
  "taint": { "sink": "sink", "min_flows": 1 },
  "clones": { "mode": "exact", "group_with": ["processCopy"], "not_grouped_with": ["sibling"] },
  "semantic": { "query": "refund payment", "top3_contains": "process", "neighbor_names": ["save"] },
  "review": {
    "file": "src/Order.java",
    "find": "return y",
    "replace": "return z",
    "must_contain": ["correctnessRoot"]
  }
}
```
