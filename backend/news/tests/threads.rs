use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use chrono::{DateTime, Utc};
use cookie::Cookie;
use sqlx::{PgPool, types::Json};
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    content::PageSize,
    error::AppError,
    persistence,
    taxonomy::Taxonomy,
    threads::{Filter, MemberInput, Merge, Mutation, Split, State, Threads},
    web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic thread curation passphrase";
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
fn instant(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}
fn size(n: u32) -> PageSize {
    PageSize::new(n).unwrap()
}
fn versions(m: &Mutation) -> Vec<(String, i64)> {
    m.threads
        .iter()
        .map(|s| (s.id.clone(), s.version))
        .collect()
}
async fn version(service: &Threads, session: &Session, id: &str) -> i64 {
    service.get(session, id).await.unwrap().version
}
async fn assign(service: &Threads, session: &Session, id: &str, story: &str, when: &str) {
    service
        .assign(
            session,
            id,
            version(service, session, id).await,
            MemberInput {
                story_id: story.into(),
                update_at: instant(when),
            },
            "Explicit synthetic development instant",
        )
        .await
        .unwrap();
}
async fn seed(
    pool: &PgPool,
    id: &str,
    language: &str,
    topics: &[&str],
    geography: &[&str],
    public: bool,
) {
    sqlx::query("INSERT INTO news.stories(id) VALUES($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let revision = format!("{id}-r1");
    sqlx::query("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,topics,geography,actor_id,provenance) VALUES($1,$2,1,$3,$4,$5,'brief','legacy_summary',$6,$7,'legacy-import','{\"pipeline\":\"legacy_jarvis\",\"review\":\"unverified\"}')")
        .bind(&revision).bind(id).bind(format!("Synthetic {id}"))
        .bind(Json(serde_json::json!([{"type":"paragraph","text":"Synthetic thread fixture"}])))
        .bind(language).bind(topics).bind(geography).execute(pool).await.unwrap();
    if public {
        sqlx::query("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES($1,$2,'imported','Synthetic historical fixture','legacy-import')").bind(id).bind(&revision).execute(pool).await.unwrap();
        sqlx::query("UPDATE news.stories SET state='published',current_revision_id=$2,published_revision_id=$2,lock_version=1 WHERE id=$1").bind(id).bind(&revision).execute(pool).await.unwrap();
    } else {
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,lock_version=1 WHERE id=$1")
            .bind(id)
            .bind(&revision)
            .execute(pool)
            .await
            .unwrap();
    }
}
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn threads_preserve_identity_counts_reversible_curation_and_public_boundaries() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_threads"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            persistence::MIGRATOR.iter().take(13).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &previous).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.stories(id) VALUES('upgrade-story'); INSERT INTO news.threads(id,title) VALUES('upgrade-one','Unverified old subject'),('upgrade-two','Ambiguous old subject'); INSERT INTO news.thread_stories(thread_id,story_id) VALUES('upgrade-one','upgrade-story'),('upgrade-two','upgrade-story');").execute(&owner).await.unwrap();
    assert!(persistence::migrate(&owner).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.thread_stories")
            .fetch_one(&owner)
            .await
            .unwrap(),
        2
    );
    // Explicit fixture-only reconciliation, never a silent migration choice.
    sqlx::query("DELETE FROM news.thread_stories WHERE thread_id='upgrade-two'")
        .execute(&owner)
        .await
        .unwrap();
    persistence::migrate(&owner).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT update_at FROM news.thread_stories WHERE story_id='upgrade-story'"
        )
        .fetch_one(&owner)
        .await
        .unwrap()
        .is_none()
    );
    for (id, role) in [
        ("thread-publisher", Role::Publisher),
        ("thread-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, id, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("CREATE ROLE fixture_thread_reader LOGIN PASSWORD 'synthetic-thread-reader'; GRANT USAGE ON SCHEMA news TO fixture_thread_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.public_thread_members,news.public_thread_heads TO fixture_thread_reader; GRANT EXECUTE ON FUNCTION news.resolve_public_thread(text) TO fixture_thread_reader;
    CREATE ROLE fixture_thread_writer LOGIN PASSWORD 'synthetic-thread-writer'; GRANT USAGE ON SCHEMA news TO fixture_thread_writer; GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.threads,news.thread_stories,news.thread_actions TO fixture_thread_writer; GRANT INSERT ON news.admin_sessions,news.admin_audit,news.threads,news.thread_stories,news.thread_actions TO fixture_thread_writer; GRANT UPDATE ON news.threads TO fixture_thread_writer; GRANT DELETE ON news.thread_stories TO fixture_thread_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_thread_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_thread_writer; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.thread_actions_id_seq TO fixture_thread_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_thread_reader").unwrap();
    url.set_password(Some("synthetic-thread-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_thread_writer").unwrap();
    url.set_password(Some("synthetic-thread-writer")).unwrap();
    let writer_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&conf);
    let writer = persistence::editor_pool(&conf).unwrap();
    let app = web::router_with_editor(conf, reader.clone(), Some(writer.clone()));
    let auth = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth, "thread-publisher").await;
    let editor = login(&app, &auth, "thread-editor").await;
    let service = Threads::new(writer.clone());
    let public = Threads::new(reader.clone());
    let policy = Taxonomy::repository();
    assert!(matches!(
        service
            .create(
                &editor,
                "denied",
                "Denied",
                "Editor cannot curate public subject links"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    for (id, lang, topics, geography, published) in [
        ("a", "fr", vec!["science"], vec!["world"], true),
        ("b", "nl", vec!["politics"], vec!["be"], true),
        ("c", "es", vec!["society"], vec!["world"], true),
        ("d", "fr", vec!["tech"], vec![], true),
        ("p", "fr", vec![], vec![], false),
        ("w", "fr", vec![], vec![], true),
        ("race-a", "fr", vec![], vec![], true),
        ("race-b", "fr", vec![], vec![], true),
    ] {
        seed(&owner, id, lang, &topics, &geography, published).await;
    }
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('w','w-r1','withdrawn','Synthetic withdrawn fixture','legacy-import'); UPDATE news.stories SET state='withdrawn' WHERE id='w';
    INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES('e1','2026-11-10','Europe/Brussels'),('e2','2026-11-11','Europe/Brussels'); INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES('e1','a','a-r1','Science',0,true),('e1','b','b-r1','Belgium',1,false),('e2','a','a-r1','Science',0,true),('e2','c','c-r1','International',1,false); UPDATE news.editions SET state='published',published_at=now();").execute(&owner).await.unwrap();
    service
        .create(&publisher, "A", "Subject A", "Explicit subject identity")
        .await
        .unwrap();
    service
        .create(&publisher, "B", "Subject B", "Separate subject identity")
        .await
        .unwrap();
    for (story, when) in [
        ("a", "2026-11-10T09:00:00Z"),
        ("b", "2026-11-10T10:00:00Z"),
        ("d", "2026-11-10T10:00:00Z"),
        ("p", "2027-01-01T00:00:00Z"),
        ("w", "2028-01-01T00:00:00Z"),
    ] {
        assign(&service, &publisher, "A", story, when).await;
    }
    assign(&service, &publisher, "B", "c", "2026-11-11T09:00:00Z").await;
    let heads = public
        .heads(&Filter::default(), None, size(100), &policy)
        .await
        .unwrap();
    assert_eq!(heads.items.len(), 2);
    assert_eq!(heads.items[0].thread_id, "B");
    let a = heads.items.iter().find(|h| h.thread_id == "A").unwrap();
    assert_eq!(a.head_story_id, "b");
    assert_eq!(
        (a.public_story_count, a.edition_count, a.date_count),
        (3, 2, 2)
    );
    let first = public
        .heads(&Filter::default(), None, size(1), &policy)
        .await
        .unwrap();
    assert_eq!(first.items[0].thread_id, "B");
    let second = public
        .heads(&Filter::default(), first.next.as_ref(), size(1), &policy)
        .await
        .unwrap();
    assert_eq!(second.items[0].thread_id, "A");
    assert!(second.next.is_none());
    let first = public.timeline("A", None, size(1)).await.unwrap();
    assert_eq!(first.items[0].story_id, "b");
    let second = public
        .timeline("A", first.next.as_ref(), size(1))
        .await
        .unwrap();
    assert_eq!(second.items[0].story_id, "d");
    let third = public
        .timeline("A", second.next.as_ref(), size(1))
        .await
        .unwrap();
    assert_eq!(third.items[0].story_id, "a");
    assert!(third.next.is_none());
    let filtered = public
        .heads(
            &Filter {
                language: Some("fr".into()),
                topic: Some("science".into()),
                geography: Some("world".into()),
                ..Filter::default()
            },
            None,
            size(10),
            &policy,
        )
        .await
        .unwrap();
    assert_eq!(filtered.items[0].head_story_id, "b");
    assert_eq!(
        public
            .heads(
                &Filter {
                    topic: Some("technology".into()),
                    ..Filter::default()
                },
                None,
                size(10),
                &policy
            )
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        public
            .heads(
                &Filter {
                    topic: Some("invented".into()),
                    ..Filter::default()
                },
                None,
                size(10),
                &policy
            )
            .await
            .is_err()
    );
    // Distinct primary membership prevents the duplicate-head/overlap regression.
    assert!(matches!(
        service
            .assign(
                &publisher,
                "B",
                2,
                MemberInput {
                    story_id: "a".into(),
                    update_at: instant("2026-11-12T00:00:00Z")
                },
                "Do not duplicate the current head"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let merged = service
        .merge(
            &publisher,
            Merge {
                source: "A",
                source_version: version(&service, &publisher, "A").await,
                target: "B",
                target_version: version(&service, &publisher, "B").await,
            },
            "Curated subject merge",
        )
        .await
        .unwrap();
    assert_eq!(public.resolve("A").await.unwrap(), "B");
    let heads = public
        .heads(&Filter::default(), None, size(100), &policy)
        .await
        .unwrap();
    assert_eq!(heads.items.len(), 1);
    assert_eq!(heads.items[0].head_story_id, "c");
    assert_eq!(
        (
            heads.items[0].public_story_count,
            heads.items[0].edition_count,
            heads.items[0].date_count
        ),
        (4, 2, 2)
    );
    assert!(
        public
            .heads(
                &Filter {
                    language: Some("es".into()),
                    geography: Some("be".into()),
                    ..Filter::default()
                },
                None,
                size(10),
                &policy
            )
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let undone = service
        .undo(
            &publisher,
            merged.action_id,
            &versions(&merged),
            "Undo curated merge",
        )
        .await
        .unwrap();
    assert_eq!(public.resolve("A").await.unwrap(), "A");
    let redone = service
        .undo(
            &publisher,
            undone.action_id,
            &versions(&undone),
            "Redo the merge explicitly",
        )
        .await
        .unwrap();
    assert_eq!(public.resolve("A").await.unwrap(), "B");
    service
        .undo(
            &publisher,
            redone.action_id,
            &versions(&redone),
            "Return to separate subjects",
        )
        .await
        .unwrap();
    let selection = vec!["d".into()];
    let split = service
        .split(
            &publisher,
            Split {
                source: "A",
                source_version: version(&service, &publisher, "A").await,
                new_id: "C",
                title: "Explicit split subject",
                members: &selection,
            },
            "Editorial split decision",
        )
        .await
        .unwrap();
    assert_eq!(public.resolve("C").await.unwrap(), "C");
    service
        .undo(
            &publisher,
            split.action_id,
            &versions(&split),
            "Undo split without deleting identity",
        )
        .await
        .unwrap();
    assert!(matches!(public.resolve("C").await, Err(AppError::NotFound)));
    assert_eq!(
        service.get(&editor, "C").await.unwrap().state,
        State::Closed
    );
    assert!(matches!(
        service
            .undo(
                &publisher,
                split.action_id,
                &versions(&split),
                "Cannot repeat inverse action"
            )
            .await,
        Err(AppError::Conflict)
    ));
    let old = service.get(&publisher, "A").await.unwrap();
    let (one, two) = tokio::join!(
        service.edit(
            &publisher,
            "A",
            old.version,
            "Concurrent title one",
            State::Active,
            "First competing edit"
        ),
        service.edit(
            &publisher,
            "A",
            old.version,
            "Concurrent title two",
            State::Active,
            "Second competing edit"
        )
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let before = service.get(&publisher, "A").await.unwrap();
    let actions: i64 = sqlx::query_scalar("SELECT count(*) FROM news.thread_actions")
        .fetch_one(&owner)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_reject_thread_action() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic thread audit failure'; END $$; CREATE TRIGGER fixture_reject_thread_action BEFORE INSERT ON news.thread_actions FOR EACH ROW EXECUTE FUNCTION news.fixture_reject_thread_action();").execute(&owner).await.unwrap();
    assert!(
        service
            .edit(
                &publisher,
                "A",
                before.version,
                "Must roll back",
                State::Closed,
                "Injected audit failure"
            )
            .await
            .is_err()
    );
    assert_eq!(service.get(&publisher, "A").await.unwrap(), before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.thread_actions")
            .fetch_one(&owner)
            .await
            .unwrap(),
        actions
    );
    sqlx::raw_sql("DROP TRIGGER fixture_reject_thread_action ON news.thread_actions; DROP FUNCTION news.fixture_reject_thread_action();").execute(&owner).await.unwrap();
    service
        .create(
            &publisher,
            "private-thread",
            "Private subject",
            "Private membership fixture",
        )
        .await
        .unwrap();
    service
        .remove(
            &publisher,
            "A",
            version(&service, &publisher, "A").await,
            "p",
            "Remove prior membership explicitly",
        )
        .await
        .unwrap();
    assign(
        &service,
        &publisher,
        "private-thread",
        "p",
        "2026-11-14T00:00:00Z",
    )
    .await;
    assert!(matches!(
        public.resolve("private-thread").await,
        Err(AppError::NotFound)
    ));
    for query in [
        "SELECT * FROM news.threads",
        "SELECT * FROM news.thread_stories",
        "SELECT * FROM news.thread_actions",
        "UPDATE news.threads SET title='denied'",
    ] {
        assert!(sqlx::query(query).execute(&reader).await.is_err());
    }
    assert!(
        sqlx::query("UPDATE news.thread_actions SET reason='denied'")
            .execute(&owner)
            .await
            .is_err()
    );
    assert_eq!(
        service
            .history(&editor, "A", None, size(2))
            .await
            .unwrap()
            .len(),
        2
    );
    // Hold a fixture-only resolver gate, merge between alias resolution and the
    // member query, and prove a timeline sees one consistent public snapshot.
    let definition: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef('news.resolve_public_thread(text)'::regprocedure)",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    let helper = definition.replace(
        "news.resolve_public_thread(",
        "news.fixture_resolve_thread(",
    );
    assert!(helper.contains("news.fixture_resolve_thread("));
    sqlx::raw_sql(&helper).execute(&owner).await.unwrap();
    sqlx::raw_sql("REVOKE ALL ON FUNCTION news.fixture_resolve_thread(text) FROM PUBLIC; CREATE OR REPLACE FUNCTION news.resolve_public_thread(wanted text) RETURNS text LANGUAGE plpgsql STABLE SECURITY DEFINER SET search_path=pg_catalog,news AS $$ DECLARE resolved text; BEGIN SELECT news.fixture_resolve_thread(wanted) INTO resolved; PERFORM pg_advisory_xact_lock(123456789); RETURN resolved; END $$;").execute(&owner).await.unwrap();
    let mut gate = owner.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(123456789)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let snapshot_reader = public.clone();
    let pending = tokio::spawn(async move { snapshot_reader.timeline("A", None, size(100)).await });
    let mut waiting = false;
    for _ in 0..100 {
        waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND classid=0 AND objid=123456789 AND NOT granted)").fetch_one(&owner).await.unwrap();
        if waiting {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting, "fixture resolver did not reach its gate");
    let racing_merge = service
        .merge(
            &publisher,
            Merge {
                source: "A",
                source_version: version(&service, &publisher, "A").await,
                target: "B",
                target_version: version(&service, &publisher, "B").await,
            },
            "Merge while a public timeline is reading",
        )
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_unlock(123456789)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let old_timeline = pending.await.unwrap().unwrap();
    assert_eq!(old_timeline.items.len(), 3);
    assert!(old_timeline.items.iter().all(|m| m.thread_id == "A"));
    assert_eq!(
        public
            .timeline("A", None, size(100))
            .await
            .unwrap()
            .items
            .len(),
        4
    );
    sqlx::raw_sql(&definition).execute(&owner).await.unwrap();
    sqlx::query("DROP FUNCTION news.fixture_resolve_thread(text)")
        .execute(&owner)
        .await
        .unwrap();
    service
        .undo(
            &publisher,
            racing_merge.action_id,
            &versions(&racing_merge),
            "Restore separate subjects after the race fixture",
        )
        .await
        .unwrap();
    // Unknown old update instants remain NULL, including keyset pagination among NULL heads.
    seed(&owner, "null-story", "fr", &[], &[], true).await;
    service
        .create(
            &publisher,
            "null-two",
            "Undated subject",
            "Synthetic old undated membership",
        )
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO news.thread_stories(thread_id,story_id) VALUES('null-two','null-story'); INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES('upgrade-r','upgrade-story',1,'Undated historical member','[]','fr','brief','legacy_summary','legacy-import'); INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('upgrade-story','upgrade-r','imported','Synthetic history','legacy-import'); UPDATE news.stories SET state='published',current_revision_id='upgrade-r',published_revision_id='upgrade-r',lock_version=1 WHERE id='upgrade-story';").execute(&owner).await.unwrap();
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut undated = vec![];
    loop {
        let page = public
            .heads(&Filter::default(), cursor.as_ref(), size(1), &policy)
            .await
            .unwrap();
        for head in page.items {
            assert!(seen.insert(head.thread_id.clone()));
            if head.update_at.is_none() {
                undated.push(head.thread_id);
            }
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(undated, vec!["null-two", "upgrade-one"]);
    // Bound graph state, preserve attempted excess work atomically.
    sqlx::raw_sql("INSERT INTO news.threads(id,title) VALUES('capacity','Bounded subject'); INSERT INTO news.stories(id) SELECT 'capacity-'||n FROM generate_series(1,201) n; INSERT INTO news.thread_stories(thread_id,story_id) SELECT 'capacity','capacity-'||n FROM generate_series(1,200) n;").execute(&owner).await.unwrap();
    assert!(matches!(
        service
            .assign(
                &publisher,
                "capacity",
                0,
                MemberInput {
                    story_id: "capacity-201".into(),
                    update_at: instant("2026-11-14T00:00:00Z")
                },
                "Reject membership beyond the explicit bound"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        service
            .get(&editor, "capacity")
            .await
            .unwrap()
            .members
            .len(),
        200
    );
    // Alias traversal and future merge depth share the same twenty-edge bound.
    service
        .create(&publisher, "alias-0", "Alias 0", "Bounded alias fixture")
        .await
        .unwrap();
    for i in 1..=20 {
        let source = format!("alias-{}", i - 1);
        let target = format!("alias-{i}");
        service
            .create(
                &publisher,
                &target,
                &format!("Alias {i}"),
                "Bounded alias fixture",
            )
            .await
            .unwrap();
        service
            .merge(
                &publisher,
                Merge {
                    source: &source,
                    source_version: version(&service, &publisher, &source).await,
                    target: &target,
                    target_version: 1,
                },
                "Explicit alias chain fixture",
            )
            .await
            .unwrap();
    }
    assign(
        &service,
        &publisher,
        "alias-20",
        "race-a",
        "2026-11-14T00:00:00Z",
    )
    .await;
    assert_eq!(public.resolve("alias-0").await.unwrap(), "alias-20");
    service
        .create(&publisher, "alias-21", "Alias 21", "Depth refusal fixture")
        .await
        .unwrap();
    assert!(matches!(
        service
            .merge(
                &publisher,
                Merge {
                    source: "alias-20",
                    source_version: version(&service, &publisher, "alias-20").await,
                    target: "alias-21",
                    target_version: 1
                },
                "Refuse an unresolvable alias chain"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert!(matches!(
        service
            .merge(
                &publisher,
                Merge {
                    source: "alias-20",
                    source_version: version(&service, &publisher, "alias-20").await,
                    target: "alias-0",
                    target_version: version(&service, &publisher, "alias-0").await
                },
                "Reject a cycle through a merged alias"
            )
            .await,
        Err(AppError::Conflict)
    ));
    assert!(!sqlx::query_scalar::<_,bool>("SELECT has_function_privilege('fixture_thread_writer','news.resolve_public_thread(text)','EXECUTE')").fetch_one(&owner).await.unwrap());
    // Cyclic/overlapping/missing legacy links remain inert hints, not subject IDs.
    let export = serde_json::json!({"stories":[{"id":"hint-a","title":"Hint A","body":"Fixture","lang":"fr","updated_at":"2026-11-08T00:00:00Z","edition_date":"2026-11-08","related_ids":["hint-b","hint-b","missing"]},{"id":"hint-b","title":"Hint B","body":"Fixture","lang":"nl","updated_at":"2026-11-08T00:00:00Z","edition_date":"2026-11-08","related_ids":["hint-a"]}]});
    let plan =
        world_news::legacy::plan_bytes(&serde_json::to_vec(&export).unwrap(), "Europe/Brussels")
            .unwrap();
    assert_eq!(plan.report.validated_rows, 2);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM news.threads")
        .fetch_one(&owner)
        .await
        .unwrap();
    assert!(
        plan.report
            .warnings
            .iter()
            .any(|w| w.code == "related_id_not_in_export")
    );
    assert_eq!(
        world_news::legacy::apply(&owner, &plan)
            .await
            .unwrap()
            .inserted,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.legacy_links WHERE story_id IN ('hint-a','hint-b')"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.threads")
            .fetch_one(&owner)
            .await
            .unwrap(),
        count
    );
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='thread-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        service
            .create(&publisher, "role-denied", "Denied", "Live role revocation")
            .await,
        Err(AppError::Forbidden)
    ));
    sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='thread-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='thread-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(matches!(
        service
            .create(
                &publisher,
                "session-denied",
                "Denied",
                "Live session revocation"
            )
            .await,
        Err(AppError::Forbidden)
    ));
    writer.close().await;
    reader.close().await;
    owner.close().await;
}
