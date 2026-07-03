use rbuilder::cli::slice_output::{fixture_cfg_response, SLICE_SCHEMA_VERSION};

#[test]
fn test_slice_cfg_json_schema_sanity() {
    let doc = serde_json::to_value(fixture_cfg_response()).unwrap();
    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(SLICE_SCHEMA_VERSION as u64)
    );
    for key in ["file", "function", "view", "nodes", "edges"] {
        assert!(doc.get(key).is_some(), "slice cfg missing '{key}'");
    }
    assert_eq!(doc.get("view").and_then(|v| v.as_str()), Some("cfg"));
}

#[test]
fn test_slice_cfg_topology_not_counts() {
    let doc = serde_json::to_value(fixture_cfg_response()).unwrap();
    assert!(doc.get("nodes").unwrap().is_array());
    assert!(doc.get("edges").unwrap().is_array());
    assert!(doc.get("blocks").is_none());
}
