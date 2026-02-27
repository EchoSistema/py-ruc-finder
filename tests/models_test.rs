use ruc_finder::models::{PaginatedResponse, SyncParams};

// ---------------------------------------------------------------------------
// PaginatedResponse serialization
// ---------------------------------------------------------------------------

#[test]
fn paginated_response_serializes_correctly() {
    let resp = PaginatedResponse {
        data: vec!["item1", "item2"],
        page: 1,
        limit: 25,
        total: 100,
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["page"], 1);
    assert_eq!(json["limit"], 25);
    assert_eq!(json["total"], 100);
    assert_eq!(json["data"].as_array().unwrap().len(), 2);
}

#[test]
fn paginated_response_empty_data() {
    let resp: PaginatedResponse<String> = PaginatedResponse {
        data: vec![],
        page: 1,
        limit: 25,
        total: 0,
    };
    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["total"], 0);
    assert!(json["data"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// SyncParams deserialization
// ---------------------------------------------------------------------------

#[test]
fn sync_params_force_true() {
    let json = r#"{"force": true}"#;
    let params: SyncParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.force, Some(true));
}

#[test]
fn sync_params_force_false() {
    let json = r#"{"force": false}"#;
    let params: SyncParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.force, Some(false));
}

#[test]
fn sync_params_force_absent() {
    let json = r#"{}"#;
    let params: SyncParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.force, None);
}

#[test]
fn sync_params_force_default_is_false() {
    let params: SyncParams = serde_json::from_str("{}").unwrap();
    assert_eq!(params.force.unwrap_or(false), false);
}

// ---------------------------------------------------------------------------
// RucSearchParams deserialization
// ---------------------------------------------------------------------------

#[test]
fn ruc_search_params_all_optional() {
    let json = r#"{}"#;
    let params: ruc_finder::models::RucSearchParams = serde_json::from_str(json).unwrap();
    assert!(params.ruc.is_none());
    assert!(params.name.is_none());
    assert!(params.status.is_none());
    assert!(params.page.is_none());
    assert!(params.limit.is_none());
}

#[test]
fn ruc_search_params_with_values() {
    let json = r#"{"ruc": "1000000", "status": "ACTIVO", "page": 2, "limit": 50}"#;
    let params: ruc_finder::models::RucSearchParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.ruc.as_deref(), Some("1000000"));
    assert_eq!(params.status.as_deref(), Some("ACTIVO"));
    assert_eq!(params.page, Some(2));
    assert_eq!(params.limit, Some(50));
}

// ---------------------------------------------------------------------------
// FuzzySearchParams deserialization
// ---------------------------------------------------------------------------

#[test]
fn fuzzy_search_params_requires_query() {
    let json = r#"{}"#;
    let result: Result<ruc_finder::models::FuzzySearchParams, _> = serde_json::from_str(json);
    assert!(result.is_err()); // query is required
}

#[test]
fn fuzzy_search_params_with_all_fields() {
    let json = r#"{"query": "JUAN CARLOS", "status": "ACTIVO", "threshold": 0.5, "page": 1, "limit": 10}"#;
    let params: ruc_finder::models::FuzzySearchParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.query, "JUAN CARLOS");
    assert_eq!(params.status.as_deref(), Some("ACTIVO"));
    assert!((params.threshold.unwrap() - 0.5).abs() < f64::EPSILON);
    assert_eq!(params.page, Some(1));
    assert_eq!(params.limit, Some(10));
}
