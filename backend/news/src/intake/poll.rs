//! Explicit configured polling; no background loop or publication side effect.
use super::{
    feed::{FeedClient, FeedResult},
    http::{FetchError, SafeHttp, Validators},
};
use crate::{content::valid_id, error::AppError, persistence};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use url::Url;

pub type FetchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<FeedResult, FetchError>> + Send + 'a>>;
/// Trusted transport injection for fixture tests/other policy-aware adapters.
/// Production construction uses FeedClient with the shared SafeHttp boundary.
pub trait FeedTransport: Send + Sync {
    fn fetch<'a>(
        &'a self,
        url: &'a str,
        max_bytes: usize,
        validators: &'a Validators,
    ) -> FetchFuture<'a>;
}
impl FeedTransport for FeedClient {
    fn fetch<'a>(
        &'a self,
        url: &'a str,
        max_bytes: usize,
        validators: &'a Validators,
    ) -> FetchFuture<'a> {
        Box::pin(FeedClient::fetch(self, url, max_bytes, validators))
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    Disabled,
    Paused,
    NotDue,
    Cancelled,
    Unchanged { attempt_id: i64 },
    Modified { attempt_id: i64, entries: usize },
    Failed { attempt_id: i64, error: FetchError },
}
#[derive(Clone)]
pub struct FeedPoller {
    pool: PgPool,
    transport: Arc<dyn FeedTransport>,
}
#[derive(sqlx::FromRow)]
struct Claim {
    id: String,
    source_id: String,
    url: String,
    poll_seconds: i32,
    max_response_bytes: i32,
    etag: Option<String>,
    last_modified: Option<String>,
    validator_url: Option<String>,
    feed_version: i64,
    source_version: i64,
    failure_count: i32,
}
impl FeedPoller {
    pub fn new(pool: PgPool, public_url: &Url) -> Result<Self, FetchError> {
        Ok(Self {
            pool,
            transport: Arc::new(FeedClient::new(SafeHttp::new(public_url)?)),
        })
    }
    pub fn with_transport(pool: PgPool, transport: Arc<dyn FeedTransport>) -> Self {
        Self { pool, transport }
    }
    pub async fn poll_due(&self, id: &str) -> Result<PollOutcome, AppError> {
        if !valid_id(id) {
            return Err(AppError::Validation("Invalid feed ID."));
        }
        if !persistence::schema_available(&self.pool).await {
            return Err(AppError::Unavailable);
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        // This atomic due reservation serialises same-feed callers without holding
        // a transaction/connection open while the HTTP client waits.
        let claim=sqlx::query_as::<_,Claim>("UPDATE news.feeds f SET next_fetch_at=now()+make_interval(secs=>f.poll_seconds),last_attempt_at=now(),last_outcome='started',last_error=NULL FROM news.sources s WHERE f.id=$1 AND s.id=f.source_id AND f.enabled AND s.enabled AND NOT f.fetch_paused AND f.next_fetch_at<=now() RETURNING f.id,f.source_id,f.url,f.poll_seconds,f.max_response_bytes,f.etag,f.last_modified,f.validator_url,f.lock_version AS feed_version,s.lock_version AS source_version,f.failure_count")
            .bind(id).fetch_optional(&mut *tx).await.map_err(db)?;
        let Some(claim) = claim else {
            let flags=sqlx::query_as::<_,(bool,bool)>("SELECT f.enabled AND s.enabled,f.fetch_paused FROM news.feeds f JOIN news.sources s ON s.id=f.source_id WHERE f.id=$1")
                .bind(id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or(AppError::NotFound)?;
            tx.commit().await.map_err(db)?;
            return Ok(if !flags.0 {
                PollOutcome::Disabled
            } else if flags.1 {
                PollOutcome::Paused
            } else {
                PollOutcome::NotDue
            });
        };
        if !(60..=86400).contains(&claim.poll_seconds)
            || !(16384..=10485760).contains(&claim.max_response_bytes)
            || !(0..=32).contains(&claim.failure_count)
        {
            return Err(AppError::InvalidData);
        }
        let attempt=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(feed_id,source_id,feed_version,source_version) VALUES ($1,$2,$3,$4) RETURNING id")
            .bind(&claim.id).bind(&claim.source_id).bind(claim.feed_version).bind(claim.source_version).fetch_one(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE news.feeds SET last_attempt_id=$2 WHERE id=$1")
            .bind(id)
            .bind(attempt)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        let validators = Validators {
            url: claim.validator_url.clone(),
            etag: claim.etag.clone(),
            last_modified: claim.last_modified.clone(),
        };
        let fetched = tokio::time::timeout(
            Duration::from_secs(20),
            self.transport
                .fetch(&claim.url, claim.max_response_bytes as usize, &validators),
        )
        .await
        .unwrap_or(Err(FetchError::Timeout));
        self.finish(claim, attempt, fetched).await
    }
    async fn finish(
        &self,
        claim: Claim,
        attempt: i64,
        mut fetched: Result<FeedResult, FetchError>,
    ) -> Result<PollOutcome, AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Lock source before feed so configuration commits have a clear ordering.
        let source = sqlx::query_as::<_, (bool, i64)>(
            "SELECT enabled,lock_version FROM news.lock_source_for_fetch($1)",
        )
        .bind(&claim.source_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let feed = sqlx::query_as::<_, (bool, i64, Option<i64>)>(
            "SELECT enabled,lock_version,last_attempt_id FROM news.feeds WHERE id=$1 FOR UPDATE",
        )
        .bind(&claim.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !source.0
            || !feed.0
            || source.1 != claim.source_version
            || feed.1 != claim.feed_version
            || feed.2 != Some(attempt)
        {
            sqlx::query("UPDATE news.feed_fetch_attempts SET outcome='cancelled',error_category='configuration_changed',finished_at=now() WHERE id=$1").bind(attempt).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("UPDATE news.feeds SET last_outcome='cancelled',last_error='configuration_changed' WHERE id=$1 AND last_attempt_id=$2").bind(&claim.id).bind(attempt).execute(&mut *tx).await.map_err(db)?;
            tx.commit().await.map_err(db)?;
            return Ok(PollOutcome::Cancelled);
        }
        if matches!(&fetched, Ok(FeedResult::NotModified(_)))
            && (claim.validator_url.is_none()
                || (claim.etag.is_none() && claim.last_modified.is_none()))
        {
            fetched = Err(FetchError::InvalidResponse);
        }
        if let Ok(FeedResult::Fetched(data)) = &fetched
            && (data.entries.len() > 500 || data.response_bytes > claim.max_response_bytes as usize)
        {
            fetched = Err(FetchError::TooLarge);
        }
        let mut batch = None;
        if let Ok(FeedResult::Fetched(data)) = &fetched {
            let metadata = json!({"schema":1,"source_id":claim.source_id,"feed_id":claim.id,"source_version":claim.source_version,"feed_version":claim.feed_version,"feed":data});
            if metadata.to_string().len() > 4_000_000 {
                fetched = Err(FetchError::TooLarge);
            } else {
                batch = Some(metadata);
            }
        }
        let (outcome, error, status, bytes, entries, cache, failures, delay, paused, result) =
            match fetched {
                Ok(FeedResult::Fetched(data)) => {
                    let entries = data.entries.len();
                    (
                        "modified",
                        None,
                        Some(200i32),
                        Some(data.response_bytes as i64),
                        Some(entries as i32),
                        Some(data.validators),
                        0,
                        i64::from(claim.poll_seconds),
                        false,
                        PollOutcome::Modified {
                            attempt_id: attempt,
                            entries,
                        },
                    )
                }
                Ok(FeedResult::NotModified(cache)) => (
                    "not_modified",
                    None,
                    Some(304),
                    None,
                    None,
                    Some(cache),
                    0,
                    i64::from(claim.poll_seconds),
                    false,
                    PollOutcome::Unchanged {
                        attempt_id: attempt,
                    },
                ),
                Err(error) => {
                    let failures = (claim.failure_count + 1).min(32);
                    let backoff =
                        (i64::from(claim.poll_seconds) * (1i64 << failures.min(10))).min(604800);
                    let (status, hint) = match error {
                        FetchError::HttpStatus(code, hint) => (Some(i32::from(code)), hint),
                        _ => (None, None),
                    };
                    let paused = hint.is_some_and(|s| s > 604800);
                    let delay = backoff.max(hint.filter(|s| *s <= 604800).unwrap_or(0) as i64);
                    (
                        "failed",
                        Some(if paused {
                            "retry_after_requires_review"
                        } else {
                            error.category()
                        }),
                        status,
                        None,
                        None,
                        None,
                        failures,
                        delay,
                        paused,
                        PollOutcome::Failed {
                            attempt_id: attempt,
                            error,
                        },
                    )
                }
            };
        if let Some(metadata) = batch {
            let hash = format!("{:x}", Sha256::digest(metadata.to_string().as_bytes()));
            sqlx::query(
                "INSERT INTO news.feed_batches(attempt_id,metadata,content_hash) VALUES ($1,$2,$3)",
            )
            .bind(attempt)
            .bind(Json(metadata))
            .bind(hash)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query("UPDATE news.feed_fetch_attempts SET outcome=$2,error_category=$3,http_status=$4,response_bytes=$5,entry_count=$6,finished_at=now() WHERE id=$1")
            .bind(attempt).bind(outcome).bind(error).bind(status).bind(bytes).bind(entries).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE news.feeds SET last_outcome=$2,last_error=$3,last_http_status=$4,failure_count=$5,fetch_paused=$6,next_fetch_at=now()+($7::bigint*interval '1 second'),last_success_at=CASE WHEN $2 IN ('modified','not_modified') THEN now() ELSE last_success_at END WHERE id=$1")
            .bind(&claim.id).bind(outcome).bind(error).bind(status).bind(failures).bind(paused).bind(delay).execute(&mut *tx).await.map_err(db)?;
        if let Some(cache) = cache {
            sqlx::query(
                "UPDATE news.feeds SET etag=$2,last_modified=$3,validator_url=$4 WHERE id=$1",
            )
            .bind(&claim.id)
            .bind(cache.etag)
            .bind(cache.last_modified)
            .bind(cache.url)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(result)
    }
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("feed_poll", error)
}
