use sqlx::PgPool;
use world_news::{config::Config, error::AppError, legacy, persistence, repository::Repository};

async fn count(pool: &PgPool, table: &str) -> i64 {
    // Identifiers are fixed test literals, never caller/export input.
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires its own fresh disposable database; use scripts/test_database.py"]
async fn legacy_import_is_atomic_idempotent_and_preserves_history() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let config =
        Config::from_lookup(|key| (key == "DATABASE_URL").then(|| database_url.clone())).unwrap();
    let pool = persistence::pool(&config);
    assert!(
        !persistence::schema_available(&pool).await,
        "this suite requires a fresh database"
    );
    persistence::migrate(&pool).await.unwrap();
    let fixture = include_bytes!("../../../fixtures/legacy-stories.json");
    let plan = legacy::plan_bytes(fixture, "Europe/Brussels").unwrap();
    assert_eq!(plan.report.validated_rows, 3);
    assert_eq!(count(&pool, "stories").await, 0);

    sqlx::raw_sql("CREATE FUNCTION news.test_reject_import() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.story_id='legacy-b' THEN RAISE EXCEPTION 'synthetic interruption' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER test_reject_import BEFORE INSERT ON news.story_revisions FOR EACH ROW EXECUTE FUNCTION news.test_reject_import();")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        legacy::apply(&pool, &plan).await,
        Err(AppError::Validation(_))
    ));
    for table in [
        "stories",
        "sources",
        "source_articles",
        "legacy_records",
        "editions",
    ] {
        assert_eq!(count(&pool, table).await, 0);
    }
    sqlx::raw_sql("DROP TRIGGER test_reject_import ON news.story_revisions; DROP FUNCTION news.test_reject_import();").execute(&pool).await.unwrap();

    let first = legacy::apply(&pool, &plan).await.unwrap();
    assert_eq!(first.inserted, 3);
    assert_eq!(first.editions_created, 3);
    assert_eq!(first.quarantined.len(), 1);
    let mappings = serde_json::to_value(&first.mappings).unwrap();
    let second = legacy::apply(&pool, &plan).await.unwrap();
    assert_eq!(second.inserted, 0);
    assert_eq!(second.already_imported, 3);
    assert_eq!(second.editions_created, 0);
    assert_eq!(count(&pool, "stories").await, 3);
    assert_eq!(count(&pool, "legacy_records").await, 3);
    assert_eq!(count(&pool, "translations").await, 1);
    assert_eq!(count(&pool, "legacy_links").await, 5);
    assert_eq!(
        count(&pool, "reporting_origins").await,
        0,
        "never fabricate origins from counts"
    );
    assert_eq!(
        count(&pool, "threads").await,
        0,
        "legacy hints do not establish threads"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.publication_events WHERE action='imported'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        3
    );
    let repository = Repository::new(pool.clone());
    let story = repository.story("legacy-a").await.unwrap();
    assert_eq!(story.title, "Invented river project");
    assert_eq!(
        story.kind,
        world_news::content::ArticleKind::LegacySynthesis
    );
    assert_eq!(repository.sources(&story.id).await.unwrap().len(), 1);
    let source_record: serde_json::Value =
        sqlx::query_scalar("SELECT original FROM news.legacy_records WHERE legacy_id='legacy-a'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source_record["story"]["source_count"], 3);
    assert_eq!(source_record["story"]["origin_count"], 1);
    assert_eq!(count(&pool, "source_articles").await, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.source_articles WHERE body IS NOT NULL"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let original: serde_json::Value =
        sqlx::query_scalar("SELECT original FROM news.legacy_records WHERE legacy_id='legacy-c'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        original["corrections"][0]["reason"],
        "Invented date corrected; preserve provenance"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM news.public_translations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Projet fictif sur la place"
    );

    let mut changed: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    changed["stories"][0]["title"] = serde_json::json!("Changed legacy source; do not overwrite");
    let changed =
        legacy::plan_bytes(&serde_json::to_vec(&changed).unwrap(), "Europe/Brussels").unwrap();
    let result = legacy::apply(&pool, &changed).await.unwrap();
    assert_eq!(result.inserted, 0);
    assert_eq!(result.already_imported, 2);
    assert!(
        result
            .quarantined
            .iter()
            .any(|i| i.code == "existing_story_conflict")
    );
    assert_eq!(
        repository.story("legacy-a").await.unwrap().title,
        "Invented river project"
    );
    let mut extra: serde_json::Value = serde_json::from_slice(fixture).unwrap();
    extra["stories"][0]["id"] = serde_json::json!("new-in-old-edition");
    extra["stories"] = serde_json::json!([extra["stories"][0].clone()]);
    let extra =
        legacy::plan_bytes(&serde_json::to_vec(&extra).unwrap(), "Europe/Brussels").unwrap();
    let result = legacy::apply(&pool, &extra).await.unwrap();
    assert_eq!(result.inserted, 0);
    assert_eq!(result.quarantined[0].code, "existing_edition_conflict");
    assert_eq!(count(&pool, "stories").await, 3);
    assert_eq!(mappings.as_array().unwrap().len(), 3);
    sqlx::raw_sql(
        "CREATE ROLE fixture_import_reader LOGIN PASSWORD 'synthetic-import-reader';
        GRANT USAGE ON SCHEMA news TO fixture_import_reader;
        GRANT SELECT ON news.schema_version TO fixture_import_reader;",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut reader_url = url.clone();
    reader_url.set_username("fixture_import_reader").unwrap();
    reader_url
        .set_password(Some("synthetic-import-reader"))
        .unwrap();
    let reader_config =
        Config::from_lookup(|key| (key == "DATABASE_URL").then(|| reader_url.to_string())).unwrap();
    let reader = persistence::pool(&reader_config);
    assert!(matches!(
        legacy::apply(&reader, &plan).await,
        Err(AppError::Forbidden)
    ));
    assert_eq!(count(&pool, "stories").await, 3);
    reader.close().await;
    pool.close().await;
}
