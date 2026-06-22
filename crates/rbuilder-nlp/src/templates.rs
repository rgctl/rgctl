//! Query templates for natural language translation
//!
//! Task 2.3.3: 20+ templates mapping questions to graph operations.

use crate::entity_extraction::{ExtractedEntities, MetricFilter};
use crate::intent::Intent;
use regex::Regex;

/// A matched query template.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchedTemplate {
    /// Detected intent
    pub intent: Intent,
    /// Internal query string or operation name
    pub operation: String,
    /// Parameters extracted from the question
    pub parameters: Vec<(String, String)>,
    /// Human-readable description
    pub description: String,
    /// Match confidence
    pub confidence: f64,
}

/// Query template registry.
pub struct QueryTemplates {
    patterns: Vec<(Regex, Intent, &'static str, f64)>,
}

impl Default for QueryTemplates {
    fn default() -> Self {
        let raw = vec![
            (r"(?i)^how many (.+)\??$", Intent::Count, "count", 0.95),
            (r"(?i)^count (.+)$", Intent::Count, "count", 0.95),
            (r"(?i)^number of (.+)$", Intent::Count, "count", 0.95),
            (r"(?i)^show me all (.+)$", Intent::List, "list", 0.95),
            (r"(?i)^list all (.+)$", Intent::List, "list", 0.95),
            (r"(?i)^list (.+)$", Intent::List, "list", 0.9),
            (r"(?i)^find all (.+)$", Intent::Find, "find", 0.9),
            (r"(?i)^find (.+)$", Intent::Find, "find", 0.85),
            (r"(?i)what calls (.+)\??$", Intent::Callers, "callers", 0.95),
            (r"(?i)who calls (.+)\??$", Intent::Callers, "callers", 0.95),
            (
                r"(?i)what breaks if (?:i )?change (.+)\??$",
                Intent::Impact,
                "impact",
                0.95,
            ),
            (
                r"(?i)impact of changing (.+)$",
                Intent::Impact,
                "impact",
                0.9,
            ),
            (
                r"(?i)complexity of (.+)$",
                Intent::Complexity,
                "complexity_symbol",
                0.9,
            ),
            (
                r"(?i)high complexity (.+)$",
                Intent::Complexity,
                "high_complexity",
                0.9,
            ),
            (
                r"(?i)find (.+) with complexity > (\d+)$",
                Intent::Find,
                "complexity_filter",
                0.95,
            ),
            (r"(?i)top (\d+) (.+)$", Intent::Compare, "top_n", 0.9),
            (
                r"(?i)most complex (.+)$",
                Intent::Compare,
                "most_complex",
                0.9,
            ),
            (
                r"(?i)circular depend",
                Intent::CircularDeps,
                "circular_deps",
                0.95,
            ),
            (r"(?i)unused config", Intent::Config, "unused_config", 0.95),
            (r"(?i)missing env", Intent::Config, "missing_env", 0.95),
            (
                r"(?i)what uses config (.+)\??$",
                Intent::Config,
                "config_usage",
                0.9,
            ),
            (
                r"(?i)dependencies of (.+)$",
                Intent::Dependencies,
                "dependencies",
                0.9,
            ),
            (
                r"(?i)what does (.+) depend on",
                Intent::Dependencies,
                "dependencies",
                0.9,
            ),
            (r"(?i)hotspots?", Intent::Compare, "hotspots", 0.85),
            (r"(?i)functions?$", Intent::List, "list", 0.8),
        ];

        let patterns = raw
            .into_iter()
            .filter_map(|(pat, intent, op, conf)| {
                Regex::new(pat).ok().map(|re| (re, intent, op, conf))
            })
            .collect();

        Self { patterns }
    }
}

impl QueryTemplates {
    /// Create default templates.
    pub fn new() -> Self {
        Self::default()
    }

    /// Find the best matching template for a question.
    pub fn find_match(
        &self,
        question: &str,
        intent: Intent,
        entities: &ExtractedEntities,
    ) -> Option<MatchedTemplate> {
        for (re, template_intent, operation, confidence) in &self.patterns {
            if *template_intent != intent {
                continue;
            }
            if let Some(caps) = re.captures(question) {
                let mut params = Vec::new();
                for (i, cap) in caps.iter().enumerate().skip(1) {
                    if let Some(m) = cap {
                        params.push((format!("arg{i}"), m.as_str().to_string()));
                    }
                }
                return Some(MatchedTemplate {
                    intent: *template_intent,
                    operation: operation.to_string(),
                    parameters: params,
                    description: format!("{operation} via template"),
                    confidence: *confidence,
                });
            }
        }

        // Fallback from entities
        Some(fallback_template(intent, entities))
    }

    /// All registered template count.
    pub fn template_count(&self) -> usize {
        self.patterns.len()
    }
}

fn fallback_template(intent: Intent, entities: &ExtractedEntities) -> MatchedTemplate {
    let node_type = entities
        .node_types
        .first()
        .cloned()
        .unwrap_or_else(|| "function".to_string());

    let (operation, description) = match intent {
        Intent::Count => ("count".to_string(), format!("Count {node_type}s")),
        Intent::List => ("list".to_string(), format!("List {node_type}s")),
        Intent::Find => ("find".to_string(), format!("Find {node_type}s")),
        Intent::Complexity => (
            "high_complexity".to_string(),
            "High complexity functions".to_string(),
        ),
        Intent::Compare => (
            "hotspots".to_string(),
            "Top complexity hotspots".to_string(),
        ),
        Intent::Callers => ("callers".to_string(), "Find callers".to_string()),
        Intent::Impact => ("impact".to_string(), "Impact analysis".to_string()),
        Intent::Dependencies => ("dependencies".to_string(), "Dependencies".to_string()),
        Intent::Config => ("unused_config".to_string(), "Config analysis".to_string()),
        Intent::CircularDeps => (
            "circular_deps".to_string(),
            "Circular dependencies".to_string(),
        ),
    };

    let mut params = vec![("node_type".to_string(), node_type)];
    if let Some(sym) = entities.symbols.first() {
        params.push(("symbol".to_string(), sym.clone()));
    }
    for metric in &entities.metrics {
        if let MetricFilter::Complexity(n) = metric {
            params.push(("threshold".to_string(), n.to_string()));
        }
    }
    if let Some(limit) = entities.limit {
        params.push(("limit".to_string(), limit.to_string()));
    }

    MatchedTemplate {
        intent,
        operation,
        parameters: params,
        description,
        confidence: 0.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_extraction::EntityExtractor;
    use crate::intent::IntentClassifier;

    #[test]
    fn test_template_matching() {
        let templates = QueryTemplates::new();
        assert!(templates.template_count() >= 20);

        let classifier = IntentClassifier::new();
        let extractor = EntityExtractor::new();
        let question = "How many functions?";
        let intent = classifier.classify(question).intent;
        let entities = extractor.extract(question);
        let matched = templates.find_match(question, intent, &entities).unwrap();
        assert_eq!(matched.intent, Intent::Count);
    }
}
