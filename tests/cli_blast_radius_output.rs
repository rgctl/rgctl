//! Blast-radius CLI JSON output schema sanity tests.

use rbuilder::cli::blast_radius_output::{
    fixture_response, response_to_json, skipped_gatekeeping, BLAST_RADIUS_SCHEMA_VERSION,
};

#[test]
fn test_blast_radius_json_schema_sanity() {
    let response = fixture_response();
    let doc = response_to_json(&response);

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(BLAST_RADIUS_SCHEMA_VERSION as u64)
    );

    for key in ["target", "metrics", "topology", "gatekeeping"] {
        assert!(
            doc.get(key).is_some(),
            "missing top-level key '{key}' in blast-radius JSON"
        );
    }

    let target = doc.get("target").expect("target");
    for key in ["id", "symbol", "class_context", "file_path"] {
        assert!(target.get(key).is_some(), "target missing '{key}'");
    }

    let metrics = doc.get("metrics").expect("metrics");
    for key in [
        "score",
        "direct_callers_count",
        "impact_zone_size",
    ] {
        assert!(metrics.get(key).is_some(), "metrics missing '{key}'");
    }

    let topology = doc.get("topology").expect("topology");
    for key in ["scc_component_id", "direct_callers", "impact_zone"] {
        assert!(topology.get(key).is_some(), "topology missing '{key}'");
    }

    let gatekeeping = doc.get("gatekeeping").expect("gatekeeping");
    assert_eq!(
        gatekeeping.get("policy_status").and_then(|v| v.as_str()),
        Some("SKIPPED")
    );
    assert!(
        gatekeeping.get("violations").and_then(|v| v.as_array()).is_some(),
        "gatekeeping.violations must be an array"
    );
    let handoffs = gatekeeping
        .get("handoffs")
        .and_then(|v| v.as_array())
        .expect("gatekeeping.handoffs must be present");
    assert!(
        handoffs.is_empty(),
        "handoffs must be [] when --with-slices is omitted"
    );
}

#[test]
fn test_blast_radius_symbol_context_shape() {
    let response = fixture_response();
    let doc = response_to_json(&response);
    let caller = doc["topology"]["direct_callers"]
        .as_array()
        .expect("direct_callers array")[0]
        .as_object()
        .expect("caller object");

    for key in ["id", "fqn", "file_path"] {
        assert!(caller.contains_key(key), "SymbolContext missing '{key}'");
    }
}

#[test]
fn test_skipped_gatekeeping_always_has_empty_handoffs() {
    let gate = skipped_gatekeeping();
    assert_eq!(gate.policy_status, "SKIPPED");
    assert!(gate.handoffs.is_empty());
    assert!(gate.violations.is_empty());
}
