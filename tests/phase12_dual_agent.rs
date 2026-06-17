//! Phase 12.3 — dual-agent query translation accuracy tests

use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use rbuilder::graph::schema::{Node, NodeType};
use rbuilder::nlp::{
    default_examples, example_count, DualAgentQuerySystem, DualTranslationMethod,
    TranslationAgent,
};

#[test]
fn test_example_library_has_twenty_plus_pairs() {
    assert!(example_count() >= 20, "expected >= 20 examples, got {}", example_count());
    assert!(!default_examples().is_empty());
}

#[test]
fn test_translation_callers_accuracy() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, score) = agent.translate("what calls authenticate").unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(score >= 0.75, "score={score}");
    assert!(pattern.contains("callers"));
    assert!(pattern.contains("authenticate"));
}

#[test]
fn test_translation_impact_accuracy() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, _) = agent
        .translate("what breaks if I change verify_token")
        .unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(pattern.contains("impact"));
    assert!(pattern.contains("verify_token"));
}

#[test]
fn test_translation_complexity_accuracy() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, _) = agent.translate("high complexity functions").unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(pattern.contains("high_complexity") || pattern.contains("complexity"));
}

#[test]
fn test_translation_signature_filter() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, _) = agent.translate("async functions").unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(pattern.contains("signature:*async*"));
    assert!(pattern.contains("type:Function"));
}

#[test]
fn test_translation_return_type_filter() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, _) = agent.translate("functions returning Result").unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(pattern.contains("return_type:Result"));
}

#[test]
fn test_translation_compound_multi_hop() {
    let agent = TranslationAgent::new(0.70);
    let (pattern, method, _) = agent
        .translate("async functions returning Result")
        .unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert!(pattern.contains("signature:*async*"));
    assert!(pattern.contains("return_type:Result"));
    assert!(pattern.contains('|'));
}

#[test]
fn test_translation_type_filter_list_functions() {
    let agent = TranslationAgent::new(0.75);
    let (pattern, method, _) = agent.translate("list all functions").unwrap();
    assert_eq!(method, DualTranslationMethod::ExampleMatch);
    assert_eq!(pattern, "type:Function");
}

#[test]
fn test_dual_agent_executes_signature_query() {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(
            Node::new(NodeType::Function, "worker".to_string())
                .with_signature("async fn worker() -> Result<()>")
                .with_return_type("Result<()>"),
        )
        .unwrap();
    backend
        .insert_node(Node::new(NodeType::Function, "plain".to_string()))
        .unwrap();

    let system = DualAgentQuerySystem::new();
    let result = system
        .query("async functions returning Result", &backend)
        .unwrap();

    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].name, "worker");
    assert!(result.confidence > 0.5);
}

#[test]
fn test_dual_agent_pattern_matcher_fallback() {
    let agent = TranslationAgent::new(0.99); // high threshold forces fallback
    let (pattern, method, _) = agent.translate("how many functions?").unwrap();
    assert_eq!(method, DualTranslationMethod::PatternMatcherFallback);
    assert!(!pattern.is_empty());
}

#[test]
fn test_dual_agent_decomposition_records_sub_queries() {
    let mut backend = MemoryBackend::new();
    backend
        .insert_node(Node::new(NodeType::Function, "auth".to_string()))
        .unwrap();

    let system = DualAgentQuerySystem::new();
    let result = system
        .query("list all functions and show me all classes", &backend)
        .unwrap();

    assert!(result.context.sub_queries.len() >= 2);
    assert!(result.answer_lines.iter().any(|l| l.contains("Answer for:")));
}

#[cfg(feature = "nlp-llm")]
#[test]
fn test_llm_stub_requires_api_key() {
    use rbuilder::nlp::dual_agent::llm::LlmDualAgentQuerySystem;

    let backend = MemoryBackend::new();
    let system = LlmDualAgentQuerySystem::new();
    std::env::remove_var("RBUILDER_LLM_API_KEY");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(system.query_with_llm("test", &backend));
    assert!(err.is_err());
}
