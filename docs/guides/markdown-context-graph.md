# Markdown Context Graph

## Introduction

rgctl indexes `.md` and `.mdx` files into a **documentation context graph** alongside your code. Headings become navigable sections, internal links become `REFERENCES` edges, code fences become searchable blocks, and frontmatter keys become queryable variables — all in the same `graph.snapshot.bin` that powers `find`, `relations`, `metrics`, and `export`.

This guide walks through `discover`, structured queries, Obsidian/OKF export, and doc-scoped semantic search. The primary example is the English docs from [kubernetes/website](https://github.com/kubernetes/website) (`content/en`, on the order of **~34k heading/module nodes on current kubernetes/website English docs**). A small in-tree fixture covers every markdown construct and doc→code linking.

## Use Cases

- **Large documentation sites.** Index an entire docs tree as heading modules with `CONTAINS` hierarchy and `REFERENCES` cross-links.
- **Obsidian vault browsing.** Export one note per heading section, with folder layout mirroring doc paths and wikilinks from internal links.
- **Agent-first doc navigation.** Query heading structure, cross-links, and community membership with `find` / `relations` instead of reading every file.
- **Natural-language section search.** Build a doc-scoped semantic index over heading bodies and code blocks (`semantic index --scope docs`).
- **Doc + code linking.** Discover markdown and Java (or other languages) together; walk doc → file → class in one query.
- **Architecture decision records.** Index ADRs with heading hierarchy, file links, and section-level `REFERENCES` edges.

## Example Projects

This guide uses **two checkouts** — each for a different part of the walkthrough, not as alternatives to the same task.

1. **[kubernetes/website](https://github.com/kubernetes/website)** (`example/k8s-website/`) — **Steps 1–4:** `discover`, Obsidian export, doc semantic search, communities, and OKF on a real documentation site. Sparse-checkout `content/en` before you start.

2. **markdown-context fixture** (`tests/fixtures/markdown-context/`) — **Step 5 and the construct showcase:** every supported markdown syntax, plus `CheckoutService.java` for doc→code linking. Always in-tree; no clone.

```bash
# kubernetes/website — sparse clone of English docs (same dest as ./scripts/fetch-profile-repos.sh)
mkdir -p example
git clone --depth 1 --filter=blob:none --sparse https://github.com/kubernetes/website.git example/.tmp-k8s-website
(cd example/.tmp-k8s-website && git sparse-checkout set content/en)
rm -rf example/k8s-website
mv example/.tmp-k8s-website/content/en example/k8s-website
rm -rf example/.tmp-k8s-website

export REPO="$(pwd)/example/k8s-website"
export REPO_FIXTURE="$(pwd)/tests/fixtures/markdown-context"
export RGCTL_NO_DAEMON=1   # write artifacts to $REPO/.rgctl/ (see Installation)
```

Maintainers who already fetch profile corpora can skip the clone: `./scripts/fetch-profile-repos.sh` then `export REPO="$(pwd)/example/k8s-website"`.

**Prerequisites:** `rgctl` on your `PATH`. For large exports (17k+ Obsidian notes), use a **release** binary — [download the latest release](https://github.com/sshaaf/rgctl/releases) or [build from source](../installation.md) (`cargo build --release --bin rgctl`). See [Installation](../installation.md).

Artifacts are written to `{repo}/.rgctl/` next to the checkout. See [Installation](../installation.md).

## What rgctl indexes

rgctl parses markdown with official `tree-sitter-md` (block + inline grammars). The table below maps author syntax to graph nodes and edges.

| Author writes | Graph | Properties / edges |
|---------------|-------|-------------------|
| `# Heading` / `## …` (ATX) | `:Module` | `kind=heading`, `level`, `slug`, QN `{path}#{slug}` |
| Setext heading (`===` / `---` underline) | `:Module` | Same as ATX (`kind=heading`) |
| Nested headings | `:Module` + `CONTAINS` | Parent heading → child heading |
| Section prose under a heading | on heading node | `body_text`, `body_hash`; large → `body_ref` |
| `` ```lang … ``` `` fenced code | `:Module` | `kind=code_block`, `language`, `body_text` |
| Indented code (4 spaces) | `:Module` | `kind=code_block` (no language) |
| `[text](./file.md)` file link | `:Import` + `REFERENCES` | Edge to `:File`; `to_type_hint=file` |
| `[text](./file.md#section)` heading link | `:Import` + `REFERENCES` | Edge to `:Module` heading; `to_type_hint=module` |
| `[jump](#slug)` same-file fragment | `:Import` + `REFERENCES` | Edge to heading in same file |
| Reference-style links (`[x][ref]` + `[ref]: url`) | `:Import` + `REFERENCES` | Full, collapsed, and shortcut forms |
| `https://…` / `mailto:` links | `:Import` only | Link symbol with `url`; **no** `REFERENCES` edge |
| `![alt](img.png)` images | — | Not indexed |
| `[[wikilink]]` | — | Not indexed |
| YAML frontmatter (`---`) | `:Variable` | `kind=frontmatter`, flattened keys (`metadata.author`), `value` |
| TOML frontmatter (`+++`) | `:Variable` | Same flattening as YAML |
| Links inside table cells | `:Import` + `REFERENCES` | Parsed from `pipe_table_cell` inline trees |
| `.mdx` files | same as `.md` | Structure only — JSX in fences is not executed |

**Qualified names:** `{file_path}#{slug}` where `slug` is ASCII-slugified heading text. Duplicate titles in one file get `-2`, `-3`, … suffixes.

**Fragments are literal:** `[link](./adr.md#payments)` targets `adr.md#payments`, not a slugified variant. Prefer slug fragments (`#checkout-flow`) over visible titles (`#Checkout Flow`).

For the full node model and query patterns, see [markdown-context.md](../markdown-context.md).

## Step-by-Step

### 1. Discover documentation (kubernetes/website)

If you built from source, ensure `target/release/rgctl` is on your `PATH` (see [Add to PATH](../installation.md#add-to-path)).

```bash
export REPO="$(pwd)/example/k8s-website"
export RGCTL_NO_DAEMON=1

rgctl -r "$REPO" discover . -l markdown
```

**Output:**

```
[>] rgctl discover
[✓] rgctl discover finished in ~seconds (release build; wall time varies)
```

**What happened:**

- rgctl parsed the sparse checkout of kubernetes/website `content/en` — thousands of `.md` files, on the order of **~34k** `:Module` nodes on the current tree (`find --type module --count-only` → **33953**), with a large overall graph (`status` reports **136527** nodes / **256210** edges including files, imports, and related kinds; `schema_version` 3).
- The graph snapshot was written to `$REPO/.rgctl/graph.snapshot.bin`.
- Section bodies larger than 32 KiB inline were stored in `$REPO/.rgctl/content_store.bin` (Blake3-keyed).

Confirm heading modules are indexed:

```bash
rgctl -r "$REPO" -f json find --type module --count-only | jq '.total'
```

Use `inventory --by type` or `find --type module --count-only` (**33953** modules at time of writing; drifts with upstream).

**Fixture equivalent** (same command, tiny graph — useful before iterating on queries):

```bash
export REPO="$REPO_FIXTURE"
rgctl -r "$REPO" discover . -l markdown
```

Live fixture telemetry: **20** files → **109** nodes / **204** edges; `find --type module --count-only` → **29**; Obsidian export → **16** notes (7 wikilinks).

Markdown is included in default `discover`. Use `-l markdown` when you want **only** documentation.

### 2. Export to Obsidian

Turn every heading section into an interlinked vault. Export reads the graph and `content_store.bin` — it does not re-parse source files.

```bash
export REPO="$(pwd)/example/k8s-website"
export RGCTL_NO_DAEMON=1

rgctl -r "$REPO" export \
  --export-format obsidian \
  --export-output "$REPO/vault" \
  --query all
```

**Output:**

```
[>] rgctl export
Exported ~34k notes (2971 wikilinks) -> …/example/k8s-website/vault
[✓] rgctl export finished in 7.0s
```

**What happened:**

- rgctl exported **~34k notes** — one per heading module. Note count tracks `find --type module` (33953 at time of writing).
- Folder layout mirrors doc paths (e.g. `docs/concepts/…/feature.md#overview` → nested vault paths). Long blog titles get truncated slugs with a stable hash suffix.
- Outgoing `REFERENCES` edges became **2,971 wikilinks** (`[[path]]`, no `.md` suffix).
- Each note's YAML frontmatter includes `qualified_name` and `level` for trace-back to graph qualified names.

**Open the vault:** Obsidian → **Open folder as vault** → select `$REPO/vault`.

**Fixture equivalent:**

```bash
export REPO="$REPO_FIXTURE"
rgctl -r "$REPO" export --export-format obsidian --export-output "$REPO/vault" --query all
```

```
Exported 16 notes (7 wikilinks) -> …/markdown-context/vault
```

Re-export after doc edits: `discover`, then `export`. Obsidian export is read-only — vault edits are not synced back to the graph.

### 3. Query, communities, and semantic search

**Cross-links across the corpus:**

```bash
export REPO="$(pwd)/example/k8s-website"
export RGCTL_NO_DAEMON=1

rgctl -r "$REPO" -f json relations --edge references --from-type module --limit 20
```

**Communities** — community assignment is computed at `discover` and stored in analysis sidecars (not stored in `graph.snapshot.bin`). List communities, then filter heading modules by `community_id`:

```bash
rgctl -r "$REPO" -f json communities list | jq '.communities[:5]'
rgctl -r "$REPO" -f json inventory --by community | jq '.counts[:5]'
```

On markdown-only graphs, community detection uses `REFERENCES` cross-links and heading `CONTAINS` trees. See [Structured graph queries](structured-query.md) and [Community Detection](community-detection.md).

**PageRank (`metrics`)** — centrality scores are not `find` node fields. Use the metrics command after `discover`:

```bash
rgctl -r "$REPO" -f json metrics --pagerank
```

The `top` array lists node UUIDs and scores. For named hotspots, combine with `relations` on structure (`REFERENCES`, `CONTAINS`) or communities above.

**Doc-scoped semantic index** (separate from `discover`; uses `semantic_index.bin`):

```bash
rgctl -r "$REPO" semantic index --scope docs --embedder hash
```

**Output:**

```
[>] rgctl semantic index
Indexed 24608 functions (sign-hash-v1, 256 dims) → …/k8s-website/.rgctl/semantic_index.bin
  incremental: 0 reused, 24608 embedded, 0 removed
[✓] rgctl semantic index finished in 2.0s
```

The CLI still prints `functions` — the count is **index entries** (heading + code-block modules when `--scope docs`).

```bash
rgctl -r "$REPO" -f json semantic query "pod scheduling" --scope docs --limit 10
```

**What happened:**

- `--scope docs` embedded `:Module` nodes with `kind=heading` and `kind=code_block`.
- Embeddings use inline `body_text` or full UTF-8 from `content_store.bin` when `body_ref` is set.
- Query `--scope docs` does not filter hits — build the index with the scope you need. Re-run `semantic index` after large doc edits.

### 4. Export OKF JSON

```bash
export REPO="$(pwd)/example/k8s-website"
export RGCTL_NO_DAEMON=1

rgctl -r "$REPO" export \
  --export-format okf \
  --export-output "$REPO/okf.json" \
  --query all
```

**Output:**

```
[>] rgctl export
Exported ~34k OKF entities -> …/example/k8s-website/okf.json
[✓] rgctl export finished in 466ms
```

Use `--query all` for doc exports (filter queries target code-graph subsets). The fixture run is identical with `REPO="$REPO_FIXTURE"` (16 entities).

### 5. Feature corpus (markdown-context fixture)

Switch to the in-tree fixture for **every markdown construct** and **doc→code** queries. Layout:

```text
markdown-context/
  README.md              ← YAML frontmatter
  docs/
    guide.md             ← headings, links, fenced code
    adr.md               ← tables, cross-links
    overview.mdx         ← `.mdx`
  src/
    CheckoutService.java ← doc→class anchor
```

```bash
export REPO="$REPO_FIXTURE"
rgctl -r "$REPO" discover . -l markdown,java   # docs + code
```

**Structured queries on the fixture** (`LIKE` = prefix/suffix globs only):

```bash
# Checkout-related headings
rgctl -r "$REPO" -f json find 'Checkout*' --type module --limit 10

# Cross-doc link (guide → payments ADR)
rgctl -r "$REPO" -f json relations --edge references --from-type module --limit 20

# Doc → Java class (needs markdown,java discover)
rgctl -r "$REPO" -f json find CheckoutService --type class --exact

# Section title lookup
rgctl -r "$REPO" -f json find 'Checkout Flow' --type module --exact --limit 1
```

## Markdown showcase (fixture)

After `discover` on `$REPO_FIXTURE`, these checks confirm each supported construct.

### Headings and hierarchy

`docs/guide.md` — ATX headings (`# Checkout Flow`, `## Cart`, `### Validation rules`). Nested `CONTAINS`: Checkout Flow → Cart → Validation rules.

```bash
rgctl -r "$REPO_FIXTURE" -f json relations --edge contains --from-type module --to-type module --limit 20
```

### Internal links

`docs/guide.md#checkout-flow` links to `./adr.md#payments`, `./adr.md`, and `../src/CheckoutService.java`. External URLs (Stripe API) get link symbols but no `REFERENCES` edge.

```bash
rgctl -r "$REPO_FIXTURE" -f json relations --edge references --from-type module --limit 20
```

### Fenced and indented code

````markdown
```java
cart.validate();
```
````

```bash
rgctl -r "$REPO_FIXTURE" -f json find --type module --limit 5
```

### Frontmatter, tables, MDX

- `README.md` — YAML frontmatter → `:Variable` with flattened keys (`metadata.author`)
- `docs/adr.md` — pipe table with internal link in a cell
- `docs/overview.mdx` — same plugin as `.md`; JSX in fences is not executed

```bash
rgctl -r "$REPO_FIXTURE" -f json find --type variable --limit 10
```

## Author linking cheat sheet

| Author writes | Resolves to | Indexed? |
|---------------|-------------|----------|
| `./adr.md` | File `docs/adr.md` | Yes — `REFERENCES` → `:File` |
| `./adr.md#payments` | Module `docs/adr.md#payments` | Yes — `REFERENCES` → `:Module` |
| `#checkout-flow` | Same-file heading slug | Yes |
| `#Checkout Flow` | Literal fragment (often a stub) | Avoid — use slug |
| `../src/Foo.java` | File node | Yes (if file is in discover set) |
| `https://…` | — | Link symbol only; no edge |
| `![alt](img.png)` | — | Ignored |
| `[[Wiki]]` | — | Ignored |

If a linked file is not in the discover set, the `REFERENCES` edge is dropped.

## What is not supported

| Feature | Notes |
|---------|-------|
| CFG / PDG / `slice` / `inspect` / `cpg flows` on `.md` | No CFG grammar; commands reject markup paths |
| MDX component execution | Structure only |
| Images and wikilinks | Skipped |
| External URL graph edges | `https://` not `REFERENCES` |
| `blast-radius` on doc nodes | Calls-only; use `find` / `relations` for docs |
| Obsidian → source sync | Export is one-way |

## Profile gates (maintainers)

Cold discover and warm Obsidian export on kubernetes/website are gated in ignored tests (`example/k8s-website` in the repo's internal profile layout). These use **ceiling** baselines (not typical laptop timings):

```bash
cargo build --release --bin rgctl
cargo test --release --test cold_profile_gates k8s_website_markdown_cold_discover_within_baseline -- --ignored --nocapture
cargo test --release --test cold_profile_gates k8s_website_obsidian_export_to_vault -- --ignored --nocapture
```

Ceilings: cold discover 3.0s wall, warm Obsidian export 30.0s wall (+10% tolerance). Details: [markdown-context.md § Cold profile](../markdown-context.md#cold-profile-kuberneteswebsite).

## Export formats for documentation

| Format | Output | Best for |
|--------|--------|----------|
| `obsidian` | Directory of `.md` notes | Human browsing in Obsidian |
| `okf` | Single JSON entity bundle | OKF / knowledge-platform tooling |

Code-graph formats (`json`, `graphml`, `graphviz`, `mermaid`) also include doc nodes when present. See [Exporting Graphs](exporting-graphs.md).

## Benefits

- **Real-site corpus.** kubernetes/website `content/en` is the profile fixture — 17k+ heading modules with real cross-links.
- **One graph for docs and code.** ADRs, guides, and services share `graph.snapshot.bin` when discovered together.
- **Low-token agent queries.** `find` returns compact rows (name, qualified_name, file) without opening whole files.
- **Human-friendly export.** Obsidian vaults mirror heading hierarchy with wikilinks.
- **Large-corpus bodies.** `content_store.bin` holds section text beyond the 32 KiB inline cap.

## Related Guides

- [Installation](../installation.md) — `rgctl` binary, PATH, artifact layout
- [Discovering and Indexing a Codebase](discovering-and-indexing.md) — `discover` flags and artifacts
- [Structured graph queries](structured-query.md)
- [Exporting Graphs](exporting-graphs.md) — JSON, GraphML, Mermaid, and filter queries
- [Semantic Search](semantic-search.md) — function and doc-scoped NL search
- [Community Detection](community-detection.md) — how communities are detected and named
- [Markdown context reference](../markdown-context.md) — node model, cold profiles, dashboard filter
- [Agent recipes](../agent-recipes.md) — copy-paste doc queries for LLM agents
