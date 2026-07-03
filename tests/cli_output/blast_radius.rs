use rbuilder::cli::blast_radius_output::{
    fixture_response, response_to_json, skipped_gatekeeping, BLAST_RADIUS_SCHEMA_VERSION,
};

#[test]
fn test_blast_radius_json_schema_sanity() {
    let doc = response_to_json(&fixture_response());

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(BLAST_RADIUS_SCHEMA_VERSION as u64)
    );

    for key in ["target", "metrics", "topology", "gatekeeping"] {
        assert!(doc.get(key).is_some(), "missing top-level key '{key}'");
    }

    let gatekeeping = doc.get("gatekeeping").expect("gatekeeping");
    let handoffs = gatekeeping
        .get("handoffs")
        .and_then(|v| v.as_array())
        .expect("gatekeeping.handoffs must be present");
    assert!(handoffs.is_empty());
}

#[test]
fn test_blast_radius_symbol_context_shape() {
    let doc = response_to_json(&fixture_response());
    let caller = doc["topology"]["direct_callers"][0].as_object().unwrap();
    for key in ["id", "fqn", "file_path"] {
        assert!(caller.contains_key(key), "SymbolContext missing '{key}'");
    }
}

#[test]
fn test_skipped_gatekeeping_always_has_empty_handoffs() {
    let gate = skipped_gatekeeping();
    assert_eq!(gate.policy_status, "SKIPPED");
    assert!(gate.handoffs.is_empty());
}
