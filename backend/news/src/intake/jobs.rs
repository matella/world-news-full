//! Durable normalization only. Fetch/extraction authority and scheduling are separate gates.
use super::normalize::{NormalizeOutcome, Normalizer};
use crate::{
    auth::{self, Permission, Session},
    content::PageSize,
    error::AppError,
    persistence,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug)]
pub struct Lease {
    job_id: i64,
    generation: i64,
    batch_id: i64,
}
impl Lease {
    pub fn job_id(&self) -> i64 {
        self.job_id
    }
    pub fn batch_id(&self) -> i64 {
        self.batch_id
    }
}
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct JobStatus {
    pub id: i64,
    pub batch_id: i64,
    pub state: String,
    pub generation: i64,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub replay_count: i64,
    pub lock_version: i64,
    pub last_error: Option<String>,
    pub available_at: DateTime<Utc>,
    pub lease_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct Attempt {
    pub generation: i64,
    pub outcome: String,
    pub error_category: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct QueueStats {
    pub queued: i64,
    pub running: i64,
    pub retry: i64,
    pub failed: i64,
    pub succeeded: i64,
    pub cancelled: i64,
    pub oldest_ready_seconds: Option<f64>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkOutcome {
    Idle,
    Completed { job_id: i64 },
    Deferred { job_id: i64 },
    Failed { job_id: i64 },
    LostLease { job_id: i64 },
}
#[derive(Clone)]
pub struct Queue {
    pool: PgPool,
}
impl Queue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    /// One explicit claim/handler; never a hidden daemon or a publishing operation.
    pub async fn work_once(&self) -> Result<WorkOutcome, AppError> {
        let Some(lease) = self.claim().await? else {
            return Ok(WorkOutcome::Idle);
        };
        self.run_claimed(&lease).await
    }
    /// Execute a previously claimed lease; the handler and success acknowledgement
    /// share a transaction. Stale/cancelled leases cannot produce new articles.
    pub async fn run_claimed(&self, lease: &Lease) -> Result<WorkOutcome, AppError> {
        match Normalizer::new(self.pool.clone())
            .apply_claimed(lease)
            .await
        {
            Ok(NormalizeOutcome::Applied(_)) => Ok(WorkOutcome::Completed {
                job_id: lease.job_id,
            }),
            result => {
                let code = match result {
                    Ok(NormalizeOutcome::HeldDisabled) => "held_disabled",
                    Err(AppError::InvalidData) => "invalid_data",
                    Err(AppError::Conflict) => "conflict",
                    Err(AppError::Forbidden) => "forbidden",
                    Err(AppError::Validation(_)) => "constraint",
                    _ => "unavailable",
                };
                match self.fail(lease, code).await {
                    Ok(true) => Ok(WorkOutcome::Failed {
                        job_id: lease.job_id,
                    }),
                    Ok(false) => Ok(WorkOutcome::Deferred {
                        job_id: lease.job_id,
                    }),
                    Err(AppError::Conflict) => Ok(WorkOutcome::LostLease {
                        job_id: lease.job_id,
                    }),
                    Err(e) => Err(e),
                }
            }
        }
    }
    pub async fn claim(&self) -> Result<Option<Lease>, AppError> {
        if !persistence::schema_available(&self.pool).await {
            return Err(AppError::Unavailable);
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Bound recovery work; SKIP LOCKED allows unrelated workers to progress.
        let expired=sqlx::query_as::<_,(i64,i64,i32,i32)>("SELECT id,generation,attempt_count,max_attempts FROM news.normalization_jobs WHERE state='running' AND lease_until<=clock_timestamp() ORDER BY lease_until,id LIMIT 32 FOR UPDATE SKIP LOCKED").fetch_all(&mut *tx).await.map_err(db)?;
        for (id, generation, count, max) in expired {
            sqlx::query("UPDATE news.normalization_attempts SET outcome='lease_expired',error_category='lease_expired',finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(id).bind(generation).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("UPDATE news.normalization_jobs SET state=$2,lease_until=NULL,last_error='lease_expired',available_at=clock_timestamp()+($3::bigint*interval '1 second'),lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(if count>=max{"failed"}else{"retry"}).bind(backoff(count)).execute(&mut *tx).await.map_err(db)?;
        }
        let selected=sqlx::query_as::<_,(i64,i64,i64)>("SELECT id,generation,batch_id FROM news.normalization_jobs WHERE state IN ('queued','retry') AND available_at<=clock_timestamp() AND attempt_count<max_attempts ORDER BY available_at,id LIMIT 1 FOR UPDATE SKIP LOCKED").fetch_optional(&mut *tx).await.map_err(db)?;
        let lease = if let Some((id, generation, batch_id)) = selected {
            let generation = generation.checked_add(1).ok_or(AppError::Conflict)?;
            sqlx::query("UPDATE news.normalization_jobs SET state='running',generation=$2,attempt_count=attempt_count+1,lease_until=clock_timestamp()+interval '60 seconds',lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(generation).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO news.normalization_attempts(job_id,generation) VALUES($1,$2)")
                .bind(id)
                .bind(generation)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            Some(Lease {
                job_id: id,
                generation,
                batch_id,
            })
        } else {
            None
        };
        tx.commit().await.map_err(db)?;
        Ok(lease)
    }
    pub async fn heartbeat(&self, lease: &Lease) -> Result<(), AppError> {
        let changed=sqlx::query("UPDATE news.normalization_jobs SET lease_until=clock_timestamp()+interval '60 seconds',updated_at=clock_timestamp() WHERE id=$1 AND generation=$2 AND state='running' AND lease_until>clock_timestamp()").bind(lease.job_id).bind(lease.generation).execute(&self.pool).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::Conflict);
        }
        Ok(())
    }
    async fn fail(&self, lease: &Lease, code: &'static str) -> Result<bool, AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_lease(&mut tx, lease).await?;
        let (count, max) = sqlx::query_as::<_, (i32, i32)>(
            "SELECT attempt_count,max_attempts FROM news.normalization_jobs WHERE id=$1",
        )
        .bind(lease.job_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let state = if count >= max { "failed" } else { "retry" };
        sqlx::query("UPDATE news.normalization_attempts SET outcome=$3,error_category=$4,finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(lease.job_id).bind(lease.generation).bind(state).bind(code).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE news.normalization_jobs SET state=$2,lease_until=NULL,last_error=$3,available_at=clock_timestamp()+($4::bigint*interval '1 second'),lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(lease.job_id).bind(state).bind(code).bind(backoff(count)).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(count >= max)
    }
}
fn backoff(count: i32) -> i64 {
    (5i64 * (1i64 << count.clamp(0, 5))).min(300)
}
/// The handler holds this row lock through its writes. Cancellation/reclaim orders
/// before the handler (no writes) or after its commit (completed work cannot vanish).
pub(crate) async fn lock_lease(
    tx: &mut Transaction<'_, Postgres>,
    lease: &Lease,
) -> Result<(), AppError> {
    let valid=sqlx::query_scalar::<_,i64>("SELECT id FROM news.normalization_jobs WHERE id=$1 AND generation=$2 AND batch_id=$3 AND state='running' AND lease_until>clock_timestamp() FOR UPDATE").bind(lease.job_id).bind(lease.generation).bind(lease.batch_id).fetch_optional(&mut **tx).await.map_err(db)?;
    if valid.is_none() {
        return Err(AppError::Conflict);
    }
    Ok(())
}
pub(crate) async fn finish_success(
    tx: &mut Transaction<'_, Postgres>,
    lease: &Lease,
) -> Result<(), AppError> {
    // Recheck deadline just before commit, not only before a long batch transaction.
    lock_lease(tx, lease).await?;
    let changed=sqlx::query("UPDATE news.normalization_attempts SET outcome='succeeded',finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(lease.job_id).bind(lease.generation).execute(&mut **tx).await.map_err(db)?;
    if changed.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }
    sqlx::query("UPDATE news.normalization_jobs SET state='succeeded',lease_until=NULL,last_error=NULL,lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(lease.job_id).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
#[derive(Clone)]
pub struct JobAdmin {
    pool: PgPool,
}
impl JobAdmin {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn enqueue(
        &self,
        session: &Session,
        batch_id: i64,
        max_attempts: i32,
        reason: &str,
    ) -> Result<JobStatus, AppError> {
        if batch_id <= 0 || !(1..=5).contains(&max_attempts) || !valid_reason(reason) {
            return Err(AppError::Validation("Invalid job request."));
        }
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let inserted=sqlx::query_scalar::<_,i64>("INSERT INTO news.normalization_jobs(batch_id,max_attempts) VALUES($1,$2) ON CONFLICT(batch_id) DO NOTHING RETURNING id").bind(batch_id).bind(max_attempts).fetch_optional(&mut *tx).await.map_err(db)?;
        let id = match inserted {
            Some(id) => {
                audit(&mut tx, id, actor.id(), "enqueued", reason, 0).await?;
                id
            }
            None => sqlx::query_scalar("SELECT id FROM news.normalization_jobs WHERE batch_id=$1")
                .bind(batch_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?,
        };
        let status = load(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn replay(
        &self,
        session: &Session,
        id: i64,
        expected: i64,
        reason: &str,
    ) -> Result<JobStatus, AppError> {
        check_change(id, expected, reason)?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let status = load(&mut tx, id).await?;
        if status.lock_version != expected
            || !matches!(status.state.as_str(), "succeeded" | "failed" | "cancelled")
        {
            return Err(AppError::Conflict);
        }
        sqlx::query("UPDATE news.normalization_jobs SET state='queued',attempt_count=0,replay_count=replay_count+1,lease_until=NULL,last_error=NULL,available_at=clock_timestamp(),lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, id, actor.id(), "replayed", reason, expected + 1).await?;
        let status = load(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn cancel(
        &self,
        session: &Session,
        id: i64,
        expected: i64,
        reason: &str,
    ) -> Result<JobStatus, AppError> {
        check_change(id, expected, reason)?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let status = load(&mut tx, id).await?;
        if status.lock_version != expected
            || !matches!(status.state.as_str(), "queued" | "retry" | "running")
        {
            return Err(AppError::Conflict);
        }
        if status.state == "running" {
            sqlx::query("UPDATE news.normalization_attempts SET outcome='cancelled',finished_at=clock_timestamp() WHERE job_id=$1 AND generation=$2 AND outcome='started'").bind(id).bind(status.generation).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE news.normalization_jobs SET state='cancelled',lease_until=NULL,generation=generation+1,lock_version=lock_version+1,updated_at=clock_timestamp() WHERE id=$1").bind(id).execute(&mut *tx).await.map_err(db)?;
        audit(&mut tx, id, actor.id(), "cancelled", reason, expected + 1).await?;
        let status = load(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn status(&self, session: &Session, id: i64) -> Result<JobStatus, AppError> {
        if id <= 0 {
            return Err(AppError::Validation("Invalid job ID."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let status = load(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(status)
    }
    pub async fn history(
        &self,
        session: &Session,
        id: i64,
        before: Option<i64>,
        limit: PageSize,
    ) -> Result<Vec<Attempt>, AppError> {
        if id <= 0 || before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid job history cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        load(&mut tx, id).await?;
        let rows=sqlx::query_as::<_,Attempt>("SELECT generation,outcome,error_category,started_at,finished_at FROM news.normalization_attempts WHERE job_id=$1 AND ($2::bigint IS NULL OR generation<$2) ORDER BY generation DESC LIMIT $3").bind(id).bind(before).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn stats(&self, session: &Session) -> Result<QueueStats, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let stats=sqlx::query_as::<_,QueueStats>("SELECT count(*) FILTER(WHERE state='queued') AS queued,count(*) FILTER(WHERE state='running') AS running,count(*) FILTER(WHERE state='retry') AS retry,count(*) FILTER(WHERE state='failed') AS failed,count(*) FILTER(WHERE state='succeeded') AS succeeded,count(*) FILTER(WHERE state='cancelled') AS cancelled,extract(epoch FROM clock_timestamp()-min(available_at) FILTER(WHERE state IN ('queued','retry') AND available_at<=clock_timestamp()))::float8 AS oldest_ready_seconds FROM news.normalization_jobs").fetch_one(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(stats)
    }
}
async fn load(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<JobStatus, AppError> {
    sqlx::query_as("SELECT id,batch_id,state,generation,attempt_count,max_attempts,replay_count,lock_version,last_error,available_at,lease_until,created_at FROM news.normalization_jobs WHERE id=$1 FOR UPDATE").bind(id).fetch_one(&mut **tx).await.map_err(db)
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
    actor: &str,
    action: &str,
    reason: &str,
    version: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.normalization_job_actions(job_id,actor_id,action,reason,job_version) VALUES($1,$2,$3,$4,$5)").bind(id).bind(actor).bind(action).bind(reason).bind(version).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
fn valid_reason(reason: &str) -> bool {
    !reason.trim().is_empty() && reason.chars().count() <= 2000 && !reason.contains('\0')
}
fn check_change(id: i64, version: i64, reason: &str) -> Result<(), AppError> {
    if id <= 0 || version < 0 || !valid_reason(reason) {
        Err(AppError::Validation("Invalid job change or reason."))
    } else {
        Ok(())
    }
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("normalization_job", error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backoff_is_bounded_and_increases() {
        assert_eq!(backoff(1), 10);
        assert!(backoff(2) > backoff(1));
        assert!(backoff(100) <= 300);
        assert!(!valid_reason(" "));
        assert!(!valid_reason("bad\0reason"));
    }
}
