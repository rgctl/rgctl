use rbuilder::cli::gql_output::{fixture_gql_json, GQL_SCHEMA_VERSION};

#[test]
fn test_gql_json_schema_sanity() {
    let doc = fixture_gql_json();

    assert_eq!(
        doc.get("schema_version").and_then(|v| v.as_u64()),
        Some(GQL_SCHEMA_VERSION as u64)
    );

    for key in ["rows", "count", "explain"] {
        assert!(doc.get(key).is_some(), "gql JSON missing '{key}'");
    }

    assert_eq!(doc.get("count").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(doc.get("explain").and_then(|v| v.as_bool()), Some(false));

    let rows = doc.get("rows").and_then(|v| v.as_array()).expect("rows array");
    let binding = rows[0][0].as_object().expect("row binding");
    for key in ["binding", "node", "type", "file"] {
        assert!(binding.contains_key(key), "gql row missing '{key}'");
    }
}

#[test]
fn test_gql_empty_rows_explicit_array() {
    use rbuilder::cli::gql_output::gql_response_from_result;
    use rbuilder_gql::QueryResult;

    let response = gql_response_from_result(&QueryResult { rows: vec![], plan: None }, false);
    assert_eq!(response.count, 0);
    assert!(response.rows.is_empty());
    let doc = serde_json::to_value(&response).unwrap();
    assert!(doc.get("rows").and_then(|v| v.as_array()).unwrap().is_empty());
}
