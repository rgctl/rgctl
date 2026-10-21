# Graph Metrics

## Introduction

The `metrics` command runs **network analytics** on your code knowledge graph: PageRank to find the most important functions, betweenness centrality to identify bridging nodes, and community detection to measure the modularity of your codebase. These are the same algorithms used to analyze web link graphs and social networks, applied to the structure of your code.

Metrics give you a quantitative view of your codebase's architecture -- which functions are critical hubs, which serve as bridges between modules, and how well the code separates into distinct clusters.

## Use Cases

- **Identify critical functions.** PageRank reveals the most structurally important functions -- the ones most depended upon.
- **Find architectural bottlenecks.** High betweenness centrality indicates functions that bridge otherwise disconnected parts of the codebase.
- **Measure modularity.** Community detection tells you how well your code separates into cohesive groups, quantified by a modularity score.
- **Prioritize refactoring.** Focus effort on high-PageRank, high-betweenness functions where changes have the largest structural impact.
- **Track architecture over time.** Run metrics on each release to detect modularity drift or emerging hotspots.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` first:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java
```

## Step-by-Step

### 1. PageRank

PageRank ranks functions by structural importance -- how many other functions depend on them, directly or transitively. Functions called by many high-PageRank callers themselves receive a high score.

```bash
rgctl -r example/coolstore -f json metrics --pagerank
```

**Output (truncated):**

```json
{
  "pagerank": {
    "converged": true,
    "iterations": 9,
    "max_delta": 3.6429738108079146e-07,
    "top": [
      {
        "file_path": "<external>",
        "kind": "interface",
        "name": "Serializable",
        "node": "f3549ab7-e2f9-5f39-bd10-0a704c9254b0",
        "pagerank": 0.03165890535246511,
        "qualified_name": "java.io.Serializable"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/model/CatalogItemEntity.java",
        "kind": "class",
        "name": "CatalogItemEntity",
        "node": "9da23d53-5e6e-5267-99d2-a6069d8e2e8f",
        "pagerank": 0.006735939393751206,
        "qualified_name": "com.redhat.coolstore.model.CatalogItemEntity"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/model/Order.java",
        "kind": "class",
        "name": "Order",
        "node": "3f133891-b72c-5d9d-bd7a-daa4e444cd7a",
        "pagerank": 0.0067091538522855675,
        "qualified_name": "com.redhat.coolstore.model.Order"
      }
    ]
  },
  "schema_version": 1
}
```

**What this tells you:**

- **`converged: true`** -- PageRank converged in 9 iterations on this Java-only graph.
- **`top`** -- highest-ranked nodes (often types / external stubs such as `Serializable`, then domain classes like `CatalogItemEntity` / `Order`).
- Each entry includes `node` (UUID), `name`, `kind`, `qualified_name`, and `file_path` when available.

To increase iterations for better convergence:

```bash
rgctl -r example/coolstore -f json metrics --pagerank --iterations 50
```

### 2. Betweenness Centrality

Betweenness centrality measures how often a function lies on the shortest path between two other functions. High-betweenness functions are architectural bridges -- removing them would disconnect parts of the call graph.

```bash
rgctl -r example/coolstore -f json metrics --betweenness
```

**Output (truncated):**

```json
{
  "betweenness": [
    {
      "file_path": "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
      "kind": "function",
      "name": "add",
      "node": "d0a70ef7-c170-5a07-90a2-c061373666c2",
      "qualified_name": "com.redhat.coolstore.rest.CartEndpoint.add",
      "score": 0.0009528547582929628
    },
    {
      "file_path": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
      "kind": "function",
      "name": "priceShoppingCart",
      "node": "28de0bfd-1b19-5c3c-bcbf-26947d067654",
      "qualified_name": "com.redhat.coolstore.service.ShoppingCartService.priceShoppingCart",
      "score": 0.00073808863588171
    }
  ],
  "schema_version": 1
}
```

**What this tells you:**

- Functions with the highest betweenness scores are the most critical bridges in the call graph (here: `CartEndpoint.add`, then `priceShoppingCart`).
- These are the functions where a bug or breaking change would propagate most widely across otherwise separate modules.
- Low betweenness means a function is "internal" to a single cluster.

### 3. Community Detection

Community detection partitions the graph into clusters of tightly connected functions using the Louvain algorithm. The `modularity` score (0--1) measures how well the code separates into distinct groups.

```bash
rgctl -r example/coolstore -f json metrics --communities
```

**Output:**

```json
{
  "communities": {
    "assignments": 608,
    "count": 473,
    "modularity": 0.3893411219138413
  },
  "schema_version": 1
}
```

**What this tells you:**

- **`count: 473`** -- the algorithm identified ~473 distinct communities on the Java-only CoolStore graph.
- **`assignments: 608`** -- all 608 nodes in the graph were assigned to a community.
- **`modularity: ~0.39`** -- values above 0.3 indicate meaningful community structure; values above 0.5 indicate strong modularity. The CoolStore application has moderate modularity, consistent with its nature as a single-deployment application.

### 4. Combining Metrics

You can run multiple metrics in a single command:

```bash
rgctl -r example/coolstore -f json metrics --pagerank --betweenness --communities
```

This returns all three analyses in a single JSON response.

### 5. Resolving Node UUIDs

The metrics output uses node UUIDs. To find out which function a UUID refers to, query the graph:

```bash
rgctl -r example/coolstore -f json find priceShoppingCart --type function --exact
```

Or use `blast-radius` which resolves names automatically:

```bash
rgctl -r example/coolstore blast-radius priceShoppingCart
```

## Understanding the Algorithms

| Metric | What It Measures | High Score Means |
|--------|-----------------|-----------------|
| **PageRank** | Recursive importance via incoming edges | Many important callers depend on this function |
| **Betweenness** | Frequency on shortest paths between pairs | This function bridges otherwise disconnected modules |
| **Community modularity** | Quality of graph partitioning | The codebase has strong, well-separated functional clusters |

## Benefits

- **Quantitative architecture analysis.** Replace subjective assessments with concrete scores.
- **Identify hotspots.** High-PageRank and high-betweenness functions are where bugs hurt most and refactoring pays off most.
- **Measure modularity.** Track whether your codebase is becoming more or less modular over time.
- **Standard algorithms.** PageRank, betweenness centrality, and Louvain community detection are well-understood network science tools.
- **Fast.** All metrics run in-memory on the graph snapshot, typically completing in under a second.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- must run `discover` before metrics
- [Community Detection](community-detection.md) -- dig deeper into community analysis
- [Blast Radius Analysis](blast-radius-analysis.md) -- per-function impact analysis that complements metrics
- [Migration Planning](migration-planning.md) -- metrics feed into migration ordering
