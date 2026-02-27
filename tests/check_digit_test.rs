use actix_web::{App, test, web};
use ruc_finder::handlers;

/// Helper: build an app with all check-digit/validate routes registered.
fn build_app() -> App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    App::new()
        .route("/api/v1/ruc/{ruc}/dv", web::get().to(handlers::compute_check_digit))
        .route("/api/v1/ruc/{ruc_dv}/validate", web::get().to(handlers::validate_ruc))
        .route("/api/v1/ruc/{ruc}/validate/{dv}", web::get().to(handlers::validate_ruc_split))
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc/{ruc}/dv — compute check digit
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn compute_dv_skywalker() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get().uri("/api/v1/ruc/1000100/dv").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["ruc"], "1000100");
    assert_eq!(body["check_digit"], 0);
    assert_eq!(body["full"], "1000100-0");
}

#[actix_web::test]
async fn compute_dv_organa() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get().uri("/api/v1/ruc/1000200/dv").to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["check_digit"], 6);
    assert_eq!(body["full"], "1000200-6");
}

#[actix_web::test]
async fn compute_dv_juridica_alianza() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get().uri("/api/v1/ruc/80001001/dv").to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["check_digit"], 9);
    assert_eq!(body["full"], "80001001-9");
}

#[actix_web::test]
async fn compute_dv_non_alphanumeric_returns_400() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get().uri("/api/v1/ruc/12%2D34/dv").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc/{ruc}/validate/{dv} — split path segments
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn validate_split_valid_pair() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100/validate/0")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["ruc"], "1000100");
    assert_eq!(body["check_digit"], "0");
    assert_eq!(body["valid"], true);
}

#[actix_web::test]
async fn validate_split_invalid_pair() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100/validate/5")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], false);
}

#[actix_web::test]
async fn validate_split_vader_cancelado() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/2000100/validate/2")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], true);
}

#[actix_web::test]
async fn validate_split_juridica_imperio() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/80002001/validate/4")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], true);
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc/{ruc-dv}/validate — hyphen-separated format
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn validate_hyphen_valid_pair() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100-0/validate")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["ruc"], "1000100");
    assert_eq!(body["check_digit"], "0");
    assert_eq!(body["valid"], true);
}

#[actix_web::test]
async fn validate_hyphen_invalid_pair() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100-9/validate")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], false);
}

#[actix_web::test]
async fn validate_hyphen_juridica() {
    let app = test::init_service(build_app()).await;
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/80001001-9/validate")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], true);
}

#[actix_web::test]
async fn validate_hyphen_missing_dv_returns_400() {
    let app = test::init_service(build_app()).await;
    // No hyphen → 400 with helpful message
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100/validate")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn validate_hyphen_empty_dv_returns_400() {
    let app = test::init_service(build_app()).await;
    // Trailing hyphen with no DV → 400
    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/1000100-/validate")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Batch: all 15 dataset entries via both formats
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn validate_all_dataset_entries_both_formats() {
    let app = test::init_service(build_app()).await;

    let entries = [
        ("1000100", "0"), ("1000200", "6"), ("1000300", "2"),
        ("1000400", "9"), ("1000500", "5"), ("2000100", "2"),
        ("2000200", "9"), ("2000300", "5"), ("2000400", "1"),
        ("2000500", "8"), ("80001001", "9"), ("80002001", "4"),
        ("80003001", "0"), ("80004001", "5"), ("80005001", "0"),
    ];

    for (ruc, dv) in entries {
        // compute endpoint
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/ruc/{ruc}/dv"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "compute_dv failed for {ruc}");
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(
            body["check_digit"].as_u64().unwrap().to_string(),
            dv,
            "Wrong DV for {ruc}: expected {dv}"
        );

        // validate via split path: /ruc/{ruc}/validate/{dv}
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/ruc/{ruc}/validate/{dv}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "validate_split failed for {ruc}-{dv}");
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["valid"], true, "Expected {ruc}/{dv} to be valid (split)");

        // validate via hyphen: /ruc/{ruc}-{dv}/validate
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/ruc/{ruc}-{dv}/validate"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "validate_hyphen failed for {ruc}-{dv}");
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["valid"], true, "Expected {ruc}-{dv} to be valid (hyphen)");
    }
}
