//! Intent classification for natural language queries
//!
//! Task 2.3.1: Classify user questions into intent categories.

/// Query intent categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intent {
    /// "How many X?"
    Count,
    /// "Show me all X"
    List,
    /// "Find X"
    Find,
    /// "What breaks if I change X?"
    Impact,
    /// Complexity-related queries
    Complexity,
    /// Dependency queries
    Dependencies,
    /// "What calls X?"
    Callers,
    /// "Top/most/highest X"
    Compare,
    /// Config-related queries
    Config,
    /// Circular dependency queries
    CircularDeps,
}

/// Classification result with confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentResult {
    /// Detected intent
    pub intent: Intent,
    /// Confidence score 0.0-1.0
    pub confidence: f64,
}

/// Keyword-based intent classifier.
pub struct IntentClassifier;

impl IntentClassifier {
    /// Create a new intent classifier.
    pub fn new() -> Self {
        Self
    }

    /// Classify a natural language question.
    pub fn classify(&self, question: &str) -> IntentResult {
        let q = question.to_lowercase();

        if q.contains("circular") || q.contains("cycle") {
            return IntentResult {
                intent: Intent::CircularDeps,
                confidence: 0.95,
            };
        }
        if q.contains("what breaks") || q.contains("what would break") || q.contains("impact of") {
            return IntentResult {
                intent: Intent::Impact,
                confidence: 0.95,
            };
        }
        if q.contains("what calls") || q.contains("who calls") || q.contains("callers of") {
            return IntentResult {
                intent: Intent::Callers,
                confidence: 0.95,
            };
        }
        if q.contains("depend") || q.contains("dependency") || q.contains("dependencies") {
            return IntentResult {
                intent: Intent::Dependencies,
                confidence: 0.9,
            };
        }
        if q.contains("complexity") || q.contains("complex") || q.contains("cyclomatic") {
            return IntentResult {
                intent: Intent::Complexity,
                confidence: 0.9,
            };
        }
        if q.contains("config") || q.contains("unused") || q.contains("environment") {
            return IntentResult {
                intent: Intent::Config,
                confidence: 0.85,
            };
        }
        if q.starts_with("how many")
            || q.starts_with("count ")
            || q.contains("number of")
            || q.contains("total ")
        {
            return IntentResult {
                intent: Intent::Count,
                confidence: 0.95,
            };
        }
        if q.contains("most ")
            || q.contains("highest")
            || q.contains("top ")
            || q.contains("hotspot")
        {
            return IntentResult {
                intent: Intent::Compare,
                confidence: 0.9,
            };
        }
        if q.starts_with("list ")
            || q.starts_with("show me")
            || q.starts_with("show all")
            || q.starts_with("give me")
            || q.starts_with("get all")
        {
            return IntentResult {
                intent: Intent::List,
                confidence: 0.9,
            };
        }
        if q.starts_with("find ") || q.contains("with complexity") || q.contains("where ") {
            return IntentResult {
                intent: Intent::Find,
                confidence: 0.85,
            };
        }

        IntentResult {
            intent: Intent::Find,
            confidence: 0.5,
        }
    }
}

impl Default for IntentClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_classification() {
        let classifier = IntentClassifier::new();
        assert_eq!(
            classifier.classify("how many functions?").intent,
            Intent::Count
        );
        assert_eq!(
            classifier.classify("show me all services").intent,
            Intent::List
        );
        assert_eq!(
            classifier.classify("what breaks if I change X?").intent,
            Intent::Impact
        );
        assert_eq!(
            classifier.classify("find high complexity code").intent,
            Intent::Complexity
        );
    }

    #[test]
    fn test_intent_variations() {
        let classifier = IntentClassifier::new();
        assert_eq!(classifier.classify("how many X").intent, Intent::Count);
        assert_eq!(classifier.classify("count X").intent, Intent::Count);
        assert_eq!(classifier.classify("number of X").intent, Intent::Count);
    }
}
