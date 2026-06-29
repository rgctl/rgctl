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
    let mut dataflow_functions = Vec::new();

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
                                "has_dataflow": pdg_data_deps > 0 || pdg_control_deps > 0,
                            })
                        }).collect();
                        analysis_data = json!(summaries);

                        // Also extract functions with interesting dataflows for visualization
                        // Collect functions with taint first, then others
                        let mut taint_funcs: Vec<_> = analyses.iter()
                            .filter(|a| {
                                a.get("taint")
                                    .and_then(|t| t.as_array())
                                    .map(|t| !t.is_empty())
                                    .unwrap_or(false)
                            })
                            .map(|a| {
                                json!({
                                    "function_id": a.get("function_id"),
                                    "function_name": a.get("function_name"),
                                    "file_path": a.get("file_path"),
                                    "cfg": a.get("cfg"),
                                    "pdg": a.get("pdg"),
                                    "dominance": a.get("dominance"),
                                    "taint": a.get("taint"),
                                })
                            })
                            .collect();

                        // Add functions with PDG/CFG data (up to 100 total)
                        let remaining = 100 - taint_funcs.len().min(100);
                        let mut other_funcs: Vec<_> = analyses.iter()
                            .filter(|a| {
                                let has_taint = a.get("taint")
                                    .and_then(|t| t.as_array())
                                    .map(|t| !t.is_empty())
                                    .unwrap_or(false);
                                if has_taint { return false; } // Skip already added

                                let has_data = a.get("pdg")
                                    .and_then(|p| p.get("data_deps"))
                                    .and_then(|d| d.as_array())
                                    .map(|d| !d.is_empty())
                                    .unwrap_or(false);
                                let has_cfg = a.get("cfg").is_some();
                                has_data || has_cfg
                            })
                            .take(remaining)
                            .map(|a| {
                                json!({
                                    "function_id": a.get("function_id"),
                                    "function_name": a.get("function_name"),
                                    "file_path": a.get("file_path"),
                                    "cfg": a.get("cfg"),
                                    "pdg": a.get("pdg"),
                                    "dominance": a.get("dominance"),
                                    "taint": a.get("taint"),
                                })
                            })
                            .collect();

                        taint_funcs.append(&mut other_funcs);
                        dataflow_functions = taint_funcs;
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
        &dataflow_functions,
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
    dataflow_functions: &[serde_json::Value],
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
    let dataflow_json = serde_json::to_string(&dataflow_functions).unwrap_or_else(|_| "[]".to_string());

    // Extract sample data for query guide examples
    let sample_functions: Vec<String> = nodes.iter()
        .filter(|n| n.get("type").and_then(|t| t.as_str()) == Some("Function"))
        .take(3)
        .filter_map(|n| n.get("name").and_then(|s| s.as_str()).map(String::from))
        .collect();

    let sample_function = sample_functions.first().unwrap_or(&"exampleFunction".to_string()).clone();

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
            <li class="nav-item">
                <button class="nav-link" id="dataflow-tab" data-bs-toggle="tab" data-bs-target="#dataflow-pane" type="button">
                    <i class="fas fa-project-diagram"></i> Dataflow
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="taint-tab" data-bs-toggle="tab" data-bs-target="#taint-pane" type="button">
                    <i class="fas fa-bug"></i> Taint Analysis
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="guide-tab" data-bs-toggle="tab" data-bs-target="#guide-pane" type="button">
                    <i class="fas fa-book"></i> Query Guide
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="slicing-tab" data-bs-toggle="tab" data-bs-target="#slicing-pane" type="button">
                    <i class="fas fa-cut"></i> Program Slicing
                </button>
            </li>
            <li class="nav-item">
                <button class="nav-link" id="blast-tab" data-bs-toggle="tab" data-bs-target="#blast-pane" type="button">
                    <i class="fas fa-radiation"></i> Blast Radius
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
                        <div class="col-md-2">
                            <label class="form-label">Search Nodes</label>
                            <input type="text" class="form-control" id="nodeSearch" placeholder="Search by name...">
                        </div>
                        <div class="col-md-2">
                            <label class="form-label">Interprocedural</label>
                            <div class="form-check form-switch mt-2">
                                <input class="form-check-input" type="checkbox" id="showCallEdges" checked>
                                <label class="form-check-label" for="showCallEdges">Show Calls</label>
                            </div>
                        </div>
                        <div class="col-md-2">
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

            <!-- Dataflow Visualization Tab -->
            <div class="tab-pane fade" id="dataflow-pane">
                <div class="row mb-3">
                    <div class="col-md-8">
                        <label class="form-label">Select Function</label>
                        <select class="form-select" id="dataflowFunctionSelect">
                            <option value="">-- Select a function with dataflows --</option>
                        </select>
                    </div>
                    <div class="col-md-4">
                        <label class="form-label">View</label>
                        <select class="form-select" id="dataflowViewSelect">
                            <option value="dataflow">Data Flow (CFG + PDG)</option>
                            <option value="dominator">Dominator Tree</option>
                            <option value="combined">Combined View</option>
                        </select>
                    </div>
                </div>
                <div id="dataflowViz" style="border: 1px solid #dee2e6; border-radius: 4px; background: white; min-height: 600px; position: relative;">
                    <div style="position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); text-align: center; color: #6c757d;">
                        <i class="fas fa-project-diagram fa-3x mb-3"></i>
                        <p>Select a function to visualize its dataflow</p>
                    </div>
                </div>
                <div class="mt-3">
                    <h6>Legend</h6>
                    <div id="dataflowLegend" class="d-flex gap-3">
                        <div><span style="display: inline-block; width: 20px; height: 20px; background: #667eea; border-radius: 50%;"></span> Statement/Block</div>
                        <div><span style="display: inline-block; width: 20px; height: 2px; background: #999; margin: 9px 0;"></span> Control Flow</div>
                        <div><span style="display: inline-block; width: 20px; height: 2px; background: #f093fb; margin: 9px 0;"></span> Data Flow</div>
                        <div><span style="display: inline-block; width: 20px; height: 2px; background: #4facfe; margin: 9px 0;"></span> Control Dependency</div>
                    </div>
                    <div id="dominatorLegend" class="d-flex gap-3" style="display:none;">
                        <div><span style="display: inline-block; width: 16px; height: 16px; background: #667eea; border-radius: 50%;"></span> Regular Block</div>
                        <div><span style="display: inline-block; width: 24px; height: 24px; background: #f093fb; border-radius: 50%;"></span> Has Dominance Frontier</div>
                        <div><span style="display: inline-block; width: 20px; height: 2px; background: #999; margin: 9px 0;"></span> Dominates</div>
                    </div>
                </div>
            </div>

            <!-- Program Slicing Tab -->
            <div class="tab-pane fade" id="slicing-pane">
                <div class="card">
                    <div class="card-body">
                        <h5 class="card-title">Backward Program Slicing</h5>
                        <p class="text-muted">Compute backward slices to see which statements affect a variable at a given point</p>

                        <div class="row g-3 mb-3">
                            <div class="col-md-4">
                                <label class="form-label">Function</label>
                                <select class="form-select" id="sliceFunctionSelect">
                                    <option value="">-- Select function --</option>
                                </select>
                            </div>
                            <div class="col-md-4">
                                <label class="form-label">Line Number</label>
                                <input type="number" class="form-control" id="sliceLineInput" placeholder="e.g., 42" min="1">
                            </div>
                            <div class="col-md-4">
                                <label class="form-label">Variable</label>
                                <input type="text" class="form-control" id="sliceVariableInput" placeholder="e.g., result">
                            </div>
                        </div>
                        <button class="btn btn-primary" id="computeSliceBtn">
                            <i class="fas fa-calculator"></i> Compute Slice
                        </button>

                        <div id="sliceResults" class="mt-4" style="display:none;">
                            <h6>Slice Results</h6>
                            <div class="alert alert-info">
                                <strong>Criterion:</strong> <span id="sliceCriterion"></span><br>
                                <strong>Slice Size:</strong> <span id="sliceSize"></span> statements<br>
                                <strong>Reduction:</strong> <span id="sliceReduction"></span>% of code excluded
                            </div>
                            <div id="sliceViz" style="border: 1px solid #dee2e6; border-radius: 4px; background: white; min-height: 500px;">
                            </div>
                        </div>

                        <div class="alert alert-secondary mt-3">
                            <strong><i class="fas fa-info-circle"></i> How it works:</strong>
                            Backward slicing traverses data and control dependencies to find all statements that could affect the selected variable.
                            Highlighted nodes show the computed slice.
                        </div>
                    </div>
                </div>
            </div>

            <!-- Blast Radius Tab -->
            <div class="tab-pane fade" id="blast-pane">
                <div class="card">
                    <div class="card-body">
                        <h5 class="card-title">Blast Radius Analysis</h5>
                        <p class="text-muted">Visualize the impact zone of changing a function - shows transitive callers and affected code</p>

                        <div class="mb-3">
                            <label class="form-label">Select Function</label>
                            <select class="form-select" id="blastFunctionSelect">
                                <option value="">-- Select a function --</option>
                            </select>
                        </div>

                        <div id="blastResults" class="mt-4" style="display:none;">
                            <div class="row">
                                <div class="col-md-3">
                                    <div class="card text-center">
                                        <div class="card-body">
                                            <h3 id="blastScore" class="text-danger">0</h3>
                                            <small class="text-muted">Blast Radius Score</small>
                                        </div>
                                    </div>
                                </div>
                                <div class="col-md-3">
                                    <div class="card text-center">
                                        <div class="card-body">
                                            <h3 id="blastDirectCallers">0</h3>
                                            <small class="text-muted">Direct Callers</small>
                                        </div>
                                    </div>
                                </div>
                                <div class="col-md-3">
                                    <div class="card text-center">
                                        <div class="card-body">
                                            <h3 id="blastImpactZone">0</h3>
                                            <small class="text-muted">Impact Zone Size</small>
                                        </div>
                                    </div>
                                </div>
                                <div class="col-md-3">
                                    <div class="card text-center">
                                        <div class="card-body">
                                            <h3 id="blastDataFlowDepth">0</h3>
                                            <small class="text-muted">Data Flow Depth</small>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <div id="blastViz" class="mt-4" style="border: 1px solid #dee2e6; border-radius: 4px; background: white; min-height: 600px;">
                            </div>

                            <div class="mt-3">
                                <h6>Legend</h6>
                                <div class="d-flex gap-3">
                                    <div><span style="display: inline-block; width: 20px; height: 20px; background: #ff6b6b; border-radius: 50%;"></span> Target Function (High Impact)</div>
                                    <div><span style="display: inline-block; width: 20px; height: 20px; background: #ffa94d; border-radius: 50%;"></span> Direct Caller</div>
                                    <div><span style="display: inline-block; width: 20px; height: 20px; background: #fab005; border-radius: 50%;"></span> Transitive Caller (Impact Zone)</div>
                                    <div><span style="display: inline-block; width: 20px; height: 20px; background: #dee2e6; border-radius: 50%;"></span> Other</div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Taint Analysis Tab -->
            <div class="tab-pane fade" id="taint-pane">
                <div class="card">
                    <div class="card-body">
                        <h5 class="card-title">Taint Analysis</h5>
                        <p class="text-muted">Track data flow from sources (user input) to sinks (sensitive operations)</p>

                        <div class="row mb-3">
                            <div class="col-md-12">
                                <label for="taint-function-select" class="form-label">Select Function:</label>
                                <select class="form-select" id="taint-function-select">
                                    <option value="">-- Choose a function --</option>
                                </select>
                            </div>
                        </div>

                        <div id="taint-summary" class="alert alert-secondary" style="display: none;">
                            <h6><i class="fas fa-chart-bar"></i> Summary</h6>
                            <div id="taint-summary-content"></div>
                        </div>

                        <div id="taint-flows-container" style="display: none;">
                            <h6>Taint Flows</h6>
                            <div id="taint-flows-list"></div>
                        </div>

                        <div id="taint-empty" class="alert alert-info">
                            <h6><i class="fas fa-info-circle"></i> No Taint Flows Detected</h6>
                            <p>No vulnerable data flows were found in the analyzed functions.</p>
                            <p><strong>What taint analysis detects:</strong></p>
                            <ul>
                                <li>SQL Injection risks (user input → database queries)</li>
                                <li>XSS vulnerabilities (user input → HTML output)</li>
                                <li>Command Injection (user input → system commands)</li>
                                <li>Code Execution (user input → eval/exec)</li>
                            </ul>
                        </div>
                    </div>
                </div>
            </div>

            <!-- Query Guide Tab -->
            <div class="tab-pane fade" id="guide-pane">
                <div class="card">
                    <div class="card-body">
                        <h4 class="card-title">rBuilder Query Guide</h4>
                        <p class="text-muted">Learn how to query your code graph using the rBuilder CLI</p>

                        <h5 class="mt-4">Quick Start</h5>
                        <p>Run queries from your terminal:</p>
                        <pre class="bg-light p-3 rounded"><code>rbuilder gql "your-query-here"</code></pre>

                        <h5 class="mt-4">Query Syntax</h5>
                        <div class="table-responsive">
                            <table class="table table-sm">
                                <thead class="table-light">
                                    <tr>
                                        <th>Pattern</th>
                                        <th>Description</th>
                                        <th>Example</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <tr>
                                        <td><code>type:TYPE</code></td>
                                        <td>Filter by node type</td>
                                        <td><code>type:Function</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>name:NAME</code></td>
                                        <td>Find by exact name</td>
                                        <td><code>name:{sample_function}</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>name_suffix:SUFFIX</code></td>
                                        <td>Find by name pattern</td>
                                        <td><code>name_suffix:Service</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>signature:*pattern*</code></td>
                                        <td>Search signatures</td>
                                        <td><code>signature:*String*</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>return_type:TYPE</code></td>
                                        <td>Filter by return type</td>
                                        <td><code>return_type:Result</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>module:NAME</code></td>
                                        <td>Filter by module</td>
                                        <td><code>module:api</code></td>
                                    </tr>
                                    <tr>
                                        <td><code>A|B</code></td>
                                        <td>Combine filters (AND)</td>
                                        <td><code>type:Function|return_type:Result</code></td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>

                        <h5 class="mt-4">Common Shortcuts</h5>
                        <div class="row">
                            <div class="col-md-6">
                                <ul class="list-unstyled">
                                    <li><code>functions</code> - All functions</li>
                                    <li><code>classes</code> - All classes</li>
                                    <li><code>files</code> - All files</li>
                                </ul>
                            </div>
                            <div class="col-md-6">
                                <ul class="list-unstyled">
                                    <li><code>config</code> - Configuration files</li>
                                    <li><code>all</code> - Everything</li>
                                </ul>
                            </div>
                        </div>

                        <h5 class="mt-4">Examples from This Codebase</h5>

                        <div class="accordion" id="examplesAccordion">
                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example1">
                                        Find all functions
                                    </button>
                                </h2>
                                <div id="example1" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "type:Function"</code></pre>
                                        <p class="text-muted mb-0">Returns: {total_functions} functions in this graph</p>
                                    </div>
                                </div>
                            </div>

                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example2">
                                        Find a specific function
                                    </button>
                                </h2>
                                <div id="example2" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "name:{sample_function}"</code></pre>
                                        <p class="text-muted mb-0">Finds the exact function named "{sample_function}"</p>
                                    </div>
                                </div>
                            </div>

                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example3">
                                        Find all classes
                                    </button>
                                </h2>
                                <div id="example3" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "type:Class"</code></pre>
                                        <p class="text-muted mb-0">Returns: {total_classes} classes in this graph</p>
                                    </div>
                                </div>
                            </div>

                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example4">
                                        Find services by naming pattern
                                    </button>
                                </h2>
                                <div id="example4" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "name_suffix:Service"</code></pre>
                                        <p class="text-muted mb-0">Finds all classes/functions ending with "Service"</p>
                                    </div>
                                </div>
                            </div>

                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example5">
                                        Find functions returning errors
                                    </button>
                                </h2>
                                <div id="example5" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "type:Function|return_type:Result"</code></pre>
                                        <p class="text-muted mb-0">Functions that return Result types (error handling)</p>
                                    </div>
                                </div>
                            </div>

                            <div class="accordion-item">
                                <h2 class="accordion-header">
                                    <button class="accordion-button collapsed" type="button" data-bs-toggle="collapse" data-bs-target="#example6">
                                        High complexity functions
                                    </button>
                                </h2>
                                <div id="example6" class="accordion-collapse collapse" data-bs-parent="#examplesAccordion">
                                    <div class="accordion-body">
                                        <p class="mb-2">Combine GQL with analysis properties:</p>
                                        <pre class="bg-light p-3 rounded mb-2"><code>rbuilder gql "type:Function" | jq '.[] | select(.properties.cyclomatic != null and (.properties.cyclomatic | tonumber) > 5)'</code></pre>
                                        <p class="text-muted mb-0">Use jq to filter by complexity metrics stored in node properties</p>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <h5 class="mt-4">Other Commands</h5>
                        <div class="table-responsive">
                            <table class="table table-sm">
                                <thead class="table-light">
                                    <tr>
                                        <th>Command</th>
                                        <th>Description</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <tr>
                                        <td><code>rbuilder ask "question"</code></td>
                                        <td>Natural language queries (AI-powered)</td>
                                    </tr>
                                    <tr>
                                        <td><code>rbuilder blast-radius SYMBOL</code></td>
                                        <td>Impact analysis for a function/class</td>
                                    </tr>
                                    <tr>
                                        <td><code>rbuilder slice --file FILE --line LINE --variable VAR</code></td>
                                        <td>Program slicing for dataflow analysis</td>
                                    </tr>
                                    <tr>
                                        <td><code>rbuilder stats</code></td>
                                        <td>Show graph statistics and reports</td>
                                    </tr>
                                    <tr>
                                        <td><code>rbuilder chat</code></td>
                                        <td>Interactive conversational mode</td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>

                        <div class="alert alert-info mt-4">
                            <strong><i class="fas fa-info-circle"></i> Tip:</strong> All queries can be combined with standard Unix tools like <code>jq</code>, <code>grep</code>, and <code>wc</code> for advanced filtering and analysis.
                        </div>
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
        const dataflowFunctions = {dataflow_json};

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
        document.getElementById('showCallEdges').addEventListener('change', filterGraph);

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

        // Dataflow visualization
        function populateDataflowSelect() {{
            const select = document.getElementById('dataflowFunctionSelect');
            dataflowFunctions.forEach(func => {{
                const option = document.createElement('option');
                option.value = func.function_id;
                option.textContent = `${{func.function_name}} (${{func.pdg?.data_deps?.length || 0}} flows)`;
                select.appendChild(option);
            }});
        }}

        function renderDataflow(functionId) {{
            const func = dataflowFunctions.find(f => f.function_id === functionId);
            if (!func) return;

            const viewType = document.getElementById('dataflowViewSelect').value;

            if (viewType === 'dominator') {{
                renderDominatorTree(func);
                return;
            }} else if (viewType === 'combined') {{
                renderCombinedView(func);
                return;
            }}

            // Default: dataflow view
            const container = d3.select('#dataflowViz');
            container.selectAll('*').remove();

            const width = container.node().getBoundingClientRect().width;
            const height = 600;

            const svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            const g = svg.append('g');

            // Add zoom
            const zoom = d3.zoom()
                .scaleExtent([0.1, 4])
                .on('zoom', (event) => g.attr('transform', event.transform));
            svg.call(zoom);

            // Build dataflow graph
            const cfg = func.cfg || {{}};
            const pdg = func.pdg || {{}};

            const nodes = [];
            const edges = [];

            // Add PDG nodes (these have UUIDs that match the edge references)
            const pdgNodes = pdg.nodes || {{}};
            Object.entries(pdgNodes).forEach(([nodeId, node]) => {{
                const stmt = node.statement || {{}};
                nodes.push({{
                    id: nodeId,
                    blockId: node.block,
                    label: stmt.text?.substring(0, 50) || 'statement',
                    line: stmt.line,
                    type: 'statement',
                    defined: (node.defined_vars || []).join(', '),
                    used: (node.used_vars || []).join(', ')
                }});
            }});

            // Add CFG edges (control flow) - these connect CFG blocks
            const cfgEdges = cfg.edges || [];
            const blockToNodes = {{}};
            Object.entries(pdgNodes).forEach(([nodeId, node]) => {{
                const blockId = node.block;
                if (!blockToNodes[blockId]) blockToNodes[blockId] = [];
                blockToNodes[blockId].push(nodeId);
            }});

            cfgEdges.forEach(edge => {{
                // Connect nodes in source block to nodes in target block
                const sourceNodes = blockToNodes[edge.from] || [];
                const targetNodes = blockToNodes[edge.to] || [];
                if (sourceNodes.length > 0 && targetNodes.length > 0) {{
                    // Connect last node of source block to first node of target block
                    edges.push({{
                        source: sourceNodes[sourceNodes.length - 1],
                        target: targetNodes[0],
                        type: 'control',
                        edgeType: edge.edge_type
                    }});
                }}
            }});

            // Add PDG data dependencies
            const dataDeps = pdg.data_deps || [];
            dataDeps.forEach(dep => {{
                edges.push({{
                    source: dep.from,
                    target: dep.to,
                    type: 'data',
                    variable: dep.variable
                }});
            }});

            // Add PDG control dependencies
            const controlDeps = pdg.control_deps || [];
            controlDeps.forEach(dep => {{
                edges.push({{
                    source: dep.from,
                    target: dep.to,
                    type: 'control_dep'
                }});
            }});

            // Debug: check for undefined node references
            const nodeIds = new Set(nodes.map(n => n.id));
            const invalidEdges = edges.filter(e => !nodeIds.has(e.source) || !nodeIds.has(e.target));
            if (invalidEdges.length > 0) {{
                console.warn(`Found ${{invalidEdges.length}} edges with invalid node references`);
                console.log('Invalid edges:', invalidEdges);
                console.log('Available node IDs:', Array.from(nodeIds));
            }}

            // Filter out edges that reference non-existent nodes
            const validEdges = edges.filter(e => nodeIds.has(e.source) && nodeIds.has(e.target));

            console.log(`Function: ${{func.function_name}}`);
            console.log(`Nodes: ${{nodes.length}}, Edges: ${{edges.length}} (valid: ${{validEdges.length}})`);

            if (nodes.length === 0) {{
                container.html('<div style="padding: 20px; text-align: center; color: #999;">No nodes to display for this function</div>');
                return;
            }}

            // Create force simulation
            const simulation = d3.forceSimulation(nodes)
                .force('link', d3.forceLink(validEdges).id(d => d.id).distance(100))
                .force('charge', d3.forceManyBody().strength(-300))
                .force('center', d3.forceCenter(width / 2, height / 2));

            // Draw edges
            const link = g.append('g')
                .selectAll('line')
                .data(validEdges)
                .join('line')
                .attr('stroke', d => {{
                    if (d.type === 'data') return '#f093fb';
                    if (d.type === 'control_dep') return '#4facfe';
                    return '#999';
                }})
                .attr('stroke-width', d => d.type === 'data' ? 2 : 1)
                .attr('stroke-dasharray', d => d.type === 'control' ? '5,5' : null);

            // Draw nodes
            const node = g.append('g')
                .selectAll('circle')
                .data(nodes)
                .join('circle')
                .attr('r', 6)
                .attr('fill', '#667eea')
                .call(d3.drag()
                    .on('start', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0.3).restart();
                        d.fx = d.x;
                        d.fy = d.y;
                    }})
                    .on('drag', (event, d) => {{
                        d.fx = event.x;
                        d.fy = event.y;
                    }})
                    .on('end', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0);
                        d.fx = null;
                        d.fy = null;
                    }}));

            // Add labels
            const label = g.append('g')
                .selectAll('text')
                .data(nodes)
                .join('text')
                .attr('dx', 8)
                .attr('dy', 4)
                .style('font-size', '10px')
                .text(d => d.label);

            // Add tooltips with variable information
            node.append('title')
                .text(d => {{
                    let tooltip = `${{d.label}}\\nLine: ${{d.line}}`;
                    if (d.defined && d.defined.length > 0) {{
                        tooltip += `\\n\\nDEFINES: ${{d.defined}}`;
                    }}
                    if (d.used && d.used.length > 0) {{
                        tooltip += `\\n\\nUSES: ${{d.used}}`;
                    }}
                    tooltip += `\\n\\nClick to select node`;
                    return tooltip;
                }});

            link.append('title')
                .text(d => {{
                    if (d.type === 'data') return `Data: ${{d.variable}}`;
                    if (d.type === 'control_dep') return 'Control Dependency';
                    return `CFG: ${{d.edgeType}}`;
                }});

            simulation.on('tick', () => {{
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
            }});
        }}

        function renderDominatorTree(func) {{
            const container = d3.select('#dataflowViz');
            container.selectAll('*').remove();

            const width = container.node().getBoundingClientRect().width;
            const height = 600;

            const dominance = func.dominance || {{}};
            const idom = dominance.idom || {{}};
            const frontiers = dominance.frontiers || {{}};
            const cfg = func.cfg || {{}};
            const blocks = cfg.blocks || {{}};

            if (Object.keys(idom).length === 0) {{
                container.html('<div style="padding: 20px; text-align: center; color: #999;">No dominator tree data available for this function</div>');
                return;
            }}

            const svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            const g = svg.append('g');

            const zoom = d3.zoom()
                .scaleExtent([0.1, 4])
                .on('zoom', (event) => g.attr('transform', event.transform));
            svg.call(zoom);

            // Build tree structure from idom
            const nodes = [];
            const edges = [];
            const blockToNode = {{}};

            Object.entries(idom).forEach(([blockId, dominator]) => {{
                if (!blockToNode[blockId]) {{
                    const block = blocks[blockId] || {{}};
                    const stmtCount = (block.statements || []).length;
                    blockToNode[blockId] = {{
                        id: blockId,
                        label: `Block ${{Object.keys(blockToNode).length + 1}}\\n(${{stmtCount}} stmts)`,
                        frontierSize: (frontiers[blockId] || []).length,
                        isDominator: blockId === dominator
                    }};
                    nodes.push(blockToNode[blockId]);
                }}

                if (dominator !== blockId && !blockToNode[dominator]) {{
                    const block = blocks[dominator] || {{}};
                    const stmtCount = (block.statements || []).length;
                    blockToNode[dominator] = {{
                        id: dominator,
                        label: `Block ${{Object.keys(blockToNode).length + 1}}\\n(${{stmtCount}} stmts)`,
                        frontierSize: (frontiers[dominator] || []).length,
                        isDominator: true
                    }};
                    nodes.push(blockToNode[dominator]);
                }}

                if (dominator !== blockId) {{
                    edges.push({{
                        source: dominator,
                        target: blockId
                    }});
                }}
            }});

            // Use tree layout
            const treeLayout = d3.tree()
                .size([width - 100, height - 100]);

            // Build hierarchy
            const root = nodes.find(n => n.isDominator && !edges.some(e => e.target === n.id));
            if (!root) {{
                container.html('<div style="padding: 20px; text-align: center; color: #999;">Cannot determine root of dominator tree</div>');
                return;
            }}

            // Build tree structure for d3.hierarchy
            function buildHierarchy(nodeId) {{
                const node = blockToNode[nodeId];
                const children = edges.filter(e => e.source === nodeId).map(e => buildHierarchy(e.target));
                return {{
                    id: nodeId,
                    label: node.label,
                    frontierSize: node.frontierSize,
                    children: children.length > 0 ? children : undefined
                }};
            }}

            const hierarchyData = buildHierarchy(root.id);
            const treeRoot = d3.hierarchy(hierarchyData);
            treeLayout(treeRoot);

            // Draw edges
            g.selectAll('.link')
                .data(treeRoot.links())
                .join('path')
                .attr('class', 'link')
                .attr('d', d3.linkVertical()
                    .x(d => d.x + 50)
                    .y(d => d.y + 50))
                .attr('fill', 'none')
                .attr('stroke', '#999')
                .attr('stroke-width', 2);

            // Draw nodes
            const nodeGroup = g.selectAll('.node')
                .data(treeRoot.descendants())
                .join('g')
                .attr('class', 'node')
                .attr('transform', d => `translate(${{d.x + 50}},${{d.y + 50}})`);

            nodeGroup.append('circle')
                .attr('r', d => d.data.frontierSize > 0 ? 12 : 8)
                .attr('fill', d => d.data.frontierSize > 0 ? '#f093fb' : '#667eea')
                .attr('stroke', '#fff')
                .attr('stroke-width', 2);

            nodeGroup.append('text')
                .attr('dy', -15)
                .attr('text-anchor', 'middle')
                .style('font-size', '11px')
                .text(d => d.data.label);

            nodeGroup.append('title')
                .text(d => `${{d.data.label}}\\nDominance Frontier: ${{d.data.frontierSize}} blocks`);
        }}

        function renderCombinedView(func) {{
            // TODO: Render both dataflow and dominator tree side-by-side
            renderDataflow(func.function_id);
        }}

        document.getElementById('dataflowFunctionSelect').addEventListener('change', (e) => {{
            if (e.target.value) {{
                renderDataflow(e.target.value);
            }}
        }});

        document.getElementById('dataflowViewSelect').addEventListener('change', (e) => {{
            const viewType = e.target.value;
            // Update legend visibility
            document.getElementById('dataflowLegend').style.display = viewType === 'dominator' ? 'none' : 'flex';
            document.getElementById('dominatorLegend').style.display = viewType === 'dominator' ? 'flex' : 'none';

            const selected = document.getElementById('dataflowFunctionSelect').value;
            if (selected) {{
                renderDataflow(selected);
            }}
        }});

        populateDataflowSelect();

        // Program Slicing
        function populateSliceFunctionSelect() {{
            const select = document.getElementById('sliceFunctionSelect');
            dataflowFunctions.forEach(func => {{
                const pdg = func.pdg || {{}};
                if (Object.keys(pdg.nodes || {{}}).length > 0) {{
                    const option = document.createElement('option');
                    option.value = func.function_id;
                    option.textContent = func.function_name;
                    select.appendChild(option);
                }}
            }});
        }}

        function computeBackwardSlice(func, line, variable) {{
            const pdg = func.pdg || {{}};
            const nodes = pdg.nodes || {{}};
            const dataDeps = pdg.data_deps || [];
            const controlDeps = pdg.control_deps || [];

            // Find criterion node (matching line and using the variable)
            let criterionNode = null;
            for (const [nodeId, node] of Object.entries(nodes)) {{
                if (node.statement && node.statement.line === line) {{
                    const usedVars = node.used_vars || [];
                    if (usedVars.includes(variable)) {{
                        criterionNode = nodeId;
                        break;
                    }}
                }}
            }}

            if (!criterionNode) {{
                return {{ success: false, error: `No statement found at line ${{line}} using variable "${{variable}}"` }};
            }}

            // Backward slice traversal
            const slice = new Set([criterionNode]);
            const worklist = [criterionNode];

            while (worklist.length > 0) {{
                const current = worklist.pop();

                // Follow data dependencies backward
                dataDeps.filter(dep => dep.to === current).forEach(dep => {{
                    if (!slice.has(dep.from)) {{
                        slice.add(dep.from);
                        worklist.push(dep.from);
                    }}
                }});

                // Follow control dependencies backward
                controlDeps.filter(dep => dep.to === current).forEach(dep => {{
                    if (!slice.has(dep.from)) {{
                        slice.add(dep.from);
                        worklist.push(dep.from);
                    }}
                }});
            }}

            const totalNodes = Object.keys(nodes).length;
            const sliceSize = slice.size;
            const reductionPercent = ((totalNodes - sliceSize) / totalNodes * 100).toFixed(1);

            return {{
                success: true,
                criterionNode,
                slice: Array.from(slice),
                sliceSize,
                reductionPercent,
                totalNodes
            }};
        }}

        function renderSlice(func, sliceResult) {{
            const container = d3.select('#sliceViz');
            container.selectAll('*').remove();

            const width = container.node().getBoundingClientRect().width;
            const height = 500;

            const pdg = func.pdg || {{}};
            const cfg = func.cfg || {{}};
            const nodes = pdg.nodes || {{}};
            const dataDeps = pdg.data_deps || [];
            const controlDeps = pdg.control_deps || [];

            const svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            const g = svg.append('g');

            const zoom = d3.zoom()
                .scaleExtent([0.1, 4])
                .on('zoom', (event) => g.attr('transform', event.transform));
            svg.call(zoom);

            // Build graph
            const graphNodes = [];
            const graphEdges = [];
            const sliceSet = new Set(sliceResult.slice);

            Object.entries(nodes).forEach(([nodeId, node]) => {{
                const stmt = node.statement || {{}};
                graphNodes.push({{
                    id: nodeId,
                    label: stmt.text?.substring(0, 40) || 'statement',
                    line: stmt.line,
                    inSlice: sliceSet.has(nodeId),
                    isCriterion: nodeId === sliceResult.criterionNode
                }});
            }});

            // Add edges
            dataDeps.forEach(dep => {{
                graphEdges.push({{
                    source: dep.from,
                    target: dep.to,
                    type: 'data'
                }});
            }});

            controlDeps.forEach(dep => {{
                graphEdges.push({{
                    source: dep.from,
                    target: dep.to,
                    type: 'control'
                }});
            }});

            // Filter to only show edges relevant to slice
            const validEdges = graphEdges.filter(e => {{
                return sliceSet.has(e.source) && sliceSet.has(e.target);
            }});

            const simulation = d3.forceSimulation(graphNodes)
                .force('link', d3.forceLink(validEdges).id(d => d.id).distance(100))
                .force('charge', d3.forceManyBody().strength(-300))
                .force('center', d3.forceCenter(width / 2, height / 2));

            // Draw edges
            const link = g.append('g')
                .selectAll('line')
                .data(validEdges)
                .join('line')
                .attr('stroke', d => d.type === 'data' ? '#f093fb' : '#4facfe')
                .attr('stroke-width', 2)
                .attr('opacity', 0.6);

            // Draw nodes
            const node = g.append('g')
                .selectAll('circle')
                .data(graphNodes)
                .join('circle')
                .attr('r', d => d.isCriterion ? 10 : 8)
                .attr('fill', d => {{
                    if (d.isCriterion) return '#ff6b6b';
                    if (d.inSlice) return '#51cf66';
                    return '#dee2e6';
                }})
                .attr('stroke', '#fff')
                .attr('stroke-width', 2)
                .attr('opacity', d => d.inSlice ? 1 : 0.3)
                .call(d3.drag()
                    .on('start', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0.3).restart();
                        d.fx = d.x;
                        d.fy = d.y;
                    }})
                    .on('drag', (event, d) => {{
                        d.fx = event.x;
                        d.fy = event.y;
                    }})
                    .on('end', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0);
                        d.fx = null;
                        d.fy = null;
                    }}));

            // Add labels
            const label = g.append('g')
                .selectAll('text')
                .data(graphNodes.filter(n => n.inSlice))
                .join('text')
                .attr('dx', 12)
                .attr('dy', 4)
                .style('font-size', '10px')
                .text(d => d.label);

            node.append('title')
                .text(d => `${{d.label}}\\nLine: ${{d.line}}\\n${{d.inSlice ? 'IN SLICE' : 'NOT IN SLICE'}}`);

            simulation.on('tick', () => {{
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
            }});
        }}

        document.getElementById('computeSliceBtn').addEventListener('click', () => {{
            const functionId = document.getElementById('sliceFunctionSelect').value;
            const line = parseInt(document.getElementById('sliceLineInput').value);
            const variable = document.getElementById('sliceVariableInput').value.trim();

            if (!functionId) {{
                alert('Please select a function');
                return;
            }}
            if (!line || line < 1) {{
                alert('Please enter a valid line number');
                return;
            }}
            if (!variable) {{
                alert('Please enter a variable name');
                return;
            }}

            const func = dataflowFunctions.find(f => f.function_id === functionId);
            if (!func) return;

            const result = computeBackwardSlice(func, line, variable);

            if (!result.success) {{
                alert(result.error);
                return;
            }}

            // Show results
            document.getElementById('sliceResults').style.display = 'block';
            document.getElementById('sliceCriterion').textContent = `Variable "${{variable}}" at line ${{line}}`;
            document.getElementById('sliceSize').textContent = result.sliceSize;
            document.getElementById('sliceReduction').textContent = result.reductionPercent;

            renderSlice(func, result);
        }});

        populateSliceFunctionSelect();

        // Blast Radius Visualization
        function populateBlastFunctionSelect() {{
            const select = document.getElementById('blastFunctionSelect');
            const functions = graphData.nodes.filter(n => n.type === 'Function');
            functions.forEach(func => {{
                const option = document.createElement('option');
                option.value = func.id;
                option.textContent = func.name;
                select.appendChild(option);
            }});
        }}

        function computeBlastRadius(targetNodeId) {{
            // Find all nodes that call the target (directly or transitively)
            const directCallers = new Set();
            const impactZone = new Set();
            const worklist = [targetNodeId];
            const visited = new Set([targetNodeId]);

            // Find direct callers
            graphData.edges.filter(e => e.type === 'Calls' && e.target === targetNodeId).forEach(e => {{
                directCallers.add(e.source);
            }});

            // Find transitive callers (BFS)
            const queue = Array.from(directCallers);
            while (queue.length > 0) {{
                const current = queue.shift();
                if (visited.has(current)) continue;
                visited.add(current);
                impactZone.add(current);

                graphData.edges.filter(e => e.type === 'Calls' && e.target === current).forEach(e => {{
                    if (!visited.has(e.source)) {{
                        queue.push(e.source);
                    }}
                }});
            }}

            return {{
                directCallers: Array.from(directCallers),
                impactZone: Array.from(impactZone)
            }};
        }}

        function renderBlastRadius(targetNode, blastData) {{
            const container = d3.select('#blastViz');
            container.selectAll('*').remove();

            const width = container.node().getBoundingClientRect().width;
            const height = 600;

            // Build subgraph
            const relevantNodes = new Set([targetNode.id, ...blastData.directCallers, ...blastData.impactZone]);
            const nodes = graphData.nodes.filter(n => relevantNodes.has(n.id));
            const edges = graphData.edges.filter(e => relevantNodes.has(e.source) && relevantNodes.has(e.target));

            const svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            const g = svg.append('g');

            const zoom = d3.zoom()
                .scaleExtent([0.1, 4])
                .on('zoom', (event) => g.attr('transform', event.transform));
            svg.call(zoom);

            const simulation = d3.forceSimulation(nodes)
                .force('link', d3.forceLink(edges).id(d => d.id).distance(150))
                .force('charge', d3.forceManyBody().strength(-500))
                .force('center', d3.forceCenter(width / 2, height / 2));

            // Draw edges
            const link = g.append('g')
                .selectAll('line')
                .data(edges)
                .join('line')
                .attr('stroke', '#999')
                .attr('stroke-width', 2)
                .attr('marker-end', 'url(#arrow)');

            // Arrow marker
            svg.append('defs').append('marker')
                .attr('id', 'arrow')
                .attr('viewBox', '0 -5 10 10')
                .attr('refX', 20)
                .attr('refY', 0)
                .attr('markerWidth', 6)
                .attr('markerHeight', 6)
                .attr('orient', 'auto')
                .append('path')
                .attr('d', 'M0,-5L10,0L0,5')
                .attr('fill', '#999');

            // Draw nodes
            const node = g.append('g')
                .selectAll('circle')
                .data(nodes)
                .join('circle')
                .attr('r', d => d.id === targetNode.id ? 12 : 8)
                .attr('fill', d => {{
                    if (d.id === targetNode.id) return '#ff6b6b';
                    if (blastData.directCallers.includes(d.id)) return '#ffa94d';
                    if (blastData.impactZone.includes(d.id)) return '#fab005';
                    return '#dee2e6';
                }})
                .attr('stroke', '#fff')
                .attr('stroke-width', 2)
                .call(d3.drag()
                    .on('start', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0.3).restart();
                        d.fx = d.x;
                        d.fy = d.y;
                    }})
                    .on('drag', (event, d) => {{
                        d.fx = event.x;
                        d.fy = event.y;
                    }})
                    .on('end', (event, d) => {{
                        if (!event.active) simulation.alphaTarget(0);
                        d.fx = null;
                        d.fy = null;
                    }}));

            // Add labels
            const label = g.append('g')
                .selectAll('text')
                .data(nodes)
                .join('text')
                .attr('dx', 15)
                .attr('dy', 4)
                .style('font-size', '11px')
                .text(d => d.name);

            node.append('title')
                .text(d => {{
                    let type = 'Other';
                    if (d.id === targetNode.id) type = 'TARGET';
                    else if (blastData.directCallers.includes(d.id)) type = 'Direct Caller';
                    else if (blastData.impactZone.includes(d.id)) type = 'Impact Zone';
                    return `${{d.name}}\\nType: ${{type}}`;
                }});

            simulation.on('tick', () => {{
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
            }});
        }}

        document.getElementById('blastFunctionSelect').addEventListener('change', (e) => {{
            const nodeId = e.target.value;
            if (!nodeId) return;

            const targetNode = graphData.nodes.find(n => n.id === nodeId);
            if (!targetNode) return;

            const blastData = computeBlastRadius(nodeId);

            // Show metrics
            document.getElementById('blastResults').style.display = 'block';
            const score = parseFloat(targetNode.properties?.blast_radius_score || 0);
            document.getElementById('blastScore').textContent = score.toFixed(1);
            document.getElementById('blastDirectCallers').textContent = blastData.directCallers.length;
            document.getElementById('blastImpactZone').textContent = blastData.impactZone.length;
            document.getElementById('blastDataFlowDepth').textContent = targetNode.properties?.blast_radius_data_flow_depth || 0;

            // Color code the score
            const scoreElem = document.getElementById('blastScore');
            if (score >= 50) {{
                scoreElem.className = 'text-danger';
            }} else if (score >= 25) {{
                scoreElem.className = 'text-warning';
            }} else {{
                scoreElem.className = 'text-success';
            }}

            renderBlastRadius(targetNode, blastData);
        }});

        populateBlastFunctionSelect();

        // Taint Analysis
        function populateTaintFunctionSelect() {{
            const select = document.getElementById('taint-function-select');
            select.innerHTML = '<option value="">-- Choose a function --</option>';

            const functionsWithTaint = dataflowFunctions.filter(f => f.taint && f.taint.length > 0);

            if (functionsWithTaint.length === 0) {{
                document.getElementById('taint-empty').style.display = 'block';
                return;
            }}

            document.getElementById('taint-empty').style.display = 'none';

            functionsWithTaint.forEach(func => {{
                const option = document.createElement('option');
                option.value = func.function_id;
                option.textContent = `${{func.function_name}} (${{func.taint.length}} flows)`;
                select.appendChild(option);
            }});
        }}

        function renderTaintAnalysis(func) {{
            const flows = func.taint || [];

            if (flows.length === 0) {{
                document.getElementById('taint-summary').style.display = 'none';
                document.getElementById('taint-flows-container').style.display = 'none';
                return;
            }}

            const vulnerable = flows.filter(f => f.sanitizers.length === 0);

            // Summary
            const summaryHtml = `
                <div class="row">
                    <div class="col-md-4">
                        <strong>Total Flows:</strong> ${{flows.length}}
                    </div>
                    <div class="col-md-4">
                        <strong class="text-danger">Vulnerable:</strong> ${{vulnerable.length}}
                    </div>
                    <div class="col-md-4">
                        <strong class="text-success">Sanitized:</strong> ${{flows.length - vulnerable.length}}
                    </div>
                </div>
            `;
            document.getElementById('taint-summary-content').innerHTML = summaryHtml;
            document.getElementById('taint-summary').style.display = 'block';

            // Flows list
            const flowsHtml = flows.map((flow, idx) => {{
                const isVulnerable = flow.sanitizers.length === 0;
                const severityClass = flow.severity >= 9 ? 'danger' : flow.severity >= 7 ? 'warning' : 'info';
                const sourceType = flow.source_type.replace(/([A-Z])/g, ' $1').trim();
                const sinkType = flow.sink_type.replace(/([A-Z])/g, ' $1').trim();

                return `
                    <div class="card mb-3 border-${{severityClass}}">
                        <div class="card-header bg-${{severityClass}} bg-opacity-10">
                            <div class="d-flex justify-content-between align-items-center">
                                <h6 class="mb-0">
                                    Flow #${{idx + 1}}: ${{flow.variable || 'data'}}
                                    ${{isVulnerable ? '<span class="badge bg-danger ms-2">Vulnerable</span>' : '<span class="badge bg-success ms-2">Sanitized</span>'}}
                                </h6>
                                <span class="badge bg-${{severityClass}}">Severity: ${{flow.severity}}/10</span>
                            </div>
                        </div>
                        <div class="card-body">
                            <div class="row mb-2">
                                <div class="col-md-6">
                                    <strong><i class="fas fa-sign-in-alt text-primary"></i> Source:</strong> ${{sourceType}}
                                </div>
                                <div class="col-md-6">
                                    <strong><i class="fas fa-sign-out-alt text-danger"></i> Sink:</strong> ${{sinkType}}
                                </div>
                            </div>

                            ${{flow.sanitizers.length > 0 ? `
                            <div class="alert alert-success mb-2 py-2">
                                <strong><i class="fas fa-shield-alt"></i> Sanitizers:</strong>
                                ${{flow.sanitizers.map(s => {{
                                    if (typeof s === 'string') return s;
                                    if (s.TypeCast) return `TypeCast(${{s.TypeCast}})`;
                                    if (s.Validation) return `Validation(${{s.Validation}})`;
                                    if (s.HtmlEscape) return 'HTML Escape';
                                    if (s.ShellEscape) return 'Shell Escape';
                                    if (s.SqlParameterize) return 'SQL Parameterize';
                                    return JSON.stringify(s);
                                }}).join(', ')}}
                            </div>
                            ` : ''}}

                            <div>
                                <strong><i class="fas fa-route"></i> Flow Path:</strong>
                                <div class="mt-2 small">
                                    ${{flow.path.map((nodeId, i) => {{
                                        const node = func.pdg?.nodes?.[nodeId];
                                        const stmt = node?.statement;
                                        const arrow = i < flow.path.length - 1 ? '<i class="fas fa-arrow-down ms-2"></i>' : '';
                                        return `
                                            <div class="mb-2 p-2 bg-light rounded">
                                                <code class="small">${{stmt?.text || nodeId}}</code>
                                                <span class="text-muted ms-2">(line ${{stmt?.line || '?'}})</span>
                                                ${{arrow}}
                                            </div>
                                        `;
                                    }}).join('')}}
                                </div>
                            </div>
                        </div>
                    </div>
                `;
            }}).join('');

            document.getElementById('taint-flows-list').innerHTML = flowsHtml;
            document.getElementById('taint-flows-container').style.display = 'block';
        }}

        document.getElementById('taint-function-select').addEventListener('change', (e) => {{
            const funcId = e.target.value;
            if (!funcId) return;

            const func = dataflowFunctions.find(f => f.function_id === funcId);
            if (!func) return;

            renderTaintAnalysis(func);
        }});

        populateTaintFunctionSelect();

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
        dataflow_json = dataflow_json,
        sample_function = sample_function,
        total_functions = function_count,
        total_classes = class_count,
    )
}
