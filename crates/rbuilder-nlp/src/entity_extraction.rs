//! Entity extraction from natural language queries
//!
//! Task 2.3.2: Extract labels, symbols, metrics, and limits.

use regex::Regex;
use std::collections::HashSet;

/// A metric filter extracted from a query.
#[derive(Debug, Clone, PartialEq)]
pub enum MetricFilter {
    /// Complexity greater than threshold
    Complexity(usize),
    /// PageRank / centrality top N
    TopN(usize),
}

/// Extracted entities from a question.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedEntities {
    /// Node type labels (e.g. "function", "class")
    pub node_types: Vec<String>,
    /// Symbol names (e.g. "verify_token")
    pub symbols: Vec<String>,
    /// Metric filters
    pub metrics: Vec<MetricFilter>,
    /// Result limit
    pub limit: Option<usize>,
    /// Free-text search terms
    pub keywords: Vec<String>,
}

/// Entity extractor using regex and keyword maps.
pub struct EntityExtractor {
    type_aliases: Vec<(Regex, String)>,
}

impl Default for EntityExtractor {
    fn default() -> Self {
        let aliases = vec![
            (r"\bfunctions?\b", "function"),
            (r"\bclasses?\b", "class"),
            (r"\bstructs?\b", "struct"),
            (r"\benums?\b", "enum"),
            (r"\bmodules?\b", "module"),
            (r"\bfiles?\b", "file"),
            (r"\bconfig(?:uration)?\s+keys?\b", "configkey"),
            (r"\bcomponents?\b", "class"),
            (r"\bservices?\b", "class"),
            (r"\binterfaces?\b", "interface"),
            (r"\bimports?\b", "import"),
        ];
        let type_aliases = aliases
            .into_iter()
            .filter_map(|(pat, label)| {
                Regex::new(&format!("(?i){pat}"))
                    .ok()
                    .map(|re| (re, label.to_string()))
            })
            .collect();
        Self { type_aliases }
    }
}

impl EntityExtractor {
    /// Create a new entity extractor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Extract entities from a natural language question.
    pub fn extract(&self, question: &str) -> ExtractedEntities {
        let mut entities = ExtractedEntities::default();
        let q = question.to_lowercase();

        for (re, label) in &self.type_aliases {
            if re.is_match(&q) && !entities.node_types.contains(label) {
                entities.node_types.push(label.clone());
            }
        }

        if let Ok(sym_re) = Regex::new(r"\b([a-z_][a-z0-9_]{2,})\b") {
            let stopwords: HashSet<&str> = [
                "how",
                "many",
                "what",
                "show",
                "find",
                "all",
                "the",
                "with",
                "that",
                "this",
                "from",
                "calls",
                "breaks",
                "change",
                "most",
                "high",
                "complexity",
                "functions",
                "function",
                "classes",
                "class",
                "files",
                "file",
                "config",
                "top",
                "list",
            ]
            .into_iter()
            .collect();

            for cap in sym_re.captures_iter(question) {
                let sym = cap[1].to_string();
                if !stopwords.contains(sym.as_str()) && sym.contains('_') {
                    entities.symbols.push(sym);
                }
            }
        }

        if let Ok(threshold_re) = Regex::new(r"(?i)complexity\s*(?:>|greater than|above)\s*(\d+)") {
            if let Some(cap) = threshold_re.captures(question) {
                if let Ok(n) = cap[1].parse() {
                    entities.metrics.push(MetricFilter::Complexity(n));
                }
            }
        }

        if let Ok(top_re) = Regex::new(r"(?i)(?:top|first)\s+(\d+)") {
            if let Some(cap) = top_re.captures(question) {
                if let Ok(n) = cap[1].parse() {
                    entities.limit = Some(n);
                    entities.metrics.push(MetricFilter::TopN(n));
                }
            }
        }

        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_extraction() {
        let extractor = EntityExtractor::new();
        let entities = extractor.extract("how many functions?");
        assert!(entities.node_types.contains(&"function".to_string()));
    }

    #[test]
    fn test_symbol_extraction() {
        let extractor = EntityExtractor::new();
        let entities = extractor.extract("what calls verify_token?");
        assert!(entities.symbols.contains(&"verify_token".to_string()));
    }

    #[test]
    fn test_metric_extraction() {
        let extractor = EntityExtractor::new();
        let entities = extractor.extract("find functions with complexity > 20");
        assert!(entities
            .metrics
            .iter()
            .any(|m| matches!(m, MetricFilter::Complexity(20))));
    }
}
