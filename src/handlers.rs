use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use sqlx::PgPool;

use crate::config::AppConfig;
use crate::errors::{AppError, ErrorResponse};
use crate::models::{FuzzySearchParams, PaginatedResponse, Ruc, RucSearchParams, RucWithScore};
use crate::repository;
use crate::scraper;

/// Look up a RUC by its exact number.
///
/// Returns a single RUC record matching the given number. You can optionally
/// include the check digit separated by a hyphen (e.g. `1000000-3`).
/// If a check digit is provided, it is also matched; otherwise only the
/// RUC number is used for lookup.
#[utoipa::path(
    get,
    path = "/api/v1/ruc/{ruc}",
    tag = "RUC",
    summary = "Get RUC by number",
    description = "Look up a single RUC by its exact number. Optionally include the check digit separated by a hyphen (e.g. `1000000-3`). Returns 404 if not found.",
    params(
        ("ruc" = String, Path, description = "RUC number, optionally with check digit", example = "1000000-3")
    ),
    responses(
        (status = 200, description = "RUC found successfully", body = Ruc,
            example = json!({
                "id": 42,
                "ruc": "1000000",
                "first_names": "JUANA DEL CARMEN",
                "last_names": "CAÑETE GONZALEZ",
                "full_name": "JUANA DEL CARMEN CAÑETE GONZALEZ",
                "check_digit": "3",
                "old_ruc": "CAGJ761720E",
                "status": "ACTIVO",
                "created_at": "2026-02-01T00:00:00Z",
                "updated_at": "2026-02-01T00:00:00Z",
                "file_metadata_id": 1
            })
        ),
        (status = 404, description = "RUC not found in the database", body = ErrorResponse,
            example = json!({"error": "RUC 9999999 not found"})
        ),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_ruc_by_number(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let raw = path.into_inner();
    let (ruc_number, check_digit) = match raw.split_once('-') {
        Some((ruc, dv)) if !dv.is_empty() => (ruc, Some(dv)),
        Some((ruc, _)) => (ruc, None),
        None => (raw.as_str(), None),
    };
    match repository::find_ruc_by_number(pool.get_ref(), ruc_number, check_digit).await? {
        Some(ruc) => Ok(HttpResponse::Ok().json(ruc)),
        None => Err(AppError::NotFound(format!("RUC {raw} not found"))),
    }
}

/// Search RUCs with combinable filters.
///
/// All text fields use accent-insensitive, case-insensitive partial matching
/// (`unaccent() + ILIKE`). The `status` filter uses exact match, enabling
/// PostgreSQL partition pruning for better performance. All filters are
/// combined with AND logic. Results are paginated.
#[utoipa::path(
    get,
    path = "/api/v1/ruc",
    tag = "RUC",
    summary = "Search RUCs with filters",
    description = "Search RUC records using combinable filters. Text fields (name, first_names, last_names, full_name, ruc, old_ruc) use accent-insensitive, case-insensitive partial matching (`unaccent() + ILIKE`). The `status` field uses exact match for partition pruning. All filters are combined with AND logic. Results are paginated.",
    params(RucSearchParams),
    responses(
        (status = 200, description = "Paginated list of matching RUCs", body = inline(PaginatedResponse<Ruc>),
            example = json!({
                "data": [{
                    "id": 42,
                    "ruc": "1000000",
                    "first_names": "JUANA DEL CARMEN",
                    "last_names": "CAÑETE GONZALEZ",
                    "full_name": "JUANA DEL CARMEN CAÑETE GONZALEZ",
                    "check_digit": "3",
                    "old_ruc": "CAGJ761720E",
                    "status": "ACTIVO",
                    "created_at": "2026-02-01T00:00:00Z",
                    "updated_at": "2026-02-01T00:00:00Z",
                    "file_metadata_id": 1
                }],
                "page": 1,
                "limit": 25,
                "total": 1
            })
        ),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn search_ruc(
    pool: web::Data<PgPool>,
    config: web::Data<Arc<AppConfig>>,
    query: web::Query<RucSearchParams>,
) -> Result<HttpResponse, AppError> {
    let mut params = query.into_inner();

    // If the ruc param contains a hyphen, split into ruc number + check_digit
    // e.g. "80077182-6" → ruc="80077182", check_digit="6"
    // A trailing hyphen (e.g. "80077182-") is stripped without setting check_digit.
    let check_digit_filter = match params.ruc.take() {
        Some(ruc_val) => {
            if let Some((number, dv)) = ruc_val.split_once('-') {
                let dv = dv.trim();
                params.ruc = Some(number.to_string());
                if dv.is_empty() {
                    None
                } else {
                    Some(dv.to_string())
                }
            } else {
                params.ruc = Some(ruc_val);
                None
            }
        }
        None => None,
    };

    let (data, total) = repository::search_ruc(
        pool.get_ref(),
        &params,
        check_digit_filter.as_deref(),
        &config,
    )
    .await?;
    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(config.pagination_limit)
        .clamp(1, config.pagination_max);
    Ok(HttpResponse::Ok().json(PaginatedResponse {
        data,
        page,
        limit,
        total,
    }))
}

/// Fuzzy search RUCs by name similarity.
///
/// Uses PostgreSQL `pg_trgm` trigram similarity with `unaccent()` for
/// accent-insensitive fuzzy matching against `full_name`. Results are
/// ranked by similarity score (highest first). Useful for finding
/// taxpayers when the exact spelling is unknown.
#[utoipa::path(
    get,
    path = "/api/v1/ruc/search",
    tag = "RUC",
    summary = "Fuzzy search RUCs by name",
    description = "Fuzzy search using PostgreSQL `pg_trgm` trigram similarity with `unaccent()`. Matches the `query` against `full_name` and returns paginated results ranked by similarity score (highest first). Useful when the exact name spelling is unknown. The `status` filter enables partition pruning.",
    params(FuzzySearchParams),
    responses(
        (status = 200, description = "Paginated list of matching RUCs with similarity scores, ordered by relevance", body = inline(PaginatedResponse<RucWithScore>),
            example = json!({
                "data": [{
                    "id": 42,
                    "ruc": "1000000",
                    "first_names": "JUAN CARLOS",
                    "last_names": "LOPEZ MARTINEZ",
                    "full_name": "JUAN CARLOS LOPEZ MARTINEZ",
                    "check_digit": "7",
                    "old_ruc": "LMJC800101A",
                    "status": "ACTIVO",
                    "created_at": "2026-02-01T00:00:00Z",
                    "updated_at": "2026-02-01T00:00:00Z",
                    "file_metadata_id": 1,
                    "score": 0.72
                }],
                "page": 1,
                "limit": 25,
                "total": 1
            })
        ),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn fuzzy_search_ruc(
    pool: web::Data<PgPool>,
    config: web::Data<Arc<AppConfig>>,
    query: web::Query<FuzzySearchParams>,
) -> Result<HttpResponse, AppError> {
    let params = query.into_inner();
    let (data, total) = repository::fuzzy_search_ruc(pool.get_ref(), &params, &config).await?;
    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(config.fuzzy_limit)
        .clamp(1, config.fuzzy_max);
    Ok(HttpResponse::Ok().json(PaginatedResponse {
        data,
        page,
        limit,
        total,
    }))
}

/// Health check endpoint.
///
/// Verifies that the API server is running and the PostgreSQL database
/// connection is responsive by executing a simple `SELECT 1` query.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "System",
    summary = "Health check",
    description = "Verifies that the API server is running and the PostgreSQL database connection is responsive. Returns `{\"status\": \"ok\"}` on success, or 500 if the database is unreachable.",
    responses(
        (status = 200, description = "Service and database are healthy", body = serde_json::Value,
            example = json!({"status": "ok"})
        ),
        (status = 500, description = "Database connection failed", body = ErrorResponse,
            example = json!({"error": "Database error: connection refused"})
        )
    )
)]
pub async fn health_check(pool: web::Data<PgPool>) -> Result<HttpResponse, AppError> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })))
}

/// Trigger a data sync from DNIT Paraguay.
///
/// Starts a background task that scrapes the DNIT website for new RUC ZIP
/// files, downloads them, parses the contents, and upserts records into the
/// database. Returns immediately with `202 Accepted`.
///
/// **Access control:** restricted to IPs within the CIDR networks configured
/// in `sync.allowed_networks`. Returns `403 Forbidden` if the caller's IP
/// is outside the allowed range. If no networks are configured, the endpoint
/// is open to all.
#[utoipa::path(
    post,
    path = "/api/v1/sync",
    tag = "System",
    summary = "Trigger data sync from DNIT",
    description = "Starts a background scraper task that downloads RUC ZIP files from DNIT Paraguay, parses the contents, and upserts records into the database. Returns immediately with 202 Accepted.\n\n**Access control:** restricted to IPs within the CIDR networks configured in `sync.allowed_networks` (e.g. `10.10.0.0/20`). Returns 403 if the caller's IP is outside the allowed range. If no networks are configured, the endpoint is open to all.\n\n**Rate limit:** respects `sync.interval_hours`. If a sync was performed recently, returns 429 Too Many Requests.",
    responses(
        (status = 202, description = "Sync started successfully in background", body = serde_json::Value,
            example = json!({"message": "Sync started in background"})
        ),
        (status = 403, description = "Forbidden - caller IP not in allowed network", body = ErrorResponse,
            example = json!({"error": "Sync endpoint is restricted to the internal network"})
        ),
        (status = 429, description = "Sync was performed recently", body = serde_json::Value,
            example = json!({"error": "Last sync was 2h ago. Minimum interval is 24h."})
        ),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn trigger_sync(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    config: web::Data<Arc<AppConfig>>,
) -> Result<HttpResponse, AppError> {
    // Network restriction: check caller IP against allowed CIDRs
    if !config.sync_allowed_networks.is_empty() {
        let peer = req.peer_addr().ok_or_else(|| {
            AppError::Internal("Unable to determine client IP address".to_string())
        })?;
        if !config.is_sync_allowed(&peer.ip()) {
            return Err(AppError::Forbidden(
                "Sync endpoint is restricted to the internal network".to_string(),
            ));
        }
    }

    // Rate limit: enforce sync_interval_hours
    if let Some(last_sync) = repository::get_last_sync_time(pool.get_ref()).await? {
        let hours_since = (Utc::now() - last_sync).num_hours();
        if hours_since < config.sync_interval_hours as i64 {
            return Ok(HttpResponse::TooManyRequests().json(serde_json::json!({
                "error": format!(
                    "Last sync was {}h ago. Minimum interval is {}h.",
                    hours_since, config.sync_interval_hours
                )
            })));
        }
    }

    let pool_clone = pool.get_ref().clone();
    let config_clone = config.into_inner();
    tokio::spawn(async move {
        scraper::run_sync_db(&pool_clone, &config_clone).await;
    });
    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "message": "Sync started in background"
    })))
}
