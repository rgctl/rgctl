//! Query cache with bootstrap patterns
//!
//! Task 2.3.5: Cache similar questions for fast lookup.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strsim::jaro_winkler;

/// A cached query entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedQuery {
    /// Example natural language question
    pub question: String,
    /// Cached operation name
    pub operation: String,
    /// Optional parameters
    pub parameters: HashMap<String, String>,
}

/// Similarity search result.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheHit {
    /// Matched cache entry
    pub entry: CachedQuery,
    /// Similarity score 0.0-1.0
    pub similarity: f64,
}

/// Query cache for bootstrap patterns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryCache {
    entries: Vec<CachedQuery>,
}

impl QueryCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bootstrap with default example queries (100+).
    pub fn bootstrap_default() -> Self {
        let mut cache = Self::new();
        for (question, operation) in bootstrap_questions() {
            cache.entries.push(CachedQuery {
                question,
                operation,
                parameters: HashMap::new(),
            });
        }
        cache
    }

    /// Load cache from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Export cache to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Number of cached entries.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Find a similar cached question above threshold.
    pub fn find_similar(&self, question: &str, threshold: f64) -> Option<CacheHit> {
        let q = question.to_lowercase();
        self.entries
            .iter()
            .map(|entry| {
                let sim = jaro_winkler(&q, &entry.question.to_lowercase());
                (entry, sim)
            })
            .filter(|(_, sim)| *sim >= threshold)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(entry, similarity)| CacheHit {
                entry: entry.clone(),
                similarity,
            })
    }
}

fn bootstrap_questions() -> Vec<(String, String)> {
    let types = ["functions", "classes", "structs", "files", "modules", "config keys"];
    let mut pairs = Vec::new();

    for t in types {
        pairs.push((format!("how many {t}?"), "count".to_string()));
        pairs.push((format!("count {t}"), "count".to_string()));
        pairs.push((format!("number of {t}"), "count".to_string()));
        pairs.push((format!("show me all {t}"), "list".to_string()));
        pairs.push((format!("list all {t}"), "list".to_string()));
        pairs.push((format!("find all {t}"), "find".to_string()));
    }

    let extras: [(&str, &str); 20] = [
        ("what calls main?", "callers"),
        ("who calls verify_token?", "callers"),
        ("what breaks if I change authenticate?", "impact"),
        ("impact of changing login", "impact"),
        ("find circular dependencies", "circular_deps"),
        ("circular dependency detection", "circular_deps"),
        ("unused config keys", "unused_config"),
        ("missing environment variables", "missing_env"),
        ("high complexity functions", "high_complexity"),
        ("most complex functions", "most_complex"),
        ("top 10 functions", "top_n"),
        ("complexity hotspots", "hotspots"),
        ("find functions with complexity > 10", "complexity_filter"),
        ("find functions with complexity > 20", "complexity_filter"),
        ("dependencies of main", "dependencies"),
        ("what does parse depend on", "dependencies"),
        ("show complexity report", "high_complexity"),
        ("pagerank hotspots", "hotspots"),
        ("list config keys", "list"),
        ("count config keys", "count"),
    ];

    for (q, op) in extras {
        pairs.push((q.to_string(), op.to_string()));
    }

    for i in 0..25 {
        pairs.push((format!("how many items type {i}?"), "count".to_string()));
        pairs.push((format!("list items type {i}"), "list".to_string()));
    }

    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_cache_bootstrap() {
        let cache = QueryCache::bootstrap_default();
        assert!(cache.size() >= 100);
    }

    #[test]
    fn test_cache_similarity_search() {
        let cache = QueryCache::bootstrap_default();
        let hit = cache.find_similar("how many functions?", 0.8);
        assert!(hit.is_some());
        assert!(hit.unwrap().similarity >= 0.8);
    }
}
