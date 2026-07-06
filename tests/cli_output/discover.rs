use rbuilder::cli::discover_output::{
    build_discover_response, fixture_discover_json, DISCOVER_SCHEMA_VERSION,
};
use rbuilder::pipeline::PipelineStats;

#[test]
fn test_discover_json_schema_sanity() {
    let doc = fixture_discover_json();

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(DISCOVER_SCHEMA_VERSION as u64)
    );
    assert_eq!(
        doc.get("command").and_then(|v| v.as_str()),
        Some("discover")
    );

    let metrics = doc.get("metrics").unwrap().as_object().unwrap();
    for key in [
        "files_discovered",
        "files_indexed",
        "files_skipped",
        "nodes_generated",
        "edges_generated",
        "duration_ms",
    ] {
        assert!(
            metrics.contains_key(key),
            "discover metrics missing '{key}'"
        );
    }
}

#[test]
fn test_discover_build_maps_pipeline_stats() {
    let stats = PipelineStats {
        files_discovered: 100,
        files_processed: 95,
        files_failed: 5,
        nodes_created: 200,
        edges_created: 400,
        duration: std::time::Duration::from_millis(500),
    };
    let response = build_discover_response(&stats, 750);
    assert_eq!(response.metrics.files_discovered, 100);
    assert_eq!(response.metrics.files_indexed, 95);
    assert_eq!(response.metrics.files_skipped, 5);
    assert_eq!(response.metrics.nodes_generated, 200);
    assert_eq!(response.metrics.edges_generated, 400);
    assert_eq!(response.metrics.duration_ms, 750);
}
