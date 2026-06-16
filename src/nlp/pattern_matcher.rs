//! Pattern-based natural language query translation
//!
//! Task 2.3.4: Integrate intent, entity extraction, templates, and execution.

use crate::graph::backend::GraphBackend;
use crate::analysis::{
    CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer,
};
use crate::config::analyzer::ConfigAnalyzer;
use crate::error::{Error, Result};
use crate::graph::backend::MemoryBackend;
use crate::graph::schema::Node;
use crate::nlp::entity_extraction::EntityExtractor;
use crate::nlp::intent::{Intent, IntentClassifier};
use crate::nlp::query_cache::QueryCache;
use crate::nlp::templates::{MatchedTemplate, QueryTemplates};

/// Translation method used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationMethod {
    /// Matched a query template
    PatternBased,
    /// Matched a cached similar question
    CacheHit,
    /// Fallback heuristic
    Heuristic,
}

/// Result of translating a natural language question.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedQuery {
    /// Original question
    pub question: String,
    /// Resolved operation
    pub operation: String,
    /// Internal query representation
    pub internal_query: String,
    /// Human-readable description
    pub description: String,
    /// Confidence 0.0-1.0
    pub confidence: f64,
    /// Translation method
    pub method: TranslationMethod,
    /// Detected intent
    pub intent: Intent,
}

/// Query execution result.
#[derive(Debug, Clone)]
pub enum QueryResult {
    /// List of matching nodes
    Nodes(Vec<Node>),
    /// Numeric count
    Count(usize),
    /// Text lines for display
    Text(Vec<String>),
}

/// Pattern-based NLP query engine.
pub struct PatternMatcher {
    classifier: IntentClassifier,
    extractor: EntityExtractor,
    templates: QueryTemplates,
    cache: QueryCache,
}

impl Default for PatternMatcher {
    fn default() -> Self {
        Self {
            classifier: IntentClassifier::new(),
            extractor: EntityExtractor::new(),
            templates: QueryTemplates::new(),
            cache: QueryCache::bootstrap_default(),
        }
    }
}

impl PatternMatcher {
    /// Create a new pattern matcher with bootstrap cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate a question to an internal query representation.
    pub fn translate(&self, question: &str) -> Result<TranslatedQuery> {
        let intent_result = self.classifier.classify(question);
        let entities = self.extractor.extract(question);

        if let Some(hit) = self.cache.find_similar(question, 0.92) {
            return Ok(TranslatedQuery {
                question: question.to_string(),
                operation: hit.entry.operation.clone(),
                internal_query: hit.entry.operation.clone(),
                description: format!("Cache hit (similarity {:.2})", hit.similarity),
                confidence: hit.similarity,
                method: TranslationMethod::CacheHit,
                intent: intent_result.intent,
            });
        }

        if let Some(template) = self
            .templates
            .find_match(question, intent_result.intent, &entities)
        {
            let internal = operation_to_query(&template);
            return Ok(TranslatedQuery {
                question: question.to_string(),
                operation: template.operation.clone(),
                internal_query: internal,
                description: template.description,
                confidence: template.confidence * intent_result.confidence,
                method: TranslationMethod::PatternBased,
                intent: template.intent,
            });
        }

        Ok(TranslatedQuery {
            question: question.to_string(),
            operation: "find".to_string(),
            internal_query: "type:Function".to_string(),
            description: "Fallback query".to_string(),
            confidence: 0.4,
            method: TranslationMethod::Heuristic,
            intent: intent_result.intent,
        })
    }

    /// Translate and execute a question against the graph.
    pub fn ask(&self, question: &str, backend: &MemoryBackend) -> Result<QueryResult> {
        let translated = self.translate(question)?;
        self.execute(&translated, backend)
    }

    /// Execute a translated query.
    pub fn execute(&self, translated: &TranslatedQuery, backend: &MemoryBackend) -> Result<QueryResult> {
        let params: std::collections::HashMap<_, _> = translated
            .internal_query
            .split('|')
            .filter_map(|part| part.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        match translated.operation.as_str() {
            "count" => {
                let q = params.get("query").map(String::as_str).unwrap_or("type:Function");
                let nodes = crate::graph::query::execute(backend, q)?;
                Ok(QueryResult::Count(nodes.len()))
            }
            "list" | "find" => {
                let q = params.get("query").map(String::as_str).unwrap_or("type:Function");
                Ok(QueryResult::Nodes(crate::graph::query::execute(backend, q)?))
            }
            "callers" => {
                let symbol = params
                    .get("symbol")
                    .cloned()
                    .or_else(|| extract_symbol_from_question(&translated.question))
                    .ok_or_else(|| Error::InvalidQuery("No symbol specified".to_string()))?;
                let callers = DependencyAnalyzer::find_callers(backend, &symbol)?;
                Ok(QueryResult::Text(
                    callers.into_iter().map(|c| format!("- {c}")).collect(),
                ))
            }
            "impact" => {
                let symbol = params
                    .get("symbol")
                    .cloned()
                    .or_else(|| extract_symbol_from_question(&translated.question))
                    .ok_or_else(|| Error::InvalidQuery("No symbol specified".to_string()))?;
                let impact = DependencyAnalyzer::calculate_impact_radius(backend, &symbol)?;
                let mut lines = vec![format!(
                    "Changing `{}` affects {} node(s) (depth {})",
                    impact.source_name,
                    impact.affected_names.len(),
                    impact.max_depth
                )];
                for name in impact.affected_names.iter().take(20) {
                    lines.push(format!("- {name}"));
                }
                Ok(QueryResult::Text(lines))
            }
            "circular_deps" => {
                let cycles = DependencyAnalyzer::find_circular_dependencies(backend)?;
                if cycles.is_empty() {
                    Ok(QueryResult::Text(vec!["No circular dependencies found.".to_string()]))
                } else {
                    let lines: Vec<String> = cycles
                        .iter()
                        .map(|c| format!("- Cycle: {}", c.names.join(" -> ")))
                        .collect();
                    Ok(QueryResult::Text(lines))
                }
            }
            "unused_config" => {
                let unused = ConfigAnalyzer::find_unused_keys(backend)?;
                if unused.is_empty() {
                    Ok(QueryResult::Text(vec!["No unused config keys found.".to_string()]))
                } else {
                    Ok(QueryResult::Text(
                        unused.iter().map(|k| format!("- {} ({})", k.key, k.file.as_deref().unwrap_or("?"))).collect(),
                    ))
                }
            }
            "missing_env" => {
                let missing = ConfigAnalyzer::find_missing_env_vars(backend, &[std::path::Path::new(".env")])?;
                if missing.is_empty() {
                    Ok(QueryResult::Text(vec!["No missing env vars detected.".to_string()]))
                } else {
                    Ok(QueryResult::Text(
                        missing.iter().map(|m| format!("- {}", m.var)).collect(),
                    ))
                }
            }
            "high_complexity" | "most_complex" | "hotspots" | "complexity_filter" => {
                let threshold: usize = params
                    .get("threshold")
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(10);
                let nodes = ComplexityAnalyzer::find_above_threshold(backend, threshold)?;
                Ok(QueryResult::Nodes(nodes))
            }
            "top_n" => {
                let limit: usize = params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(10);
                let report = ComplexityAnalyzer::analyze(backend)?;
                let nodes: Vec<Node> = report.functions.into_iter().take(limit).map(|f| f.node).collect();
                Ok(QueryResult::Nodes(nodes))
            }
            "complexity_symbol" => {
                let symbol = params
                    .get("symbol")
                    .cloned()
                    .or_else(|| extract_symbol_from_question(&translated.question))
                    .ok_or_else(|| Error::InvalidQuery("No symbol specified".to_string()))?;
                let report = ComplexityAnalyzer::analyze(backend)?;
                if let Some(f) = report.functions.iter().find(|f| f.node.name == symbol) {
                    Ok(QueryResult::Text(vec![format!(
                        "{}: cyclomatic={}, cognitive={}, level={:?}",
                        f.node.name, f.cyclomatic, f.cognitive, f.level
                    )]))
                } else {
                    Err(Error::NodeNotFound(symbol))
                }
            }
            "dependencies" => {
                let symbol = params
                    .get("symbol")
                    .cloned()
                    .or_else(|| extract_symbol_from_question(&translated.question))
                    .ok_or_else(|| Error::InvalidQuery("No symbol specified".to_string()))?;
                let callers = DependencyAnalyzer::find_callers(backend, &symbol)?;
                Ok(QueryResult::Text(
                    callers.into_iter().map(|c| format!("- depends via call: {c}")).collect(),
                ))
            }
            _ => {
                Ok(QueryResult::Nodes(crate::graph::query::execute(
                    backend,
                    &translated.internal_query,
                )?))
            }
        }
    }

    /// Run analysis reports (community, centrality).
    pub fn analyze_communities(&self, backend: &MemoryBackend) -> Result<String> {
        let result = CommunityDetector::new().detect(backend)?;
        Ok(format!(
            "Detected {} communities (modularity {:.3})",
            result.communities.len(),
            result.modularity
        ))
    }

    /// Run centrality analysis summary.
    pub fn analyze_centrality(&self, backend: &MemoryBackend) -> Result<String> {
        let report = CentralityAnalyzer::new().analyze(backend)?;
        let top = report
            .top_pagerank
            .iter()
            .take(5)
            .filter_map(|(id, score)| {
                backend.get_node(*id).ok()?.map(|n| format!("{} ({score:.4})", n.name))
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("Top PageRank nodes: {top}"))
    }
}

fn operation_to_query(template: &MatchedTemplate) -> String {
    let node_type = template
        .parameters
        .iter()
        .find(|(k, _)| k == "arg1" || k == "node_type")
        .map(|(_, v)| normalize_node_type(v))
        .unwrap_or_else(|| "Function".to_string());

    let mut parts = vec![format!("query=type:{node_type}")];

    if let Some((_, sym)) = template.parameters.iter().find(|(k, _)| k == "symbol" || k == "arg1") {
        if sym.contains('_') {
            parts.push(format!("symbol={sym}"));
        }
    }
    if let Some((_, threshold)) = template.parameters.iter().find(|(k, _)| k == "arg2" || k == "threshold") {
        if threshold.chars().all(|c| c.is_ascii_digit()) {
            parts.push(format!("threshold={threshold}"));
        }
    }
    if let Some((_, limit)) = template.parameters.iter().find(|(k, _)| k == "arg1" && k != "node_type") {
        if limit.chars().all(|c| c.is_ascii_digit()) {
            parts.push(format!("limit={limit}"));
        }
    }

    match template.operation.as_str() {
        "count" | "list" | "find" => parts.join("|"),
        other => format!("operation={other}|{}", parts.join("|")),
    }
}

fn normalize_node_type(raw: &str) -> String {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "functions" | "function" => "Function".to_string(),
        "classes" | "class" | "components" | "component" | "services" | "service" => {
            "Class".to_string()
        }
        "structs" | "struct" => "Struct".to_string(),
        "files" | "file" => "File".to_string(),
        "modules" | "module" => "Module".to_string(),
        "config keys" | "config" | "configkeys" => "ConfigKey".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                None => "Function".to_string(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

fn extract_symbol_from_question(question: &str) -> Option<String> {
    if let Ok(re) = regex::Regex::new(r"(?i)(?:calls|change|depend(?:s|encies)?\s+on)\s+([a-zA-Z_][a-zA-Z0-9_]*)") {
        if let Some(cap) = re.captures(question) {
            return Some(cap[1].to_string());
        }
    }
    EntityExtractor::new()
        .extract(question)
        .symbols
        .into_iter()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Node, NodeType};

    #[test]
    fn test_pattern_based_translation() {
        let matcher = PatternMatcher::new();
        let result = matcher.translate("How many Rust structs exist?").unwrap();
        assert!(result.confidence > 0.5);
        assert_eq!(result.method, TranslationMethod::PatternBased);
    }

    #[test]
    fn test_ask_count() {
        let mut backend = MemoryBackend::new();
        backend.insert_node(Node::new(NodeType::Function, "main".to_string())).unwrap();
        backend.insert_node(Node::new(NodeType::Function, "helper".to_string())).unwrap();

        let matcher = PatternMatcher::new();
        let result = matcher.ask("how many functions?", &backend).unwrap();
        assert!(matches!(result, QueryResult::Count(2)));
    }
}
