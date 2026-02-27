use actix_web::ResponseError;
use ruc_finder::errors::AppError;

// ---------------------------------------------------------------------------
// AppError Display formatting
// ---------------------------------------------------------------------------

#[test]
fn app_error_display_not_found() {
    let err = AppError::NotFound("RUC 9999999 not found".to_string());
    assert_eq!(err.to_string(), "Not found: RUC 9999999 not found");
}

#[test]
fn app_error_display_forbidden() {
    let err = AppError::Forbidden("restricted".to_string());
    assert_eq!(err.to_string(), "Forbidden: restricted");
}

#[test]
fn app_error_display_internal() {
    let err = AppError::Internal("something broke".to_string());
    assert_eq!(err.to_string(), "Internal error: something broke");
}

// ---------------------------------------------------------------------------
// AppError HTTP status codes
// ---------------------------------------------------------------------------

#[test]
fn app_error_not_found_returns_404() {
    let err = AppError::NotFound("gone".to_string());
    let resp = err.error_response();
    assert_eq!(resp.status(), 404);
}

#[test]
fn app_error_forbidden_returns_403() {
    let err = AppError::Forbidden("nope".to_string());
    let resp = err.error_response();
    assert_eq!(resp.status(), 403);
}

#[test]
fn app_error_internal_returns_500() {
    let err = AppError::Internal("boom".to_string());
    let resp = err.error_response();
    assert_eq!(resp.status(), 500);
}

// ---------------------------------------------------------------------------
// AppError JSON body structure
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn app_error_not_found_body_has_error_field() {
    let err = AppError::NotFound("RUC 123 not found".to_string());
    let resp = err.error_response();
    let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "RUC 123 not found");
}

#[actix_web::test]
async fn app_error_forbidden_body_has_error_field() {
    let err = AppError::Forbidden("denied".to_string());
    let resp = err.error_response();
    let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "denied");
}

#[actix_web::test]
async fn app_error_internal_body_has_error_field() {
    let err = AppError::Internal("crash".to_string());
    let resp = err.error_response();
    let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("crash"));
}

// ---------------------------------------------------------------------------
// AppError From<sqlx::Error>
// ---------------------------------------------------------------------------

#[test]
fn app_error_from_sqlx_error() {
    let sqlx_err = sqlx::Error::RowNotFound;
    let app_err = AppError::from(sqlx_err);
    let msg = app_err.to_string();
    assert!(msg.starts_with("Database error:"));
}

#[test]
fn app_error_db_variant_returns_500() {
    let sqlx_err = sqlx::Error::RowNotFound;
    let app_err = AppError::from(sqlx_err);
    let resp = app_err.error_response();
    assert_eq!(resp.status(), 500);
}
