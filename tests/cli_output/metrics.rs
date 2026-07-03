use rbuilder::cli::metrics_output::{fixture_metrics_json, METRICS_SCHEMA_VERSION};

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
    use serde_json::json;

    let mut payload = json!({ "pagerank": { "top": [] } });
    wrap_metrics_payload(&mut payload);
    assert_eq!(
        payload.get("schema_version").and_then(|v| v.as_u64()),
        Some(METRICS_SCHEMA_VERSION as u64)
    );
}
