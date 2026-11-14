use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use html_scraper::{Html, Selector};
use http_body_util::BodyExt;
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
const PASSWORD: &str = "synthetic structured content passphrase";
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
fn input(format: ArticleFormat) -> RevisionInput {
    RevisionInput {
        evidence: Some(world_news::sourcing::SourcePackage {
            statement: Some(world_news::sourcing::SourceStatement {
                mode: world_news::sourcing::SourceMode::Original,
                attribution: "Explicit original synthetic fixture, no external reporting".into(),
                limits: None,
            }),
            firsthand: vec![world_news::sourcing::FirsthandEvidence {
                method: world_news::sourcing::FirsthandMethod::Observation,
                claim_ids: vec!["fact-1".into(), "limit-1".into()],
                attribution: "Synthetic test observation".into(),
                private_note: "Synthetic fixture evidence only".into(),
            }],
            ..Default::default()
        }),
        title: "Synthetic <script>headline</script>".into(),
        standfirst: Some("Repeated authored prose marker.".into()),
        summary: Some("Distinct explicit overview marker.".into()),
        body: vec![
            ContentBlock::Paragraph {
                text: "Repeated authored prose marker.".into(),
            },
            ContentBlock::Heading {
                level: 2,
                text: "Authored subheading".into(),
            },
            ContentBlock::Claim {
                id: "fact-1".into(),
                text: "Literal <img src=x onerror=alert(1)> remains prose.".into(),
            },
            ContentBlock::Section {
                role: SectionRole::Context,
                blocks: vec![SectionBlock::Paragraph {
                    text: "Explicit context marker.".into(),
                }],
            },
            ContentBlock::Section {
                role: SectionRole::Uncertainty,
                blocks: vec![SectionBlock::Claim {
                    id: "limit-1".into(),
                    text: "Explicit uncertainty marker.".into(),
                }],
            },
        ],
        language: "nl".into(),
        format,
        kind: ArticleKind::Reporting,
        topics: vec!["science".into()],
        geography: vec!["world".into()],
        reason: "Original synthetic structured content".into(),
    }
}
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn structured_formats_round_trip_and_render_as_inert_prose() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(
        url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_structured")
    );
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    auth::bootstrap(
        &owner,
        "structured-publisher",
        Role::Publisher,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE ROLE fixture_structure_reader LOGIN PASSWORD 'synthetic-structure-reader'; GRANT USAGE ON SCHEMA news TO fixture_structure_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations TO fixture_structure_reader;
    CREATE ROLE fixture_structure_writer LOGIN PASSWORD 'synthetic-structure-writer'; GRANT USAGE ON SCHEMA news TO fixture_structure_writer; GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.story_revisions,news.editorial_actions,news.editorial_reviews,news.publication_events TO fixture_structure_writer; GRANT INSERT ON news.admin_sessions,news.admin_audit,news.stories,news.story_revisions,news.editorial_actions,news.editorial_reviews,news.publication_events TO fixture_structure_writer; GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_structure_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_structure_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_structure_writer; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.editorial_actions_id_seq,news.editorial_reviews_id_seq,news.publication_events_id_seq TO fixture_structure_writer;").execute(&owner).await.unwrap();
    sqlx::raw_sql("GRANT SELECT ON news.citations,news.source_articles,news.article_origin_labels,news.reporting_origins TO fixture_structure_writer; GRANT EXECUTE ON FUNCTION news.lock_citation_evidence(text) TO fixture_structure_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_structure_reader").unwrap();
    url.set_password(Some("synthetic-structure-reader"))
        .unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_structure_writer").unwrap();
    url.set_password(Some("synthetic-structure-writer"))
        .unwrap();
    let writer_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&conf);
    let writer = persistence::editor_pool(&conf).unwrap();
    let app = web::router_with_editor(conf, reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth, "structured-publisher").await;
    let service = EditorialService::new(writer.clone());
    let repository = Repository::new(reader.clone());
    for (id, format) in [
        ("rich-brief", ArticleFormat::Brief),
        ("rich-standard", ArticleFormat::Standard),
        ("rich-analysis", ArticleFormat::Analysis),
    ] {
        let mut value = input(format);
        if format == ArticleFormat::Analysis {
            value.kind = ArticleKind::Analysis;
        }
        if format == ArticleFormat::Standard {
            value.standfirst = Some("Distinct standard introduction.".into());
            value.summary = value.standfirst.clone();
        }
        let expected_body = serde_json::to_value(&value.body).unwrap();
        let expected_standfirst = value.standfirst.clone();
        let expected_summary = value.summary.clone();
        let mut head = service.create(&publisher, id, value).await.unwrap();
        head = service
            .submit(&publisher, id, head.lock_version)
            .await
            .unwrap();
        head = service
            .review(
                &publisher,
                id,
                head.lock_version,
                ReviewOutcome::Approved,
                None,
            )
            .await
            .unwrap();
        head = service
            .publish(
                &publisher,
                id,
                head.lock_version,
                "Synthetic article publication",
            )
            .await
            .unwrap();
        let saved = repository.story(id).await.unwrap();
        assert_eq!(serde_json::to_value(&saved.body).unwrap(), expected_body);
        assert_eq!(saved.standfirst, expected_standfirst);
        assert_eq!(saved.summary, expected_summary);
        assert_eq!(saved.language, "nl");
        assert_eq!(saved.format, format);
        let provenance: serde_json::Value =
            sqlx::query_scalar("SELECT provenance FROM news.story_revisions WHERE id=$1")
                .bind(&head.current_revision_id)
                .fetch_one(&owner)
                .await
                .unwrap();
        assert_eq!(provenance["body_schema"], 2);
        assert_eq!(provenance["origin"], "manual_submission");
        assert!(saved.overview().is_some());
        let (status, html) = public(&app, &format!("/story/{id}")).await;
        assert_eq!(status, StatusCode::OK);
        let document = Html::parse_document(&html);
        assert_eq!(
            document
                .select(&Selector::parse("script,img,style").unwrap())
                .count(),
            0
        );
        assert_eq!(
            document
                .select(&Selector::parse("article[lang=nl]").unwrap())
                .count(),
            1
        );
        assert_eq!(
            document
                .select(&Selector::parse("p#claim-fact-1").unwrap())
                .count(),
            1
        );
        assert_eq!(
            document
                .select(&Selector::parse("p#claim-limit-1").unwrap())
                .count(),
            1
        );
        assert_eq!(
            document
                .select(&Selector::parse("h2#section-context[lang=fr]").unwrap())
                .count(),
            1
        );
        assert_eq!(
            document
                .select(&Selector::parse("h2#section-uncertainty").unwrap())
                .count(),
            1
        );
        assert_eq!(html.matches("Repeated authored prose marker.").count(), 1);
        assert_eq!(
            document
                .select(&Selector::parse("details").unwrap())
                .count(),
            usize::from(format != ArticleFormat::Standard)
        );
        if format == ArticleFormat::Standard {
            assert_eq!(html.matches("Distinct standard introduction.").count(), 1);
        }
    }
    let mut heading = input(ArticleFormat::Brief);
    heading.evidence = None;
    heading.body = vec![ContentBlock::Heading {
        level: 2,
        text: "Only a heading".into(),
    }];
    let head = service
        .create(&publisher, "heading-only", heading)
        .await
        .unwrap();
    assert!(matches!(
        service
            .submit(&publisher, "heading-only", head.lock_version)
            .await,
        Err(AppError::Validation(_))
    ));
    let mut duplicate = input(ArticleFormat::Brief);
    duplicate.body.push(ContentBlock::Claim {
        id: "fact-1".into(),
        text: "Duplicate anchor".into(),
    });
    assert!(matches!(
        service
            .create(&publisher, "duplicate-anchor", duplicate)
            .await,
        Err(AppError::Validation(_))
    ));
    let raw = serde_json::json!({"stories":[{"id":"legacy-literal","title":"Historical text","body":"# This remains literal prose\n<script>historical text</script>","lang":"en","edition_date":"2026-11-01","updated_at":"2026-11-01T12:00:00Z","synthesized":false}]});
    let plan =
        world_news::legacy::plan_bytes(&serde_json::to_vec(&raw).unwrap(), "Europe/Brussels")
            .unwrap();
    assert_eq!(plan.report.validated_rows, 1);
    world_news::legacy::apply(&owner, &plan).await.unwrap();
    let legacy = repository.story("legacy-literal").await.unwrap();
    assert!(legacy.standfirst.is_none() && legacy.summary.is_none() && legacy.overview().is_none());
    assert!(
        legacy
            .body
            .iter()
            .all(|b| matches!(b, ContentBlock::Paragraph { .. }))
    );
    let (status, html) = public(&app, "/story/legacy-literal").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("historical text"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("section-context"));
    writer.close().await;
    reader.close().await;
    owner.close().await;
}
