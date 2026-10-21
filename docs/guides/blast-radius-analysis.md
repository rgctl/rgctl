# Blast Radius Analysis

## Introduction

The `blast-radius` command measures the **upstream impact** of changing a function. It answers the question: "If I modify this function, what other parts of the codebase could be affected?" It does this by walking the call graph backwards from the target function, collecting every direct caller and the full transitive impact zone.

The result is a **blast-radius score** (0--100), a list of direct callers, and the complete impact zone -- everything you need to make an informed decision about the risk of a code change.

## Use Cases

- **Pre-change risk assessment.** Before modifying a function, check how many callers depend on it and how far the impact propagates.
- **Code review prioritization.** Focus review effort on changes to high-blast-radius functions.
- **Refactoring safety.** Identify all the code paths that exercise a function before extracting, renaming, or modifying it.
- **Migration planning.** Understand which functions are safe to extract into microservices (low blast radius) versus those that are deeply coupled (high blast radius).
- **CI guardrails.** Combine with `--policy-file` to fail builds when changes affect too many downstream consumers.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` first:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java
```

## Step-by-Step

### 1. Basic Blast Radius (Text)

Check the impact of the `priceShoppingCart` method:

```bash
rgctl -r example/coolstore blast-radius priceShoppingCart
```

**Output:**

```
Blast radius for 'priceShoppingCart'
  Score: 40.6/100
  Direct callers: 6
  Impact zone: 12
  Callers: com.redhat.coolstore.rest.CartEndpoint.add, com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem, com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems, com.redhat.coolstore.rest.CartEndpoint.set, com.redhat.coolstore.rest.CartEndpoint.delete, com.redhat.coolstore.service.ShoppingCartService.checkOutShoppingCart
  Impact: com.redhat.coolstore.rest.CartEndpoint.add, com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem, com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems, com.redhat.coolstore.utils.Transformers.shoppingCartToJson, com.redhat.coolstore.service.ShoppingCartOrderProcessor.process, com.redhat.coolstore.service.ShoppingCartService.checkOutShoppingCart, com.redhat.coolstore.rest.CartEndpoint.checkout, com.redhat.coolstore.rest.CartEndpoint.delete, com.redhat.coolstore.rest.CartEndpoint.set, com.redhat.coolstore.utils.Transformers.jsonToOrder, com.redhat.coolstore.service.InventoryNotificationMDB.onMessage, com.redhat.coolstore.service.OrderServiceMDB.onMessage
```

**What this tells you:**

- **Score: 40.6/100** -- a moderate impact score. This function is called by several REST endpoints and the checkout flow.
- **6 direct callers** -- six functions call `priceShoppingCart` directly.
- **Impact zone: 12** -- the full transitive closure affects 12 functions, meaning a change here could ripple through 12 code paths.
- The callers span `ShoppingCartService`, `CartEndpoint` (REST), and `ShoppingCart` (model), showing that this function bridges service, API, and domain layers.

### 2. JSON Output

For machine consumption or scripting, use `-f json`:

```bash
rgctl -r example/coolstore -f json blast-radius priceShoppingCart
```

**Output (truncated):**

```json
{
  "metrics": {
    "direct_callers_count": 6,
    "impact_zone_size": 12,
    "score": 40.6
  },
  "schema_version": 2,
  "target": {
    "canonical_fqn": "ShoppingCartService::priceShoppingCart",
    "class_context": "ShoppingCartService",
    "file_path": "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
    "id": "28de0bfd-1b19-5c3c-bcbf-26947d067654",
    "language": "java",
    "signature": "public void priceShoppingCart(ShoppingCart sc) {",
    "symbol": "priceShoppingCart"
  },
  "topology": {
    "direct_callers": [
      {
        "file_path": "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
        "fqn": "com.redhat.coolstore.rest.CartEndpoint.add",
        "id": "d0a70ef7-c170-5a07-90a2-c061373666c2"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/model/ShoppingCart.java",
        "fqn": "com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem",
        "id": "9ce1fde8-1ebf-5fae-8d09-db90fab10830"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
        "fqn": "com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems",
        "id": "5318808f-a0e2-551c-b6f7-d15a271d089e"
      }
    ],
    "impact_zone": [
      {
        "file_path": "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
        "fqn": "com.redhat.coolstore.rest.CartEndpoint.add",
        "id": "d0a70ef7-c170-5a07-90a2-c061373666c2"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/model/ShoppingCart.java",
        "fqn": "com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem",
        "id": "9ce1fde8-1ebf-5fae-8d09-db90fab10830"
      },
      {
        "file_path": "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
        "fqn": "com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems",
        "id": "5318808f-a0e2-551c-b6f7-d15a271d089e"
      }
    ]
  }
}
```

The JSON output provides full detail: the target function's signature, file path, language, and unique ID; each caller and impact-zone member with their FQNs and file paths; and the SCC (strongly connected component) ID for cycle detection.

### 3. Limiting Depth

By default, blast radius computes the full transitive closure. To limit analysis to a specific number of call hops:

```bash
rgctl -r example/coolstore blast-radius getShoppingCart --depth 3
```

**Output:**

```
Blast radius for 'getShoppingCart'
  Score: 40.6/100
  Direct callers: 7
  Impact zone: 13
  Callers: com.redhat.coolstore.rest.CartEndpoint.add,
           com.redhat.coolstore.model.ShoppingCart.addShoppingCartItem,
           com.redhat.coolstore.rest.CartEndpoint.dedupeCartItems,
           com.redhat.coolstore.rest.CartEndpoint.set,
           com.redhat.coolstore.rest.CartEndpoint.getCart,
           com.redhat.coolstore.rest.CartEndpoint.delete,
           com.redhat.coolstore.service.ShoppingCartService.checkOutShoppingCart
```

The `--depth 3` flag limits the impact zone to at most 3 levels of callers, useful for large graphs where the full closure is too expensive or noisy.

### 4. Low Blast Radius Example

Compare with a constructor, which typically has zero callers:

```bash
rgctl -r example/coolstore blast-radius ShoppingCartService
```

**Output:**

```
Blast radius for 'ShoppingCartService'
  Score: 0.0/100
  Direct callers: 0
  Impact zone: 0
```

A score of 0 means this function (the constructor) has no upstream callers in the graph -- it is safe to modify in isolation.

### 5. Blast Radius with Policy

Use `--policy-file` to enforce thresholds:

```bash
cat > /tmp/coolstore-ci-policy.json <<'EOF'
{"max_impact_nodes": 15, "centrality_alert_threshold": 0.15}
EOF
rgctl -r example/coolstore -f json blast-radius priceShoppingCart \
  --policy-file /tmp/coolstore-ci-policy.json
```

The policy file defines rules like maximum impact zone size. If `priceShoppingCart` violates any rule, the `violations` array in the output will be populated and the exit code will be non-zero. (CoolStore does not ship a `policy.json` in-tree.)

## Understanding the Score

The blast-radius score (0--100) combines:

- **Direct callers count** -- how many functions call the target directly
- **Impact zone size** -- how many functions are in the full transitive closure
- **Graph centrality** -- the target's position in the overall call graph

A score of 0 means the function is isolated. Scores above 50 indicate high-impact functions that should be changed with caution.

## Benefits

- **Quantified risk.** Replace intuition with a concrete score before making changes.
- **Full topology.** See not just who calls a function, but the entire upstream ripple effect.
- **Multi-format output.** Text for quick checks, JSON for CI pipelines and agent workflows.
- **Depth limiting.** Control the analysis scope for large codebases.
- **Policy integration.** Combine with `check` to enforce blast-radius thresholds in CI.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- must run `discover` before blast-radius
- [CI Policy Checks](ci-policy-checks.md) -- enforce blast-radius policies in CI
- [Structured graph queries](structured-query.md) -- `callers`, `callees`, and `relations --edge calls`
- [Hybrid CPG](hybrid-cpg.md) -- combine blast-radius with per-function CPG analysis
- [Migration Planning](migration-planning.md) -- blast-radius scores feed into migration ordering
