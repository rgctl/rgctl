# Inspecting CFG, PDG, and Dominance

## Introduction

The `inspect` command lets you examine the raw **Control-Flow Graph (CFG)**, **Program Dependence Graph (PDG)**, and **Dominator Tree** for any function in your codebase. These are the foundational data structures of program analysis -- the CFG shows how execution flows through a function, the PDG shows data and control dependencies between statements, and the dominator tree shows which basic blocks dominate others.

While higher-level commands like `slice` and `cpg flows` build on these structures, `inspect` gives you direct access to the raw graphs when you need to understand the internal structure of a function at the deepest level.

## Use Cases

- **Understand complex control flow.** Visualize how branches, loops, and exceptions create execution paths through a function.
- **Debug program analysis.** When a slice or flow result is unexpected, inspect the underlying CFG/PDG to understand why.
- **Academic and research use.** Access standard program analysis data structures for experimentation.
- **Security analysis.** Examine control-flow and data-dependency structures for potential vulnerabilities.
- **Compiler-style optimization analysis.** Use dominator trees and dominance frontiers for SSA-related analysis.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` with `--with-cfg`:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover . -l java --with-cfg
```

## Step-by-Step

### 1. Control-Flow Graph (CFG)

Inspect the CFG for `priceShoppingCart`:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart cfg
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "layer": "cfg",
  "symbol": "priceShoppingCart",
  "pruned": false,
  "nodes": [
    { "id": "block_0", "start_line": 0, "end_line": 0 },
    { "id": "block_1", "start_line": 56, "end_line": 56 },
    { "id": "block_2", "start_line": 58, "end_line": 58 },
    { "id": "block_3", "start_line": 60, "end_line": 60 }
  ],
  "edges": [
    { "kind": "iftrue", "source": "block_1", "target": "block_2" },
    { "kind": "iffalse", "source": "block_1", "target": "block_0" },
    { "kind": "next", "source": "block_2", "target": "block_3" },
    { "kind": "iftrue", "source": "block_3", "target": "block_4" }
  ]
}
```

**What this tells you:**

- Top-level keys are `nodes`, `edges`, `layer`, `symbol`, `pruned`, and `schema_version` (there is no `node_count` / `nodes_sample` summary field — count the arrays).
- On clean CoolStore (`-l java --with-cfg`), `priceShoppingCart` has **12** CFG blocks and **16** edges.
- Edge kinds such as `iftrue` / `iffalse` / `next` / `jump` describe branch and fall-through control flow.

### 2. Program Dependence Graph (PDG)

Inspect data and control dependencies for the same function:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart pdg
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "layer": "pdg",
  "symbol": "priceShoppingCart",
  "data_deps": 5,
  "control_deps": 19,
  "nodes": [
    { "id": "node_0" },
    { "id": "node_1" }
  ],
  "edges": [
    { "kind": "data", "source": "node_4", "target": "node_5", "variable": "sc" },
    { "kind": "data", "source": "node_7", "target": "node_8", "variable": "sci" },
    { "kind": "control", "source": "node_6", "target": "node_12" }
  ]
}
```

**What this tells you:**

- **`data_deps` / `control_deps`** summarize dependence counts (here **5** data and **19** control).
- Data edges carry a `variable` name (e.g. `sc`, `sci`); control edges link statement nodes that gate execution.
- The PDG is the substrate for `slice` and `cpg flows`.

### 3. Dominator Tree

Inspect immediate dominators for each CFG block:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart dom
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "layer": "dom",
  "symbol": "priceShoppingCart",
  "idom": [
    { "block": 11, "immediate_dominator": 1 },
    { "block": 4, "immediate_dominator": 2 },
    { "block": 6, "immediate_dominator": 1 },
    { "block": 8, "immediate_dominator": 6 },
    { "block": 0, "immediate_dominator": 1 },
    { "block": 1, "immediate_dominator": 1 }
  ],
  "frontiers": [],
  "nodes": [
    { "id": "block_0" },
    { "id": "block_1" }
  ]
}
```

**What this tells you:**

- **`idom`** -- the immediate dominator for each basic block. Block `1` dominates many successors (entry-side control).
- **`frontiers`** -- dominance frontiers (empty unless you request them with `--frontiers`).
- The dominator tree is rooted at the entry block; each block's immediate dominator is the closest block that must execute before it on every path.

### 4. Dominance Frontiers

Request dominance frontiers explicitly:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart dom --frontiers
```

Dominance frontiers identify the join points in the CFG where phi functions would be needed in SSA form.

### 5. Pruned CFG

For a cleaner view, prune unreachable blocks:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart cfg --prune
```

### 6. PDG with Edge Layer Filtering

Filter PDG edges by layer:

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart pdg --edge-layer data
```

This shows only data-dependency edges, filtering out control dependencies for a cleaner view.

```bash
rgctl -r example/coolstore -f json inspect priceShoppingCart pdg --def-use
```

The `--def-use` flag adds def-use chain information to the edges.

## Inspect Subcommands

| Subcommand | Description | Key Options |
|------------|-------------|-------------|
| `cfg` | Control-flow graph | `--prune` (remove unreachable blocks) |
| `pdg` | Program dependence graph | `--edge-layer` (filter by data/control), `--def-use` |
| `dom` | Dominator tree | `--frontiers` (show dominance frontiers) |

## Benefits

- **Full transparency.** See exactly the data structures that power slicing, flow analysis, and the CPG.
- **Standard representations.** CFG, PDG, and dominator trees are well-understood program analysis structures with extensive academic literature.
- **Debugging aid.** When higher-level commands produce unexpected results, inspect the underlying graphs to understand why.
- **Multi-format output.** Use `-f json` for programmatic access or `-f graphviz` for visualization.
- **Function-level precision.** Each graph is scoped to a single function, keeping the output manageable.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- `discover --with-cfg` is required
- [Program Slicing](program-slicing.md) -- slicing operates on the PDG that `inspect` exposes
- [Hybrid CPG](hybrid-cpg.md) -- the CPG facade that combines these graphs with the call graph
- [Blast Radius Analysis](blast-radius-analysis.md) -- function-level analysis that complements statement-level inspection
