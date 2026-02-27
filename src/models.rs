use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use serde_json::json;
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};

/// Registro de contribuyente (RUC) del sistema tributario paraguayo (DNIT).
#[derive(Debug, FromRow, Serialize, ToSchema)]
#[schema(example = json!({
    "id": 42,
    "ruc": "1000000",
    "first_names": "JUANA DEL CARMEN",
    "last_names": "CAÑETE GONZALEZ",
    "full_name": "JUANA DEL CARMEN CAÑETE GONZALEZ",
    "check_digit": "3",
    "old_ruc": "CAGJ761720E",
    "status": "ACTIVO",
    "reference_date": "2026-02-01",
    "created_at": "2026-02-01T00:00:00Z",
    "updated_at": "2026-02-01T00:00:00Z",
    "file_metadata_id": 1
}))]
pub struct Ruc {
    /// Internal auto-increment ID.
    pub id: i32,
    /// RUC number (tax ID), e.g. "1000000".
    #[schema(example = "1000000")]
    pub ruc: String,
    /// First names of the taxpayer.
    #[schema(example = "JUANA DEL CARMEN")]
    pub first_names: Option<String>,
    /// Last names of the taxpayer.
    #[schema(example = "CAÑETE GONZALEZ")]
    pub last_names: Option<String>,
    /// Full name (first + last names concatenated).
    #[schema(example = "JUANA DEL CARMEN CAÑETE GONZALEZ")]
    pub full_name: Option<String>,
    /// Check digit for RUC validation.
    #[schema(example = "3")]
    pub check_digit: Option<String>,
    /// Legacy RUC identifier.
    #[schema(example = "CAGJ761720E")]
    pub old_ruc: Option<String>,
    /// Taxpayer status: ACTIVO, CANCELADO, SUSPENSION TEMPORAL, etc.
    #[schema(example = "ACTIVO")]
    pub status: Option<String>,
    /// DNIT reference date ("Actualizado al ...") — indicates when the source data was last published.
    #[schema(example = "2026-02-01")]
    pub reference_date: Option<NaiveDate>,
    /// Timestamp when this record was first inserted.
    pub created_at: Option<DateTime<Utc>>,
    /// Timestamp of the last update to this record.
    pub updated_at: Option<DateTime<Utc>>,
    /// FK to ruc_file_metadata — identifies the source file.
    #[schema(example = 1)]
    pub file_metadata_id: Option<i32>,
}

/// Query parameters for filtered RUC search.
///
/// All text fields use accent-insensitive, case-insensitive partial matching
/// (`unaccent() + ILIKE`). The `status` field uses exact match for PostgreSQL
/// partition pruning. All filters are combinable (AND logic).
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RucSearchParams {
    /// Filter by RUC number (partial match via ILIKE).
    #[param(example = "100000")]
    pub ruc: Option<String>,
    /// Search across full_name (accent/case insensitive, partial match).
    #[param(example = "CAÑETE")]
    pub name: Option<String>,
    /// Filter by first names (accent/case insensitive, partial match).
    #[param(example = "JUANA")]
    pub first_names: Option<String>,
    /// Filter by last names (accent/case insensitive, partial match).
    #[param(example = "GONZALEZ")]
    pub last_names: Option<String>,
    /// Filter by full name (accent/case insensitive, partial match).
    pub full_name: Option<String>,
    /// Filter by old/legacy RUC identifier (partial match via ILIKE).
    pub old_ruc: Option<String>,
    /// Exact status filter. Enables partition pruning. Values: ACTIVO, CANCELADO, SUSPENSION TEMPORAL, BLOQUEADO.
    #[param(example = "ACTIVO")]
    pub status: Option<String>,
    /// Page number (1-based). Defaults to 1.
    #[param(example = 1, minimum = 1)]
    pub page: Option<i64>,
    /// Results per page. Defaults to 25, max 200.
    #[param(example = 25, minimum = 1, maximum = 200)]
    pub limit: Option<i64>,
}

/// Paginated API response wrapper.
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "data": [],
    "page": 1,
    "limit": 25,
    "total": 1234
}))]
pub struct PaginatedResponse<T: Serialize> {
    /// Array of results for the current page.
    pub data: Vec<T>,
    /// Current page number (1-based).
    #[schema(example = 1)]
    pub page: i64,
    /// Number of results per page.
    #[schema(example = 25)]
    pub limit: i64,
    /// Total number of records matching the query (across all pages).
    #[schema(example = 1234)]
    pub total: i64,
}

/// Query parameters for fuzzy (trigram similarity) search.
///
/// Uses PostgreSQL `pg_trgm` extension with `unaccent()` for accent-insensitive
/// fuzzy matching. Results are ranked by similarity score (highest first).
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FuzzySearchParams {
    /// Text to search for using trigram similarity. Matched against full_name.
    #[param(example = "JUAN CARLOS LOPES")]
    pub query: String,
    /// Exact status filter. Enables partition pruning. Values: ACTIVO, CANCELADO, SUSPENSION TEMPORAL, BLOQUEADO.
    #[param(example = "ACTIVO")]
    pub status: Option<String>,
    /// Minimum similarity threshold (0.0–1.0). Lower = broader matches. Defaults to 0.3, range: 0.1–0.9.
    #[param(example = 0.3, minimum = 0.1, maximum = 0.9)]
    pub threshold: Option<f64>,
    /// Page number (1-based). Defaults to 1.
    #[param(example = 1, minimum = 1)]
    pub page: Option<i64>,
    /// Results per page. Defaults to 25, max 200.
    #[param(example = 25, minimum = 1, maximum = 200)]
    pub limit: Option<i64>,
}

/// RUC record enriched with a similarity score from fuzzy search.
#[derive(Debug, FromRow, Serialize, ToSchema)]
#[schema(example = json!({
    "id": 42,
    "ruc": "1000000",
    "first_names": "JUAN CARLOS",
    "last_names": "LOPEZ MARTINEZ",
    "full_name": "JUAN CARLOS LOPEZ MARTINEZ",
    "check_digit": "7",
    "old_ruc": "LMJC800101A",
    "status": "ACTIVO",
    "reference_date": "2026-02-01",
    "created_at": "2026-02-01T00:00:00Z",
    "updated_at": "2026-02-01T00:00:00Z",
    "file_metadata_id": 1,
    "score": 0.72
}))]
pub struct RucWithScore {
    /// Internal auto-increment ID.
    pub id: i32,
    /// RUC number (tax ID).
    #[schema(example = "1000000")]
    pub ruc: String,
    /// First names of the taxpayer.
    #[schema(example = "JUAN CARLOS")]
    pub first_names: Option<String>,
    /// Last names of the taxpayer.
    #[schema(example = "LOPEZ MARTINEZ")]
    pub last_names: Option<String>,
    /// Full name (first + last names concatenated).
    #[schema(example = "JUAN CARLOS LOPEZ MARTINEZ")]
    pub full_name: Option<String>,
    /// Check digit for RUC validation.
    #[schema(example = "7")]
    pub check_digit: Option<String>,
    /// Legacy RUC identifier.
    #[schema(example = "LMJC800101A")]
    pub old_ruc: Option<String>,
    /// Taxpayer status.
    #[schema(example = "ACTIVO")]
    pub status: Option<String>,
    /// DNIT reference date ("Actualizado al ...") — indicates when the source data was last published.
    #[schema(example = "2026-02-01")]
    pub reference_date: Option<NaiveDate>,
    /// Timestamp when this record was first inserted.
    pub created_at: Option<DateTime<Utc>>,
    /// Timestamp of the last update to this record.
    pub updated_at: Option<DateTime<Utc>>,
    /// FK to ruc_file_metadata — identifies the source file.
    #[schema(example = 1)]
    pub file_metadata_id: Option<i32>,
    /// Trigram similarity score (0.0–1.0). Higher = closer match.
    #[schema(example = 0.72)]
    pub score: f32,
}

/// Lightweight row for backfilling file hashes.
#[derive(Debug, FromRow)]
pub struct FileMetadataRow {
    pub id: i32,
    pub file_name: String,
    pub file_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParsedRuc {
    pub ruc: String,
    pub first_names: String,
    pub last_names: String,
    pub full_name: String,
    pub check_digit: String,
    pub old_ruc: String,
    pub status: String,
}
