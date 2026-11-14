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
    content::PageSize,
    error::AppError,
    intake::{
        article::{Extracted, ExtractionError},
        extraction_jobs::*,
        http::FetchError,
        manual::{ArticleTransport, ExtractFuture, IntakeService},
    },
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic extraction job passphrase";
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
async fn seed(pool: &PgPool, id: &str, source: &str) {
    sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES($1,$2,$1,$3,'Synthetic metadata','metadata_only')").bind(id).bind(source).bind(format!("https://{}.invalid/{id}",if source=="source-a"{"source"}else{"hint"})).execute(pool).await.unwrap();
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn due(pool: &PgPool, id: i64) {
    sqlx::query("UPDATE news.extraction_jobs SET available_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(pool).await.unwrap();
}
async fn expire(pool: &PgPool, id: i64) {
    sqlx::query("UPDATE news.extraction_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1 AND state='running'").bind(id).execute(pool).await.unwrap();
}
struct Fixture {
    owner: PgPool,
    calls: AtomicUsize,
    mode: AtomicUsize,
    editor_pool: PgPool,
    editor: Session,
    publisher: Session,
}
impl ArticleTransport for Fixture {
    fn extract<'a>(&'a self, url: &'a str) -> ExtractFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let article = url.rsplit('/').next().unwrap();
            match self.mode.load(Ordering::SeqCst) {
                1 => return Err(ExtractionError::Fetch(FetchError::Timeout)),
                2 => return Err(ExtractionError::NoArticle),
                3 => {
                    sqlx::query(
                        "UPDATE news.sources SET lock_version=lock_version+1 WHERE id='source-a'",
                    )
                    .execute(&self.owner)
                    .await
                    .unwrap();
                }
                4 => {
                    sqlx::query(
                        "UPDATE news.admin_users SET role='editor' WHERE id='extract-publisher'",
                    )
                    .execute(&self.owner)
                    .await
                    .unwrap();
                }
                5 => {
                    IntakeService::new(
                        self.editor_pool.clone(),
                        &url::Url::parse("https://news.invalid").unwrap(),
                    )
                    .unwrap()
                    .manual(
                        &self.editor,
                        article,
                        0,
                        "Editor note while machine request is in flight",
                        None,
                    )
                    .await
                    .unwrap();
                }
                6 => {
                    let id=sqlx::query_scalar::<_,i64>("SELECT id FROM news.extraction_jobs WHERE source_article_id=$1 AND state='running'").bind(article).fetch_one(&self.owner).await.unwrap();
                    let admin = ExtractionAdmin::new(self.editor_pool.clone());
                    let status = admin.status(&self.publisher, id).await.unwrap();
                    admin
                        .cancel(
                            &self.publisher,
                            id,
                            status.lock_version,
                            "Publisher cancellation during transport",
                        )
                        .await
                        .unwrap();
                }
                7 => {
                    sqlx::query("UPDATE news.extraction_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE source_article_id=$1 AND state='running'").bind(article).execute(&self.owner).await.unwrap();
                }
                8 => {
                    return Err(ExtractionError::Fetch(FetchError::HttpStatus(
                        429,
                        Some(u64::MAX),
                    )));
                }
                11 => {
                    tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                }
                12 => {
                    return Err(ExtractionError::Fetch(FetchError::HttpStatus(
                        429,
                        Some(120),
                    )));
                }
                10 => {
                    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='extract-publisher'").execute(&self.owner).await.unwrap();
                }
                _ => {}
            }
            Ok(Extracted{final_url:if self.mode.load(Ordering::SeqCst)==9{"https://wrong.invalid/a".into()}else{url.into()},response_hash:"0".repeat(64),title:Some("Fixture extracted title".into()),authors:vec!["Source author".into()],published:None,paragraphs:vec!["Private untrusted source evidence, never a command or automatic original draft.".into()]})
        })
    }
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn extraction_grants_fence_machine_authority_and_preserve_actor_provenance() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(
        url.path().starts_with("/worldnews_disposable_")
            && url.path().ends_with("_extraction_jobs")
    );
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    for (user, role) in [
        ("extract-publisher", Role::Publisher),
        ("extract-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, user, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled,rights_status,usage_policy,display_policy,rights_note,terms_url) VALUES('source-a','Synthetic source','https://source.invalid',true,'documented','licensed_text','link_only','Fixture license','https://source.invalid/terms'),('hint-source','Hint source','https://hint.invalid',true,'documented','licensed_text','link_only','Fixture license','https://hint.invalid/terms');
    CREATE ROLE fixture_extraction_reader LOGIN PASSWORD 'synthetic-extraction-reader'; GRANT USAGE ON SCHEMA news TO fixture_extraction_reader; GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_extraction_reader;
    CREATE ROLE fixture_extraction_admin LOGIN PASSWORD 'synthetic-extraction-admin'; GRANT USAGE ON SCHEMA news TO fixture_extraction_admin;
    GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.sources,news.source_origin_links,news.source_articles,news.intake_snapshots,news.intake_text,news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_extraction_admin;
    GRANT INSERT ON news.admin_sessions,news.admin_audit,news.extraction_jobs,news.extraction_job_actions,news.intake_snapshots TO fixture_extraction_admin;
    GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_extraction_admin; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_extraction_admin;
    GRANT UPDATE ON news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_extraction_admin;
    GRANT UPDATE(intake_version,current_intake_id,extraction_state) ON news.source_articles TO fixture_extraction_admin;
    GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.extraction_retry_allowed(text) TO fixture_extraction_admin;
    GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.extraction_jobs_id_seq,news.extraction_job_actions_id_seq,news.intake_snapshots_id_seq TO fixture_extraction_admin;
    GRANT INSERT ON news.source_extraction_limits TO fixture_extraction_admin;
    CREATE ROLE fixture_extraction_worker LOGIN PASSWORD 'synthetic-extraction-worker'; GRANT USAGE ON SCHEMA news TO fixture_extraction_worker;
    GRANT SELECT ON news.schema_version,news.extraction_jobs,news.extraction_attempts,news.sources,news.source_origin_links,news.source_articles,news.source_extraction_limits TO fixture_extraction_worker;
    GRANT SELECT(id) ON news.intake_snapshots TO fixture_extraction_worker;
    GRANT INSERT(source_article_id,version,source_version,worker_job_id,kind,error_category,policy,extraction_metadata) ON news.intake_snapshots TO fixture_extraction_worker;
    GRANT INSERT ON news.intake_text,news.extraction_attempts,news.source_extraction_limits TO fixture_extraction_worker;
    GRANT UPDATE ON news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_extraction_worker;
    GRANT UPDATE(intake_version,current_intake_id,extraction_state) ON news.source_articles TO fixture_extraction_worker;
    GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_extraction_requester(bigint),news.extraction_retry_allowed(text) TO fixture_extraction_worker;
    GRANT USAGE ON SEQUENCE news.intake_snapshots_id_seq TO fixture_extraction_worker;")
    .execute(&owner).await.unwrap();
    url.set_username("fixture_extraction_reader").unwrap();
    url.set_password(Some("synthetic-extraction-reader"))
        .unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_extraction_admin").unwrap();
    url.set_password(Some("synthetic-extraction-admin"))
        .unwrap();
    let admin_url = url.to_string();
    url.set_username("fixture_extraction_worker").unwrap();
    url.set_password(Some("synthetic-extraction-worker"))
        .unwrap();
    let worker_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&admin_url)));
    let reader = persistence::pool(&conf);
    let editor_pool = persistence::editor_pool(&conf).unwrap();
    let worker = persistence::pool(&config(&worker_url, None));
    let app = web::router_with_editor(conf, reader.clone(), Some(editor_pool.clone()));
    let auth = AuthService::new(
        editor_pool.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let mut publisher = login(&app, &auth, "extract-publisher").await;
    let editor = login(&app, &auth, "extract-editor").await;
    let admin = ExtractionAdmin::new(editor_pool.clone());
    let fixture = Arc::new(Fixture {
        owner: owner.clone(),
        calls: AtomicUsize::new(0),
        mode: AtomicUsize::new(0),
        editor_pool: editor_pool.clone(),
        editor: editor.clone(),
        publisher: publisher.clone(),
    });
    let queue = ExtractionQueue::with_transport(worker.clone(), fixture.clone());
    let intake = IntakeService::with_transport(editor_pool.clone(), fixture.clone());
    seed(&owner, "normal", "source-a").await;
    assert!(matches!(
        admin
            .request(&editor, "normal", 0, "Editor cannot grant")
            .await,
        Err(AppError::Forbidden)
    ));
    let first = admin
        .request(
            &publisher,
            "normal",
            0,
            "Explicit one-hour extraction grant",
        )
        .await
        .unwrap();
    assert_eq!(first.requested_by, "extract-publisher");
    assert_eq!(
        admin
            .request(&publisher, "normal", 0, "Duplicate is same grant")
            .await
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(count(&owner, "extraction_job_actions").await, 1);
    let (a, b) = tokio::join!(queue.claim(), queue.claim());
    let lease = match (a.unwrap(), b.unwrap()) {
        (Some(l), None) | (None, Some(l)) => l,
        _ => panic!("one claim winner"),
    };
    assert_eq!(
        queue.run_claimed(&lease).await.unwrap(),
        ExtractionOutcome::Completed { job_id: first.id }
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    let history = intake
        .history(&editor, "normal", None, PageSize::new(10).unwrap())
        .await
        .unwrap();
    assert_eq!(history[0].actor_id, None);
    assert_eq!(history[0].worker_job_id, Some(first.id));
    let snapshot = intake
        .snapshot(&editor, "normal", history[0].id)
        .await
        .unwrap();
    assert!(snapshot.extracted.is_some());
    assert_eq!(
        queue.run_claimed(&lease).await.unwrap(),
        ExtractionOutcome::LostLease { job_id: first.id }
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    seed(&owner, "retry", "source-a").await;
    let retry = admin
        .request(&publisher, "retry", 0, "Retry fixture")
        .await
        .unwrap();
    fixture.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Deferred { job_id: retry.id }
    );
    due(&owner, retry.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Deferred { job_id: retry.id }
    );
    due(&owner, retry.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Failed { job_id: retry.id }
    );
    assert_eq!(
        admin.status(&editor, retry.id).await.unwrap().attempt_count,
        3
    );
    assert_eq!(
        intake
            .head(&editor, "retry")
            .await
            .unwrap()
            .extraction_state,
        "failed"
    );
    seed(&owner, "cancel", "source-a").await;
    let cancel = admin
        .request(&publisher, "cancel", 0, "Cancellation fixture")
        .await
        .unwrap();
    fixture.mode.store(6, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::LostLease { job_id: cancel.id }
    );
    assert_eq!(
        intake.head(&editor, "cancel").await.unwrap().intake_version,
        0
    );
    seed(&owner, "manual-race", "source-a").await;
    let manual = admin
        .request(&publisher, "manual-race", 0, "Manual race fixture")
        .await
        .unwrap();
    fixture.mode.store(5, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Cancelled { job_id: manual.id }
    );
    let history = intake
        .history(&editor, "manual-race", None, PageSize::new(10).unwrap())
        .await
        .unwrap();
    assert_eq!(history[0].actor_id.as_deref(), Some("extract-editor"));
    assert_eq!(history[0].worker_job_id, None);
    seed(&owner, "role-race", "source-a").await;
    let role = admin
        .request(&publisher, "role-race", 0, "Role revocation fixture")
        .await
        .unwrap();
    fixture.mode.store(4, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Cancelled { job_id: role.id }
    );
    assert_eq!(
        intake
            .head(&editor, "role-race")
            .await
            .unwrap()
            .intake_version,
        0
    );
    sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='extract-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    seed(&owner, "source-race", "source-a").await;
    let source = admin
        .request(&publisher, "source-race", 0, "Source race fixture")
        .await
        .unwrap();
    fixture.mode.store(3, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Cancelled { job_id: source.id }
    );
    seed(&owner, "expired-grant", "source-a").await;
    sqlx::query("INSERT INTO news.extraction_jobs(source_article_id,expected_intake_version,source_version,requested_by,authorized_until) VALUES('expired-grant',0,1,'extract-publisher',clock_timestamp()-interval '1 second')").execute(&owner).await.unwrap();
    let before = fixture.calls.load(Ordering::SeqCst);
    assert!(queue.claim().await.unwrap().is_none());
    assert_eq!(fixture.calls.load(Ordering::SeqCst), before);
    seed(&owner, "grant-expires-in-flight", "source-a").await;
    let expiring=sqlx::query_scalar::<_,i64>("INSERT INTO news.extraction_jobs(source_article_id,expected_intake_version,source_version,requested_by,authorized_until) VALUES('grant-expires-in-flight',0,1,'extract-publisher',clock_timestamp()+interval '2 seconds') RETURNING id").fetch_one(&owner).await.unwrap();
    fixture.mode.store(11, Ordering::SeqCst);
    let calls = fixture.calls.load(Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Cancelled { job_id: expiring }
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), calls + 1);
    assert_eq!(
        intake
            .head(&editor, "grant-expires-in-flight")
            .await
            .unwrap()
            .intake_version,
        0
    );
    // A storage/acknowledgement failure rolls back text, snapshot and intake version.
    seed(&owner, "atomic", "source-a").await;
    let atomic = admin
        .request(&publisher, "atomic", 0, "Atomic fixture")
        .await
        .unwrap();
    fixture.mode.store(0, Ordering::SeqCst);
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_extraction_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.outcome='succeeded' THEN RAISE EXCEPTION 'fixture acknowledgement failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_fail BEFORE UPDATE ON news.extraction_attempts FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_extraction_ack();").execute(&owner).await.unwrap();
    let snapshots = count(&owner, "intake_snapshots").await;
    let texts = count(&owner, "intake_text").await;
    assert!(matches!(
        queue.work_once().await,
        Err(AppError::Unavailable)
    ));
    assert_eq!(count(&owner, "intake_snapshots").await, snapshots);
    assert_eq!(count(&owner, "intake_text").await, texts);
    assert_eq!(
        intake.head(&editor, "atomic").await.unwrap().intake_version,
        0
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.extraction_attempts; DROP FUNCTION news.fixture_fail_extraction_ack();").execute(&owner).await.unwrap();
    expire(&owner, atomic.id).await;
    assert_eq!(queue.recover_expired().await.unwrap(), 1);
    due(&owner, atomic.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Completed { job_id: atomic.id }
    );
    seed(&owner, "late-owner", "source-a").await;
    let late = admin
        .request(&publisher, "late-owner", 0, "Late lease fixture")
        .await
        .unwrap();
    let old = queue.claim().await.unwrap().unwrap();
    fixture.mode.store(7, Ordering::SeqCst);
    assert_eq!(
        queue.run_claimed(&old).await.unwrap(),
        ExtractionOutcome::LostLease { job_id: late.id }
    );
    assert_eq!(queue.recover_expired().await.unwrap(), 1);
    due(&owner, late.id).await;
    fixture.mode.store(0, Ordering::SeqCst);
    let next = queue.claim().await.unwrap().unwrap();
    assert_eq!(
        queue.run_claimed(&old).await.unwrap(),
        ExtractionOutcome::LostLease { job_id: late.id }
    );
    assert_eq!(
        queue.run_claimed(&next).await.unwrap(),
        ExtractionOutcome::Completed { job_id: late.id }
    );
    seed(&owner, "logout", "source-a").await;
    let logout = admin
        .request(
            &publisher,
            "logout",
            0,
            "Deliberately delegated before logout",
        )
        .await
        .unwrap();
    fixture.mode.store(10, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Completed { job_id: logout.id }
    );
    assert!(matches!(
        admin
            .request(
                &publisher,
                "logout",
                1,
                "Revoked session cannot grant again"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    publisher = login(&app, &auth, "extract-publisher").await;
    seed(&owner, "manual-hint", "hint-source").await;
    seed(&owner, "manual-hint-other", "hint-source").await;
    fixture.mode.store(12, Ordering::SeqCst);
    assert_eq!(
        intake
            .extract(&editor, "manual-hint", 0)
            .await
            .unwrap()
            .extraction_state,
        "failed"
    );
    assert!(matches!(
        admin
            .request(
                &publisher,
                "manual-hint-other",
                0,
                "Cannot bypass manually observed source hint"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert!(sqlx::query_scalar::<_,bool>("SELECT manual_snapshot_id IS NOT NULL AND job_id IS NULL FROM news.source_extraction_limits WHERE source_id='hint-source'").fetch_one(&owner).await.unwrap());
    sqlx::query("UPDATE news.source_extraction_limits SET observed_at=clock_timestamp()-interval '121 seconds' WHERE source_id='hint-source'").execute(&owner).await.unwrap();
    // Retry instructions gate other jobs and manual extraction for the whole source.
    seed(&owner, "hint", "hint-source").await;
    seed(&owner, "hint-wait", "hint-source").await;
    let hint = admin
        .request(&publisher, "hint", 0, "Source hint fixture")
        .await
        .unwrap();
    let waiting = admin
        .request(
            &publisher,
            "hint-wait",
            0,
            "Queued before source instruction",
        )
        .await
        .unwrap();
    fixture.mode.store(8, Ordering::SeqCst);
    assert_eq!(
        queue.work_once().await.unwrap(),
        ExtractionOutcome::Failed { job_id: hint.id }
    );
    let before = fixture.calls.load(Ordering::SeqCst);
    assert!(queue.claim().await.unwrap().is_none());
    assert_eq!(
        admin
            .status(&editor, waiting.id)
            .await
            .unwrap()
            .attempt_count,
        0
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), before);
    assert!(matches!(
        admin
            .request(&publisher, "hint", 1, "Cannot bypass long hint")
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        intake
            .extract(&editor, "hint-wait", 0)
            .await
            .unwrap()
            .extraction_state,
        "metadata_only"
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), before);
    // Rights withdrawal removes machine text through the same existing policy trigger.
    sqlx::query("UPDATE news.sources SET rights_status='unknown',usage_policy='metadata_only',display_policy='link_only',rights_note=NULL,terms_url=NULL,lock_version=lock_version+1 WHERE id='source-a'").execute(&owner).await.unwrap();
    let old = intake
        .snapshot(&editor, "normal", snapshot.record.id)
        .await
        .unwrap();
    assert!(old.text_withheld);
    assert!(old.extracted.is_none());
    assert_eq!(count(&owner, "intake_text").await, 0);
    assert_eq!(
        admin
            .history(&editor, retry.id, None, PageSize::new(1).unwrap())
            .await
            .unwrap()
            .len(),
        1
    );
    let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/world-news");
    let cli = Command::new(binary.canonicalize().unwrap())
        .arg("--work-extraction-once")
        .env_clear()
        .env("DATABASE_URL", &worker_url)
        .env("APP_ENV", "development")
        .output()
        .unwrap();
    assert!(cli.status.success());
    assert!(!String::from_utf8_lossy(&cli.stdout).contains("synthetic-extraction"));
    for sql in [
        "SELECT * FROM news.extraction_jobs",
        "SELECT * FROM news.intake_text",
        "SELECT news.lock_extraction_requester(1)",
        "SELECT news.extraction_retry_allowed('source-a')",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err());
    }
    for sql in [
        "SELECT * FROM news.admin_users",
        "INSERT INTO news.extraction_jobs(source_article_id,expected_intake_version,source_version,requested_by) VALUES('normal',0,0,'extract-publisher')",
        "INSERT INTO news.intake_snapshots(source_article_id,version,source_version,actor_id,kind,policy) VALUES('normal',99,0,'extract-publisher','manual','{}')",
        "UPDATE news.sources SET enabled=false",
        "INSERT INTO news.stories(id) VALUES('cannot-publish')",
        "CREATE TABLE news.forbidden_extraction(id int)",
    ] {
        assert!(sqlx::query(sql).execute(&worker).await.is_err(), "{sql}");
    }
    for sql in [
        "UPDATE news.extraction_jobs SET requested_by='extract-editor'",
        "UPDATE news.extraction_attempts SET outcome='retry' WHERE outcome='succeeded'",
        "DELETE FROM news.extraction_job_actions",
    ] {
        assert!(sqlx::query(sql).execute(&owner).await.is_err(), "{sql}");
    }
    assert_eq!(count(&owner, "stories").await, 0);
    owner.close().await;
    reader.close().await;
    editor_pool.close().await;
    worker.close().await;
}
