# rgctl Guides

Practical, step-by-step guides for every major rgctl feature.

> **Which fixture?**  
> - **In-tree quickstart:** `rgctl-tests/ecommerce-java` (ships with this repo — no extra clone). Best for first `discover` / `find` / `blast-radius` runs and the [User Guide](../user-guide.md).  
> - **Full feature guides:** **CoolStore** (`example/coolstore` or [konveyor-ecosystem/coolstore](https://github.com/konveyor-ecosystem/coolstore)) — larger Java EE app used in the how-tos below.

## Guides

| Guide | Feature | Description |
|-------|---------|-------------|
| [Discovering and Indexing a Codebase](discovering-and-indexing.md) | `discover` | Build the knowledge graph from source code |
| [Structured graph queries](structured-query.md) | `find` / `callers` / `relations` / … | Agent-facing graph exploration verbs |
| [Blast Radius Analysis](blast-radius-analysis.md) | `blast-radius` | Measure upstream impact before changing a function |
| [Semantic Search](semantic-search.md) | `semantic` | Natural-language search over function symbols |
| [Graph Metrics](graph-metrics.md) | `metrics` | PageRank, betweenness, and community detection analytics |
| [Community Detection](community-detection.md) | `communities` | Identify and label functional clusters in your codebase |
| [Program Slicing](program-slicing.md) | `slice` | Extract the minimal set of statements affecting a variable |
| [Hybrid CPG](hybrid-cpg.md) | `cpg` | Combined call-graph + per-function CFG/PDG analysis |
| [Inspecting CFG, PDG, and Dominance](inspecting-cfg-pdg-dominance.md) | `inspect` | Examine low-level control flow, data dependence, and dominator trees |
| [Exporting Graphs](exporting-graphs.md) | `export` | Serialize graph data to JSON, GraphML, Graphviz, Mermaid, or Obsidian |
| [Markdown Context Graph](markdown-context-graph.md) | `discover -l markdown` · `export` | Index docs, Obsidian/OKF export, fixture feature tour (k8s-website scale example) |
| [CI Policy Checks](ci-policy-checks.md) | `check`, `pr-check` | PR temporal gates and local policy checks |
| [Pull Request Review](pr-review.md) | `review paths`, `review check` | Structural before/after call-path inspection and PR gates |
| [HTTP Server and Dashboard](http-server-and-dashboard.md) | `serve` | Run an HTTP API and browser-based dashboard |
| [Watch mode and incremental update](watch-mode.md) | `update`, `serve --watch` | Keep the graph fresh without full rediscover |
| [Migration Planning](migration-planning.md) | `discover --export-migration-hints` | Generate a dependency-aware migration roadmap |
| [Clone Detection](clone-detection.md) | `clones` | Detect exact (Type-1), bloom, and sub-function fragment clones |
| [Vulnerability and OSV Supply-Chain Analysis](vulnerability-and-osv-analysis.md) | `vuln`, `deps`, `taint`, `security` | End-to-end OSV advisory triage, manifest scanning, call-path reachability, and OpenVEX generation |
| [Agent pack](agent-skill.md) | CoolStore install walkthrough | Use cases; NL → `rgctl` skill → CLI |

## Prerequisites

All guides assume you have rgctl installed. See the [Installation guide](../installation.md) to get started.

To follow the examples, clone the repository and navigate to the example project:

```bash
git clone https://github.com/konveyor-ecosystem/coolstore.git
cd coolstore
```

## Related Resources

- [User Guide](../user-guide.md) -- full CLI reference and tutorial
- [JSON API Reference](../json-api.md) -- schema details for `-f json` output
- [Agent pack](agent-skill.md) -- install skills and route natural-language intents to `rgctl`
- [Introduction](../Introduction.md) -- definitions of key terms
