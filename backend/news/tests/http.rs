use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    config::Config,
    content::{ArticleFormat, ArticleKind, DraftInput},
    legacy, persistence,
    repository::Repository,
    web,
};

async fn get(app: &Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("host", "untrusted.invalid")
                .header("forwarded", "host=untrusted.invalid;proto=http")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-origin")
    );
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
fn config(url: &str) -> Config {
    Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(url.into()),
        "PUBLIC_URL" => Some("https://news.invalid".into()),
        _ => None,
    })
    .unwrap()
}
#[tokio::test]
#[ignore = "requires its own fresh disposable DB; use scripts/test_database.py"]
async fn published_route_contract_uses_only_restricted_views() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_http"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner_config = config(&database_url);
    let owner = persistence::pool(&owner_config);
    assert!(!persistence::schema_available(&owner).await);
    persistence::migrate(&owner).await.unwrap();
    sqlx::raw_sql("CREATE ROLE fixture_http_reader LOGIN PASSWORD 'synthetic-http-reader';
        GRANT USAGE ON SCHEMA news TO fixture_http_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,
            news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_http_reader;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_http_reader").unwrap();
    url.set_password(Some("synthetic-http-reader")).unwrap();
    let reader_config = Arc::new(config(url.as_str()));
    let reader = persistence::pool(&reader_config);
    assert!(
        sqlx::query("SELECT * FROM news.story_revisions")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    let app = web::router(reader_config, reader.clone());
    let (status, body) = get(&app, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Aucune édition publiée"));
    let (status, body) = get(&app, "/archive").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Aucune édition sur cette page"));
    assert_eq!(
        get(&app, "/edition/2026-01-10").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(get(&app, "/story/missing").await.0, StatusCode::NOT_FOUND);

    let mut fixture: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../../fixtures/legacy-stories.json")).unwrap();
    fixture["stories"][0]["title"] = serde_json::json!("<script>synthetic-title()</script>");
    fixture["stories"][0]["body"] =
        serde_json::json!("<img src=x onerror=synthetic-body()>\n\nPreserved original prose.");
    let plan =
        legacy::plan_bytes(&serde_json::to_vec(&fixture).unwrap(), "Europe/Brussels").unwrap();
    legacy::apply(&owner, &plan).await.unwrap();
    let mapping = plan
        .report
        .mappings
        .iter()
        .find(|m| m.id == "legacy-a")
        .unwrap();
    let old_revision = &mapping.revision_id;
    Repository::new(owner.clone())
        .create_draft(DraftInput {
            id: "private-draft".into(),
            title: "private-draft-marker".into(),
            body: vec![],
            language: "fr".into(),
            format: ArticleFormat::Brief,
            kind: ArticleKind::Reporting,
            actor_id: "synthetic-editor".into(),
        })
        .await
        .unwrap();
    for uri in [
        "/",
        "/archive",
        "/edition/2026-01-10",
        "/story/legacy-a?paper=cream&masthead=ignored",
    ] {
        let (status, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert!(!body.contains("untrusted.invalid") && !body.contains("private-draft-marker"));
        assert!(body.contains("https://news.invalid/"));
    }
    let (status, body) = get(&app, "/story/legacy-a").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<script>") && !body.contains("<img src=x"));
    assert!(body.contains("&#60;script&#62;") && body.contains("&#60;img"));
    assert!(
        body.contains("Sources et références")
            && body.contains("https://dispatch.invalid/story/legacy-a")
    );
    assert!(body.contains("Archive importée") && body.contains("ne sont pas attestés"));
    assert_eq!(
        get(&app, "/story/private-draft").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, "/story/legacy-b?revision=private-draft").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &format!("/story/legacy-b?revision={old_revision}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, body) = get(&app, "/edition/2026-01-10").await;
    assert!(body.contains(&format!("/story/legacy-a?revision={old_revision}")));
    assert_eq!(
        get(&app, "/edition/2026-02-01").await.0,
        StatusCode::NOT_FOUND
    );

    sqlx::raw_sql("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES ('route-correction','legacy-a',2,'Synthetic corrected title','[]','fr','brief','reporting','fixture-editor');
        INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('legacy-a','route-correction','corrected','Synthetic correction','fixture-editor');
        UPDATE news.stories SET current_revision_id='route-correction',published_revision_id='route-correction' WHERE id='legacy-a';")
        .execute(&owner).await.unwrap();
    assert!(
        get(&app, "/story/legacy-a")
            .await
            .1
            .contains("Synthetic corrected title")
    );
    let (status, body) = get(&app, &format!("/story/legacy-a?revision={old_revision}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("révision conservée dans une édition") && body.contains("&#60;script&#62;")
    );
    assert!(body.contains("Lire la version actuelle"));
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('legacy-a','route-correction','withdrawn','Synthetic withdrawal','fixture-editor');
        UPDATE news.stories SET state='withdrawn' WHERE id='legacy-a';").execute(&owner).await.unwrap();
    assert_eq!(get(&app, "/story/legacy-a").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        get(&app, &format!("/story/legacy-a?revision={old_revision}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, body) = get(&app, "/edition/2026-01-10").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Article retiré") && !body.contains("synthetic-title"));

    reader.close().await;
    assert_eq!(
        get(&app, "/archive").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    owner.close().await;
}
