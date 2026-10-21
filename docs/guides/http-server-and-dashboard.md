# HTTP Server and Dashboard

## Introduction

The `serve` command launches an HTTP server that provides both a **browser-based dashboard** for visual exploration and a **semantic search API** for programmatic NL lookup. The dashboard lets you explore the code graph, visualize blast radius, inspect CFGs, and browse communities — all from your browser.

For **structural** questions (symbols, callers, relations, inventories), agents and scripts should use CLI verbs with `-f json` (`find`, `callers`, `callees`, `relations`, `inventory`, `status`) — see [Structured graph queries](structured-query.md).

## Use Cases

- **Interactive exploration.** Browse functions, classes, and communities in a visual UI.
- **Persistent semantic session.** Keep the server running and POST natural-language queries to `/api/semantic/query`.
- **Team sharing.** Run the server on a shared host so the whole team can explore the codebase.
- **Demo and presentation.** Show stakeholders the architecture of a codebase through the dashboard.

## Example Project

This guide uses the **CoolStore** (`example/coolstore`). `rgctl serve` can start the full pipeline itself. To only serve existing artifacts:

CoolStore examples use `-l java` to index the Java backend only (skip Angular/bower).

```bash
rgctl -r example/coolstore discover -l java --with-cfg --with-dashboard
rgctl -r example/coolstore serve --no-pipeline --open
```

The `--with-dashboard` flag exports the static dashboard bundle to `.rgctl/dashboard/`.

## Step-by-Step

### 1. Start the Server

Launch the HTTP server with the dashboard:

```bash
rgctl -r example/coolstore serve --open
# Keep the structural graph fresh while editing:
rgctl -r example/coolstore serve --watch --open
```

See [Watch mode](watch-mode.md) for `serve --watch` vs `rgctl update`.

**What happens:**

- The server starts on `http://127.0.0.1:8080`.
- The `--open` flag opens the dashboard in your default browser.
- The dashboard serves from `.rgctl/dashboard/`. Semantic search is available at `/api/semantic/*` when an index exists.

### 2. Custom Host and Port

Bind to a different address or port:

```bash
rgctl -r example/coolstore serve --host 0.0.0.0 --port 8080
```

This makes the server accessible on all network interfaces at port 8080, useful for team sharing.

### 3. Query API Only

If you only need the API (no dashboard UI):

```bash
rgctl -r example/coolstore serve --query-only
```

This starts a lighter server with `/api/status` and `/api/semantic/*` (no static files).

### 4. Dashboard Only

If you only need the visual dashboard:

```bash
rgctl -r example/coolstore serve --dashboard-only
```

### 5. Structural queries from the CLI

While the server is running, issue graph queries in another terminal (same repo root):

```bash
rgctl -r example/coolstore -f json find priceShoppingCart --exact | jq '.entities[0]'
rgctl -r example/coolstore -f json callers priceShoppingCart | jq '.callers[].name'
```

### 6. Semantic Search via API

Query the semantic index over HTTP:

```bash
curl -s http://127.0.0.1:8080/api/semantic/query \
  -H "Content-Type: application/json" \
  -d '{"query": "shopping cart checkout", "limit": 5}'
```

Build the index first: `rgctl semantic index` (then restart `serve`).

### 7. Dashboard tabs

When the dashboard opens in your browser, you will see several tabs:

| Tab | Description |
|-----|-------------|
| **Search** | Full-text and semantic search across functions and classes |
| **Graph** | Interactive force-directed graph visualization |
| **Functions** | Sortable table of all functions with metrics |
| **CFG** | Control-flow graph viewer for individual functions |
| **Dataflow** | Data-flow and PDG visualization |
| **Slice** | Interactive program slicing |
| **Blast** | Blast radius visualization with caller/impact trees |
| **Taint** | Taint analysis results (requires `--with-taint`) |
| **Migration** | Migration roadmap viewer (requires `--export-migration-hints`) |

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Pipeline status document (`schema_version` 1; same shape as `.rgctl/pipeline_status.json`). Distinct from CLI `rgctl status` (`schema_version` 3). |
| `/api/semantic/query` | POST | Semantic search |
| `/api/semantic/status` | GET | Semantic index availability |
| `/` | GET | Dashboard UI |

## Server Options Reference

| Option | Default | Description |
|--------|---------|-------------|
| `--host` | `127.0.0.1` | Bind host |
| `--port` | `8080` | HTTP port |
| `--open` | off | Open dashboard in browser (preparing page if the bundle is not ready) |
| `--query-only` | off | Serve API only, no dashboard |
| `--dashboard-only` | off | Serve dashboard only, no API |
| `--no-pipeline` | off | Fail fast if artifacts are missing (old `serve` behavior) |
| `--dashboard-dir` | `.rgctl/dashboard` | Dashboard directory |

## Benefits

- **Zero setup.** One command to launch a full-featured analysis dashboard.
- **Dual interface.** Visual dashboard for humans; CLI JSON for agents and scripts.
- **Session persistence.** The server keeps artifacts hot; repeated semantic queries stay fast.
- **Team accessible.** Bind to `0.0.0.0` to share the dashboard across a network.
- **Low resource.** HTTP `serve` stays up until Ctrl+C.

## Related Guides

- [Discovering and Indexing a Codebase](discovering-and-indexing.md) -- `discover --with-dashboard` generates the dashboard bundle
- [Structured graph queries](structured-query.md) -- CLI verbs for agents
- [Semantic Search](semantic-search.md) -- semantic queries available via `/api/semantic/*`
- [Blast Radius Analysis](blast-radius-analysis.md) -- blast-radius visualization in the dashboard
