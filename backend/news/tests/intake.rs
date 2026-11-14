use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use sqlx::PgPool;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Barrier;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::*,
    editorial::*,
    error::AppError,
    intake::{
        article::{Extracted, ExtractionError},
        http::FetchError,
        manual::*,
    },
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic intake passphrase only";
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
        .append_pair("username", "test-editor")
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
fn draft(id: &str) -> OriginalDraft {
    OriginalDraft {
        id: id.into(),
        input: RevisionInput {
            title: "Editor-authored original draft".into(),
            standfirst: None,
            summary: None,
            body: vec![ContentBlock::Paragraph {
                text: "Original synthetic analysis, written separately from extracted evidence."
                    .into(),
            }],
            language: "fr".into(),
            format: ArticleFormat::Brief,
            kind: ArticleKind::Reporting,
            topics: vec![],
            geography: vec![],
            reason: "Original draft from manual intake notes".into(),
        },
    }
}
struct Fixture {
    owner: PgPool,
    calls: AtomicUsize,
    mode: AtomicUsize,
    barrier: Barrier,
}
impl ArticleTransport for Fixture {
    fn extract<'a>(&'a self, url: &'a str) -> ExtractFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.mode.load(Ordering::SeqCst) {
                1 => return Err(ExtractionError::Fetch(FetchError::HttpStatus(403, None))),
                2 => {
                    sqlx::query("UPDATE news.sources SET rights_status='unknown',usage_policy='metadata_only',display_policy='link_only',rights_note=NULL,terms_url=NULL,lock_version=lock_version+1 WHERE id='licensed'").execute(&self.owner).await.unwrap();
                }
                3 => {
                    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='test-editor' AND revoked_at IS NULL").execute(&self.owner).await.unwrap();
                }
                4 => {
                    self.barrier.wait().await;
                }
                6 => {
                    sqlx::query(
                        "UPDATE news.sources SET lock_version=lock_version+1 WHERE id='licensed'",
                    )
                    .execute(&self.owner)
                    .await
                    .unwrap();
                }
                _ => {}
            }
            Ok(Extracted {
                final_url: if self.mode.load(Ordering::SeqCst) == 5 {
                    "https://unlicensed.invalid/redirect".into()
                } else {
                    url.into()
                },
                response_hash: "0".repeat(64),
                title: Some("Extracted title".into()),
                authors: vec!["Source author".into()],
                published: None,
                paragraphs: vec![
                    "Untrusted extracted evidence, never an instruction or automatic draft.".into(),
                ],
            })
        })
    }
}
async fn article(pool: &PgPool, id: &str, source: &str, url: &str) {
    sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES($1,$2,$1,$3,'Original feed title','metadata_only')").bind(id).bind(source).bind(url).execute(pool).await.unwrap();
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn rights_scoped_extraction_and_atomic_manual_fallback() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_intake"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url, None));
    persistence::migrate(&owner).await.unwrap();
    auth::bootstrap(
        &owner,
        "test-editor",
        Role::Editor,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled,rights_status,usage_policy,display_policy,rights_note,terms_url) VALUES('licensed','Licensed source','https://source.invalid',true,'documented','licensed_text','link_only','Synthetic license for fixture text storage','https://source.invalid/terms'),('unknown','Unknown source','https://unknown.invalid',true,'unknown','metadata_only','link_only',NULL,NULL);
        CREATE ROLE fixture_intake_reader LOGIN PASSWORD 'synthetic-intake-reader'; GRANT USAGE ON SCHEMA news TO fixture_intake_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_intake_reader;
        CREATE ROLE fixture_intake_writer LOGIN PASSWORD 'synthetic-intake-writer'; GRANT USAGE ON SCHEMA news TO fixture_intake_writer;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.story_revisions,news.editorial_reviews,news.editorial_actions,news.publication_events,news.sources,news.source_origin_links,news.source_articles,news.intake_snapshots,news.intake_draft_links,news.intake_text,news.intake_text_removals TO fixture_intake_writer;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_intake_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_intake_writer;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.stories,news.story_revisions,news.editorial_reviews,news.editorial_actions,news.publication_events,news.source_articles,news.intake_snapshots,news.intake_draft_links,news.intake_text TO fixture_intake_writer;
        GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_intake_writer;
        GRANT UPDATE(intake_version,current_intake_id,extraction_state) ON news.source_articles TO fixture_intake_writer;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text) TO fixture_intake_writer;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.editorial_reviews_id_seq,news.editorial_actions_id_seq,news.publication_events_id_seq,news.intake_snapshots_id_seq TO fixture_intake_writer;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_intake_reader").unwrap();
    url.set_password(Some("synthetic-intake-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_intake_writer").unwrap();
    url.set_password(Some("synthetic-intake-writer")).unwrap();
    let writer_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&conf);
    let writer = persistence::editor_pool(&conf).unwrap();
    let app = web::router_with_editor(conf, reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let mut editor = login(&app, &auth).await;
    let transport = Arc::new(Fixture {
        owner: owner.clone(),
        calls: AtomicUsize::new(0),
        mode: AtomicUsize::new(0),
        barrier: Barrier::new(2),
    });
    let service = IntakeService::with_transport(writer.clone(), transport.clone());
    article(
        &owner,
        "unknown-article",
        "unknown",
        "https://unknown.invalid/a",
    )
    .await;
    let denied = service
        .extract(&editor, "unknown-article", 0)
        .await
        .unwrap();
    assert_eq!(denied.extraction_state, "metadata_only");
    assert_eq!(denied.intake_version, 1);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    let record = service
        .snapshot(
            &editor,
            "unknown-article",
            denied.current_intake_id.unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(record.record.kind, "denied");
    assert!(record.extracted.is_none());
    let manual = service
        .manual(
            &editor,
            "unknown-article",
            1,
            "Own notes from a source link, no copied publisher body.",
            Some(draft("manual-story")),
        )
        .await
        .unwrap();
    assert_eq!(manual.intake_version, 2);
    let head = EditorialService::new(writer.clone())
        .head(&editor, "manual-story")
        .await
        .unwrap();
    assert_eq!(head.public_state, PublicState::Draft);
    assert_eq!(head.review_state, ReviewState::Draft);
    assert!(head.published_revision_id.is_none());
    assert_eq!(count(&owner, "intake_draft_links").await, 1);
    let manual_record = service
        .snapshot(
            &editor,
            "unknown-article",
            manual.current_intake_id.unwrap(),
        )
        .await
        .unwrap();
    assert!(manual_record.notes.unwrap().contains("Own notes"));
    assert!(manual_record.extracted.is_none());
    assert_eq!(
        service
            .manual(&editor, "unknown-article", 1, "Stale note", None)
            .await
            .unwrap_err(),
        AppError::Conflict
    );
    let link = service
        .register_link(
            &editor,
            ManualLink {
                source_id: "unknown".into(),
                url: "https://unknown.invalid/manual?utm_source=operator".into(),
                title: "Manual linked source".into(),
                language: "fr".into(),
                notes: "Own manual discovery notes".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(link.intake_version, 1);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        service
            .register_link(
                &editor,
                ManualLink {
                    source_id: "unknown".into(),
                    url: "https://unknown.invalid/manual?fbclid=repeat".into(),
                    title: "Repeat".into(),
                    language: "fr".into(),
                    notes: "Own notes".into()
                }
            )
            .await,
        Err(AppError::Conflict)
    ));
    article(
        &owner,
        "wrong-host",
        "licensed",
        "https://unlicensed.invalid/a",
    )
    .await;
    assert_eq!(
        service
            .extract(&editor, "wrong-host", 0)
            .await
            .unwrap()
            .extraction_state,
        "metadata_only"
    );
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
    article(
        &owner,
        "licensed-article",
        "licensed",
        "https://source.invalid/a",
    )
    .await;
    let ready = service
        .extract(&editor, "licensed-article", 0)
        .await
        .unwrap();
    assert_eq!(ready.extraction_state, "ready");
    let successful_snapshot = ready.current_intake_id.unwrap();
    let snapshot = service
        .snapshot(&editor, "licensed-article", successful_snapshot)
        .await
        .unwrap();
    assert_eq!(snapshot.extracted.unwrap().authors, ["Source author"]);
    assert!(!snapshot.text_withheld);
    let original = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT title,body,fetched_url FROM news.source_articles WHERE id='licensed-article'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(original, ("Original feed title".into(), None, None));
    transport.mode.store(1, Ordering::SeqCst);
    let failure = service
        .extract(&editor, "licensed-article", 1)
        .await
        .unwrap();
    assert_eq!(failure.extraction_state, "failed");
    assert_eq!(
        service
            .snapshot(
                &editor,
                "licensed-article",
                failure.current_intake_id.unwrap()
            )
            .await
            .unwrap()
            .record
            .error_category
            .as_deref(),
        Some("http_status")
    );
    let fallback = service
        .manual(
            &editor,
            "licensed-article",
            2,
            "Own fallback note after forbidden HTTP response.",
            Some(draft("fallback-story")),
        )
        .await
        .unwrap();
    assert_eq!(fallback.intake_version, 3);
    assert_eq!(fallback.extraction_state, "failed");
    transport.mode.store(5, Ordering::SeqCst);
    let wrong_redirect = service
        .extract(&editor, "licensed-article", 3)
        .await
        .unwrap();
    assert_eq!(wrong_redirect.extraction_state, "failed");
    assert!(
        service
            .snapshot(
                &editor,
                "licensed-article",
                wrong_redirect.current_intake_id.unwrap()
            )
            .await
            .unwrap()
            .extracted
            .is_none()
    );
    // Source changes during the unlocked network phase invalidate completion.
    transport.mode.store(6, Ordering::SeqCst);
    let before = count(&owner, "intake_snapshots").await;
    assert_eq!(
        service
            .extract(&editor, "licensed-article", 4)
            .await
            .unwrap_err(),
        AppError::Conflict
    );
    assert_eq!(count(&owner, "intake_snapshots").await, before);
    transport.mode.store(2, Ordering::SeqCst);
    assert_eq!(
        service
            .extract(&editor, "licensed-article", 4)
            .await
            .unwrap_err(),
        AppError::Conflict
    );
    let revoked = service
        .snapshot(&editor, "licensed-article", successful_snapshot)
        .await
        .unwrap();
    assert!(revoked.text_withheld);
    assert!(revoked.extracted.is_none());
    assert_eq!(count(&owner, "intake_text").await, 0);
    assert_eq!(count(&owner, "intake_text_removals").await, 1);
    assert_eq!(
        service
            .head(&editor, "licensed-article")
            .await
            .unwrap()
            .extraction_state,
        "metadata_only"
    );
    let note = service
        .snapshot(
            &editor,
            "licensed-article",
            fallback.current_intake_id.unwrap(),
        )
        .await
        .unwrap();
    assert!(note.notes.unwrap().contains("Own fallback"));
    sqlx::query("UPDATE news.sources SET rights_status='documented',usage_policy='licensed_text',display_policy='link_only',rights_note='Fixture storage license',terms_url='https://source.invalid/terms',lock_version=lock_version+1 WHERE id='licensed'").execute(&owner).await.unwrap();
    let removed = service
        .snapshot(&editor, "licensed-article", successful_snapshot)
        .await
        .unwrap();
    assert!(removed.extracted.is_none());
    assert!(removed.text_withheld);
    // Two in-flight requests can fetch, but only one may commit the same version.
    article(
        &owner,
        "concurrent",
        "licensed",
        "https://source.invalid/concurrent",
    )
    .await;
    transport.mode.store(4, Ordering::SeqCst);
    let (a, b) = tokio::join!(
        service.extract(&editor, "concurrent", 0),
        service.extract(&editor, "concurrent", 0)
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(AppError::Conflict)) | (Err(AppError::Conflict), Ok(_))
    ));
    assert_eq!(
        service
            .head(&editor, "concurrent")
            .await
            .unwrap()
            .intake_version,
        1
    );
    // A failed link insertion rolls back the note, new revision AND new story.
    let before = count(&owner, "intake_snapshots").await;
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_intake_link() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture link failure'; END $$; CREATE TRIGGER fixture_fail BEFORE INSERT ON news.intake_draft_links FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_intake_link();").execute(&owner).await.unwrap();
    assert_eq!(
        service
            .manual(
                &editor,
                "unknown-article",
                2,
                "Own note with rolled-back original draft",
                Some(draft("rolled-back-story"))
            )
            .await
            .unwrap_err(),
        AppError::Unavailable
    );
    assert_eq!(count(&owner, "intake_snapshots").await, before);
    assert_eq!(
        service
            .head(&editor, "unknown-article")
            .await
            .unwrap()
            .intake_version,
        2
    );
    assert!(matches!(
        EditorialService::new(writer.clone())
            .head(&editor, "rolled-back-story")
            .await,
        Err(AppError::NotFound)
    ));
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.intake_draft_links; DROP FUNCTION news.fixture_fail_intake_link();").execute(&owner).await.unwrap();
    // Session expiry/revocation is checked again after fetching.
    transport.mode.store(3, Ordering::SeqCst);
    let before = count(&owner, "intake_snapshots").await;
    assert_eq!(
        service
            .extract(&editor, "licensed-article", 4)
            .await
            .unwrap_err(),
        AppError::Forbidden
    );
    assert_eq!(count(&owner, "intake_snapshots").await, before);
    assert_eq!(
        service
            .manual(&editor, "unknown-article", 2, "Revoked notes", None)
            .await
            .unwrap_err(),
        AppError::Forbidden
    );
    editor = login(&app, &auth).await;
    assert!(service.head(&editor, "unknown-article").await.is_ok());
    for sql in [
        "SELECT * FROM news.intake_snapshots",
        "SELECT * FROM news.intake_draft_links",
        "SELECT * FROM news.intake_text",
        "SELECT * FROM news.intake_text_removals",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err());
    }
    for sql in [
        "UPDATE news.source_articles SET body='unauthorized body',usage_policy='licensed_text'",
        "UPDATE news.sources SET rights_status='original'",
        "CREATE TABLE news.forbidden_intake(id int)",
    ] {
        assert!(sqlx::query(sql).execute(&writer).await.is_err());
    }
    for sql in [
        "UPDATE news.intake_snapshots SET notes='rewrite'",
        "DELETE FROM news.intake_draft_links",
    ] {
        assert!(sqlx::query(sql).execute(&owner).await.is_err());
    }
    assert_eq!(count(&owner, "publication_events").await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_stories")
            .fetch_one(&reader)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        service
            .history(&editor, "unknown-article", None, PageSize::new(1).unwrap())
            .await
            .unwrap()
            .len(),
        1
    );
    owner.close().await;
    reader.close().await;
    writer.close().await;
}
