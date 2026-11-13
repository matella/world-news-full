use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use world_news::{
    config::Config,
    error::AppError,
    intake::{
        feed::{Entry, FeedData, FeedResult},
        http::{FetchError, Validators},
        poll::*,
    },
    persistence,
};

struct Fixture {
    calls: AtomicUsize,
    mode: AtomicUsize,
    owner: PgPool,
}
impl FeedTransport for Fixture {
    fn fetch<'a>(&'a self, url: &'a str, _: usize, validators: &'a Validators) -> FetchFuture<'a> {
        Box::pin(async move {
            let number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            match self.mode.load(Ordering::SeqCst) {
                1 => return Ok(FeedResult::NotModified(validators.clone())),
                2 => return Err(FetchError::Timeout),
                3 => return Err(FetchError::HttpStatus(429, Some(7200))),
                4 => return Err(FetchError::HttpStatus(503, Some(604801))),
                5 => {
                    sqlx::query("UPDATE news.sources SET enabled=false,lock_version=lock_version+1 WHERE id='source-a'").execute(&self.owner).await.unwrap();
                }
                _ => {}
            }
            Ok(FeedResult::Fetched(FeedData {
                final_url: url.into(),
                format: "rss2".into(),
                response_bytes: 200,
                title: Some("Fixture feed".into()),
                entries: vec![Entry {
                    parser_id: "parser-item".into(),
                    title: Some("Fixture item".into()),
                    links: vec!["https://source.invalid/article".into()],
                    published: None,
                    updated: None,
                    language: Some("fr".into()),
                    authors: vec![],
                }],
                validators: Validators {
                    url: Some(url.into()),
                    etag: Some(format!("\"fixture-{number}\"")),
                    last_modified: None,
                },
            }))
        })
    }
}
fn config(url: &str) -> Config {
    Config::from_lookup(|key| (key == "DATABASE_URL").then(|| url.into())).unwrap()
}
async fn due(pool: &PgPool) {
    sqlx::query("UPDATE news.feeds SET next_fetch_at=now()-interval '1 second',retry_after_observed_at=CASE WHEN retry_after_seconds IS NULL THEN NULL ELSE now()-interval '8 days' END WHERE id='feed-a'")
        .execute(pool)
        .await
        .unwrap();
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn feed_polling_reserves_cadence_and_atomically_preserves_batches_before_cache() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_feed"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url));
    assert!(!persistence::schema_available(&owner).await);
    persistence::migrate(&owner).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES ('source-a','Fixture source','https://source.invalid',true);
        INSERT INTO news.feeds(id,source_id,url,enabled,poll_seconds) VALUES ('feed-a','source-a','https://source.invalid/rss',true,60);
        CREATE ROLE fixture_fetch_worker LOGIN PASSWORD 'synthetic-fetch-worker'; GRANT USAGE ON SCHEMA news TO fixture_fetch_worker;
        GRANT SELECT ON news.schema_version,news.feeds,news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_job_origins TO fixture_fetch_worker;
        GRANT SELECT(id,enabled,lock_version) ON news.sources TO fixture_fetch_worker;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text) TO fixture_fetch_worker;
        GRANT UPDATE(next_fetch_at,last_attempt_at,last_outcome,last_error,last_http_status,failure_count,fetch_paused,last_success_at,last_attempt_id,etag,last_modified,validator_url,fetch_control_version,fetch_pause_reason,retry_after_seconds,retry_after_observed_at) ON news.feeds TO fixture_fetch_worker;
        GRANT INSERT ON news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_job_origins TO fixture_fetch_worker;
        GRANT UPDATE(outcome,error_category,http_status,response_bytes,entry_count,finished_at,lease_until) ON news.feed_fetch_attempts TO fixture_fetch_worker;
        GRANT USAGE ON SEQUENCE news.feed_fetch_attempts_id_seq,news.normalization_jobs_id_seq TO fixture_fetch_worker;
        CREATE ROLE fixture_fetch_reader LOGIN PASSWORD 'synthetic-fetch-reader'; GRANT USAGE ON SCHEMA news TO fixture_fetch_reader;
        GRANT SELECT ON news.schema_version TO fixture_fetch_reader;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_fetch_worker").unwrap();
    url.set_password(Some("synthetic-fetch-worker")).unwrap();
    let worker = persistence::pool(&config(url.as_str()));
    url.set_username("fixture_fetch_reader").unwrap();
    url.set_password(Some("synthetic-fetch-reader")).unwrap();
    let reader = persistence::pool(&config(url.as_str()));
    assert!(
        sqlx::query("SELECT * FROM news.feed_batches")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("SELECT * FROM news.lock_source_for_fetch('source-a')")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE news.sources SET enabled=false WHERE id='source-a'")
            .execute(&worker)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO news.stories(id) VALUES ('must-not-publish')")
            .execute(&worker)
            .await
            .is_err()
    );
    let fixture = Arc::new(Fixture {
        calls: AtomicUsize::new(0),
        mode: AtomicUsize::new(0),
        owner: owner.clone(),
    });
    let poller = FeedPoller::with_transport(worker.clone(), fixture.clone());
    let (left, right) = tokio::join!(poller.poll_due("feed-a"), poller.poll_due("feed-a"));
    let first = match (left.unwrap(), right.unwrap()) {
        (
            PollOutcome::Modified {
                attempt_id,
                entries: 1,
            },
            PollOutcome::NotDue,
        )
        | (
            PollOutcome::NotDue,
            PollOutcome::Modified {
                attempt_id,
                entries: 1,
            },
        ) => attempt_id,
        _ => panic!("one due reservation and one transport call expected"),
    };
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert_eq!(count(&owner, "feed_batches").await, 1);
    let first_cache =
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='feed-a'")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(first_cache, "\"fixture-1\"");
    assert!(
        sqlx::query("UPDATE news.feed_fetch_attempts SET error_category='rewrite' WHERE id=$1")
            .bind(first)
            .execute(&owner)
            .await
            .is_err()
    );
    let batch = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT metadata FROM news.feed_batches WHERE attempt_id=$1",
    )
    .bind(first)
    .fetch_one(&worker)
    .await
    .unwrap();
    assert_eq!(batch["feed"]["entries"][0]["title"], "Fixture item");
    assert!(batch["feed"].get("validators").is_none());
    assert_eq!(batch["source_version"], 0);
    assert_eq!(batch["feed"]["response_bytes"], 200);
    assert_eq!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::NotDue
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    due(&owner).await;
    fixture.mode.store(1, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Unchanged { .. }
    ));
    assert_eq!(count(&owner, "feed_batches").await, 1);
    assert!(sqlx::query_scalar::<_,bool>("SELECT last_success_at IS NOT NULL AND last_http_status=304 FROM news.feeds WHERE id='feed-a'").fetch_one(&owner).await.unwrap());
    due(&owner).await;
    fixture.mode.store(2, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Failed {
            error: FetchError::Timeout,
            ..
        }
    ));
    assert!(sqlx::query_scalar::<_,bool>("SELECT last_error='timeout' AND failure_count=1 AND next_fetch_at>=now()+interval '119 seconds' FROM news.feeds WHERE id='feed-a'").fetch_one(&owner).await.unwrap());
    due(&owner).await;
    fixture.mode.store(3, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Failed {
            error: FetchError::HttpStatus(429, Some(7200)),
            ..
        }
    ));
    assert!(sqlx::query_scalar::<_,bool>("SELECT last_http_status=429 AND next_fetch_at>=now()+interval '7199 seconds' FROM news.feeds WHERE id='feed-a'").fetch_one(&owner).await.unwrap());
    due(&owner).await;
    fixture.mode.store(4, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Failed { .. }
    ));
    assert!(sqlx::query_scalar::<_,bool>("SELECT fetch_paused AND last_error='retry_after_requires_review' FROM news.feeds WHERE id='feed-a'").fetch_one(&owner).await.unwrap());
    due(&owner).await;
    assert_eq!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Paused
    );
    sqlx::query("UPDATE news.feeds SET fetch_paused=false WHERE id='feed-a'")
        .execute(&owner)
        .await
        .unwrap();
    fixture.mode.store(5, Ordering::SeqCst);
    assert_eq!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Cancelled
    );
    assert_eq!(count(&owner, "feed_batches").await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='feed-a'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        first_cache
    );
    let calls = fixture.calls.load(Ordering::SeqCst);
    assert_eq!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Disabled
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), calls);
    sqlx::query(
        "UPDATE news.sources SET enabled=true,lock_version=lock_version+1 WHERE id='source-a'",
    )
    .execute(&owner)
    .await
    .unwrap();
    due(&owner).await;
    fixture.mode.store(0, Ordering::SeqCst);
    sqlx::raw_sql("CREATE FUNCTION news.test_reject_feed_batch() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic batch interruption' USING ERRCODE='23514'; END $$; CREATE TRIGGER test_reject_feed_batch BEFORE INSERT ON news.feed_batches FOR EACH ROW EXECUTE FUNCTION news.test_reject_feed_batch();").execute(&owner).await.unwrap();
    assert!(poller.poll_due("feed-a").await.is_err());
    assert_eq!(count(&owner, "feed_batches").await, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='feed-a'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        first_cache
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT last_outcome FROM news.feeds WHERE id='feed-a'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        "started"
    );
    sqlx::raw_sql("DROP TRIGGER test_reject_feed_batch ON news.feed_batches;DROP FUNCTION news.test_reject_feed_batch();").execute(&owner).await.unwrap();
    // A storage failure leaves a started leased attempt; it must be recovered,
    // not silently replaced by a fresh due reservation.
    assert_eq!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::NotDue
    );
    sqlx::query("UPDATE news.feed_fetch_attempts SET lease_until=clock_timestamp()-interval '1 second' WHERE outcome='started'").execute(&owner).await.unwrap();
    assert_eq!(poller.recover_expired().await.unwrap(), 1);

    due(&owner).await;
    assert!(matches!(
        poller.poll_due("feed-a").await.unwrap(),
        PollOutcome::Modified { .. }
    ));
    assert_eq!(count(&owner, "feed_batches").await, 2);
    assert_ne!(
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='feed-a'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        first_cache
    );
    assert!(
        sqlx::query("UPDATE news.feed_batches SET content_hash=repeat('0',64)")
            .execute(&owner)
            .await
            .is_err()
    );
    assert!(matches!(
        poller.poll_due("missing").await,
        Err(AppError::NotFound)
    ));
    assert_eq!(count(&owner, "source_articles").await, 0);
    assert_eq!(count(&owner, "stories").await, 0);
    reader.close().await;
    worker.close().await;
    owner.close().await;
}
