use ruc_finder::models::{PaginatedResponse, SyncParams, compute_check_digit, validate_ruc};

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

// ---------------------------------------------------------------------------
// compute_check_digit — ruc0.txt (ACTIVO personas físicas)
// ---------------------------------------------------------------------------

#[test]
fn check_digit_skywalker_luke() {
    // 1000100|SKYWALKER, LUKE|0|...
    assert_eq!(compute_check_digit("1000100"), 0);
}

#[test]
fn check_digit_organa_leia() {
    // 1000200|ORGANA, LEIA|6|...
    assert_eq!(compute_check_digit("1000200"), 6);
}

#[test]
fn check_digit_solo_han() {
    // 1000300|SOLO, HAN|2|...
    assert_eq!(compute_check_digit("1000300"), 2);
}

#[test]
fn check_digit_kenobi_obi_wan() {
    // 1000400|KENOBI, OBI WAN|9|...
    assert_eq!(compute_check_digit("1000400"), 9);
}

#[test]
fn check_digit_calrissian_lando() {
    // 1000500|CALRISSIAN, LANDO|5|...
    assert_eq!(compute_check_digit("1000500"), 5);
}

// ---------------------------------------------------------------------------
// compute_check_digit — ruc1.txt (mixed statuses)
// ---------------------------------------------------------------------------

#[test]
fn check_digit_vader_darth() {
    // 2000100|VADER, DARTH|2|...
    assert_eq!(compute_check_digit("2000100"), 2);
}

#[test]
fn check_digit_palpatine_sheev() {
    // 2000200|PALPATINE, SHEEV|9|...
    assert_eq!(compute_check_digit("2000200"), 9);
}

#[test]
fn check_digit_maul_darth() {
    // 2000300|MAUL, DARTH|5|...
    assert_eq!(compute_check_digit("2000300"), 5);
}

#[test]
fn check_digit_dooku_conde() {
    // 2000400|DOOKU, CONDE|1|...
    assert_eq!(compute_check_digit("2000400"), 1);
}

#[test]
fn check_digit_fett_boba() {
    // 2000500|FETT, BOBA|8|...
    assert_eq!(compute_check_digit("2000500"), 8);
}

// ---------------------------------------------------------------------------
// compute_check_digit — ruc2.txt (jurídicas)
// ---------------------------------------------------------------------------

#[test]
fn check_digit_alianza_rebelde() {
    // 80001001|ALIANZA REBELDE S.A.|9|...
    assert_eq!(compute_check_digit("80001001"), 9);
}

#[test]
fn check_digit_imperio_galactico() {
    // 80002001|IMPERIO GALACTICO S.R.L.|4|...
    assert_eq!(compute_check_digit("80002001"), 4);
}

#[test]
fn check_digit_orden_jedi() {
    // 80003001|ORDEN JEDI LTDA.|0|...
    assert_eq!(compute_check_digit("80003001"), 0);
}

#[test]
fn check_digit_federacion_comercio() {
    // 80004001|FEDERACION DE COMERCIO S.A.|5|...
    assert_eq!(compute_check_digit("80004001"), 5);
}

#[test]
fn check_digit_mandalorian_security() {
    // 80005001|MANDALORIAN SECURITY S.A.|0|...
    assert_eq!(compute_check_digit("80005001"), 0);
}

// ---------------------------------------------------------------------------
// validate_ruc — valid pairs from all datasets
// ---------------------------------------------------------------------------

#[test]
fn validate_ruc_valid_pairs() {
    // All 15 entries from the Star Wars test datasets
    let valid_pairs = [
        ("1000100", "0"), ("1000200", "6"), ("1000300", "2"),
        ("1000400", "9"), ("1000500", "5"), ("2000100", "2"),
        ("2000200", "9"), ("2000300", "5"), ("2000400", "1"),
        ("2000500", "8"), ("80001001", "9"), ("80002001", "4"),
        ("80003001", "0"), ("80004001", "5"), ("80005001", "0"),
    ];
    for (ruc, dv) in valid_pairs {
        assert!(validate_ruc(ruc, dv), "Expected {ruc}-{dv} to be valid");
    }
}

#[test]
fn validate_ruc_invalid_check_digit() {
    // Correct DV for 1000100 is 0, so 1 should fail
    assert!(!validate_ruc("1000100", "1"));
    assert!(!validate_ruc("1000100", "9"));
    assert!(!validate_ruc("80001001", "3"));
}

#[test]
fn validate_ruc_non_numeric_dv() {
    assert!(!validate_ruc("1000100", "X"));
    assert!(!validate_ruc("1000100", ""));
    assert!(!validate_ruc("1000100", "abc"));
}

#[test]
fn validate_ruc_whitespace_dv_trimmed() {
    // validate_ruc trims whitespace before parsing
    assert!(validate_ruc("1000100", " 0 "));
    assert!(validate_ruc("2000100", "  2\t"));
}
