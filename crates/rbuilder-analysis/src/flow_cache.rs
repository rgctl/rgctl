//! CFG/PDG cache keyed by code hash.

use crate::cfg::ControlFlowGraph;
use crate::cfg_builder::build_cfg_for_function;
use crate::pdg::ProgramDependenceGraph;
use rbuilder_error::Result;
use rbuilder_graph::code_index::hash_code;
use std::collections::HashMap;

/// Cached CFG and PDG for a function body.
#[derive(Debug, Clone)]
pub struct CachedAnalysis {
    /// Code hash at build time.
    pub code_hash: String,
    /// Control-flow graph.
    pub cfg: ControlFlowGraph,
    /// Program dependence graph.
    pub pdg: ProgramDependenceGraph,
}

/// In-memory cache for CFG/PDG analysis results.
#[derive(Debug, Default)]
pub struct CfgPdgCache {
    entries: HashMap<String, CachedAnalysis>,
}

impl CfgPdgCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build or return cached CFG/PDG for a function body.
    pub fn get_or_build(
        &mut self,
        language: &str,
        source: &str,
        function_name: &str,
    ) -> Result<&CachedAnalysis> {
        let code_hash = hash_code(source);
        let key = cache_key(language, function_name, &code_hash);

        if !self.entries.contains_key(&key) {
            let cfg = build_cfg_for_function(language, source, function_name)?;
            let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes())?;
            self.entries.insert(
                key.clone(),
                CachedAnalysis {
                    code_hash,
                    cfg,
                    pdg,
                },
            );
        }

        Ok(self.entries.get(&key).expect("cache entry just inserted"))
    }

    /// Invalidate all entries for a function name (any hash).
    pub fn invalidate_function(&mut self, language: &str, function_name: &str) {
        let prefix = format!("{language}::{function_name}::");
        self.entries.retain(|k, _| !k.starts_with(&prefix));
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true when no entries are cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn cache_key(language: &str, function_name: &str, code_hash: &str) -> String {
    format!("{language}::{function_name}::{code_hash}")
}

/// Node-id indexed PDG storage for graph-level tools (blast radius).
#[derive(Debug, Default)]
pub struct NodePdgCache {
    entries: std::collections::HashMap<uuid::Uuid, ProgramDependenceGraph>,
}

/// Alias used by blast radius analysis.
pub type FlowCache = NodePdgCache;

impl NodePdgCache {
    /// Create an empty node PDG cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a PDG for a graph node id.
    pub fn insert_pdg(&mut self, node_id: uuid::Uuid, pdg: ProgramDependenceGraph) {
        self.entries.insert(node_id, pdg);
    }

    /// Lookup PDG for a graph node id.
    pub fn get_pdg(&self, node_id: uuid::Uuid) -> Option<&ProgramDependenceGraph> {
        self.entries.get(&node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_on_same_source() {
        let mut cache = CfgPdgCache::new();
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let hash = {
            let first = cache.get_or_build("rust", code, "add").unwrap();
            first.code_hash.clone()
        };
        let second = cache.get_or_build("rust", code, "add").unwrap();
        assert_eq!(hash, second.code_hash);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_miss_on_changed_source() {
        let mut cache = CfgPdgCache::new();
        cache
            .get_or_build("rust", "fn f() { let x = 1; }", "f")
            .unwrap();
        cache
            .get_or_build("rust", "fn f() { let x = 2; }", "f")
            .unwrap();
        assert_eq!(cache.len(), 2);
    }
}
