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

use rbuilder_error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

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
}

impl CommunityTable {
    /// Create a new empty table with capacity for `node_count` nodes.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            assignments: vec![0; node_count],
            modularity: 0.0,
            num_communities: 0,
        }
    }

    /// Get community ID for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<usize> {
        self.assignments.get(id as usize).copied()
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
            in_degree: vec![0; node_count],
            out_degree: vec![0; node_count],
        }
    }

    /// Get centrality metrics for a compact node ID.
    pub fn get(&self, id: CompactId) -> Option<CentralityMetrics> {
        Some(CentralityMetrics {
            pagerank: *self.pagerank.get(id as usize)?,
            betweenness: *self.betweenness.get(id as usize)?,
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

    /// Initialize complexity table.
    pub fn init_complexity(&mut self) -> &mut ComplexityTable {
        self.complexity = Some(ComplexityTable::with_capacity(self.node_count()));
        self.complexity.as_mut().unwrap()
    }

    /// Initialize centrality table.
    pub fn init_centrality(&mut self) -> &mut CentralityTable {
        self.centrality = Some(CentralityTable::with_capacity(self.node_count()));
        self.centrality.as_mut().unwrap()
    }

    /// Initialize blast radius table.
    pub fn init_blast_radius(&mut self) -> &mut BlastRadiusTable {
        self.blast_radius = Some(BlastRadiusTable::with_capacity(self.node_count()));
        self.blast_radius.as_mut().unwrap()
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
        let file = std::fs::File::create(path)?;
        bincode::serialize_into(file, self)
            .map_err(|e| rbuilder_error::Error::SerdeError(format!("Failed to serialize: {}", e)))?;
        Ok(())
    }

    /// Load analysis results from a binary file.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        bincode::deserialize_from(file)
            .map_err(|e| rbuilder_error::Error::SerdeError(format!("Failed to deserialize: {}", e)))
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
}
