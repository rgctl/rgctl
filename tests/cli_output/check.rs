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

#[test]
fn test_check_passed_false_contract() {
    use rbuilder::cli::check_output::{build_check_response, CheckViolationEntry};

    let response = build_check_response(
        "policy.json",
        vec![CheckViolationEntry {
            symbol: "foo".into(),
            error: None,
            violation: Some("scale failure".into()),
        }],
    );
    assert!(!response.passed);
    let doc = serde_json::to_value(&response).unwrap();
    assert_eq!(doc["passed"].as_bool(), Some(false));
    assert!(!doc["violations"].as_array().unwrap().is_empty());
    // Subprocess exit 1 when !passed: check_policy_violation_fails_closed_with_exit_one
    // (fixture `publishEvent` has upstream caller `checkout`).
}
