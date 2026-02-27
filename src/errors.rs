use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
#[allow(unused_imports)]
use serde_json::json;
use std::fmt;
use utoipa::ToSchema;

/// Standard error response body returned by all API error responses.
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({"error": "RUC 9999999 not found"}))]
pub struct ErrorResponse {
    /// Human-readable error message describing what went wrong.
    #[schema(example = "RUC 9999999 not found")]
    pub error: String,
}

#[derive(Debug)]
pub enum AppError {
    Db(sqlx::Error),
    BadRequest(String),
    NotFound(String),
    Forbidden(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Db(e) => write!(f, "Database error: {e}"),
            AppError::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            AppError::NotFound(msg) => write!(f, "Not found: {msg}"),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {msg}"),
            AppError::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Db(_) | AppError::Internal(_) => {
                HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": self.to_string()
                }))
            }
            AppError::BadRequest(msg) => HttpResponse::BadRequest().json(serde_json::json!({
                "error": msg
            })),
            AppError::NotFound(msg) => HttpResponse::NotFound().json(serde_json::json!({
                "error": msg
            })),
            AppError::Forbidden(msg) => HttpResponse::Forbidden().json(serde_json::json!({
                "error": msg
            })),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Db(e)
    }
}
