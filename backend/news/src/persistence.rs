use crate::config::Config;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub fn pool(config: &Config) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(30))
        .connect_lazy_with(config.database_options())
}

// Connectivity only. Schema/version readiness is added with owned migrations (#11).
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
