# Phase 14: Visualization & Export - Implementation Guide

**Linked from:** [.github/TASK_PLAN.md](/.github/TASK_PLAN.md#phase-14-visualization--export-weeks-38-41)  
**Status:** Not Started  
**Estimated Effort:** 6-8 weeks  
**Target Grade:** A (90%+ coverage)

---

## Overview

**Goal**: Match GitNexus visualization features and exceed with interactive web UI.

**Success Metrics**:
- ✅ Mermaid diagram generation (CLI + MCP tool)
- ✅ Graphviz DOT export (CLI)
- ✅ PNG/SVG rendering via Graphviz
- ✅ GraphML export for external tools (Gephi, Neo4j)
- ✅ Interactive D3.js web graph explorer
- ✅ Web dashboard with metrics

**Priority Order** (implement in this sequence):
1. **Week 1-2**: Mermaid + Graphviz exports (CLI foundation)
2. **Week 3**: PNG/SVG rendering + GraphML export
3. **Week 4-6**: D3.js interactive web explorer
4. **Week 7**: Web dashboard with metrics
5. **Week 8**: Testing, documentation, polish

---

## Part 1: Diagram Export (Weeks 1-3)

### 1.1 Mermaid Diagram Export (Week 1)

**Files to Create**:
- `src/export/mod.rs` - Export module root
- `src/export/mermaid.rs` - Mermaid diagram generator
- `tests/phase14_mermaid.rs` - Mermaid tests

**Files to Modify**:
- `src/lib.rs` - Add `pub mod export;`
- `src/cli/mod.rs` - Add `diagram` subcommand
- `src/mcp/tools.rs` - Add `generate_diagram` tool

#### Architecture

```rust
// src/export/mermaid.rs

pub enum DiagramType {
    Flowchart,
    ClassDiagram,
    CallGraph,
    DependencyGraph,
}

pub struct MermaidOptions {
    pub diagram_type: DiagramType,
    pub max_depth: Option<usize>,
    pub include_external: bool,
    pub vertical: bool,  // TB vs LR
}

pub fn generate_mermaid(
    graph: &CodeGraph,
    query: &str,
    options: MermaidOptions,
) -> Result<String> {
    // 1. Execute query to get nodes
    // 2. Get relevant edges
    // 3. Build Mermaid syntax
    // 4. Return markdown string
}

fn render_flowchart(nodes: &[Node], edges: &[Edge]) -> String {
    // graph TD\n    A[main] --> B[foo]\n...
}

fn render_class_diagram(nodes: &[Node], edges: &[Edge]) -> String {
    // classDiagram\n    class User {...}\n...
}
```

#### CLI Command

```rust
// src/cli/mod.rs - Add subcommand

pub struct DiagramCommand {
    /// Query to select nodes (e.g., "type:Function|repo:backend")
    query: String,
    
    /// Output format: mermaid, dot, graphml
    #[arg(long, default_value = "mermaid")]
    format: String,
    
    /// Diagram type for Mermaid: flowchart, class, call-graph
    #[arg(long, default_value = "flowchart")]
    diagram_type: String,
    
    /// Output file (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,
    
    /// Max depth for traversal
    #[arg(long)]
    max_depth: Option<usize>,
}

// Usage:
// rbuilder diagram "type:Function" --format mermaid --output arch.md
// rbuilder diagram "name:User" --format mermaid --diagram-type class
```

#### MCP Tool

```rust
// src/mcp/tools.rs - Add new tool

pub fn generate_diagram_tool() -> Tool {
    Tool {
        name: "generate_diagram".into(),
        description: "Generate Mermaid or Graphviz diagram from graph query".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Graph query (e.g., 'type:Function')"
                },
                "format": {
                    "type": "string",
                    "enum": ["mermaid", "dot"],
                    "default": "mermaid"
                },
                "diagram_type": {
                    "type": "string",
                    "enum": ["flowchart", "class", "call-graph"],
                    "default": "flowchart"
                }
            },
            "required": ["query"]
        }),
    }
}
```

#### Tests (Target: 8 tests)

```rust
// tests/phase14_mermaid.rs

#[test]
fn test_mermaid_flowchart_basic() {
    // Create graph with a → b → c
    // Generate mermaid
    // Assert contains "graph TD", "A[a]", "A --> B"
}

#[test]
fn test_mermaid_class_diagram_with_fields() {
    // Create class nodes with fields
    // Assert contains "classDiagram", "class User", "+email"
}

#[test]
fn test_mermaid_max_depth_limits_traversal() {
    // Create deep chain a→b→c→d→e
    // Generate with max_depth=2
    // Assert only a, b, c in output
}

#[test]
fn test_mermaid_call_graph_shows_function_calls() {
    // Create call graph: main → auth → validate
    // Assert proper call arrows
}

#[test]
fn test_mermaid_empty_query_returns_empty() {
    // Query that matches nothing
    // Assert empty diagram or error
}

#[test]
fn test_mermaid_vertical_vs_horizontal() {
    // Test TB vs LR direction
}

#[test]
fn test_mermaid_escapes_special_chars() {
    // Node names with quotes, brackets, etc.
    // Assert proper escaping
}

#[test]
fn test_mermaid_mcp_tool_integration() {
    // Call via MCP tool, verify JSON response
}
```

---

### 1.2 Graphviz DOT Export (Week 2)

**Files to Create**:
- `src/export/graphviz.rs` - DOT format generator
- `tests/phase14_graphviz.rs` - DOT tests

**Files to Modify**:
- `src/export/mod.rs` - Add `pub mod graphviz;`
- `src/cli/mod.rs` - Add `--format dot` support

#### Architecture

```rust
// src/export/graphviz.rs

pub struct GraphvizOptions {
    pub layout: Layout,  // dot, neato, fdp, circo
    pub rankdir: RankDir,  // LR, TB, RL, BT
    pub node_style: HashMap<NodeType, NodeStyle>,
    pub edge_style: HashMap<EdgeType, EdgeStyle>,
}

pub enum Layout {
    Dot,    // Hierarchical
    Neato,  // Spring model
    Fdp,    // Force-directed
    Circo,  // Circular
}

pub struct NodeStyle {
    pub shape: String,  // box, ellipse, diamond, etc.
    pub color: String,
    pub fillcolor: Option<String>,
}

pub struct EdgeStyle {
    pub style: String,  // solid, dashed, dotted
    pub color: String,
    pub label: Option<String>,
}

pub fn generate_dot(
    graph: &CodeGraph,
    query: &str,
    options: GraphvizOptions,
) -> Result<String> {
    // 1. Execute query
    // 2. Build DOT syntax
    // 3. Apply node/edge styles
    // 4. Return DOT string
}
```

#### Default Styles

```rust
// Default node styles by type
Function    → shape=box, color=blue
Class       → shape=ellipse, color=green
Module      → shape=folder, color=orange
Interface   → shape=diamond, color=purple

// Default edge styles by type
Calls       → style=solid, color=black
Extends     → style=dashed, color=red, label="extends"
Implements  → style=dashed, color=blue, label="implements"
Uses        → style=dotted, color=gray
```

#### Tests (Target: 6 tests)

```rust
#[test]
fn test_dot_basic_digraph() {
    // Assert "digraph CodeGraph {", "->", "}"
}

#[test]
fn test_dot_node_shapes_by_type() {
    // Create function + class nodes
    // Assert function has shape=box, class has shape=ellipse
}

#[test]
fn test_dot_edge_styles_by_type() {
    // Calls vs Extends edges
    // Assert different styles
}

#[test]
fn test_dot_rankdir_horizontal() {
    // Generate with rankdir=LR
    // Assert contains "rankdir=LR"
}

#[test]
fn test_dot_special_char_escaping() {
    // Node names with quotes, etc.
}

#[test]
fn test_dot_cli_output_to_file() {
    // Run CLI command with -o output.dot
    // Assert file created, valid DOT syntax
}
```

---

### 1.3 PNG/SVG Rendering (Week 3, Part 1)

**Files to Create**:
- `src/export/render.rs` - Image rendering via Graphviz CLI

**Files to Modify**:
- `src/cli/mod.rs` - Add `--output graph.png` support

#### Architecture

```rust
// src/export/render.rs

pub enum ImageFormat {
    Png,
    Svg,
    Pdf,
}

pub fn render_to_file(
    dot_content: &str,
    output_path: &Path,
    format: ImageFormat,
    layout: Layout,
) -> Result<()> {
    // 1. Check if Graphviz installed (dot --version)
    // 2. Write DOT to temp file
    // 3. Run: dot -Tpng -o output.png input.dot
    // 4. Return success or error
}

pub fn check_graphviz_installed() -> bool {
    Command::new("dot")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

#### CLI Flow

```bash
# User runs:
rbuilder diagram "type:Function" --output graph.png

# Internals:
# 1. Generate DOT content
# 2. Check if Graphviz installed
# 3. If yes: render to PNG
# 4. If no: error with instructions to install Graphviz
```

#### Error Handling

```rust
if !check_graphviz_installed() {
    return Err(Error::Other(
        "Graphviz not found. Install with: brew install graphviz".into()
    ));
}
```

#### Tests (Target: 4 tests)

```rust
#[test]
#[ignore] // Requires Graphviz installed
fn test_render_png_creates_file() {
    // Generate DOT
    // Render to PNG
    // Assert file exists, is valid PNG
}

#[test]
fn test_graphviz_not_installed_returns_error() {
    // Mock Command to fail
    // Assert error message suggests installation
}

#[test]
#[ignore]
fn test_render_svg_creates_file() {
    // Render to SVG
}

#[test]
fn test_cli_png_output_end_to_end() {
    // Run full CLI command
    // Assert PNG created
}
```

---

### 1.4 GraphML Export (Week 3, Part 2)

**Files to Create**:
- `src/export/graphml.rs` - GraphML XML generator
- `tests/phase14_graphml.rs` - GraphML tests

#### Architecture

```rust
// src/export/graphml.rs

pub fn export_graphml(graph: &CodeGraph, query: &str) -> Result<String> {
    // 1. Execute query
    // 2. Build GraphML XML
    // 3. Include all node properties
    // 4. Return XML string
}

fn xml_header() -> String {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns">
  <key id="name" for="node" attr.name="name" attr.type="string"/>
  <key id="type" for="node" attr.name="type" attr.type="string"/>
  <key id="complexity" for="node" attr.name="complexity" attr.type="int"/>
  <graph id="G" edgedefault="directed">"#.into()
}

fn render_node_xml(node: &Node) -> String {
    format!(
        r#"    <node id="{}">
      <data key="name">{}</data>
      <data key="type">{:?}</data>
    </node>"#,
        node.id, node.name, node.node_type
    )
}
```

#### CLI Command

```bash
rbuilder export --format graphml -o graph.graphml
rbuilder export "type:Class" --format graphml -o classes.graphml
```

#### Tests (Target: 5 tests)

```rust
#[test]
fn test_graphml_valid_xml() {
    // Generate GraphML
    // Parse with XML parser
    // Assert valid
}

#[test]
fn test_graphml_includes_node_properties() {
    // Assert <data key="name">, <data key="type">
}

#[test]
fn test_graphml_includes_edges() {
    // Assert <edge source="n0" target="n1">
}

#[test]
fn test_graphml_escapes_xml_special_chars() {
    // Node with <>&" in name
}

#[test]
fn test_graphml_cli_export() {
    // Run CLI command
    // Assert file created, valid GraphML
}
```

---

## Part 2: Interactive Web Graph Explorer (Weeks 4-6)

### 2.1 Backend HTTP API (Week 4)

**Files to Create**:
- `src/web/mod.rs` - Web module root
- `src/web/api.rs` - HTTP API endpoints
- `src/web/server.rs` - Web server startup
- `tests/phase14_web_api.rs` - API tests

**Files to Modify**:
- `src/lib.rs` - Add `pub mod web;`
- `src/cli/mod.rs` - Add `serve-web` subcommand
- `src/mcp/server.rs` - Reuse API endpoints if possible

#### API Endpoints

```rust
// src/web/api.rs

// GET /api/graph?query=<query>&limit=100
async fn get_graph(
    Extension(state): Extension<AppState>,
    Query(params): Query<GraphQueryParams>,
) -> Result<Json<GraphResponse>, StatusCode> {
    // Execute query, return nodes + edges JSON
}

// GET /api/node/:id
async fn get_node(
    Extension(state): Extension<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Node>, StatusCode> {
    // Return single node details
}

// GET /api/node/:id/neighbors?depth=1
async fn get_neighbors(
    Extension(state): Extension<AppState>,
    Path(id): Path<String>,
    Query(params): Query<NeighborParams>,
) -> Result<Json<NeighborResponse>, StatusCode> {
    // Return adjacent nodes + edges
}

// GET /api/stats
async fn get_stats(
    Extension(state): Extension<AppState>,
) -> Result<Json<GraphStats>, StatusCode> {
    // Return graph statistics
}

// POST /api/query
async fn execute_query(
    Extension(state): Extension<AppState>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, StatusCode> {
    // Execute complex query
}
```

#### Response Schemas

```rust
#[derive(Serialize)]
pub struct GraphResponse {
    pub nodes: Vec<NodeData>,
    pub edges: Vec<EdgeData>,
}

#[derive(Serialize)]
pub struct NodeData {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub file_path: Option<String>,
    pub complexity: Option<i64>,
    pub x: Option<f64>,  // For D3 layout persistence
    pub y: Option<f64>,
}

#[derive(Serialize)]
pub struct EdgeData {
    pub source: String,  // node ID
    pub target: String,  // node ID
    pub edge_type: String,
}

#[derive(Serialize)]
pub struct GraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: HashMap<String, usize>,
    pub avg_complexity: f64,
    pub max_complexity: i64,
    pub languages: Vec<String>,
}
```

#### Web Server

```rust
// src/web/server.rs

pub async fn start_web_server(
    repo_root: &Path,
    port: u16,
) -> Result<()> {
    let state = AppState::from_repo(repo_root)?;
    
    let app = Router::new()
        .route("/api/graph", get(get_graph))
        .route("/api/node/:id", get(get_node))
        .route("/api/node/:id/neighbors", get(get_neighbors))
        .route("/api/stats", get(get_stats))
        .route("/api/query", post(execute_query))
        .nest_service("/", ServeDir::new("web/dist"))
        .layer(Extension(state))
        .layer(CorsLayer::permissive());
    
    let addr = format!("127.0.0.1:{port}");
    println!("Web UI: http://{addr}");
    
    axum::Server::bind(&addr.parse()?)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}
```

#### CLI Command

```bash
rbuilder serve-web --port 3000
# Opens http://localhost:3000 in browser
```

#### Tests (Target: 8 tests)

```rust
#[tokio::test]
async fn test_api_get_graph_returns_nodes_edges() {
    // Start test server
    // GET /api/graph
    // Assert JSON has nodes, edges arrays
}

#[tokio::test]
async fn test_api_get_node_by_id() {
    // GET /api/node/{id}
    // Assert returns node details
}

#[tokio::test]
async fn test_api_get_neighbors() {
    // GET /api/node/{id}/neighbors
    // Assert returns adjacent nodes
}

#[tokio::test]
async fn test_api_stats() {
    // GET /api/stats
    // Assert returns total_nodes, total_edges, etc.
}

#[tokio::test]
async fn test_api_query_with_filter() {
    // POST /api/query with {"query": "type:Function"}
    // Assert filtered results
}

#[tokio::test]
async fn test_api_cors_headers() {
    // Assert CORS headers present
}

#[tokio::test]
async fn test_api_error_invalid_node_id() {
    // GET /api/node/invalid
    // Assert 404
}

#[tokio::test]
async fn test_serve_web_starts_server() {
    // Start server, make request, assert success
}
```

---

### 2.2 D3.js Frontend (Weeks 5-6)

**Files to Create**:
- `web/index.html` - Main page
- `web/css/style.css` - Styles
- `web/js/graph.js` - D3.js graph visualization
- `web/js/api.js` - API client
- `web/js/app.js` - App initialization

#### Directory Structure

```
web/
├── index.html
├── css/
│   └── style.css
├── js/
│   ├── app.js         # Main app
│   ├── graph.js       # D3 force graph
│   ├── api.js         # API calls
│   ├── filters.js     # UI filters
│   └── details.js     # Node details panel
└── lib/
    └── d3.v7.min.js   # D3.js library
```

#### HTML Structure

```html
<!-- web/index.html -->
<!DOCTYPE html>
<html>
<head>
    <title>rBuilder Graph Explorer</title>
    <link rel="stylesheet" href="css/style.css">
    <script src="lib/d3.v7.min.js"></script>
</head>
<body>
    <div class="container">
        <!-- Sidebar -->
        <aside class="sidebar">
            <h2>rBuilder</h2>
            
            <!-- Search -->
            <div class="search-box">
                <input type="text" id="search" placeholder="Search nodes...">
            </div>
            
            <!-- Filters -->
            <div class="filters">
                <h3>Filters</h3>
                <label><input type="checkbox" value="Function" checked> Functions</label>
                <label><input type="checkbox" value="Class" checked> Classes</label>
                <label><input type="checkbox" value="Module" checked> Modules</label>
            </div>
            
            <!-- Stats -->
            <div class="stats">
                <h3>Statistics</h3>
                <div id="stats-content"></div>
            </div>
        </aside>
        
        <!-- Main Graph -->
        <main class="graph-container">
            <svg id="graph"></svg>
        </main>
        
        <!-- Details Panel -->
        <aside class="details-panel">
            <h3>Node Details</h3>
            <div id="node-details"></div>
        </aside>
    </div>
    
    <script type="module" src="js/app.js"></script>
</body>
</html>
```

#### D3.js Force Graph

```javascript
// web/js/graph.js

export class GraphVisualization {
    constructor(svgId) {
        this.svg = d3.select(`#${svgId}`);
        this.width = window.innerWidth * 0.7;
        this.height = window.innerHeight;
        
        this.simulation = d3.forceSimulation()
            .force("link", d3.forceLink().id(d => d.id).distance(100))
            .force("charge", d3.forceManyBody().strength(-300))
            .force("center", d3.forceCenter(this.width / 2, this.height / 2))
            .force("collision", d3.forceCollide().radius(30));
    }
    
    render(graphData) {
        // Clear existing
        this.svg.selectAll("*").remove();
        
        const g = this.svg.append("g");
        
        // Zoom behavior
        const zoom = d3.zoom()
            .scaleExtent([0.1, 4])
            .on("zoom", (event) => g.attr("transform", event.transform));
        this.svg.call(zoom);
        
        // Draw edges
        const link = g.append("g")
            .selectAll("line")
            .data(graphData.edges)
            .join("line")
            .attr("stroke", d => this.edgeColor(d.edge_type))
            .attr("stroke-width", 2);
        
        // Draw nodes
        const node = g.append("g")
            .selectAll("circle")
            .data(graphData.nodes)
            .join("circle")
            .attr("r", 15)
            .attr("fill", d => this.nodeColor(d.node_type))
            .call(this.drag(this.simulation))
            .on("click", (event, d) => this.onNodeClick(d))
            .on("dblclick", (event, d) => this.onNodeDoubleClick(d));
        
        // Node labels
        const label = g.append("g")
            .selectAll("text")
            .data(graphData.nodes)
            .join("text")
            .text(d => d.name)
            .attr("font-size", 10)
            .attr("dx", 20)
            .attr("dy", 4);
        
        // Update positions on simulation tick
        this.simulation.nodes(graphData.nodes);
        this.simulation.force("link").links(graphData.edges);
        
        this.simulation.on("tick", () => {
            link
                .attr("x1", d => d.source.x)
                .attr("y1", d => d.source.y)
                .attr("x2", d => d.target.x)
                .attr("y2", d => d.target.y);
            
            node
                .attr("cx", d => d.x)
                .attr("cy", d => d.y);
            
            label
                .attr("x", d => d.x)
                .attr("y", d => d.y);
        });
    }
    
    nodeColor(nodeType) {
        const colors = {
            Function: "#4285F4",
            Class: "#34A853",
            Module: "#FBBC05",
            Interface: "#EA4335",
        };
        return colors[nodeType] || "#9E9E9E";
    }
    
    edgeColor(edgeType) {
        return edgeType === "Calls" ? "#000" : "#999";
    }
    
    drag(simulation) {
        function dragstarted(event) {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            event.subject.fx = event.subject.x;
            event.subject.fy = event.subject.y;
        }
        
        function dragged(event) {
            event.subject.fx = event.x;
            event.subject.fy = event.y;
        }
        
        function dragended(event) {
            if (!event.active) simulation.alphaTarget(0);
            event.subject.fx = null;
            event.subject.fy = null;
        }
        
        return d3.drag()
            .on("start", dragstarted)
            .on("drag", dragged)
            .on("end", dragended);
    }
    
    onNodeClick(node) {
        // Show node details panel
        window.app.showNodeDetails(node);
    }
    
    async onNodeDoubleClick(node) {
        // Expand neighbors
        const neighbors = await window.api.getNeighbors(node.id);
        this.addNodes(neighbors);
    }
}
```

#### API Client

```javascript
// web/js/api.js

export class API {
    constructor(baseUrl = '') {
        this.baseUrl = baseUrl;
    }
    
    async getGraph(query = '', limit = 100) {
        const response = await fetch(
            `${this.baseUrl}/api/graph?query=${encodeURIComponent(query)}&limit=${limit}`
        );
        return response.json();
    }
    
    async getNode(id) {
        const response = await fetch(`${this.baseUrl}/api/node/${id}`);
        return response.json();
    }
    
    async getNeighbors(id, depth = 1) {
        const response = await fetch(
            `${this.baseUrl}/api/node/${id}/neighbors?depth=${depth}`
        );
        return response.json();
    }
    
    async getStats() {
        const response = await fetch(`${this.baseUrl}/api/stats`);
        return response.json();
    }
    
    async executeQuery(query) {
        const response = await fetch(`${this.baseUrl}/api/query`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ query }),
        });
        return response.json();
    }
}
```

#### App Initialization

```javascript
// web/js/app.js

import { GraphVisualization } from './graph.js';
import { API } from './api.js';

class App {
    constructor() {
        this.api = new API();
        this.graph = new GraphVisualization('graph');
        this.init();
    }
    
    async init() {
        // Load initial graph
        const data = await this.api.getGraph('', 50);
        this.graph.render(data);
        
        // Load stats
        const stats = await this.api.getStats();
        this.renderStats(stats);
        
        // Setup event listeners
        this.setupFilters();
        this.setupSearch();
    }
    
    setupFilters() {
        document.querySelectorAll('.filters input').forEach(checkbox => {
            checkbox.addEventListener('change', () => this.applyFilters());
        });
    }
    
    setupSearch() {
        document.getElementById('search').addEventListener('input', (e) => {
            this.searchNodes(e.target.value);
        });
    }
    
    async applyFilters() {
        const selected = Array.from(document.querySelectorAll('.filters input:checked'))
            .map(cb => cb.value);
        
        const query = `type:${selected.join('|type:')}`;
        const data = await this.api.getGraph(query);
        this.graph.render(data);
    }
    
    async searchNodes(term) {
        if (!term) {
            this.applyFilters();
            return;
        }
        
        const query = `name:*${term}*`;
        const data = await this.api.executeQuery(query);
        this.graph.render(data);
    }
    
    showNodeDetails(node) {
        const details = document.getElementById('node-details');
        details.innerHTML = `
            <p><strong>Name:</strong> ${node.name}</p>
            <p><strong>Type:</strong> ${node.node_type}</p>
            <p><strong>File:</strong> ${node.file_path || 'N/A'}</p>
            <p><strong>Complexity:</strong> ${node.complexity || 'N/A'}</p>
        `;
    }
    
    renderStats(stats) {
        const content = document.getElementById('stats-content');
        content.innerHTML = `
            <p>Nodes: ${stats.total_nodes}</p>
            <p>Edges: ${stats.total_edges}</p>
            <p>Avg Complexity: ${stats.avg_complexity.toFixed(1)}</p>
        `;
    }
}

// Initialize app
window.app = new App();
window.api = window.app.api;
```

#### CSS Styling

```css
/* web/css/style.css */

* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    overflow: hidden;
}

.container {
    display: flex;
    height: 100vh;
}

.sidebar {
    width: 300px;
    background: #f5f5f5;
    padding: 20px;
    overflow-y: auto;
}

.sidebar h2 {
    color: #333;
    margin-bottom: 20px;
}

.search-box input {
    width: 100%;
    padding: 10px;
    border: 1px solid #ddd;
    border-radius: 4px;
    margin-bottom: 20px;
}

.filters label {
    display: block;
    margin: 10px 0;
}

.graph-container {
    flex: 1;
    position: relative;
}

#graph {
    width: 100%;
    height: 100%;
}

.details-panel {
    width: 300px;
    background: #fff;
    padding: 20px;
    border-left: 1px solid #ddd;
    overflow-y: auto;
}

.stats {
    margin-top: 30px;
}

.stats h3 {
    margin-bottom: 10px;
}
```

#### Tests (Target: 6 integration tests)

Since frontend is mostly manual testing, create integration tests:

```rust
// tests/phase14_web_integration.rs

#[test]
#[ignore] // Requires web server running
fn test_web_ui_loads() {
    // Start server
    // Fetch http://localhost:3000
    // Assert HTML contains "rBuilder Graph Explorer"
}

#[test]
fn test_api_graph_endpoint_json_structure() {
    // Start server
    // GET /api/graph
    // Assert JSON has nodes, edges arrays
}

// Use headless browser tests for D3 interactions (optional)
```

---

### 2.3 Web Dashboard (Week 7)

**Files to Create**:
- `web/dashboard.html` - Dashboard page
- `web/js/dashboard.js` - Dashboard logic
- `web/js/charts.js` - Chart.js integration

#### Dashboard Widgets

1. **Repository Stats Card**
   - Total files, functions, classes, LOC
   
2. **Complexity Distribution** (Histogram)
   - X-axis: Complexity buckets (0-10, 11-20, etc.)
   - Y-axis: Count
   
3. **Top 10 Most Complex Functions** (Table)
   - Name, Complexity, File
   
4. **Language Breakdown** (Pie Chart)
   - Rust 45%, Python 30%, etc.
   
5. **Node Type Distribution** (Bar Chart)
   - Functions: 500, Classes: 100, etc.

#### Implementation

```javascript
// web/js/charts.js using Chart.js

import Chart from 'chart.js/auto';

export class DashboardCharts {
    async renderComplexityDistribution(stats) {
        const ctx = document.getElementById('complexity-chart');
        new Chart(ctx, {
            type: 'bar',
            data: {
                labels: ['0-10', '11-20', '21-30', '31-50', '50+'],
                datasets: [{
                    label: 'Functions by Complexity',
                    data: stats.complexity_buckets,
                    backgroundColor: '#4285F4',
                }]
            },
        });
    }
    
    async renderLanguageBreakdown(stats) {
        const ctx = document.getElementById('language-chart');
        new Chart(ctx, {
            type: 'pie',
            data: {
                labels: Object.keys(stats.languages),
                datasets: [{
                    data: Object.values(stats.languages),
                    backgroundColor: [
                        '#4285F4', '#34A853', '#FBBC05', '#EA4335'
                    ],
                }]
            },
        });
    }
}
```

#### Tests

Manual testing sufficient for dashboard. Ensure API endpoints return correct data.

---

## Part 3: Testing & Documentation (Week 8)

### 3.1 Comprehensive Testing

**Test Coverage Target: 90%+**

| Component | Target Tests | Priority |
|-----------|--------------|----------|
| Mermaid Export | 8 | High |
| Graphviz Export | 6 | High |
| PNG/SVG Render | 4 | Medium |
| GraphML Export | 5 | High |
| Web API | 8 | High |
| Web Integration | 4 | Low |
| **Total** | **35** | |

### 3.2 Documentation

**Files to Create**:
- `docs/phase14_visualization.md` - User guide
- `web/README.md` - Web UI setup
- Update main `README.md` with screenshots

#### User Guide Structure

```markdown
# Phase 14: Visualization & Export

## Quick Start

### Mermaid Diagrams
rbuilder diagram "type:Function" --format mermaid

### Graphviz DOT
rbuilder diagram "type:Class" --format dot -o classes.dot
dot -Tpng classes.dot -o classes.png

### GraphML Export
rbuilder export --format graphml -o graph.graphml

### Web UI
rbuilder serve-web --port 3000
# Open http://localhost:3000

## Examples
[Include screenshots and examples]

## Troubleshooting
- Graphviz not found: Install with brew install graphviz
- Web UI not loading: Check port 3000 is not in use
```

---

## Implementation Checklist

### Week 1: Mermaid Export
- [ ] Create `src/export/mod.rs` + `src/export/mermaid.rs`
- [ ] Implement `generate_mermaid()` with flowchart + class diagram support
- [ ] Add `diagram` CLI subcommand
- [ ] Add `generate_diagram` MCP tool
- [ ] Write 8 tests in `tests/phase14_mermaid.rs`
- [ ] Test CLI: `rbuilder diagram "type:Function" --format mermaid`

### Week 2: Graphviz Export
- [ ] Create `src/export/graphviz.rs`
- [ ] Implement `generate_dot()` with node/edge styling
- [ ] Add `--format dot` CLI support
- [ ] Write 6 tests in `tests/phase14_graphviz.rs`
- [ ] Test CLI: `rbuilder diagram "type:Class" --format dot -o out.dot`

### Week 3: Rendering + GraphML
- [ ] Create `src/export/render.rs` for PNG/SVG
- [ ] Implement Graphviz subprocess execution
- [ ] Add error handling for missing Graphviz
- [ ] Create `src/export/graphml.rs`
- [ ] Write 9 tests (4 render + 5 GraphML)
- [ ] Test CLI: `rbuilder diagram "functions" --output graph.png`

### Week 4: Web API
- [ ] Create `src/web/mod.rs`, `src/web/api.rs`, `src/web/server.rs`
- [ ] Implement 5 API endpoints (graph, node, neighbors, stats, query)
- [ ] Add `serve-web` CLI command
- [ ] Write 8 API tests
- [ ] Test: `rbuilder serve-web`, curl http://localhost:3000/api/stats

### Week 5-6: D3.js Frontend
- [ ] Create `web/` directory structure
- [ ] Build `index.html` with sidebar, graph canvas, details panel
- [ ] Implement D3.js force graph (`web/js/graph.js`)
- [ ] Implement API client (`web/js/api.js`)
- [ ] Add filters, search, node click/double-click
- [ ] Test in browser: drag nodes, click for details, filter by type

### Week 7: Dashboard
- [ ] Create `web/dashboard.html`
- [ ] Implement Chart.js charts (complexity, languages, top 10)
- [ ] Add stats API endpoint enhancements if needed
- [ ] Test dashboard loads and displays data

### Week 8: Testing + Documentation
- [ ] Run all 35+ tests, aim for 90%+ pass rate
- [ ] Write `docs/phase14_visualization.md`
- [ ] Add screenshots to README
- [ ] Manual testing: Full user flow walkthrough
- [ ] Create `PHASE_14_REVIEW.md` with grade

---

## Success Criteria

**Minimum (Grade B - 80%)**:
- ✅ Mermaid + DOT export working
- ✅ GraphML export working
- ✅ 25+ tests passing
- ✅ Basic documentation

**Target (Grade A - 90%)**:
- ✅ All export formats working (Mermaid, DOT, GraphML, PNG/SVG)
- ✅ Web UI with D3.js force graph
- ✅ 5 API endpoints functional
- ✅ 35+ tests passing
- ✅ Comprehensive documentation

**Exceptional (Grade A+ - 95%+)**:
- ✅ All above + Dashboard with charts
- ✅ 40+ tests
- ✅ E2E tests with headless browser
- ✅ Beautiful UI with smooth animations
- ✅ Export to PDF support

---

## Notes for Cursor

1. **Start Simple**: Begin with Mermaid/DOT exports before web UI
2. **Reuse Code**: Leverage existing query system, don't rebuild
3. **Test Early**: Write tests alongside implementation
4. **Document As You Go**: Update docs after each component
5. **TASK_PLAN Sync**: After completing each task, mark it [x] in TASK_PLAN.md
6. **Link This Guide**: Add link to this guide in TASK_PLAN Phase 14 header

**Final Deliverable**: Create `PHASE_14_REVIEW.md` with:
- Implementation summary
- Test results (X/35 passing)
- Screenshots of web UI
- Grade assessment
- Link back to TASK_PLAN.md

Good luck! 🚀
