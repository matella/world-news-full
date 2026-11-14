use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use cookie::Cookie;
use html_scraper::{Html, Selector};
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthService, Role, Session},
    config::Config,
    intake::jobs::{JobAdmin, Queue, WorkOutcome},
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic pipeline administration passphrase";
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
struct Login {
    cookie: String,
    csrf: String,
    session: Session,
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    login: Option<&Login>,
    origin: Option<&str>,
    body: String,
) -> axum::response::Response {
    let mut r = Request::builder().method(method).uri(path);
    if let Some(l) = login {
        r = r.header(header::COOKIE, &l.cookie);
    }
    if let Some(origin) = origin {
        r = r.header(header::ORIGIN, origin);
    }
    if method == "POST" {
        r = r.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    }
    app.clone()
        .oneshot(r.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}
async fn text(r: axum::response::Response) -> String {
    String::from_utf8(r.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}
fn body(fields: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().copied())
        .finish()
}
async fn post(
    app: &Router,
    path: &str,
    l: &Login,
    fields: &[(&str, &str)],
) -> axum::response::Response {
    let mut fields = fields.to_vec();
    fields.push(("csrf", &l.csrf));
    request(
        app,
        "POST",
        path,
        Some(l),
        Some("https://news.invalid"),
        body(&fields),
    )
    .await
}
async fn login(app: &Router, auth: &AuthService, user: &str) -> Login {
    let r = request(
        app,
        "POST",
        "/admin/login",
        None,
        Some("https://news.invalid"),
        body(&[("username", user), ("password", PASSWORD)]),
    )
    .await;
    assert_eq!(r.status(), StatusCode::SEE_OTHER);
    let mut csrf = String::new();
    let cookie = r
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| {
            let c = Cookie::parse(v.to_str().unwrap()).unwrap();
            if c.name().ends_with("csrf") {
                csrf = c.value().into();
            }
            format!("{}={}", c.name(), c.value())
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut h = axum::http::HeaderMap::new();
    h.insert(header::COOKIE, cookie.parse().unwrap());
    let session = auth.authenticate(&h).await.unwrap();
    Login {
        cookie,
        csrf,
        session,
    }
}
fn input(html: &str, name: &str) -> String {
    Html::parse_document(html)
        .select(&Selector::parse(&format!("input[name='{name}']")).unwrap())
        .next()
        .unwrap()
        .value()
        .attr("value")
        .unwrap()
        .into()
}
fn location(r: &axum::response::Response) -> String {
    r.headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
async fn batch(pool: &PgPool) -> i64 {
    let id=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(feed_id,source_id,feed_version,source_version,outcome,entry_count,finished_at) VALUES('feed-a','source-a',0,0,'modified',1,now()) RETURNING id").fetch_one(pool).await.unwrap();
    let meta = json!({"schema":1,"source_id":"source-a","feed_id":"feed-a","source_version":0,"feed_version":0,"feed":{"final_url":"https://source.invalid/rss","format":"rss2","response_bytes":200,"title":"Fixture feed","entries":[{"parser_id":"parser","title":"Available after retry","links":["https://source.invalid/retried"],"published":null,"updated":null,"language":"fr","authors":[]}]}});
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
fn source_change(csrf: &str) -> Vec<(&str, &str)> {
    vec![
        ("csrf", csrf),
        ("version", "0"),
        ("name", "Licensed fixture"),
        ("url", "https://licensed.invalid"),
        ("enabled", "yes"),
        ("rights_status", "unknown"),
        ("storage", "metadata_only"),
        ("display", "link_only"),
        ("terms_url", ""),
        ("rights_note", ""),
        ("languages", "fr"),
        ("geography", ""),
        ("attribution", ""),
        ("ownership_note", ""),
        ("editorial_note", ""),
    ]
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn pipeline_ui_enforces_roles_csrf_versions_and_manual_publishing_boundaries() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_admin_ui"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    for (user, role) in [
        ("ops-publisher", Role::Publisher),
        ("ops-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, user, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES('source-a','Synthetic <img src=x onerror=alert(1)>','https://source.invalid',true); INSERT INTO news.feeds(id,source_id,url,enabled,last_success_at) VALUES('feed-a','source-a','https://source.invalid/rss',true,clock_timestamp()-interval '2 days');
    CREATE ROLE fixture_ops_reader LOGIN PASSWORD 'synthetic-ops-reader'; GRANT USAGE ON SCHEMA news TO fixture_ops_reader; GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_ops_reader;
    CREATE ROLE fixture_ops_writer LOGIN PASSWORD 'synthetic-ops-writer'; GRANT USAGE ON SCHEMA news TO fixture_ops_writer;
    GRANT SELECT ON news.article_origin_labels,news.publication_events,news.citations,news.schema_version,news.admin_users,news.admin_sessions,news.sources,news.feeds,news.reporting_origins,news.source_origin_links,news.registry_actions,news.source_articles,news.feed_fetch_attempts,news.feed_batches,news.normalization_jobs,news.normalization_attempts,news.normalization_job_actions,news.source_observations,news.normalized_batches,news.extraction_jobs,news.extraction_attempts,news.extraction_job_actions,news.intake_snapshots,news.intake_text,news.intake_draft_links,news.stories,news.story_revisions,news.editorial_actions,news.source_extraction_limits TO fixture_ops_writer;
    GRANT INSERT ON news.citations,news.admin_sessions,news.admin_audit,news.sources,news.feeds,news.source_origin_links,news.registry_actions,news.source_articles,news.intake_snapshots,news.intake_text,news.intake_draft_links,news.stories,news.story_revisions,news.editorial_actions,news.normalization_jobs,news.normalization_job_actions,news.extraction_jobs,news.extraction_job_actions,news.feed_operational_actions,news.source_extraction_limits TO fixture_ops_writer;
    GRANT UPDATE ON news.sources,news.feeds,news.reporting_origins,news.normalization_jobs,news.normalization_attempts,news.extraction_jobs,news.extraction_attempts,news.source_extraction_limits TO fixture_ops_writer; GRANT DELETE ON news.source_origin_links TO fixture_ops_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_ops_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_ops_writer; GRANT UPDATE(intake_version,current_intake_id,extraction_state) ON news.source_articles TO fixture_ops_writer; GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_ops_writer; GRANT UPDATE(outcome,error_category,finished_at,lease_until) ON news.feed_fetch_attempts TO fixture_ops_writer;
    GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.extraction_retry_allowed(text) TO fixture_ops_writer; GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.registry_actions_id_seq,news.intake_snapshots_id_seq,news.editorial_actions_id_seq,news.normalization_jobs_id_seq,news.normalization_job_actions_id_seq,news.extraction_jobs_id_seq,news.extraction_job_actions_id_seq,news.feed_operational_actions_id_seq TO fixture_ops_writer;
    CREATE ROLE fixture_ops_worker LOGIN PASSWORD 'synthetic-ops-worker'; GRANT USAGE ON SCHEMA news TO fixture_ops_worker; GRANT SELECT ON news.schema_version,news.feed_batches,news.feed_fetch_attempts,news.source_articles,news.normalized_batches,news.source_observations,news.normalization_jobs,news.normalization_attempts TO fixture_ops_worker; GRANT SELECT(id,name) ON news.sources TO fixture_ops_worker; GRANT INSERT ON news.source_articles,news.normalized_batches,news.source_observations,news.normalization_attempts TO fixture_ops_worker; GRANT UPDATE ON news.normalization_jobs,news.normalization_attempts TO fixture_ops_worker; GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_feed_for_normalization(text) TO fixture_ops_worker;")
    .execute(&owner).await.unwrap();
    url.set_username("fixture_ops_reader").unwrap();
    url.set_password(Some("synthetic-ops-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_ops_writer").unwrap();
    url.set_password(Some("synthetic-ops-writer")).unwrap();
    let writer_url = url.to_string();
    url.set_username("fixture_ops_worker").unwrap();
    url.set_password(Some("synthetic-ops-worker")).unwrap();
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
    let publisher = login(&app, &auth, "ops-publisher").await;
    let editor = login(&app, &auth, "ops-editor").await;
    for path in [
        "/admin/pipeline",
        "/admin/sources",
        "/admin/feeds",
        "/admin/articles",
        "/admin/jobs",
        "/admin/extractions",
        "/admin/batches",
    ] {
        assert_eq!(
            request(&app, "GET", path, None, None, String::new())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(&app, "GET", path, Some(&editor), None, String::new())
                .await
                .status(),
            StatusCode::OK,
            "{path}"
        );
    }
    let source_html = text(
        request(
            &app,
            "GET",
            "/admin/sources",
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    assert!(source_html.contains("&#60;img"));
    assert!(!source_html.contains("<img src=x"));
    assert!(!source_html.contains("Ajouter une source"));
    let feeds = text(
        request(
            &app,
            "GET",
            "/admin/feeds",
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    assert!(feeds.contains("Contrôle réussi ancien"));
    assert!(!feeds.contains("Jarvis"));
    let batch = batch(&owner).await;
    let job = JobAdmin::new(writer.clone())
        .enqueue(&publisher.session, batch, 1, "Fixture ingestion retry")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_admin_import() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture import interruption'; END $$; CREATE TRIGGER fixture_fail BEFORE INSERT ON news.source_observations FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_admin_import();").execute(&owner).await.unwrap();
    assert_eq!(
        Queue::new(worker.clone()).work_once().await.unwrap(),
        WorkOutcome::Failed { job_id: job.id }
    );
    let failed = JobAdmin::new(writer.clone())
        .status(&publisher.session, job.id)
        .await
        .unwrap();
    let path = format!("/admin/jobs/{}/replay", job.id);
    let version = failed.lock_version.to_string();
    assert_eq!(
        post(
            &app,
            &path,
            &editor,
            &[("version", &version), ("reason", "Editor cannot replay")]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let before = count(&owner, "normalization_job_actions").await;
    for (origin, fields) in [
        (
            Some("https://evil.invalid"),
            vec![
                ("csrf", publisher.csrf.as_str()),
                ("version", version.as_str()),
                ("reason", "Cross origin"),
            ],
        ),
        (
            Some("https://news.invalid"),
            vec![("version", version.as_str()), ("reason", "Missing token")],
        ),
        (
            Some("https://news.invalid"),
            vec![
                ("csrf", publisher.csrf.as_str()),
                ("csrf", publisher.csrf.as_str()),
                ("version", version.as_str()),
                ("reason", "Duplicate token"),
            ],
        ),
    ] {
        assert_eq!(
            request(&app, "POST", &path, Some(&publisher), origin, body(&fields))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(count(&owner, "normalization_job_actions").await, before);
    assert_eq!(
        post(
            &app,
            &path,
            &publisher,
            &[("version", &version), ("reason", "Reviewed fixture retry")]
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        post(
            &app,
            &path,
            &publisher,
            &[("version", &version), ("reason", "Stale replay")]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.source_observations; DROP FUNCTION news.fixture_fail_admin_import();").execute(&owner).await.unwrap();
    assert_eq!(
        Queue::new(worker.clone()).work_once().await.unwrap(),
        WorkOutcome::Completed { job_id: job.id }
    );
    let available = text(
        request(
            &app,
            "GET",
            "/admin/articles?state=metadata_only",
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    assert!(available.contains("Available after retry"));
    let new = post(
        &app,
        "/admin/articles",
        &editor,
        &[
            ("source_id", "source-a"),
            ("url", "https://source.invalid/manual"),
            ("title", "Manual metadata <script>bad()</script>"),
            ("language", "fr"),
            ("notes", "My own initial notes"),
        ],
    )
    .await;
    assert_eq!(new.status(), StatusCode::SEE_OTHER);
    let article = location(&new);
    let own = text(request(&app, "GET", &article, Some(&editor), None, String::new()).await).await;
    assert!(own.contains("&#60;script&#62;"));
    assert!(!own.contains("<script>bad()"));
    assert!(!own.contains("Autoriser un traitement pendant une heure"));
    assert_eq!(
        post(
            &app,
            &format!("{article}/notes"),
            &editor,
            &[("version", "1"), ("notes", "My original follow-up notes")]
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    let draft = post(
        &app,
        &format!("{article}/draft"),
        &editor,
        &[
            ("version", "2"),
            ("notes", "Original reporting context"),
            ("title", "Our original draft"),
            (
                "body",
                "Original first paragraph.\n\nOriginal second paragraph.",
            ),
            ("language", "fr"),
            ("format", "brief"),
        ],
    )
    .await;
    assert_eq!(draft.status(), StatusCode::SEE_OTHER);
    let draft_path = location(&draft);
    let draft_html =
        text(request(&app, "GET", &draft_path, Some(&editor), None, String::new()).await).await;
    assert!(draft_html.contains("Original second paragraph."));
    assert!(draft_html.contains("Déclaration de provenance absente"));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM news.stories")
            .fetch_one(&owner)
            .await
            .unwrap(),
        "draft"
    );
    assert_eq!(count(&owner, "intake_draft_links").await, 1);
    assert_eq!(
        post(
            &app,
            &format!("{article}/notes"),
            &editor,
            &[("version", "2"), ("notes", "Stale notes")]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let own = text(request(&app, "GET", &article, Some(&editor), None, String::new()).await).await;
    assert!(own.contains(&draft_path));
    assert_eq!(
        post(
            &app,
            &format!("{article}/extract"),
            &publisher,
            &[("version", "3"), ("reason", "No rights should not grant")]
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    // Rights downgrade requires a concrete confirmation before retained text removal.
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled,rights_status,usage_policy,display_policy,rights_note,terms_url) VALUES('licensed','Licensed fixture','https://licensed.invalid',true,'documented','licensed_text','link_only','Fixture license','https://licensed.invalid/terms');INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES('licensed-article','licensed','licensed-article','https://licensed.invalid/a','Licensed metadata','ready');").execute(&owner).await.unwrap();
    let evidence = json!({"final_url":"https://licensed.invalid/a","response_hash":"0".repeat(64),"title":"Fixture licensed text","authors":[],"published":null,"paragraphs":["private-licensed-body-marker"]});
    let mut metadata = evidence.clone();
    metadata.as_object_mut().unwrap().remove("paragraphs");
    let snapshot=sqlx::query_scalar::<_,i64>("INSERT INTO news.intake_snapshots(source_article_id,version,source_version,actor_id,kind,policy,extraction_metadata) VALUES('licensed-article',1,0,'ops-publisher','extracted','{}',$1) RETURNING id").bind(Json(metadata)).fetch_one(&owner).await.unwrap();
    sqlx::query("INSERT INTO news.intake_text(snapshot_id,extracted) VALUES($1,$2)")
        .bind(snapshot)
        .bind(Json(evidence))
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE news.source_articles SET intake_version=1,current_intake_id=$1 WHERE id='licensed-article'").bind(snapshot).execute(&owner).await.unwrap();
    // A rights-eligible publisher may delegate a bounded job, never execute HTTP here.
    let grant = post(
        &app,
        "/admin/articles/licensed-article/extract",
        &publisher,
        &[
            ("version", "1"),
            ("reason", "Inspect fixture with documented rights"),
        ],
    )
    .await;
    assert_eq!(grant.status(), StatusCode::SEE_OTHER);
    let grant_path = location(&grant);
    assert!(grant_path.starts_with("/admin/extractions/"));
    let grant_page = request(&app, "GET", &grant_path, Some(&editor), None, String::new()).await;
    assert_eq!(grant_page.status(), StatusCode::OK);
    assert!(text(grant_page).await.contains("En attente"));
    let cancel = post(
        &app,
        &format!("{grant_path}/cancel"),
        &publisher,
        &[("version", "0"), ("reason", "Cancel unused fixture grant")],
    )
    .await;
    assert_eq!(cancel.status(), StatusCode::SEE_OTHER);
    assert_eq!(count(&owner, "extraction_attempts").await, 0);
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/sources/licensed",
            Some(&editor),
            Some("https://news.invalid"),
            body(&source_change(&editor.csrf))
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let confirmation = request(
        &app,
        "POST",
        "/admin/sources/licensed",
        Some(&publisher),
        Some("https://news.invalid"),
        body(&source_change(&publisher.csrf)),
    )
    .await;
    assert_eq!(confirmation.status(), StatusCode::OK);
    let confirmation = text(confirmation).await;
    assert!(confirmation.contains("Confirmer le retrait"));
    assert_eq!(count(&owner, "intake_text").await, 1);
    let payload = input(&confirmation, "payload");
    assert_eq!(
        post(
            &app,
            "/admin/sources/licensed/confirm",
            &publisher,
            &[("version", "0"), ("payload", &payload)]
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(count(&owner, "intake_text").await, 1);
    assert_eq!(
        post(
            &app,
            "/admin/sources/licensed/confirm",
            &publisher,
            &[
                ("version", "0"),
                ("payload", &payload),
                ("confirmed", "yes")
            ]
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(count(&owner, "intake_text").await, 0);
    assert_eq!(count(&owner, "intake_text_removals").await, 1);
    assert_eq!(
        post(
            &app,
            "/admin/sources/licensed/confirm",
            &publisher,
            &[
                ("version", "0"),
                ("payload", &payload),
                ("confirmed", "yes")
            ]
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let withheld = text(
        request(
            &app,
            "GET",
            &format!("/admin/articles/licensed-article/snapshots/{snapshot}"),
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    assert!(!withheld.contains("private-licensed-body-marker"));
    assert!(withheld.contains("retiré"));
    // Bounded filters/keyset pages and truthful errors.
    for n in 0..25 {
        sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES($1,'source-a',$1,$2,'Pagination fixture','metadata_only')").bind(format!("zz-page-{n:02}")).bind(format!("https://source.invalid/page/{n}")).execute(&owner).await.unwrap();
    }
    let page = text(
        request(
            &app,
            "GET",
            "/admin/articles?source=source-a",
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    let html = Html::parse_document(&page);
    assert_eq!(
        html.select(&Selector::parse("tbody tr").unwrap()).count(),
        20
    );
    let next = html
        .select(&Selector::parse("a[rel='next']").unwrap())
        .next()
        .unwrap()
        .value()
        .attr("href")
        .unwrap();
    assert_eq!(
        request(&app, "GET", next, Some(&editor), None, String::new())
            .await
            .status(),
        StatusCode::OK
    );
    for (path, expected) in [
        ("/admin/articles?state=made-up", StatusCode::BAD_REQUEST),
        ("/admin/articles/missing", StatusCode::NOT_FOUND),
        ("/admin/jobs?before=-1", StatusCode::BAD_REQUEST),
        (
            "/admin/articles?after=one&after=two",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        assert_eq!(
            request(&app, "GET", path, Some(&editor), None, String::new())
                .await
                .status(),
            expected
        );
    }
    let css = request(
        &app,
        "GET",
        "/admin/assets.css",
        Some(&editor),
        None,
        String::new(),
    )
    .await;
    assert_eq!(css.status(), StatusCode::OK);
    assert!(
        css.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/css")
    );
    assert!(
        css.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("style-src 'self'")
    );
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='ops-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        post(
            &app,
            "/admin/sources/source-a/disable",
            &publisher,
            &[("version", "0")]
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='ops-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='ops-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            "/admin/pipeline",
            Some(&publisher),
            None,
            String::new()
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(count(&owner, "publication_events").await, 0);
    // Classification edits preserve legacy authorship/content/citations and public pointers.
    sqlx::raw_sql("INSERT INTO news.stories(id) VALUES('legacy-classify'); INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,topics,actor_id,provenance) VALUES('legacy-classify-r1','legacy-classify',1,'Historical classification fixture','[{\"type\":\"paragraph\",\"text\":\"Preserved historical content.\"}]','nl','brief','legacy_summary',ARRAY['Monde'],'legacy-import','{\"pipeline\":\"legacy_jarvis\",\"review\":\"unverified\"}'); INSERT INTO news.citations(revision_id,claim_id,source_article_id,location) SELECT 'legacy-classify-r1','claim-a',id,'paragraph 1' FROM news.source_articles ORDER BY id LIMIT 1; INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('legacy-classify','legacy-classify-r1','imported','Synthetic historical fixture','legacy-import');  UPDATE news.stories SET state='published',current_revision_id='legacy-classify-r1',published_revision_id='legacy-classify-r1',lock_version=1,review_state='approved' WHERE id='legacy-classify'; ").execute(&owner).await.unwrap();
    let classify_path = "/admin/drafts/legacy-classify/classification";
    let form_page = text(
        request(
            &app,
            "GET",
            "/admin/drafts/legacy-classify",
            Some(&editor),
            None,
            String::new(),
        )
        .await,
    )
    .await;
    assert!(form_page.contains("Motif du classement"));
    let choices = [
        ("version", "1"),
        ("reason", "Editorial geography verification"),
        ("topic:politics", "yes"),
        ("geography:be", "yes"),
    ];
    let result = post(&app, classify_path, &editor, &choices).await;
    assert_eq!(result.status(), StatusCode::SEE_OTHER);
    let revision: String = sqlx::query_scalar(
        "SELECT current_revision_id FROM news.stories WHERE id='legacy-classify'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    let stored: (String, String, Vec<String>, Vec<String>, serde_json::Value) = sqlx::query_as(
        "SELECT language,kind,topics,geography,provenance FROM news.story_revisions WHERE id=$1",
    )
    .bind(&revision)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(stored.0, "nl");
    assert_eq!(stored.1, "legacy_summary");
    assert_eq!(stored.2, vec!["politics"]);
    assert_eq!(stored.3, vec!["be"]);
    assert_eq!(stored.4["pipeline"], "legacy_jarvis");
    assert_eq!(stored.4["classification"]["basis"], "editor_override");
    assert!(sqlx::query_scalar::<_,bool>("SELECT (a.title,a.standfirst,a.summary,a.body,a.language,a.format,a.kind) IS NOT DISTINCT FROM (b.title,b.standfirst,b.summary,b.body,b.language,b.format,b.kind) FROM news.story_revisions a JOIN news.story_revisions b ON b.id='legacy-classify-r1' WHERE a.id=$1").bind(&revision).fetch_one(&owner).await.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, Vec<String>>(
            "SELECT topics FROM news.public_stories WHERE id='legacy-classify'"
        )
        .fetch_one(&reader)
        .await
        .unwrap(),
        vec!["Monde"]
    );
    assert_eq!(
        stored.4["classification"]["policy_hash"],
        world_news::taxonomy::Taxonomy::repository().fingerprint()
    );
    let pointers: (String, String) = sqlx::query_as(
        "SELECT published_revision_id,review_state FROM news.stories WHERE id='legacy-classify'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(pointers, ("legacy-classify-r1".into(), "draft".into()));
    let old_topics: Vec<String> =
        sqlx::query_scalar("SELECT topics FROM news.story_revisions WHERE id='legacy-classify-r1'")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(old_topics, vec!["Monde"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.citations WHERE revision_id=$1")
            .bind(&revision)
            .fetch_one(&owner)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        post(&app, classify_path, &editor, &choices).await.status(),
        StatusCode::CONFLICT
    );
    for fields in [
        vec![
            ("version", "2"),
            ("reason", "Invalid duplicate"),
            ("topic:science", "yes"),
            ("topic:science", "yes"),
        ],
        vec![
            ("version", "2"),
            ("reason", "Invalid tag"),
            ("topic:made-up", "yes"),
        ],
        vec![
            ("version", "2"),
            ("reason", "Invalid field"),
            ("language", "fr"),
        ],
    ] {
        assert_eq!(
            post(&app, classify_path, &editor, &fields).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_classification() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic citation failure'; END $$; CREATE TRIGGER fixture_fail_classification BEFORE INSERT ON news.citations FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_classification();").execute(&owner).await.unwrap();
    let failed = post(
        &app,
        classify_path,
        &editor,
        &[
            ("version", "2"),
            ("reason", "Rollback fixture"),
            ("topic:science", "yes"),
            ("geography:world", "yes"),
        ],
    )
    .await;
    assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.story_revisions WHERE story_id='legacy-classify'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT current_revision_id FROM news.stories WHERE id='legacy-classify'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        revision
    );
    sqlx::raw_sql("DROP TRIGGER fixture_fail_classification ON news.citations; DROP FUNCTION news.fixture_fail_classification();").execute(&owner).await.unwrap();
    let mut broken = url::Url::parse(&reader_url).unwrap();
    broken.set_port(Some(9)).unwrap();
    let conf = Arc::new(config(broken.as_str(), Some(broken.as_str())));
    let broken_reader = persistence::pool(&conf);
    let broken_writer = persistence::editor_pool(&conf).unwrap();
    let offline = web::router_with_editor(conf, broken_reader.clone(), Some(broken_writer.clone()));
    assert_eq!(
        request(
            &offline,
            "GET",
            "/admin/pipeline",
            Some(&editor),
            None,
            String::new()
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    broken_reader.close().await;
    broken_writer.close().await;
    for sql in [
        "SELECT * FROM news.intake_text",
        "SELECT * FROM news.source_observations",
        "SELECT * FROM news.extraction_jobs",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err());
    }
    owner.close().await;
    reader.close().await;
    writer.close().await;
    worker.close().await;
}
