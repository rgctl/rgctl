# Phase 14: Visualization & Export

Phase 14 adds diagram export formats and an interactive web UI for exploring the code knowledge graph.

## Diagram export

Generate diagrams from graph queries (same DSL as `rbuilder query`):

```bash
# Mermaid flowchart (stdout)
rbuilder diagram "type:Function" --format mermaid

# Mermaid call graph
rbuilder diagram "functions" --format mermaid --diagram-type call-graph

# Graphviz DOT
rbuilder diagram "type:Class" --format dot -o graph.dot

# PNG (requires Graphviz: brew install graphviz)
rbuilder diagram "functions" --format png --output graph.png

# GraphML via export command
rbuilder export --format graphml --output graph.graphml --query "all"
```

### Supported formats

| Format | CLI flag | Notes |
|--------|----------|-------|
| Mermaid | `--format mermaid` | flowchart, class, call-graph |
| DOT | `--format dot` | Graphviz source |
| GraphML | `export --format graphml` | XML for yEd, Gephi |
| PNG/SVG/PDF | `--format png/svg/pdf` | Requires `dot` binary |

### MCP tool

Agents can call `generate_diagram` with:

```json
{
  "query": "type:Function",
  "format": "mermaid",
  "diagram_type": "call-graph",
  "depth": 2
}
```

## Web UI

Start the server (default port 8080, or use `serve-web` on port 3000):

```bash
rbuilder init   # build graph first
rbuilder serve-web --open
```

| URL | Description |
|-----|-------------|
| http://localhost:3000/ | Vis.js graph browser |
| http://localhost:3000/explorer.html | D3 force-directed explorer |
| http://localhost:3000/dashboard.html | Chart.js metrics dashboard (complexity, communities, centrality, hotspots) |

### REST API (Phase 14)

| Endpoint | Description |
|----------|-------------|
| `GET /api/graph?query=...&depth=N` | Nodes + edges for query |
| `GET /api/node/{id}` | Node details |
| `GET /api/node/{id}/neighbors` | Adjacent nodes |
| `GET /api/stats` | Graph statistics |
| `GET /api/dashboard` | Dashboard chart data (complexity, communities, centrality, hotspots) |
| `GET /api/dashboard/advanced` | Labeled communities, risk hotspots, top-20 centrality |
| `POST /api/query` | NLP or DSL query |

## Examples

**Call graph with depth expansion:**

```bash
rbuilder diagram "name:authenticate_user" --format mermaid --diagram-type call-graph --depth 2
```

**Horizontal DOT layout:**

```bash
rbuilder diagram "type:Module" --format dot --rankdir LR -o modules.dot
```

## Requirements

- **Graphviz** (optional): needed for PNG/SVG/PDF rendering
- **Feature flag**: web server requires `mcp-server` feature (enabled in default bundle)
