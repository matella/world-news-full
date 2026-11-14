use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use serde_json::Value;
use sqlx::PgPool;
use std::{
    path::Path,
    process::{Child, Command},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::PageSize,
    error::AppError,
    intake::{
        feed::{Entry, FeedData, FeedResult},
        feed_admin::*,
        http::{FetchError, Validators},
        jobs::{Queue, WorkOutcome},
        poll::*,
    },
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic feed job passphrase only";
fn config(reader: &str, writer: Option<&str>) -> Config {
    Config::from_lookup(|k| match k {
        "DATABASE_URL" => Some(reader.into()),
        "EDITOR_DATABASE_URL" => writer.map(str::to_owned),
        "NEWS_ADMIN_ENABLED" => Some(if writer.is_some() { "true" } else { "false" }.into()),
        "PUBLIC_URL" => Some("https://news.invalid".into()),
        _ => None,
    })
    .unwrap()
}
async fn login(app: &Router, auth: &AuthService, user: &str) -> Session {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", user)
        .append_pair("password", PASSWORD)
        .finish();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/login")
                .header(header::ORIGIN, "https://news.invalid")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| {
            let c = Cookie::parse(v.to_str().unwrap()).unwrap();
            format!("{}={}", c.name(), c.value())
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    auth.authenticate(&headers).await.unwrap()
}
async fn seed(pool: &PgPool, id: &str) {
    sqlx::query("INSERT INTO news.feeds(id,source_id,url,enabled,poll_seconds) VALUES($1,'source-a',$2,true,60)").bind(id).bind(format!("https://source.invalid/rss/{id}")).execute(pool).await.unwrap();
}
async fn due(pool: &PgPool, id: &str) {
    sqlx::query(
        "UPDATE news.feeds SET next_fetch_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}
async fn expire(pool: &PgPool, id: &str) {
    sqlx::query("UPDATE news.feed_fetch_attempts SET lease_until=clock_timestamp()-interval '1 second' WHERE feed_id=$1 AND outcome='started'").bind(id).execute(pool).await.unwrap();
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
struct Fixture {
    owner: PgPool,
    calls: AtomicUsize,
    mode: AtomicUsize,
    admin: FeedAdmin,
    publisher: Session,
    entered: Notify,
    release: Notify,
}
impl FeedTransport for Fixture {
    fn fetch<'a>(&'a self, url: &'a str, _: usize, cache: &'a Validators) -> FetchFuture<'a> {
        Box::pin(async move {
            let number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let id = url.rsplit('/').next().unwrap();
            match self.mode.load(Ordering::SeqCst) {
                1 => return Ok(FeedResult::NotModified(cache.clone())),
                2 => return Err(FetchError::Timeout),
                3 => return Err(FetchError::HttpStatus(429, Some(u64::MAX))),
                4 => return Err(FetchError::HttpStatus(503, Some(604801))),
                5 => expire(&self.owner, id).await,
                6 => {
                    let status = self.admin.status(&self.publisher, id).await.unwrap();
                    self.admin
                        .pause(
                            &self.publisher,
                            id,
                            status.control_version,
                            "Pause during fixture HTTP",
                        )
                        .await
                        .unwrap();
                }
                7 => {
                    sqlx::query(
                        "UPDATE news.sources SET lock_version=lock_version+1 WHERE id='source-a'",
                    )
                    .execute(&self.owner)
                    .await
                    .unwrap();
                }
                8 => {
                    self.entered.notify_one();
                    self.release.notified().await;
                }
                9 => {
                    sqlx::query("UPDATE news.feed_fetch_attempts SET lease_until=clock_timestamp()+interval '1 second' WHERE feed_id=$1 AND outcome='started'").bind(id).execute(&self.owner).await.unwrap();
                }
                _ => {}
            }
            Ok(FeedResult::Fetched(FeedData {
                final_url: url.into(),
                format: "rss2".into(),
                response_bytes: 200,
                title: Some("Synthetic feed".into()),
                entries: vec![Entry {
                    parser_id: "parser-id".into(),
                    title: Some("Synthetic metadata".into()),
                    links: vec![format!("https://source.invalid/article/{id}")],
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
struct Hanging;
impl FeedTransport for Hanging {
    fn fetch<'a>(&'a self, _: &'a str, _: usize, _: &'a Validators) -> FetchFuture<'a> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
#[ignore = "child-process fixture; only active with an explicit isolated-test marker"]
async fn killed_feed_worker_fixture() {
    if std::env::var("WORLDNEWS_FEED_KILL_FIXTURE").as_deref() != Ok("1") {
        return;
    }
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(url.username(), "fixture_feed_jobs_worker");
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_feed_jobs"));
    let pool = persistence::pool(&config(&database, None));
    let poller = FeedPoller::with_transport(pool.clone(), Arc::new(Hanging));
    poller.poll_due("crash").await.unwrap();
    pool.close().await;
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn cli(database: &str, args: &[&str]) -> std::process::Output {
    Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/world-news")
            .canonicalize()
            .unwrap(),
    )
    .args(args)
    .env_clear()
    .env("DATABASE_URL", database)
    .env("APP_ENV", "development")
    .output()
    .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn feed_leases_fence_cache_recover_crashes_and_enqueue_atomically() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_feed_jobs"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    let old = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            persistence::MIGRATOR.iter().take(10).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &old).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES('legacy-source','Legacy fixture','https://legacy.invalid',true); INSERT INTO news.feeds(id,source_id,url,enabled,fetch_paused,last_error,failure_count) VALUES('legacy-hold','legacy-source','https://legacy.invalid/hold',true,true,'retry_after_requires_review',2),('legacy-budget','legacy-source','https://legacy.invalid/budget',true,false,'timeout',6),('legacy-started','legacy-source','https://legacy.invalid/started',true,false,NULL,0); WITH started AS (INSERT INTO news.feed_fetch_attempts(feed_id,source_id,feed_version,source_version) VALUES('legacy-started','legacy-source',0,0) RETURNING id) UPDATE news.feeds SET last_attempt_id=(SELECT id FROM started),last_outcome='started' WHERE id='legacy-started';").execute(&owner).await.unwrap();
    persistence::migrate(&owner).await.unwrap();
    for (user, role) in [
        ("feed-publisher", Role::Publisher),
        ("feed-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, user, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES('source-a','Fixture source','https://source.invalid',true);
        CREATE ROLE fixture_feed_jobs_reader LOGIN PASSWORD 'synthetic-feed-jobs-reader'; GRANT USAGE ON SCHEMA news TO fixture_feed_jobs_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_feed_jobs_reader;
        CREATE ROLE fixture_feed_jobs_admin LOGIN PASSWORD 'synthetic-feed-jobs-admin'; GRANT USAGE ON SCHEMA news TO fixture_feed_jobs_admin;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.feeds,news.feed_fetch_attempts TO fixture_feed_jobs_admin;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_feed_jobs_admin; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_feed_jobs_admin;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.feed_operational_actions TO fixture_feed_jobs_admin;
        GRANT UPDATE(fetch_paused,fetch_pause_reason,failure_count,fetch_control_version,last_outcome,last_error,next_fetch_at) ON news.feeds TO fixture_feed_jobs_admin;
        GRANT UPDATE(outcome,error_category,finished_at,lease_until) ON news.feed_fetch_attempts TO fixture_feed_jobs_admin;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text) TO fixture_feed_jobs_admin;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.feed_operational_actions_id_seq TO fixture_feed_jobs_admin;
        CREATE ROLE fixture_feed_jobs_worker LOGIN PASSWORD 'synthetic-feed-jobs-worker'; GRANT USAGE ON SCHEMA news TO fixture_feed_jobs_worker;
        GRANT SELECT ON news.schema_version,news.feeds,news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_attempts,news.normalization_job_origins,news.source_articles,news.normalized_batches,news.source_observations TO fixture_feed_jobs_worker;
        GRANT SELECT(id,name,enabled,lock_version) ON news.sources TO fixture_feed_jobs_worker;
        GRANT INSERT ON news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_job_origins,news.normalization_attempts,news.source_articles,news.normalized_batches,news.source_observations TO fixture_feed_jobs_worker;
        GRANT UPDATE(next_fetch_at,last_attempt_at,last_outcome,last_error,last_http_status,failure_count,fetch_paused,last_success_at,last_attempt_id,etag,last_modified,validator_url,fetch_control_version,fetch_pause_reason,retry_after_seconds,retry_after_observed_at) ON news.feeds TO fixture_feed_jobs_worker;
        GRANT UPDATE(outcome,error_category,http_status,response_bytes,entry_count,finished_at,lease_until) ON news.feed_fetch_attempts TO fixture_feed_jobs_worker;
        GRANT UPDATE ON news.normalization_jobs,news.normalization_attempts TO fixture_feed_jobs_worker;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_feed_for_normalization(text) TO fixture_feed_jobs_worker;
        GRANT USAGE ON SEQUENCE news.feed_fetch_attempts_id_seq,news.normalization_jobs_id_seq TO fixture_feed_jobs_worker;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_feed_jobs_reader").unwrap();
    url.set_password(Some("synthetic-feed-jobs-reader"))
        .unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_feed_jobs_admin").unwrap();
    url.set_password(Some("synthetic-feed-jobs-admin")).unwrap();
    let admin_url = url.to_string();
    url.set_username("fixture_feed_jobs_worker").unwrap();
    url.set_password(Some("synthetic-feed-jobs-worker"))
        .unwrap();
    let worker_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&admin_url)));
    let reader = persistence::pool(&conf);
    let admin_pool = persistence::editor_pool(&conf).unwrap();
    let worker = persistence::pool(&config(&worker_url, None));
    let app = web::router_with_editor(conf, reader.clone(), Some(admin_pool.clone()));
    let auth = AuthService::new(
        admin_pool.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth, "feed-publisher").await;
    let editor = login(&app, &auth, "feed-editor").await;
    let admin = FeedAdmin::new(admin_pool.clone());
    let fixture = Arc::new(Fixture {
        owner: owner.clone(),
        calls: AtomicUsize::new(0),
        mode: AtomicUsize::new(8),
        admin: admin.clone(),
        publisher: publisher.clone(),
        entered: Notify::new(),
        release: Notify::new(),
    });
    let poller = FeedPoller::with_transport(worker.clone(), fixture.clone());
    let legacy = admin.status(&publisher, "legacy-hold").await.unwrap();
    assert!(legacy.paused);
    assert_eq!(
        legacy.pause_reason.as_deref(),
        Some("legacy_retry_instruction_unknown")
    );
    assert!(matches!(
        admin
            .resume(
                &publisher,
                "legacy-hold",
                legacy.control_version,
                "Cannot infer missing source limit"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        admin
            .status(&editor, "legacy-budget")
            .await
            .unwrap()
            .pause_reason
            .as_deref(),
        Some("retry_budget_exhausted")
    );
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    seed(&owner, "normal").await;
    let running = poller.clone();
    let task = tokio::spawn(async move { running.poll_due("normal").await });
    tokio::time::timeout(Duration::from_secs(5), fixture.entered.notified())
        .await
        .unwrap();
    due(&owner, "normal").await;
    assert_eq!(
        poller.poll_due("normal").await.unwrap(),
        PollOutcome::NotDue
    );
    fixture.release.notify_one();
    let first = task.await.unwrap().unwrap();
    let PollOutcome::Modified { attempt_id, .. } = first else {
        panic!("modified required")
    };
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    assert_eq!(count(&owner, "feed_batches").await, 1);
    assert_eq!(count(&owner, "normalization_jobs").await, 1);
    assert_eq!(count(&owner, "normalization_job_origins").await, 1);
    assert_eq!(count(&owner, "normalization_job_actions").await, 0);
    let job =
        sqlx::query_scalar::<_, i64>("SELECT id FROM news.normalization_jobs WHERE batch_id=$1")
            .bind(attempt_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(
        Queue::new(worker.clone()).work_once().await.unwrap(),
        WorkOutcome::Completed { job_id: job }
    );
    assert_eq!(count(&owner, "source_articles").await, 1);
    let cache = sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='normal'")
        .fetch_one(&owner)
        .await
        .unwrap();
    fixture.mode.store(1, Ordering::SeqCst);
    due(&owner, "normal").await;
    assert!(matches!(
        poller.poll_due("normal").await.unwrap(),
        PollOutcome::Unchanged { .. }
    ));
    assert_eq!(count(&owner, "normalization_jobs").await, 1);
    // Expired in-flight completion cannot advance cache; explicit recovery closes it.
    fixture.mode.store(5, Ordering::SeqCst);
    due(&owner, "normal").await;
    assert_eq!(
        poller.poll_due("normal").await.unwrap(),
        PollOutcome::LeaseLost
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='normal'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        cache
    );
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    assert_eq!(
        admin
            .status(&editor, "normal")
            .await
            .unwrap()
            .last_error
            .as_deref(),
        Some("lease_expired")
    );
    // Recover while an old transport is still in flight, complete a replacement,
    // then prove the old owner cannot overwrite the new cache.
    seed(&owner, "stale-owner").await;
    fixture.mode.store(8, Ordering::SeqCst);
    let running = poller.clone();
    let old_task = tokio::spawn(async move { running.poll_due("stale-owner").await });
    tokio::time::timeout(Duration::from_secs(5), fixture.entered.notified())
        .await
        .unwrap();
    expire(&owner, "stale-owner").await;
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    due(&owner, "stale-owner").await;
    fixture.mode.store(0, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("stale-owner").await.unwrap(),
        PollOutcome::Modified { .. }
    ));
    let replacement_cache =
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='stale-owner'")
            .fetch_one(&owner)
            .await
            .unwrap();
    fixture.release.notify_one();
    assert_eq!(old_task.await.unwrap().unwrap(), PollOutcome::LeaseLost);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT etag FROM news.feeds WHERE id='stale-owner'")
            .fetch_one(&owner)
            .await
            .unwrap(),
        replacement_cache
    );
    // Historic superseded attempts can be closed without changing current status.
    seed(&owner, "orphan").await;
    let orphan=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(feed_id,source_id,feed_version,source_version,lease_until) VALUES('orphan','source-a',0,0,clock_timestamp()-interval '1 second') RETURNING id").fetch_one(&owner).await.unwrap();
    assert!(matches!(
        poller.poll_due("orphan").await.unwrap(),
        PollOutcome::Modified { .. }
    ));
    let latest = admin.status(&editor, "orphan").await.unwrap();
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    assert_eq!(
        admin
            .status(&editor, "orphan")
            .await
            .unwrap()
            .control_version,
        latest.control_version
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT error_category FROM news.feed_fetch_attempts WHERE id=$1"
        )
        .bind(orphan)
        .fetch_one(&owner)
        .await
        .unwrap(),
        "superseded"
    );
    // Failure of machine-origin enqueue rolls back batch, job, cache and terminal status.
    fixture.mode.store(0, Ordering::SeqCst);
    seed(&owner, "atomic").await;
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_origin() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture origin failure'; END $$; CREATE TRIGGER fixture_fail BEFORE INSERT ON news.normalization_job_origins FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_origin();").execute(&owner).await.unwrap();
    let batches = count(&owner, "feed_batches").await;
    let jobs = count(&owner, "normalization_jobs").await;
    assert!(poller.poll_due("atomic").await.is_err());
    assert_eq!(count(&owner, "feed_batches").await, batches);
    assert_eq!(count(&owner, "normalization_jobs").await, jobs);
    assert_eq!(
        admin
            .status(&editor, "atomic")
            .await
            .unwrap()
            .last_outcome
            .as_deref(),
        Some("started")
    );
    assert!(
        sqlx::query_scalar::<_, Option<String>>("SELECT etag FROM news.feeds WHERE id='atomic'")
            .fetch_one(&owner)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.normalization_job_origins; DROP FUNCTION news.fixture_fail_origin();").execute(&owner).await.unwrap();
    expire(&owner, "atomic").await;
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    due(&owner, "atomic").await;
    assert!(matches!(
        poller.poll_due("atomic").await.unwrap(),
        PollOutcome::Modified { .. }
    ));
    // A deadline crossed after batch insertion still rolls back all success writes.
    seed(&owner, "deadline").await;
    fixture.mode.store(9, Ordering::SeqCst);
    sqlx::raw_sql("CREATE FUNCTION news.fixture_delay_origin() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.2); RETURN NEW; END $$; CREATE TRIGGER fixture_delay BEFORE INSERT ON news.normalization_job_origins FOR EACH ROW EXECUTE FUNCTION news.fixture_delay_origin();").execute(&owner).await.unwrap();
    let before = count(&owner, "feed_batches").await;
    assert_eq!(
        poller.poll_due("deadline").await.unwrap_err(),
        AppError::Conflict
    );
    assert_eq!(count(&owner, "feed_batches").await, before);
    sqlx::raw_sql("DROP TRIGGER fixture_delay ON news.normalization_job_origins; DROP FUNCTION news.fixture_delay_origin();").execute(&owner).await.unwrap();
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    seed(&owner, "busy").await;
    fixture.mode.store(5, Ordering::SeqCst);
    assert_eq!(
        poller.poll_due("busy").await.unwrap(),
        PollOutcome::LeaseLost
    );
    let mut locked = owner.begin().await.unwrap();
    sqlx::query("SELECT id FROM news.feeds WHERE id='busy' FOR UPDATE")
        .fetch_one(&mut *locked)
        .await
        .unwrap();
    assert_eq!(poller.recover_expired().await.unwrap(), 0);
    locked.rollback().await.unwrap();
    assert_eq!(poller.recover_expired().await.unwrap(), 1);
    seed(&owner, "budget").await;
    fixture.mode.store(2, Ordering::SeqCst);
    for _ in 0..5 {
        due(&owner, "budget").await;
        assert!(matches!(
            poller.poll_due("budget").await.unwrap(),
            PollOutcome::Failed { .. }
        ));
    }
    let budget = admin.status(&editor, "budget").await.unwrap();
    assert!(budget.paused);
    assert_eq!(
        budget.pause_reason.as_deref(),
        Some("retry_budget_exhausted")
    );
    assert_eq!(budget.failure_count, 5);
    assert_eq!(
        poller.poll_due("budget").await.unwrap(),
        PollOutcome::Paused
    );
    assert!(matches!(
        admin
            .resume(
                &editor,
                "budget",
                budget.control_version,
                "Editor cannot resume"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    let resumed = admin
        .resume(
            &publisher,
            "budget",
            budget.control_version,
            "Reviewed failure budget",
        )
        .await
        .unwrap();
    assert!(!resumed.paused);
    assert_eq!(resumed.failure_count, 0);
    assert!(resumed.next_fetch_at >= budget.next_fetch_at);
    assert!(matches!(
        admin
            .pause(
                &publisher,
                "budget",
                budget.control_version,
                "Stale control"
            )
            .await,
        Err(AppError::Conflict)
    ));
    seed(&owner, "huge").await;
    fixture.mode.store(3, Ordering::SeqCst);
    assert!(matches!(
        poller.poll_due("huge").await.unwrap(),
        PollOutcome::Failed { .. }
    ));
    let huge = admin.status(&publisher, "huge").await.unwrap();
    assert!(matches!(
        admin
            .resume(
                &publisher,
                "huge",
                huge.control_version,
                "Cannot bypass source limit"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    // The worker also enforces the persisted source minimum if an operator
    // changes the pause flag/due time outside the application controls.
    sqlx::query("UPDATE news.feeds SET fetch_paused=false,next_fetch_at=clock_timestamp()-interval '1 second' WHERE id='huge'").execute(&owner).await.unwrap();
    let before = fixture.calls.load(Ordering::SeqCst);
    assert_eq!(poller.poll_due("huge").await.unwrap(), PollOutcome::NotDue);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), before);
    sqlx::query("UPDATE news.feeds SET fetch_paused=true WHERE id='huge'")
        .execute(&owner)
        .await
        .unwrap();
    seed(&owner, "long-hint").await;
    fixture.mode.store(4, Ordering::SeqCst);
    poller.poll_due("long-hint").await.unwrap();
    let long = admin.status(&publisher, "long-hint").await.unwrap();
    assert!(matches!(
        admin
            .resume(&publisher, "long-hint", long.control_version, "Too early")
            .await,
        Err(AppError::Validation(_))
    ));
    sqlx::query("UPDATE news.feeds SET retry_after_observed_at=clock_timestamp()-interval '8 days' WHERE id='long-hint'").execute(&owner).await.unwrap();
    assert!(
        !admin
            .resume(
                &publisher,
                "long-hint",
                long.control_version,
                "Retry instruction elapsed"
            )
            .await
            .unwrap()
            .paused
    );
    seed(&owner, "cancel").await;
    fixture.mode.store(6, Ordering::SeqCst);
    assert_eq!(
        poller.poll_due("cancel").await.unwrap(),
        PollOutcome::LeaseLost
    );
    let cancel = admin.status(&editor, "cancel").await.unwrap();
    assert!(cancel.paused);
    assert_eq!(cancel.pause_reason.as_deref(), Some("manual_pause"));
    assert_eq!(cancel.last_outcome.as_deref(), Some("cancelled"));
    seed(&owner, "change").await;
    fixture.mode.store(7, Ordering::SeqCst);
    assert_eq!(
        poller.poll_due("change").await.unwrap(),
        PollOutcome::Cancelled
    );
    // Actual process kill after durable reservation; transport is a hanging test adapter,
    // never a production private-network escape. Advance deadline deterministically.
    seed(&owner, "crash").await;
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "killed_feed_worker_fixture",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_clear()
            .env("WORLDNEWS_FEED_KILL_FIXTURE", "1")
            .env("WORLDNEWS_TEST_DATABASE_URL", &worker_url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(5),async{loop{let started=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM news.feed_fetch_attempts WHERE feed_id='crash' AND outcome='started')").fetch_one(&owner).await.unwrap();if started{break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    expire(&owner, "crash").await;
    let recovered = cli(&worker_url, &["--recover-feed-leases-once"]);
    assert!(recovered.status.success());
    let report: Value = serde_json::from_slice(&recovered.stdout).unwrap();
    assert_eq!(report["attempts"], 1);
    assert_eq!(
        admin
            .status(&editor, "crash")
            .await
            .unwrap()
            .last_error
            .as_deref(),
        Some("lease_expired")
    );
    fixture.mode.store(0, Ordering::SeqCst);
    due(&owner, "crash").await;
    assert!(matches!(
        poller.poll_due("crash").await.unwrap(),
        PollOutcome::Modified { .. }
    ));
    sqlx::query("UPDATE news.feeds SET enabled=false WHERE id='crash'")
        .execute(&owner)
        .await
        .unwrap();
    let disabled = cli(&worker_url, &["--poll-feed-once", "crash"]);
    assert!(disabled.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&disabled.stdout).unwrap()["status"],
        "disabled"
    );
    assert!(!String::from_utf8_lossy(&disabled.stdout).contains("synthetic-feed-jobs"));
    let history = admin
        .history(&editor, "crash", None, PageSize::new(1).unwrap())
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        admin
            .history(
                &editor,
                "crash",
                Some(history[0].id),
                PageSize::new(1).unwrap()
            )
            .await
            .unwrap()[0]
            .error_category
            .as_deref(),
        Some("lease_expired")
    );
    let status = admin.status(&publisher, "normal").await.unwrap();
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='feed-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        admin
            .pause(
                &publisher,
                "normal",
                status.control_version,
                "Revoked request"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    for sql in [
        "SELECT * FROM news.feed_operational_actions",
        "SELECT * FROM news.normalization_job_origins",
        "SELECT * FROM news.feed_fetch_attempts",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err());
    }
    for sql in [
        "SELECT * FROM news.admin_users",
        "INSERT INTO news.stories(id) VALUES('cannot-publish')",
        "UPDATE news.sources SET enabled=false",
        "CREATE TABLE news.forbidden_feed_job(id int)",
    ] {
        assert!(sqlx::query(sql).execute(&worker).await.is_err());
    }
    for sql in [
        "UPDATE news.normalization_job_origins SET created_at=now()",
        "DELETE FROM news.feed_operational_actions",
        "UPDATE news.feed_fetch_attempts SET lease_until=now() WHERE outcome='modified'",
    ] {
        assert!(sqlx::query(sql).execute(&owner).await.is_err());
    }
    assert_eq!(count(&owner, "stories").await, 0);
    owner.close().await;
    reader.close().await;
    admin_pool.close().await;
    worker.close().await;
}
