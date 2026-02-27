use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use chrono::Utc;
use sqlx::PgPool;

use crate::config::AppConfig;
use crate::errors::{AppError, ErrorResponse};
use crate::models::{
    CheckDigitResponse, FuzzySearchParams, PaginatedResponse, Ruc, RucSearchParams, RucWithScore,
    SyncParams, ValidateRucResponse,
};
use crate::repository;
use crate::scraper;

/// Get RUC by number.
///
/// Look up a single RUC by its exact number. Optionally include the check digit
/// separated by a hyphen (e.g. `1000000-3`). Returns 404 if not found.
#[utoipa::path(get, path = "/api/v1/ruc/{ruc}", tag = "RUC",
    params(("ruc" = String, Path, description = "RUC number, optionally with check digit", example = "1000000-3")),
    responses(
        (status = 200, description = "RUC found", body = Ruc),
        (status = 404, description = "RUC not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
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

/// Search RUCs with filters.
///
/// Search RUC records using combinable filters. Text fields use accent-insensitive,
/// case-insensitive partial matching (`unaccent() + ILIKE`). The `status` field uses
/// exact match for partition pruning. All filters are combined with AND logic.
#[utoipa::path(get, path = "/api/v1/ruc", tag = "RUC",
    params(RucSearchParams),
    responses(
        (status = 200, description = "Paginated results", body = inline(PaginatedResponse<Ruc>)),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn search_ruc(
    pool: web::Data<PgPool>,
    config: web::Data<Arc<AppConfig>>,
    query: web::Query<RucSearchParams>,
) -> Result<HttpResponse, AppError> {
    let mut params = query.into_inner();

    // Split "80077182-6" → ruc="80077182", check_digit="6"
    let check_digit_filter = match params.ruc.take() {
        Some(ruc_val) => {
            if let Some((number, dv)) = ruc_val.split_once('-') {
                let dv = dv.trim();
                params.ruc = Some(number.to_string());
                if dv.is_empty() { None } else { Some(dv.to_string()) }
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
    Ok(HttpResponse::Ok().json(PaginatedResponse { data, page, limit, total }))
}

/// Fuzzy search RUCs by name.
///
/// Fuzzy search using PostgreSQL `pg_trgm` trigram similarity with `unaccent()`.
/// Matches `query` against `full_name` and returns results ranked by similarity
/// score (highest first). The `status` filter enables partition pruning.
#[utoipa::path(get, path = "/api/v1/ruc/search", tag = "RUC",
    params(FuzzySearchParams),
    responses(
        (status = 200, description = "Paginated results with similarity scores", body = inline(PaginatedResponse<RucWithScore>)),
        (status = 500, description = "Internal server error", body = ErrorResponse),
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
    Ok(HttpResponse::Ok().json(PaginatedResponse { data, page, limit, total }))
}

/// Health check.
///
/// Verifies that the API server is running and the database connection is responsive.
#[utoipa::path(get, path = "/api/v1/health", tag = "System",
    responses(
        (status = 200, description = "Healthy", body = serde_json::Value,
            example = json!({"status": "ok"})),
        (status = 500, description = "Database unreachable", body = ErrorResponse),
    )
)]
pub async fn health_check(pool: web::Data<PgPool>) -> Result<HttpResponse, AppError> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool.get_ref())
        .await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })))
}

/// Trigger data sync from DNIT.
///
/// Starts a background scraper that downloads RUC ZIP files from DNIT Paraguay,
/// parses the contents, and upserts records. Returns 202 immediately.
///
/// **Access control:** restricted to `sync.allowed_networks` CIDRs.
/// **Rate limit:** respects `sync.interval_hours` (skipped with `?force=true`).
/// **Force mode:** `?force=true` bypasses rate limit and date/hash checks.
#[utoipa::path(post, path = "/api/v1/sync", tag = "System",
    params(("force" = Option<bool>, Query, description = "Bypass interval and date/hash checks")),
    responses(
        (status = 202, description = "Sync started", body = serde_json::Value,
            example = json!({"message": "Sync started in background"})),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 429, description = "Too recent", body = serde_json::Value,
            example = json!({"error": "Last sync was 2h ago. Minimum interval is 24h."})),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn trigger_sync(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    config: web::Data<Arc<AppConfig>>,
    query: web::Query<SyncParams>,
) -> Result<HttpResponse, AppError> {
    let force = query.force.unwrap_or(false);

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

    if !force {
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
    }

    let pool_clone = pool.get_ref().clone();
    let config_clone = config.into_inner();
    tokio::spawn(async move {
        scraper::run_sync_db(&pool_clone, &config_clone, force).await;
    });

    let message = if force { "Forced sync started in background" } else { "Sync started in background" };
    Ok(HttpResponse::Accepted().json(serde_json::json!({ "message": message })))
}

/// Compute check digit for a RUC.
///
/// Computes the dígito verificador using the Módulo 11 algorithm (DNIT Paraguay).
#[utoipa::path(get, path = "/api/v1/ruc/{ruc}/dv", tag = "RUC",
    params(("ruc" = String, Path, description = "RUC number (without check digit)", example = "1000100")),
    responses(
        (status = 200, description = "Check digit computed", body = CheckDigitResponse),
        (status = 400, description = "Invalid RUC format", body = ErrorResponse),
    )
)]
pub async fn compute_check_digit(
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let ruc = path.into_inner();
    require_alphanumeric(&ruc)?;
    let dv = crate::models::compute_check_digit(&ruc);
    Ok(HttpResponse::Ok().json(CheckDigitResponse {
        full: format!("{ruc}-{dv}"),
        ruc,
        check_digit: dv,
    }))
}

/// Validate a RUC check digit.
///
/// Validates whether the check digit matches the Módulo 11 computation.
///
/// Accepts: `/api/v1/ruc/1000100-0/validate` (hyphen) or
/// `/api/v1/ruc/1000100/validate/0` (split path).
#[utoipa::path(get, path = "/api/v1/ruc/{ruc_dv}/validate", tag = "RUC",
    params(("ruc_dv" = String, Path, description = "RUC-DV (e.g. `1000100-0`)", example = "1000100-0")),
    responses(
        (status = 200, description = "Validation result", body = ValidateRucResponse),
        (status = 400, description = "Invalid format", body = ErrorResponse),
    )
)]
pub async fn validate_ruc(
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let raw = path.into_inner();
    let (ruc, dv) = parse_ruc_dv(&raw)?;
    do_validate_ruc(&ruc, &dv)
}

/// Alternative validate route: `/api/v1/ruc/{ruc}/validate/{dv}`
pub async fn validate_ruc_split(
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (ruc, dv) = path.into_inner();
    do_validate_ruc(&ruc, &dv)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn require_alphanumeric(ruc: &str) -> Result<(), AppError> {
    if ruc.is_empty() || !ruc.chars().all(|c| c.is_alphanumeric()) {
        return Err(AppError::BadRequest(
            "RUC must be a non-empty alphanumeric string".to_string(),
        ));
    }
    Ok(())
}

fn do_validate_ruc(ruc: &str, dv: &str) -> Result<HttpResponse, AppError> {
    require_alphanumeric(ruc)?;
    let valid = crate::models::validate_ruc(ruc, dv);
    Ok(HttpResponse::Ok().json(ValidateRucResponse {
        ruc: ruc.to_string(),
        check_digit: dv.to_string(),
        valid,
    }))
}

fn parse_ruc_dv(input: &str) -> Result<(String, String), AppError> {
    let (ruc, dv) = input.split_once('-').ok_or_else(|| {
        AppError::BadRequest(
            "Expected format RUC-DV (e.g. 1000100-0). Use /ruc/{ruc}/validate/{dv} for separate segments.".to_string(),
        )
    })?;
    require_alphanumeric(ruc)?;
    if dv.is_empty() {
        return Err(AppError::BadRequest(
            "Check digit is required (e.g. 1000100-0)".to_string(),
        ));
    }
    Ok((ruc.to_string(), dv.to_string()))
}
