use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use sqlx::{PgPool, types::Json};
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::PageSize,
    editorial::EditorialService,
    error::AppError,
    origins::{OriginEvidence, OriginInput, OriginState},
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic origin verification passphrase";
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
fn input(state: OriginState, ids: &[&str], note: &str) -> OriginInput {
    OriginInput {
        state,
        origin_ids: ids.iter().map(|id| (*id).into()).collect(),
        public_basis: format!("Synthetic public attribution: {note}"),
        private_note: format!("PRIVATE-EVIDENCE-MARKER {note}"),
    }
}
async fn count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM news.article_origin_assertions")
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn candidate(pool: &PgPool, story: &str, revision: &str) {
    sqlx::query("INSERT INTO news.stories(id) VALUES($1)")
        .bind(story)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES($1,$2,1,'Synthetic source attribution',$3,'fr','brief','reporting','origin-publisher')")
        .bind(revision).bind(story).bind(Json(serde_json::json!([{"type":"claim","id":"fact-1","text":"Synthetic source claim"},{"type":"claim","id":"fact-2","text":"Another synthetic claim"}]))).execute(pool).await.unwrap();
}
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn article_origin_assertions_preserve_unknowns_and_frozen_citation_evidence() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_origins"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            persistence::MIGRATOR.iter().take(14).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &previous).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url) VALUES('outlet-a','Outlet A','https://a.invalid'),('outlet-b','Outlet B','https://b.invalid'); INSERT INTO news.reporting_origins(id,name) VALUES('wire','Synthetic wire service'),('reporter-b','Separate reporting group'); INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES('a','outlet-a','a','https://a.invalid/a','Wire copy A','metadata_only'),('b','outlet-b','b','https://b.invalid/b','Wire copy B','metadata_only'),('c','outlet-a','c','https://a.invalid/c','Unknown actual origin','metadata_only'); INSERT INTO news.source_origin_links(source_id,origin_id) VALUES('outlet-a','wire'); INSERT INTO news.source_article_origins(source_article_id,origin_id) VALUES('c','wire');").execute(&owner).await.unwrap();
    candidate(&owner, "old", "old-r1").await;
    sqlx::raw_sql("INSERT INTO news.citations(revision_id,claim_id,source_article_id) VALUES('old-r1','fact-1','a'); INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('old','old-r1','imported','Synthetic old publication','legacy-import'); UPDATE news.stories SET state='published',current_revision_id='old-r1',published_revision_id='old-r1',lock_version=1 WHERE id='old';").execute(&owner).await.unwrap();
    persistence::migrate(&owner).await.unwrap();
    let old:(Option<String>,String,i64,serde_json::Value)=sqlx::query_as("SELECT anchor_id,origin_state,origin_version,origin_snapshot FROM news.citations WHERE revision_id='old-r1'").fetch_one(&owner).await.unwrap();
    assert_eq!(old, (None, "unknown".into(), 0, serde_json::json!([])));
    for (id, role) in [
        ("origin-publisher", Role::Publisher),
        ("origin-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, id, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("CREATE ROLE fixture_origin_reader LOGIN PASSWORD 'synthetic-origin-reader'; GRANT USAGE ON SCHEMA news TO fixture_origin_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations TO fixture_origin_reader;
    CREATE ROLE fixture_origin_writer LOGIN PASSWORD 'synthetic-origin-writer'; GRANT USAGE ON SCHEMA news TO fixture_origin_writer; GRANT SELECT ON news.publication_events,news.schema_version,news.admin_users,news.admin_sessions,news.sources,news.reporting_origins,news.source_articles,news.article_origin_assertions,news.article_origin_labels,news.stories,news.story_revisions,news.citations TO fixture_origin_writer; GRANT INSERT ON news.admin_sessions,news.admin_audit,news.article_origin_assertions,news.article_origin_labels,news.story_revisions,news.citations,news.editorial_actions TO fixture_origin_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_origin_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_origin_writer; GRANT UPDATE(origin_version,current_origin_assertion_id) ON news.source_articles TO fixture_origin_writer; GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_origin_writer; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.article_origin_assertions_id_seq,news.editorial_actions_id_seq TO fixture_origin_writer; GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_reporting_origins_for_evidence(text[]) TO fixture_origin_writer;
    CREATE ROLE fixture_origin_worker LOGIN PASSWORD 'synthetic-origin-worker'; GRANT USAGE ON SCHEMA news TO fixture_origin_worker; GRANT SELECT ON news.schema_version,news.source_articles TO fixture_origin_worker; GRANT UPDATE(origin_version,current_origin_assertion_id) ON news.source_articles TO fixture_origin_worker;").execute(&owner).await.unwrap();
    url.set_username("fixture_origin_reader").unwrap();
    url.set_password(Some("synthetic-origin-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_origin_writer").unwrap();
    url.set_password(Some("synthetic-origin-writer")).unwrap();
    let writer_url = url.to_string();
    url.set_username("fixture_origin_worker").unwrap();
    url.set_password(Some("synthetic-origin-worker")).unwrap();
    let worker_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&conf);
    let writer = persistence::editor_pool(&conf).unwrap();
    let worker = persistence::pool(&config(&worker_url, None));
    let app = web::router_with_editor(conf, reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth, "origin-publisher").await;
    let editor = login(&app, &auth, "origin-editor").await;
    let evidence = OriginEvidence::new(writer.clone());
    assert!(matches!(
        evidence
            .assert(
                &editor,
                "a",
                0,
                input(OriginState::Documented, &["wire"], "denied")
            )
            .await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        OriginEvidence::new(worker.clone())
            .assert(
                &publisher,
                "a",
                0,
                input(OriginState::Documented, &["wire"], "worker denied")
            )
            .await,
        Err(AppError::Forbidden)
    ));
    let a = evidence
        .assert(
            &publisher,
            "a",
            0,
            input(
                OriginState::Documented,
                &["wire"],
                "byline and dispatch evidence A",
            ),
        )
        .await
        .unwrap();
    let b = evidence
        .assert(
            &publisher,
            "b",
            0,
            input(OriginState::Documented, &["wire"], "same dispatch B"),
        )
        .await
        .unwrap();
    assert_eq!(a.actor_id, "origin-publisher");
    assert_eq!(b.version, 1);
    assert_eq!(a.origins.0[0].id, "wire");
    assert!(
        evidence
            .details(&editor, a.id)
            .await
            .unwrap()
            .private_note
            .contains("PRIVATE-EVIDENCE-MARKER")
    );
    assert!(matches!(
        evidence
            .assert(
                &publisher,
                "a",
                0,
                input(OriginState::Unknown, &[], "stale change")
            )
            .await,
        Err(AppError::Conflict)
    ));
    candidate(&owner, "new", "new-r1").await;
    sqlx::raw_sql("INSERT INTO news.citations(revision_id,claim_id,source_article_id,anchor_id,location) VALUES('new-r1','claim:fact-1','a','fact-1','paragraph 3'),('new-r1','claim:fact-1','b','fact-1','same dispatch'),('new-r1','legacy-reference','c',NULL,'general background'); INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('new','new-r1','imported','Synthetic attribution fixture','legacy-import'); UPDATE news.stories SET state='published',current_revision_id='new-r1',published_revision_id='new-r1',lock_version=1 WHERE id='new';").execute(&owner).await.unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(DISTINCT source_id) FROM news.public_citations WHERE revision_id='new-r1' AND anchor_id='fact-1'").fetch_one(&reader).await.unwrap(),2);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(DISTINCT item->>'id') FROM news.public_citations c CROSS JOIN LATERAL jsonb_array_elements(c.origin_snapshot) item WHERE c.revision_id='new-r1' AND c.origin_state='documented'").fetch_one(&reader).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT origin_state FROM news.public_citations WHERE revision_id='new-r1' AND source_article_id='c'").fetch_one(&reader).await.unwrap(),"unknown");
    let json: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(c)) FROM news.public_citations c WHERE revision_id='new-r1'",
    )
    .fetch_one(&reader)
    .await
    .unwrap();
    assert!(!json.to_string().contains("PRIVATE-EVIDENCE-MARKER"));
    assert!(!json.to_string().contains("origin-publisher"));
    let before = count(&owner).await;
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_origin_pointer() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.origin_version IS DISTINCT FROM OLD.origin_version THEN RAISE EXCEPTION 'synthetic pointer failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_fail_origin_pointer BEFORE UPDATE ON news.source_articles FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_origin_pointer();").execute(&owner).await.unwrap();
    assert!(
        evidence
            .assert(
                &publisher,
                "b",
                1,
                input(OriginState::Unknown, &[], "rollback fixture")
            )
            .await
            .is_err()
    );
    assert_eq!(count(&owner).await, before);
    sqlx::raw_sql("DROP TRIGGER fixture_fail_origin_pointer ON news.source_articles; DROP FUNCTION news.fixture_fail_origin_pointer();").execute(&owner).await.unwrap();
    let changed = evidence
        .assert(
            &publisher,
            "a",
            1,
            input(
                OriginState::Unknown,
                &[],
                "dispatch attribution now uncertain",
            ),
        )
        .await
        .unwrap();
    assert_eq!(changed.version, 2);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT origin_state FROM news.public_citations WHERE revision_id='new-r1' AND source_article_id='a'").fetch_one(&reader).await.unwrap(),"documented");
    assert!(!sqlx::query_scalar::<_,bool>("SELECT origins_current FROM news.public_citations WHERE revision_id='new-r1' AND source_article_id='a'").fetch_one(&reader).await.unwrap());
    let one = input(
        OriginState::Documented,
        &["wire"],
        "concurrent provenance one",
    );
    let two = input(
        OriginState::Documented,
        &["reporter-b"],
        "concurrent provenance two",
    );
    let (one, two) = tokio::join!(
        evidence.assert(&publisher, "c", 0, one),
        evidence.assert(&publisher, "c", 0, two)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let classification = EditorialService::new(writer.clone())
        .reclassify(
            &publisher,
            "new",
            1,
            world_news::taxonomy::Classification {
                topics: vec!["science".into()],
                geography: vec!["world".into()],
            },
            "Classification cannot rewrite captured citation origins",
            &world_news::taxonomy::Taxonomy::repository(),
        )
        .await
        .unwrap();
    let captured:Vec<(String,i64,Option<String>)>=sqlx::query_as("SELECT source_article_id,origin_version,anchor_id FROM news.citations WHERE revision_id=$1 ORDER BY source_article_id").bind(&classification.current_revision_id).fetch_all(&owner).await.unwrap();
    assert_eq!(
        captured,
        vec![
            ("a".into(), 1, Some("fact-1".into())),
            ("b".into(), 1, Some("fact-1".into())),
            ("c".into(), 0, None)
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT intake_version FROM news.source_articles WHERE id='a'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        0
    );
    for query in [
        "UPDATE news.citations SET origin_state='unknown'",
        "DELETE FROM news.citations",
        "UPDATE news.article_origin_assertions SET public_basis='rewritten'",
        "DELETE FROM news.article_origin_labels",
    ] {
        assert!(sqlx::query(query).execute(&owner).await.is_err());
    }
    assert!(sqlx::query("INSERT INTO news.article_origin_labels(assertion_id,origin_id,captured_name) VALUES($1,'reporter-b','Late invented origin')").bind(a.id).execute(&owner).await.is_err());
    assert!(sqlx::query("UPDATE news.source_articles SET origin_version=0,current_origin_assertion_id=NULL WHERE id='a'").execute(&worker).await.is_err());
    assert!(
        sqlx::query("SELECT * FROM news.admin_users")
            .execute(&worker)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("SELECT * FROM news.article_origin_assertions")
            .execute(&reader)
            .await
            .is_err()
    );
    candidate(&owner, "invalid", "invalid-r1").await;
    assert!(sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,anchor_id) VALUES('invalid-r1','bad-anchor','a','not-in-body')").execute(&owner).await.is_err());
    sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,anchor_id,origin_version,origin_state,origin_snapshot) VALUES('invalid-r1','first','a','fact-1',1,'conflicting','[{\"id\":\"invented\"}]')").execute(&owner).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT origin_state FROM news.citations WHERE revision_id='invalid-r1'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        "documented"
    );
    assert!(sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,anchor_id,origin_version) VALUES('invalid-r1','second','a','fact-2',2)").execute(&owner).await.is_err());
    sqlx::query("UPDATE news.reporting_origins SET enabled=false WHERE id='wire'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(
        evidence
            .assert(
                &publisher,
                "b",
                1,
                input(OriginState::Documented, &["wire"], "disabled origin")
            )
            .await
            .is_err()
    );
    sqlx::query("UPDATE news.sources SET name='Renamed outlet' WHERE id='outlet-a'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT source_name FROM news.public_citations WHERE revision_id='new-r1' AND source_article_id='a'").fetch_one(&reader).await.unwrap(),"Outlet A");
    assert_eq!(
        evidence
            .history(&editor, "a", None, PageSize::new(1).unwrap())
            .await
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='origin-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        evidence
            .assert(
                &publisher,
                "b",
                1,
                input(OriginState::Unknown, &[], "revoked session")
            )
            .await,
        Err(AppError::Forbidden)
    ));
    worker.close().await;
    writer.close().await;
    reader.close().await;
    owner.close().await;
}
