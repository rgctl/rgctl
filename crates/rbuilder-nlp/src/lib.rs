//! Natural language query processing

pub mod conversation;
#[cfg(feature = "nlp-patterns")]
pub mod dual_agent;
pub mod entity_extraction;
pub mod intent;
pub mod pattern_detection;
pub mod pattern_matcher;
pub mod query_cache;
#[cfg(feature = "nlp-patterns")]
pub mod query_examples;
pub mod templates;

pub use conversation::ConversationContext;
#[cfg(feature = "nlp-patterns")]
pub use dual_agent::{
    DualAgentQuerySystem, DualAgentResult, DualTranslationMethod, PrimaryAgent, QueryContext,
    SubQuery, TranslationAgent,
};
pub use entity_extraction::{EntityExtractor, ExtractedEntities, MetricFilter};
pub use intent::{Intent, IntentClassifier, IntentResult};
pub use pattern_detection::{DomainContext, LabelPattern, NamingPattern, PatternDetector};
pub use pattern_matcher::{PatternMatcher, QueryResult, TranslatedQuery, TranslationMethod};
pub use query_cache::{CacheHit, CachedQuery, QueryCache};
#[cfg(feature = "nlp-patterns")]
pub use query_examples::{default_examples, example_count, QueryExample};
pub use templates::{MatchedTemplate, QueryTemplates};
