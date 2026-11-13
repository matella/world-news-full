//! Private publisher controls. Pause/resume never performs an HTTP request.
use crate::{
    auth::{self, Permission, Session},
    content::{PageSize, valid_id},
    error::AppError,
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
#[derive(Debug, sqlx::FromRow)]
pub struct FeedStatus {
    pub id: String,
    pub control_version: i64,
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub failure_count: i32,
    pub retry_after_seconds: Option<String>,
    pub retry_after_observed_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
    pub last_error: Option<String>,
    pub next_fetch_at: DateTime<Utc>,
    pub last_attempt_id: Option<i64>,
    pub lease_until: Option<DateTime<Utc>>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct FetchHistory {
    pub id: i64,
    pub outcome: String,
    pub error_category: Option<String>,
    pub http_status: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}
#[derive(Clone)]
pub struct FeedAdmin {
    pool: PgPool,
}
impl FeedAdmin {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn status(&self, session: &Session, id: &str) -> Result<FeedStatus, AppError> {
        check_id(id)?;
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let status = status(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn pause(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        reason: &str,
    ) -> Result<FeedStatus, AppError> {
        check_change(id, expected, reason)?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let current = control(&mut tx, id, expected).await?;
        if current.paused {
            return Err(AppError::Conflict);
        }
        if let Some(attempt) = current.last_attempt_id {
            sqlx::query("UPDATE news.feed_fetch_attempts SET outcome='cancelled',error_category='operator_paused',finished_at=clock_timestamp(),lease_until=NULL WHERE id=$1 AND outcome='started'").bind(attempt).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE news.feeds SET fetch_paused=true,fetch_pause_reason='manual_pause',last_outcome=CASE WHEN last_outcome='started' THEN 'cancelled' ELSE last_outcome END,last_error=CASE WHEN last_outcome='started' THEN 'operator_paused' ELSE last_error END,fetch_control_version=fetch_control_version+1 WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, id, actor.id(), "paused", reason, expected + 1).await?;
        let status = status(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn resume(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        reason: &str,
    ) -> Result<FeedStatus, AppError> {
        check_change(id, expected, reason)?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let current = control(&mut tx, id, expected).await?;
        if !current.paused {
            return Err(AppError::Conflict);
        }
        if current.pause_reason.as_deref() == Some("legacy_retry_instruction_unknown") {
            return Err(AppError::Validation(
                "Legacy source retry instruction needs independent verification.",
            ));
        }
        let allowed=sqlx::query_scalar::<_,bool>("SELECT retry_after_seconds IS NULL OR extract(epoch FROM clock_timestamp()-retry_after_observed_at)>=retry_after_seconds::numeric FROM news.feeds WHERE id=$1").bind(id).fetch_one(&mut *tx).await.map_err(db)?;
        if !allowed {
            return Err(AppError::Validation(
                "Source retry instruction has not elapsed.",
            ));
        }
        // Do not turn manual retry into an early retry of an existing source delay.
        sqlx::query("UPDATE news.feeds SET fetch_paused=false,fetch_pause_reason=NULL,failure_count=0,fetch_control_version=fetch_control_version+1,next_fetch_at=GREATEST(next_fetch_at,clock_timestamp()) WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, id, actor.id(), "resumed", reason, expected + 1).await?;
        let status = status(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn history(
        &self,
        session: &Session,
        id: &str,
        before: Option<i64>,
        limit: PageSize,
    ) -> Result<Vec<FetchHistory>, AppError> {
        check_id(id)?;
        if before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid fetch history cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        status(&mut tx, id).await?;
        let rows=sqlx::query_as::<_,FetchHistory>("SELECT id,outcome,error_category,http_status,started_at,finished_at FROM news.feed_fetch_attempts WHERE feed_id=$1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT $3").bind(id).bind(before).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
}
async fn status(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<FeedStatus, AppError> {
    sqlx::query_as("SELECT f.id,f.fetch_control_version AS control_version,f.fetch_paused AS paused,f.fetch_pause_reason AS pause_reason,f.failure_count,f.retry_after_seconds,f.retry_after_observed_at,f.last_outcome,f.last_error,f.next_fetch_at,f.last_attempt_id,a.lease_until FROM news.feeds f LEFT JOIN news.feed_fetch_attempts a ON a.id=f.last_attempt_id WHERE f.id=$1").bind(id).fetch_one(&mut **tx).await.map_err(db)
}
async fn control(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    expected: i64,
) -> Result<FeedStatus, AppError> {
    let source = sqlx::query_scalar::<_, String>("SELECT source_id FROM news.feeds WHERE id=$1")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    sqlx::query("SELECT * FROM news.lock_source_for_fetch($1)")
        .bind(source)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    sqlx::query("SELECT id FROM news.feeds WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)?;
    let status = status(tx, id).await?;
    if status.control_version != expected {
        return Err(AppError::Conflict);
    }
    Ok(status)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    actor: &str,
    action: &str,
    reason: &str,
    version: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.feed_operational_actions(feed_id,actor_id,action,reason,control_version) VALUES($1,$2,$3,$4,$5)").bind(id).bind(actor).bind(action).bind(reason).bind(version).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid feed ID."))
    }
}
fn check_change(id: &str, version: i64, reason: &str) -> Result<(), AppError> {
    check_id(id)?;
    if version < 0
        || reason.trim().is_empty()
        || reason.chars().count() > 2000
        || reason.contains('\0')
    {
        return Err(AppError::Validation("Invalid feed control change."));
    }
    Ok(())
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("feed_control", error)
}
