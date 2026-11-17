use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use cookie::Cookie;
use sqlx::PgPool;
use std::{borrow::Cow, sync::Arc};
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    domain::EditorialPolicy,
    editions::{
        WarningKind,
        drafts::{Drafts, EntryInput},
    },
    error::AppError,
    persistence,
    taxonomy::Taxonomy,
    web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic edition draft passphrase";
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
async fn login(app: &Router, auth: &AuthService) -> Session {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", "edition-editor")
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
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|h| {
            let c = Cookie::parse(h.to_str().unwrap()).unwrap();
            format!("{}={}", c.name(), c.value())
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, cookie.parse().unwrap());
    auth.authenticate(&headers).await.unwrap()
}
async fn seed(pool: &PgPool, id: &str, published: bool) {
    // Owner-only synthetic publications isolate selection from the article workflow tests.
    let revision = format!("revision-{id}");
    sqlx::query("INSERT INTO news.stories(id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id,topics,geography) VALUES ($1,$2,1,$2,$3,'fr','brief','reporting','synthetic-owner',ARRAY['society'],ARRAY['be'])")
        .bind(&revision).bind(id).bind(sqlx::types::Json(serde_json::json!([{"type":"paragraph","text":std::iter::repeat_n("mot",80).collect::<Vec<_>>().join(" ")}]))).execute(pool).await.unwrap();
    if published {
        sqlx::query("INSERT INTO news.publication_events(story_id,revision_id,actor_id,action,public_reason) VALUES ($1,$2,'synthetic-owner','published','PUBLIC-SYNTHETIC-PUBLICATION')").bind(id).bind(&revision).execute(pool).await.unwrap();
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,published_revision_id=$2,state='published' WHERE id=$1").bind(id).bind(&revision).execute(pool).await.unwrap();
    } else {
        sqlx::query("UPDATE news.stories SET current_revision_id=$2 WHERE id=$1")
            .bind(id)
            .bind(&revision)
            .execute(pool)
            .await
            .unwrap();
    }
}
fn entry(id: &str, version: i64) -> EntryInput {
    EntryInput {
        story_id: id.into(),
        revision_id: format!("revision-{id}"),
        section: "belgium".into(),
        expected_story_version: version,
    }
}
#[tokio::test]
#[ignore = "requires a disposable database; use scripts/test_database.py"]
async fn edition_drafts_fence_selection_preserve_history_and_never_publish() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_editions"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    let previous = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            persistence::MIGRATOR
                .iter()
                .filter(|m| m.version <= 18)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &previous).await.unwrap();
    seed(&owner, "alpha", true).await;
    seed(&owner, "beta", true).await;
    seed(&owner, "private", false).await;
    sqlx::raw_sql("INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES ('historical-edition','2026-01-01','Europe/Brussels');INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES ('historical-edition','alpha','revision-alpha','belgium',0,true);UPDATE news.editions SET state='published',published_at=now() WHERE id='historical-edition';CREATE ROLE fixture_edition_reader LOGIN PASSWORD 'synthetic-edition-reader';GRANT USAGE ON SCHEMA news TO fixture_edition_reader;GRANT SELECT ON news.schema_version,news.public_editions,news.public_edition_entries,news.public_stories,news.public_revisions,news.public_notices,news.public_citations,news.public_translations TO fixture_edition_reader;").execute(&owner).await.unwrap();
    persistence::migrate(&owner).await.unwrap();
    assert!(persistence::schema_available(&owner).await);
    let preserved:(i64,Option<i64>)=sqlx::query_as("SELECT e.lock_version,m.prepared_story_version FROM news.editions e JOIN news.edition_entries m ON m.edition_id=e.id WHERE e.id='historical-edition'").fetch_one(&owner).await.unwrap();
    assert_eq!(preserved, (0, None)); // No invented import author or prepared story fence.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.edition_actions")
            .fetch_one(&owner)
            .await
            .unwrap(),
        0
    );
    auth::bootstrap(
        &owner,
        "edition-editor",
        Role::Editor,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    sqlx::raw_sql("CREATE ROLE fixture_edition_writer LOGIN PASSWORD 'synthetic-edition-writer';GRANT USAGE ON SCHEMA news TO fixture_edition_writer;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.editions,news.edition_entries,news.edition_actions,news.stories,news.story_revisions,news.publication_events TO fixture_edition_writer;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_edition_writer;GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_edition_writer;
        GRANT INSERT ON news.admin_sessions,news.admin_audit,news.editions,news.edition_entries,news.edition_actions TO fixture_edition_writer;
        GRANT DELETE ON news.edition_entries TO fixture_edition_writer;
        GRANT UPDATE(lock_version) ON news.editions,news.stories TO fixture_edition_writer;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.edition_actions_id_seq TO fixture_edition_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_edition_reader").unwrap();
    url.set_password(Some("synthetic-edition-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_edition_writer").unwrap();
    url.set_password(Some("synthetic-edition-writer")).unwrap();
    let writer_url = url.to_string();
    let config = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&config);
    let writer = persistence::editor_pool(&config).unwrap();
    let app = web::router_with_editor(config.clone(), reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let session = login(&app, &auth).await;
    let drafts = Drafts::new(
        writer.clone(),
        Arc::new(config.editorial.clone()),
        Arc::new(config.taxonomy.clone()),
    );
    for year in [0, 10000] {
        let day = chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        assert!(matches!(
            drafts.create(&session, day, "PRIVATE-INVALID-DATE").await,
            Err(AppError::Validation(_))
        ));
    }
    let day = "2026-11-17".parse().unwrap();
    let head = drafts
        .create(&session, day, "PRIVATE-EDITION-CREATE")
        .await
        .unwrap();
    assert_eq!(head.lock_version, 1);
    assert_eq!(head.state, "draft");
    assert!(matches!(
        drafts.create(&session, day, "PRIVATE-DUPLICATE").await,
        Err(AppError::Conflict)
    ));
    assert!(
        drafts
            .get(&session, &head.id)
            .await
            .unwrap()
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::Empty)
    );
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                1,
                &[entry("private", 0)],
                Some("private"),
                "PRIVATE-NO-AUTO-PUBLISH"
            )
            .await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                1,
                &[entry("alpha", 0), entry("alpha", 0)],
                Some("alpha"),
                "PRIVATE-DUPLICATE"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let mut bad = entry("alpha", 0);
    bad.section = "invented".into();
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                1,
                &[bad],
                Some("alpha"),
                "PRIVATE-INVALID-SECTION"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let selections = vec![entry("beta", 0), entry("alpha", 0)];
    let (a, b) = tokio::join!(
        drafts.save(
            &session,
            &head.id,
            1,
            &selections,
            Some("alpha"),
            "PRIVATE-CONCURRENT-A"
        ),
        drafts.save(
            &session,
            &head.id,
            1,
            &selections,
            Some("alpha"),
            "PRIVATE-CONCURRENT-B"
        )
    );
    assert!(matches!(
        (&a, &b),
        (Ok(_), Err(AppError::Conflict)) | (Err(AppError::Conflict), Ok(_))
    ));
    let stored = drafts.get(&session, &head.id).await.unwrap();
    assert_eq!(stored.head.lock_version, 2);
    assert_eq!(
        stored
            .entries
            .iter()
            .map(|e| e.story_id.as_str())
            .collect::<Vec<_>>(),
        ["beta", "alpha"]
    );
    assert!(!stored.entries[0].is_lead);
    assert!(stored.entries[1].is_lead);
    assert_eq!(stored.preview.total_words, 160);
    let history = drafts.history(&session, &head.id, None).await.unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history
            .iter()
            .all(|a| a.actor_id == session.principal().id())
    );
    assert_eq!(
        drafts
            .history(&session, &head.id, Some(history[1].id))
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_editions")
            .fetch_one(&reader)
            .await
            .unwrap(),
        1
    );
    assert!(
        sqlx::query("SELECT * FROM news.edition_actions")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("SELECT * FROM news.edition_entries")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE news.edition_actions SET reason='changed'")
            .execute(&owner)
            .await
            .is_err()
    );
    assert!(matches!(
        drafts
            .save(
                &session,
                "historical-edition",
                0,
                &[],
                None,
                "PRIVATE-NO-ARCHIVE-REWRITE"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let historical = drafts.get(&session, "historical-edition").await.unwrap();
    assert!(
        historical
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::StaleSelection)
    );
    // Inject failure after membership replacement to prove the entire change rolls back.
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_edition_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='saved' THEN RAISE EXCEPTION 'synthetic audit failure' USING ERRCODE='23514';END IF;RETURN NEW;END $$;CREATE TRIGGER fixture_edition_failure BEFORE INSERT ON news.edition_actions FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_edition_audit();").execute(&owner).await.unwrap();
    assert!(
        drafts
            .save(
                &session,
                &head.id,
                2,
                &[entry("alpha", 0)],
                Some("alpha"),
                "PRIVATE-ROLLBACK"
            )
            .await
            .is_err()
    );
    sqlx::raw_sql("DROP TRIGGER fixture_edition_failure ON news.edition_actions")
        .execute(&owner)
        .await
        .unwrap();
    let after = drafts.get(&session, &head.id).await.unwrap();
    assert_eq!(after.head.lock_version, 2);
    assert_eq!(after.entries.len(), 2);
    assert_eq!(
        drafts
            .history(&session, &head.id, None)
            .await
            .unwrap()
            .len(),
        2
    );
    sqlx::query("UPDATE news.stories SET lock_version=lock_version+1 WHERE id='alpha'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(
        drafts
            .get(&session, &head.id)
            .await
            .unwrap()
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::StaleSelection)
    );
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                2,
                &selections,
                Some("alpha"),
                "PRIVATE-STALE-STORY"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let refreshed = drafts
        .save(
            &session,
            &head.id,
            2,
            &[entry("alpha", 1), entry("beta", 0)],
            Some("beta"),
            "PRIVATE-EXPLICIT-REFRESH",
        )
        .await
        .unwrap();
    assert_eq!(refreshed.lock_version, 3);
    let changed = drafts.get(&session, &head.id).await.unwrap();
    assert!(
        !changed
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::StaleSelection)
    );
    assert!(changed.entries[1].is_lead);
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,actor_id,action,public_reason) VALUES ('beta','revision-beta','synthetic-owner','withdrawn','PUBLIC-SYNTHETIC-WITHDRAWAL');UPDATE news.stories SET state='withdrawn',lock_version=lock_version+1 WHERE id='beta';").execute(&owner).await.unwrap();
    assert!(
        drafts
            .get(&session, &head.id)
            .await
            .unwrap()
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::StaleSelection)
    );
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                3,
                &[entry("beta", 1)],
                Some("beta"),
                "PRIVATE-WITHDRAWN"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let empty = drafts
        .save(&session, &head.id, 3, &[], None, "PRIVATE-EMPTY-DRAFT")
        .await
        .unwrap();
    assert_eq!(empty.lock_version, 4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_editions")
            .fetch_one(&reader)
            .await
            .unwrap(),
        1
    );
    let bad = Drafts::new(
        writer.clone(),
        Arc::new({
            let mut p = EditorialPolicy::repository();
            p.publication_timezone = "UTC".into();
            p
        }),
        Arc::new(Taxonomy::repository()),
    );
    assert!(matches!(
        bad.save(&session, &head.id, 4, &[], None, "PRIVATE-TIMEZONE-DRIFT")
            .await,
        Err(AppError::Validation(_))
    ));
    assert!(
        bad.get(&session, &head.id)
            .await
            .unwrap()
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::PublicationTimezone)
    );
    // A real import event records bookkeeping, not the original publication instant.
    let plan = world_news::legacy::plan_bytes(
        include_bytes!("../../../fixtures/legacy-stories.json"),
        "Europe/Brussels",
    )
    .unwrap();
    assert_eq!(
        world_news::legacy::apply(&owner, &plan)
            .await
            .unwrap()
            .inserted,
        3
    );
    let (legacy_revision, legacy_version): (String, i64) = sqlx::query_as(
        "SELECT published_revision_id,lock_version FROM news.stories WHERE id='legacy-a'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    let legacy_head = drafts
        .create(
            &session,
            "2026-11-19".parse().unwrap(),
            "PRIVATE-LEGACY-SELECTION",
        )
        .await
        .unwrap();
    drafts
        .save(
            &session,
            &legacy_head.id,
            1,
            &[EntryInput {
                story_id: "legacy-a".into(),
                revision_id: legacy_revision,
                section: "international".into(),
                expected_story_version: legacy_version,
            }],
            Some("legacy-a"),
            "PRIVATE-EXPLICIT-LEGACY-SELECTION",
        )
        .await
        .unwrap();
    let legacy = drafts.get(&session, &legacy_head.id).await.unwrap();
    assert!(
        legacy
            .preview
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::UnknownPublicationTime)
    );
    assert!(!legacy.preview.warnings.iter().any(|w| matches!(
        w.kind,
        WarningKind::StalePublication | WarningKind::FuturePublication
    )));
    // The draft writer receives no edition-publication column authority.
    assert!(
        sqlx::query("UPDATE news.editions SET state='published',published_at=now() WHERE id=$1")
            .bind(&head.id)
            .execute(&writer)
            .await
            .is_err()
    );
    seed(&owner, "oversized", true).await;
    sqlx::query("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES ('revision-oversized-body','oversized',2,'Synthetic oversized stored body',jsonb_build_array(jsonb_build_object('type','paragraph','text',repeat('x',9*1024*1024))),'fr','analysis','reporting','synthetic-owner')").execute(&owner).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,actor_id,action,public_reason) VALUES ('oversized','revision-oversized-body','synthetic-owner','corrected','PUBLIC-SYNTHETIC-OVERSIZED');UPDATE news.stories SET current_revision_id='revision-oversized-body',published_revision_id='revision-oversized-body' WHERE id='oversized';").execute(&owner).await.unwrap();
    let mut oversized = entry("oversized", 0);
    oversized.revision_id = "revision-oversized-body".into();
    assert!(matches!(
        drafts
            .save(
                &session,
                &head.id,
                4,
                &[oversized],
                Some("oversized"),
                "PRIVATE-BODY-BOUND"
            )
            .await,
        Err(AppError::Validation(
            "Edition preview exceeds its body-byte bound."
        ))
    ));
    let unchanged = drafts.get(&session, &head.id).await.unwrap();
    assert_eq!(unchanged.head.lock_version, 4);
    assert!(unchanged.entries.is_empty());
    auth::disable(&owner, "edition-editor").await.unwrap();
    assert!(matches!(
        drafts.get(&session, &head.id).await,
        Err(AppError::Forbidden)
    ));
    assert!(matches!(
        drafts
            .create(&session, "2026-11-18".parse().unwrap(), "PRIVATE-REVOKED")
            .await,
        Err(AppError::Forbidden)
    ));
    reader.close().await;
    writer.close().await;
    owner.close().await;
}
