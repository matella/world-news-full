use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use sqlx::PgPool;
use std::{
    path::Path,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    intake::{
        article::Extracted,
        extraction_jobs::{ExtractionAdmin, ExtractionQueue},
        feed::{Entry, FeedData, FeedResult},
        http::Validators,
        jobs::Queue,
        manual::{ArticleTransport, ExtractFuture},
        poll::{FeedPoller, FeedTransport, FetchFuture},
    },
    persistence, web,
    worker::{IntakeWorker, Stage, WorkerPolicy},
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic worker orchestration passphrase";
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
async fn login(app: &Router, auth: &AuthService) -> Session {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", "worker-publisher")
        .append_pair("password", PASSWORD)
        .finish();
    let r = app
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
    assert_eq!(r.status(), StatusCode::SEE_OTHER);
    let cookies = r
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| {
            let c = Cookie::parse(v.to_str().unwrap()).unwrap();
            format!("{}={}", c.name(), c.value())
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut h = HeaderMap::new();
    h.insert(header::COOKIE, cookies.parse().unwrap());
    auth.authenticate(&h).await.unwrap()
}
fn policy(pairs: &[(&str, &str)]) -> WorkerPolicy {
    WorkerPolicy::from_lookup(|key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    })
    .unwrap()
}
struct Feeds {
    calls: AtomicUsize,
    slow: AtomicUsize,
}
impl FeedTransport for Feeds {
    fn fetch<'a>(&'a self, url: &'a str, _: usize, _: &'a Validators) -> FetchFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.slow.load(Ordering::SeqCst) == 1 {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
            let id = url.rsplit('/').next().unwrap();
            Ok(FeedResult::Fetched(FeedData {
                final_url: url.into(),
                format: "rss2".into(),
                response_bytes: 200,
                title: Some("Worker fixture".into()),
                entries: vec![Entry {
                    parser_id: "parser".into(),
                    title: Some("Synthetic source article".into()),
                    links: vec![format!("https://source.invalid/article/{id}")],
                    published: None,
                    updated: None,
                    language: Some("fr".into()),
                    authors: vec![],
                }],
                validators: Validators {
                    url: Some(url.into()),
                    etag: Some("\"worker-fixture\"".into()),
                    last_modified: None,
                },
            }))
        })
    }
}
struct Articles {
    calls: AtomicUsize,
}
impl ArticleTransport for Articles {
    fn extract<'a>(&'a self, url: &'a str) -> ExtractFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Extracted {
                final_url: url.into(),
                response_hash: "0".repeat(64),
                title: Some("Source evidence".into()),
                authors: vec![],
                published: None,
                paragraphs: vec![
                    "Private untrusted source evidence. No story is automatically written.".into(),
                ],
            })
        })
    }
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn bounded_worker_runs_authorized_stages_and_recovers_interruption() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_worker"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    auth::bootstrap(
        &owner,
        "worker-publisher",
        Role::Publisher,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled,rights_status,usage_policy,display_policy,rights_note,terms_url) VALUES('worker-source','Synthetic source','https://source.invalid',true,'documented','licensed_text','link_only','Fixture license','https://source.invalid/terms');
    INSERT INTO news.feeds(id,source_id,url,enabled,poll_seconds) VALUES('f1','worker-source','https://source.invalid/rss/f1',true,60),('f2','worker-source','https://source.invalid/rss/f2',true,60),('f3','worker-source','https://source.invalid/rss/f3',true,60),('slow','worker-source','https://source.invalid/rss/slow',true,60),('cli-timeout','worker-source','https://source.invalid/rss/cli-timeout',true,60);
    CREATE ROLE fixture_orchestration_reader LOGIN PASSWORD 'synthetic-orchestration-reader'; GRANT USAGE ON SCHEMA news TO fixture_orchestration_reader; GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_orchestration_reader;
    CREATE ROLE fixture_orchestration_admin LOGIN PASSWORD 'synthetic-orchestration-admin'; GRANT USAGE ON SCHEMA news TO fixture_orchestration_admin; GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.sources,news.source_origin_links,news.source_articles,news.extraction_jobs,news.extraction_attempts TO fixture_orchestration_admin; GRANT INSERT ON news.admin_sessions,news.admin_audit,news.extraction_jobs,news.extraction_job_actions TO fixture_orchestration_admin; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_orchestration_admin; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_orchestration_admin; GRANT UPDATE ON news.extraction_jobs,news.extraction_attempts TO fixture_orchestration_admin; GRANT UPDATE(intake_version) ON news.source_articles TO fixture_orchestration_admin; GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.extraction_retry_allowed(text) TO fixture_orchestration_admin; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.extraction_jobs_id_seq,news.extraction_job_actions_id_seq TO fixture_orchestration_admin;
    CREATE ROLE fixture_orchestration_worker LOGIN PASSWORD 'synthetic-orchestration-worker'; GRANT USAGE ON SCHEMA news TO fixture_orchestration_worker; GRANT SELECT ON news.schema_version,news.sources,news.source_origin_links,news.feeds,news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_attempts,news.normalization_job_origins,news.source_articles,news.normalized_batches,news.source_observations,news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_orchestration_worker; GRANT SELECT(id) ON news.intake_snapshots TO fixture_orchestration_worker;
    GRANT INSERT ON news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_attempts,news.normalization_job_origins,news.source_articles,news.normalized_batches,news.source_observations,news.extraction_attempts,news.source_extraction_limits,news.intake_text TO fixture_orchestration_worker;
    GRANT INSERT(source_article_id,version,source_version,worker_job_id,kind,error_category,policy,extraction_metadata) ON news.intake_snapshots TO fixture_orchestration_worker;
    GRANT UPDATE(next_fetch_at,last_attempt_at,last_outcome,last_error,last_http_status,failure_count,fetch_paused,last_success_at,last_attempt_id,etag,last_modified,validator_url,fetch_control_version,fetch_pause_reason,retry_after_seconds,retry_after_observed_at) ON news.feeds TO fixture_orchestration_worker; GRANT UPDATE(outcome,error_category,http_status,response_bytes,entry_count,finished_at,lease_until) ON news.feed_fetch_attempts TO fixture_orchestration_worker; GRANT UPDATE ON news.normalization_jobs,news.normalization_attempts,news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_orchestration_worker; GRANT UPDATE(intake_version,current_intake_id,extraction_state) ON news.source_articles TO fixture_orchestration_worker;
    GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_feed_for_normalization(text),news.lock_extraction_requester(bigint),news.extraction_retry_allowed(text) TO fixture_orchestration_worker; GRANT USAGE ON SEQUENCE news.feed_fetch_attempts_id_seq,news.normalization_jobs_id_seq,news.intake_snapshots_id_seq TO fixture_orchestration_worker;")
    .execute(&owner).await.unwrap();
    url.set_username("fixture_orchestration_reader").unwrap();
    url.set_password(Some("synthetic-orchestration-reader"))
        .unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_orchestration_admin").unwrap();
    url.set_password(Some("synthetic-orchestration-admin"))
        .unwrap();
    let admin_url = url.to_string();
    url.set_username("fixture_orchestration_worker").unwrap();
    url.set_password(Some("synthetic-orchestration-worker"))
        .unwrap();
    let worker_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&admin_url)));
    let reader = persistence::pool(&conf);
    let admin_pool = persistence::editor_pool(&conf).unwrap();
    let pool = persistence::pool(&config(&worker_url, None));
    let app = web::router_with_editor(conf, reader.clone(), Some(admin_pool.clone()));
    let auth = AuthService::new(
        admin_pool.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth).await;
    let feeds = Arc::new(Feeds {
        calls: AtomicUsize::new(0),
        slow: AtomicUsize::new(0),
    });
    let articles = Arc::new(Articles {
        calls: AtomicUsize::new(0),
    });
    let poller = FeedPoller::with_transport(pool.clone(), feeds.clone());
    let engine = IntakeWorker::with_services(
        poller.clone(),
        Queue::new(pool.clone()),
        ExtractionQueue::with_transport(pool.clone(), articles.clone()),
    );
    let first = engine
        .run_once(&policy(&[("NEWS_WORKER_FEEDS", r#"["f1"]"#)]))
        .await;
    assert!(first.succeeded(), "{first:?}");
    assert_eq!(first.feeds.completed, 1);
    assert_eq!(first.normalization.completed, 1);
    assert_eq!(articles.calls.load(Ordering::SeqCst), 0);
    assert_eq!(count(&owner, "source_articles").await, 1);
    let article=sqlx::query_scalar::<_,String>("SELECT id FROM news.source_articles WHERE original_url='https://source.invalid/article/f1'").fetch_one(&owner).await.unwrap();
    ExtractionAdmin::new(admin_pool.clone())
        .request(
            &publisher,
            &article,
            0,
            "Explicit source extraction after feed normalization",
        )
        .await
        .unwrap();
    let calls = feeds.calls.load(Ordering::SeqCst);
    let defaults = engine.run_once(&policy(&[])).await;
    assert!(defaults.succeeded());
    assert_eq!(feeds.calls.load(Ordering::SeqCst), calls);
    assert_eq!(articles.calls.load(Ordering::SeqCst), 0);
    assert_eq!(defaults.extraction.calls, 0);
    let extracted = engine
        .run_once(&policy(&[("NEWS_WORKER_EXTRACTION_ENABLED", "true")]))
        .await;
    assert!(extracted.succeeded(), "{extracted:?}");
    assert_eq!(extracted.extraction.completed, 1);
    assert_eq!(articles.calls.load(Ordering::SeqCst), 1);
    assert!(sqlx::query_scalar::<_,bool>("SELECT actor_id IS NULL AND worker_job_id IS NOT NULL FROM news.intake_snapshots WHERE source_article_id=$1").bind(&article).fetch_one(&owner).await.unwrap());
    poller.poll_due("f2").await.unwrap();
    poller.poll_due("f3").await.unwrap();
    let capped = engine
        .run_once(&policy(&[("NEWS_WORKER_MAX_NORMALIZATIONS", "1")]))
        .await;
    assert!(capped.succeeded());
    assert_eq!(capped.normalization.calls, 1);
    assert_eq!(capped.normalization.completed, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.normalization_jobs WHERE state='queued'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        1
    );
    assert!(engine.run_once(&policy(&[])).await.succeeded());
    feeds.slow.store(1, Ordering::SeqCst);
    let timed = engine
        .run_once(&policy(&[
            ("NEWS_WORKER_FEEDS", r#"["slow"]"#),
            ("NEWS_WORKER_BUDGET_SECONDS", "1"),
            ("NEWS_WORKER_MAX_NORMALIZATIONS", "0"),
        ]))
        .await;
    assert!(timed.interrupted);
    assert!(!timed.succeeded());
    assert!(matches!(timed.active_stage, Some(Stage::Feeds)));
    assert_eq!(timed.feeds.completed, 0);
    assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM news.feed_fetch_attempts WHERE feed_id='slow' AND outcome='started')").fetch_one(&owner).await.unwrap());
    sqlx::query("UPDATE news.feed_fetch_attempts SET lease_until=clock_timestamp()-interval '1 second' WHERE feed_id='slow' AND outcome='started'").execute(&owner).await.unwrap();
    let recovered = engine
        .run_once(&policy(&[("NEWS_WORKER_MAX_NORMALIZATIONS", "0")]))
        .await;
    assert!(recovered.succeeded());
    assert_eq!(recovered.recovered_feeds, 1);
    feeds.slow.store(0, Ordering::SeqCst);
    sqlx::query(
        "UPDATE news.feeds SET next_fetch_at=clock_timestamp()-interval '1 second' WHERE id='slow'",
    )
    .execute(&owner)
    .await
    .unwrap();
    assert!(
        engine
            .run_once(&policy(&[("NEWS_WORKER_FEEDS", r#"["slow"]"#)]))
            .await
            .succeeded()
    );
    // Default real command does not fetch any configured feed or execute grants.
    let binary = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/world-news")
        .canonicalize()
        .unwrap();
    let result = Command::new(&binary)
        .arg("--work-intake-once")
        .env_clear()
        .env("DATABASE_URL", &worker_url)
        .env("APP_ENV", "development")
        .output()
        .unwrap();
    assert!(result.status.success());
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["extraction_enabled"], false);
    assert_eq!(report["feeds"]["calls"], 0);
    assert!(!String::from_utf8_lossy(&result.stdout).contains("https://source.invalid"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("synthetic-orchestration"));
    // A real command interrupted during DB work reports partial state and exits 2.
    poller.poll_due("cli-timeout").await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_worker_delay() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(3); RETURN NEW; END $$; CREATE TRIGGER fixture_delay BEFORE INSERT ON news.source_observations FOR EACH ROW EXECUTE FUNCTION news.fixture_worker_delay();").execute(&owner).await.unwrap();
    let timed = Command::new(&binary)
        .arg("--work-intake-once")
        .env_clear()
        .env("DATABASE_URL", &worker_url)
        .env("APP_ENV", "development")
        .env("NEWS_WORKER_BUDGET_SECONDS", "1")
        .output()
        .unwrap();
    assert_eq!(timed.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&timed.stdout).unwrap();
    assert_eq!(report["interrupted"], true);
    assert_eq!(report["stages_finished"], false);
    assert_eq!(report["active_stage"], "normalization");
    sqlx::raw_sql("DROP TRIGGER fixture_delay ON news.source_observations; DROP FUNCTION news.fixture_worker_delay();").execute(&owner).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.normalization_jobs WHERE state='running'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        1
    );
    let denied = IntakeWorker::new(
        reader.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    )
    .unwrap()
    .run_once(&policy(&[]))
    .await;
    assert!(!denied.succeeded());
    assert_eq!(denied.errors.len(), 1);
    assert_eq!(denied.errors[0].category, "forbidden");
    assert_eq!(count(&owner, "stories").await, 0);
    for sql in [
        "SELECT * FROM news.admin_users",
        "INSERT INTO news.stories(id) VALUES('cannot-publish')",
        "UPDATE news.sources SET enabled=false",
    ] {
        assert!(sqlx::query(sql).execute(&pool).await.is_err());
    }
    owner.close().await;
    reader.close().await;
    admin_pool.close().await;
    pool.close().await;
}
