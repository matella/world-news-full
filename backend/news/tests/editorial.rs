use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::*,
    editorial::*,
    error::AppError,
    persistence,
    repository::Repository,
    web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic editorial passphrase only";
fn config(reader: &str, writer: Option<&str>) -> Config {
    Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(reader.into()),
        "EDITOR_DATABASE_URL" => writer.map(str::to_owned),
        "NEWS_ADMIN_ENABLED" => Some(if writer.is_some() { "true" } else { "false" }.into()),
        "PUBLIC_URL" => Some("https://news.invalid".into()),
        _ => None,
    })
    .unwrap()
}
fn input(title: &str) -> RevisionInput {
    RevisionInput {
        evidence: Some(world_news::sourcing::SourcePackage {
            statement: Some(world_news::sourcing::SourceStatement {
                mode: world_news::sourcing::SourceMode::Original,
                attribution: "Explicit original synthetic fixture, no external reporting".into(),
                limits: None,
            }),
            firsthand: vec![world_news::sourcing::FirsthandEvidence {
                method: world_news::sourcing::FirsthandMethod::Observation,
                claim_ids: vec![],
                attribution: "Synthetic test observation".into(),
                private_note: "Synthetic fixture evidence only".into(),
            }],
            ..Default::default()
        }),
        title: title.into(),
        standfirst: Some("Synthetic introduction".into()),
        summary: None,
        body: vec![ContentBlock::Paragraph {
            text: "Synthetic reporting fixture. No real news or external publisher.".into(),
        }],
        language: "fr".into(),
        format: ArticleFormat::Brief,
        kind: ArticleKind::Reporting,
        topics: vec!["Monde".into()],
        geography: vec![],
        reason: format!("Synthetic change: {title}"),
    }
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
        .map(|value| {
            let cookie = Cookie::parse(value.to_str().unwrap()).unwrap();
            format!("{}={}", cookie.name(), cookie.value())
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, cookies.parse().unwrap());
    auth.authenticate(&headers).await.unwrap()
}
async fn public(app: &Router, path: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
async fn row_count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires fresh disposable DB; use scripts/test_database.py"]
async fn editorial_transactions_preserve_publication_history_and_live_permissions() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_editorial"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url, None));
    assert!(!persistence::schema_available(&owner).await);
    persistence::migrate(&owner).await.unwrap();
    auth::bootstrap(
        &owner,
        "test-editor",
        Role::Editor,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    auth::bootstrap(
        &owner,
        "test-publisher",
        Role::Publisher,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE ROLE fixture_editorial_reader LOGIN PASSWORD 'synthetic-editorial-reader';
        GRANT USAGE ON SCHEMA news TO fixture_editorial_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_editorial_reader;
        CREATE ROLE fixture_editorial_writer LOGIN PASSWORD 'synthetic-editorial-writer';
        GRANT USAGE ON SCHEMA news TO fixture_editorial_writer;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.story_revisions,news.editorial_reviews,news.editorial_actions,news.publication_events TO fixture_editorial_writer;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_editorial_writer;
        GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_editorial_writer;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.stories,news.story_revisions,news.editorial_reviews,news.editorial_actions,news.publication_events TO fixture_editorial_writer;
        GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_editorial_writer;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.editorial_reviews_id_seq,news.editorial_actions_id_seq,news.publication_events_id_seq TO fixture_editorial_writer;")
        .execute(&owner).await.unwrap();
    sqlx::raw_sql("GRANT SELECT ON news.citations,news.source_articles,news.article_origin_labels,news.reporting_origins TO fixture_editorial_writer; GRANT EXECUTE ON FUNCTION news.lock_citation_evidence(text) TO fixture_editorial_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_editorial_reader").unwrap();
    url.set_password(Some("synthetic-editorial-reader"))
        .unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_editorial_writer").unwrap();
    url.set_password(Some("synthetic-editorial-writer"))
        .unwrap();
    let writer_url = url.to_string();
    let config = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&config);
    let writer = persistence::editor_pool(&config).unwrap();
    let app = web::router_with_editor(config, reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let editor = login(&app, &auth, "test-editor").await;
    let publisher = login(&app, &auth, "test-publisher").await;
    let service = EditorialService::new(writer.clone());
    let repository = Repository::new(reader.clone());
    assert!(
        sqlx::query("SELECT * FROM news.editorial_actions")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='test-editor'")
            .execute(&writer)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE news.story_revisions SET title='corrupted'")
            .execute(&writer)
            .await
            .is_err()
    );
    let mut head = service
        .create(&editor, "news-a", input("Original synthetic draft"))
        .await
        .unwrap();
    let first_revision = head.current_revision_id.clone();
    assert_eq!(head.public_state, PublicState::Draft);
    assert_eq!(head.review_state, ReviewState::Draft);
    assert_eq!(head.lock_version, 1);
    assert!(matches!(
        repository.story("news-a").await,
        Err(AppError::NotFound)
    ));
    assert_eq!(public(&app, "/story/news-a").await.0, StatusCode::NOT_FOUND);
    assert!(matches!(
        service
            .publish(&editor, "news-a", 1, "Synthetic publication")
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        service
            .publish(&publisher, "news-a", 1, "Synthetic publication")
            .await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        service
            .review(&editor, "news-a", 1, ReviewOutcome::Approved, None)
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        service
            .review(&publisher, "news-a", 1, ReviewOutcome::Approved, None)
            .await,
        Err(AppError::Conflict)
    ));
    head = service
        .submit(&editor, "news-a", head.lock_version)
        .await
        .unwrap();
    assert_eq!(head.review_state, ReviewState::InReview);
    assert_eq!(head.public_state, PublicState::InReview);
    assert!(matches!(
        service
            .review(
                &publisher,
                "news-a",
                head.lock_version,
                ReviewOutcome::ChangesRequested,
                None
            )
            .await,
        Err(AppError::Validation(_))
    ));
    head = service
        .review(
            &publisher,
            "news-a",
            head.lock_version,
            ReviewOutcome::ChangesRequested,
            Some("private-review-marker; revise the context"),
        )
        .await
        .unwrap();
    assert_eq!(head.review_state, ReviewState::ChangesRequested);
    assert_eq!(head.public_state, PublicState::Draft);
    assert!(matches!(
        service
            .revise(&editor, "news-a", 1, input("Stale must not survive"))
            .await,
        Err(AppError::Conflict)
    ));
    head = service
        .revise(
            &editor,
            "news-a",
            head.lock_version,
            input("Second synthetic draft"),
        )
        .await
        .unwrap();
    head = service
        .submit(&editor, "news-a", head.lock_version)
        .await
        .unwrap();
    head = service
        .review(
            &publisher,
            "news-a",
            head.lock_version,
            ReviewOutcome::Approved,
            None,
        )
        .await
        .unwrap();
    let approved_old = head.current_revision_id.clone();
    assert_eq!(head.review_state, ReviewState::Approved);
    head = service
        .revise(
            &editor,
            "news-a",
            head.lock_version,
            input("First published synthetic version"),
        )
        .await
        .unwrap();
    assert_eq!(head.review_state, ReviewState::Draft);
    assert!(matches!(
        service
            .publish(
                &publisher,
                "news-a",
                head.lock_version,
                "Cannot reuse previous approval"
            )
            .await,
        Err(AppError::Conflict)
    ));
    assert_ne!(head.current_revision_id, approved_old);
    head = service
        .submit(&editor, "news-a", head.lock_version)
        .await
        .unwrap();
    head = service
        .review(
            &publisher,
            "news-a",
            head.lock_version,
            ReviewOutcome::Approved,
            Some("private-approval-marker"),
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .publish(&publisher, "news-a", head.lock_version, " ")
            .await,
        Err(AppError::Validation(_))
    ));
    head = service
        .publish(
            &publisher,
            "news-a",
            head.lock_version,
            "Synthetic first publication after review",
        )
        .await
        .unwrap();
    assert_eq!(head.public_state, PublicState::Published);
    let snapshot_revision = head.current_revision_id.clone();
    assert_eq!(
        repository.story("news-a").await.unwrap().title,
        "First published synthetic version"
    );
    assert!(matches!(
        repository.revision(&first_revision).await,
        Err(AppError::NotFound)
    ));
    sqlx::query("INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES ('synthetic-snapshot','2026-11-13','Europe/Brussels')").execute(&owner).await.unwrap();
    sqlx::query("INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES ('synthetic-snapshot','news-a',$1,'Monde',0,true)").bind(&snapshot_revision).execute(&owner).await.unwrap();
    sqlx::query("UPDATE news.editions SET state='published',published_at=now() WHERE id='synthetic-snapshot'").execute(&owner).await.unwrap();
    let version = head.lock_version;
    let (left, right) = tokio::join!(
        service.revise(
            &editor,
            "news-a",
            version,
            input("Concurrent replacement A")
        ),
        service.revise(
            &editor,
            "news-a",
            version,
            input("Concurrent replacement B")
        )
    );
    head = match (left, right) {
        (Ok(head), Err(AppError::Conflict)) | (Err(AppError::Conflict), Ok(head)) => head,
        _ => panic!("exactly one concurrent edit must commit"),
    };
    assert_eq!(head.lock_version, version + 1);
    assert_eq!(head.public_state, PublicState::Published);
    assert_eq!(head.review_state, ReviewState::Draft);
    assert_eq!(
        repository.story("news-a").await.unwrap().title,
        "First published synthetic version"
    );
    assert!(matches!(
        repository.revision(&head.current_revision_id).await,
        Err(AppError::NotFound)
    ));
    let replacement = service
        .revision(&editor, &head.current_revision_id)
        .await
        .unwrap()
        .title;
    head = service
        .submit(&editor, "news-a", head.lock_version)
        .await
        .unwrap();
    assert_eq!(
        repository.story("news-a").await.unwrap().title,
        "First published synthetic version"
    );
    head = service
        .review(
            &publisher,
            "news-a",
            head.lock_version,
            ReviewOutcome::Approved,
            None,
        )
        .await
        .unwrap();
    let events = row_count(&owner, "publication_events").await;
    let actions = row_count(&owner, "editorial_actions").await;
    sqlx::raw_sql("CREATE FUNCTION news.test_reject_editorial_action() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='corrected' THEN RAISE EXCEPTION 'synthetic interruption' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_reject_editorial_action BEFORE INSERT ON news.editorial_actions FOR EACH ROW EXECUTE FUNCTION news.test_reject_editorial_action();")
        .execute(&owner).await.unwrap();
    assert!(matches!(
        service
            .publish(
                &publisher,
                "news-a",
                head.lock_version,
                "Synthetic correction"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(row_count(&owner, "publication_events").await, events);
    assert_eq!(row_count(&owner, "editorial_actions").await, actions);
    assert_eq!(
        service.head(&editor, "news-a").await.unwrap().lock_version,
        head.lock_version
    );
    assert_eq!(
        repository.story("news-a").await.unwrap().title,
        "First published synthetic version"
    );
    sqlx::raw_sql("DROP TRIGGER test_reject_editorial_action ON news.editorial_actions; DROP FUNCTION news.test_reject_editorial_action();").execute(&owner).await.unwrap();
    head = service
        .publish(
            &publisher,
            "news-a",
            head.lock_version,
            "Synthetic correction: updated supported context",
        )
        .await
        .unwrap();
    assert_eq!(repository.story("news-a").await.unwrap().title, replacement);
    let snapshot = repository
        .edition_entries("synthetic-snapshot")
        .await
        .unwrap();
    assert_eq!(
        snapshot[0].revision.as_ref().unwrap().title,
        "First published synthetic version"
    );
    let (status, html) = public(&app, &format!("/story/news-a?revision={snapshot_revision}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains("First published synthetic version")
            && !html.contains("private-approval-marker")
            && !html.contains("private-review-marker")
    );
    let reason=sqlx::query_scalar::<_,String>("SELECT public_reason FROM news.public_notices WHERE story_id='news-a' AND action='corrected'").fetch_one(&reader).await.unwrap();
    assert!(reason.contains("updated supported context"));
    let history = service.history(&editor, "news-a", None).await.unwrap();
    assert_eq!(history.len(), 4);
    assert!(history.iter().all(|r| {
        r.actor_id == "test-editor"
            && r.change_reason
                .as_ref()
                .is_some_and(|r| r.starts_with("Synthetic change:"))
    }));
    let previous = service
        .history(&editor, "news-a", Some(history[0].number))
        .await
        .unwrap();
    assert_eq!(previous.len(), 3);
    assert!(previous.iter().all(|r| r.id != history[0].id));
    assert_eq!(
        service
            .revision(&editor, &first_revision)
            .await
            .unwrap()
            .title,
        "Original synthetic draft"
    );
    assert!(
        sqlx::query("UPDATE news.editorial_actions SET reason='rewrite history'")
            .execute(&owner)
            .await
            .is_err()
    );
    head = service
        .revise(
            &editor,
            "news-a",
            head.lock_version,
            input("Private pending work at withdrawal"),
        )
        .await
        .unwrap();
    let published = head.published_revision_id.clone().unwrap();
    head = service
        .withdraw(
            &publisher,
            "news-a",
            head.lock_version,
            "Synthetic withdrawal: reporting no longer reliable",
        )
        .await
        .unwrap();
    assert_eq!(head.public_state, PublicState::Withdrawn);
    assert_ne!(head.current_revision_id, published);
    let withdrawn_revision=sqlx::query_scalar::<_,String>("SELECT revision_id FROM news.public_notices WHERE story_id='news-a' AND action='withdrawn'").fetch_one(&reader).await.unwrap();
    assert_eq!(withdrawn_revision, published);
    assert!(matches!(
        service
            .revise(
                &editor,
                "news-a",
                head.lock_version,
                input("Cannot silently revive")
            )
            .await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        service
            .publish(
                &publisher,
                "news-a",
                head.lock_version,
                "Cannot silently revive"
            )
            .await,
        Err(AppError::Conflict)
    ));
    assert_eq!(public(&app, "/story/news-a").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        public(&app, &format!("/story/news-a?revision={snapshot_revision}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, html) = public(&app, "/edition/2026-11-13").await;
    assert!(html.contains("Article retiré") && !html.contains("First published synthetic version"));

    let mut empty = input("Empty draft");
    empty.body.clear();
    let empty = service.create(&editor, "empty", empty).await.unwrap();
    assert!(matches!(
        service.submit(&editor, "empty", empty.lock_version).await,
        Err(AppError::Validation(_))
    ));
    let mut invalid = input("NUL is rejected");
    invalid.body = vec![ContentBlock::Paragraph {
        text: "unsupported\0input".into(),
    }];
    assert!(matches!(
        service.create(&editor, "invalid", invalid).await,
        Err(AppError::Validation(_))
    ));
    let before = (
        row_count(&owner, "stories").await,
        row_count(&owner, "story_revisions").await,
        row_count(&owner, "editorial_actions").await,
    );
    sqlx::raw_sql("CREATE FUNCTION news.test_reject_create() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.story_id='reject-create' THEN RAISE EXCEPTION 'synthetic create interruption' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_reject_create BEFORE INSERT ON news.story_revisions FOR EACH ROW EXECUTE FUNCTION news.test_reject_create();")
        .execute(&owner).await.unwrap();
    assert!(
        service
            .create(&editor, "reject-create", input("Atomic creation"))
            .await
            .is_err()
    );
    assert_eq!(
        (
            row_count(&owner, "stories").await,
            row_count(&owner, "story_revisions").await,
            row_count(&owner, "editorial_actions").await
        ),
        before
    );
    sqlx::raw_sql("DROP TRIGGER test_reject_create ON news.story_revisions; DROP FUNCTION news.test_reject_create();").execute(&owner).await.unwrap();
    let mut bulk = service
        .create(&editor, "history", input("History 0"))
        .await
        .unwrap();
    for index in 1..23 {
        bulk = service
            .revise(
                &editor,
                "history",
                bulk.lock_version,
                input(&format!("History {index}")),
            )
            .await
            .unwrap();
    }
    let page = service.history(&editor, "history", None).await.unwrap();
    assert_eq!(page.len(), 20);
    let tail = service
        .history(&editor, "history", Some(page.last().unwrap().number))
        .await
        .unwrap();
    assert_eq!(tail.len(), 3);
    assert!(tail.iter().all(|r| r.number < page.last().unwrap().number));
    // Reuse previously authenticated Session objects after DB changes: service
    // transaction checks must reject them even without another HTTP guard pass.
    let mut role_change = owner.begin().await.unwrap();
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='test-publisher'")
        .execute(&mut *role_change)
        .await
        .unwrap();
    let operation = service.review(
        &publisher,
        "empty",
        empty.lock_version,
        ReviewOutcome::Approved,
        None,
    );
    tokio::pin!(operation);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut operation)
            .await
            .is_err(),
        "editorial action must wait for the identity row lock"
    );
    role_change.commit().await.unwrap();
    assert!(matches!(operation.await, Err(AppError::Forbidden)));
    sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='test-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE news.admin_sessions SET created_at=now()-interval '9 hours',expires_at=now()-interval '1 hour' WHERE user_id='test-editor'").execute(&owner).await.unwrap();
    assert!(matches!(
        service
            .revise(
                &editor,
                "history",
                bulk.lock_version,
                input("Expired must not write")
            )
            .await,
        Err(AppError::Forbidden)
    ));
    auth::disable(&owner, "test-publisher").await.unwrap();
    assert!(matches!(
        service
            .withdraw(
                &publisher,
                "news-a",
                head.lock_version,
                "Revoked must not write"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    assert_eq!(
        service
            .history(&login(&app, &auth, "test-editor").await, "history", None)
            .await
            .unwrap()
            .len(),
        20
    );
    assert!(row_count(&owner, "admin_audit").await > 0);
    assert_eq!(public(&app, "/health/ready").await.0, StatusCode::OK);
    writer.close().await;
    reader.close().await;
    owner.close().await;
}
