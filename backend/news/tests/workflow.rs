use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use cookie::Cookie;
use std::{collections::BTreeMap, sync::Arc};
use tower::ServiceExt;
use world_news::{
    auth::{self, Role},
    config::Config,
    persistence, web,
};
use zeroize::Zeroizing;
const PASSWORD: &str = "synthetic complete workflow passphrase";
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
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    origin: bool,
    fields: &BTreeMap<String, String>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if origin {
        builder = builder.header(header::ORIGIN, "https://news.invalid");
    }
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish();
    app.clone()
        .oneshot(
            builder
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
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
fn form(html: &str, action: &str) -> BTreeMap<String, String> {
    let doc = html_scraper::Html::parse_document(html);
    let selector = html_scraper::Selector::parse("form[action]").unwrap();
    let form = doc
        .select(&selector)
        .find(|f| f.value().attr("action") == Some(action))
        .unwrap_or_else(|| panic!("Missing form {action}"));
    let controls =
        html_scraper::Selector::parse("input[name],textarea[name],select[name]").unwrap();
    let mut result = BTreeMap::new();
    for c in form.select(&controls) {
        if c.value().attr("type") == Some("checkbox") && c.value().attr("checked").is_none() {
            continue;
        }
        let value = match c.value().name() {
            "textarea" => c.text().collect(),
            "select" => {
                let options = html_scraper::Selector::parse("option").unwrap();
                let options: Vec<_> = c.select(&options).collect();
                options
                    .iter()
                    .find(|o| o.value().attr("selected").is_some())
                    .unwrap_or(&options[0])
                    .value()
                    .attr("value")
                    .unwrap()
                    .into()
            }
            _ => c.value().attr("value").unwrap_or("").into(),
        };
        result.insert(c.value().attr("name").unwrap().into(), value);
    }
    result
}
async fn login(app: &Router, user: &str) -> String {
    let response = request(
        app,
        "POST",
        "/admin/login",
        None,
        true,
        &BTreeMap::from([
            ("username".into(), user.into()),
            ("password".into(), PASSWORD.into()),
        ]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|h| {
            let c = Cookie::parse(h.to_str().unwrap()).unwrap();
            format!("{}={}", c.name(), c.value())
        })
        .collect::<Vec<_>>()
        .join("; ")
}
async fn page_form(
    app: &Router,
    page: &str,
    action: &str,
    cookie: &str,
) -> BTreeMap<String, String> {
    let response = request(app, "GET", page, Some(cookie), false, &BTreeMap::new()).await;
    assert_eq!(response.status(), StatusCode::OK);
    form(&text(response).await, action)
}
async fn create(app: &Router, cookie: &str, title: &str) -> (String, String) {
    let mut f = page_form(app, "/admin/editor/new", "/admin/editor", cookie).await;
    f.insert("title".into(), title.into());
    f.insert("kind_0".into(), "claim".into());
    f.insert(
        "text_0".into(),
        std::iter::repeat_n("mot", 80).collect::<Vec<_>>().join(" "),
    );
    f.insert("intent".into(), "save".into());
    let response = request(app, "POST", "/admin/editor", Some(cookie), true, &f).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let path = response.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_owned();
    let id = path.rsplit('/').next().unwrap().to_owned();
    (id, path)
}
async fn sources(app: &Router, id: &str, edit: &str, cookie: &str, proof: &str) {
    let body = page_form(app, edit, edit, cookie).await;
    let claim = body["claim_0"].clone();
    let path = format!("/admin/editor/story/{id}/sources");
    let mut f = page_form(app, &path, &path, cookie).await;
    if f["methods"] == "0" {
        f.insert("intent".into(), "add_method".into());
        let response = request(app, "POST", &path, Some(cookie), true, &f).await;
        assert_eq!(response.status(), StatusCode::OK);
        f = form(&text(response).await, &path);
    }
    f.insert("mode".into(), "original".into());
    f.insert(
        "attribution".into(),
        "Original observation declared for a synthetic fixture".into(),
    );
    f.insert(
        "limits".into(),
        "One declared observer; synthetic example, no independence certificate".into(),
    );
    f.insert(
        "method_attribution_0".into(),
        "Synthetic human observation method".into(),
    );
    f.insert("private_0".into(), proof.into());
    if f["scope_count_0"] == "0" {
        f.insert("new_claim_0".into(), claim);
    } else {
        f.insert("new_claim_0".into(), "".into());
    }
    f.insert(
        "reason".into(),
        "Human explicitly confirms this revision's evidence".into(),
    );
    f.insert("intent".into(), "save".into());
    let response = request(app, "POST", &path, Some(cookie), true, &f).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
}
async fn classify(app: &Router, id: &str, cookie: &str) {
    let detail = format!("/admin/drafts/{id}");
    let action = format!("{detail}/classification");
    let mut f = page_form(app, &detail, &action, cookie).await;
    f.insert("topic:science".into(), "yes".into());
    f.insert("geography:world".into(), "yes".into());
    f.insert(
        "reason".into(),
        "Human confirms topic and geography independently of language".into(),
    );
    assert_eq!(
        request(app, "POST", &action, Some(cookie), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
}
async fn approve(
    app: &Router,
    id: &str,
    editor: &str,
    publisher: &str,
) -> BTreeMap<String, String> {
    let page = format!("/admin/editor/story/{id}/workflow");
    let submit = format!("/admin/editor/story/{id}/submit");
    let mut f = page_form(app, &page, &submit, editor).await;
    assert_eq!(
        request(app, "POST", &submit, Some(editor), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let review = format!("/admin/editor/story/{id}/review");
    f = page_form(app, &page, &review, publisher).await;
    f.insert("outcome".into(), "approved".into());
    f.insert(
        "internal_note".into(),
        "PRIVATE-WORKFLOW-APPROVAL reviewed evidence, prose and format".into(),
    );
    assert_eq!(
        request(app, "POST", &review, Some(publisher), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    page_form(
        app,
        &page,
        &format!("/admin/editor/story/{id}/publish"),
        publisher,
    )
    .await
}
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn manual_author_review_publish_correct_withdraw_is_private_fenced_and_transparent() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_workflow"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            persistence::MIGRATOR.iter().take(17).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    persistence::migrate_with(&owner, &previous).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.stories(id) VALUES('notice-upgrade'); INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id) VALUES('notice-upgrade-r1','notice-upgrade',1,'Synthetic upgrade article','[{\"type\":\"paragraph\",\"text\":\"Synthetic text\"}]','fr','brief','reporting','migration-fixture'); INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES('notice-upgrade','notice-upgrade-r1','published','PUBLIC-UPGRADE-REASON','migration-fixture'); UPDATE news.stories SET state='published',current_revision_id='notice-upgrade-r1',published_revision_id='notice-upgrade-r1' WHERE id='notice-upgrade';
      CREATE ROLE fixture_workflow_reader LOGIN PASSWORD 'synthetic-workflow-reader'; GRANT USAGE ON SCHEMA news TO fixture_workflow_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations TO fixture_workflow_reader;").execute(&owner).await.unwrap();
    url.set_username("fixture_workflow_reader").unwrap();
    url.set_password(Some("synthetic-workflow-reader")).unwrap();
    let reader_url = url.to_string();
    let reader = persistence::pool(&config(&reader_url, None));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.public_notices WHERE story_id='notice-upgrade'"
        )
        .fetch_one(&reader)
        .await
        .unwrap(),
        0
    );
    persistence::migrate(&owner).await.unwrap();
    assert!(persistence::schema_available(&reader).await);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM news.public_notices WHERE story_id='notice-upgrade' AND action='published'").fetch_one(&reader).await.unwrap(),1);
    assert!(
        sqlx::query("SELECT actor_id FROM news.public_notices")
            .execute(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("SELECT * FROM news.publication_events")
            .execute(&reader)
            .await
            .is_err()
    );
    for (user, role) in [
        ("workflow-editor", Role::Editor),
        ("workflow-publisher", Role::Publisher),
        ("workflow-browser", Role::Publisher),
    ] {
        auth::bootstrap(&owner, user, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("CREATE ROLE fixture_workflow_writer LOGIN PASSWORD 'synthetic-workflow-writer'; GRANT USAGE ON SCHEMA news TO fixture_workflow_writer;
      GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions,news.stories,news.story_revisions,news.citations,news.source_articles,news.reporting_origins,news.article_origin_labels,news.editorial_reviews,news.publication_events TO fixture_workflow_writer;
      GRANT INSERT ON news.stories,news.story_revisions,news.citations,news.editorial_reviews,news.publication_events,news.editorial_actions,news.admin_sessions,news.admin_audit TO fixture_workflow_writer;
      GRANT UPDATE(state,current_revision_id,published_revision_id,review_state,lock_version,updated_at) ON news.stories TO fixture_workflow_writer; GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_workflow_writer; GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_workflow_writer;
      GRANT USAGE ON SEQUENCE news.admin_audit_id_seq,news.editorial_actions_id_seq,news.editorial_reviews_id_seq,news.publication_events_id_seq TO fixture_workflow_writer; GRANT EXECUTE ON FUNCTION news.lock_citation_evidence(text) TO fixture_workflow_writer;").execute(&owner).await.unwrap();
    url.set_username("fixture_workflow_writer").unwrap();
    url.set_password(Some("synthetic-workflow-writer")).unwrap();
    let writer_url = url.to_string();
    let conf = Arc::new(config(&reader_url, Some(&writer_url)));
    let writer = persistence::editor_pool(&conf).unwrap();
    let app = web::router_with_editor(conf, reader.clone(), Some(writer.clone()));
    let editor = login(&app, "workflow-editor").await;
    let publisher = login(&app, "workflow-publisher").await;
    let empty = BTreeMap::new();
    let (id, edit) = create(
        &app,
        &editor,
        "Original workflow <script>plain title</script>",
    )
    .await;
    let workflow = format!("{edit}/workflow");
    let submit = format!("{edit}/submit");
    let review = format!("{edit}/review");
    let publish = format!("{edit}/publish");
    let withdraw = format!("{edit}/withdraw");
    let public = format!("/story/{id}");
    assert_eq!(
        request(&app, "GET", &workflow, None, false, &empty)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", &public, None, false, &empty)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let html = text(request(&app, "GET", &workflow, Some(&editor), false, &empty).await).await;
    assert!(html.contains("explicit sourcing declaration"));
    assert!(!html.contains("<script>plain title</script>"));
    let mut bad = form(&html, &submit);
    bad.insert("public_reason".into(), "Cannot bypass review".into());
    assert_eq!(
        request(&app, "POST", &publish, Some(&editor), true, &bad)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let pub_token = form(
        &text(request(&app, "GET", &workflow, Some(&publisher), false, &empty).await).await,
        &submit,
    )["csrf"]
        .clone();
    bad.insert("csrf".into(), pub_token);
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &bad)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    bad.insert("ready".into(), "true".into());
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &bad)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let f = page_form(&app, &workflow, &submit, &editor).await;
    let mut missing = f.clone();
    missing.remove("csrf");
    assert_eq!(
        request(&app, "POST", &submit, Some(&editor), true, &missing)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "POST", &submit, Some(&editor), false, &f)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "POST", &submit, Some(&editor), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let mut f = page_form(&app, &workflow, &review, &publisher).await;
    f.insert("outcome".into(), "changes_requested".into());
    assert_eq!(
        request(&app, "POST", &review, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    f.insert(
        "internal_note".into(),
        "PRIVATE-WORKFLOW-REVIEW please document actual sources".into(),
    );
    assert_eq!(
        request(&app, "POST", &review, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    sources(
        &app,
        &id,
        &edit,
        &editor,
        "PRIVATE-WORKFLOW-PROOF initial human declaration",
    )
    .await;
    classify(&app, &id, &editor).await;
    let stale_publish = approve(&app, &id, &editor, &publisher).await;
    let mut correction = page_form(&app, &edit, &edit, &editor).await;
    correction.insert("title".into(), "Initial reviewed workflow headline".into());
    correction.insert(
        "reason".into(),
        "PRIVATE-WORKFLOW-CHANGE initial title preparation".into(),
    );
    correction.insert("intent".into(), "save".into());
    assert_eq!(
        request(&app, "POST", &edit, Some(&editor), true, &correction)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let mut stale_publish = stale_publish;
    stale_publish.insert(
        "public_reason".into(),
        "PUBLIC-STALE-MUST-NOT-PUBLISH".into(),
    );
    let stale = request(
        &app,
        "POST",
        &publish,
        Some(&publisher),
        true,
        &stale_publish,
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        form(&text(stale).await, &publish)["public_reason"],
        "PUBLIC-STALE-MUST-NOT-PUBLISH"
    );
    assert_eq!(
        request(&app, "GET", &public, None, false, &empty)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    sources(
        &app,
        &id,
        &edit,
        &editor,
        "PRIVATE-WORKFLOW-PROOF reconfirmed initial declaration",
    )
    .await;
    let mut f = approve(&app, &id, &editor, &publisher).await;
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    f.insert("public_reason".into(), "PUBLIC-WORKFLOW-INITIAL".into());
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let first_revision: String =
        sqlx::query_scalar("SELECT published_revision_id FROM news.stories WHERE id=$1")
            .bind(&id)
            .fetch_one(&owner)
            .await
            .unwrap();
    let response = request(&app, "GET", &public, None, false, &empty).await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = text(response).await;
    assert!(
        html.contains("PUBLIC-WORKFLOW-INITIAL")
            && html.contains("Initial reviewed workflow headline")
    );
    assert!(
        !html.contains("PRIVATE-WORKFLOW")
            && !html.contains("workflow-publisher")
            && !html.contains("workflow-editor")
    );
    sqlx::query("INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES('workflow-edition','2026-11-17','Europe/Brussels')").execute(&owner).await.unwrap();
    sqlx::query("INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES('workflow-edition',$1,$2,'general',0,true)").bind(&id).bind(&first_revision).execute(&owner).await.unwrap();
    sqlx::query(
        "UPDATE news.editions SET state='published',published_at=now() WHERE id='workflow-edition'",
    )
    .execute(&owner)
    .await
    .unwrap();
    let mut f = page_form(&app, &edit, &edit, &editor).await;
    f.insert("title".into(), "Corrected workflow headline".into());
    f.insert(
        "text_0".into(),
        std::iter::repeat_n("nouveau", 80)
            .collect::<Vec<_>>()
            .join(" "),
    );
    f.insert(
        "reason".into(),
        "PRIVATE-WORKFLOW-CHANGE corrected factual wording".into(),
    );
    f.insert("intent".into(), "save".into());
    assert_eq!(
        request(&app, "POST", &edit, Some(&editor), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let html = text(request(&app, "GET", &public, None, false, &empty).await).await;
    assert!(
        html.contains("Initial reviewed workflow headline")
            && !html.contains("Corrected workflow headline")
    );
    sources(
        &app,
        &id,
        &edit,
        &editor,
        "PRIVATE-WORKFLOW-PROOF corrected declaration",
    )
    .await;
    let mut f = approve(&app, &id, &editor, &publisher).await;
    f.insert(
        "public_reason".into(),
        "PUBLIC-WORKFLOW-CORRECTION <script>literal public reason</script>".into(),
    );
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let corrected_revision: String =
        sqlx::query_scalar("SELECT published_revision_id FROM news.stories WHERE id=$1")
            .bind(&id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_ne!(corrected_revision, first_revision);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revision_id FROM news.edition_entries WHERE edition_id='workflow-edition'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        first_revision
    );
    let html = text(request(&app, "GET", &public, None, false, &empty).await).await;
    assert!(
        html.contains("Corrected workflow headline") && html.contains("PUBLIC-WORKFLOW-CORRECTION")
    );
    assert!(
        !html.contains("<script>literal public reason</script>")
            && !html.contains("PRIVATE-WORKFLOW")
    );
    let old = format!("{public}?revision={first_revision}");
    let response = request(&app, "GET", &old, None, false, &empty).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        text(response)
            .await
            .contains("Initial reviewed workflow headline")
    );
    let history = format!("{edit}/history/revision/{first_revision}");
    let response = request(&app, "GET", &history, Some(&editor), false, &empty).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        text(response)
            .await
            .contains("Initial reviewed workflow headline")
    );
    let mut f = page_form(&app, &edit, &edit, &editor).await;
    f.insert("title".into(), "PRIVATE-PENDING-AT-WITHDRAWAL".into());
    f.insert(
        "reason".into(),
        "Keep this pending candidate private".into(),
    );
    f.insert("intent".into(), "save".into());
    assert_eq!(
        request(&app, "POST", &edit, Some(&editor), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let mut f = page_form(&app, &workflow, &withdraw, &publisher).await;
    f.insert("public_reason".into(), "PUBLIC-WORKFLOW-WITHDRAW".into());
    assert_eq!(
        request(&app, "POST", &withdraw, Some(&editor), true, &f)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "POST", &withdraw, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT revision_id FROM news.publication_events WHERE story_id=$1 AND action='withdrawn'").bind(&id).fetch_one(&owner).await.unwrap(),corrected_revision);
    for path in [&public, &old] {
        let response = request(&app, "GET", path, None, false, &empty).await;
        assert_eq!(response.status(), StatusCode::GONE);
        let html = text(response).await;
        assert!(html.contains("PUBLIC-WORKFLOW-WITHDRAW"));
        assert!(
            !html.contains("<article")
                && !html.contains("PRIVATE-PENDING")
                && !html.contains("PRIVATE-WORKFLOW")
        );
    }
    let html = text(request(&app, "GET", "/edition/2026-11-17", None, false, &empty).await).await;
    assert!(html.contains("Article retiré") && !html.contains("Corrected workflow headline"));
    let fields = page_form(&app, &edit, &edit, &editor).await;
    let mut revive = BTreeMap::from([
        ("csrf".into(), f["csrf"].clone()),
        ("version".into(), fields["version"].clone()),
        ("revision".into(), fields["revision"].clone()),
        (
            "public_reason".into(),
            "Cannot revive a withdrawn article".into(),
        ),
    ]);
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &revive)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    revive.insert(
        "internal_note".into(),
        "PRIVATE-MUST-NOT-BECOME-PUBLIC".into(),
    );
    assert_eq!(
        request(&app, "POST", &publish, Some(&publisher), true, &revive)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    // The reserved-looking legacy story ID review remains an editable story, not a publisher command.
    let mut input = world_news::editorial::RevisionInput {
        title: "Reserved-looking story identity".into(),
        standfirst: None,
        summary: None,
        body: vec![world_news::content::ContentBlock::Paragraph {
            text: "mot ".repeat(80),
        }],
        language: "fr".into(),
        format: world_news::content::ArticleFormat::Brief,
        kind: world_news::content::ArticleKind::Reporting,
        topics: vec![],
        geography: vec![],
        reason: "Synthetic compatibility fixture".into(),
        evidence: None,
        length_override: None,
    };
    let auth = world_news::auth::AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::COOKIE, editor.parse().unwrap());
    let session = auth.authenticate(&headers).await.unwrap();
    world_news::editorial::EditorialService::new(writer.clone())
        .create(&session, "review", input.clone())
        .await
        .unwrap();
    let path = "/admin/editor/story/review";
    let mut f = page_form(&app, path, path, &editor).await;
    f.insert("title".into(), "Still editable by an editor".into());
    f.insert(
        "reason".into(),
        "No reserved-ID collision with review commands".into(),
    );
    f.insert("intent".into(), "save".into());
    assert_eq!(
        request(&app, "POST", path, Some(&editor), true, &f)
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    input.kind = world_news::content::ArticleKind::AutomatedSummary;
    world_news::editorial::EditorialService::new(writer.clone())
        .create(&session, "simulated-ai-draft", input)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            "/story/simulated-ai-draft",
            None,
            false,
            &empty
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT review_state FROM news.stories WHERE id='simulated-ai-draft'"
        )
        .fetch_one(&owner)
        .await
        .unwrap(),
        "draft"
    );
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='workflow-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        request(&app, "POST", &withdraw, Some(&publisher), true, &f)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    sqlx::query(
        "UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='workflow-publisher'",
    )
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(
        request(&app, "GET", &workflow, Some(&publisher), false, &empty)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    writer.close().await;
    reader.close().await;
    owner.close().await;
}
