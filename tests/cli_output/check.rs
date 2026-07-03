use rbuilder::cli::check_output::{fixture_check_json, CHECK_SCHEMA_VERSION};

#[test]
fn test_check_json_schema_sanity() {
    let doc = fixture_check_json();

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(CHECK_SCHEMA_VERSION as u64)
    );

    for key in ["policy", "violations", "passed"] {
        assert!(doc.get(key).is_some(), "check JSON missing '{key}'");
    }

    assert_eq!(doc.get("passed").and_then(|v| v.as_bool()), Some(true));
    let violations = doc
        .get("violations")
        .and_then(|v| v.as_array())
        .expect("violations must be an array");
    assert!(violations.is_empty());
}

#[test]
fn test_check_violations_always_array_when_passing() {
    use rbuilder::cli::check_output::build_check_response;

    let response = build_check_response("policy.json", vec![]);
    let doc = serde_json::to_value(&response).unwrap();
    assert!(doc["violations"].is_array());
    assert!(doc["passed"].as_bool().unwrap());
}
