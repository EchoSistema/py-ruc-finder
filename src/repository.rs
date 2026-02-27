use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

use crate::config::AppConfig;
use crate::models::{
    FileMetadataRow, FuzzySearchParams, ParsedRuc, Ruc, RucSearchParams, RucWithScore,
};

const RUC_JOIN: &str = "FROM ruc r LEFT JOIN ruc_file_metadata m ON r.file_metadata_id = m.id";

const RUC_COLUMNS: &str = "r.id, r.ruc, r.first_names, r.last_names, r.full_name, r.check_digit, \
     r.old_ruc, r.status, m.reference_date, r.created_at, r.updated_at, r.file_metadata_id";

pub async fn get_last_reference_date(pool: &PgPool) -> Result<Option<NaiveDate>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<NaiveDate>>(
        "SELECT reference_date FROM ruc_file_metadata ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.flatten())
}

/// Returns the timestamp of the most recent sync (file metadata insertion).
pub async fn get_last_sync_time(pool: &PgPool) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT created_at FROM ruc_file_metadata ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map(|opt| opt.flatten())
}

/// Returns the file_hash of the most recent metadata entry for a given file_name.
pub async fn get_last_file_hash(
    pool: &PgPool,
    file_name: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT file_hash FROM ruc_file_metadata
         WHERE file_name = $1
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(file_name)
    .fetch_optional(pool)
    .await
    .map(|opt| opt.flatten())
}

pub async fn insert_file_metadata(
    pool: &PgPool,
    file_name: &str,
    file_url: &str,
    reference_date: Option<NaiveDate>,
    file_hash: i64,
) -> Result<i32, sqlx::Error> {
    let row: (i32,) = sqlx::query_as(
        "INSERT INTO ruc_file_metadata (file_name, file_url, reference_date, file_hash)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(file_name)
    .bind(file_url)
    .bind(reference_date)
    .bind(file_hash)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Returns all file metadata rows where file_hash is NULL.
pub async fn get_metadata_without_hash(pool: &PgPool) -> Result<Vec<FileMetadataRow>, sqlx::Error> {
    sqlx::query_as::<_, FileMetadataRow>(
        "SELECT id, file_name, file_url FROM ruc_file_metadata WHERE file_hash IS NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

/// Updates the file_hash for a given metadata row.
pub async fn update_file_hash(pool: &PgPool, id: i32, file_hash: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ruc_file_metadata SET file_hash = $1 WHERE id = $2")
        .bind(file_hash)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn upsert_ruc_batch(
    pool: &PgPool,
    batch: &[ParsedRuc],
    file_metadata_id: i32,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for record in batch {
        sqlx::query(
            "INSERT INTO ruc (ruc, first_names, last_names, full_name, check_digit, old_ruc, status, file_metadata_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (ruc, status) DO NOTHING",
        )
        .bind(&record.ruc)
        .bind(&record.first_names)
        .bind(&record.last_names)
        .bind(&record.full_name)
        .bind(&record.check_digit)
        .bind(&record.old_ruc)
        .bind(&record.status)
        .bind(file_metadata_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn find_ruc_by_number(
    pool: &PgPool,
    ruc: &str,
    check_digit: Option<&str>,
) -> Result<Option<Ruc>, sqlx::Error> {
    match check_digit {
        Some(dv) => {
            sqlx::query_as::<_, Ruc>(&format!(
                "SELECT {RUC_COLUMNS} {RUC_JOIN} WHERE r.ruc = $1 AND r.check_digit = $2"
            ))
            .bind(ruc)
            .bind(dv)
            .fetch_optional(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, Ruc>(&format!("SELECT {RUC_COLUMNS} {RUC_JOIN} WHERE r.ruc = $1"))
                .bind(ruc)
                .fetch_optional(pool)
                .await
        }
    }
}

/// Standard search with ILIKE + unaccent for accent-insensitive matching.
/// Status uses exact match (=) to enable partition pruning.
pub async fn search_ruc(
    pool: &PgPool,
    params: &RucSearchParams,
    check_digit: Option<&str>,
    config: &AppConfig,
) -> Result<(Vec<Ruc>, i64), sqlx::Error> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(config.pagination_limit)
        .clamp(1, config.pagination_max);
    let offset = (page - 1) * limit;

    let mut conditions: Vec<String> = Vec::new();
    let mut bind_idx = 1u32;
    let mut filter_values: Vec<String> = Vec::new();

    // Status uses exact match (=) for partition pruning
    if let Some(ref v) = params.status
        && !v.is_empty()
    {
        conditions.push(format!("r.status = ${bind_idx}"));
        filter_values.push(v.clone());
        bind_idx += 1;
    }

    // RUC uses plain ILIKE (no accents in numbers)
    if let Some(ref v) = params.ruc
        && !v.is_empty()
    {
        conditions.push(format!("r.ruc ILIKE ${bind_idx}"));
        filter_values.push(format!("%{v}%"));
        bind_idx += 1;
    }

    // Check digit uses exact match (parsed from "ruc" param when it contains a hyphen)
    if let Some(dv) = check_digit {
        conditions.push(format!("r.check_digit = ${bind_idx}"));
        filter_values.push(dv.to_string());
        bind_idx += 1;
    }

    // Text fields use immutable_unaccent() for accent-insensitive search
    let text_filters: Vec<(&str, &Option<String>)> = vec![
        ("r.full_name", &params.name),
        ("r.first_names", &params.first_names),
        ("r.last_names", &params.last_names),
        ("r.full_name", &params.full_name),
    ];
    for (col, param) in text_filters {
        if let Some(v) = param
            && !v.is_empty()
        {
            conditions.push(format!("immutable_unaccent({col}) ILIKE immutable_unaccent(${bind_idx})"));
            filter_values.push(format!("%{v}%"));
            bind_idx += 1;
        }
    }

    // old_ruc uses plain ILIKE (no accents)
    if let Some(ref v) = params.old_ruc
        && !v.is_empty()
    {
        conditions.push(format!("r.old_ruc ILIKE ${bind_idx}"));
        filter_values.push(format!("%{v}%"));
        bind_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM ruc r {where_clause}");
    let data_sql = format!(
        "SELECT {RUC_COLUMNS} {RUC_JOIN} {where_clause} ORDER BY r.id LIMIT ${bind_idx} OFFSET ${}",
        bind_idx + 1
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for v in &filter_values {
        count_query = count_query.bind(v);
    }
    let total = count_query.fetch_one(pool).await?;

    let mut data_query = sqlx::query_as::<_, Ruc>(&data_sql);
    for v in &filter_values {
        data_query = data_query.bind(v);
    }
    data_query = data_query.bind(limit).bind(offset);
    let rows = data_query.fetch_all(pool).await?;

    Ok((rows, total))
}

/// Fuzzy search using pg_trgm similarity + unaccent.
/// Returns paginated results ranked by similarity score.
///
/// Optimization: uses a subquery to first filter and paginate from the ruc
/// table, then JOINs with ruc_file_metadata only for the result page.
/// The COUNT uses count_estimate for large result sets to avoid slow exact counts.
pub async fn fuzzy_search_ruc(
    pool: &PgPool,
    params: &FuzzySearchParams,
    config: &AppConfig,
) -> Result<(Vec<RucWithScore>, i64), sqlx::Error> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params
        .limit
        .unwrap_or(config.fuzzy_limit)
        .clamp(1, config.fuzzy_max);
    let offset = (page - 1) * limit;
    let threshold = params.threshold.unwrap_or(config.fuzzy_threshold);

    let mut conditions: Vec<String> = Vec::new();
    let mut bind_idx = 2u32; // $1 is reserved for the search term

    // Set similarity threshold for this query
    let threshold_sql = format!(
        "SET LOCAL pg_trgm.similarity_threshold = {}",
        threshold.clamp(config.fuzzy_threshold_min, config.fuzzy_threshold_max)
    );

    // Status exact match for partition pruning
    if let Some(ref v) = params.status
        && !v.is_empty()
    {
        conditions.push(format!("r.status = ${bind_idx}"));
        bind_idx += 1;
    }

    // Fuzzy match on full_name with unaccent
    conditions.push("immutable_unaccent(r.full_name) % immutable_unaccent($1)".to_string());

    let where_clause = format!("WHERE {}", conditions.join(" AND "));

    // Count total matches
    let count_sql = format!("SELECT COUNT(*) FROM ruc r {where_clause}");

    // Subquery: filter + paginate from ruc, then JOIN metadata only for the page
    let data_sql = format!(
        "SELECT sub.id, sub.ruc, sub.first_names, sub.last_names, sub.full_name, \
                sub.check_digit, sub.old_ruc, sub.status, m.reference_date, \
                sub.created_at, sub.updated_at, sub.file_metadata_id, sub.score \
         FROM ( \
             SELECT r.*, similarity(immutable_unaccent(r.full_name), immutable_unaccent($1)) AS score \
             FROM ruc r \
             {where_clause} \
             ORDER BY score DESC \
             LIMIT ${bind_idx} OFFSET ${} \
         ) sub \
         LEFT JOIN ruc_file_metadata m ON sub.file_metadata_id = m.id \
         ORDER BY sub.score DESC",
        bind_idx + 1
    );

    // Execute threshold setting and queries in a transaction
    let mut tx = pool.begin().await?;
    sqlx::query(&threshold_sql).execute(&mut *tx).await?;

    // Count total matches
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    count_query = count_query.bind(&params.query);
    if let Some(ref v) = params.status
        && !v.is_empty()
    {
        count_query = count_query.bind(v);
    }
    let total = count_query.fetch_one(&mut *tx).await?;

    // Fetch paginated data
    let mut query = sqlx::query_as::<_, RucWithScore>(&data_sql);
    query = query.bind(&params.query);
    if let Some(ref v) = params.status
        && !v.is_empty()
    {
        query = query.bind(v);
    }
    query = query.bind(limit).bind(offset);

    let rows = query.fetch_all(&mut *tx).await?;
    tx.commit().await?;

    Ok((rows, total))
}
