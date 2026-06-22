//! Conversation context for multi-turn NLP queries

use crate::pattern_matcher::QueryResult;
use std::collections::VecDeque;

/// Tracks conversation state across multiple queries.
#[derive(Debug, Clone, Default)]
pub struct ConversationContext {
    history: VecDeque<String>,
    focused_nodes: Vec<String>,
    last_community: Option<String>,
    last_count: Option<usize>,
    max_history: usize,
}

impl ConversationContext {
    /// Create a new empty conversation context.
    pub fn new() -> Self {
        Self {
            max_history: 20,
            ..Default::default()
        }
    }

    /// Record a user query in history.
    pub fn add_query(&mut self, query: &str) {
        self.history.push_back(query.to_string());
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Focus on a node mentioned in the conversation.
    pub fn add_focused_node(&mut self, name: &str) {
        if !self.focused_nodes.iter().any(|n| n == name) {
            self.focused_nodes.push(name.to_string());
        }
        if self.focused_nodes.len() > 5 {
            self.focused_nodes.remove(0);
        }
    }

    /// Set the last community discussed.
    pub fn set_last_community(&mut self, name: impl Into<String>) {
        self.last_community = Some(name.into());
    }

    /// Record the result count from the last query.
    pub fn set_last_count(&mut self, count: usize) {
        self.last_count = Some(count);
    }

    /// Extract focused node names from query results.
    pub fn update_from_result(&mut self, question: &str, result: &QueryResult) {
        match result {
            QueryResult::Nodes(nodes) => {
                self.set_last_count(nodes.len());
                if nodes.len() == 1 {
                    self.add_focused_node(&nodes[0].name);
                } else if let Some(node) = nodes.first() {
                    self.add_focused_node(&node.name);
                }
            }
            QueryResult::Count(n) => self.set_last_count(*n),
            QueryResult::Text(lines) => {
                for line in lines {
                    if let Some(name) = extract_symbol_from_line(line) {
                        self.add_focused_node(&name);
                    }
                }
            }
        }

        // Extract explicit symbol mentions from the question
        for word in question.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if clean.len() > 2
                && clean
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase() || c == '_')
            {
                self.add_focused_node(clean);
            }
        }
    }

    /// Resolve pronouns and references in a follow-up question.
    pub fn resolve_references(&self, question: &str) -> String {
        let q_lower = question.to_lowercase();
        let mut resolved = question.to_string();

        let pronouns = ["its", "it", "that", "those", "this", "they"];
        let has_pronoun = pronouns.iter().any(|p| {
            q_lower.contains(&format!(" {p} "))
                || q_lower.ends_with(&format!(" {p}"))
                || q_lower.ends_with(&format!(" {p}?"))
                || q_lower.contains(&format!(" {p}?"))
        });

        if has_pronoun {
            if let Some(node) = self.focused_nodes.last() {
                for p in pronouns {
                    resolved = resolved
                        .replace(&format!(" {p} "), &format!(" {node} "))
                        .replace(&format!(" {p}?"), &format!(" {node}?"))
                        .replace(&format!(" {p}."), &format!(" {node}."))
                        .replace(&format!(" {p}'s "), &format!(" {node}'s "))
                        .replace(&format!(" {p}'s?"), &format!(" {node}'s?"));
                }
                for p in pronouns {
                    if q_lower.ends_with(&format!(" {p}")) || q_lower.ends_with(&format!(" {p}?")) {
                        let suffix = if question.ends_with('?') { "?" } else { "" };
                        let base = question.trim_end_matches('?').trim();
                        if base.to_lowercase().ends_with(p) {
                            let prefix = &base[..base.len() - p.len()].trim_end();
                            resolved = format!("{prefix} {node}{suffix}");
                            break;
                        }
                    }
                }
            } else if let Some(ref community) = self.last_community {
                if q_lower.contains("those") || q_lower.contains("they") {
                    resolved = resolved
                        .replace("those", community)
                        .replace("they", community);
                }
            }
        }

        resolved
    }

    /// Query history (most recent last).
    pub fn history(&self) -> impl Iterator<Item = &String> {
        self.history.iter()
    }

    /// Currently focused node names.
    pub fn focused_nodes(&self) -> &[String] {
        &self.focused_nodes
    }
}

fn extract_symbol_from_line(line: &str) -> Option<String> {
    // Lines like "- main" or "1. AuthenticationService"
    let trimmed = line
        .trim_start_matches(|c: char| c == '-' || c == '.' || c.is_numeric() || c.is_whitespace());
    let name = trimmed.split_whitespace().next()?;
    if name.len() > 1 {
        Some(name.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_context() {
        let mut ctx = ConversationContext::new();
        ctx.add_query("How many services?");
        ctx.add_focused_node("AuthenticationService");

        let resolved = ctx.resolve_references("What's its complexity?");
        assert!(resolved.contains("AuthenticationService"));
    }

    #[test]
    fn test_pronoun_it() {
        let mut ctx = ConversationContext::new();
        ctx.add_focused_node("verify_token");
        let resolved = ctx.resolve_references("Who calls it?");
        assert!(resolved.contains("verify_token"));
    }
}
