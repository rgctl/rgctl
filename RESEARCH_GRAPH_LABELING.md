# Graph Labeling Research & Implementation for rBuilder

**Research Date**: June 18, 2026  
**Context**: Migration scenarios, deprecated APIs, community detection with semantic labels  
**Goal**: Implement multi-dimensional labeling system for code knowledge graphs

---

## Research Summary: State of the Art (2024-2025)

### 1. Code Knowledge Graphs & Evolution

#### **Knowledge Graph Based Repository-Level Code Generation** (May 2025)
- **arXiv**: [2505.14394](https://arxiv.org/abs/2505.14394)
- **Key Contribution**: Knowledge graph representation of code repositories capturing structural and relational information
- **Benchmark**: Evolutionary Code Benchmark (EvoCodeBench) for repository-level code generation
- **Relevance**: Shows how graph-based representations outperform baseline approaches for tracking code evolution

#### **Code Graph Model (CGM)** (May 2025)
- **arXiv**: [2505.16901](https://arxiv.org/pdf/2505.16901)
- **Key Contribution**: Graph-based framework for software maintenance and evolution
- **Approach**: Repository-level code graphs with R4 chain (Rewriter, Retriever, Reranker, Reader)
- **Relevance**: Demonstrates how to structure code graphs for evolution tracking

#### **Bridging Code Property Graphs and Language Models** (March 2026)
- **arXiv**: [2603.24837](https://arxiv.org/html/2603.24837v1)
- **Tool**: codebadger MCP server integrating Joern's CPG with LLMs
- **Relevance**: Shows integration of semantic analysis with code graphs

#### **Related Systems**
- **RepoGraph** (ICLR 2025): Repository-level code graphs that boost agents by 32.8% on SWE-bench
- **KGCompass** (NAACL 2025): Links issues and code entities, achieving 58.3% on SWE-bench Lite
- **GraphCoder** (ASE 2024): Code Context Graphs for repository-level completion
- **CodexGraph** (NAACL 2025): Exposes code graphs to LLM agents via graph database interfaces

**Sources**: 
- [Knowledge Graph Based Repository-Level Code Generation](https://arxiv.org/abs/2505.14394)
- [Code Graph Model](https://arxiv.org/pdf/2505.16901)
- [Awesome Repository-Level Code Generation](https://github.com/YerbaPage/Awesome-Repo-Level-Code-Generation)

---

### 2. Label Propagation & Community Detection

#### **Graph Embedding Based Label Propagation** (November 2024)
- **Source**: [Scientific Reports](https://www.nature.com/articles/s41598-025-25905-5)
- **Algorithm**: Embedding-based Label Propagation (ELP)
- **Key Contribution**: Combines label propagation with node embedding to integrate local connectivity and global structure
- **Relevance**: Can propagate migration labels through dependency graphs

#### **Overlapping Community Detection Survey** (December 2024)
- **Source**: [Multimedia Tools and Applications](https://link.springer.com/article/10.1007/s11042-024-20485-4)
- **Focus**: Label propagation for overlapping community identification
- **Relevance**: Code modules can belong to multiple communities (e.g., both "deprecated" and "security-critical")

#### **Knowledge Graph Enhanced Community Detection** (2019, still widely cited)
- **Source**: [ACM WSDM 2019](https://dl.acm.org/doi/10.1145/3289600.3291031)
- **Key Finding**: ~20% improvement on F-measure and Jaccard when incorporating domain-specific hierarchical concept graphs
- **Relevance**: Shows how contextual labels improve community detection accuracy

#### **Comprehensive Review of Community Detection** (2024)
- **Source**: [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0925231224009408)
- **Scope**: Survey of deep learning approaches for community detection
- **Relevance**: Modern techniques for identifying related code components

**Sources**:
- [Graph Embedding Based Label Propagation](https://www.nature.com/articles/s41598-025-25905-5)
- [Community Detection Survey](https://link.springer.com/article/10.1007/s11042-024-20485-4)
- [Knowledge Graph Enhanced Community Detection](https://dl.acm.org/doi/10.1145/3289600.3291031)

---

### 3. Node Classification & Semantic Labeling

#### **Data-Efficient Graph Learning Survey** (IJCAI 2024)
- **Source**: [IJCAI 2024](https://www.ijcai.org/proceedings/2024/0896.pdf)
- **Categories**: Self-supervised, semi-supervised, and few-shot graph learning
- **Relevance**: Techniques for labeling with limited training data

#### **NoisyGL: Label Noise Benchmark** (NeurIPS 2024)
- **Source**: [GitHub](https://github.com/eaglelab-zju/NoisyGL)
- **Contribution**: Comprehensive benchmark for GNNs under label noise
- **Relevance**: Important for real-world scenarios where labels may be imperfect (e.g., automated deprecation detection)

#### **Combining GCN and Label Propagation** (2021, foundational)
- **Source**: [Stanford](https://www-cs.stanford.edu/~jure/pubs/gcnlpa-tois21.pdf)
- **Key Insight**: Both LPA and GCN are message passing algorithms
- **Approach**: LPA propagates label information, GCN propagates feature information
- **Relevance**: Can combine structural (GCN) and semantic (LPA) information

**Sources**:
- [Data-Efficient Graph Learning Survey](https://www.ijcai.org/proceedings/2024/0896.pdf)
- [NoisyGL Benchmark](https://github.com/eaglelab-zju/NoisyGL)
- [Combining GCN and Label Propagation](https://www-cs.stanford.edu/~jure/pubs/gcnlpa-tois21.pdf)

---

### 4. Code Migration & API Evolution

#### **Amazon MigrationBench** (2024)
- **Source**: [AWS DevOps Blog](https://aws.amazon.com/blogs/devops/amazon-introduces-two-benchmark-datasets-for-evaluating-ai-agents-ability-on-code-migration/)
- **Datasets**: MigrationBench and Poly-MigrationBench
- **Migration Types**:
  - Runtime upgrade
  - Deprecated API replacement
  - Test framework optimization
  - Syntax modernization
- **Relevance**: Standardized benchmark for evaluating migration scenarios

#### **Azure AD Graph Migration** (Retiring 2025)
- **Source**: [Microsoft Learn](https://learn.microsoft.com/en-us/graph/migrate-azure-ad-graph-overview)
- **Timeline**: Full retirement August 31, 2025
- **Migration Patterns**: Real-world example of large-scale API deprecation
- **Relevance**: Case study for tracking deprecated APIs in knowledge graphs

**Sources**:
- [Amazon MigrationBench](https://aws.amazon.com/blogs/devops/amazon-introduces-two-benchmark-datasets-for-evaluating-ai-agents-ability-on-code-migration/)
- [Azure AD Graph Migration](https://learn.microsoft.com/en-us/graph/migrate-azure-ad-graph-overview)

---

## Use Cases for rBuilder

### Use Case 1: Deprecated API Tracking
**Scenario**: Track deprecated APIs across a large codebase and identify migration paths

**Example**:
```python
# Old deprecated API
response = requests.get(url, verify=False)  # Deprecated: insecure SSL

# New recommended API
response = requests.get(url, verify=True, cert='/path/to/cert')
```

**Labels**:
- `deprecated:ssl-verify-false` (on the old pattern)
- `migration-target:ssl-verify-true` (on the new pattern)
- `security-risk:high` (on usages of deprecated pattern)
- `migration-effort:low` (automated fix available)

---

### Use Case 2: Framework Migration
**Scenario**: Migrate from JUnit 4 to JUnit 5

**Labels**:
- `junit4:test-annotation` → functions using `@Test` from JUnit 4
- `junit5:test-annotation` → functions using `@Test` from JUnit 5
- `migration-pattern:test-annotation` → link between old and new
- `migration-status:pending` / `in-progress` / `completed`

**Community Detection**: Identify clusters of test files that depend on each other and should be migrated together

---

### Use Case 3: Breaking Change Impact Analysis
**Scenario**: Library X version 2.0 removes `deprecated_function()`

**Labels**:
- `uses:library-x-v1` (all code using v1)
- `breaking-change:deprecated-function` (specific call sites)
- `blast-radius:high` (functions with many dependents)
- `migration-blocker` (code that can't be easily migrated)

**Community Analysis**: Find which teams/modules are most affected

---

### Use Case 4: Technical Debt Zones
**Scenario**: Identify areas of technical debt that should be addressed together

**Labels**:
- `tech-debt:high` (complex, poorly tested code)
- `security-debt:critical` (security issues)
- `performance-debt:medium` (slow paths)
- `test-debt:no-coverage` (untested code)

**Community Detection**: Find clusters of tech debt that share dependencies

---

## Implementation Options for rBuilder

### Option 1: Simple Label System (Quick Implementation)

**Schema Extension**:
```rust
// src/graph/schema.rs

pub struct Node {
    pub id: Uuid,
    pub node_type: NodeType,
    pub name: String,
    pub file_path: Option<String>,
    pub properties: HashMap<String, String>,
    pub signature: Option<String>,
    
    // NEW: Label system
    pub labels: HashSet<String>,  // Simple string labels
}

pub struct Edge {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    pub edge_type: EdgeType,
    
    // NEW: Edge labels
    pub labels: HashSet<String>,
}
```

**API**:
```rust
// Add labels to nodes
backend.add_label(node_id, "deprecated:api")?;
backend.add_label(node_id, "migration-target:new-api")?;
backend.add_label(node_id, "security-risk:high")?;

// Query by labels
let deprecated_nodes = backend.find_nodes_by_label("deprecated:api")?;
let high_security_risks = backend.find_nodes_by_label("security-risk:high")?;

// Combine label queries
let deprecated_and_risky = backend.find_nodes_by_labels(&["deprecated:api", "security-risk:high"])?;
```

**Pros**:
- Easy to implement (1-2 days)
- Flexible (any string can be a label)
- Query-friendly

**Cons**:
- No structure or validation
- No label hierarchy
- No label metadata

---

### Option 2: Structured Label System (Recommended)

**Schema**:
```rust
// src/graph/labels.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Label namespace (e.g., "deprecated", "migration", "security")
    pub namespace: String,
    
    /// Label key (e.g., "api", "target", "risk")
    pub key: String,
    
    /// Label value (e.g., "ssl-verify-false", "new-api", "high")
    pub value: String,
    
    /// When the label was applied
    pub timestamp: chrono::DateTime<Utc>,
    
    /// Who/what applied the label (e.g., "security-scanner", "manual", "migration-tool")
    pub source: String,
    
    /// Optional confidence score (0.0-1.0)
    pub confidence: Option<f64>,
    
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

impl Label {
    /// Format as "namespace:key:value"
    pub fn to_string(&self) -> String {
        format!("{}:{}:{}", self.namespace, self.key, self.value)
    }
    
    /// Parse from string "namespace:key:value"
    pub fn from_str(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(Error::InvalidLabel(s.to_string()));
        }
        Ok(Label {
            namespace: parts[0].to_string(),
            key: parts[1].to_string(),
            value: parts[2].to_string(),
            timestamp: Utc::now(),
            source: "unknown".to_string(),
            confidence: None,
            metadata: HashMap::new(),
        })
    }
}

pub struct Node {
    // ... existing fields
    
    /// Multi-dimensional labels
    pub labels: Vec<Label>,
}
```

**API**:
```rust
// Add structured labels
let label = Label {
    namespace: "deprecated".to_string(),
    key: "api".to_string(),
    value: "ssl-verify-false".to_string(),
    timestamp: Utc::now(),
    source: "security-scanner".to_string(),
    confidence: Some(0.95),
    metadata: [
        ("reason".to_string(), "Insecure SSL verification".to_string()),
        ("cwe".to_string(), "CWE-295".to_string()),
    ].into_iter().collect(),
};

backend.add_label(node_id, label)?;

// Query by namespace
let deprecated = backend.find_nodes_by_label_namespace("deprecated")?;

// Query by namespace + key
let deprecated_apis = backend.find_nodes_by_label_nk("deprecated", "api")?;

// Query by full label
let ssl_issues = backend.find_nodes_by_label_nkv("deprecated", "api", "ssl-verify-false")?;

// Get all labels for a node
let labels = backend.get_node_labels(node_id)?;
```

**Pros**:
- Structured and validatable
- Supports metadata and confidence scores
- Audit trail (timestamp, source)
- Can build label hierarchies

**Cons**:
- More complex implementation (1 week)
- Requires schema migration

---

### Option 3: Label Propagation System (Advanced)

Based on research papers (ELP, GCN+LPA), implement label propagation through the graph.

**Schema**:
```rust
// src/analysis/label_propagation.rs

pub struct LabelPropagator {
    backend: Arc<MemoryBackend>,
    max_iterations: usize,
    convergence_threshold: f64,
}

impl LabelPropagator {
    /// Propagate labels through dependency edges
    pub fn propagate_labels(
        &self,
        seed_labels: HashMap<Uuid, Vec<Label>>,
        edge_types: &[EdgeType],
    ) -> Result<HashMap<Uuid, Vec<Label>>> {
        let mut node_labels = seed_labels.clone();
        
        for iteration in 0..self.max_iterations {
            let mut updated = false;
            
            for edge in self.backend.all_edges()? {
                if !edge_types.contains(&edge.edge_type) {
                    continue;
                }
                
                // Propagate labels from source to target
                if let Some(from_labels) = node_labels.get(&edge.from) {
                    let propagated = self.propagate_along_edge(
                        from_labels,
                        &edge,
                        iteration,
                    );
                    
                    let to_labels = node_labels.entry(edge.to).or_insert(vec![]);
                    for label in propagated {
                        if !to_labels.contains(&label) {
                            to_labels.push(label);
                            updated = true;
                        }
                    }
                }
            }
            
            if !updated {
                break;  // Converged
            }
        }
        
        Ok(node_labels)
    }
    
    fn propagate_along_edge(
        &self,
        from_labels: &[Label],
        edge: &Edge,
        iteration: usize,
    ) -> Vec<Label> {
        from_labels.iter()
            .filter(|label| self.should_propagate(label, edge))
            .map(|label| {
                let mut propagated = label.clone();
                // Decay confidence with distance
                if let Some(conf) = propagated.confidence {
                    propagated.confidence = Some(conf * 0.9);
                }
                propagated.metadata.insert(
                    "propagated_from".to_string(),
                    edge.from.to_string(),
                );
                propagated.metadata.insert(
                    "propagation_depth".to_string(),
                    iteration.to_string(),
                );
                propagated
            })
            .collect()
    }
    
    fn should_propagate(&self, label: &Label, edge: &Edge) -> bool {
        // Custom rules for which labels propagate along which edges
        match (label.namespace.as_str(), edge.edge_type) {
            ("deprecated", EdgeType::Calls) => true,  // Deprecated APIs propagate to callers
            ("security-risk", EdgeType::Calls) => true,  // Security risks propagate
            ("migration-target", _) => false,  // Migration targets don't propagate
            _ => false,
        }
    }
}
```

**Usage**:
```rust
// 1. Seed deprecated APIs
let mut seed_labels = HashMap::new();
for node in backend.find_nodes_by_name("old_deprecated_api")? {
    seed_labels.insert(node.id, vec![
        Label::new("deprecated", "api", "v1")
            .with_confidence(1.0)
            .with_source("manual"),
    ]);
}

// 2. Propagate through call graph
let propagator = LabelPropagator::new(backend.clone());
let propagated = propagator.propagate_labels(
    seed_labels,
    &[EdgeType::Calls, EdgeType::Imports],
)?;

// 3. Find all affected code
for (node_id, labels) in propagated {
    if labels.iter().any(|l| l.namespace == "deprecated") {
        println!("Node {} is affected by deprecated API", node_id);
    }
}
```

**Pros**:
- Automatic impact analysis
- Finds indirect dependencies
- Confidence scores show propagation distance
- Based on proven research (ELP, GCN+LPA)

**Cons**:
- Complex implementation (2-3 weeks)
- Performance considerations for large graphs
- Requires tuning (iterations, thresholds)

---

### Option 4: Community Detection with Labels

Based on knowledge graph enhanced community detection research.

**Schema**:
```rust
// src/analysis/labeled_communities.rs

pub struct LabeledCommunity {
    /// Community ID
    pub id: Uuid,
    
    /// Nodes in this community
    pub members: HashSet<Uuid>,
    
    /// Dominant labels in the community
    pub labels: HashMap<String, f64>,  // label -> frequency
    
    /// Community metrics
    pub density: f64,
    pub modularity: f64,
}

pub struct LabeledCommunityDetector {
    backend: Arc<MemoryBackend>,
}

impl LabeledCommunityDetector {
    /// Detect communities considering both graph structure and labels
    pub fn detect_communities(
        &self,
        label_weight: f64,  // 0.0 = structure only, 1.0 = labels only
    ) -> Result<Vec<LabeledCommunity>> {
        // 1. Build similarity matrix based on structure + labels
        let similarity = self.compute_similarity(label_weight)?;
        
        // 2. Run community detection (Louvain, Leiden, or label propagation)
        let communities = self.run_louvain(&similarity)?;
        
        // 3. Characterize communities by their labels
        let labeled = self.characterize_communities(communities)?;
        
        Ok(labeled)
    }
    
    fn compute_similarity(&self, label_weight: f64) -> Result<SimilarityMatrix> {
        let nodes = self.backend.all_nodes()?;
        let mut similarity = SimilarityMatrix::new(nodes.len());
        
        for (i, node_i) in nodes.iter().enumerate() {
            for (j, node_j) in nodes.iter().enumerate().skip(i + 1) {
                // Structural similarity (connected nodes)
                let struct_sim = self.structural_similarity(node_i, node_j)?;
                
                // Label similarity (shared labels)
                let label_sim = self.label_similarity(node_i, node_j);
                
                // Combined similarity
                let combined = (1.0 - label_weight) * struct_sim + label_weight * label_sim;
                similarity.set(i, j, combined);
            }
        }
        
        Ok(similarity)
    }
    
    fn label_similarity(&self, node_a: &Node, node_b: &Node) -> f64 {
        let labels_a: HashSet<_> = node_a.labels.iter().map(|l| l.to_string()).collect();
        let labels_b: HashSet<_> = node_b.labels.iter().map(|l| l.to_string()).collect();
        
        let intersection = labels_a.intersection(&labels_b).count() as f64;
        let union = labels_a.union(&labels_b).count() as f64;
        
        if union == 0.0 {
            0.0
        } else {
            intersection / union  // Jaccard similarity
        }
    }
}
```

**Usage**:
```rust
// Find communities of deprecated code
let detector = LabeledCommunityDetector::new(backend.clone());

// Weight labels heavily (0.7) to group by deprecation status
let communities = detector.detect_communities(0.7)?;

for community in communities {
    if community.labels.get("deprecated:api").unwrap_or(&0.0) > &0.5 {
        println!("Found deprecated API community with {} members", community.members.len());
        println!("Dominant labels: {:?}", community.labels);
        
        // Analyze migration effort for this community
        let effort = estimate_migration_effort(&community)?;
        println!("Migration effort: {:?}", effort);
    }
}
```

**Pros**:
- Groups related code for coordinated migration
- Identifies migration "zones"
- Based on research showing 20% improvement over structure-only
- Useful for team planning

**Cons**:
- Very complex implementation (3-4 weeks)
- Requires community detection algorithm (Louvain/Leiden)
- Performance intensive

---

## Recommended Implementation Roadmap

### Phase 1: Simple Labels (Week 1-2)
**Goal**: Basic labeling infrastructure

1. Extend Node/Edge schema with `labels: HashSet<String>`
2. Add indexing for label queries
3. Implement basic CRUD operations:
   - `add_label(node_id, label)`
   - `remove_label(node_id, label)`
   - `find_nodes_by_label(label)`
4. Add CLI commands:
   - `rbuilder label add <node> <label>`
   - `rbuilder label query <label>`
5. Test with deprecated API use case

**Deliverables**:
- Updated schema
- Label API
- CLI integration
- 10+ tests

---

### Phase 2: Structured Labels (Week 3-4)
**Goal**: Rich label metadata

1. Create `Label` struct with namespace:key:value
2. Add timestamp, source, confidence
3. Implement label hierarchy/taxonomy
4. Add label metadata storage
5. Create label validation rules
6. Build label export (JSON, CSV)

**Deliverables**:
- Label struct and validation
- Metadata support
- Export functionality
- 15+ tests

---

### Phase 3: Automated Labeling (Week 5-6)
**Goal**: Detect and apply labels automatically

1. Create `LabelDetector` trait
2. Implement detectors:
   - `DeprecatedAPIDetector` (pattern matching)
   - `SecurityRiskDetector` (from security scanners)
   - `ComplexityDetector` (from complexity metrics)
3. Add MCP tools:
   - `detect_deprecated_apis`
   - `label_security_risks`
4. Integrate with existing pipelines

**Deliverables**:
- 3+ detectors
- MCP integration
- Pipeline integration
- 20+ tests

---

### Phase 4: Label Propagation (Week 7-9)
**Goal**: Automatic impact analysis

1. Implement `LabelPropagator`
2. Add propagation rules engine
3. Add confidence decay with distance
4. Create visualization:
   - Propagation depth heat map
   - Affected components diagram
5. Add CLI commands:
   - `rbuilder label propagate <seed-label>`
   - `rbuilder label impact <label>`

**Deliverables**:
- Label propagation algorithm
- Visualization support
- Impact analysis CLI
- 25+ tests

---

### Phase 5: Community Detection (Week 10-12)
**Goal**: Group related code by labels

1. Implement `LabeledCommunityDetector`
2. Integrate Louvain or Leiden algorithm
3. Add label-weighted similarity
4. Create community reports:
   - Migration zones
   - Tech debt clusters
   - Ownership boundaries
5. Add visualization:
   - Community graph
   - Label distribution per community

**Deliverables**:
- Community detection
- Label-aware communities
- Reports and visualization
- 30+ tests

---

## Example: End-to-End Migration Scenario

### Step 1: Detect Deprecated APIs
```bash
# Use security scanner to find deprecated patterns
rbuilder security-scan . --pattern deprecated --output labels.json

# Manually label known deprecated APIs
rbuilder label add "function:old_ssl_verify" "deprecated:api:ssl-verify-false"
rbuilder label add "function:old_ssl_verify" "migration-target:new_ssl_verify"
rbuilder label add "function:old_ssl_verify" "security-risk:high"
rbuilder label add "function:old_ssl_verify" "migration-effort:low"
```

### Step 2: Propagate Impact
```bash
# Propagate "deprecated:api" through call graph
rbuilder label propagate "deprecated:api:ssl-verify-false" --max-depth 5

# Find all affected code
rbuilder label query "deprecated:api:ssl-verify-false" --include-propagated
```

Output:
```
Found 47 nodes with label "deprecated:api:ssl-verify-false"
  - 1 direct (source)
  - 12 depth-1 (direct callers)
  - 23 depth-2 (indirect callers)
  - 11 depth-3 (transitive dependencies)

Affected teams:
  - auth-team: 15 nodes
  - api-team: 18 nodes
  - mobile-team: 14 nodes
```

### Step 3: Detect Migration Communities
```bash
# Group code by migration labels
rbuilder community detect --label-weight 0.7 --filter "deprecated:api"
```

Output:
```
Found 3 migration communities:

Community 1: "auth-service" (15 nodes)
  - Dominant label: deprecated:api:ssl-verify-false (100%)
  - Migration effort: LOW
  - Suggested order: 1st (no external dependencies)
  
Community 2: "api-gateway" (18 nodes)
  - Dominant labels:
    - deprecated:api:ssl-verify-false (100%)
    - security-risk:high (83%)
  - Migration effort: MEDIUM
  - Suggested order: 2nd (depends on auth-service)
  
Community 3: "mobile-backend" (14 nodes)
  - Dominant label: deprecated:api:ssl-verify-false (100%)
  - Migration effort: HIGH (requires client updates)
  - Suggested order: 3rd (depends on api-gateway)
```

### Step 4: Generate Migration Plan
```bash
# Export migration plan
rbuilder label export --format migration-plan --output migration.md
```

Output (`migration.md`):
```markdown
# SSL Verify Migration Plan

## Overview
- Total affected: 47 code locations
- Migration communities: 3
- Estimated effort: 2-3 weeks

## Phase 1: auth-service (Week 1)
- **Team**: auth-team
- **Effort**: Low (1-2 days)
- **Files**: 15
- **Risk**: Low (no external dependencies)

### Changes Required
1. Update `auth/ssl_utils.py:verify_connection()`
2. Update 12 call sites in auth service
3. Update tests

## Phase 2: api-gateway (Week 2)
- **Team**: api-team
- **Effort**: Medium (3-4 days)
- **Files**: 18
- **Risk**: Medium (depends on Phase 1)
- **Security**: HIGH PRIORITY (83% security-risk:high)

### Changes Required
1. Update gateway SSL configuration
2. Update 15 API endpoints
3. Add new SSL cert validation
4. Update integration tests

## Phase 3: mobile-backend (Week 3)
- **Team**: mobile-team
- **Effort**: High (5-7 days)
- **Files**: 14
- **Risk**: High (requires client updates)

### Changes Required
1. Update backend SSL handling
2. Coordinate with mobile app release
3. Update API documentation
4. End-to-end testing
```

---

## Integration with Existing rBuilder Features

### 1. Security Scanners
```rust
// src/security/ansible.rs (updated)

impl AnsibleSecurityScanner {
    pub fn scan_node(&self, node: &Node) -> Vec<AnsibleSecurityFinding> {
        let findings = /* ... existing logic ... */;
        
        // NEW: Also apply labels
        let mut labels = Vec::new();
        for finding in &findings {
            let label = Label {
                namespace: "security-risk".to_string(),
                key: "severity".to_string(),
                value: format!("{:?}", finding.severity).to_lowercase(),
                timestamp: Utc::now(),
                source: "ansible-security-scanner".to_string(),
                confidence: Some(1.0),
                metadata: [
                    ("cwe".to_string(), finding.cwe.clone().unwrap_or_default()),
                    ("message".to_string(), finding.message.clone()),
                ].into_iter().collect(),
            };
            labels.push(label);
        }
        
        findings
    }
}
```

### 2. MCP Tools
```rust
// src/mcp/tools.rs (updated)

/// Find all deprecated APIs in the graph
pub fn find_deprecated_apis(backend: &MemoryBackend) -> Result<Vec<DeprecatedAPI>> {
    let deprecated = backend.find_nodes_by_label_namespace("deprecated")?;
    
    deprecated.iter()
        .filter_map(|node| {
            let api_label = node.labels.iter()
                .find(|l| l.namespace == "deprecated" && l.key == "api")?;
            
            let migration_target = node.labels.iter()
                .find(|l| l.namespace == "migration-target")
                .map(|l| l.value.clone());
            
            Some(DeprecatedAPI {
                name: node.name.clone(),
                deprecated_version: api_label.value.clone(),
                migration_target,
                affected_count: count_callers(backend, node.id).ok()?,
            })
        })
        .collect()
}
```

### 3. CLI Commands
```bash
# New CLI commands
rbuilder label add <node> <namespace>:<key>:<value> [--confidence 0.9] [--source manual]
rbuilder label remove <node> <label>
rbuilder label query <label-pattern> [--format json|text|mermaid]
rbuilder label propagate <seed-label> [--max-depth 5] [--edge-types Calls,Imports]
rbuilder label export <label-pattern> --format migration-plan|csv|json
rbuilder community detect [--label-weight 0.7] [--filter <label>]
rbuilder migration plan <deprecated-label> [--output migration.md]
```

---

## Performance Considerations

### Indexing Strategy
```rust
// src/graph/backend/memory.rs (updated)

pub struct MemoryBackend {
    // Existing indices
    nodes: RwLock<HashMap<Uuid, Node>>,
    edges: RwLock<HashMap<Uuid, Edge>>,
    node_name_index: RwLock<HashMap<String, HashSet<Uuid>>>,
    node_type_index: RwLock<HashMap<NodeType, HashSet<Uuid>>>,
    
    // NEW: Label indices
    label_namespace_index: RwLock<HashMap<String, HashSet<Uuid>>>,  // namespace -> node IDs
    label_nk_index: RwLock<HashMap<(String, String), HashSet<Uuid>>>,  // (namespace, key) -> IDs
    label_nkv_index: RwLock<HashMap<(String, String, String), HashSet<Uuid>>>,  // full label -> IDs
}
```

**Query Performance**:
- `find_by_label_namespace`: O(1) lookup + O(n) result copy
- `find_by_label_nk`: O(1) lookup
- `find_by_label_nkv`: O(1) lookup
- Label propagation: O(V + E) per iteration (graph traversal)

---

## Storage Format

### JSON Export
```json
{
  "nodes": [
    {
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "type": "Function",
      "name": "old_ssl_verify",
      "labels": [
        {
          "namespace": "deprecated",
          "key": "api",
          "value": "ssl-verify-false",
          "timestamp": "2026-06-18T10:30:00Z",
          "source": "security-scanner",
          "confidence": 0.95,
          "metadata": {
            "cwe": "CWE-295",
            "reason": "Insecure SSL verification",
            "migration_guide": "https://docs/ssl-migration"
          }
        },
        {
          "namespace": "security-risk",
          "key": "severity",
          "value": "high",
          "timestamp": "2026-06-18T10:30:00Z",
          "source": "security-scanner",
          "confidence": 1.0,
          "metadata": {}
        }
      ]
    }
  ]
}
```

---

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_label_propagation_with_confidence_decay() {
    let mut backend = MemoryBackend::new();
    
    // Create graph: A -> B -> C
    let node_a = Node::new(NodeType::Function, "A".into());
    let node_b = Node::new(NodeType::Function, "B".into());
    let node_c = Node::new(NodeType::Function, "C".into());
    
    backend.add_node(node_a.clone())?;
    backend.add_node(node_b.clone())?;
    backend.add_node(node_c.clone())?;
    
    backend.add_edge(Edge::new(node_a.id, node_b.id, EdgeType::Calls))?;
    backend.add_edge(Edge::new(node_b.id, node_c.id, EdgeType::Calls))?;
    
    // Seed label on A with confidence 1.0
    let seed_label = Label::new("deprecated", "api", "v1")
        .with_confidence(1.0);
    
    backend.add_label(node_a.id, seed_label)?;
    
    // Propagate
    let propagator = LabelPropagator::new(backend.clone());
    let propagated = propagator.propagate_labels(
        [(node_a.id, vec![seed_label])].into_iter().collect(),
        &[EdgeType::Calls],
    )?;
    
    // Verify confidence decay
    assert_eq!(propagated.get(&node_b.id).unwrap()[0].confidence, Some(0.9));
    assert_eq!(propagated.get(&node_c.id).unwrap()[0].confidence, Some(0.81));
}
```

---

## Next Steps

1. **Prototype Phase 1** (Simple Labels) - 1 week
   - Validate approach with real deprecated API data
   - Measure query performance
   - Get user feedback

2. **Implement Phase 2** (Structured Labels) - 2 weeks
   - Full label metadata support
   - Export functionality
   - CLI integration

3. **Research Integration** - Ongoing
   - Monitor new papers on graph labeling
   - Evaluate GNN approaches for label propagation
   - Consider GraphRAG integration for LLM queries

4. **Production Hardening** - 1 week
   - Performance optimization
   - Large-scale testing (10k+ nodes)
   - Documentation and examples

---

## References

### Code Knowledge Graphs
- [Knowledge Graph Based Repository-Level Code Generation](https://arxiv.org/abs/2505.14394)
- [Code Graph Model (CGM)](https://arxiv.org/pdf/2505.16901)
- [Bridging Code Property Graphs and Language Models](https://arxiv.org/html/2603.24837v1)
- [Awesome Repo-Level Code Generation](https://github.com/YerbaPage/Awesome-Repo-Level-Code-Generation)

### Label Propagation & Community Detection
- [Graph Embedding Based Label Propagation](https://www.nature.com/articles/s41598-025-25905-5)
- [Overlapping Community Detection Survey](https://link.springer.com/article/10.1007/s11042-024-20485-4)
- [Knowledge Graph Enhanced Community Detection](https://dl.acm.org/doi/10.1145/3289600.3291031)
- [Comprehensive Review of Community Detection](https://www.sciencedirect.com/science/article/abs/pii/S0925231224009408)

### Node Classification
- [Data-Efficient Graph Learning Survey](https://www.ijcai.org/proceedings/2024/0896.pdf)
- [NoisyGL Benchmark](https://github.com/eaglelab-zju/NoisyGL)
- [Combining GCN and Label Propagation](https://www-cs.stanford.edu/~jure/pubs/gcnlpa-tois21.pdf)

### Migration & Deprecation
- [Amazon MigrationBench](https://aws.amazon.com/blogs/devops/amazon-introduces-two-benchmark-datasets-for-evaluating-ai-agents-ability-on-code-migration/)
- [Azure AD Graph Migration](https://learn.microsoft.com/en-us/graph/migrate-azure-ad-graph-overview)

---

**Research Compiled**: June 18, 2026  
**Next Update**: Review new papers quarterly  
**Implementation Target**: Phase 1 by end of Q3 2026
