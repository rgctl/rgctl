# Agent pack (skills)

## Introduction

The rgctl **agent pack** teaches AI coding agents (Claude Code, Antigravity, Codex, Cursor, …) how to answer structural questions with the rgctl CLI. Install writes **one skill** named `rgctl`:

| Piece | What it is | Example (Cursor, repo-local) |
|-------|------------|------------------------------|
| **Skill `rgctl`** | Single skill + `references/` (command encyclopedia, workflow scenarios) | `.cursor/skills/rgctl/SKILL.md` |
| **Policy snippet** | Optional structural bias | `.cursor/rules/rgctl-structural.mdc` (`--with-policy`) |

The pack is **embedded in the `rgctl` binary**. `rgctl install --skill` installs the meta skill **`rgctl`**, additional skills declared with commands (today **`rgctl-review`**), and **slash commands** for adapters that support them (OpenSpec-style dual install — no separate `--with-commands` flag). Retired per-workflow skills (`rgctl-discover`, `rgctl-gql`, …) are **not** reintroduced. Agents load skills / invoke `/rgctl:review` and run `rgctl -f json` structured verbs. Scenario playbooks live under `references/workflows.md` inside the meta skill.

## Use Cases

The pack unlocks several workflows where structural graph analysis and AI reasoning combine:

### Refactoring with Confidence

Before renaming, extracting, or restructuring a function, the agent can automatically check the blast radius, trace callers, and verify that no hidden dependencies will break. Instead of manually running commands, you ask the agent a natural-language question and it handles the analysis.

### Monolith-to-Microservice Migration

The agent can generate a complete migration roadmap, explain the ordering rationale, identify high-risk extraction targets, and guide you through each step -- all from conversational prompts like "generate a migration plan for this repo" or "what should we extract first?"

### Porting a Function from One Language to Another

When rewriting a function from Java to Go (or any language pair), the agent can extract the function's data-flow graph, call neighborhood, and field mutations from the source language, then verify that the target-language implementation preserves the same logic and data-flow structure. The graph provides a language-independent ground truth.

### Writing Better Test Cases

The agent can analyze a function's control-flow graph, identify all branch paths, trace data dependencies, and use that structural information to generate test cases that cover the actual code paths rather than guessing at coverage.

### Continuous Architecture Review

With the pack installed, every code review conversation has access to architectural context. The agent can check policy compliance, detect coupling drift, and flag high-impact changes before they are merged.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` first:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java --with-cfg
```

## Step-by-Step

### 1. Install the pack

Install the `rgctl` skill into your repository:

```bash
rgctl -r example/coolstore install --skill --tools cursor,claude,codex,antigravity,agents
```

Text mode lists each created or updated path. Typical layout (Cursor example):

- `.cursor/skills/rgctl/SKILL.md` — the meta skill
- `.cursor/skills/rgctl/references/` — command encyclopedia, workflows, communities & policy
- `.cursor/skills/rgctl-review/SKILL.md` — PR/branch review protocol
- `.cursor/commands/rgctl-review.md` — slash command `/rgctl:review` (writes `REVIEW_REPORT.md`)

**What happened:**

- rgctl unpacked the embedded **agent pack** (generated at build time from `skills/rgctl/`, including `references/workflows.md`).
- **Claude** uses `.claude/skills/`.
- **Codex / agents / zed** share `.agents/skills/` (install dedupes).
- **Cursor** uses `.cursor/skills/`.
- Scenario prose lives in `skills/rgctl/references/workflows.md` and is copied into the installed skill.
- No network — content matches your `rgctl` binary version.

See [Install options reference](#install-options-reference) below for the full flag table and registry (`install --list-agents`).

### 2. Verify with JSON Output

Check the install status programmatically:

```bash
rgctl -r example/coolstore -f json install --skill --tools cursor \
  | jq '{schema_version, scope, agents, writes: [.writes[] | {agent, workflow, kind, status}]}'
```

**Output (schema version 3, abbreviated):**

```json
{
  "schema_version": 3,
  "command": "install",
  "skill": "rgctl",
  "repo": "/path/to/example/coolstore",
  "scope": "local",
  "agents": ["cursor"],
  "with_policy": false,
  "force": false,
  "writes": [
    {
      "agent": "cursor",
      "workflow": null,
      "kind": "meta",
      "path": "/path/to/example/coolstore/.cursor/skills/rgctl/SKILL.md",
      "status": "unchanged"
    }
  ]
}
```

The `status` field for each write is one of:
- `created` -- new file written
- `unchanged` -- file already exists with identical content (idempotent)
- `overwritten` -- existing file replaced (with `--force`)
- `skipped_exists` -- file differs but `--force` was not set (exit code 1)

### 3. Install for specific agents

Limit adapters with **`--tools`** (comma-separated registry ids):

```bash
rgctl -r example/coolstore install --skill --tools claude
rgctl -r example/coolstore install --skill --tools codex,agents
rgctl -r example/coolstore install --skill --tools antigravity
rgctl -r example/coolstore install --skill --tools cursor
rgctl install --list-agents   # all ids and paths
```

**`--host`** is deprecated; it still maps to a subset of tools but prints a warning.

### 4. Update After Upgrading rgctl

When you upgrade rgctl, the embedded skill may have changed. Update it with `--force`:

```bash
rgctl -r example/coolstore install --skill --force
```

This overwrites any existing skill files, even if they have been modified locally.

### 5. The Agent Loop

Once installed, the agent follows a 5-step loop for every structural question:

```
1. USER PROMPT     "What's the impact of changing priceShoppingCart?"
2. TOOL CALL       rgctl -f json blast-radius priceShoppingCart
3. GRAPH FACTS     Parse JSON: score 40.6, 6 callers, impact zone 12
4. LLM REASONING   Summarize: moderate risk, spans service and REST layers
5. ACTION          Report findings, suggest next steps
```

The skill includes a **decision table** that maps 19 categories of natural-language questions to the right rgctl command. The agent does not need to be told which command to use -- it matches the user's intent automatically.

---

## Use Case: Refactoring with Confidence

### Scenario

You want to refactor `priceShoppingCart` in the CoolStore application -- perhaps splitting it into separate pricing methods for items and shipping. Before making changes, you need to understand the full impact.

### What the Agent Does

When you ask the agent: *"What happens if I change priceShoppingCart?"*

The agent follows the skill's decision table (row 10: "Impact if I change X") and runs:

```bash
rgctl -r example/coolstore -f json blast-radius priceShoppingCart
```

**Output:**

```json
{
  "metrics": {
    "score": 40.6,
    "direct_callers_count": 6,
    "impact_zone_size": 12
  },
  "schema_version": 2,
  "target": {
    "canonical_fqn": "ShoppingCartService::priceShoppingCart",
    "file_path": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
    "id": "28de0bfd-1b19-5c3c-bcbf-26947d067654",
    "signature": "public void priceShoppingCart(ShoppingCart sc) {"
  },
  "topology": {
    "direct_callers": [
      {"fqn": "com.redhat.coolstore.rest.CartEndpoint.add"},
      {"fqn": "com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem"},
      {"fqn": "com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems"},
      {"fqn": "com.redhat.coolstore.rest.CartEndpoint.set"},
      {"fqn": "com.redhat.coolstore.rest.CartEndpoint.delete"},
      {"fqn": "com.redhat.coolstore.service.ShoppingCartService.checkOutShoppingCart"}
    ]
  }
}
```

The agent then reports:

> priceShoppingCart has a blast-radius score of 40.6/100 (moderate risk). It has 6 direct callers (checkOutShoppingCart, CartEndpoint add/set/delete/dedupeCartItems, and ShoppingCart.addShoppingCartItem). The impact zone is 12 functions. Changing the signature requires updating those callers.

### Going Deeper: Call Neighborhood

The agent can then trace the call neighborhood to understand what `priceShoppingCart` itself depends on:

```bash
rgctl -r example/coolstore -f json cpg calls priceShoppingCart
```

**Output (truncated):**

```json
{
  "function_id": "28de0bfd-1b19-5c3c-bcbf-26947d067654",
  "function_name": "priceShoppingCart",
  "schema_version": 1,
  "edges": [
    {"direction": "out", "id": "f3b1f695-a18e-54bf-be9f-24580254b25a", "name": "initShoppingCartForPricing"},
    {"direction": "out", "id": "7a403968-ae88-50cb-a12c-eb795bfad085", "name": "getShoppingCartItemList"},
    {"direction": "out", "id": "e007180a-4d55-5ddd-937d-69005d0f84c5", "name": "setCartItemPromoSavings"},
    {"direction": "out", "id": "6f3d4d58-027a-5937-bb45-43f79c11da93", "name": "setCartItemTotal"},
    {"direction": "out", "id": "df5f0e9c-fdae-51b2-a73c-2cbcae8ba265", "name": "setShippingTotal"},
    {"direction": "out", "id": "294f7da2-3bc2-55ab-9487-f86ae0568177", "name": "setCartTotal"},
    {"direction": "in", "id": "d0a70ef7-c170-5a07-90a2-c061373666c2", "name": "add"}
  ]
}
```

Keys are `edges`, `function_id`, `function_name`, `schema_version` (not `edges_sample` / `returned`). Live CoolStore returns **17** edges (outgoing callees plus inbound callers). The agent reports: *"priceShoppingCart participates in a dense call neighborhood — initializing pricing, iterating cart items for totals/promo savings, shipping lookups, and final cart total — with inbound REST/service callers. A refactoring that splits item pricing from shipping pricing should preserve these call relationships."*

### Verifying Field Mutations

Before splitting the function, check which fields it modifies:

```bash
rgctl -r example/coolstore -f json cpg mutations \
  --type ShoppingCart --exclude-ctors
```

**Output** (envelope keys; CoolStore currently returns an empty `mutations` array even though `cpg status` reports `field_write_count: 41` — type matching for Java setters is incomplete on this corpus):

```json
{
  "exclude_ctors": true,
  "include_unresolved": false,
  "member": null,
  "mutations": [],
  "schema_version": 1,
  "type_name": "ShoppingCart"
}
```

The agent should still cross-check field writes via `cpg status` / PDG / source when `mutations` is empty, and prefer `inspect` / `cpg flows` for pricing-field updates on `sc`.

---

## Use Case: Migration Planning

### Scenario

You need to decompose the CoolStore application into microservices and want a data-driven extraction plan.

### What the Agent Does

When you ask: *"Generate a migration plan for this codebase"*

The agent follows the skill's decision table (row 1: "Generate a migration plan") and runs:

```bash
rgctl -r example/coolstore discover -l java \
  --with-cfg --with-harmonic --export-migration-hints \
  --migration-preset hybrid_default --migration-order scheduled
```

Then reads the plan:

With `-f json`, discover stdout **is** the migration plan (`schema_version` 2, `steps`); the same JSON is written to `.rgctl/migration_plan.json`.

```json
{
  "schema_version": 2,
  "preset": "hybrid_default",
  "order_mode": "scheduled",
  "steps": [
    {
      "step": 1,
      "label": "com.redhat.coolstore.persistence",
      "priority_score": -0.17,
      "max_blast": 0.0
    },
    {
      "step": 2,
      "label": "com.redhat.coolstore.utils",
      "priority_score": 0.4095343470271542,
      "max_blast": 0.0
    },
    {
      "step": 3,
      "label": "com.redhat.coolstore.rest",
      "priority_score": 0.273298724649785,
      "max_blast": 0.0
    }
  ]
}
```

The agent reports:

> The migration plan contains 5 scheduled steps using the hybrid_default preset (balanced weighting of PageRank, harmonic centrality, and blast radius). First extraction targets are low-coupling packages such as `com.redhat.coolstore.persistence`, then utilities and REST, before denser service/model communities.

### Investigating a Migration Step

When the user asks: *"Tell me more about the coolstore model community"*

The agent queries community members:

```bash
rgctl -r example/coolstore -f json communities list
rgctl -r example/coolstore -f json semantic query "coolstore model" --scope community --limit 20
rgctl -r example/coolstore -f json blast-radius getShoppingCart --depth 3
```

And checks blast radius on key functions (already shown above).
---

## Use Case: Porting a Function to Another Language

### Scenario

You are migrating `priceShoppingCart` from Java to Go (or TypeScript, Rust, Python, etc.) and need to ensure the new implementation preserves the same data flow and logic.

### What the Agent Does

**Step 1: Extract the structural blueprint from the source language.**

The agent captures the function's data-flow graph, which is language-independent:

```bash
rgctl -r example/coolstore -f json cpg flows \
  ./src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 68 --variable sc --function priceShoppingCart --direction forward
```

**Output:**

```json
{
  "direction": "forward",
  "function": "priceShoppingCart",
  "steps": [
    {"code": "for-each sc.getShoppingCartItemList()", "line": 64},
    {"code": "sc.setCartItemTotal(sc.getCartItemTotal() + sci.getPrice() * sci.getQuantity())", "line": 68},
    {"code": "ps.applyShippingPromotions(sc)", "line": 81},
    {"code": "sc.setCartTotal(sc.getCartItemTotal() + sc.getShippingTotal())", "line": 83}
  ],
  "reduction_percent": 66.67
}
```

**Step 2: Extract the call neighborhood.**

```bash
rgctl -r example/coolstore -f json cpg calls priceShoppingCart
```

This gives the agent the complete list of outgoing calls that the new implementation must replicate.

**Step 3: Extract field mutations.**

```bash
rgctl -r example/coolstore -f json cpg mutations \
  --type ShoppingCart --exclude-ctors
```

This lists every field that `priceShoppingCart` writes to, which the new implementation must also write.

**Step 4: Extract the PDG.**

```bash
rgctl -r example/coolstore -f json cpg pdg priceShoppingCart
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "symbol": "priceShoppingCart",
  "layer": "pdg",
  "control_deps": 19,
  "data_deps": 5,
  "edges": [
    {"kind": "data", "source": "node_4", "target": "node_5", "variable": "sc"},
    {"kind": "data", "source": "node_7", "target": "node_8", "variable": "sci"},
    {"kind": "data", "source": "node_7", "target": "node_8", "variable": "sc"},
    {"kind": "data", "source": "node_7", "target": "node_8", "variable": "getQuantity"}
  ]
}
```

**How the agent uses this:**

The agent now has four language-independent facts about `priceShoppingCart`:

1. **Data flow path**: iteration over cart items, accumulation of totals, shipping calculation, final total.
2. **Call contract**: the outgoing callees from `cpg calls` (plus inbound callers in the same envelope).
3. **Mutation contract**: field writes on `ShoppingCart` (via PDG / flows when `cpg mutations` is empty).
4. **Dependency graph**: 19 control edges and 5 data edges encoding the computation structure.

When writing the Go implementation, the agent verifies each fact against the new code. If a data-flow step is missing, a call is omitted, or a field mutation is skipped, the agent flags it.

The agent can report: *"The Java implementation of priceShoppingCart has 4 data-flow steps, a dense call neighborhood (17 cpg call edges on CoolStore), and PDG deps (19 control / 5 data). Your Go implementation should replicate the same iteration pattern (for-each over cart items), accumulate the same totals, and call equivalent functions for shipping calculation and promo application."*

---

## Use Case: Writing Better Test Cases

### Scenario

You need to write tests for `priceShoppingCart` and want to ensure you cover all branch paths and edge cases.

### What the Agent Does

**Step 1: Examine the control-flow graph.**

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart cfg
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "symbol": "priceShoppingCart",
  "layer": "cfg",
  "pruned": false,
  "nodes": [
    {
      "id": "block_1",
      "block_index": 1,
      "start_line": 56,
      "end_line": 56,
      "statements": [{"kind": "Branch", "line": 56, "text": "sc != null"}]
    }
  ],
  "edges": [
    {"kind": "iftrue", "source": "block_1", "target": "block_2"},
    {"kind": "iffalse", "source": "block_1", "target": "block_0"},
    {"kind": "next", "source": "block_2", "target": "block_3"},
    {"kind": "iftrue", "source": "block_3", "target": "block_4"},
    {"kind": "iffalse", "source": "block_3", "target": "block_11"}
  ]
}
```

Keys are `edges`, `layer`, `nodes`, `pruned`, `schema_version`, `symbol` (12 nodes / 16 edges on CoolStore — not `node_count` / `edge_count` / `nodes_sample`). The agent identifies branch points from `nodes[].statements`:

> priceShoppingCart CFG starts with a null check (`sc != null` on block_1) and continues through list/loop/shipping branches — test with null, empty, single-item, multi-item, and shipping-threshold carts.

**Step 2: Examine data dependencies.**

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart pdg --edge-layer data
```

The agent identifies the key data flows:

> Variable `sc` flows through 3 data-dependency edges. Variable `sci` flows through the loop body. Test cases should verify that each accumulation step (cartItemTotal, cartItemPromoSavings, shippingTotal) produces correct values.

**Step 3: Check field mutations to verify test assertions.**

```bash
rgctl -r example/coolstore -f json cpg mutations \
  --type ShoppingCart --exclude-ctors
```

The agent reports: *"Your tests should assert on ShoppingCart pricing fields after calling priceShoppingCart (cartItemTotal, shippingTotal, cartTotal, promo savings) using PDG/flows when `cpg mutations` returns no rows. Here are the test cases derived from the CFG:"*

The agent can then generate concrete test cases:

1. **Null input**: call `priceShoppingCart(null)` -- no fields should be mutated.
2. **Empty cart**: cart with no items -- totals should be zero.
3. **Single item below threshold**: one item priced below 25 -- no shipping insurance.
4. **Single item above threshold**: one item priced at or above 25 -- shipping insurance applied.
5. **Multiple items with promotions**: verify promo savings accumulate correctly.
6. **Boundary case**: cart total exactly 25 -- verify threshold behavior.

---

## The NL-to-Command Decision Table

The skill embeds a decision table that maps natural-language patterns to CLI commands. Here are the key mappings:

| What You Ask the Agent | What the Agent Runs |
|------------------------|---------------------|
| "Generate a migration plan" | `discover . --with-cfg --with-harmonic --export-migration-hints` |
| "What are the bottlenecks?" | `metrics --pagerank` |
| "List all functions" | `find --type function --count-only` / `find --type function --limit 50` |
| "What communities exist?" | `communities list` |
| "Where is the checkout flow?" | `semantic index` then `semantic query "checkout flow"` |
| "What's the impact of changing X?" | `blast-radius X` |
| "Show the call stack around X" | `callers X --depth 3` / `callees X` |
| "Where is ShoppingCart mutated?" | `cpg mutations --type ShoppingCart` |
| "Trace variable X forward" | `cpg flows FILE --line N --variable X --direction forward` |
| "Validate against policies" | `check --policy-file policy.json` |

The agent handles disambiguation (e.g., adding `--class` or `--file` when a symbol name is ambiguous) and error recovery (e.g., running `discover --with-cfg` if slicing fails because CFG data is missing).

## Install options reference

```bash
rgctl [-r REPO] install [FLAGS]
```

You must pass at least one of **`--skill`** or **`--with-policy`**.

| Flag | Effect |
|------|--------|
| **`--skill`** | Install meta skill **`rgctl`**, companion skills (e.g. **`rgctl-review`**), and slash **commands** (e.g. `/rgctl:review`) for commands-enabled adapters. |
| **`--with-policy`** | Structural bias snippet (e.g. `.cursor/rules/rgctl-structural.mdc`). Optional; does not replace skills. |
| **`--tools id1,id2`** or **`--tools all`** | Which **registry adapters** receive files. **Default (omit flag):** `cursor`, `claude`, `codex`, `agents`, `antigravity`. **`all`** = full registry (~40 products). Unknown ids: stderr warning; if none valid, exit **1**. |
| **`-g` / `--global`** | Install under your **home** (e.g. `~/.cursor/skills/…`) instead of repo-local paths. Only agents with `supports_global: true` (see `--list-agents`). |
| **`--list-agents`** | Print the registry table and exit (no install). |
| **`--force`** | Overwrite rgctl-managed files that differ from the bundled version. |
| **`--host`** | **Deprecated** — use **`--tools`**. |
| **`-f json`** | Schema version **3** install payload (`scope`, `agents`, per-write `kind` / `status`). |

### Typical installs

```bash
cd /path/to/your-app

# Skills for common IDEs (repo-local)
rgctl install --skill --tools cursor,claude,codex,antigravity,agents

# Cursor only
rgctl install --skill --tools cursor

# Skills + Cursor structural policy
rgctl install --skill --tools cursor --with-policy

# User-home install (adapters that support -g)
rgctl install --skill -g --tools cursor
```

Install does **not** run `discover`. Index separately (`rgctl discover .`), then query with `rgctl -f json …`.

### What gets written

Paths come from **`agent-pack/agents/registry.toml`**.

| Kind | Example (Cursor, repo-local) |
|------|------------------------------|
| Skill | `.cursor/skills/rgctl/SKILL.md` + `references/` |
| Policy | `.cursor/rules/rgctl-structural.mdc` (with `--with-policy`) |

**Shared dedup:** `codex`, `agents`, and `zed` share `.agents/skills/` — install writes each destination once. Run `rgctl install --list-agents` for the full adapter table.

### Scenarios inside the skill

Intent → CLI mappings live in the skill’s NL routing table and `references/workflows.md` (discover, impact, flow, search, migrate, gate, vuln). There are **no** separate `rgctl-*` skill directories — one skill covers all of them.

**Migrate** uses `--export-migration-hints` → `.rgctl/migration_plan.json` as the agent-facing migration deliverable.

## How the pack is distributed

At **rgctl build** time, `rgctl-agent-pack-codegen` generates the pack from `agent-pack/manifest.yaml`, `agent-pack/agents/registry.toml`, and `skills/rgctl/` (including `references/workflows.md`), then embeds it as a zip in the binary. This means:

- No network access needed to install.
- Pack version matches the CLI version.
- Upgrading `rgctl` and running `install --skill --force` refreshes skills.
- Target repos do not need a checkout of the rgctl source tree.

## Benefits

- **Zero-configuration AI integration.** One command installs everything the agent needs to understand your codebase structurally.
- **Natural-language interface.** Developers ask questions in English; the agent translates to CLI commands.
- **Language-independent analysis.** Data-flow graphs, call neighborhoods, and mutation tracking work across all 9 Tier 1 languages.
- **Refactoring safety net.** Blast radius, call tracing, and mutation analysis catch breaking changes before they ship.
- **Test case generation.** CFG branch analysis and data dependencies produce higher-coverage tests.
- **Migration guidance.** The agent can generate, explain, and walk through a complete migration roadmap.
- **Cross-language porting.** Data-flow and dependency graphs provide a structural blueprint that transcends language syntax.
- **Always in sync.** The skill is embedded in the binary, so it always matches the CLI version.

## Related Guides

- [USER_AGENTS_TEMPLATE](../agents/USER_AGENTS_TEMPLATE.md) — paste into *another* repo as `AGENTS.md`
- [Installation](../installation.md) — binary install / PATH
- [Structured graph queries](structured-query.md) — `find` / `callers` / `relations`
- [Discovering and Indexing a Codebase](discovering-and-indexing.md) — the `discover` step that all agent queries depend on
- [Blast Radius Analysis](blast-radius-analysis.md) — the most common agent query for refactoring safety
- [Hybrid CPG](hybrid-cpg.md) — mutations, flows, and call neighborhoods used in porting and testing
- [Migration Planning](migration-planning.md) — the migration roadmap the agent can generate and explain
- [CI Policy Checks](ci-policy-checks.md) — policy validation the agent runs for continuous architecture review
- [JSON API §18](../json-api.md#18-install) — install JSON schema
