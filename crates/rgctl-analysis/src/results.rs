//! Columnar storage for analysis results.
//!
//! This module provides high-performance, cache-efficient storage for analysis
//! results that is completely decoupled from the graph topology. Analysis results
//! are stored in separate Vec-based tables indexed by compact u32 node IDs.
//!
//! ## Architecture
//!
//! - **Immutable Graph**: Graph structure stays read-only during all analyses
//! - **Columnar Tables**: Each metric stored in contiguous Vec<T> arrays
//! - **Compact IDs**: Internal u32 IDs for dense array indexing (not UUIDs)
//! - **Zero Lock Contention**: No graph mutation = perfect parallelism
//!
//! ## Performance
//!
//! - **Memory**: Contiguous Vec storage = 100% CPU cache line efficiency
//! - **Lookups**: O(1) array access vs HashMap + RwLock
//! - **Parallelism**: Multiple analyses can run concurrently on immutable graph
//! - **Serialization**: Simple binary format, no need to reconstruct graph

use rgctl_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

use crate::community::CommunityResult;
use crate::complexity::ComplexityReport;
use crate::graph_utils::PetGraphView;

/// Compact node ID for dense array indexing.
/// Internal representation - not exposed outside this module.
type CompactId = u32;

/// Community detection results stored in columnar format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityTable {
    /// Community ID for each node (indexed by CompactId)
    pub assignments: Vec<usize>,
    /// Modularity score for the entire graph
    pub modularity: f64,
    /// Number of distinct communities
    pub num_communities: usize,
    /// Human-readable labels keyed by community id (heuristic / override).
    #[serde(default)]
    pub labels: HashMap<usize, String>,
    /// Community id for stripped infrastructure hubs, if any.
    #[serde(default)]
    pub infrastructure_community_id: Option<usize>,
}

impl CommunityTable {
    /// Create a new empty table with capacity for `node_count` nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            assignments: vec![0; node_count],
            modularity: 0.0,
            num_communities: 0,
            labels: HashMap::new(),
            infrastructure_community_id: None,
        }
    }

    /// Get community ID for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<usize> {
        self.assignments.get(id as usize).copied()
    }

    /// Label for a community id, if known.
    pub fn label(&self, community_id: usize) -> Option<&str> {
        self.labels.get(&community_id).map(String::as_str)
    }
}

/// Complexity metrics stored in columnar format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityTable {
    /// Cyclomatic complexity (indexed by CompactId)
    pub cyclomatic: Vec<u32>,
    /// Cognitive complexity (indexed by CompactId)
    pub cognitive: Vec<u32>,
    /// Average cyclomatic complexity
    pub avg_cyclomatic: f64,
    /// Maximum cyclomatic complexity
    pub max_cyclomatic: u32,
}

impl ComplexityTable {
    /// Create a new empty table with capacity for `node_count` nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            cyclomatic: vec![0; node_count],
            cognitive: vec![0; node_count],
            avg_cyclomatic: 0.0,
            max_cyclomatic: 0,
        }
    }

    /// Get complexity metrics for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<(u32, u32)> {
        let cyc = self.cyclomatic.get(id as usize)?;
        let cog = self.cognitive.get(id as usize)?;
        Some((*cyc, *cog))
    }
}

/// Centrality metrics stored in columnar format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentralityTable {
    /// PageRank scores (indexed by CompactId)
    pub pagerank: Vec<f32>,
    /// Betweenness centrality (indexed by CompactId)
    pub betweenness: Vec<f32>,
    /// Harmonic centrality (indexed by CompactId)
    pub harmonic: Vec<f32>,
    /// In-degree (indexed by CompactId)
    pub in_degree: Vec<u32>,
    /// Out-degree (indexed by CompactId)
    pub out_degree: Vec<u32>,
}

impl CentralityTable {
    /// Create a new empty table with capacity for `node_count` nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            pagerank: vec![0.0; node_count],
            betweenness: vec![0.0; node_count],
            harmonic: vec![0.0; node_count],
            in_degree: vec![0; node_count],
            out_degree: vec![0; node_count],
        }
    }

    /// Get centrality metrics for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<CentralityMetrics> {
        Some(CentralityMetrics {
            pagerank: *self.pagerank.get(id as usize)?,
            betweenness: *self.betweenness.get(id as usize)?,
            harmonic: *self.harmonic.get(id as usize)?,
            in_degree: *self.in_degree.get(id as usize)?,
            out_degree: *self.out_degree.get(id as usize)?,
        })
    }
}

/// Centrality metrics for a single node.
#[derive(Debug, Clone, Copy)]
pub struct CentralityMetrics {
    /// PageRank centrality score
    pub pagerank: f32,
    /// Betweenness centrality score
    pub betweenness: f32,
    /// Harmonic centrality score (normalized 0–1)
    pub harmonic: f32,
    /// Number of incoming edges
    pub in_degree: u32,
    /// Number of outgoing edges
    pub out_degree: u32,
}

/// Blast radius metrics stored in columnar format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastRadiusTable {
    /// Impact score (indexed by CompactId)
    pub scores: Vec<f32>,
    /// Number of direct callers (indexed by CompactId)
    pub direct_callers: Vec<u32>,
    /// Size of impact zone (indexed by CompactId)
    pub impact_zone_size: Vec<u32>,
    /// SCC ID (indexed by CompactId)
    pub scc_id: Vec<u32>,
    /// SCC size (indexed by CompactId)
    pub scc_size: Vec<u32>,
}

impl BlastRadiusTable {
    /// Create a new empty table with capacity for `node_count` nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            scores: vec![0.0; node_count],
            direct_callers: vec![0; node_count],
            impact_zone_size: vec![0; node_count],
            scc_id: vec![0; node_count],
            scc_size: vec![0; node_count],
        }
    }

    /// Get blast radius metrics for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<BlastRadiusMetrics> {
        Some(BlastRadiusMetrics {
            score: *self.scores.get(id as usize)?,
            direct_callers: *self.direct_callers.get(id as usize)?,
            impact_zone_size: *self.impact_zone_size.get(id as usize)?,
            scc_id: *self.scc_id.get(id as usize)?,
            scc_size: *self.scc_size.get(id as usize)?,
        })
    }
}

/// Blast radius metrics for a single node.
#[derive(Debug, Clone, Copy)]
pub struct BlastRadiusMetrics {
    /// Impact score (0-100 scale)
    pub score: f32,
    /// Number of functions that directly call this node
    pub direct_callers: u32,
    /// Total size of the impact zone (transitive callers)
    pub impact_zone_size: u32,
    /// ID of the strongly connected component this node belongs to
    pub scc_id: u32,
    /// Size of the strongly connected component this node belongs to
    pub scc_size: u32,
}

/// Columnar 256-bit token bloom sketches (eager structural index at discover time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralSketchTable {
    /// Four u64 words per compact node id (`256` bits).
    pub token_blooms: Vec<u64>,
}

impl StructuralSketchTable {
    /// Create an empty table sized for `node_count` graph nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            token_blooms: vec![0; node_count * rgctl_graph::TOKEN_BLOOM_WORDS],
        }
    }

    /// Write one bloom sketch for `compact_id`.
    pub fn set_bloom(&mut self, compact_id: CompactId, bloom: rgctl_graph::TokenBloom) {
        let offset = compact_id as usize * rgctl_graph::TOKEN_BLOOM_WORDS;
        if offset + rgctl_graph::TOKEN_BLOOM_WORDS <= self.token_blooms.len() {
            self.token_blooms[offset..offset + rgctl_graph::TOKEN_BLOOM_WORDS]
                .copy_from_slice(&bloom);
        }
    }

    /// Read the bloom sketch for `compact_id`.
    pub fn bloom(&self, compact_id: CompactId) -> Option<rgctl_graph::TokenBloom> {
        let offset = compact_id as usize * rgctl_graph::TOKEN_BLOOM_WORDS;
        let words = self
            .token_blooms
            .get(offset..offset + rgctl_graph::TOKEN_BLOOM_WORDS)?;
        Some([words[0], words[1], words[2], words[3]])
    }

    /// True when every keyword matches via per-keyword bloom probes.
    pub fn satisfies_keyword_and(&self, compact_id: CompactId, keywords: &[String]) -> bool {
        self.bloom(compact_id)
            .is_some_and(|bloom| rgctl_graph::satisfies_keyword_and(keywords, &bloom))
    }

    /// Fraction of keywords matched in the stored bloom sketch.
    pub fn keyword_overlap(&self, compact_id: CompactId, keywords: &[String]) -> f64 {
        self.bloom(compact_id)
            .map(|bloom| rgctl_graph::keyword_overlap_score(keywords, &bloom))
            .unwrap_or(0.0)
    }
}

/// Complete analysis results for a repository.
///
/// This structure holds all analysis results in columnar format, completely
/// decoupled from the graph topology. It uses compact u32 IDs for indexing
/// to achieve dense array packing and cache efficiency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResults {
    /// Mapping from UUID to compact ID
    uuid_to_compact: HashMap<Uuid, CompactId>,
    /// Reverse mapping from compact ID to UUID
    compact_to_uuid: Vec<Uuid>,
    /// Community detection results
    pub community: Option<CommunityTable>,
    /// Complexity analysis results
    pub complexity: Option<ComplexityTable>,
    /// Centrality analysis results
    pub centrality: Option<CentralityTable>,
    /// Blast radius analysis results
    pub blast_radius: Option<BlastRadiusTable>,
    /// Eager token bloom sketches copied from graph nodes at discover time
    #[serde(default)]
    pub structural_sketch: Option<StructuralSketchTable>,
}

impl AnalysisResults {
    /// Create a new results structure from a list of node UUIDs.
    ///
    /// This builds the compact ID mapping for efficient array indexing.
    pub fn new(node_ids: Vec<Uuid>) -> Self {
        let node_count = node_ids.len();
        let mut uuid_to_compact = HashMap::with_capacity(node_count);
        let mut compact_to_uuid = Vec::with_capacity(node_count);

        for (compact_id, uuid) in node_ids.iter().enumerate() {
            uuid_to_compact.insert(*uuid, compact_id as CompactId);
            compact_to_uuid.push(*uuid);
        }

        Self {
            uuid_to_compact,
            compact_to_uuid,
            community: None,
            complexity: None,
            centrality: None,
            blast_radius: None,
            structural_sketch: None,
        }
    }

    /// Get compact ID for a UUID.
    pub fn get_compact_id(&self, uuid: Uuid) -> Option<CompactId> {
        self.uuid_to_compact.get(&uuid).copied()
    }

    /// Get UUID for a compact ID.
    pub fn get_uuid(&self, compact_id: CompactId) -> Option<Uuid> {
        self.compact_to_uuid.get(compact_id as usize).copied()
    }

    /// Number of nodes in the analysis.
    pub fn node_count(&self) -> usize {
        self.compact_to_uuid.len()
    }

    /// Initialize community table.
    pub fn init_community(&mut self) -> &mut CommunityTable {
        self.community = Some(CommunityTable::with_capacity(self.node_count()));
        self.community.as_mut().unwrap()
    }

    /// Write community assignments directly into the columnar table (no staging buffer).
    pub fn fill_community(&mut self, result: &CommunityResult) {
        let node_count = self.node_count();
        if self.community.is_none() {
            self.community = Some(CommunityTable::with_capacity(node_count));
        }
        let uuid_to_compact = &self.uuid_to_compact;
        let table = self.community.as_mut().unwrap();
        table.modularity = result.modularity;
        table.infrastructure_community_id = result.infrastructure_community_id;
        let mut max_cid = 0usize;
        for (node_id, community_id) in &result.assignments {
            if let Some(&compact_id) = uuid_to_compact.get(node_id) {
                table.assignments[compact_id as usize] = *community_id;
                max_cid = max_cid.max(*community_id);
            }
        }
        // Derive total community count from dense assignment ids when `result.communities`
        // omits singleton isolates on large graphs (>200k nodes).
        let dense_count = if result.assignments.is_empty() {
            0
        } else {
            max_cid + 1
        };
        table.num_communities = result.communities.len().max(dense_count);
    }

    /// Initialize complexity table.
    pub fn init_complexity(&mut self) -> &mut ComplexityTable {
        self.complexity = Some(ComplexityTable::with_capacity(self.node_count()));
        self.complexity.as_mut().unwrap()
    }

    /// Write complexity metrics directly into the columnar table (no staging buffer).
    pub fn fill_complexity(&mut self, report: &ComplexityReport) {
        let node_count = self.node_count();
        if self.complexity.is_none() {
            self.complexity = Some(ComplexityTable::with_capacity(node_count));
        }
        let uuid_to_compact = &self.uuid_to_compact;
        let table = self.complexity.as_mut().unwrap();
        table.avg_cyclomatic = report.avg_cyclomatic;
        table.max_cyclomatic = report.max_cyclomatic as u32;
        for func in &report.functions {
            if let Some(&compact_id) = uuid_to_compact.get(&func.node.id) {
                let idx = compact_id as usize;
                table.cyclomatic[idx] = func.cyclomatic as u32;
                table.cognitive[idx] = func.cognitive as u32;
            }
        }
    }

    /// Fill complexity directly from hot `(node_id, cyclomatic, cognitive)` tuples (zero cold `NodeExtension` touches).
    pub fn fill_complexity_from_raw(
        &mut self,
        rows: impl IntoIterator<Item = (Uuid, u32, u32)>,
        avg_cyclomatic: f64,
        max_cyclomatic: u32,
    ) {
        let node_count = self.node_count();
        if self.complexity.is_none() {
            self.complexity = Some(ComplexityTable::with_capacity(node_count));
        }
        let uuid_to_compact = &self.uuid_to_compact;
        let table = self.complexity.as_mut().unwrap();
        table.avg_cyclomatic = avg_cyclomatic;
        table.max_cyclomatic = max_cyclomatic;
        for (id, cyclomatic, cognitive) in rows {
            if let Some(&compact_id) = uuid_to_compact.get(&id) {
                let idx = compact_id as usize;
                table.cyclomatic[idx] = cyclomatic;
                table.cognitive[idx] = cognitive;
            }
        }
    }

    /// Initialize centrality table.
    pub fn init_centrality(&mut self) -> &mut CentralityTable {
        self.centrality = Some(CentralityTable::with_capacity(self.node_count()));
        self.centrality.as_mut().unwrap()
    }

    /// Write flat centrality arrays into the columnar table without intermediate mappings.
    pub fn fill_centrality_from_flat(
        &mut self,
        view: &PetGraphView,
        pagerank: &[f64],
        betweenness: &[f64],
        harmonic: &[f64],
        in_degree: &[usize],
        out_degree: &[usize],
    ) {
        let compact_to_uuid = &self.compact_to_uuid;
        let node_count = compact_to_uuid.len();
        let table = self
            .centrality
            .get_or_insert_with(|| CentralityTable::with_capacity(node_count));
        for (slot, uuid) in compact_to_uuid.iter().enumerate() {
            let Some(node_idx) = view.uuid_to_index.get(uuid) else {
                continue;
            };
            let flat_id = node_idx.index();
            table.pagerank[slot] = pagerank[flat_id] as f32;
            table.betweenness[slot] = betweenness[flat_id] as f32;
            table.harmonic[slot] = harmonic[flat_id] as f32;
            table.in_degree[slot] = in_degree[flat_id] as u32;
            table.out_degree[slot] = out_degree[flat_id] as u32;
        }
    }

    /// Initialize blast radius table.
    pub fn init_blast_radius(&mut self) -> &mut BlastRadiusTable {
        self.blast_radius = Some(BlastRadiusTable::with_capacity(self.node_count()));
        self.blast_radius.as_mut().unwrap()
    }

    /// Initialize structural sketch table.
    pub fn init_structural_sketch(&mut self) -> &mut StructuralSketchTable {
        self.structural_sketch = Some(StructuralSketchTable::with_capacity(self.node_count()));
        self.structural_sketch.as_mut().unwrap()
    }

    /// Copy eager token blooms from graph function nodes into columnar storage.
    pub fn fill_structural_sketch_from_graph(
        &mut self,
        backend: &rgctl_graph::backend::MemoryBackend,
    ) -> rgctl_error::Result<()> {
        self.fill_structural_sketch_from_lookup(backend)
    }

    /// Fill structural sketch blooms from any [`crate::node_lookup::NodeLookup`].
    pub fn fill_structural_sketch_from_lookup<L: crate::node_lookup::NodeLookup + ?Sized>(
        &mut self,
        lookup: &L,
    ) -> rgctl_error::Result<()> {
        let mut rows = Vec::new();
        lookup.for_each_node(&mut |node| {
            if let Some(bloom) = node.token_bloom
                && let Some(compact_id) = self.get_compact_id(node.id)
            {
                rows.push((compact_id, bloom));
            }
        })?;
        let table = self.init_structural_sketch();
        for (compact_id, bloom) in rows {
            table.set_bloom(compact_id, bloom);
        }
        Ok(())
    }

    /// Fill structural sketch blooms from an already-loaded function node slice.
    ///
    /// Prefer this over [`Self::fill_structural_sketch_from_lookup`] on cold discover
    /// — only Function/Method nodes carry `token_bloom`, and a full-graph cold_ext
    /// scan is wasteful once `functions` is in memory.
    pub fn fill_structural_sketch_from_nodes(
        &mut self,
        nodes: &[rgctl_graph::schema::Node],
    ) -> rgctl_error::Result<()> {
        let mut rows = Vec::new();
        for node in nodes {
            if let Some(bloom) = node.token_bloom
                && let Some(compact_id) = self.get_compact_id(node.id)
            {
                rows.push((compact_id, bloom));
            }
        }
        let table = self.init_structural_sketch();
        for (compact_id, bloom) in rows {
            table.set_bloom(compact_id, bloom);
        }
        Ok(())
    }

    /// Fill structural sketch blooms from hot `(node_id, bloom)` pairs (zero `Node` materialization).
    pub fn fill_structural_sketch_from_blooms(
        &mut self,
        blooms: impl IntoIterator<Item = (Uuid, [u64; 4])>,
    ) -> rgctl_error::Result<()> {
        let mut rows = Vec::new();
        for (id, bloom) in blooms {
            if let Some(compact_id) = self.get_compact_id(id) {
                rows.push((compact_id, bloom));
            }
        }
        let table = self.init_structural_sketch();
        for (compact_id, bloom) in rows {
            table.set_bloom(compact_id, bloom);
        }
        Ok(())
    }

    /// Get community ID for a UUID.
    pub fn get_community(&self, uuid: Uuid) -> Option<usize> {
        let compact_id = self.get_compact_id(uuid)?;
        self.community.as_ref()?.get(compact_id)
    }

    /// Get complexity metrics for a UUID.
    pub fn get_complexity(&self, uuid: Uuid) -> Option<(u32, u32)> {
        let compact_id = self.get_compact_id(uuid)?;
        self.complexity.as_ref()?.get(compact_id)
    }

    /// Get centrality metrics for a UUID.
    pub fn get_centrality(&self, uuid: Uuid) -> Option<CentralityMetrics> {
        let compact_id = self.get_compact_id(uuid)?;
        self.centrality.as_ref()?.get(compact_id)
    }

    /// Get blast radius metrics for a UUID.
    pub fn get_blast_radius(&self, uuid: Uuid) -> Option<BlastRadiusMetrics> {
        let compact_id = self.get_compact_id(uuid)?;
        self.blast_radius.as_ref()?.get(compact_id)
    }

    /// Save analysis results to a binary file.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::time::Instant;

        let serialize_start = Instant::now();
        let payload = bincode::serialize(self)
            .map_err(|e| rgctl_error::Error::SerdeError(format!("Failed to serialize: {}", e)))?;
        let serialize_secs = serialize_start.elapsed().as_secs_f64();

        let write_start = Instant::now();
        std::fs::write(path, &payload)?;
        let write_secs = write_start.elapsed().as_secs_f64();

        tracing::info!(
            target: "profile",
            serialize_secs,
            write_secs,
            bytes = payload.len(),
            "[profile] save_analysis breakdown"
        );

        Ok(())
    }

    /// Load analysis results from a binary file.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        bincode::deserialize_from(file)
            .map_err(|e| rgctl_error::Error::SerdeError(format!("Failed to deserialize: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_id_mapping() {
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        let uuid3 = Uuid::new_v4();

        let results = AnalysisResults::new(vec![uuid1, uuid2, uuid3]);

        assert_eq!(results.get_compact_id(uuid1), Some(0));
        assert_eq!(results.get_compact_id(uuid2), Some(1));
        assert_eq!(results.get_compact_id(uuid3), Some(2));

        assert_eq!(results.get_uuid(0), Some(uuid1));
        assert_eq!(results.get_uuid(1), Some(uuid2));
        assert_eq!(results.get_uuid(2), Some(uuid3));
    }

    #[test]
    fn test_community_table() {
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        let mut results = AnalysisResults::new(vec![uuid1, uuid2]);
        let table = results.init_community();

        table.assignments[0] = 1;
        table.assignments[1] = 2;
        table.num_communities = 2;

        assert_eq!(results.get_community(uuid1), Some(1));
        assert_eq!(results.get_community(uuid2), Some(2));
    }

    #[test]
    fn test_centrality_table() {
        let uuid1 = Uuid::new_v4();
        let mut results = AnalysisResults::new(vec![uuid1]);

        let table = results.init_centrality();
        table.pagerank[0] = 0.15;
        table.in_degree[0] = 5;

        let metrics = results.get_centrality(uuid1).unwrap();
        assert_eq!(metrics.pagerank, 0.15);
        assert_eq!(metrics.in_degree, 5);
    }

    #[test]
    #[ignore = "manual: profile save_analysis on example/linux artifact"]
    fn profile_save_linux_artifact() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("profile=info")
            .try_init();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../example/linux/.rgctl/analysis_results.bin");
        if !path.is_file() {
            eprintln!(
                "skip: {} missing (run discover on example/linux first)",
                path.display()
            );
            return;
        }
        let results = AnalysisResults::load(&path).unwrap();
        results
            .save(&Path::new("/tmp/rgctl-analysis_results-resave.bin"))
            .unwrap();
    }
}
