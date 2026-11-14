//! Explicit expiring publisher grants; machine work never impersonates an editor.
use super::{
    article::{ArticleClient, ExtractionError},
    http::{FetchError, SafeHttp},
    manual::{self, ArticleTransport, Context},
};
use crate::{
    auth::{self, Permission, Session},
    content::{PageSize, valid_id},
    error::AppError,
    persistence,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use std::{sync::Arc, time::Duration};
use url::Url;

#[derive(Debug, sqlx::FromRow)]
pub struct ExtractionStatus {
    pub id: i64,
    pub source_article_id: String,
    pub requested_by: String,
    pub authorized_until: DateTime<Utc>,
    pub state: String,
    pub generation: i64,
    pub attempt_count: i32,
    pub lock_version: i64,
    pub last_error: Option<String>,
    pub available_at: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub expected_intake_version: i64,
    pub source_version: i64,
    pub grant_live: bool,
    pub lease_live: bool,
}
#[derive(Debug)]
pub struct ExtractionLease {
    id: i64,
    generation: i64,
}
impl ExtractionLease {
    pub fn job_id(&self) -> i64 {
        self.id
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ExtractionOutcome {
    Idle,
    Completed { job_id: i64 },
    Deferred { job_id: i64 },
    Failed { job_id: i64 },
    Cancelled { job_id: i64 },
    LostLease { job_id: i64 },
}
#[derive(Clone)]
pub struct ExtractionAdmin {
    pool: PgPool,
}
impl ExtractionAdmin {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn request(
        &self,
        session: &Session,
        article: &str,
        expected: i64,
        reason: &str,
    ) -> Result<ExtractionStatus, AppError> {
        if !valid_id(article) || expected < 0 || !valid_reason(reason) {
            return Err(AppError::Validation("Invalid extraction request."));
        }
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        // Serialize grants before context locks. Workers never acquire this lock.
        sqlx::query("SELECT pg_advisory_xact_lock(781642903)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let existing=sqlx::query_scalar::<_,i64>("SELECT id FROM news.extraction_jobs WHERE source_article_id=$1 AND expected_intake_version=$2 AND state IN ('queued','running','retry')").bind(article).bind(expected).fetch_optional(&mut *tx).await.map_err(db)?;
        if let Some(id) = existing {
            let status = load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
            tx.commit().await.map_err(db)?;
            return Ok(status);
        }
        let ctx = manual::context(&mut tx, article, Some(expected)).await?;
        if !ctx.may_fetch() || !manual::retry_allowed(&mut tx, &ctx.source_id).await? {
            return Err(AppError::Validation(
                "Current rights, source scope or retry instruction deny extraction.",
            ));
        }
        let id=sqlx::query_scalar::<_,i64>("INSERT INTO news.extraction_jobs(source_article_id,expected_intake_version,source_version,requested_by) VALUES($1,$2,$3,$4) RETURNING id").bind(article).bind(expected).bind(ctx.source_version).bind(actor.id()).fetch_one(&mut *tx).await.map_err(db)?;
        audit(&mut tx, id, actor.id(), "requested", reason, 0).await?;
        let status = load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn cancel(
        &self,
        session: &Session,
        id: i64,
        expected: i64,
        reason: &str,
    ) -> Result<ExtractionStatus, AppError> {
        if id <= 0 || expected < 0 || !valid_reason(reason) {
            return Err(AppError::Validation("Invalid extraction cancellation."));
        }
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let job = load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
        if job.lock_version != expected || !active(&job.state) {
            return Err(AppError::Conflict);
        }
        cancel_job(&mut tx, &job, "operator_cancelled").await?;
        audit(&mut tx, id, actor.id(), "cancelled", reason, expected + 1).await?;
        let status = load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn status(&self, session: &Session, id: i64) -> Result<ExtractionStatus, AppError> {
        if id <= 0 {
            return Err(AppError::Validation("Invalid extraction job ID."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let job = load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
        tx.commit().await.map_err(db)?;
        Ok(job)
    }
    pub async fn history(
        &self,
        session: &Session,
        id: i64,
        before: Option<i64>,
        limit: PageSize,
    ) -> Result<Vec<super::jobs::Attempt>, AppError> {
        if id <= 0 || before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid extraction history cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        load(&mut tx, id, false).await?.ok_or(AppError::NotFound)?;
        let history=sqlx::query_as("SELECT generation,outcome,error_category,started_at,finished_at FROM news.extraction_attempts WHERE job_id=$1 AND ($2::bigint IS NULL OR generation<$2) ORDER BY generation DESC LIMIT $3").bind(id).bind(before).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(history)
    }
}
#[derive(Clone)]
pub struct ExtractionQueue {
    pool: PgPool,
    transport: Arc<dyn ArticleTransport>,
}
impl ExtractionQueue {
    pub fn new(pool: PgPool, public_url: &Url) -> Result<Self, FetchError> {
        Ok(Self {
            pool,
            transport: Arc::new(ArticleClient::new(SafeHttp::new(public_url)?)),
        })
    }
    pub fn with_transport(pool: PgPool, transport: Arc<dyn ArticleTransport>) -> Self {
        Self { pool, transport }
    }
    pub async fn work_once(&self) -> Result<ExtractionOutcome, AppError> {
        let Some(lease) = self.claim().await? else {
            return Ok(ExtractionOutcome::Idle);
        };
        self.run_claimed(&lease).await
    }
    pub async fn claim(&self) -> Result<Option<ExtractionLease>, AppError> {
        if !persistence::schema_available(&self.pool).await {
            return Err(AppError::Unavailable);
        }
        self.recover_expired().await?;
        let ids=sqlx::query_scalar::<_,i64>("SELECT id FROM news.extraction_jobs WHERE state IN ('queued','retry') AND available_at<=clock_timestamp() ORDER BY available_at,id LIMIT 16").fetch_all(&self.pool).await.map_err(db)?;
        for id in ids {
            let mut tx = begin(&self.pool).await?;
            let issuer = issuer(&mut tx, id).await?;
            let Some(job) = load(&mut tx, id, true).await? else {
                continue;
            };
            if !matches!(job.state.as_str(), "queued" | "retry")
                || job.available_at > sql_now(&mut tx).await?
            {
                continue;
            }
            if !issuer || !job.grant_live {
                cancel_job(&mut tx, &job, "authority_expired").await?;
                tx.commit().await.map_err(db)?;
                continue;
            }
            let Some(ctx) = valid_context(&mut tx, &job).await? else {
                cancel_job(&mut tx, &job, "context_changed").await?;
                tx.commit().await.map_err(db)?;
                continue;
            };
            if !manual::retry_allowed(&mut tx, &ctx.source_id).await? {
                sqlx::query("UPDATE news.extraction_jobs SET available_at=clock_timestamp()+interval '60 seconds',last_error='source_retry_wait',lock_version=lock_version+1 WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
                tx.commit().await.map_err(db)?;
                continue;
            }
            let generation = job.generation + 1;
            sqlx::query("UPDATE news.extraction_jobs SET state='running',generation=$2,attempt_count=attempt_count+1,lease_until=clock_timestamp()+interval '60 seconds',lock_version=lock_version+1 WHERE id=$1").bind(id).bind(generation).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO news.extraction_attempts(job_id,generation) VALUES($1,$2)")
                .bind(id)
                .bind(generation)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            tx.commit().await.map_err(db)?;
            return Ok(Some(ExtractionLease { id, generation }));
        }
        Ok(None)
    }
    pub async fn run_claimed(
        &self,
        lease: &ExtractionLease,
    ) -> Result<ExtractionOutcome, AppError> {
        let mut tx = begin(&self.pool).await?;
        let authorized = issuer(&mut tx, lease.id).await?;
        let Some(job) = live_lease(&mut tx, lease).await? else {
            return Ok(ExtractionOutcome::LostLease { job_id: lease.id });
        };
        if !authorized || !job.grant_live {
            cancel_job(&mut tx, &job, "authority_expired").await?;
            tx.commit().await.map_err(db)?;
            return Ok(ExtractionOutcome::Cancelled { job_id: lease.id });
        }
        let Some(ctx) = valid_context(&mut tx, &job).await? else {
            cancel_job(&mut tx, &job, "context_changed").await?;
            tx.commit().await.map_err(db)?;
            return Ok(ExtractionOutcome::Cancelled { job_id: lease.id });
        };
        if !manual::retry_allowed(&mut tx, &ctx.source_id).await? {
            let outcome = retry_or_fail(&mut tx, &job, "source_retry_wait", true, 60, None).await?;
            tx.commit().await.map_err(db)?;
            return Ok(outcome);
        }
        ensure_live(&mut tx, lease).await?;
        tx.commit().await.map_err(db)?;
        let mut result =
            tokio::time::timeout(Duration::from_secs(20), self.transport.extract(&ctx.url))
                .await
                .unwrap_or(Err(ExtractionError::Timeout));
        if let Ok(data) = &result
            && (!data.valid()
                || Url::parse(&data.final_url).ok().map(|u| u.origin())
                    != Url::parse(&ctx.url).ok().map(|u| u.origin()))
        {
            result = Err(ExtractionError::UnsupportedDocument);
        }
        // A real observed source instruction remains conservative metadata even if
        // ownership is cancelled during HTTP. It never authorizes a content write.
        let hint = match &result {
            Err(ExtractionError::Fetch(FetchError::HttpStatus(_, hint))) => *hint,
            _ => None,
        };
        if let Some(seconds) = hint {
            self.record_hint(&ctx.source_id, lease.id, seconds).await?;
        }
        let mut tx = begin(&self.pool).await?;
        let authorized = issuer(&mut tx, lease.id).await?;
        let Some(job) = live_lease(&mut tx, lease).await? else {
            return Ok(ExtractionOutcome::LostLease { job_id: lease.id });
        };
        if !authorized || !job.grant_live {
            cancel_job(&mut tx, &job, "authority_expired").await?;
            tx.commit().await.map_err(db)?;
            return Ok(ExtractionOutcome::Cancelled { job_id: lease.id });
        }
        let Some(current) = valid_context(&mut tx, &job).await? else {
            cancel_job(&mut tx, &job, "context_changed").await?;
            tx.commit().await.map_err(db)?;
            return Ok(ExtractionOutcome::Cancelled { job_id: lease.id });
        };
        let outcome = match result {
            Ok(data) => {
                manual::save_worker(
                    &mut tx,
                    lease.id,
                    &job.source_article_id,
                    &current,
                    "extracted",
                    None,
                    Some(&data),
                )
                .await?;
                ensure_live(&mut tx, lease).await?;
                finish(&mut tx, &job, "succeeded", None, 0).await?;
                ExtractionOutcome::Completed { job_id: lease.id }
            }
            Err(error) => {
                let retryable = transient(error) && hint.is_none_or(|v| v <= 3600);
                let code = if hint.is_some_and(|v| v > 3600) {
                    "retry_after_requires_review"
                } else {
                    error.category()
                };
                retry_or_fail(
                    &mut tx,
                    &job,
                    code,
                    retryable,
                    hint.unwrap_or(0),
                    Some(&current),
                )
                .await?
            }
        };
        tx.commit().await.map_err(db)?;
        Ok(outcome)
    }
    pub async fn recover_expired(&self) -> Result<usize, AppError> {
        let ids=sqlx::query_scalar::<_,i64>("SELECT id FROM news.extraction_jobs WHERE state='running' AND lease_until<=clock_timestamp() ORDER BY lease_until,id LIMIT 32").fetch_all(&self.pool).await.map_err(db)?;
        let mut count = 0;
        for id in ids {
            let mut tx = begin(&self.pool).await?;
            let authorized = issuer(&mut tx, id).await?;
            let Some(job) = load(&mut tx, id, true).await? else {
                continue;
            };
            if job.state != "running" || job.lease_live {
                continue;
            }
            let state = if !authorized || !job.grant_live {
                "cancelled"
            } else if job.attempt_count >= 3 {
                "failed"
            } else {
                "retry"
            };
            sqlx::query("UPDATE news.extraction_attempts SET outcome='lease_expired',error_category='lease_expired',finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(id).bind(job.generation).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("UPDATE news.extraction_jobs SET state=$2,last_error='lease_expired',lease_until=NULL,available_at=clock_timestamp()+interval '10 seconds',lock_version=lock_version+1 WHERE id=$1").bind(id).bind(state).execute(&mut *tx).await.map_err(db)?;
            tx.commit().await.map_err(db)?;
            count += 1;
        }
        Ok(count)
    }
    async fn record_hint(&self, source: &str, job: i64, seconds: u64) -> Result<(), AppError> {
        let mut tx = begin(&self.pool).await?;
        manual::record_retry(&mut tx, source, Some(job), None, seconds).await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}
fn transient(error: ExtractionError) -> bool {
    match error {
        ExtractionError::Busy | ExtractionError::Timeout => true,
        ExtractionError::Fetch(e) => matches!(
            e,
            FetchError::Busy
                | FetchError::Timeout
                | FetchError::Network
                | FetchError::Resolution
                | FetchError::HttpStatus(429, _)
                | FetchError::HttpStatus(500..=599, _)
        ),
        _ => false,
    }
}
async fn valid_context(
    tx: &mut Transaction<'_, Postgres>,
    job: &ExtractionStatus,
) -> Result<Option<Context>, AppError> {
    match manual::context(
        tx,
        &job.source_article_id,
        Some(job.expected_intake_version),
    )
    .await
    {
        Ok(ctx) if ctx.source_version == job.source_version && ctx.may_fetch() => Ok(Some(ctx)),
        Ok(_) | Err(AppError::Conflict) => Ok(None),
        Err(e) => Err(e),
    }
}
async fn retry_or_fail(
    tx: &mut Transaction<'_, Postgres>,
    job: &ExtractionStatus,
    code: &str,
    retry: bool,
    hint: u64,
    ctx: Option<&Context>,
) -> Result<ExtractionOutcome, AppError> {
    let retry = retry && job.attempt_count < 3;
    let state = if retry { "retry" } else { "failed" };
    if !retry && let Some(ctx) = ctx {
        manual::save_worker(
            tx,
            job.id,
            &job.source_article_id,
            ctx,
            "failed",
            Some(code),
            None,
        )
        .await?;
    }
    ensure_live(
        tx,
        &ExtractionLease {
            id: job.id,
            generation: job.generation,
        },
    )
    .await?;
    finish(
        tx,
        job,
        state,
        Some(code),
        (5i64 << job.attempt_count).max(hint.min(3600) as i64),
    )
    .await?;
    Ok(if retry {
        ExtractionOutcome::Deferred { job_id: job.id }
    } else {
        ExtractionOutcome::Failed { job_id: job.id }
    })
}
async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    job: &ExtractionStatus,
    state: &str,
    code: Option<&str>,
    delay: i64,
) -> Result<(), AppError> {
    let changed=sqlx::query("UPDATE news.extraction_attempts SET outcome=$3,error_category=$4,finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(job.id).bind(job.generation).bind(state).bind(code).execute(&mut **tx).await.map_err(db)?;
    if changed.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }
    sqlx::query("UPDATE news.extraction_jobs SET state=$2,last_error=$3,lease_until=NULL,available_at=clock_timestamp()+($4::bigint*interval '1 second'),lock_version=lock_version+1 WHERE id=$1").bind(job.id).bind(state).bind(code).bind(delay).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
async fn cancel_job(
    tx: &mut Transaction<'_, Postgres>,
    job: &ExtractionStatus,
    code: &str,
) -> Result<(), AppError> {
    if job.state == "running" {
        sqlx::query("UPDATE news.extraction_attempts SET outcome='cancelled',error_category=$3,finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(job.id).bind(job.generation).bind(code).execute(&mut **tx).await.map_err(db)?;
    }
    sqlx::query("UPDATE news.extraction_jobs SET state='cancelled',last_error=$2,lease_until=NULL,generation=generation+1,lock_version=lock_version+1 WHERE id=$1").bind(job.id).bind(code).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
const SELECT: &str = "SELECT id,source_article_id,requested_by,authorized_until,state,generation,attempt_count,lock_version,last_error,available_at,lease_until,expected_intake_version,source_version,authorized_until>clock_timestamp() AS grant_live,COALESCE(lease_until>clock_timestamp(),false) AS lease_live FROM news.extraction_jobs";
async fn load(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
    skip: bool,
) -> Result<Option<ExtractionStatus>, AppError> {
    sqlx::query_as(&format!(
        "{SELECT} WHERE id=$1 FOR UPDATE {}",
        if skip { "SKIP LOCKED" } else { "" }
    ))
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)
}
async fn issuer(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<bool, AppError> {
    Ok(
        sqlx::query_scalar::<_, Option<bool>>("SELECT news.lock_extraction_requester($1)")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?
            .unwrap_or(false),
    )
}
async fn live_lease(
    tx: &mut Transaction<'_, Postgres>,
    lease: &ExtractionLease,
) -> Result<Option<ExtractionStatus>, AppError> {
    let job = load(tx, lease.id, false).await?;
    Ok(match job {
        Some(j) if j.state == "running" && j.generation == lease.generation && j.lease_live => {
            Some(j)
        }
        _ => None,
    })
}
async fn ensure_live(
    tx: &mut Transaction<'_, Postgres>,
    lease: &ExtractionLease,
) -> Result<(), AppError> {
    let valid=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM news.extraction_jobs WHERE id=$1 AND generation=$2 AND state='running' AND lease_until>clock_timestamp() AND authorized_until>clock_timestamp())").bind(lease.id).bind(lease.generation).fetch_one(&mut **tx).await.map_err(db)?;
    if !valid {
        return Err(AppError::Conflict);
    }
    Ok(())
}
async fn sql_now(tx: &mut Transaction<'_, Postgres>) -> Result<DateTime<Utc>, AppError> {
    sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(db)
}
async fn begin(pool: &PgPool) -> Result<Transaction<'static, Postgres>, AppError> {
    let mut tx = pool.begin().await.map_err(db)?;
    sqlx::query("SET LOCAL lock_timeout='3s'")
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("SET LOCAL statement_timeout='5s'")
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    Ok(tx)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    job: i64,
    actor: &str,
    action: &str,
    reason: &str,
    version: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.extraction_job_actions(job_id,actor_id,action,reason,job_version) VALUES($1,$2,$3,$4,$5)").bind(job).bind(actor).bind(action).bind(reason).bind(version).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
fn active(state: &str) -> bool {
    matches!(state, "queued" | "running" | "retry")
}
fn valid_reason(reason: &str) -> bool {
    !reason.trim().is_empty() && reason.chars().count() <= 2000 && !reason.contains('\0')
}
fn db(e: sqlx::Error) -> AppError {
    AppError::database("extraction_job", e)
}
