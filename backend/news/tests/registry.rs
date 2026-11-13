use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use sqlx::{PgPool, migrate::Migrator};
use std::{borrow::Cow, sync::Arc};
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::PageSize,
    error::AppError,
    persistence::{self, MIGRATOR},
    registry::*,
    repository::Repository,
    web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic registry passphrase only";
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
async fn login(app: &Router, service: &AuthService, user: &str) -> Session {
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
    service.authenticate(&headers).await.unwrap()
}
fn spec(name: &str) -> SourceSpec {
    SourceSpec {
        name: name.into(),
        homepage_url: "https://source.invalid".into(),
        enabled: false,
        ownership_note: Some("Independent ownership declaration, not a quality score".into()),
        usage_policy: StoragePolicy::MetadataOnly,
        terms_url: None,
        languages: vec!["fr".into(), "nl".into()],
        geographic_coverage: vec!["Belgium".into()],
        attribution: Some("Preserve outlet credit".into()),
        editorial_note: Some("Internal review note, not an objective trust score".into()),
        rights_status: RightsStatus::Unknown,
        display_policy: DisplayPolicy::LinkOnly,
        rights_note: None,
        origin_ids: vec![],
    }
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own fresh disposable DB; use scripts/test_database.py"]
async fn registry_policy_and_retirement_preserve_historical_provenance() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_registry"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url, None));
    assert!(!persistence::schema_available(&owner).await);
    let before_registry = Migrator {
        migrations: Cow::Owned(MIGRATOR.iter().take(5).cloned().collect()),
        ..Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &before_registry)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,usage_policy) VALUES ('legacy-outlet','Captured old outlet','https://legacy.invalid','licensed_text');
        INSERT INTO news.feeds(id,source_id,url) VALUES ('legacy-feed','legacy-outlet','https://legacy.invalid/rss');
        INSERT INTO news.source_articles(id,source_id,feed_id,canonical_key,original_url,title) VALUES ('legacy-reference','legacy-outlet','legacy-feed','legacy-reference','https://legacy.invalid/article','Legacy reference');
        INSERT INTO news.stories(id) VALUES ('legacy-story');
        INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES ('legacy-revision','legacy-story',1,'Historical synthetic story','[]','fr','brief','legacy_summary','legacy-fixture');
        INSERT INTO news.citations(revision_id,claim_id,source_article_id) VALUES ('legacy-revision','reference','legacy-reference');
        INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('legacy-story','legacy-revision','imported','Synthetic history, not a verified approval','legacy-fixture');
        UPDATE news.stories SET state='published',current_revision_id='legacy-revision',published_revision_id='legacy-revision' WHERE id='legacy-story';")
        .execute(&owner).await.unwrap();
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
    sqlx::raw_sql("CREATE ROLE fixture_registry_reader LOGIN PASSWORD 'synthetic-registry-reader'; GRANT USAGE ON SCHEMA news TO fixture_registry_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_registry_reader;
        CREATE ROLE fixture_registry_writer LOGIN PASSWORD 'synthetic-registry-writer'; GRANT USAGE ON SCHEMA news TO fixture_registry_writer;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.sources,news.feeds,news.reporting_origins,news.source_origin_links TO fixture_registry_writer;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_registry_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_registry_writer;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.sources,news.feeds,news.reporting_origins,news.source_origin_links,news.registry_actions TO fixture_registry_writer;
        GRANT UPDATE(name,homepage_url,enabled,ownership_note,usage_policy,terms_url,languages,geographic_coverage,attribution,editorial_note,rights_status,display_policy,rights_note,lock_version) ON news.sources TO fixture_registry_writer;
        GRANT UPDATE(enabled,poll_seconds,max_response_bytes,lock_version) ON news.feeds TO fixture_registry_writer;
        GRANT UPDATE(name,source_id,note,enabled,lock_version) ON news.reporting_origins TO fixture_registry_writer;
        GRANT DELETE ON news.source_origin_links TO fixture_registry_writer;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.registry_actions_id_seq TO fixture_registry_writer;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_registry_reader").unwrap();
    url.set_password(Some("synthetic-registry-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_registry_writer").unwrap();
    url.set_password(Some("synthetic-registry-writer")).unwrap();
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
    let registry = Registry::new(writer.clone());
    let public = Repository::new(reader.clone());
    assert!(
        sqlx::query("SELECT * FROM news.registry_actions")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM news.sources")
            .execute(&writer)
            .await
            .is_err()
    );
    let mut old = registry.source(&editor, "legacy-outlet").await.unwrap();
    assert_eq!(old.spec.usage_policy, StoragePolicy::LicensedText);
    assert_eq!(old.spec.rights_status, RightsStatus::Unknown);
    assert_eq!(old.spec.effective_storage(), StoragePolicy::MetadataOnly);
    assert_eq!(old.spec.effective_display(), DisplayPolicy::LinkOnly);
    assert_eq!(
        public.sources("legacy-revision").await.unwrap()[0].source_name,
        "Captured old outlet"
    );
    old.spec.name = "Renamed current outlet".into();
    old.spec.usage_policy = StoragePolicy::MetadataOnly;
    old = registry
        .save_source(&publisher, &old.id, Some(old.version), old.spec)
        .await
        .unwrap();
    assert_eq!(old.spec.name, "Renamed current outlet");
    assert_eq!(
        public.sources("legacy-revision").await.unwrap()[0].source_name,
        "Captured old outlet"
    );
    sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title) VALUES ('new-reference','legacy-outlet','new-reference','https://legacy.invalid/new','New reference')").execute(&owner).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT attribution_name FROM news.source_articles WHERE id='new-reference'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        "Renamed current outlet"
    );
    assert!(sqlx::query("UPDATE news.source_articles SET attribution_name='Rewrite old credit' WHERE id='legacy-reference'").execute(&owner).await.is_err());
    assert!(matches!(
        registry
            .save_source(
                &editor,
                "denied",
                None,
                spec("Editor cannot configure sources")
            )
            .await,
        Err(AppError::Forbidden)
    ));
    let mut source = registry
        .save_source(&publisher, "outlet-a", None, spec("New outlet"))
        .await
        .unwrap();
    assert!(!source.spec.enabled);
    assert_eq!(source.spec.languages, ["fr", "nl"]);
    assert_eq!(source.spec.geographic_coverage, ["Belgium"]);
    assert!(matches!(
        registry
            .save_source(&publisher, "outlet-a", None, spec("No overwrite"))
            .await,
        Err(AppError::Conflict)
    ));
    let original_version = source.version;
    source.spec.enabled = true;
    source = registry
        .save_source(&publisher, &source.id, Some(source.version), source.spec)
        .await
        .unwrap();
    assert!(matches!(
        registry
            .save_source(
                &publisher,
                &source.id,
                Some(original_version),
                source.spec.clone()
            )
            .await,
        Err(AppError::Conflict)
    ));
    let origin = registry
        .save_origin(
            &publisher,
            "wire-a",
            None,
            OriginSpec {
                name: "Synthetic dispatch origin".into(),
                source_id: Some(source.id.clone()),
                note: Some(
                    "Provider declaration; not per-article independence verification".into(),
                ),
                enabled: true,
            },
        )
        .await
        .unwrap();
    source.spec.origin_ids = vec![origin.id.clone()];
    source = registry
        .save_source(&publisher, &source.id, Some(source.version), source.spec)
        .await
        .unwrap();
    assert_eq!(source.spec.origin_ids, ["wire-a"]);
    assert_eq!(
        count(&owner, "source_article_origins").await,
        0,
        "configuration must not fabricate article reporting origins"
    );
    let mut changed = origin.spec.clone();
    changed.name = "Different identity".into();
    assert!(matches!(
        registry
            .save_origin(&publisher, &origin.id, Some(origin.version), changed)
            .await,
        Err(AppError::Conflict)
    ));
    let mut bad = source.spec.clone();
    bad.display_policy = DisplayPolicy::FullText;
    assert!(matches!(
        registry
            .save_source(&publisher, &source.id, Some(source.version), bad)
            .await,
        Err(AppError::Validation(_))
    ));
    let mut licensed = source.spec.clone();
    licensed.rights_status = RightsStatus::Documented;
    licensed.usage_policy = StoragePolicy::LicensedText;
    licensed.display_policy = DisplayPolicy::LinkOnly;
    assert!(matches!(
        registry
            .save_source(
                &publisher,
                &source.id,
                Some(source.version),
                licensed.clone()
            )
            .await,
        Err(AppError::Validation(_))
    ));
    licensed.terms_url = Some("https://source.invalid/terms#reuse".into());
    licensed.rights_note = Some("Synthetic internal-storage-only agreement declaration".into());
    source = registry
        .save_source(&publisher, &source.id, Some(source.version), licensed)
        .await
        .unwrap();
    assert_eq!(source.spec.effective_storage(), StoragePolicy::LicensedText);
    assert_eq!(source.spec.effective_display(), DisplayPolicy::LinkOnly);
    let mut secret = spec("Do not retain credentials");
    secret.homepage_url = "https://source.invalid/?access_token=synthetic-private".into();
    let before = count(&owner, "sources").await;
    assert!(matches!(
        registry
            .save_source(&publisher, "secret-url", None, secret)
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(count(&owner, "sources").await, before);
    let mut feed = registry
        .save_feed(
            &publisher,
            "feed-a",
            None,
            FeedSpec {
                source_id: source.id.clone(),
                url: "https://source.invalid/feed?lang=fr".into(),
                enabled: false,
                poll_seconds: 3600,
                max_response_bytes: 2097152,
            },
        )
        .await
        .unwrap();
    assert!(
        registry
            .feeds(&editor, None, PageSize::new(20).unwrap(), true)
            .await
            .unwrap()
            .iter()
            .all(|f| f.id != "feed-a")
    );
    feed.spec.enabled = true;
    feed = registry
        .save_feed(&publisher, &feed.id, Some(feed.version), feed.spec)
        .await
        .unwrap();
    assert!(
        registry
            .feeds(&editor, None, PageSize::new(20).unwrap(), true)
            .await
            .unwrap()
            .iter()
            .any(|f| f.id == "feed-a")
    );
    let mut redirected = feed.spec.clone();
    redirected.url = "https://other.invalid/feed".into();
    assert!(matches!(
        registry
            .save_feed(&publisher, &feed.id, Some(feed.version), redirected)
            .await,
        Err(AppError::Conflict)
    ));
    let disabled = registry
        .disable(&publisher, Entity::Source, &source.id, source.version)
        .await
        .unwrap();
    assert!(
        registry
            .feeds(&editor, None, PageSize::new(20).unwrap(), true)
            .await
            .unwrap()
            .iter()
            .all(|f| f.id != "feed-a")
    );
    assert!(
        registry.feed(&editor, "feed-a").await.unwrap().spec.enabled,
        "source disable does not rewrite feed's configured intent"
    );
    assert!(matches!(
        registry
            .disable(&publisher, Entity::Source, &source.id, source.version)
            .await,
        Err(AppError::Conflict)
    ));
    assert_eq!(
        registry.source(&editor, &source.id).await.unwrap().version,
        disabled
    );
    let old_feed = registry.feed(&editor, "legacy-feed").await.unwrap();
    registry
        .disable(&publisher, Entity::Feed, &old_feed.id, old_feed.version)
        .await
        .unwrap();
    assert_eq!(
        public.sources("legacy-revision").await.unwrap()[0].original_url,
        "https://legacy.invalid/article"
    );
    assert_eq!(
        public.sources("legacy-revision").await.unwrap()[0].source_name,
        "Captured old outlet"
    );
    assert_eq!(count(&owner, "source_articles").await, 2);
    let before = (
        count(&owner, "sources").await,
        count(&owner, "registry_actions").await,
    );
    let mut missing = spec("Rollback missing relation");
    missing.origin_ids = vec!["missing-origin".into()];
    assert!(
        registry
            .save_source(&publisher, "rollback-source", None, missing)
            .await
            .is_err()
    );
    assert_eq!(
        (
            count(&owner, "sources").await,
            count(&owner, "registry_actions").await
        ),
        before
    );
    let before = (
        registry.source(&editor, "outlet-a").await.unwrap(),
        count(&owner, "registry_actions").await,
    );
    sqlx::raw_sql("CREATE FUNCTION news.test_reject_registry() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.entity_id='outlet-a' THEN RAISE EXCEPTION 'synthetic registry interruption' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER test_reject_registry BEFORE INSERT ON news.registry_actions FOR EACH ROW EXECUTE FUNCTION news.test_reject_registry();").execute(&owner).await.unwrap();
    let mut updated = before.0.spec.clone();
    updated.editorial_note = Some("Must roll back".into());
    assert!(
        registry
            .save_source(&publisher, "outlet-a", Some(before.0.version), updated)
            .await
            .is_err()
    );
    assert_eq!(
        registry.source(&editor, "outlet-a").await.unwrap().version,
        before.0.version
    );
    assert_eq!(
        registry
            .source(&editor, "outlet-a")
            .await
            .unwrap()
            .spec
            .editorial_note,
        before.0.spec.editorial_note
    );
    assert_eq!(count(&owner, "registry_actions").await, before.1);
    sqlx::raw_sql("DROP TRIGGER test_reject_registry ON news.registry_actions;DROP FUNCTION news.test_reject_registry();").execute(&owner).await.unwrap();
    assert!(
        sqlx::query("UPDATE news.registry_actions SET entity_id='rewrite'")
            .execute(&owner)
            .await
            .is_err()
    );
    let first = registry
        .sources(&editor, None, PageSize::new(1).unwrap())
        .await
        .unwrap();
    let next = registry
        .sources(&editor, Some(&first[0].id), PageSize::new(1).unwrap())
        .await
        .unwrap();
    assert_ne!(first[0].id, next[0].id);
    assert_eq!(
        registry
            .origins(&editor, None, PageSize::new(20).unwrap())
            .await
            .unwrap()
            .len(),
        1
    );
    auth::disable(&owner, "test-publisher").await.unwrap();
    assert!(matches!(
        registry
            .disable(&publisher, Entity::Origin, "wire-a", origin.version)
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(count(&owner, "registry_actions").await > 0);
    reader.close().await;
    writer.close().await;
    owner.close().await;
}
