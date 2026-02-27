use std::sync::{Arc, Once};

use actix_web::{App, web, test};
use sqlx::PgPool;

use ruc_finder::config::AppConfig;
use ruc_finder::handlers;

static INIT: Once = Once::new();

/// Load .env.test once for all tests in this file.
fn load_env() {
    INIT.call_once(|| {
        dotenvy::from_filename(".env.test").ok();
    });
}

/// Helper: creates a test PgPool from DATABASE_URL env var.
/// Returns None if DATABASE_URL is not set (tests skip gracefully).
async fn test_pool() -> Option<PgPool> {
    load_env();
    let url = std::env::var("DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .ok()
}

fn test_config() -> Arc<AppConfig> {
    load_env();
    Arc::new(AppConfig::load(Some("/tmp/nonexistent_ruc_finder_test.conf")))
}

/// Skip the test if no database connection is available.
/// Prints a visible warning so skipped tests don't go unnoticed.
macro_rules! require_db {
    () => {
        match test_pool().await {
            Some(pool) => pool,
            None => {
                eprintln!(
                    "\n  ⚠ SKIPPED: {} — DATABASE_URL not set or DB unreachable.\n    \
                     Configure .env.test with a valid DATABASE_URL to run API tests.\n",
                    module_path!()
                );
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn health_check_returns_200() {
    let pool = require_db!();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .route("/api/v1/health", web::get().to(handlers::health_check)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "ok");
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc/{ruc} — not found
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn get_ruc_nonexistent_returns_404() {
    let pool = require_db!();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .route("/api/v1/ruc/{ruc}", web::get().to(handlers::get_ruc_by_number)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/9999999999")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc — search with no filters
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn search_ruc_no_filters_returns_200() {
    let pool = require_db!();
    let config = test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(config))
            .route("/api/v1/ruc", web::get().to(handlers::search_ruc)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/v1/ruc").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    assert!(body["page"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["total"].is_number());
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc — search with status filter
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn search_ruc_with_status_returns_200() {
    let pool = require_db!();
    let config = test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(config))
            .route("/api/v1/ruc", web::get().to(handlers::search_ruc)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/ruc?status=ACTIVO&limit=5")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

// ---------------------------------------------------------------------------
// GET /api/v1/ruc/search — fuzzy search
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn fuzzy_search_returns_200() {
    let pool = require_db!();
    let config = test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(config))
            .route("/api/v1/ruc/search", web::get().to(handlers::fuzzy_search_ruc)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/ruc/search?query=GONZALEZ&limit=5")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
}

// ---------------------------------------------------------------------------
// POST /api/v1/sync — force param
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn trigger_sync_force_returns_202() {
    let pool = require_db!();
    let config = test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(config))
            .route("/api/v1/sync", web::post().to(handlers::trigger_sync)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/sync?force=true")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // 202 Accepted (force bypasses rate limit)
    assert_eq!(resp.status(), 202);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["message"].as_str().unwrap().contains("sync"));
}

// ---------------------------------------------------------------------------
// POST /api/v1/sync — no force, may get 429 or 202
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn trigger_sync_without_force_respects_rate_limit() {
    let pool = require_db!();
    let config = test_config();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(config))
            .route("/api/v1/sync", web::post().to(handlers::trigger_sync)),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/api/v1/sync")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Either 202 (no previous sync) or 429 (rate limited)
    let status = resp.status().as_u16();
    assert!(status == 202 || status == 429, "Expected 202 or 429, got {status}");
}
