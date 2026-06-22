//! Pattern-based natural language query translation
//!
//! Task 2.3.4: Integrate intent, entity extraction, templates, and execution.

use crate::entity_extraction::EntityExtractor;
use crate::intent::{Intent, IntentClassifier};
use crate::pattern_detection::DomainContext;
use crate::query_cache::{CachedQuery, QueryCache};
use crate::templates::{MatchedTemplate, QueryTemplates};
use rbuilder_analysis::{
    CentralityAnalyzer, CommunityDetector, ComplexityAnalyzer, DependencyAnalyzer,
};
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::GraphBackend;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::Node;
use rbuilder_project_config::analyzer::ConfigAnalyzer;

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
    domain: Option<DomainContext>,
}

impl Default for PatternMatcher {
    fn default() -> Self {
        Self {
            classifier: IntentClassifier::new(),
            extractor: EntityExtractor::new(),
            templates: QueryTemplates::new(),
            cache: QueryCache::bootstrap_default(),
            domain: None,
        }
    }
}

impl PatternMatcher {
    /// Create a new pattern matcher with bootstrap cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a matcher with domain context learned from the graph.
    pub fn from_graph(backend: &MemoryBackend) -> Result<Self> {
        let domain = crate::pattern_detection::PatternDetector::new().analyze(backend)?;
        Ok(Self::new().with_domain(domain))
    }

    /// Attach domain context for improved translation.
    pub fn with_domain(mut self, domain: DomainContext) -> Self {
        self.domain = Some(domain);
        self
    }

    /// Translate using the dual-agent example matcher first, then fall back to templates/cache.
    #[cfg(feature = "nlp-patterns")]
    pub fn translate_with_dual_agent(&self, question: &str) -> Result<TranslatedQuery> {
        use crate::dual_agent::{DualTranslationMethod, TranslationAgent};

        let agent = TranslationAgent::new(0.82).with_pattern_matcher(self.clone_for_fallback());
        let (pattern, method, confidence) = agent.translate(question)?;

        let dual_method = match method {
            DualTranslationMethod::ExampleMatch => TranslationMethod::PatternBased,
            DualTranslationMethod::PatternMatcherFallback => TranslationMethod::Heuristic,
            DualTranslationMethod::Llm => TranslationMethod::PatternBased,
        };

        let (operation, internal_query) = parse_dual_pattern(&pattern);
        let intent_result = self.classifier.classify(question);

        Ok(TranslatedQuery {
            question: question.to_string(),
            operation,
            internal_query,
            description: format!("Dual-agent translation ({method:?})"),
            confidence,
            method: dual_method,
            intent: intent_result.intent,
        })
    }

    #[cfg(feature = "nlp-patterns")]
    fn clone_for_fallback(&self) -> Self {
        Self {
            classifier: IntentClassifier::new(),
            extractor: EntityExtractor::new(),
            templates: QueryTemplates::new(),
            cache: QueryCache::bootstrap_default(),
            domain: self.domain.clone(),
        }
    }

    /// Translate a question to an internal query representation.
    pub fn translate(&self, question: &str) -> Result<TranslatedQuery> {
        let intent_result = self.classifier.classify(question);
        let mut entities = self.extractor.extract(question);

        // Apply domain vocabulary (Task 4.2.2)
        if let Some(ref domain) = self.domain {
            let q_lower = question.to_lowercase();
            for (term, label) in &domain.term_to_label {
                if q_lower.contains(term) {
                    entities.keywords.push(label.clone());
                }
            }
            // Also inject node type information from naming patterns
            for (term, node_type) in &domain.term_to_node_type {
                if q_lower.contains(term) && !entities.node_types.contains(node_type) {
                    entities.node_types.push(node_type.clone());
                }
            }
        }

        if let Some(template) = self
            .templates
            .find_match(question, intent_result.intent, &entities)
        {
            let mut internal = operation_to_query(&template);

            // Domain-aware label and naming pattern routing
            if let Some(ref domain) = self.domain {
                let q_lower = question.to_lowercase();

                // Check for label-based routing first
                for (term, label) in &domain.term_to_label {
                    if q_lower.contains(term) {
                        internal = format!("query=label:{label}");
                        break;
                    }
                }

                // If no label match, check for naming pattern routing
                if !internal.contains("label:") {
                    for pattern in &domain.naming_patterns {
                        let suffix_lower = pattern.suffix.to_lowercase();
                        let plural = format!("{}s", suffix_lower);

                        if q_lower.contains(&suffix_lower) || q_lower.contains(&plural) {
                            // Route to nodes matching this naming pattern
                            internal = format!("query=name_suffix:{}", pattern.suffix);
                            break;
                        }
                    }
                }
            }

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

        if let Some(hit) = self.cache.find_similar(question, 0.92) {
            let mut internal = cache_hit_to_query(&hit.entry);
            if let Some(ref domain) = self.domain {
                let q_lower = question.to_lowercase();
                for (term, label) in &domain.term_to_label {
                    if q_lower.contains(term) {
                        internal = format!("query=label:{label}");
                        break;
                    }
                }
            }
            return Ok(TranslatedQuery {
                question: question.to_string(),
                operation: hit.entry.operation.clone(),
                internal_query: internal,
                description: format!("Cache hit (similarity {:.2})", hit.similarity),
                confidence: hit.similarity,
                method: TranslationMethod::CacheHit,
                intent: intent_result.intent,
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
    pub fn execute(
        &self,
        translated: &TranslatedQuery,
        backend: &MemoryBackend,
    ) -> Result<QueryResult> {
        let params: std::collections::HashMap<_, _> = translated
            .internal_query
            .split('|')
            .filter_map(|part| part.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        match translated.operation.as_str() {
            "count" => {
                let q = params
                    .get("query")
                    .map(String::as_str)
                    .unwrap_or("type:Function");
                if q.starts_with("label:") {
                    let label = q.strip_prefix("label:").unwrap_or("");
                    let nodes = backend.all_nodes()?;
                    let count = nodes.iter().filter(|n| n.has_label(label)).count();
                    Ok(QueryResult::Count(count))
                } else {
                    let nodes = rbuilder_graph::query::execute(backend, q)?;
                    Ok(QueryResult::Count(nodes.len()))
                }
            }
            "list" | "find" => {
                let q = params
                    .get("query")
                    .map(String::as_str)
                    .unwrap_or("type:Function");
                Ok(QueryResult::Nodes(rbuilder_graph::query::execute(
                    backend, q,
                )?))
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
                    Ok(QueryResult::Text(vec![
                        "No circular dependencies found.".to_string()
                    ]))
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
                    Ok(QueryResult::Text(vec![
                        "No unused config keys found.".to_string()
                    ]))
                } else {
                    Ok(QueryResult::Text(
                        unused
                            .iter()
                            .map(|k| format!("- {} ({})", k.key, k.file.as_deref().unwrap_or("?")))
                            .collect(),
                    ))
                }
            }
            "missing_env" => {
                let missing = ConfigAnalyzer::find_missing_env_vars(
                    backend,
                    &[std::path::Path::new(".env")],
                )?;
                if missing.is_empty() {
                    Ok(QueryResult::Text(vec![
                        "No missing env vars detected.".to_string()
                    ]))
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
                let limit: usize = params
                    .get("limit")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(10);
                let report = ComplexityAnalyzer::analyze(backend)?;
                let nodes: Vec<Node> = report
                    .functions
                    .into_iter()
                    .take(limit)
                    .map(|f| f.node)
                    .collect();
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
                    callers
                        .into_iter()
                        .map(|c| format!("- depends via call: {c}"))
                        .collect(),
                ))
            }
            _ => Ok(QueryResult::Nodes(rbuilder_graph::query::execute(
                backend,
                &translated.internal_query,
            )?)),
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
                backend
                    .get_node(*id)
                    .ok()?
                    .map(|n| format!("{} ({score:.4})", n.name))
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("Top PageRank nodes: {top}"))
    }
}

fn cache_hit_to_query(entry: &CachedQuery) -> String {
    match entry.operation.as_str() {
        "count" | "list" | "find" => "query=type:Function".to_string(),
        other => format!("operation={other}|query=type:Function"),
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

    if let Some((_, sym)) = template
        .parameters
        .iter()
        .find(|(k, _)| k == "symbol" || k == "arg1")
    {
        if sym.contains('_') {
            parts.push(format!("symbol={sym}"));
        }
    }
    if let Some((_, threshold)) = template
        .parameters
        .iter()
        .find(|(k, _)| k == "arg2" || k == "threshold")
    {
        if threshold.chars().all(|c| c.is_ascii_digit()) {
            parts.push(format!("threshold={threshold}"));
        }
    }
    if let Some((_, limit)) = template
        .parameters
        .iter()
        .find(|(k, _)| k == "arg1" && k != "node_type")
    {
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
    let lower = raw.trim_end_matches(['?', '.', ',', ';']).to_lowercase();
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

#[cfg(feature = "nlp-patterns")]
fn parse_dual_pattern(pattern: &str) -> (String, String) {
    let params: std::collections::HashMap<_, _> = pattern
        .split('|')
        .filter_map(|part| part.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if let Some(op) = params.get("operation") {
        let internal = params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("|");
        return (op.clone(), internal);
    }

    if pattern.contains('=') && !pattern.contains(':') {
        ("find".to_string(), pattern.to_string())
    } else {
        ("find".to_string(), format!("query={pattern}"))
    }
}

fn extract_symbol_from_question(question: &str) -> Option<String> {
    if let Ok(re) = regex::Regex::new(
        r"(?i)(?:calls|change|depend(?:s|encies)?\s+on)\s+([a-zA-Z_][a-zA-Z0-9_]*)",
    ) {
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
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};

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
        backend
            .insert_node(Node::new(NodeType::Function, "main".to_string()))
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Function, "helper".to_string()))
            .unwrap();

        let matcher = PatternMatcher::new();
        let result = matcher.ask("how many functions?", &backend).unwrap();
        assert!(matches!(result, QueryResult::Count(2)));
    }
}
