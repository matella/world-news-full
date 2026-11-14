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
    citations::Citations,
    config::Config,
    content::*,
    editorial::{EditorialService, Head, ReviewOutcome, RevisionInput},
    error::AppError,
    origins::{OriginEvidence, OriginInput, OriginState},
    persistence,
    sourcing::*,
    web,
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
fn draft() -> RevisionInput {
    RevisionInput {
        length_override: Some(
            "Explicitly short synthetic test fixture; no padding or real article.".into(),
        ),
        title: "Synthetic sourced account".into(),
        standfirst: None,
        summary: None,
        body: ["one", "two", "three", "own"]
            .into_iter()
            .map(|id| ContentBlock::Claim {
                id: id.into(),
                text: format!("Synthetic attributed position {id}"),
            })
            .collect(),
        language: "fr".into(),
        format: ArticleFormat::Brief,
        kind: ArticleKind::Reporting,
        topics: vec![],
        geography: vec![],
        reason: "Synthetic evidence fixture".into(),
        evidence: None,
    }
}
fn package(limits: bool) -> SourcePackage {
    SourcePackage {
        statement: Some(SourceStatement {
            mode: SourceMode::External,
            attribution: "Attributed synthetic coverage".into(),
            limits: limits.then(|| {
                "One shared dispatch; attribution is not independent verification.".into()
            }),
        }),
        bindings: vec![
            Binding {
                source_article_id: "a".into(),
                anchor_id: Some("one".into()),
                location: Some("Paragraph 2 <script>plain locator</script>".into()),
            },
            Binding {
                source_article_id: "b".into(),
                anchor_id: Some("two".into()),
                location: None,
            },
            Binding {
                source_article_id: "c".into(),
                anchor_id: Some("three".into()),
                location: Some("Page 4".into()),
            },
        ],
        firsthand: vec![FirsthandEvidence {
            method: FirsthandMethod::Observation,
            claim_ids: vec!["own".into()],
            attribution: "Synthetic named observation".into(),
            private_note: "PRIVATE-FIRSTHAND-EVIDENCE".into(),
        }],
        disagreements: vec![Disagreement {
            subject: "Synthetic conflicting accounts".into(),
            positions: vec![
                Position {
                    claim_id: "one".into(),
                    attribution: "Attributed outlet A position".into(),
                },
                Position {
                    claim_id: "two".into(),
                    attribution: "Attributed outlet B position".into(),
                },
            ],
        }],
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
#[tokio::test]
#[ignore = "requires its own disposable database; use scripts/test_database.py"]
async fn citations_enforce_revision_attribution_and_preserve_public_private_boundaries() {
    let database = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_citations"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database, None));
    persistence::migrate(&owner).await.unwrap();
    for (id, role) in [
        ("citation-publisher", Role::Publisher),
        ("citation-editor", Role::Editor),
    ] {
        auth::bootstrap(&owner, id, role, Zeroizing::new(PASSWORD.into()))
            .await
            .unwrap();
    }
    sqlx::raw_sql("CREATE ROLE fixture_citation_reader LOGIN PASSWORD 'synthetic-citation-reader'; GRANT USAGE ON SCHEMA news TO fixture_citation_reader; GRANT SELECT ON news.schema_version,news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations TO fixture_citation_reader;
        INSERT INTO news.sources(id,name,homepage_url) VALUES('outlet-a','Outlet A','https://a.invalid'),('outlet-b','Outlet B','https://b.invalid'); INSERT INTO news.reporting_origins(id,name) VALUES('wire','Synthetic shared dispatch'),('other','Another declared origin');
        INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES('a','outlet-a','a','https://a.invalid/report?utm_source=one','Synthetic A','metadata_only'),('b','outlet-b','b','https://a.invalid/report?utm_source=two','Same document, second registry record','metadata_only'),('c','outlet-b','c','https://b.invalid/copy','Syndicated document','metadata_only');").execute(&owner).await.unwrap();
    url.set_username("fixture_citation_reader").unwrap();
    url.set_password(Some("synthetic-citation-reader")).unwrap();
    let reader_url = url.to_string();
    let reader = persistence::pool(&config(&reader_url, None));
    let cfg = Arc::new(config(&reader_url, Some(&database)));
    let app = web::router_with_editor(cfg, reader.clone(), Some(owner.clone()));
    let auth = AuthService::new(
        owner.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    let publisher = login(&app, &auth, "citation-publisher").await;
    let editor = login(&app, &auth, "citation-editor").await;
    let origins = OriginEvidence::new(owner.clone());
    for id in ["a", "b", "c"] {
        origins
            .assert(
                &publisher,
                id,
                0,
                OriginInput {
                    state: OriginState::Documented,
                    origin_ids: vec!["wire".into()],
                    public_basis: "Each record attributes the same synthetic wire".into(),
                    private_note: "PRIVATE-ORIGIN-EVIDENCE".into(),
                },
            )
            .await
            .unwrap();
    }
    let service = EditorialService::new(owner.clone());
    let reports = Citations::new(owner.clone());
    let head = service.create(&editor, "sourced", draft()).await.unwrap();
    let revision = service
        .revision(&editor, &head.current_revision_id)
        .await
        .unwrap();
    let report = reports.private(&editor, &revision).await.unwrap();
    assert_eq!(report.missing_claims.len(), 4);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("provenance absente"))
    );
    assert!(
        Citations::new(reader.clone())
            .public(&revision)
            .await
            .is_err()
    );
    let head = approve(&service, &publisher, "sourced", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "sourced",
                head.lock_version,
                "Missing evidence must fail"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        service.head(&editor, "sourced").await.unwrap().lock_version,
        head.lock_version
    );
    let head = service
        .source_revision(
            &editor,
            "sourced",
            head.lock_version,
            package(false),
            "Attach exact source mappings",
        )
        .await
        .unwrap();
    assert!(matches!(
        head.review_state,
        world_news::editorial::ReviewState::Draft
    ));
    let head = approve(&service, &publisher, "sourced", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "sourced",
                head.lock_version,
                "Single-origin needs limits"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let mut incomplete = package(true);
    incomplete.bindings.remove(1);
    incomplete.disagreements.clear();
    let head = service
        .source_revision(
            &editor,
            "sourced",
            head.lock_version,
            incomplete,
            "Incomplete factual attribution",
        )
        .await
        .unwrap();
    let head = approve(&service, &publisher, "sourced", head).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "sourced",
                head.lock_version,
                "Missing claim must fail"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let head = service
        .source_revision(
            &editor,
            "sourced",
            head.lock_version,
            package(true),
            "Make attribution and limits explicit",
        )
        .await
        .unwrap();
    let head = approve(&service, &publisher, "sourced", head).await;
    assert!(
        service
            .publish(
                &editor,
                "sourced",
                head.lock_version,
                "Editor cannot publish"
            )
            .await
            .is_err()
    );
    let head = service
        .publish(
            &publisher,
            "sourced",
            head.lock_version,
            "Reviewed synthetic attribution",
        )
        .await
        .unwrap();
    let published = head.current_revision_id.clone();
    let public_revision = world_news::repository::Repository::new(reader.clone())
        .story("sourced")
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&public_revision.body).unwrap(),
        serde_json::to_value(&draft().body).unwrap()
    );
    let report = Citations::new(reader.clone())
        .public(&public_revision)
        .await
        .unwrap();
    assert_eq!(
        (report.documents, report.outlets, report.documented_origins),
        (2, 2, 1)
    );
    assert!(report.missing_claims.is_empty());
    assert_eq!(report.for_claim("one").count(), 1);
    let public_json: String =
        sqlx::query_scalar("SELECT to_jsonb(r)::text FROM news.public_revisions r WHERE id=$1")
            .bind(&published)
            .fetch_one(&reader)
            .await
            .unwrap();
    assert!(!public_json.contains("PRIVATE-"));
    assert!(!public_json.contains("citation-publisher"));
    assert!(
        sqlx::query("SELECT provenance FROM news.story_revisions")
            .execute(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("SELECT news.lock_citation_evidence('anything')")
            .execute(&reader)
            .await
            .is_err()
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/story/sourced")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(
        http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(html.contains("2 document(s) distinct(s)"));
    assert!(html.contains("1 origine(s) de reportage"));
    assert!(html.contains("href=\"#reference-1\""));
    assert!(html.contains("Positions contradictoires attribuées"));
    assert!(html.contains("Attributed outlet A position"));
    assert!(!html.contains("PRIVATE-"));
    assert!(!html.contains("<script>plain locator"));
    assert!(!html.contains("dix sources"));
    // Lock the source first and change it while publication waits on the evidence lock.
    let next = service
        .source_revision(
            &editor,
            "sourced",
            head.lock_version,
            package(true),
            "Candidate before origin changes",
        )
        .await
        .unwrap();
    let next = approve(&service, &publisher, "sourced", next).await;
    let mut gate = owner.begin().await.unwrap();
    sqlx::query("SELECT id FROM news.source_articles WHERE id='a' FOR UPDATE")
        .execute(&mut *gate)
        .await
        .unwrap();
    let concurrent_service = service.clone();
    let concurrent_publisher = publisher.clone();
    let version = next.lock_version;
    let pending = tokio::spawn(async move {
        concurrent_service
            .publish(
                &concurrent_publisher,
                "sourced",
                version,
                "Must recheck changed origins",
            )
            .await
    });
    let mut blocked = false;
    for _ in 0..200 {
        blocked=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%lock_citation_evidence%' AND pid<>pg_backend_pid())").fetch_one(&owner).await.unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(blocked, "Publication did not reach its held evidence lock");
    sqlx::raw_sql("INSERT INTO news.article_origin_assertions(source_article_id,version,source_version,state,public_basis,private_note,actor_id) VALUES('a',2,0,'unknown','New evidence leaves origin uncertain','PRIVATE-RACE-EVIDENCE','citation-publisher'); UPDATE news.source_articles SET origin_version=2,current_origin_assertion_id=(SELECT id FROM news.article_origin_assertions WHERE source_article_id='a' AND version=2) WHERE id='a';").execute(&mut *gate).await.unwrap();
    gate.commit().await.unwrap();
    assert!(matches!(
        pending.await.unwrap(),
        Err(AppError::Validation(_))
    ));
    let current = world_news::repository::Repository::new(reader.clone())
        .story("sourced")
        .await
        .unwrap();
    assert_eq!(current.id, published);
    let old_report = Citations::new(reader.clone())
        .public(&current)
        .await
        .unwrap();
    assert_eq!(old_report.citations[0].origin_state, "documented");
    assert!(old_report.warnings.iter().any(|w| w.contains("diffère")));
    let refreshed = service
        .source_revision(
            &editor,
            "sourced",
            next.lock_version,
            package(true),
            "Refresh evidence after public limitation review",
        )
        .await
        .unwrap();
    let refreshed = approve(&service, &publisher, "sourced", refreshed).await;
    service
        .publish(
            &publisher,
            "sourced",
            refreshed.lock_version,
            "Clearly labelled uncertain-source correction",
        )
        .await
        .unwrap();
    let revised = world_news::repository::Repository::new(reader.clone())
        .story("sourced")
        .await
        .unwrap();
    let report = Citations::new(reader.clone())
        .public(&revised)
        .await
        .unwrap();
    assert_eq!(report.unknown_documents, 1);
    assert_eq!(report.documented_origins, 1);
    // Two contradictory origin assertions for the same canonical document are not two independent reports.
    origins
        .assert(
            &publisher,
            "b",
            1,
            OriginInput {
                state: OriginState::Documented,
                origin_ids: vec!["other".into()],
                public_basis: "Competing declared origin".into(),
                private_note: "PRIVATE-CONFLICT".into(),
            },
        )
        .await
        .unwrap();
    origins
        .assert(
            &publisher,
            "a",
            2,
            OriginInput {
                state: OriginState::Documented,
                origin_ids: vec!["wire".into()],
                public_basis: "Wire attribution retained".into(),
                private_note: "PRIVATE-CONFLICT".into(),
            },
        )
        .await
        .unwrap();
    let head = service.head(&editor, "sourced").await.unwrap();
    let head = service
        .source_revision(
            &editor,
            "sourced",
            head.lock_version,
            package(true),
            "Retain conflicting document attribution explicitly",
        )
        .await
        .unwrap();
    let revision = service
        .revision(&editor, &head.current_revision_id)
        .await
        .unwrap();
    let report = reports.private(&editor, &revision).await.unwrap();
    assert_eq!(report.conflicting_documents, 1);
    assert_eq!(report.documented_origins, 1);
    // A missing source rolls back the new revision/head, not merely its citation row.
    let before = service.head(&editor, "sourced").await.unwrap();
    let mut missing = package(true);
    missing.bindings[0].source_article_id = "missing".into();
    assert!(
        service
            .source_revision(
                &editor,
                "sourced",
                before.lock_version,
                missing,
                "Missing record must roll back"
            )
            .await
            .is_err()
    );
    assert_eq!(
        service.head(&editor, "sourced").await.unwrap().lock_version,
        before.lock_version
    );
    let raw = serde_json::json!({"stories":[{"id":"historical","title":"Historical archive","body":"Unmapped original prose","lang":"en","edition_date":"2026-11-01","updated_at":"2026-11-01T12:00:00Z","synthesized":false,"claims_json":[{"claim":"Unmapped original claim","sources":[0]}],"sources_json":[{"name":"Historical source","url":"https://historical.invalid/a"}]}]});
    let plan =
        world_news::legacy::plan_bytes(&serde_json::to_vec(&raw).unwrap(), "Europe/Brussels")
            .unwrap();
    world_news::legacy::apply(&owner, &plan).await.unwrap();
    let legacy = world_news::repository::Repository::new(reader.clone())
        .story("historical")
        .await
        .unwrap();
    let report = reports.private(&editor, &legacy).await.unwrap();
    assert!(report.citations.iter().all(|c| c.anchor_id.is_none()));
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("anciennes correspondances"))
    );
    assert!(report.metadata.statement.is_none());
    // A real single-document report is allowed with clear limits, without a source quota.
    let mut single = draft();
    single.body.truncate(1);
    let mut single_package = package(true);
    single_package.bindings.truncate(1);
    single_package.firsthand.clear();
    single_package.disagreements.clear();
    single.evidence = Some(single_package);
    let single = service.create(&editor, "single", single).await.unwrap();
    let single = approve(&service, &publisher, "single", single).await;
    service
        .publish(
            &publisher,
            "single",
            single.lock_version,
            "One-source synthetic coverage with explicit limits",
        )
        .await
        .unwrap();
    let single = world_news::repository::Repository::new(reader.clone())
        .story("single")
        .await
        .unwrap();
    let report = Citations::new(reader.clone())
        .public(&single)
        .await
        .unwrap();
    assert_eq!(
        (report.documents, report.outlets, report.documented_origins),
        (1, 1, 1)
    );
    // Neither a caller-selected historical mode nor an opinion label can manufacture provenance.
    let fake = service
        .create(&editor, "fake-history", draft())
        .await
        .unwrap();
    let mut fake_package = package(true);
    fake_package.statement.as_mut().unwrap().mode = SourceMode::Historical;
    let fake = service
        .source_revision(
            &editor,
            "fake-history",
            fake.lock_version,
            fake_package,
            "Try a historical label without import lineage",
        )
        .await
        .unwrap();
    let fake = approve(&service, &publisher, "fake-history", fake).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "fake-history",
                fake.lock_version,
                "Not a real historical import"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    let legacy_head = service.head(&editor, "historical").await.unwrap();
    let historical_package = SourcePackage {
        statement: Some(SourceStatement {
            mode: SourceMode::Historical,
            attribution: "Preserved imported archive, not newly verified reporting".into(),
            limits: Some(
                "Original claim mappings and editorial verification are unavailable.".into(),
            ),
        }),
        ..Default::default()
    };
    let legacy_head = service
        .source_revision(
            &editor,
            "historical",
            legacy_head.lock_version,
            historical_package,
            "Add transparent historical limitations",
        )
        .await
        .unwrap();
    let legacy_head = service
        .length_revision(
            &editor,
            "historical",
            legacy_head.lock_version,
            Some(
                "Short historical metadata correction; preserve the original archive text.".into(),
            ),
            "Explicit historical length review",
        )
        .await
        .unwrap();
    let legacy_head = approve(&service, &publisher, "historical", legacy_head).await;
    service
        .publish(
            &publisher,
            "historical",
            legacy_head.lock_version,
            "Explicitly unverified historical attribution correction",
        )
        .await
        .unwrap();
    // Projection drops unknown nested fields as well as known private-note fields.
    let mut hostile = serde_json::to_value(package(true)).unwrap();
    hostile["statement"]["private_note"] = serde_json::json!("PRIVATE-NESTED-STATEMENT");
    hostile["disagreements"][0]["positions"][0]["private_note"] =
        serde_json::json!("PRIVATE-NESTED-POSITION");
    let projected: serde_json::Value = sqlx::query_scalar("SELECT news.project_sourcing($1)")
        .bind(sqlx::types::Json(hostile))
        .fetch_one(&reader)
        .await
        .unwrap();
    assert!(!projected.to_string().contains("PRIVATE-"));
    // Editing prose does not implicitly transfer source references, notes or old approval.
    let current = service.head(&editor, "single").await.unwrap();
    let replacement = service
        .revise(&editor, "single", current.lock_version, draft())
        .await
        .unwrap();
    let replacement = approve(&service, &publisher, "single", replacement).await;
    assert!(matches!(
        service
            .publish(
                &publisher,
                "single",
                replacement.lock_version,
                "New prose without evidence must fail"
            )
            .await,
        Err(AppError::Validation(_))
    ));
    sqlx::query("UPDATE news.admin_sessions SET revoked_at=now() WHERE user_id='citation-editor'")
        .execute(&owner)
        .await
        .unwrap();
    assert!(
        service
            .source_revision(
                &editor,
                "sourced",
                before.lock_version,
                package(true),
                "Revoked editors cannot change evidence"
            )
            .await
            .is_err()
    );
    owner.close().await;
    reader.close().await;
}
