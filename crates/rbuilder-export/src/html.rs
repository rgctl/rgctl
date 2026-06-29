//! HTML dashboard export with PatternFly, Bootstrap, and D3.js visualization.

use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{EdgeType, NodeType};
use serde_json::json;
use std::fs;
use std::path::Path;

/// Generate a self-contained HTML dashboard.
pub fn export_html_dashboard(
    backend: &MemoryBackend,
    analysis_dir: Option<&Path>,
    output_path: &Path,
) -> Result<(), String> {
    let nodes = backend.all_nodes().map_err(|e| e.to_string())?;
    let edges = backend.all_edges().map_err(|e| e.to_string())?;

    // Load analysis data if available - create summaries to avoid huge HTML files
    let mut analysis_data = json!([]);
    if let Some(analysis_path) = analysis_dir {
        let all_analyses_file = analysis_path.join("all_analyses.json");
        if all_analyses_file.exists() {
            if let Ok(content) = fs::read_to_string(all_analyses_file) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Create lightweight summaries instead of full CFG/PDG data
                    if let Some(analyses) = data.as_array() {
                        let summaries: Vec<_> = analyses.iter().map(|a| {
                            let cfg_blocks = a.get("cfg")
                                .and_then(|c| c.get("blocks"))
                                .and_then(|b| b.as_object())
                                .map(|b| b.len())
                                .unwrap_or(0);
                            let cfg_edges = a.get("cfg")
                                .and_then(|c| c.get("edges"))
                                .and_then(|e| e.as_array())
                                .map(|e| e.len())
                                .unwrap_or(0);
                            let pdg_data_deps = a.get("pdg")
                                .and_then(|p| p.get("data_deps"))
                                .and_then(|d| d.as_array())
                                .map(|d| d.len())
                                .unwrap_or(0);
                            let pdg_control_deps = a.get("pdg")
                                .and_then(|p| p.get("control_deps"))
                                .and_then(|d| d.as_array())
                                .map(|d| d.len())
                                .unwrap_or(0);
                            let dom_count = a.get("dominance")
                                .and_then(|d| d.get("idom"))
                                .and_then(|i| i.as_object())
                                .map(|i| i.len())
                                .unwrap_or(0);

                            json!({
                                "function_id": a.get("function_id"),
                                "function_name": a.get("function_name"),
                                "file_path": a.get("file_path"),
                                "cfg_blocks": cfg_blocks,
                                "cfg_edges": cfg_edges,
                                "pdg_data_deps": pdg_data_deps,
                                "pdg_control_deps": pdg_control_deps,
                                "dominators": dom_count,
                                "has_analysis": cfg_blocks > 0 || pdg_data_deps > 0 || dom_count > 0,
                            })
                        }).collect();
                        analysis_data = json!(summaries);
                    }
                }
            }
        }
    }

    // Prepare graph data for D3
    let graph_nodes: Vec<_> = nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id.to_string(),
                "name": n.name,
                "type": format!("{:?}", n.node_type),
                "file_path": n.file_path,
                "properties": n.properties,
            })
        })
        .collect();

    let graph_edges: Vec<_> = edges
        .iter()
        .map(|e| {
            json!({
                "source": e.from.to_string(),
                "target": e.to.to_string(),
                "type": format!("{:?}", e.edge_type),
            })
        })
        .collect();

    // Calculate statistics
    let total_nodes = nodes.len();
    let total_edges = edges.len();
    let function_count = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .count();
    let class_count = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Class)
        .count();
    let calls_count = edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Calls)
        .count();

    let avg_complexity: f64 = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .filter_map(|n| n.properties.get("cyclomatic"))
        .filter_map(|v| v.parse::<f64>().ok())
        .sum::<f64>()
        / function_count.max(1) as f64;

    let high_blast_radius_count = nodes
        .iter()
        .filter(|n| n.node_type == NodeType::Function)
        .filter(|n| {
            n.properties
                .get("blast_radius_score")
                .and_then(|v| v.parse::<f64>().ok())
                .map(|s| s > 50.0)
                .unwrap_or(false)
        })
        .count();

    // Generate HTML
    let html = generate_html_template(
        &graph_nodes,
        &graph_edges,
        &analysis_data,
        total_nodes,
        total_edges,
        function_count,
        class_count,
        calls_count,
        avg_complexity,
        high_blast_radius_count,
    );

    fs::write(output_path, html).map_err(|e| e.to_string())?;
    Ok(())
}

fn generate_html_template(
    nodes: &[serde_json::Value],
    edges: &[serde_json::Value],
    analysis_data: &serde_json::Value,
    total_nodes: usize,
    total_edges: usize,
    function_count: usize,
    class_count: usize,
    calls_count: usize,
    avg_complexity: f64,
    high_blast_radius_count: usize,
) -> String {
    let nodes_json = serde_json::to_string(&nodes).unwrap_or_else(|_| "[]".to_string());
    let edges_json = serde_json::to_string(&edges).unwrap_or_else(|_| "[]".to_string());
    let analysis_json = serde_json::to_string(&analysis_data).unwrap_or_else(|_| "[]".to_string());

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>rBuilder Analysis Dashboard</title>

    <!-- Bootstrap CSS -->
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css" rel="stylesheet">

    <!-- PatternFly CSS -->
    <link rel="stylesheet" href="https://unpkg.com/@patternfly/patternfly@5.2.0/patternfly.min.css">

    <!-- Font Awesome -->
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css">

    <style>
        body {{
            background-color: #f5f5f5;
            font-family: 'Red Hat Text', 'Helvetica Neue', Arial, sans-serif;
        }}
        .dashboard-header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 2rem 0;
            margin-bottom: 2rem;
            box-shadow: 0 4px 6px rgba(0,0,0,0.1);
        }}
        .stat-card {{
            background: white;
            border-radius: 8px;
            padding: 1.5rem;
            margin-bottom: 1.5rem;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
            border-left: 4px solid #667eea;
        }}
        .stat-card h3 {{
            font-size: 2.5rem;
            font-weight: bold;
            margin: 0;
            color: #667eea;
        }}
        .stat-card p {{
            color: #6c757d;
            margin: 0;
            font-size: 0.9rem;
        }}
        #graph-container {{
            background: white;
            border-radius: 8px;
            padding: 1rem;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
            min-height: 600px;
        }}
        #graph {{
            width: 100%;
            height: 600px;
            border: 1px solid #dee2e6;
            border-radius: 4px;
        }}
        .node {{
            cursor: pointer;
            stroke: #fff;
            stroke-width: 2px;
        }}
        .node.Function {{ fill: #667eea; }}
        .node.Class {{ fill: #f093fb; }}
        .node.Interface {{ fill: #4facfe; }}
        .node.File {{ fill: #43e97b; }}
        .node.Module {{ fill: #fa709a; }}
        .link {{
            stroke: #999;
            stroke-opacity: 0.6;
        }}
        .link.Calls {{ stroke: #667eea; stroke-width: 2px; }}
        .link.Contains {{ stroke: #aaa; stroke-width: 1px; stroke-dasharray: 5,5; }}
        .link.Implements {{ stroke: #4facfe; stroke-width: 2px; }}
        .link.Extends {{ stroke: #f093fb; stroke-width: 2px; }}
        .node-label {{
            font-size: 12px;
            pointer-events: none;
            fill: #333;
        }}
        .table-container {{
            background: white;
            border-radius: 8px;
            padding: 1.5rem;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
            margin-bottom: 1.5rem;
        }}
        .badge-custom {{
            padding: 0.25rem 0.75rem;
            border-radius: 12px;
            font-size: 0.75rem;
            font-weight: 600;
        }}
        .badge-high {{ background-color: #dc3545; color: white; }}
        .badge-medium {{ background-color: #ffc107; color: black; }}
        .badge-low {{ background-color: #28a745; color: white; }}
        .filter-section {{
            background: white;
            padding: 1rem;
            border-radius: 8px;
            margin-bottom: 1rem;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .legend {{
            background: white;
            padding: 1rem;
            border-radius: 4px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
            margin-top: 1rem;
        }}
        .legend-item {{
            display: inline-block;
            margin-right: 1.5rem;
            margin-bottom: 0.5rem;
        }}
        .legend-color {{
            display: inline-block;
            width: 20px;
            height: 20px;
            border-radius: 4px;
            margin-right: 0.5rem;
            vertical-align: middle;
        }}
        .tooltip {{
            position: absolute;
            padding: 8px;
            background: rgba(0, 0, 0, 0.8);
            color: white;
            border-radius: 4px;
            pointer-events: none;
            font-size: 12px;
            z-index: 1000;
        }}
    </style>
</head>
<body>
    <div class="dashboard-header">
        <div class="container">
            <h1><i class="fas fa-project-diagram"></i> rBuilder Analysis Dashboard</h1>
            <p class="lead">Comprehensive code analysis visualization</p>
        </div>
    </div>

    <div class="container-fluid px-4">
        <!-- Statistics Cards -->
        <div class="row mb-4">
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{total_nodes}</h3>
                    <p><i class="fas fa-circle-nodes"></i> Total Nodes</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{total_edges}</h3>
                    <p><i class="fas fa-arrow-right-arrow-left"></i> Total Edges</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{function_count}</h3>
                    <p><i class="fas fa-code"></i> Functions</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{avg_complexity:.1}</h3>
                    <p><i class="fas fa-chart-line"></i> Avg Complexity</p>
                </div>
            </div>
        </div>

        <div class="row mb-4">
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{class_count}</h3>
                    <p><i class="fas fa-cube"></i> Classes</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{calls_count}</h3>
                    <p><i class="fas fa-phone"></i> Call Edges</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3>{high_blast_radius_count}</h3>
                    <p><i class="fas fa-explosion"></i> High Blast Radius</p>
                </div>
            </div>
            <div class="col-md-3">
                <div class="stat-card">
                    <h3 id="selected-count">0</h3>
                    <p><i class="fas fa-filter"></i> Filtered Nodes</p>
                </div>
            </div>
        </div>

        <!-- Tabs -->
        <ul class="nav nav-tabs" id="mainTabs" role="tablist">
            <li class="nav-item">
                <button class="nav-link active" id="graph-tab" data-bs-toggle="tab" data-bs-target="#graph-pane" type="button">
                    <i class="fas fa-project-diagram"></i> Graph Visualization
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="functions-tab" data-bs-toggle="tab" data-bs-target="#functions-pane" type="button">
                    <i class="fas fa-code"></i> Functions
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="analysis-tab" data-bs-toggle="tab" data-bs-target="#analysis-pane" type="button">
                    <i class="fas fa-chart-bar"></i> CFG/PDG Analysis
                </button>
            </li>
        </ul>

        <div class="tab-content mt-3" id="mainTabContent">
            <!-- Graph Tab -->
            <div class="tab-pane fade show active" id="graph-pane">
                <div class="filter-section">
                    <div class="row">
                        <div class="col-md-3">
                            <label class="form-label">Node Type</label>
                            <select class="form-select" id="nodeTypeFilter">
                                <option value="">All Types</option>
                                <option value="Function">Function</option>
                                <option value="Class">Class</option>
                                <option value="Interface">Interface</option>
                                <option value="File">File</option>
                                <option value="Module">Module</option>
                            </select>
                        </div>
                        <div class="col-md-3">
                            <label class="form-label">Edge Type</label>
                            <select class="form-select" id="edgeTypeFilter">
                                <option value="">All Edges</option>
                                <option value="Calls">Calls</option>
                                <option value="Contains">Contains</option>
                                <option value="Implements">Implements</option>
                                <option value="Extends">Extends</option>
                            </select>
                        </div>
                        <div class="col-md-3">
                            <label class="form-label">Search Nodes</label>
                            <input type="text" class="form-control" id="nodeSearch" placeholder="Search by name...">
                        </div>
                        <div class="col-md-3">
                            <label class="form-label">&nbsp;</label>
                            <button class="btn btn-primary w-100" id="resetGraph">
                                <i class="fas fa-redo"></i> Reset View
                            </button>
                        </div>
                    </div>
                </div>

                <div id="graph-container">
                    <div id="graph"></div>
                    <div class="legend">
                        <strong>Legend:</strong>
                        <div class="legend-item">
                            <span class="legend-color" style="background-color: #667eea;"></span>
                            Function
                        </div>
                        <div class="legend-item">
                            <span class="legend-color" style="background-color: #f093fb;"></span>
                            Class
                        </div>
                        <div class="legend-item">
                            <span class="legend-color" style="background-color: #4facfe;"></span>
                            Interface
                        </div>
                        <div class="legend-item">
                            <span class="legend-color" style="background-color: #43e97b;"></span>
                            File
                        </div>
                        <div class="legend-item">
                            <span class="legend-color" style="background-color: #fa709a;"></span>
                            Module
                        </div>
                    </div>
                </div>
            </div>

            <!-- Functions Tab -->
            <div class="tab-pane fade" id="functions-pane">
                <div class="table-container">
                    <div class="mb-3">
                        <input type="text" class="form-control" id="functionSearch" placeholder="Search functions...">
                    </div>
                    <div class="table-responsive">
                        <table class="table table-hover" id="functionsTable">
                            <thead class="table-light">
                                <tr>
                                    <th>Name</th>
                                    <th>File</th>
                                    <th>Complexity</th>
                                    <th>Blast Radius</th>
                                    <th>PageRank</th>
                                    <th>Community</th>
                                </tr>
                            </thead>
                            <tbody id="functionsTableBody">
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>

            <!-- CFG/PDG Analysis Tab -->
            <div class="tab-pane fade" id="analysis-pane">
                <div class="table-container">
                    <div class="mb-3">
                        <input type="text" class="form-control" id="analysisSearch" placeholder="Search analyzed functions...">
                    </div>
                    <div class="table-responsive">
                        <table class="table table-hover" id="analysisTable">
                            <thead class="table-light">
                                <tr>
                                    <th>Function</th>
                                    <th>File</th>
                                    <th>CFG Blocks</th>
                                    <th>CFG Edges</th>
                                    <th>Dominators</th>
                                    <th>Data Deps</th>
                                    <th>Control Deps</th>
                                </tr>
                            </thead>
                            <tbody id="analysisTableBody">
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- Bootstrap Bundle with Popper -->
    <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/js/bootstrap.bundle.min.js"></script>

    <!-- D3.js -->
    <script src="https://d3js.org/d3.v7.min.js"></script>

    <script>
        // Embedded data
        const graphData = {{
            nodes: {nodes_json},
            edges: {edges_json}
        }};

        const analysisData = {analysis_json};

        // Initialize visualization
        let simulation;
        let svg;
        let link, node, label;

        function initGraph() {{
            const container = d3.select('#graph');
            const width = container.node().getBoundingClientRect().width;
            const height = 600;

            // Clear existing
            container.selectAll('*').remove();

            // Create SVG
            svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            const g = svg.append('g');

            // Add zoom
            const zoom = d3.zoom()
                .scaleExtent([0.1, 10])
                .on('zoom', (event) => {{
                    g.attr('transform', event.transform);
                }});

            svg.call(zoom);

            // Create links
            link = g.append('g')
                .selectAll('line')
                .data(graphData.edges)
                .join('line')
                .attr('class', d => `link ${{d.type}}`)
                .attr('marker-end', 'url(#arrowhead)');

            // Create nodes
            node = g.append('g')
                .selectAll('circle')
                .data(graphData.nodes)
                .join('circle')
                .attr('class', d => `node ${{d.type}}`)
                .attr('r', 8)
                .call(drag());

            // Create labels
            label = g.append('g')
                .selectAll('text')
                .data(graphData.nodes)
                .join('text')
                .attr('class', 'node-label')
                .attr('dx', 12)
                .attr('dy', 4)
                .text(d => d.name);

            // Arrow marker
            svg.append('defs').append('marker')
                .attr('id', 'arrowhead')
                .attr('viewBox', '-0 -5 10 10')
                .attr('refX', 20)
                .attr('refY', 0)
                .attr('orient', 'auto')
                .attr('markerWidth', 6)
                .attr('markerHeight', 6)
                .append('svg:path')
                .attr('d', 'M 0,-5 L 10 ,0 L 0,5')
                .attr('fill', '#999');

            // Tooltip
            const tooltip = d3.select('body').append('div')
                .attr('class', 'tooltip')
                .style('opacity', 0);

            node.on('mouseover', (event, d) => {{
                tooltip.transition().duration(200).style('opacity', .9);
                tooltip.html(createTooltipContent(d))
                    .style('left', (event.pageX + 10) + 'px')
                    .style('top', (event.pageY - 28) + 'px');
            }})
            .on('mouseout', () => {{
                tooltip.transition().duration(500).style('opacity', 0);
            }});

            // Force simulation
            simulation = d3.forceSimulation(graphData.nodes)
                .force('link', d3.forceLink(graphData.edges).id(d => d.id).distance(100))
                .force('charge', d3.forceManyBody().strength(-300))
                .force('center', d3.forceCenter(width / 2, height / 2))
                .force('collision', d3.forceCollide().radius(15))
                .on('tick', ticked);

            function ticked() {{
                link
                    .attr('x1', d => d.source.x)
                    .attr('y1', d => d.source.y)
                    .attr('x2', d => d.target.x)
                    .attr('y2', d => d.target.y);

                node
                    .attr('cx', d => d.x)
                    .attr('cy', d => d.y);

                label
                    .attr('x', d => d.x)
                    .attr('y', d => d.y);
            }}
        }}

        function drag() {{
            function dragstarted(event, d) {{
                if (!event.active) simulation.alphaTarget(0.3).restart();
                d.fx = d.x;
                d.fy = d.y;
            }}

            function dragged(event, d) {{
                d.fx = event.x;
                d.fy = event.y;
            }}

            function dragended(event, d) {{
                if (!event.active) simulation.alphaTarget(0);
                d.fx = null;
                d.fy = null;
            }}

            return d3.drag()
                .on('start', dragstarted)
                .on('drag', dragged)
                .on('end', dragended);
        }}

        function createTooltipContent(d) {{
            let content = `<strong>${{d.name}}</strong><br>Type: ${{d.type}}`;
            if (d.file_path) content += `<br>File: ${{d.file_path}}`;
            if (d.properties.cyclomatic) content += `<br>Complexity: ${{d.properties.cyclomatic}}`;
            if (d.properties.blast_radius_score) content += `<br>Blast Radius: ${{d.properties.blast_radius_score}}`;
            return content;
        }}

        function populateFunctionsTable() {{
            const tbody = document.getElementById('functionsTableBody');
            const functions = graphData.nodes.filter(n => n.type === 'Function');

            tbody.innerHTML = functions.map(f => `
                <tr>
                    <td><strong>${{f.name}}</strong></td>
                    <td><small>${{f.file_path || 'N/A'}}</small></td>
                    <td>
                        <span class="badge-custom ${{getComplexityBadge(f.properties.cyclomatic)}}">
                            ${{f.properties.cyclomatic || 'N/A'}}
                        </span>
                    </td>
                    <td>
                        <span class="badge-custom ${{getBlastRadiusBadge(f.properties.blast_radius_score)}}">
                            ${{f.properties.blast_radius_score || 'N/A'}}
                        </span>
                    </td>
                    <td>${{parseFloat(f.properties.pagerank || 0).toFixed(4)}}</td>
                    <td>${{f.properties.community || 'N/A'}}</td>
                </tr>
            `).join('');
        }}

        function populateAnalysisTable() {{
            const tbody = document.getElementById('analysisTableBody');

            tbody.innerHTML = analysisData.filter(a => a.has_analysis).map(a => `
                <tr>
                    <td><strong>${{a.function_name}}</strong></td>
                    <td><small>${{a.file_path}}</small></td>
                    <td>${{a.cfg_blocks || 0}}</td>
                    <td>${{a.cfg_edges || 0}}</td>
                    <td>${{a.dominators || 0}}</td>
                    <td>${{a.pdg_data_deps || 0}}</td>
                    <td>${{a.pdg_control_deps || 0}}</td>
                </tr>
            `).join('');
        }}

        function getComplexityBadge(complexity) {{
            const c = parseInt(complexity) || 0;
            if (c >= 10) return 'badge-high';
            if (c >= 5) return 'badge-medium';
            return 'badge-low';
        }}

        function getBlastRadiusBadge(score) {{
            const s = parseFloat(score) || 0;
            if (s >= 50) return 'badge-high';
            if (s >= 25) return 'badge-medium';
            return 'badge-low';
        }}

        // Filters
        document.getElementById('nodeTypeFilter').addEventListener('change', filterGraph);
        document.getElementById('edgeTypeFilter').addEventListener('change', filterGraph);
        document.getElementById('nodeSearch').addEventListener('input', filterGraph);

        document.getElementById('resetGraph').addEventListener('click', () => {{
            document.getElementById('nodeTypeFilter').value = '';
            document.getElementById('edgeTypeFilter').value = '';
            document.getElementById('nodeSearch').value = '';
            filterGraph();
        }});

        function filterGraph() {{
            const nodeType = document.getElementById('nodeTypeFilter').value;
            const edgeType = document.getElementById('edgeTypeFilter').value;
            const search = document.getElementById('nodeSearch').value.toLowerCase();

            node.style('display', d => {{
                const matchType = !nodeType || d.type === nodeType;
                const matchSearch = !search || d.name.toLowerCase().includes(search);
                return (matchType && matchSearch) ? null : 'none';
            }});

            label.style('display', d => {{
                const matchType = !nodeType || d.type === nodeType;
                const matchSearch = !search || d.name.toLowerCase().includes(search);
                return (matchType && matchSearch) ? null : 'none';
            }});

            link.style('display', d => {{
                return !edgeType || d.type === edgeType ? null : 'none';
            }});

            const visibleCount = graphData.nodes.filter(d => {{
                const matchType = !nodeType || d.type === nodeType;
                const matchSearch = !search || d.name.toLowerCase().includes(search);
                return matchType && matchSearch;
            }}).length;

            document.getElementById('selected-count').textContent = visibleCount;
        }}

        // Search functions table
        document.getElementById('functionSearch').addEventListener('input', (e) => {{
            const search = e.target.value.toLowerCase();
            const rows = document.querySelectorAll('#functionsTableBody tr');
            rows.forEach(row => {{
                const text = row.textContent.toLowerCase();
                row.style.display = text.includes(search) ? '' : 'none';
            }});
        }});

        // Search analysis table
        document.getElementById('analysisSearch').addEventListener('input', (e) => {{
            const search = e.target.value.toLowerCase();
            const rows = document.querySelectorAll('#analysisTableBody tr');
            rows.forEach(row => {{
                const text = row.textContent.toLowerCase();
                row.style.display = text.includes(search) ? '' : 'none';
            }});
        }});

        // Initialize
        initGraph();
        populateFunctionsTable();
        populateAnalysisTable();
        document.getElementById('selected-count').textContent = graphData.nodes.length;
    </script>
</body>
</html>"##,
        total_nodes = total_nodes,
        total_edges = total_edges,
        function_count = function_count,
        class_count = class_count,
        calls_count = calls_count,
        avg_complexity = avg_complexity,
        high_blast_radius_count = high_blast_radius_count,
        nodes_json = nodes_json,
        edges_json = edges_json,
        analysis_json = analysis_json,
    )
}
