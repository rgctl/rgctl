# Program Slicing

## Introduction

The `slice` command performs **line-level program slicing** -- it extracts the minimal set of statements in a function that affect (or are affected by) a specific variable at a specific line. This is a classic program analysis technique that lets you focus on exactly the code that matters for understanding a data flow, without reading the entire function.

Slicing works in two directions: **backward** (what statements contributed to this variable's value?) and **forward** (what statements does this variable's value influence?).

## Use Cases

- **Bug investigation.** Trace a variable's value backward to find where an incorrect computation originates.
- **Impact analysis at the statement level.** Before changing a line, see which other lines in the function would be affected.
- **Understanding complex functions.** Reduce a long function to just the statements relevant to a specific variable.
- **Security review.** Trace untrusted input forward to see where it flows within a function.
- **Code comprehension.** Focus on the minimal subset of code that produces a particular output.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` with `--with-cfg` to enable CFG/PDG analysis:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java --with-cfg
```

The `--with-cfg` flag is required because slicing depends on the Program Dependence Graph (PDG), which is built from the control-flow graph.

## Step-by-Step

### 1. Backward Slice

Find all statements that contribute to the value of `sc` at line 68 in `ShoppingCartService.java`:

```bash
rgctl -r example/coolstore -f json slice \
  ./src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 68 --variable sc --function priceShoppingCart --direction backward
```

**Output:**

```json
{
  "criterion": {
    "line": 68,
    "variable": "sc"
  },
  "direction": "backward",
  "edges": [
    {
      "kind": "data",
      "source": "node_0",
      "target": "node_1",
      "variable": "sc"
    },
    {
      "kind": "data",
      "source": "node_0",
      "target": "node_1",
      "variable": "sci"
    },
    {
      "kind": "data",
      "source": "node_0",
      "target": "node_1",
      "variable": "getQuantity"
    }
  ],
  "file": "./src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
  "lines": [
    66,
    68
  ],
  "nodes": [
    {
      "id": "node_0",
      "kind": "Expression",
      "label": "sc.setCartItemPromoSavings(\n                            sc.getCartItemPromoSavings() + sci.getPromoSavings() * sci.getQuantity())",
      "line": 66
    },
    {
      "id": "node_1",
      "kind": "Expression",
      "label": "sc.setCartItemTotal(sc.getCartItemTotal() + sci.getPrice() * sci.getQuantity())",
      "line": 68
    }
  ],
  "reduction_percent": 83.33333333333334,
  "schema_version": 1
}
```

**What this tells you:**

- **`lines: [66, 68]`** -- only 2 lines out of the entire function are relevant to the value of `sc` at line 68.
- **`reduction_percent: 83.3%`** -- the slice eliminated 83% of the function's statements, leaving only the minimal relevant subset.
- **`nodes`** -- the actual code at each line. Line 66 sets `cartItemPromoSavings` on `sc`, and line 68 sets `cartItemTotal` -- both modify the `sc` object.
- **`edges`** -- the data-dependency edges connecting the nodes. The variable `sc` flows from node_0 to node_1, along with `sci` and `getQuantity`.
- The backward direction means: "what code contributes to `sc`'s state at line 68?"

### 2. Forward Slice

Trace how the variable `sc` at line 68 influences later statements:

```bash
rgctl -r example/coolstore -f json cpg flows \
  ./src/main/java/com/redhat/coolstore/service/ShoppingCartService.java \
  --line 68 --variable sc --function priceShoppingCart --direction forward
```

**Output:**

```json
{
  "direction": "forward",
  "file": "./src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
  "function": "priceShoppingCart",
  "line": 68,
  "lines": [64, 68, 81, 83],
  "reduction_percent": 66.66666666666667,
  "schema_version": 1,
  "steps": [
    {
      "code": "for-each sc.getShoppingCartItemList()",
      "line": 64
    },
    {
      "code": "sc.setCartItemTotal(sc.getCartItemTotal() + sci.getPrice() * sci.getQuantity())",
      "line": 68
    },
    {
      "code": "ps.applyShippingPromotions(sc)",
      "line": 81
    },
    {
      "code": "sc.setCartTotal(sc.getCartItemTotal() + sc.getShippingTotal())",
      "line": 83
    }
  ],
  "variable": "sc"
}
```

**What this tells you:**

- **`lines: [64, 68, 81, 83]`** -- 4 lines are influenced by `sc` at line 68.
- **`reduction_percent: 66.7%`** -- two-thirds of the function is irrelevant to this data flow.
- **`steps`** -- the flow path: the for-each loop (line 64), the cart item total calculation (line 68), shipping promotions (line 81), and the final cart total (line 83).
- This shows the complete computation chain from item pricing through to the final cart total.

## Slice Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `FILE` | Yes | Path to the source file (relative to `--repo`) |
| `--line` | Yes | Line number of the slicing criterion |
| `--variable` | Yes | Variable name at the criterion line |
| `--function` | No | Function name (disambiguates if multiple functions span the line) |
| `--direction` | No | `backward` (default) or `forward` |
| `--taint` | No | Enable taint tracking on the slice |
| `--view` | No | `text` (default), `cfg`, or `pdg` |
| `--language` | No | Override language detection |

## Understanding the Output

| Field | Meaning |
|-------|---------|
| `lines` | The line numbers included in the slice |
| `reduction_percent` | How much of the function was eliminated (higher = more focused) |
| `nodes` | PDG nodes with code labels and line numbers |
| `edges` | Data and control dependency edges between nodes |
| `criterion` | The starting point (line + variable) of the slice |

## Benefits

- **Precision.** Only see the statements that actually matter for a specific data flow.
- **Quantified focus.** The `reduction_percent` tells you exactly how much irrelevant code was eliminated.
- **Bidirectional.** Trace data backward to its origins or forward to its consumers.
- **Statement-level granularity.** More precise than function-level blast-radius analysis.
- **Bug isolation.** Quickly narrow down where a value was computed incorrectly.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- must run `discover --with-cfg` for slicing
- [Hybrid CPG](hybrid-cpg.md) -- `cpg flows` wraps slicing with a higher-level interface
- [Inspecting CFG, PDG, and Dominance](inspecting-cfg-pdg-dominance.md) -- examine the raw PDG that slicing uses
- [Blast Radius Analysis](blast-radius-analysis.md) -- function-level impact analysis (vs. statement-level slicing)
