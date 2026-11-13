use crate::config::Config;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub fn pool(config: &Config) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(30))
        .connect_lazy_with(config.database_options())
}

pub fn editor_pool(config: &Config) -> Option<PgPool> {
    config.editor_database_options().map(|options| {
        PgPoolOptions::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(3))
            .idle_timeout(Duration::from_secs(30))
            .connect_lazy_with(options)
    })
}

/// Explicit operator command only; web startup never runs DDL.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrate_with(pool, &MIGRATOR).await
}

pub async fn migrate_with(
    pool: &PgPool,
    migrator: &sqlx::migrate::Migrator,
) -> Result<(), sqlx::migrate::MigrateError> {
    let mut connection = pool.acquire().await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS news")
        .execute(&mut *connection)
        .await?;
    let result = migrator.run(&mut *connection).await;
    // Close the migration session even on failure, releasing any advisory locks.
    let _ = connection.close().await;
    result
}

pub async fn schema_available(pool: &PgPool) -> bool {
    let expected: Vec<_> = MIGRATOR.iter().collect();
    let query = sqlx::query_as::<_, (i64, Vec<u8>, bool)>(
        "SELECT version,checksum,success FROM news.schema_version ORDER BY version LIMIT $1",
    )
    .bind(expected.len() as i64 + 1)
    .fetch_all(pool);
    match tokio::time::timeout(Duration::from_secs(2), query).await {
        Ok(Ok(rows)) if rows.len() == expected.len() => {
            rows.iter().zip(expected).all(|(row, migration)| {
                row.0 == migration.version
                    && row.1.as_slice() == migration.checksum.as_ref()
                    && row.2
            })
        }
        _ => false,
    }
}

// Kept separate from schema readiness for operational diagnosis.
pub async fn database_available(pool: &PgPool) -> bool {
    matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool)
        )
        .await,
        Ok(Ok(1))
    )
}
