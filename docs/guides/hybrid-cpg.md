# Hybrid CPG

## Introduction

The `cpg` command provides a **Hybrid Code Property Graph (CPG)** -- a unified facade that bridges the repository-level call graph (L_repo) with per-function CFG/PDG analysis (L_proc). It gives you a single interface for function lookups, call neighborhoods, field mutation tracking, data flow analysis, PDG inspection, program slicing, and CPG export.

The "hybrid" in the name refers to the two-resolution architecture: the coarse-grained repository graph provides call and containment relationships across the entire codebase, while the fine-grained per-function analysis provides control flow, data dependencies, and statement-level detail within each function.

## Use Cases

- **DTO safety analysis.** Use `cpg mutations` to find every place a data transfer object is modified, ensuring encapsulation.
- **Data flow tracing.** Use `cpg flows` to trace a variable through a function without needing to understand PDG internals.
- **Call neighborhood exploration.** Use `cpg calls` to see all incoming and outgoing calls for a function.
- **Function resolution.** Use `cpg function` to look up a function by name and check if deep analysis data is available.
- **CPG export.** Export the hybrid graph in GraphSON or GraphML format for external tools.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` with `--with-cfg`:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover . -l java --with-cfg
```

## Step-by-Step

### 1. Check CPG Status

First, verify that the CPG archive is available:

```bash
rgctl -r example/coolstore -f json cpg status
```

**Output:**

```json
{
  "archive_path": ".rgctl/analysis/cfg_pdg.archive.bin",
  "archive_present": true,
  "ast_skeleton_count": 0,
  "ast_skeleton_present": false,
  "field_write_count": 41,
  "field_write_index_present": true,
  "function_count": 150,
  "graph_digest": "ea1fe06c37a324653f95191c222a0416cb5c82dfdaffb1f3b419d50efe7d31da",
  "schema_version": 1
}
```

**What this tells you:**

- **`archive_present: true`** -- the CFG/PDG archive exists (from `discover --with-cfg`).
- **`function_count: 150`** -- 150 functions have been analyzed at the L_proc level.
- **`field_write_count: 41`** -- 41 field-write sites are indexed for mutation tracking.
- **`field_write_index_present: true`** -- the `cpg mutations` command is available.
- **`ast_skeleton_present: false`** -- the AST skeleton archive is not present (requires `discover --with-ast-skeleton`).

### 2. Resolve a Function

Look up a function by name and check if L_proc data exists:

```bash
rgctl -r example/coolstore -f json cpg function priceShoppingCart
```

**Output:**

```json
{
  "file_path": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
  "has_l_proc": true,
  "id": "16482c73-06a9-5dde-9755-3ccd8b98c5e9",
  "is_constructor": false,
  "name": "priceShoppingCart",
  "qualified_name": "com.redhat.coolstore.service.ShoppingCartService.priceShoppingCart",
  "schema_version": 1,
  "start_line": 54
}
```

**What this tells you:**

- **`has_l_proc: true`** -- this function has CFG/PDG data available for deep analysis.
- **`start_line: 54`** -- the function starts at line 54, useful for `slice` and `flows` commands.
- **`qualified_name`** -- the fully qualified name for unambiguous reference.

### 3. Call Neighborhood

See all functions called by (and calling) `priceShoppingCart`:

```bash
rgctl -r example/coolstore -f json cpg calls priceShoppingCart
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "function_id": "16482c73-06a9-5dde-9755-3ccd8b98c5e9",
  "function_name": "priceShoppingCart",
  "edges": [
    { "direction": "out", "id": "0d127bc6-bef7-5b17-bae3-1838040c2698", "name": "getShippingTotal" },
    { "direction": "out", "id": "116d5b35-0fa3-59e6-ba42-880aab46b46f", "name": "initShoppingCartForPricing" },
    { "direction": "out", "id": "24ef0bdb-ade9-5834-b76d-1314c3e224e4", "name": "applyShippingPromotions" },
    { "direction": "out", "id": "2a8cc6ce-5e32-53d1-bbd7-03adc8a76645", "name": "getShoppingCartItemList" },
    { "direction": "out", "id": "2dffc2a0-a88e-52d7-af65-7da937b4e2be", "name": "setCartItemTotal" },
    { "direction": "out", "id": "43bb6516-ac66-5122-bc10-e48758af6438", "name": "getCartItemTotal" },
    { "direction": "out", "id": "572088b3-5224-5da9-bdf6-e72e1c6b43b3", "name": "setCartItemPromoSavings" },
    { "direction": "out", "id": "5d348831-afab-5a41-b316-87216b4e4e73", "name": "lookupShippingServiceRemote" }
  ]
}
```

**What this tells you:**

- Top-level keys are `edges`, `function_id`, `function_name`, and `schema_version` (there is no `edges_sample` / `returned` wrapper).
- On clean CoolStore, `priceShoppingCart` has **17** neighborhood edges (incoming + outgoing).
- **`direction: "out"`** -- outgoing calls (callees). Incoming callers use `"in"`.
- The function calls setters/getters on `ShoppingCart`, `initShoppingCartForPricing`, promotion helpers, and shipping lookup.

### 4. Field Mutations

The `cpg mutations` command finds every place a type's fields are modified -- critical for DTO safety and encapsulation analysis:

```bash
rgctl -r example/coolstore -f json cpg mutations \
  --type ShoppingCart --exclude-ctors
```

**Output (live CoolStore `-l java --with-cfg`):**

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

**What this tells you:**

- The JSON envelope always includes `mutations`, `type_name`, `exclude_ctors`, `include_unresolved`, `member`, and `schema_version`.
- `cpg status` still reports `field_write_count: 41` for this corpus; type-filtered hits require a resolved `receiver_type` match. When the index rows are unresolved / unmatched, `mutations` is an empty array (use `--include-unresolved` only when you intentionally want the unresolved bucket).

### 5. Data Flows

Trace how a variable flows through a function:

```bash
rgctl -r example/coolstore -f json cpg flows \
  src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 68 --variable sc --function priceShoppingCart --direction forward
```

**Output:**

```json
{
  "direction": "forward",
  "file": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
  "function": "priceShoppingCart",
  "line": 68,
  "lines": [64, 68, 81, 83],
  "reduction_percent": 66.66666666666667,
  "schema_version": 1,
  "steps": [
    { "code": "for-each sc.getShoppingCartItemList()", "line": 64 },
    { "code": "sc.setCartItemTotal(sc.getCartItemTotal() + sci.getPrice() * sci.getQuantity())", "line": 68 },
    { "code": "ps.applyShippingPromotions(sc)", "line": 81 },
    { "code": "sc.setCartTotal(sc.getCartItemTotal() + sc.getShippingTotal())", "line": 83 }
  ],
  "variable": "sc"
}
```

The `flows` subcommand wraps `slice` with a simpler interface, showing the step-by-step data flow path through the function.

### 6. PDG Overlay

Inspect the program dependence graph for a function:

```bash
rgctl -r example/coolstore -f json cpg pdg priceShoppingCart
```

**Output (truncated):**

```json
{
  "schema_version": 1,
  "layer": "pdg",
  "symbol": "priceShoppingCart",
  "data_deps": 5,
  "control_deps": 19,
  "edges": [
    { "kind": "data", "source": "node_4", "target": "node_5", "variable": "sc" },
    { "kind": "data", "source": "node_7", "target": "node_8", "variable": "sci" },
    { "kind": "control", "source": "node_6", "target": "node_12" }
  ]
}
```

This shows **19** control dependencies and **5** data dependencies in `priceShoppingCart`, with the full edge list for graph analysis.

## CPG Subcommands Reference

| Subcommand | Purpose | Requires |
|------------|---------|----------|
| `status` | Check archive readiness | `discover` |
| `function` | Resolve a function by name | `discover` |
| `calls` | Call neighborhood (in/out) | `discover` |
| `mutations` | Field mutations for a type | `discover --with-cfg` |
| `flows` | Data/control flow tracing | `discover --with-cfg` |
| `pdg` | PDG overlay | `discover --with-cfg` |
| `slice` | Line-level slice (wraps `slice`) | `discover --with-cfg` |
| `ast` | AST skeleton | `discover --with-ast-skeleton` |
| `export` | Export CPG view | `discover --with-cfg` |

## Benefits

- **Unified interface.** One command family for both repository-level and function-level analysis.
- **DTO safety.** `mutations` gives you a complete audit of where a type's fields are changed.
- **Readable data flows.** `flows` presents data dependencies as a step-by-step narrative.
- **No source code reading.** Get call neighborhoods, mutations, and flows without loading files.
- **Exportable.** Export the hybrid CPG to GraphSON or GraphML for external analysis tools.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- `discover --with-cfg` is required for most CPG subcommands
- [Program Slicing](program-slicing.md) -- the underlying technique that `cpg flows` and `cpg slice` use
- [Inspecting CFG, PDG, and Dominance](inspecting-cfg-pdg-dominance.md) -- lower-level inspection of the same data
- [Blast Radius Analysis](blast-radius-analysis.md) -- function-level impact analysis from the same graph
