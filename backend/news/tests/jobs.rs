use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::{path::Path, process::Command, sync::Arc};
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::PageSize,
    error::AppError,
    intake::jobs::*,
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic jobs passphrase only";
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
async fn batch(pool: &PgPool, slug: &str) -> i64 {
    let id=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(source_id,feed_id,source_version,feed_version,outcome,entry_count,finished_at) VALUES('job-source','job-feed',0,0,'modified',1,clock_timestamp()) RETURNING id").fetch_one(pool).await.unwrap();
    let meta = json!({"schema":1,"source_id":"job-source","feed_id":"job-feed","source_version":0,"feed_version":0,"feed":{"final_url":"https://source.invalid/rss","format":"rss2","response_bytes":200,"title":"Synthetic feed","entries":[{"parser_id":"not-identity","title":"Synthetic title","links":[format!("https://source.invalid/{slug}")],"published":null,"updated":null,"language":"fr","authors":[]}]}});
    let hash = format!("{:x}", Sha256::digest(meta.to_string().as_bytes()));
    sqlx::query("INSERT INTO news.feed_batches(attempt_id,metadata,content_hash) VALUES($1,$2,$3)")
        .bind(id)
        .bind(Json(meta))
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();
    id
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn due(pool: &PgPool, id: i64) {
    sqlx::query("UPDATE news.normalization_jobs SET available_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(id).execute(pool).await.unwrap();
}
async fn expire(pool: &PgPool, id: i64) {
    sqlx::query("UPDATE news.normalization_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1 AND state='running'").bind(id).execute(pool).await.unwrap();
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn durable_jobs_recover_fence_retry_and_commit_atomically() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_jobs"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url, None));
    persistence::migrate(&owner).await.unwrap();
    for (user, role) in [
        ("job-publisher", Role::Publisher),
        ("job-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, user, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES('job-source','Synthetic source','https://source.invalid',true); INSERT INTO news.feeds(id,source_id,url,enabled) VALUES('job-feed','job-source','https://source.invalid/rss',true);
        CREATE ROLE fixture_jobs_reader LOGIN PASSWORD 'synthetic-jobs-reader'; GRANT USAGE ON SCHEMA news TO fixture_jobs_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_jobs_reader;
        CREATE ROLE fixture_jobs_admin LOGIN PASSWORD 'synthetic-jobs-admin'; GRANT USAGE ON SCHEMA news TO fixture_jobs_admin;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.normalization_jobs,news.normalization_attempts TO fixture_jobs_admin;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_jobs_admin; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_jobs_admin;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.normalization_jobs,news.normalization_job_actions TO fixture_jobs_admin;
        GRANT UPDATE ON news.normalization_jobs,news.normalization_attempts TO fixture_jobs_admin;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.normalization_jobs_id_seq,news.normalization_job_actions_id_seq TO fixture_jobs_admin;
        CREATE ROLE fixture_jobs_worker LOGIN PASSWORD 'synthetic-jobs-worker'; GRANT USAGE ON SCHEMA news TO fixture_jobs_worker;
        GRANT SELECT ON news.schema_version,news.feed_batches,news.feed_fetch_attempts,news.source_articles,news.normalized_batches,news.source_observations,news.normalization_jobs,news.normalization_attempts TO fixture_jobs_worker;
        GRANT SELECT(id,name) ON news.sources TO fixture_jobs_worker;
        GRANT INSERT ON news.source_articles,news.normalized_batches,news.source_observations,news.normalization_attempts TO fixture_jobs_worker;
        GRANT UPDATE ON news.normalization_jobs,news.normalization_attempts TO fixture_jobs_worker;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_feed_for_normalization(text) TO fixture_jobs_worker;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_jobs_reader").unwrap();
    url.set_password(Some("synthetic-jobs-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_jobs_admin").unwrap();
    url.set_password(Some("synthetic-jobs-admin")).unwrap();
    let admin_url = url.to_string();
    url.set_username("fixture_jobs_worker").unwrap();
    url.set_password(Some("synthetic-jobs-worker")).unwrap();
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
    let publisher = login(&app, &auth, "job-publisher").await;
    let editor = login(&app, &auth, "job-editor").await;
    let admin = JobAdmin::new(admin_pool.clone());
    let queue = Queue::new(worker.clone());
    let first_batch = batch(&owner, "first").await;
    assert!(matches!(
        admin
            .enqueue(&editor, first_batch, 3, "Editor cannot authorize jobs")
            .await,
        Err(AppError::Forbidden)
    ));
    let first = admin
        .enqueue(&publisher, first_batch, 3, "Normalize synthetic batch")
        .await
        .unwrap();
    let duplicate = admin
        .enqueue(
            &publisher,
            first_batch,
            1,
            "Duplicate request does not change budget",
        )
        .await
        .unwrap();
    assert_eq!(first.id, duplicate.id);
    assert_eq!(duplicate.max_attempts, 3);
    assert_eq!(count(&owner, "normalization_job_actions").await, 1);
    let (a, b) = tokio::join!(queue.claim(), queue.claim());
    let lease = match (a.unwrap(), b.unwrap()) {
        (Some(a), None) | (None, Some(a)) => a,
        _ => panic!("one claim winner required"),
    };
    assert_eq!(lease.job_id(), first.id);
    queue.heartbeat(&lease).await.unwrap();
    assert_eq!(
        queue.run_claimed(&lease).await.unwrap(),
        WorkOutcome::Completed { job_id: first.id }
    );
    assert_eq!(count(&owner, "source_articles").await, 1);
    assert_eq!(count(&owner, "normalized_batches").await, 1);
    assert_eq!(
        admin.status(&editor, first.id).await.unwrap().state,
        "succeeded"
    );
    assert_eq!(
        queue.run_claimed(&lease).await.unwrap(),
        WorkOutcome::LostLease { job_id: first.id }
    );
    let status = admin.status(&publisher, first.id).await.unwrap();
    admin
        .replay(
            &publisher,
            first.id,
            status.lock_version,
            "Replay confirmed durable batch",
        )
        .await
        .unwrap();
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Completed { job_id: first.id }
    );
    assert_eq!(count(&owner, "source_articles").await, 1);
    assert_eq!(count(&owner, "normalized_batches").await, 1);
    assert_eq!(
        admin
            .history(&editor, first.id, None, PageSize::new(10).unwrap())
            .await
            .unwrap()
            .len(),
        2
    );
    // A new Queue after abandonment reaps the expired lease, then observes backoff.
    let crash = admin
        .enqueue(
            &publisher,
            batch(&owner, "crash-recovery").await,
            3,
            "Crash fixture",
        )
        .await
        .unwrap();
    let abandoned = queue.claim().await.unwrap().unwrap();
    expire(&owner, crash.id).await;
    assert_eq!(queue.heartbeat(&abandoned).await, Err(AppError::Conflict));
    let restarted = Queue::new(worker.clone());
    assert!(restarted.claim().await.unwrap().is_none());
    let recovered = admin.status(&editor, crash.id).await.unwrap();
    assert_eq!(recovered.state, "retry");
    assert_eq!(recovered.last_error.as_deref(), Some("lease_expired"));
    assert!(recovered.available_at > chrono::Utc::now());
    due(&owner, crash.id).await;
    let new_lease = restarted.claim().await.unwrap().unwrap();
    assert_eq!(
        restarted.run_claimed(&abandoned).await.unwrap(),
        WorkOutcome::LostLease { job_id: crash.id }
    );
    assert_eq!(
        restarted.run_claimed(&new_lease).await.unwrap(),
        WorkOutcome::Completed { job_id: crash.id }
    );
    let hist = admin
        .history(&editor, crash.id, None, PageSize::new(10).unwrap())
        .await
        .unwrap();
    assert_eq!(hist[0].outcome, "succeeded");
    assert_eq!(hist[1].outcome, "lease_expired");
    assert!(hist[0].generation > hist[1].generation);
    // Cancellation wins before handler entry: old lease cannot normalize anything.
    let cancel_batch = batch(&owner, "cancelled").await;
    let cancelled = admin
        .enqueue(&publisher, cancel_batch, 3, "Cancel fixture")
        .await
        .unwrap();
    let old = queue.claim().await.unwrap().unwrap();
    let current = admin.status(&editor, cancelled.id).await.unwrap();
    let cancelled = admin
        .cancel(
            &publisher,
            cancelled.id,
            current.lock_version,
            "Operator cancellation",
        )
        .await
        .unwrap();
    assert_eq!(cancelled.state, "cancelled");
    let before = count(&owner, "source_articles").await;
    assert_eq!(
        queue.run_claimed(&old).await.unwrap(),
        WorkOutcome::LostLease {
            job_id: cancelled.id
        }
    );
    assert_eq!(count(&owner, "source_articles").await, before);
    assert!(matches!(
        admin
            .cancel(
                &publisher,
                cancelled.id,
                current.lock_version,
                "Stale cancellation"
            )
            .await,
        Err(AppError::Conflict)
    ));
    // Unknown/disabled source is retryable but bounded, then a terminal failure.
    let held = admin
        .enqueue(
            &publisher,
            batch(&owner, "held").await,
            2,
            "Disabled fixture",
        )
        .await
        .unwrap();
    sqlx::query("UPDATE news.sources SET enabled=false WHERE id='job-source'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Deferred { job_id: held.id }
    );
    assert_eq!(admin.status(&editor, held.id).await.unwrap().state, "retry");
    due(&owner, held.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Failed { job_id: held.id }
    );
    let exhausted = admin.status(&editor, held.id).await.unwrap();
    assert_eq!(exhausted.state, "failed");
    assert_eq!(exhausted.attempt_count, 2);
    assert_eq!(exhausted.last_error.as_deref(), Some("held_disabled"));
    assert_eq!(queue.work_once().await.unwrap(), WorkOutcome::Idle);
    sqlx::query("UPDATE news.sources SET enabled=true WHERE id='job-source'")
        .execute(&owner)
        .await
        .unwrap();
    // Last allowed claim abandoned -> terminal; recovery must not grant a sixth try.
    let last = admin
        .enqueue(
            &publisher,
            batch(&owner, "last-expired").await,
            1,
            "Final attempt fixture",
        )
        .await
        .unwrap();
    queue.claim().await.unwrap().unwrap();
    expire(&owner, last.id).await;
    assert!(queue.claim().await.unwrap().is_none());
    assert_eq!(
        admin.status(&editor, last.id).await.unwrap().state,
        "failed"
    );
    // Success acknowledgement failure must roll back articles/receipt before retry.
    let atomic = admin
        .enqueue(
            &publisher,
            batch(&owner, "atomic").await,
            3,
            "Atomic acknowledgement fixture",
        )
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_job_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.outcome='succeeded' THEN RAISE EXCEPTION 'fixture ack failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_fail_ack BEFORE UPDATE ON news.normalization_attempts FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_job_ack();").execute(&owner).await.unwrap();
    let articles = count(&owner, "source_articles").await;
    let receipts = count(&owner, "normalized_batches").await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Deferred { job_id: atomic.id }
    );
    assert_eq!(count(&owner, "source_articles").await, articles);
    assert_eq!(count(&owner, "normalized_batches").await, receipts);
    assert_eq!(
        admin.status(&editor, atomic.id).await.unwrap().state,
        "retry"
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail_ack ON news.normalization_attempts; DROP FUNCTION news.fixture_fail_job_ack();").execute(&owner).await.unwrap();
    due(&owner, atomic.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Completed { job_id: atomic.id }
    );
    // Expiry between handler entry and ack also rolls back the whole transaction.
    let mid = admin
        .enqueue(
            &publisher,
            batch(&owner, "expired-mid-handler").await,
            3,
            "Mid-handler expiry fixture",
        )
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_expire_job() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE news.normalization_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE batch_id=NEW.attempt_id AND state='running'; RETURN NEW; END $$; CREATE TRIGGER fixture_expire BEFORE INSERT ON news.normalized_batches FOR EACH ROW EXECUTE FUNCTION news.fixture_expire_job();").execute(&owner).await.unwrap();
    let before = count(&owner, "source_articles").await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Deferred { job_id: mid.id }
    );
    assert_eq!(count(&owner, "source_articles").await, before);
    assert_eq!(admin.status(&editor, mid.id).await.unwrap().state, "retry");
    sqlx::raw_sql("DROP TRIGGER fixture_expire ON news.normalized_batches; DROP FUNCTION news.fixture_expire_job();").execute(&owner).await.unwrap();
    due(&owner, mid.id).await;
    assert_eq!(
        queue.work_once().await.unwrap(),
        WorkOutcome::Completed { job_id: mid.id }
    );
    // Cancellation racing a handler commit cannot undo acknowledged work.
    let race = admin
        .enqueue(
            &publisher,
            batch(&owner, "cancel-race").await,
            3,
            "Cancellation race fixture",
        )
        .await
        .unwrap();
    let race_lease = queue.claim().await.unwrap().unwrap();
    let before_race = admin.status(&editor, race.id).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_pause_job() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(89123); RETURN NEW; END $$; CREATE TRIGGER fixture_pause BEFORE INSERT ON news.normalized_batches FOR EACH ROW EXECUTE FUNCTION news.fixture_pause_job();").execute(&owner).await.unwrap();
    let mut gate = owner.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(89123)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let runner = queue.clone();
    let running = tokio::spawn(async move { runner.run_claimed(&race_lease).await });
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {let waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND classid=0 AND objid=89123 AND NOT granted)").fetch_one(&owner).await.unwrap();if waiting{break;}tokio::task::yield_now().await;}
    }).await.unwrap();
    let controller = admin.clone();
    let actor = publisher.clone();
    let racing_cancel = tokio::spawn(async move {
        controller
            .cancel(
                &actor,
                race.id,
                before_race.lock_version,
                "Cancellation while handler holds job lock",
            )
            .await
    });
    sqlx::query("SELECT pg_advisory_unlock(89123)")
        .execute(&mut *gate)
        .await
        .unwrap();
    drop(gate);
    assert_eq!(
        running.await.unwrap().unwrap(),
        WorkOutcome::Completed { job_id: race.id }
    );
    assert!(matches!(
        racing_cancel.await.unwrap(),
        Err(AppError::Conflict)
    ));
    sqlx::raw_sql("DROP TRIGGER fixture_pause ON news.normalized_batches; DROP FUNCTION news.fixture_pause_job();").execute(&owner).await.unwrap();
    // Real one-shot CLI uses the worker's restricted DB identity, no human session.
    let cli = admin
        .enqueue(&publisher, batch(&owner, "cli").await, 3, "CLI fixture")
        .await
        .unwrap();
    let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/world-news");
    let output = Command::new(binary.canonicalize().unwrap())
        .arg("--work-normalization-once")
        .env_clear()
        .env("DATABASE_URL", &worker_url)
        .env("APP_ENV", "development")
        .output()
        .unwrap();
    assert!(output.status.success(), "CLI failed");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["job_id"], cli.id);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-jobs"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("https://source.invalid"));
    let stats = admin.stats(&editor).await.unwrap();
    assert!(stats.succeeded >= 5);
    assert_eq!(stats.failed, 2);
    assert_eq!(stats.cancelled, 1);
    assert_eq!(stats.queued, 0);
    assert!(stats.oldest_ready_seconds.is_none());
    let history = admin
        .history(&editor, first.id, None, PageSize::new(1).unwrap())
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        admin
            .history(
                &editor,
                first.id,
                Some(history[0].generation),
                PageSize::new(1).unwrap()
            )
            .await
            .unwrap()
            .len(),
        1
    );
    // Cached publisher role and revoked sessions cannot authorize replay.
    let completed = admin.status(&publisher, first.id).await.unwrap();
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='job-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        admin
            .replay(&publisher, first.id, completed.lock_version, "Stale role")
            .await,
        Err(AppError::Forbidden)
    ));
    sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='job-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='job-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        admin
            .replay(
                &publisher,
                first.id,
                completed.lock_version,
                "Revoked session"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    for sql in [
        "SELECT * FROM news.normalization_jobs",
        "SELECT * FROM news.normalization_attempts",
        "SELECT * FROM news.normalization_job_actions",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err());
    }
    for sql in [
        "SELECT * FROM news.admin_users",
        "INSERT INTO news.normalization_job_actions(job_id,actor_id,action,reason,job_version) VALUES(1,'job-publisher','replayed','invented',1)",
        "INSERT INTO news.stories(id) VALUES('worker-cannot-publish')",
        "UPDATE news.sources SET enabled=false",
        "CREATE TABLE news.forbidden_job(id int)",
    ] {
        assert!(sqlx::query(sql).execute(&worker).await.is_err(), "{sql}");
    }
    for sql in [
        "UPDATE news.normalization_attempts SET outcome='retry' WHERE outcome='succeeded'",
        "DELETE FROM news.normalization_job_actions",
        "DELETE FROM news.normalization_attempts",
        "UPDATE news.normalization_jobs SET batch_id=-1",
        "DELETE FROM news.normalization_jobs",
    ] {
        assert!(sqlx::query(sql).execute(&owner).await.is_err(), "{sql}");
    }
    assert_eq!(count(&owner, "stories").await, 0);
    owner.close().await;
    reader.close().await;
    admin_pool.close().await;
    worker.close().await;
}
