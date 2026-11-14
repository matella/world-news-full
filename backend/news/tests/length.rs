use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;

use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::*,
    editorial::{EditorialService, Head, ReviewOutcome, RevisionInput},
    error::AppError,
    persistence,
    sourcing::*,
    web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic length verification passphrase";
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
async fn login(app: &Router, auth: &AuthService, user: &str) -> (Session, String) {
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
    (auth.authenticate(&headers).await.unwrap(), cookies)
}
fn draft(count: usize, language: &str, override_reason: Option<&str>) -> RevisionInput {
    RevisionInput {
        title: "Synthetic editorial length".into(),
        standfirst: Some("Overview words are excluded".into()),
        summary: Some("An explicit summary also does not affect body count".into()),
        body: vec![ContentBlock::Paragraph {
            text: std::iter::repeat_n("mot", count)
                .collect::<Vec<_>>()
                .join(" "),
        }],
        language: language.into(),
        format: ArticleFormat::Brief,
        kind: ArticleKind::Reporting,
        topics: vec![],
        geography: vec![],
        reason: "Synthetic length fixture".into(),
        length_override: override_reason.map(str::to_owned),
        evidence: Some(SourcePackage {
            statement: Some(SourceStatement {
                mode: SourceMode::Original,
                attribution: "Synthetic original observation".into(),
                limits: Some(
                    "Synthetic single reference; no claim of independent reporting".into(),
                ),
            }),
            firsthand: vec![FirsthandEvidence {
                method: FirsthandMethod::Observation,
                claim_ids: vec![],
                attribution: "Synthetic method declaration".into(),
                private_note: "PRIVATE-LENGTH-EVIDENCE".into(),
            }],
            bindings: vec![Binding {
                source_article_id: "reference".into(),
                anchor_id: None,
                location: Some("Synthetic general reference".into()),
            }],
            disagreements: vec![],
        }),
    }
}
async fn approve(service: &EditorialService, session: &Session, id: &str, head: Head) -> Head {
    let head = service
        .submit(session, id, head.lock_version)
        .await
        .unwrap();
    service
        .review(
            session,
            id,
            head.lock_version,
            ReviewOutcome::Approved,
            None,
        )
        .await
        .unwrap()
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    origin: bool,
    fields: &[(&str, &str)],
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if origin {
        builder = builder.header(header::ORIGIN, "https://news.invalid");
    }
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in fields {
        form.append_pair(key, value);
    }
    builder = builder.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    app.clone()
        .oneshot(builder.body(Body::from(form.finish())).unwrap())
        .await
        .unwrap()
}
async fn text(response: axum::response::Response) -> String {
    String::from_utf8(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn length_overrides_preserve_prose_evidence_and_exact_review_authority() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_length"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    for (id, role) in [
        ("length-publisher", Role::Publisher),
        ("length-editor", Role::Editor),
        ("length-revoked", Role::Editor),
    ] {
        auth::bootstrap(&owner, id, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url) VALUES('reference','Synthetic reference','https://reference.invalid'); INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES('reference','reference','reference','https://reference.invalid/report','Synthetic reference','metadata_only');
        CREATE ROLE fixture_length_reader LOGIN PASSWORD 'synthetic-length-reader'; GRANT USAGE ON SCHEMA news TO fixture_length_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations TO fixture_length_reader;
        CREATE ROLE fixture_length_writer LOGIN PASSWORD 'synthetic-length-writer'; GRANT USAGE ON SCHEMA news TO fixture_length_writer; GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.story_revisions,news.citations,news.source_articles,news.reporting_origins,news.article_origin_labels,news.publication_events,news.editorial_reviews TO fixture_length_writer; GRANT INSERT ON news.admin_sessions,news.admin_audit,news.story_revisions,news.citations,news.editorial_actions TO fixture_length_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_length_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_length_writer; GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_length_writer; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.editorial_actions_id_seq TO fixture_length_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_length_reader").unwrap();
    url.set_password(Some("synthetic-length-reader")).unwrap();
    let reader_url = url.to_string();
    let reader = persistence::pool(&config(&reader_url, None));
    url.set_username("fixture_length_writer").unwrap();
    url.set_password(Some("synthetic-length-writer")).unwrap();
    let writer_url = url.to_string();
    let writer = persistence::pool(&config(&writer_url, None));
    let app = web::router_with_editor(
        Arc::new(config(&reader_url, Some(&writer_url))),
        reader.clone(),
        Some(writer.clone()),
    );
    let auth = AuthService::new(
        owner.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let (publisher, _) = login(&app, &auth, "length-publisher").await;
    let (editor, cookie) = login(&app, &auth, "length-editor").await;
    let (revoked, _) = login(&app, &auth, "length-revoked").await;
    let service = EditorialService::new(owner.clone());
    let head = service
        .create(&editor, "short", draft(79, "fr", None))
        .await
        .unwrap();
    let initial = head.current_revision_id.clone();
    let head = approve(&service, &publisher, "short", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "short",
                head.lock_version,
                "Missing override must fail"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let report = service
        .length_report(&editor, &head.current_revision_id)
        .await
        .unwrap();
    assert_eq!(report.current.words, 79);
    assert_eq!(report.reading_minutes, 1);
    assert!(report.current.needs_override());
    let html = text(
        request(
            &app,
            "GET",
            "/admin/drafts/short",
            Some(&cookie),
            false,
            &[],
        )
        .await,
    )
    .await;
    assert!(html.contains("79 mots dans le corps"));
    assert!(html.contains("80–150 mots"));
    assert!(html.contains("Motif public de dérogation"));
    let document = html_scraper::Html::parse_document(&html);
    let selector = html_scraper::Selector::parse("input[name=csrf]").unwrap();
    let csrf = document
        .select(&selector)
        .next()
        .unwrap()
        .value()
        .attr("value")
        .unwrap()
        .to_owned();
    let version = head.lock_version.to_string();
    let path = "/admin/drafts/short/length";
    assert_eq!(
        request(
            &app,
            "POST",
            path,
            Some(&cookie),
            true,
            &[("version", &version), ("reason", "No CSRF")]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "POST",
            path,
            Some(&cookie),
            false,
            &[
                ("csrf", &csrf),
                ("version", &version),
                ("reason", "No Origin")
            ]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let excessive = "x".repeat(2001);
    assert_eq!(
        request(
            &app,
            "POST",
            path,
            Some(&cookie),
            true,
            &[
                ("csrf", &csrf),
                ("version", &version),
                ("reason", "Invalid override"),
                ("override_reason", &excessive)
            ]
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        service.head(&editor, "short").await.unwrap().lock_version,
        head.lock_version
    );
    let response = request(
        &app,
        "POST",
        path,
        Some(&cookie),
        true,
        &[
            ("csrf", &csrf),
            ("version", &version),
            ("reason", "Keep the urgent notice concise"),
            (
                "override_reason",
                "A short urgent notice; preserve the complete supported account without padding.",
            ),
        ],
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        request(
            &app,
            "POST",
            path,
            Some(&cookie),
            true,
            &[
                ("csrf", &csrf),
                ("version", &version),
                ("reason", "Stale override"),
                ("override_reason", "Stale must not survive")
            ]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let head = service.head(&editor, "short").await.unwrap();
    assert!(matches!(
        head.review_state,
        world_news::editorial::ReviewState::Draft
    ));
    let retained: serde_json::Value =
        sqlx::query_scalar("SELECT body FROM news.story_revisions WHERE id=$1")
            .bind(&head.current_revision_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    let original: serde_json::Value =
        sqlx::query_scalar("SELECT body FROM news.story_revisions WHERE id=$1")
            .bind(&initial)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(retained, original);
    let first:Vec<(String,Option<String>,i64)>=sqlx::query_as("SELECT source_article_id,anchor_id,origin_version FROM news.citations WHERE revision_id=$1").bind(&initial).fetch_all(&owner).await.unwrap();
    let second:Vec<(String,Option<String>,i64)>=sqlx::query_as("SELECT source_article_id,anchor_id,origin_version FROM news.citations WHERE revision_id=$1").bind(&head.current_revision_id).fetch_all(&owner).await.unwrap();
    assert_eq!(first, second);
    assert!(matches!(
        service
            .publish(
                &publisher,
                "short",
                head.lock_version,
                "Old approval cannot transfer"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let head = approve(&service, &publisher, "short", head).await;
    let head = service
        .publish(
            &publisher,
            "short",
            head.lock_version,
            "Reviewed concise synthetic notice",
        )
        .await
        .unwrap();
    let published = head.current_revision_id.clone();
    let html = text(request(&app, "GET", "/story/short", None, false, &[]).await).await;
    assert!(html.contains("79 mots dans le corps"));
    assert!(html.contains("Temps de lecture estimé : environ 1 min"));
    assert!(html.contains("A short urgent notice"));
    assert!(!html.contains("PRIVATE-LENGTH-EVIDENCE"));
    assert!(!html.contains("length-publisher"));
    let mut changed = world_news::domain::EditorialPolicy::repository();
    changed.formats.brief.min_words = 70;
    let changed = EditorialService::with_policy(owner.clone(), Arc::new(changed));
    let candidate = changed
        .source_revision(
            &editor,
            "short",
            head.lock_version,
            draft(79, "fr", None).evidence.unwrap(),
            "Sourcing edit retains the old length policy snapshot",
        )
        .await
        .unwrap();
    let candidate = approve(&changed, &publisher, "short", candidate).await;
    assert!(matches!(
        changed
            .publish(
                &publisher,
                "short",
                candidate.lock_version,
                "Stale policy must not transfer"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        world_news::repository::Repository::new(reader.clone())
            .story("short")
            .await
            .unwrap()
            .id,
        published
    );
    let candidate = changed
        .length_revision(
            &editor,
            "short",
            candidate.lock_version,
            None,
            "Reassess against the changed configured range",
        )
        .await
        .unwrap();
    let candidate = approve(&changed, &publisher, "short", candidate).await;
    changed
        .publish(
            &publisher,
            "short",
            candidate.lock_version,
            "Reviewed against a configured range",
        )
        .await
        .unwrap();
    let head = service
        .create(&editor, "unsupported", draft(80, "zh", None))
        .await
        .unwrap();
    let head = approve(&service, &publisher, "unsupported", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "unsupported",
                head.lock_version,
                "Unsupported counting needs explicit review"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let head = service
        .length_revision(
            &editor,
            "unsupported",
            head.lock_version,
            Some("Human-reviewed content in a language without a validated counting rule.".into()),
            "Document the counting limitation",
        )
        .await
        .unwrap();
    let head = approve(&service, &publisher, "unsupported", head).await;
    service
        .publish(
            &publisher,
            "unsupported",
            head.lock_version,
            "Explicit approximate counting limitation",
        )
        .await
        .unwrap();
    let html = text(request(&app, "GET", "/story/unsupported", None, false, &[]).await).await;
    assert!(html.contains("Temps de lecture non estimé"));
    assert!(!html.contains("environ 1 min"));
    let mut empty = draft(1, "fr", Some("Override cannot create article content"));
    empty.body = vec![ContentBlock::Paragraph {
        text: "!!! 👋".into(),
    }];
    let head = service.create(&editor, "punctuation", empty).await.unwrap();
    let head = approve(&service, &publisher, "punctuation", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "punctuation",
                head.lock_version,
                "No article words"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    // Fail the head update after inserting the candidate and its exact citation copies.
    let before = service.head(&editor, "short").await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM news.story_revisions")
        .fetch_one(&owner)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_length() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.current_revision_id IS DISTINCT FROM OLD.current_revision_id THEN RAISE EXCEPTION 'synthetic length head failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_fail_length BEFORE UPDATE ON news.stories FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_length();").execute(&owner).await.unwrap();
    assert!(
        service
            .length_revision(
                &editor,
                "short",
                before.lock_version,
                Some("Must roll back".into()),
                "Atomic length update"
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.story_revisions")
            .fetch_one(&owner)
            .await
            .unwrap(),
        rows
    );
    assert_eq!(
        service.head(&editor, "short").await.unwrap().lock_version,
        before.lock_version
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail_length ON news.stories; DROP FUNCTION news.fixture_fail_length();").execute(&owner).await.unwrap();
    service
        .create(&editor, "browser-length", draft(79, "fr", None))
        .await
        .unwrap();
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='length-revoked'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(
        service
            .length_revision(
                &revoked,
                "short",
                before.lock_version,
                None,
                "Revoked identity cannot override"
            )
            .await
            .is_err()
    );
    owner.close().await;
    reader.close().await;
    writer.close().await;
}
