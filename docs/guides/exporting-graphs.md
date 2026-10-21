# Exporting Graphs

## Introduction

The `export` command serializes your code knowledge graph into standard file formats for use in external tools, visualization platforms, and documentation systems. rgctl supports six export formats: JSON, GraphML, Graphviz (DOT), Mermaid, Obsidian vault, and OKF (Open Knowledge Foundation).

Whether you want to load your code graph into Neo4j, visualize it in Gephi, embed diagrams in documentation, or browse it as an Obsidian vault, `export` produces the right format.

## Use Cases

- **Graph database import.** Export to GraphML for loading into Neo4j, Gephi, or yEd.
- **Documentation diagrams.** Export to Mermaid or Graphviz for embedding in markdown docs.
- **Knowledge management.** Export to Obsidian for browsing the code graph as interlinked notes.
- **Data analysis.** Export to JSON for custom scripts and analysis pipelines.
- **Interoperability.** Export to OKF for knowledge management platforms.
- **Archival.** Save a snapshot of the codebase structure for later comparison.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). Make sure you have run `discover` first:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover . -l java --with-cfg
```

After `discover . -l java --with-cfg` on clean CoolStore you should see **608** nodes / **1,709** edges / **152** functions (`rgctl -f json status` / `find --type function --count-only`).

## Step-by-Step

### 1. Export to GraphML

Export all functions to GraphML format:

```bash
rgctl -r example/coolstore export \
  --export-format graphml \
  --export-output /tmp/coolstore.graphml \
  --query "type:Function"
```

**Output:**

```
Exported 152 nodes, 130 edges -> /tmp/coolstore.graphml
```

**What happened:**

- rgctl exported all **152** function nodes and **130** call edges for the Java CoolStore backend.
- The `--query "type:Function"` filter restricts the export to function nodes only.
- Open the GraphML file in Gephi, yEd, or any GraphML-compatible tool.

### 2. Export Everything to JSON

Export the entire graph (all node types) to JSON:

```bash
rgctl -r example/coolstore export \
  --export-format json \
  --export-output /tmp/coolstore.json \
  --query all
```

**Output:**

```
Exported 608 nodes, 1709 edges -> /tmp/coolstore.json
```

The `all` query exports every node and edge in the `-l java --with-cfg` graph -- **608** nodes and **1,709** edges for CoolStore’s Java backend.

### 3. Filtered Export

Use query filters to export subsets of the graph:

```bash
# Export only classes
rgctl -r example/coolstore export \
  --export-format json \
  --export-output /tmp/coolstore-classes.json \
  --query "type:Class"

# Export by name
rgctl -r example/coolstore export \
  --export-format json \
  --export-output /tmp/cart-service.json \
  --query "name:ShoppingCartService"

# Export all functions
rgctl -r example/coolstore export \
  --export-format graphml \
  --export-output /tmp/coolstore-functions.graphml \
  --query functions
```

### 4. Export to Obsidian Vault

Create an Obsidian vault where each heading section becomes an interlinked note:

```bash
rgctl -r example/coolstore export \
  --export-format obsidian \
  --export-output /tmp/coolstore-vault \
  --query all
```

This creates a directory of markdown files that you can open in Obsidian, with wikilinks between related nodes. Each function, class, and module becomes a note with backlinks to its callers and callees.

For documentation-only repos, see [Markdown Context Graph](markdown-context-graph.md) — Obsidian export of heading sections, OKF JSON, and doc semantic index.

### 5. Export to Graphviz (DOT)

Generate a DOT file for rendering with Graphviz:

```bash
rgctl -r example/coolstore export \
  --export-format graphviz \
  --export-output /tmp/coolstore.dot \
  --query "name:ShoppingCartService"
```

Render to SVG:

```bash
dot -Tsvg /tmp/coolstore.dot -o /tmp/coolstore.svg
```

### 6. Export to Mermaid

Generate a Mermaid diagram for embedding in markdown:

```bash
rgctl -r example/coolstore export \
  --export-format mermaid \
  --export-output /tmp/coolstore.mmd \
  --query "name:ShoppingCartService"
```

The output can be pasted directly into a markdown file or rendered by any Mermaid-compatible viewer.

### 7. Export to OKF

Export in Open Knowledge Foundation JSON format:

```bash
rgctl -r example/coolstore export \
  --export-format okf \
  --export-output /tmp/coolstore-okf.json \
  --query all
```

## Query Filter Syntax

The `--query` parameter accepts the following filter expressions:

| Filter | Description | Example |
|--------|-------------|---------|
| `all` | Export everything | `--query all` |
| `functions` | All function nodes | `--query functions` |
| `type:TYPE` | Nodes of a specific type | `--query "type:Class"` |
| `name:NAME` | Nodes matching a name | `--query "name:ShoppingCartService"` |

Note: these are filter expressions, not `find` / `relations` patterns.

## Export Formats Comparison

| Format | File Extension | Best For | Tools |
|--------|---------------|----------|-------|
| **json** | `.json` | Scripts, APIs, data analysis | jq, Python, custom tools |
| **graphml** | `.graphml` | Graph databases, visualization | Neo4j, Gephi, yEd |
| **graphviz** | `.dot` | Static diagrams | Graphviz (dot, neato) |
| **mermaid** | `.mmd` | Documentation | Markdown renderers, Mermaid Live |
| **obsidian** | directory | Knowledge browsing | Obsidian |
| **okf** | `.json` | Knowledge platforms | CKAN, DataHub |

## Benefits

- **Standard formats.** Every export format is an industry standard with broad tool support.
- **Flexible filtering.** Export the entire graph or just the subset you need.
- **Fast.** Exporting 14,000+ nodes with 50,000+ edges completes in under 100ms.
- **Multiple use cases.** From CI artifacts to documentation to interactive exploration.
- **Obsidian integration.** Turn your code graph into a browsable knowledge vault.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- must run `discover` before exporting
- [Structured graph queries](structured-query.md) -- explore the graph before exporting subsets
- [HTTP Server and Dashboard](http-server-and-dashboard.md) -- interactive exploration as an alternative to static export
- [Hybrid CPG](hybrid-cpg.md) -- `cpg export` for CPG-specific GraphSON/GraphML export
