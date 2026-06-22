//! Dual-agent natural language query system (Phase 12.3).
//!
//! Rule-based primary agent decomposes questions; translation agent maps sub-queries
//! to graph patterns via fuzzy example matching, with [`PatternMatcher`] fallback.

use crate::pattern_matcher::{PatternMatcher, QueryResult, TranslatedQuery, TranslationMethod};
use crate::query_examples::{default_examples, QueryExample};
use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::query;
use rbuilder_graph::schema::Node;
use regex::Regex;
use std::collections::HashMap;
use strsim::jaro_winkler;

/// How a sub-query pattern was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DualTranslationMethod {
    /// Fuzzy match against example library
    ExampleMatch,
    /// Existing pattern matcher (templates / cache / heuristic)
    PatternMatcherFallback,
    /// LLM-assisted translation (feature `nlp-llm`)
    Llm,
}

/// A decomposed sub-question and its execution state.
#[derive(Debug, Clone)]
pub struct SubQuery {
    /// Natural language sub-question
    pub natural_language: String,
    /// Resolved graph or operation pattern
    pub translated_pattern: Option<String>,
    /// How the pattern was translated
    pub translation_method: Option<DualTranslationMethod>,
    /// Match confidence when available
    pub confidence: f64,
    /// Nodes returned for graph queries (empty for text-only operations)
    pub results: Vec<Node>,
    /// Text lines from operation-style queries (callers, impact, etc.)
    pub text_results: Vec<String>,
}

/// Accumulated state while answering a compound question.
#[derive(Debug, Clone)]
pub struct QueryContext {
    /// Original user question
    pub original_question: String,
    /// Decomposed sub-queries and their results
    pub sub_queries: Vec<SubQuery>,
}

impl QueryContext {
    /// Start a new query context.
    pub fn new(question: &str) -> Self {
        Self {
            original_question: question.to_string(),
            sub_queries: Vec::new(),
        }
    }

    /// Record a completed sub-query.
    pub fn add_sub_query(&mut self, sub: SubQuery) {
        self.sub_queries.push(sub);
    }

    /// Whether every sub-query has a translated pattern.
    pub fn all_translated(&self) -> bool {
        !self.sub_queries.is_empty()
            && self
                .sub_queries
                .iter()
                .all(|sq| sq.translated_pattern.is_some())
    }
}

/// Final result of the dual-agent pipeline.
#[derive(Debug, Clone)]
pub struct DualAgentResult {
    /// Original question
    pub question: String,
    /// Full query context including sub-queries
    pub context: QueryContext,
    /// Primary synthesized graph pattern (first sub-query or merged)
    pub primary_pattern: Option<String>,
    /// Aggregated node results across sub-queries
    pub nodes: Vec<Node>,
    /// Human-readable answer lines
    pub answer_lines: Vec<String>,
    /// Overall confidence
    pub confidence: f64,
}

/// Rule-based question planner (no LLM required).
pub struct PrimaryAgent {
    max_sub_queries: usize,
}

impl PrimaryAgent {
    /// Create a planner with a sub-query cap.
    pub fn new(max_sub_queries: usize) -> Self {
        Self { max_sub_queries }
    }

    /// Decompose a question into answerable sub-queries using keyword rules.
    pub fn decompose(&self, question: &str, context: &QueryContext) -> Result<Vec<String>> {
        if context.all_translated() {
            return Ok(Vec::new());
        }

        let q = question.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }

        let lower = q.to_lowercase();
        let mut parts: Vec<String> = Vec::new();

        // Multi-clause: "X and Y"
        if lower.contains(" and ") {
            for segment in q.split(" and ") {
                let segment = segment.trim().trim_end_matches('?');
                if !segment.is_empty() {
                    parts.push(normalize_clause(segment));
                }
            }
        }

        // Compound impact + detail: "what breaks if I change X and who calls X"
        if parts.is_empty() && (lower.contains("what breaks") || lower.contains("impact of")) {
            if let Some(symbol) = extract_symbol(q) {
                parts.push(format!("impact of changing {symbol}"));
                if lower.contains("who calls") || lower.contains("what calls") {
                    parts.push(format!("what calls {symbol}"));
                }
            }
        }

        // Security-style multi-hop (guide example)
        if parts.is_empty()
            && (lower.contains("security") || lower.contains("authentication"))
            && lower.contains("issue")
        {
            parts.push("find authentication-related functions".to_string());
            parts.push("functions that handle user input".to_string());
            parts.push("functions that construct SQL queries".to_string());
        }

        if parts.is_empty() {
            parts.push(q.trim_end_matches('?').to_string());
        }

        parts.truncate(self.max_sub_queries);
        Ok(parts)
    }

    /// Rule-based satisfaction check: done when every sub-query has results.
    pub fn has_sufficient_context(&self, context: &QueryContext) -> bool {
        context.all_translated()
            && context
                .sub_queries
                .iter()
                .any(|sq| !sq.results.is_empty() || !sq.text_results.is_empty())
    }

    /// Build a short textual synthesis from sub-query results.
    pub fn synthesize_answer(&self, question: &str, context: &QueryContext) -> Vec<String> {
        let mut lines = vec![format!("Answer for: {question}")];

        for sq in &context.sub_queries {
            let pattern = sq.translated_pattern.as_deref().unwrap_or("(untranslated)");
            lines.push(format!("- {} → `{pattern}`", sq.natural_language));

            if !sq.text_results.is_empty() {
                lines.extend(sq.text_results.iter().cloned());
            } else if !sq.results.is_empty() {
                for node in sq.results.iter().take(10) {
                    lines.push(format!("  • {} ({:?})", node.name, node.node_type));
                }
                if sq.results.len() > 10 {
                    lines.push(format!("  … and {} more", sq.results.len() - 10));
                }
            } else {
                lines.push("  (no matches)".to_string());
            }
        }

        lines
    }
}

impl Default for PrimaryAgent {
    fn default() -> Self {
        Self::new(4)
    }
}

/// Maps natural language to graph query patterns via fuzzy example matching.
pub struct TranslationAgent {
    examples: Vec<QueryExample>,
    min_similarity: f64,
    pattern_matcher: PatternMatcher,
}

impl TranslationAgent {
    /// Create an agent with built-in examples and similarity threshold.
    pub fn new(min_similarity: f64) -> Self {
        Self {
            examples: default_examples().to_vec(),
            min_similarity,
            pattern_matcher: PatternMatcher::new(),
        }
    }

    /// Attach domain-aware pattern matcher fallback.
    pub fn with_pattern_matcher(mut self, matcher: PatternMatcher) -> Self {
        self.pattern_matcher = matcher;
        self
    }

    /// Translate natural language to a query pattern string.
    pub fn translate(&self, nl_query: &str) -> Result<(String, DualTranslationMethod, f64)> {
        let normalized = normalize_clause(nl_query);
        let mut best: Option<(&QueryExample, f64)> = None;

        for example in &self.examples {
            let score = jaro_winkler(&normalized, &normalize_clause(example.nl));
            if score >= self.min_similarity && best.map(|(_, s)| score > s).unwrap_or(true) {
                best = Some((example, score));
            }
        }

        if let Some((example, score)) = best {
            let pattern = substitute_symbols(&normalized, example.pattern);
            return Ok((pattern, DualTranslationMethod::ExampleMatch, score));
        }

        // Fallback to existing PatternMatcher
        let translated = self.pattern_matcher.translate(nl_query)?;
        let pattern = pattern_matcher_to_string(&translated);
        Ok((
            pattern,
            DualTranslationMethod::PatternMatcherFallback,
            translated.confidence,
        ))
    }
}

impl Default for TranslationAgent {
    fn default() -> Self {
        Self::new(0.82)
    }
}

/// Dual-agent orchestrator: decompose → translate → execute → synthesize.
pub struct DualAgentQuerySystem {
    primary_agent: PrimaryAgent,
    translation_agent: TranslationAgent,
    max_iterations: usize,
}

impl DualAgentQuerySystem {
    /// Create with default agents.
    pub fn new() -> Self {
        Self {
            primary_agent: PrimaryAgent::default(),
            translation_agent: TranslationAgent::default(),
            max_iterations: 3,
        }
    }

    /// Override iteration limit.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Use a custom translation agent (e.g. with domain-aware matcher).
    pub fn with_translation_agent(mut self, agent: TranslationAgent) -> Self {
        self.translation_agent = agent;
        self
    }

    /// Process a natural language question against the graph (sync, no LLM).
    pub fn query(&self, question: &str, backend: &MemoryBackend) -> Result<DualAgentResult> {
        let mut context = QueryContext::new(question);

        for _ in 0..self.max_iterations {
            let sub_questions = self.primary_agent.decompose(question, &context)?;

            if sub_questions.is_empty() {
                break;
            }

            for nl_query in sub_questions {
                if context
                    .sub_queries
                    .iter()
                    .any(|sq| sq.natural_language == nl_query)
                {
                    continue;
                }

                let (pattern, method, confidence) = self.translation_agent.translate(&nl_query)?;
                let (nodes, text) = execute_pattern(&pattern, &nl_query, backend)?;

                context.add_sub_query(SubQuery {
                    natural_language: nl_query,
                    translated_pattern: Some(pattern),
                    translation_method: Some(method),
                    confidence,
                    results: nodes,
                    text_results: text,
                });
            }

            if self.primary_agent.has_sufficient_context(&context) {
                break;
            }
        }

        let answer_lines = self.primary_agent.synthesize_answer(question, &context);
        let primary_pattern = context
            .sub_queries
            .first()
            .and_then(|sq| sq.translated_pattern.clone());
        let confidence = context
            .sub_queries
            .iter()
            .map(|sq| sq.confidence)
            .fold(0.0_f64, f64::max);
        let mut nodes = Vec::new();
        for sq in &context.sub_queries {
            nodes.extend(sq.results.clone());
        }
        nodes.sort_by_key(|n| n.id);
        nodes.dedup_by_key(|n| n.id);

        Ok(DualAgentResult {
            question: question.to_string(),
            context,
            primary_pattern,
            nodes,
            answer_lines,
            confidence,
        })
    }

    /// Execute via PatternMatcher-compatible API for a single question.
    pub fn translate(&self, question: &str) -> Result<(String, DualTranslationMethod, f64)> {
        self.translation_agent.translate(question)
    }
}

impl Default for DualAgentQuerySystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a pattern string against the graph backend.
pub fn execute_pattern(
    pattern: &str,
    question: &str,
    backend: &MemoryBackend,
) -> Result<(Vec<Node>, Vec<String>)> {
    if pattern.contains("operation=") {
        return execute_operation_pattern(pattern, question, backend);
    }

    let nodes = query::execute(backend, pattern)?;
    Ok((nodes, Vec::new()))
}

fn execute_operation_pattern(
    pattern: &str,
    question: &str,
    backend: &MemoryBackend,
) -> Result<(Vec<Node>, Vec<String>)> {
    let params: HashMap<String, String> = pattern
        .split('|')
        .filter_map(|part| part.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let operation = params
        .get("operation")
        .cloned()
        .unwrap_or_else(|| "find".to_string());

    let internal = if params.contains_key("query") {
        params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("|")
    } else {
        pattern.to_string()
    };

    let translated = TranslatedQuery {
        question: question.to_string(),
        operation,
        internal_query: internal,
        description: "Dual-agent operation".to_string(),
        confidence: 1.0,
        method: TranslationMethod::PatternBased,
        intent: crate::intent::Intent::Find,
    };

    let matcher = PatternMatcher::new();
    match matcher.execute(&translated, backend)? {
        QueryResult::Nodes(nodes) => Ok((nodes, Vec::new())),
        QueryResult::Count(n) => Ok((Vec::new(), vec![format!("Count: {n}")])),
        QueryResult::Text(lines) => Ok((Vec::new(), lines)),
    }
}

fn pattern_matcher_to_string(translated: &TranslatedQuery) -> String {
    if translated.internal_query.contains("operation=") {
        translated.internal_query.clone()
    } else {
        match translated.operation.as_str() {
            "count" | "list" | "find" => translated.internal_query.clone(),
            other => {
                if translated.internal_query.is_empty() {
                    format!("operation={other}")
                } else {
                    format!("operation={other}|{}", translated.internal_query)
                }
            }
        }
    }
}

fn normalize_clause(text: &str) -> String {
    text.trim()
        .trim_end_matches('?')
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_symbol(question: &str) -> Option<String> {
    if let Ok(re) = Regex::new(
        r"(?i)(?:change|changing|calls|callers of|impact of changing)\s+([a-zA-Z_][a-zA-Z0-9_]*)",
    ) {
        if let Some(cap) = re.captures(question) {
            return Some(cap[1].to_string());
        }
    }
    crate::entity_extraction::EntityExtractor::new()
        .extract(question)
        .symbols
        .into_iter()
        .next()
}

/// Replace `symbol=` placeholders when the NL query names a concrete symbol.
fn substitute_symbols(nl: &str, pattern: &str) -> String {
    if !pattern.contains("symbol=") {
        return pattern.to_string();
    }

    let symbol = extract_symbol(nl).or_else(|| {
        Regex::new(r"\b([a-z_][a-z0-9_]{2,})\b")
            .ok()
            .and_then(|re| re.captures(nl).map(|cap| cap[1].to_string()))
    });

    if let Some(sym) = symbol {
        pattern
            .replace("symbol=authenticate", &format!("symbol={sym}"))
            .replace("symbol=verify_token", &format!("symbol={sym}"))
            .replace("symbol=process_payment", &format!("symbol={sym}"))
            .replace("symbol=process_order", &format!("symbol={sym}"))
            .replace("symbol=handle_request", &format!("symbol={sym}"))
            .replace("symbol=main", &format!("symbol={sym}"))
            .replace("symbol=fetch_user", &format!("symbol={sym}"))
    } else {
        pattern.to_string()
    }
}

#[cfg(feature = "nlp-llm")]
pub mod llm {
    //! Optional LLM-backed translation (requires `nlp-llm` feature and API key).

    use super::*;
    use rbuilder_error::Error;
    use std::env;

    /// LLM-enhanced dual agent stub: returns an error when no API key is configured.
    pub struct LlmDualAgentQuerySystem {
        inner: DualAgentQuerySystem,
    }

    impl LlmDualAgentQuerySystem {
        /// Wrap the rule-based system with optional LLM escalation.
        pub fn new() -> Self {
            Self {
                inner: DualAgentQuerySystem::new(),
            }
        }

        /// Attempt LLM translation; fails fast without `RBUILDER_LLM_API_KEY`.
        pub async fn query_with_llm(
            &self,
            _question: &str,
            _backend: &MemoryBackend,
        ) -> Result<DualAgentResult> {
            let api_key = env::var("RBUILDER_LLM_API_KEY").map_err(|_| {
                Error::NlpError(
                    "LLM translation requires RBUILDER_LLM_API_KEY environment variable".into(),
                )
            })?;

            if api_key.trim().is_empty() {
                return Err(Error::NlpError(
                    "LLM translation requires a non-empty RBUILDER_LLM_API_KEY".into(),
                ));
            }

            // Placeholder: real HTTP client integration would go here.
            Err(Error::NlpError(
                "LLM dual-agent path is not yet implemented; use rule-based query()".into(),
            ))
        }

        /// Fall back to rule-based query when LLM is unavailable.
        pub fn query(&self, question: &str, backend: &MemoryBackend) -> Result<DualAgentResult> {
            self.inner.query(question, backend)
        }
    }

    impl Default for LlmDualAgentQuerySystem {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};

    #[test]
    fn translation_agent_matches_callers_example() {
        let agent = TranslationAgent::new(0.75);
        let (pattern, method, score) = agent.translate("who calls verify_token").unwrap();
        assert_eq!(method, DualTranslationMethod::ExampleMatch);
        assert!(score > 0.75);
        assert!(pattern.contains("callers"));
        assert!(pattern.contains("verify_token"));
    }

    #[test]
    fn translation_agent_matches_compound_signature() {
        let agent = TranslationAgent::new(0.75);
        let (pattern, _, _) = agent.translate("async functions returning Result").unwrap();
        assert!(pattern.contains("signature:*async*"));
        assert!(pattern.contains("return_type:Result"));
    }

    #[test]
    fn dual_agent_decomposes_and_queries() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(
                Node::new(NodeType::Function, "run".to_string())
                    .with_signature("async fn run() -> Result<()>"),
            )
            .unwrap();
        backend
            .insert_node(Node::new(NodeType::Function, "sync".to_string()))
            .unwrap();

        let system = DualAgentQuerySystem::new();
        let result = system.query("async functions", &backend).unwrap();
        assert!(!result.nodes.is_empty());
        assert_eq!(result.nodes[0].name, "run");
    }
}
