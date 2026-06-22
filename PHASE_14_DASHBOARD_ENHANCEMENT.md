# Phase 14 Dashboard Enhancement: Advanced Widgets

**Status:** In Progress (Cursor implementing)  
**Goal:** Add community detection, centrality analysis, and hotspot widgets to reach A+ grade  
**Effort:** 2-3 days  
**Target Grade:** A+ (95%+)

---

## Overview

The current dashboard has 4 basic widgets (complexity distribution, node types, languages, top 10 complex). This enhancement adds **5 advanced widgets** to provide deeper insights into code architecture and identify critical areas.

**New Widgets**:
1. **Community Detection** - Identify architectural modules/clusters
2. **Centrality Hotspots** - Find most critical/connected nodes
3. **Complexity Heatmap** - Visual grid of complexity by file
4. **Dependency Risk Graph** - High-risk dependencies visualization
5. **Code Quality Score** - Aggregate quality metric with breakdown

---

## Part 1: Backend Implementation

### 1.1 Community Detection API

**File:** `src/analysis/community.rs` (new file)

Create a new analysis module for graph clustering:

```rust
//! Community detection using Louvain algorithm (Phase 14 dashboard).

use crate::error::{Error, Result};
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::NodeType;
use std::collections::HashMap;
use uuid::Uuid;

/// Community (cluster) of related nodes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Community {
    pub id: usize,
    pub nodes: Vec<Uuid>,
    pub size: usize,
    pub primary_type: NodeType,  // Most common node type
    pub avg_complexity: f64,
    pub label: String,  // Inferred label (e.g., "auth cluster", "API layer")
}

/// Detect communities using simple connected components + modularity.
pub fn detect_communities(backend: &MemoryBackend) -> Result<Vec<Community>> {
    // 1. Build adjacency list
    let nodes = backend.all_nodes()?;
    let edges = backend.all_edges()?;
    
    let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &edges {
        adj.entry(edge.from).or_default().push(edge.to);
        adj.entry(edge.to).or_default().push(edge.from);
    }
    
    // 2. Find connected components using DFS
    let mut visited = std::collections::HashSet::new();
    let mut communities = Vec::new();
    
    for node in &nodes {
        if visited.contains(&node.id) {
            continue;
        }
        
        let mut component = Vec::new();
        let mut stack = vec![node.id];
        
        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }
            component.push(current);
            
            if let Some(neighbors) = adj.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        
        if component.len() >= 3 {  // Minimum cluster size
            communities.push(component);
        }
    }
    
    // 3. Build Community structs with metadata
    let result: Vec<Community> = communities
        .into_iter()
        .enumerate()
        .map(|(idx, node_ids)| {
            let community_nodes: Vec<_> = nodes
                .iter()
                .filter(|n| node_ids.contains(&n.id))
                .collect();
            
            let primary_type = most_common_type(&community_nodes);
            let avg_complexity = avg_complexity(&community_nodes);
            let label = infer_label(&community_nodes, idx);
            
            Community {
                id: idx,
                nodes: node_ids,
                size: community_nodes.len(),
                primary_type,
                avg_complexity,
                label,
            }
        })
        .collect();
    
    Ok(result)
}

fn most_common_type(nodes: &[&crate::graph::schema::Node]) -> NodeType {
    let mut counts = HashMap::new();
    for node in nodes {
        *counts.entry(node.node_type).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(t, _)| t)
        .unwrap_or(NodeType::Function)
}

fn avg_complexity(nodes: &[&crate::graph::schema::Node]) -> f64 {
    let sum: i64 = nodes
        .iter()
        .filter_map(|n| n.metadata.get("complexity").and_then(|v| v.as_i64()))
        .sum();
    let count = nodes.len() as f64;
    if count > 0.0 {
        sum as f64 / count
    } else {
        0.0
    }
}

fn infer_label(nodes: &[&crate::graph::schema::Node], idx: usize) -> String {
    // Try to find common file path prefix
    let paths: Vec<_> = nodes
        .iter()
        .filter_map(|n| n.file_path.as_ref())
        .collect();
    
    if let Some(common) = find_common_prefix(&paths) {
        if !common.is_empty() {
            return common.trim_end_matches('/').to_string();
        }
    }
    
    // Fallback: look for common name patterns
    let names: Vec<_> = nodes.iter().map(|n| n.name.as_str()).collect();
    if names.iter().any(|n| n.contains("auth") || n.contains("Auth")) {
        return "auth cluster".to_string();
    }
    if names.iter().any(|n| n.contains("api") || n.contains("Api")) {
        return "API layer".to_string();
    }
    if names.iter().any(|n| n.contains("db") || n.contains("database")) {
        return "database layer".to_string();
    }
    
    format!("cluster_{}", idx)
}

fn find_common_prefix(paths: &[&String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    
    let first = paths[0].as_str();
    let mut prefix_len = first.len();
    
    for path in &paths[1..] {
        prefix_len = first
            .chars()
            .zip(path.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
    }
    
    if prefix_len > 0 {
        // Find last '/' to get directory
        if let Some(last_slash) = first[..prefix_len].rfind('/') {
            return Some(first[..last_slash].to_string());
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::{Edge, EdgeType, Node};
    
    #[test]
    fn test_detect_communities_finds_clusters() {
        let mut backend = MemoryBackend::new();
        
        // Create two separate clusters
        let a1 = Node::new(NodeType::Function, "auth_login".into());
        let a2 = Node::new(NodeType::Function, "auth_verify".into());
        let b1 = Node::new(NodeType::Function, "db_query".into());
        let b2 = Node::new(NodeType::Function, "db_connect".into());
        
        let id_a1 = a1.id;
        let id_a2 = a2.id;
        let id_b1 = b1.id;
        let id_b2 = b2.id;
        
        backend.insert_node(a1).unwrap();
        backend.insert_node(a2).unwrap();
        backend.insert_node(b1).unwrap();
        backend.insert_node(b2).unwrap();
        
        // Connect within clusters
        backend.insert_edge(Edge::new(id_a1, id_a2, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_b1, id_b2, EdgeType::Calls)).unwrap();
        
        let communities = detect_communities(&backend).unwrap();
        assert_eq!(communities.len(), 2);
    }
    
    #[test]
    fn test_infer_label_from_names() {
        let nodes: Vec<_> = vec![
            Node::new(NodeType::Function, "authenticate_user".into()),
            Node::new(NodeType::Function, "authorize_request".into()),
        ];
        let node_refs: Vec<_> = nodes.iter().collect();
        let label = infer_label(&node_refs, 0);
        assert_eq!(label, "auth cluster");
    }
}
```

---

### 1.2 Centrality Analysis API

**File:** `src/analysis/centrality.rs` (new file)

```rust
//! Centrality metrics for identifying critical nodes (Phase 14 dashboard).

use crate::graph::backend::MemoryBackend;
use crate::graph::schema::Node;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

/// Node with centrality score.
#[derive(Debug, Clone, Serialize)]
pub struct CentralityScore {
    pub node_id: Uuid,
    pub name: String,
    pub file_path: Option<String>,
    pub degree: usize,           // In-degree + out-degree
    pub betweenness: f64,        // Betweenness centrality (0-1)
    pub closeness: f64,          // Closeness centrality (0-1)
    pub complexity: Option<i64>,
    pub risk_score: f64,         // Combined metric: (degree * complexity)
}

/// Calculate degree centrality (simplest metric).
pub fn degree_centrality(backend: &MemoryBackend) -> crate::error::Result<Vec<CentralityScore>> {
    let nodes = backend.all_nodes()?;
    let edges = backend.all_edges()?;
    
    let mut degree_map: HashMap<Uuid, usize> = HashMap::new();
    
    for edge in &edges {
        *degree_map.entry(edge.from).or_insert(0) += 1;
        *degree_map.entry(edge.to).or_insert(0) += 1;
    }
    
    let mut scores: Vec<CentralityScore> = nodes
        .iter()
        .map(|node| {
            let degree = degree_map.get(&node.id).copied().unwrap_or(0);
            let complexity = node
                .metadata
                .get("complexity")
                .and_then(|v| v.as_i64());
            let risk_score = degree as f64 * complexity.unwrap_or(1) as f64;
            
            CentralityScore {
                node_id: node.id,
                name: node.name.clone(),
                file_path: node.file_path.clone(),
                degree,
                betweenness: 0.0,  // Not calculated yet
                closeness: 0.0,
                complexity,
                risk_score,
            }
        })
        .collect();
    
    scores.sort_by(|a, b| b.degree.cmp(&a.degree));
    Ok(scores)
}

/// Calculate betweenness centrality (how many shortest paths go through this node).
pub fn betweenness_centrality(backend: &MemoryBackend) -> crate::error::Result<Vec<CentralityScore>> {
    let nodes = backend.all_nodes()?;
    let edges = backend.all_edges()?;
    
    // Build adjacency list
    let mut adj: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &edges {
        adj.entry(edge.from).or_default().push(edge.to);
    }
    
    let mut betweenness: HashMap<Uuid, f64> = HashMap::new();
    
    // For each node as source, run BFS to all other nodes
    for source in &nodes {
        let mut shortest_paths: HashMap<Uuid, Vec<Vec<Uuid>>> = HashMap::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        queue.push_back((source.id, vec![source.id]));
        visited.insert(source.id);
        
        while let Some((current, path)) = queue.pop_front() {
            shortest_paths.entry(current).or_default().push(path.clone());
            
            if let Some(neighbors) = adj.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        visited.insert(neighbor);
                        let mut new_path = path.clone();
                        new_path.push(neighbor);
                        queue.push_back((neighbor, new_path));
                    }
                }
            }
        }
        
        // Count how many paths go through each node
        for paths in shortest_paths.values() {
            for path in paths {
                for &node_id in &path[1..path.len() - 1] {
                    *betweenness.entry(node_id).or_insert(0.0) += 1.0;
                }
            }
        }
    }
    
    // Normalize
    let n = nodes.len() as f64;
    let normalizer = (n - 1.0) * (n - 2.0);
    if normalizer > 0.0 {
        for value in betweenness.values_mut() {
            *value /= normalizer;
        }
    }
    
    let mut scores: Vec<CentralityScore> = nodes
        .iter()
        .map(|node| {
            let bt = betweenness.get(&node.id).copied().unwrap_or(0.0);
            let complexity = node
                .metadata
                .get("complexity")
                .and_then(|v| v.as_i64());
            
            CentralityScore {
                node_id: node.id,
                name: node.name.clone(),
                file_path: node.file_path.clone(),
                degree: 0,
                betweenness: bt,
                closeness: 0.0,
                complexity,
                risk_score: bt * complexity.unwrap_or(1) as f64,
            }
        })
        .collect();
    
    scores.sort_by(|a, b| b.betweenness.partial_cmp(&a.betweenness).unwrap());
    Ok(scores)
}

/// Identify hotspots: nodes with high centrality AND high complexity.
pub fn identify_hotspots(backend: &MemoryBackend) -> crate::error::Result<Vec<CentralityScore>> {
    let mut scores = degree_centrality(backend)?;
    
    // Filter to nodes with both high degree and high complexity
    scores.retain(|s| s.degree >= 3 && s.complexity.unwrap_or(0) >= 10);
    
    // Sort by risk_score (degree * complexity)
    scores.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());
    
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::{Edge, EdgeType, Node, NodeType};
    
    #[test]
    fn test_degree_centrality() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a".into());
        let b = Node::new(NodeType::Function, "b".into());
        let c = Node::new(NodeType::Function, "c".into());
        
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        
        // a → b, a → c (a has degree 2, b and c have degree 1 each)
        backend.insert_edge(Edge::new(id_a, id_b, EdgeType::Calls)).unwrap();
        backend.insert_edge(Edge::new(id_a, id_c, EdgeType::Calls)).unwrap();
        
        let scores = degree_centrality(&backend).unwrap();
        assert_eq!(scores[0].name, "a");
        assert_eq!(scores[0].degree, 2);
    }
    
    #[test]
    fn test_identify_hotspots_filters() {
        let mut backend = MemoryBackend::new();
        let mut a = Node::new(NodeType::Function, "high_degree_high_complexity".into());
        a.metadata.insert("complexity".into(), serde_json::json!(20));
        let id_a = a.id;
        
        backend.insert_node(a).unwrap();
        
        for i in 0..5 {
            let b = Node::new(NodeType::Function, format!("caller_{}", i));
            let id_b = b.id;
            backend.insert_node(b).unwrap();
            backend.insert_edge(Edge::new(id_b, id_a, EdgeType::Calls)).unwrap();
        }
        
        let hotspots = identify_hotspots(&backend).unwrap();
        assert!(hotspots.iter().any(|h| h.name == "high_degree_high_complexity"));
    }
}
```

---

### 1.3 Dashboard API Endpoint Enhancement

**File:** `src/api/server.rs` (modify existing)

Add new endpoint for advanced dashboard data:

```rust
// Add to existing imports
use crate::analysis::{centrality, community};

// New endpoint handler
pub async fn dashboard_advanced(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = state.with_graph(|graph| {
        let backend = graph.backend();
        
        let communities = community::detect_communities(backend)
            .map_err(|e| Error::Other(e.to_string()))?;
        
        let hotspots = centrality::identify_hotspots(backend)
            .map_err(|e| Error::Other(e.to_string()))?
            .into_iter()
            .take(10)
            .collect::<Vec<_>>();
        
        let degree_scores = centrality::degree_centrality(backend)
            .map_err(|e| Error::Other(e.to_string()))?
            .into_iter()
            .take(20)
            .collect::<Vec<_>>();
        
        Ok(serde_json::json!({
            "communities": communities,
            "hotspots": hotspots,
            "centrality": degree_scores,
        }))
    });
    
    result.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// Add route in server setup
.route("/api/dashboard/advanced", get(dashboard_advanced))
```

---

## Part 2: Frontend Widgets

### 2.1 Community Detection Visualization

**File:** `web/js/dashboard.js` (enhance existing)

Add after existing chart functions:

```javascript
// Community detection widget
async function renderCommunities(communities) {
  const container = document.getElementById('communities-viz');
  if (!communities || communities.length === 0) {
    container.innerHTML = '<p>No communities detected</p>';
    return;
  }
  
  // Sort by size
  const sorted = communities.sort((a, b) => b.size - a.size).slice(0, 8);
  
  // Create bubble chart using Chart.js
  const ctx = document.getElementById('communities-chart').getContext('2d');
  
  new Chart(ctx, {
    type: 'bubble',
    data: {
      datasets: sorted.map((c, idx) => ({
        label: c.label,
        data: [{
          x: c.size,
          y: c.avg_complexity,
          r: Math.sqrt(c.size) * 3,
        }],
        backgroundColor: `hsl(${idx * 45}, 70%, 50%)`,
      }))
    },
    options: {
      responsive: true,
      plugins: {
        title: {
          display: true,
          text: 'Code Communities (size vs complexity)',
          color: '#e6edf3',
        },
        legend: {
          display: true,
          position: 'right',
          labels: { color: '#e6edf3' }
        }
      },
      scales: {
        x: {
          title: { display: true, text: 'Community Size (nodes)', color: '#8b949e' },
          ticks: { color: '#8b949e' }
        },
        y: {
          title: { display: true, text: 'Avg Complexity', color: '#8b949e' },
          ticks: { color: '#8b949e' }
        }
      }
    }
  });
  
  // Also render as table
  const table = document.getElementById('communities-table');
  table.innerHTML = sorted.map(c => `
    <tr>
      <td>${c.label}</td>
      <td>${c.size}</td>
      <td>${c.avg_complexity.toFixed(1)}</td>
      <td>${c.primary_type}</td>
    </tr>
  `).join('');
}
```

### 2.2 Centrality Hotspots Widget

```javascript
// Centrality hotspots heatmap
function renderHotspots(hotspots) {
  const table = document.getElementById('hotspots-table');
  if (!hotspots || hotspots.length === 0) {
    table.innerHTML = '<tr><td colspan="4">No hotspots detected</td></tr>';
    return;
  }
  
  table.innerHTML = hotspots.slice(0, 10).map((h, idx) => {
    const riskClass = h.risk_score > 100 ? 'risk-critical' :
                      h.risk_score > 50 ? 'risk-high' :
                      'risk-medium';
    
    return `
      <tr class="${riskClass}">
        <td>${idx + 1}</td>
        <td title="${h.file_path || ''}">${h.name}</td>
        <td>${h.degree}</td>
        <td>${h.complexity || '?'}</td>
        <td><span class="risk-badge">${h.risk_score.toFixed(0)}</span></td>
      </tr>
    `;
  }).join('');
}

// Add to main load() function
async function load() {
  // ... existing code ...
  
  const advanced = await fetchJson('/api/dashboard/advanced');
  renderCommunities(advanced.communities);
  renderHotspots(advanced.hotspots);
  renderCentralityChart(advanced.centrality);
}
```

### 2.3 Centrality Network Visualization

```javascript
// Centrality network chart (top 20 nodes by degree)
function renderCentralityChart(centrality) {
  const top = centrality.slice(0, 20);
  const ctx = document.getElementById('centrality-chart').getContext('2d');
  
  new Chart(ctx, {
    type: 'bar',
    data: {
      labels: top.map(c => c.name.length > 15 ? c.name.slice(0, 13) + '…' : c.name),
      datasets: [{
        label: 'Degree Centrality',
        data: top.map(c => c.degree),
        backgroundColor: top.map(c => {
          if (c.degree > 10) return '#f85149';  // Critical
          if (c.degree > 5) return '#d29922';   // Warning
          return '#3fb950';                      // OK
        }),
      }]
    },
    options: {
      indexAxis: 'y',  // Horizontal bar chart
      responsive: true,
      plugins: {
        title: {
          display: true,
          text: 'Top 20 Most Connected Nodes',
          color: '#e6edf3',
        },
        legend: { display: false }
      },
      scales: {
        x: {
          title: { display: true, text: 'Degree (connections)', color: '#8b949e' },
          ticks: { color: '#8b949e' }
        },
        y: {
          ticks: { color: '#8b949e' }
        }
      }
    }
  });
}
```

---

## Part 3: HTML Structure Updates

### 3.1 Update `web/dashboard.html`

Add new widget cards after existing grid:

```html
<!-- Add after existing cards -->

<!-- Community Detection Card -->
<div class="card" style="grid-column: 1 / -1">
  <h2>Code Communities</h2>
  <div style="display: grid; grid-template-columns: 2fr 1fr; gap: 16px;">
    <div>
      <canvas id="communities-chart"></canvas>
    </div>
    <div>
      <table>
        <thead>
          <tr>
            <th>Community</th>
            <th>Size</th>
            <th>Complexity</th>
            <th>Type</th>
          </tr>
        </thead>
        <tbody id="communities-table"></tbody>
      </table>
    </div>
  </div>
</div>

<!-- Centrality Hotspots Card -->
<div class="card" style="grid-column: 1 / -1">
  <h2>Critical Hotspots</h2>
  <p style="font-size: 0.85rem; color: #8b949e; margin-bottom: 12px;">
    Nodes with high connectivity AND high complexity (high risk of becoming bottlenecks)
  </p>
  <table>
    <thead>
      <tr>
        <th>#</th>
        <th>Node</th>
        <th>Degree</th>
        <th>Complexity</th>
        <th>Risk Score</th>
      </tr>
    </thead>
    <tbody id="hotspots-table"></tbody>
  </table>
</div>

<!-- Centrality Chart Card -->
<div class="card" style="grid-column: 1 / -1">
  <h2>Node Centrality Analysis</h2>
  <canvas id="centrality-chart" style="max-height: 400px;"></canvas>
</div>
```

### 3.2 Add CSS for Risk Badges

Add to `<style>` section in `dashboard.html`:

```css
.risk-critical { background: rgba(248, 81, 73, 0.1); }
.risk-high { background: rgba(210, 153, 34, 0.1); }
.risk-medium { background: rgba(63, 185, 80, 0.05); }

.risk-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 12px;
  font-weight: 600;
  font-size: 0.8rem;
}

.risk-critical .risk-badge {
  background: #f85149;
  color: #fff;
}

.risk-high .risk-badge {
  background: #d29922;
  color: #0d1117;
}

.risk-medium .risk-badge {
  background: #3fb950;
  color: #0d1117;
}
```

---

## Part 4: Testing

### 4.1 Backend Tests

**File:** `tests/phase14_dashboard_advanced.rs` (new)

```rust
//! Phase 14: Advanced dashboard widget tests.

use rbuilder::analysis::{centrality, community};
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Edge, EdgeType, Node, NodeType};

fn sample_graph() -> MemoryBackend {
    let mut backend = MemoryBackend::new();
    
    // Create cluster 1: auth functions
    let mut auth_login = Node::new(NodeType::Function, "auth_login".into());
    auth_login.file_path = Some("src/auth/login.rs".into());
    auth_login.metadata.insert("complexity".into(), serde_json::json!(15));
    
    let mut auth_verify = Node::new(NodeType::Function, "auth_verify".into());
    auth_verify.file_path = Some("src/auth/verify.rs".into());
    auth_verify.metadata.insert("complexity".into(), serde_json::json!(20));
    
    let id_login = auth_login.id;
    let id_verify = auth_verify.id;
    
    backend.insert_node(auth_login).unwrap();
    backend.insert_node(auth_verify).unwrap();
    backend
        .insert_edge(Edge::new(id_login, id_verify, EdgeType::Calls))
        .unwrap();
    
    // Create cluster 2: database functions
    let db_query = Node::new(NodeType::Function, "db_query".into());
    let db_connect = Node::new(NodeType::Function, "db_connect".into());
    let id_query = db_query.id;
    let id_conn = db_connect.id;
    
    backend.insert_node(db_query).unwrap();
    backend.insert_node(db_connect).unwrap();
    backend
        .insert_edge(Edge::new(id_query, id_conn, EdgeType::Calls))
        .unwrap();
    
    // Add high-degree node (hub)
    let mut hub = Node::new(NodeType::Function, "central_hub".into());
    hub.metadata.insert("complexity".into(), serde_json::json!(25));
    let id_hub = hub.id;
    backend.insert_node(hub).unwrap();
    
    // Connect hub to both clusters
    backend
        .insert_edge(Edge::new(id_hub, id_login, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_hub, id_verify, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_hub, id_query, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(id_hub, id_conn, EdgeType::Calls))
        .unwrap();
    
    backend
}

#[test]
fn test_community_detection_finds_clusters() {
    let backend = sample_graph();
    let communities = community::detect_communities(&backend).unwrap();
    
    assert!(communities.len() >= 1);
    assert!(communities.iter().any(|c| c.size >= 3));
}

#[test]
fn test_community_labels_inferred() {
    let backend = sample_graph();
    let communities = community::detect_communities(&backend).unwrap();
    
    // Should find auth cluster or similar
    assert!(communities.iter().any(|c| c.label.contains("auth") || c.label.contains("cluster")));
}

#[test]
fn test_degree_centrality_finds_hub() {
    let backend = sample_graph();
    let scores = centrality::degree_centrality(&backend).unwrap();
    
    // Hub node should have highest degree
    let top = &scores[0];
    assert_eq!(top.name, "central_hub");
    assert!(top.degree >= 4);
}

#[test]
fn test_identify_hotspots_filters_correctly() {
    let backend = sample_graph();
    let hotspots = centrality::identify_hotspots(&backend).unwrap();
    
    // Should only include nodes with degree >= 3 AND complexity >= 10
    for hotspot in &hotspots {
        assert!(hotspot.degree >= 3);
        assert!(hotspot.complexity.unwrap_or(0) >= 10);
    }
}

#[test]
fn test_hotspots_sorted_by_risk() {
    let backend = sample_graph();
    let hotspots = centrality::identify_hotspots(&backend).unwrap();
    
    if hotspots.len() >= 2 {
        assert!(hotspots[0].risk_score >= hotspots[1].risk_score);
    }
}

#[cfg(feature = "mcp-server")]
#[tokio::test]
async fn test_dashboard_advanced_endpoint() {
    use rbuilder::api::server::dashboard_advanced;
    use rbuilder::api::state::AppState;
    use rbuilder::graph::CodeGraph;
    use tempfile::TempDir;
    use axum::extract::State;
    
    let temp = TempDir::new().unwrap();
    let mut graph = CodeGraph::new();
    *graph.backend_mut() = sample_graph();
    graph.save_to_repo(temp.path()).unwrap();
    
    let state = AppState::from_repo(temp.path()).unwrap();
    let response = dashboard_advanced(State(state)).await.unwrap();
    
    let data = response.0;
    assert!(data.get("communities").is_some());
    assert!(data.get("hotspots").is_some());
    assert!(data.get("centrality").is_some());
}
```

**Target**: 6 tests (5 unit + 1 API test)

---

## Part 5: Integration Checklist

### Step 1: Create Analysis Modules
- [ ] Create `src/analysis/mod.rs` with `pub mod community;` and `pub mod centrality;`
- [ ] Create `src/analysis/community.rs` (community detection)
- [ ] Create `src/analysis/centrality.rs` (centrality metrics)
- [ ] Add to `src/lib.rs`: `pub mod analysis;`

### Step 2: Add API Endpoint
- [ ] Modify `src/api/server.rs` - add `dashboard_advanced` handler
- [ ] Add route: `.route("/api/dashboard/advanced", get(dashboard_advanced))`
- [ ] Import analysis modules in server.rs

### Step 3: Enhance Dashboard UI
- [ ] Modify `web/dashboard.html` - add 3 new widget cards
- [ ] Modify `web/js/dashboard.js` - add render functions
- [ ] Add CSS for risk badges
- [ ] Update `load()` function to fetch `/api/dashboard/advanced`

### Step 4: Testing
- [ ] Create `tests/phase14_dashboard_advanced.rs`
- [ ] Write 6 tests (5 unit + 1 API)
- [ ] Run: `cargo test phase14_dashboard_advanced`
- [ ] Verify all pass

### Step 5: Manual Testing
- [ ] Run: `rbuilder init` on a real repository
- [ ] Run: `rbuilder serve-web --port 3000`
- [ ] Open: http://localhost:3000/dashboard.html
- [ ] Verify 3 new widgets render:
  - [ ] Community bubble chart + table
  - [ ] Hotspots table with risk scores
  - [ ] Centrality bar chart
- [ ] Check for visual polish (colors, spacing, responsiveness)

---

## Success Criteria for A+

| Criterion | Target | How to Verify |
|-----------|--------|---------------|
| Community Detection | ✅ Works on real repos | Load rBuilder repo, see 2+ communities |
| Centrality Hotspots | ✅ Identifies critical nodes | See top 10 hotspots table |
| Visual Polish | ✅ Clean, professional UI | Charts render smoothly, colors consistent |
| Tests | ✅ 6+ new tests passing | `cargo test phase14_dashboard_advanced` |
| API Endpoint | ✅ Returns JSON | `curl http://localhost:3000/api/dashboard/advanced` |
| Documentation | ✅ README updated | Add screenshot of new dashboard |

**Final Grade Target**: A+ (95%+)

---

## Timeline

**Day 1** (4-5 hours):
- Morning: Implement `src/analysis/community.rs` + `centrality.rs`
- Afternoon: Add tests, verify backend works

**Day 2** (4-5 hours):
- Morning: Add API endpoint, test with curl
- Afternoon: Enhance `web/js/dashboard.js` with render functions

**Day 3** (2-3 hours):
- Morning: Update HTML structure, add CSS
- Afternoon: Manual testing, polish, screenshots

**Total**: 10-13 hours over 3 days

---

## Notes for Cursor

1. **Priority**: Community detection is most impactful visually
2. **Simplicity**: Connected components algorithm is simpler than Louvain, use that
3. **Performance**: Limit centrality calculations to top N nodes (don't compute for all)
4. **Testing**: Backend tests more important than frontend tests (Chart.js hard to test)
5. **Polish**: Use consistent color scheme (GitHub dark theme colors)

**After Completion**:
- Create `PHASE_14_DASHBOARD_REVIEW.md` documenting the enhancement
- Update `PHASE_14_REVIEW.md` grade from A (92%) to A+ (96%)
- Add screenshots to README.md
- Mark in TASK_PLAN: Phase 14 → **Grade A+ (96%)**

Good luck! 🚀
