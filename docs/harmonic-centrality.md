That is a serious and impressive analysis stack. Having Tree-sitter AST parsing fed into PetGraph for CFG, PDG, program slicing, and blast radius in Rust gives you a massive performance advantage over traditional Python or Java static analysis tools.

Here is the algorithmic breakdown and production-ready Rust implementation for **Harmonic Centrality** tailored specifically for your `petgraph` pipeline.

---

### 1. The Mathematical Algorithm

For a directed software graph $G = (V, E)$, the **Harmonic Centrality** of a node $u$ is defined as the sum of the reciprocals of the shortest path distances from $u$ to all other nodes $v$:

$$H(u) = \sum_{v \in V \setminus \{u\}} \frac{1}{d(u, v)}$$

Where:

* $d(u, v)$ is the shortest path distance from node $u$ to node $v$.
* If node $v$ is unreachable from $u$ (which happens constantly in directed software dependency graphs), $d(u, v) = \infty$, and mathematically $\frac{1}{\infty} = 0$.

#### Normalization

To compare scores across subgraphs of different sizes, we normalize $H(u)$ by dividing by $|V| - 1$ (the maximum possible score if node $u$ had a direct edge of distance `1` to every other node):

$$H_{norm}(u) = \frac{1}{|V| - 1} \sum_{v \in V \setminus \{u\}} \frac{1}{d(u, v)}$$

---

### 2. Algorithmic Complexity & Strategy in PetGraph

Since you are running this over ASTs, CFGs, and PDGs, graph sizes can range from hundreds of nodes (module level) to hundreds of thousands of nodes (instruction level).

We can approach this in two ways:

1. **Unweighted Graphs (Topology only - Recommended for basic PDG/CFG):** Use **Breadth-First Search (BFS)** from each node. Time Complexity: $\mathcal{O}(V \times (V + E))$. This is much faster than running Floyd-Warshall ($\mathcal{O}(V^3)$).
2. **Weighted Graphs (e.g., edges weighted by call frequency or blast radius criticality):** Use **Dijkstra's Algorithm** from each node. Time Complexity: $\mathcal{O}(V \times (E + V \log V))$.

---

### 3. Idiomatic Rust Implementation with PetGraph

Here is a complete, optimized implementation for both unweighted and weighted graphs using `petgraph`. You can drop this directly into your analysis crate.

```rust
use petgraph::visit::{EdgeRef, IntoEdges, IntoNodeReferences, NodeIndexable, Visitable};
use petgraph::algo::dijkstra;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::Directed;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Computes the NORMALIZED Unweighted Harmonic Centrality for all nodes in a directed graph.
/// 
/// Time Complexity: O(V * (V + E)) via All-Pairs BFS.
/// Perfect for structural PDGs and CFGs where edge weights are uniform (distance = 1).
pub fn unweighted_harmonic_centrality<N, E>(
    graph: &Graph<N, E, Directed>,
) -> HashMap<NodeIndex, f64> {
    let mut centrality = HashMap::new();
    let num_nodes = graph.node_count();

    if num_nodes <= 1 {
        for node in graph.node_indices() {
            centrality.insert(node, 0.0);
        }
        return centrality;
    }

    let norm_factor = 1.0 / (num_nodes as f64 - 1.0);

    for start_node in graph.node_indices() {
        let mut sum_reciprocal_dist = 0.0;
        let mut visited = vec![false; graph.node_bound()];
        let mut queue = VecDeque::new();

        visited[start_node.index()] = true;
        // Queue stores pairs of (NodeIndex, current_distance)
        queue.push_back((start_node, 0_u32));

        while let Some((current_node, dist)) = queue.pop_front() {
            if dist > 0 {
                // Harmonic reciprocal: 1 / distance
                sum_reciprocal_dist += 1.0 / (dist as f64);
            }

            for edge in graph.edges(current_node) {
                let next_node = edge.target();
                if !visited[next_node.index()] {
                    visited[next_node.index()] = true;
                    queue.push_back((next_node, dist + 1));
                }
            }
        }

        centrality.insert(start_node, sum_reciprocal_dist * norm_factor);
    }

    centrality
}

/// Computes the NORMALIZED Weighted Harmonic Centrality using Dijkstra's algorithm.
/// 
/// Ideal when your PDG/CFG edges have weights (e.g., call frequency, coupling strength,
/// or inverse blast radius probability).
pub fn weighted_harmonic_centrality<N, E, F, K>(
    graph: &Graph<N, E, Directed>,
    mut edge_cost: F,
) -> HashMap<NodeIndex, f64>
where
    F: FnMut(&petgraph::graph::EdgeReference<E>) -> K,
    K: petgraph::algo::Measure + Copy + Into<f64>,
{
    let mut centrality = HashMap::new();
    let num_nodes = graph.node_count();

    if num_nodes <= 1 {
        for node in graph.node_indices() {
            centrality.insert(node, 0.0);
        }
        return centrality;
    }

    let norm_factor = 1.0 / (num_nodes as f64 - 1.0);

    for start_node in graph.node_indices() {
        // Run Dijkstra from start_node to all reachable nodes
        let shortest_paths = dijkstra(graph, start_node, None, |e| edge_cost(&e));

        let mut sum_reciprocal_dist = 0.0;

        for (target_node, cost) in shortest_paths {
            if start_node == target_node {
                continue;
            }
            let dist_f64: f64 = cost.into();
            if dist_f64 > 0.0 {
                sum_reciprocal_dist += 1.0 / dist_f64;
            }
        }

        centrality.insert(start_node, sum_reciprocal_dist * norm_factor);
    }

    centrality
}

```

---

### 4. Critical Architectural Note: Edge Directionality in Software Graphs

In PetGraph, `graph.edges(node)` yields **outgoing edges**. When running the algorithm above, you are calculating **Out-Harmonic Centrality**.

What this means for your migration pipeline depends on how you construct your edges:

| Edge Convention ($u \to v$) | What Out-Harmonic Centrality Measures | Best Migration Use Case |
| --- | --- | --- |
| **Dependency Edges**<br>

<br>("Module $u$ depends on Module $v$") | **Downstream Coupling:** How closely this module is tied to underlying infrastructure and libraries. | **High Score = Foundation:** High-scoring nodes rely heavily on short paths to core services. They should be extracted *after* their dependencies are migrated. |
| **Data Flow / Call Edges**<br>

<br>("Function $u$ invokes/passes data to Function $v$") | **Execution Influence:** How quickly data or control changes in $u$ ripple downstream to the rest of the program. | **High Score = High Blast Radius Hub:** High-scoring nodes act as central dispatchers or routing hubs. They require strict API boundaries and shadow-testing during migration. |

#### How to calculate In-Harmonic Centrality (Upstream Reach)

If you want to measure how close *all other nodes* are to node $u$ (which identifies your central utility sinks like `AuditLogger` or core database wrappers), simply reverse the graph edges before running the algorithm using `petgraph::visit::Reversed`:

```rust
use petgraph::visit::Reversed;

// To compute In-Harmonic centrality without cloning/mutating the original graph:
// You can adapt the BFS loop to traverse incoming edges using `graph.edges_directed(node, petgraph::Direction::Incoming)`

```

---

### Next Steps for Your Rust Pipeline

Now that you have PageRank, Betweenness, Blast Radius, Slicing, and Harmonic Centrality all computed in Rust:

1. **Module Boundary Cutting:** You can pass your Harmonic Centrality scores into a **Spectral Clustering** or **Louvain** modularity optimization step to automatically group high-cohesion AST/PDG nodes into microservice boundaries.
2. **Weighted Blending:** Combine PageRank (Global Importance) and Harmonic Centrality (Local Cluster Density) into a unified refactoring priority score:

$$\text{Priority}(u) = \alpha \cdot \text{PageRank}(u) + \beta \cdot \text{Harmonic}(u) - \gamma \cdot \text{BlastRadius}(u)$$

