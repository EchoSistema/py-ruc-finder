use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config::AppConfig;

pub async fn create_pool(config: &AppConfig) -> PgPool {
    let url = config
        .database_url
        .as_ref()
        .expect("DATABASE_URL must be set to use the database");
    PgPoolOptions::new()
        .max_connections(config.db_pool_size)
        .connect(url)
        .await
        .expect("Failed to create database pool")
}
