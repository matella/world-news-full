//! Typed private operational reads. Every call revalidates a live editor session.
use crate::{
    auth::{self, Permission, Session},
    content::{PageSize, valid_id},
    error::AppError,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
#[derive(sqlx::FromRow)]
pub struct Overview {
    pub sources: i64,
    pub active_feeds: i64,
    pub source_articles: i64,
    pub quarantined: i64,
    pub normalization_waiting: i64,
    pub extraction_waiting: i64,
    pub paused_feeds: i64,
    pub last_feed_success: Option<DateTime<Utc>>,
}
#[derive(Clone, sqlx::FromRow)]
pub struct Article {
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub original_url: String,
    pub attribution_name: String,
    pub language: String,
    pub extraction_state: String,
    pub intake_version: i64,
    pub current_intake_id: Option<i64>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
}
#[derive(sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub target: String,
    pub state: String,
    pub version: i64,
    pub attempts: i32,
    pub error: Option<String>,
    pub available_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub expired: bool,
    pub quarantined: i64,
}
#[derive(sqlx::FromRow)]
pub struct Batch {
    pub id: i64,
    pub feed_id: String,
    pub outcome: String,
    pub created_at: DateTime<Utc>,
    pub job_id: Option<i64>,
}
#[derive(sqlx::FromRow)]
pub struct FeedRow {
    pub id: String,
    pub source_id: String,
    pub enabled: bool,
    pub source_enabled: bool,
    pub paused: bool,
    pub last_outcome: Option<String>,
    pub last_error: Option<String>,
    pub next_fetch_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub stale: bool,
}
#[derive(sqlx::FromRow)]
pub struct DraftLink {
    pub story_id: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone)]
pub struct Operations {
    pool: PgPool,
}
impl Operations {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn feeds(
        &self,
        session: &Session,
        after: Option<&str>,
        limit: PageSize,
    ) -> Result<Vec<FeedRow>, AppError> {
        if after.is_some_and(|id| !valid_id(id)) {
            return Err(AppError::Validation("Invalid feed cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as("SELECT f.id,f.source_id,f.enabled,s.enabled AS source_enabled,f.fetch_paused AS paused,f.last_outcome,f.last_error,f.next_fetch_at,f.last_success_at,COALESCE(f.last_success_at<clock_timestamp()-make_interval(secs=>f.poll_seconds*2),false) AS stale FROM news.feeds f JOIN news.sources s ON s.id=f.source_id WHERE ($1::text IS NULL OR f.id>$1) ORDER BY f.id LIMIT $2").bind(after).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn draft_links(
        &self,
        session: &Session,
        id: &str,
    ) -> Result<Vec<DraftLink>, AppError> {
        if !valid_id(id) {
            return Err(AppError::Validation("Invalid article ID."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as("SELECT l.story_id,i.created_at FROM news.intake_draft_links l JOIN news.intake_snapshots i ON i.id=l.snapshot_id WHERE i.source_article_id=$1 ORDER BY i.id DESC LIMIT 20").bind(id).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn overview(&self, session: &Session) -> Result<Overview, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let row=sqlx::query_as("SELECT (SELECT count(*) FROM news.sources) AS sources,(SELECT count(*) FROM news.feeds f JOIN news.sources s ON s.id=f.source_id WHERE f.enabled AND s.enabled) AS active_feeds,(SELECT count(*) FROM news.source_articles) AS source_articles,(SELECT count(*) FROM news.source_observations WHERE disposition='quarantined') AS quarantined,(SELECT count(*) FROM news.normalization_jobs WHERE state IN ('queued','retry','running')) AS normalization_waiting,(SELECT count(*) FROM news.extraction_jobs WHERE state IN ('queued','retry','running')) AS extraction_waiting,(SELECT count(*) FROM news.feeds WHERE fetch_paused) AS paused_feeds,(SELECT max(last_success_at) FROM news.feeds) AS last_feed_success").fetch_one(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(row)
    }
    pub async fn articles(
        &self,
        session: &Session,
        after: Option<&str>,
        state: Option<&str>,
        source: Option<&str>,
        limit: PageSize,
    ) -> Result<Vec<Article>, AppError> {
        if after.is_some_and(|v| !valid_id(v))
            || source.is_some_and(|v| !valid_id(v))
            || state.is_some_and(|v| !matches!(v, "metadata_only" | "pending" | "ready" | "failed"))
        {
            return Err(AppError::Validation("Invalid source article filter."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as("SELECT id,source_id,title,original_url,attribution_name,language,extraction_state,intake_version,current_intake_id,published_at,fetched_at FROM news.source_articles WHERE ($1::text IS NULL OR id>$1) AND ($2::text IS NULL OR extraction_state=$2) AND ($3::text IS NULL OR source_id=$3) ORDER BY id LIMIT $4").bind(after).bind(state).bind(source).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn article(&self, session: &Session, id: &str) -> Result<Article, AppError> {
        if !valid_id(id) {
            return Err(AppError::Validation("Invalid article ID."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let row=sqlx::query_as("SELECT id,source_id,title,original_url,attribution_name,language,extraction_state,intake_version,current_intake_id,published_at,fetched_at FROM news.source_articles WHERE id=$1").bind(id).fetch_one(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(row)
    }
    pub async fn jobs(
        &self,
        session: &Session,
        extraction: bool,
        before: Option<i64>,
        state: Option<&str>,
        limit: PageSize,
    ) -> Result<Vec<Job>, AppError> {
        if before.is_some_and(|v| v <= 0)
            || state.is_some_and(|s| {
                !matches!(
                    s,
                    "queued" | "retry" | "running" | "succeeded" | "failed" | "cancelled"
                )
            })
        {
            return Err(AppError::Validation("Invalid job filter."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let sql = if extraction {
            "SELECT id,source_article_id AS target,state,lock_version AS version,attempt_count AS attempts,last_error AS error,available_at,created_at,COALESCE(lease_until<=clock_timestamp(),false) AS expired,0::bigint AS quarantined FROM news.extraction_jobs WHERE ($1::bigint IS NULL OR id<$1) AND ($2::text IS NULL OR state=$2) ORDER BY id DESC LIMIT $3"
        } else {
            "SELECT j.id,j.batch_id::text AS target,j.state,j.lock_version AS version,j.attempt_count AS attempts,j.last_error AS error,j.available_at,j.created_at,COALESCE(j.lease_until<=clock_timestamp(),false) AS expired,(SELECT count(*) FROM news.source_observations o WHERE o.attempt_id=j.batch_id AND o.disposition='quarantined') AS quarantined FROM news.normalization_jobs j WHERE ($1::bigint IS NULL OR j.id<$1) AND ($2::text IS NULL OR j.state=$2) ORDER BY j.id DESC LIMIT $3"
        };
        let rows = sqlx::query_as(sql)
            .bind(before)
            .bind(state)
            .bind(i64::from(limit.get()))
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn batches(
        &self,
        session: &Session,
        before: Option<i64>,
        limit: PageSize,
    ) -> Result<Vec<Batch>, AppError> {
        if before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid batch cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as("SELECT b.attempt_id AS id,a.feed_id,a.outcome,b.created_at,j.id AS job_id FROM news.feed_batches b JOIN news.feed_fetch_attempts a ON a.id=b.attempt_id LEFT JOIN news.normalization_jobs j ON j.batch_id=b.attempt_id WHERE ($1::bigint IS NULL OR b.attempt_id<$1) ORDER BY b.attempt_id DESC LIMIT $2").bind(before).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
}
fn db(e: sqlx::Error) -> AppError {
    AppError::database("operations_read", e)
}
