use rbuilder::cli::metrics_output::{
    build_metrics_response, fixture_metrics_json, metrics_response_to_json,
    MetricsPagerankSection, METRICS_SCHEMA_VERSION,
};
use serde_json::json;

#[test]
fn test_metrics_json_schema_sanity() {
    let doc = fixture_metrics_json();

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(METRICS_SCHEMA_VERSION as u64)
    );

    for section in ["pagerank", "betweenness", "communities"] {
        assert!(doc.get(section).is_some(), "metrics missing section '{section}'");
    }

    let pr = doc.get("pagerank").unwrap().as_object().unwrap();
    for key in ["top", "converged", "iterations", "max_delta"] {
        assert!(pr.contains_key(key), "pagerank missing '{key}'");
    }
    assert!(pr.get("top").unwrap().is_array());

    assert!(doc.get("betweenness").unwrap().is_array());

    let cm = doc.get("communities").unwrap().as_object().unwrap();
    for key in ["count", "modularity", "assignments"] {
        assert!(cm.contains_key(key), "communities missing '{key}'");
    }
}

#[test]
fn test_metrics_wrap_adds_schema_version() {
    use rbuilder::cli::metrics_output::wrap_metrics_payload;

    let mut payload = json!({ "pagerank": { "top": [] } });
    wrap_metrics_payload(&mut payload);
    assert_eq!(
        payload.get("schema_version").and_then(|v| v.as_u64()),
        Some(METRICS_SCHEMA_VERSION as u64)
    );
}

#[test]
fn test_metrics_pagerank_only_omits_other_sections() {
    let response = build_metrics_response(
        Some(MetricsPagerankSection {
            top: vec![json!({ "node": "00000000-0000-0000-0000-000000000001", "pagerank": 0.1 })],
            converged: true,
            iterations: 20,
            max_delta: 1e-9,
        }),
        None,
        None,
    );

    let doc = metrics_response_to_json(&response);
    assert!(doc.get("pagerank").is_some());
    assert!(doc.get("betweenness").is_none(), "betweenness must be absent, not null/[]");
    assert!(doc.get("communities").is_none(), "communities must be absent, not null/[]");

    let json_str = serde_json::to_string(&response).expect("metrics serializes");
    assert!(!json_str.contains("betweenness"));
    assert!(!json_str.contains("communities"));
}
