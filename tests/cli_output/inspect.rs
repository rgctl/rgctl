use rbuilder::cli::inspect_output::{fixture_inspect_cfg_response, INSPECT_SCHEMA_VERSION};

#[test]
fn test_inspect_cfg_json_schema_sanity() {
    let doc = serde_json::to_value(fixture_inspect_cfg_response()).unwrap();
    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(INSPECT_SCHEMA_VERSION as u64)
    );
    for key in ["symbol", "layer", "nodes", "edges"] {
        assert!(doc.get(key).is_some(), "inspect cfg missing '{key}'");
    }
}

#[test]
fn test_inspect_cfg_block_has_index() {
    let doc = serde_json::to_value(fixture_inspect_cfg_response()).unwrap();
    let node = doc["nodes"][0].as_object().unwrap();
    assert!(node.contains_key("block_index"));
    assert!(node.contains_key("start_line"));
}
