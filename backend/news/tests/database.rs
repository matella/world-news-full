use sqlx::{PgPool, Row, migrate::Migrator};
use std::borrow::Cow;
use world_news::{
    config::Config,
    persistence::{self, MIGRATOR},
};
use world_news::{content::*, error::AppError, repository::Repository};

async fn expect_code(pool: &PgPool, statement: &str, expected: &str) {
    let error = sqlx::raw_sql(statement).execute(pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected)
    );
}

// A deliberate opt-in. scripts/test_database.py supplies a fresh local database.
#[tokio::test]
#[ignore = "requires a disposable database; use scripts/test_database.py"]
async fn migrations_constraints_snapshots_and_reader_boundary() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").expect("test URL required");
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
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(database.starts_with("worldnews_disposable_"));
    assert!(!persistence::schema_available(&pool).await);
    sqlx::raw_sql("CREATE TABLE public.legacy_sentinel(value text); INSERT INTO public.legacy_sentinel VALUES ('preserved')")
        .execute(&pool).await.unwrap();

    let first = Migrator {
        migrations: Cow::Owned(vec![MIGRATOR.iter().next().unwrap().clone()]),
        ..Migrator::DEFAULT
    };
    persistence::migrate_with(&pool, &first).await.unwrap();
    assert!(!persistence::schema_available(&pool).await);
    let version_two = Migrator {
        migrations: Cow::Owned(MIGRATOR.iter().take(2).cloned().collect()),
        ..Migrator::DEFAULT
    };
    persistence::migrate_with(&pool, &version_two)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url) VALUES ('pre-v3','Pre-upgrade fixture','https://pre.invalid');
        INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title) VALUES ('pre-v3','pre-v3','pre-v3','https://pre.invalid/article','Pre-upgrade fixture');")
        .execute(&pool).await.unwrap();
    persistence::migrate(&pool).await.unwrap();
    persistence::migrate(&pool).await.unwrap();
    assert!(persistence::schema_available(&pool).await);
    expect_code(
        &pool,
        "UPDATE news.source_articles SET original_url='https://changed.invalid' WHERE id='pre-v3'",
        "23514",
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM public.legacy_sentinel")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "preserved"
    );

    sqlx::raw_sql("INSERT INTO news.stories(id) VALUES ('a'),('b');
        INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id)
        VALUES ('a1','a',1,'Original title','[{\"type\":\"paragraph\",\"text\":\"Synthetic body\"}]','fr','brief','reporting','fixture-editor'),
        ('b1','b',1,'Private draft','[]','fr','brief','reporting','fixture-editor');
        UPDATE news.stories SET current_revision_id=CASE id WHEN 'a' THEN 'a1' ELSE 'b1' END;")
        .execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_stories")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let owner_repository = Repository::new(pool.clone());
    assert!(matches!(
        owner_repository.story("a").await,
        Err(AppError::NotFound)
    ));
    assert!(
        owner_repository
            .latest_edition("Europe/Brussels")
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        owner_repository.story("invalid/id").await,
        Err(AppError::Validation(_))
    ));
    expect_code(
        &pool,
        "UPDATE news.story_revisions SET title='silently changed' WHERE id='a1'",
        "23514",
    )
    .await;
    expect_code(
        &pool,
        "UPDATE news.stories SET state='published',published_revision_id='a1' WHERE id='a'",
        "23514",
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE news.stories SET current_revision_id='b1' WHERE id='a'")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(tx.commit().await.is_err()); // deferred ownership FK
    expect_code(
        &pool,
        "INSERT INTO news.jobs(kind,state) VALUES ('fetch','running')",
        "23514",
    )
    .await;
    expect_code(&pool, "INSERT INTO news.source_articles(source_id,canonical_key,original_url,title,body) VALUES ('missing','x','https://feed.invalid/x','x','unlicensed copy')", "23514").await;

    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url) VALUES ('source','Synthetic outlet','https://feed.invalid');
        INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title) VALUES ('article','source','fixture-key','https://feed.invalid/article','Synthetic article');
        INSERT INTO news.citations(revision_id,claim_id,source_article_id) VALUES ('a1','claim-1','article');")
        .execute(&pool).await.unwrap();
    {
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('a','a1','published','Synthetic publication','fixture-editor');
            UPDATE news.stories SET state='published',published_revision_id='a1' WHERE id='a'")
            .execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_stories")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('a','a1','published','Synthetic publication','fixture-editor');
        UPDATE news.stories SET state='published',published_revision_id='a1' WHERE id='a';
        INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES ('ed','2026-01-10','Europe/Brussels');
        INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES ('ed','a','a1','Monde',0,true);
        UPDATE news.editions SET state='published',published_at=now() WHERE id='ed';")
        .execute(&pool).await.unwrap();
    assert_eq!(
        owner_repository.story("a").await.unwrap().title,
        "Original title"
    );
    assert_eq!(owner_repository.sources("a1").await.unwrap().len(), 1);
    let entries = owner_repository.edition_entries("ed").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].revision.as_ref().unwrap().id, "a1");
    expect_code(&pool,"UPDATE news.source_articles SET original_url='https://elsewhere.invalid' WHERE id='article'","23514").await;
    sqlx::raw_sql("INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES ('ed-next','2026-01-12','Europe/Brussels');
        INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES ('ed-next','a','a1','Monde',0,true);
        UPDATE news.editions SET state='published',published_at=now() WHERE id='ed-next'")
        .execute(&pool).await.unwrap();
    let first_page = owner_repository
        .editions("Europe/Brussels", None, PageSize::new(1).unwrap())
        .await
        .unwrap();
    assert_eq!(first_page.items[0].id, "ed-next");
    let next_page = owner_repository
        .editions(
            "Europe/Brussels",
            first_page.next.as_ref(),
            PageSize::new(1).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next_page.items[0].id, "ed");
    assert!(next_page.next.is_none());
    expect_code(
        &pool,
        "UPDATE news.citations SET location='silent change' WHERE revision_id='a1'",
        "23514",
    )
    .await;
    expect_code(
        &pool,
        "UPDATE news.edition_entries SET position=1 WHERE edition_id='ed'",
        "23514",
    )
    .await;
    expect_code(&pool, "DELETE FROM news.editions WHERE id='ed'", "23514").await;
    expect_code(&pool, "INSERT INTO news.editions(id,edition_date,publication_timezone,state,published_at) VALUES ('empty','2026-01-11','Europe/Brussels','published',now())", "23514").await;

    sqlx::raw_sql("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id)
        VALUES ('a2','a',2,'Corrected title','[]','fr','brief','reporting','fixture-editor');
        UPDATE news.stories SET current_revision_id='a2' WHERE id='a';")
        .execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM news.public_stories WHERE id='a'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Original title"
    );
    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('a','a2','corrected','Synthetic correction','fixture-editor');
        UPDATE news.stories SET published_revision_id='a2' WHERE id='a'")
        .execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT revision_id FROM news.public_edition_entries WHERE edition_id='ed'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "a1"
    );

    sqlx::raw_sql("CREATE ROLE fixture_public LOGIN PASSWORD 'synthetic-public-only';
        GRANT USAGE ON SCHEMA news TO fixture_public;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,
            news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_public;")
        .execute(&pool).await.unwrap();
    let mut reader_url = url.clone();
    reader_url.set_username("fixture_public").unwrap();
    reader_url
        .set_password(Some("synthetic-public-only"))
        .unwrap();
    let reader_config =
        Config::from_lookup(|key| (key == "DATABASE_URL").then(|| reader_url.to_string())).unwrap();
    let reader = persistence::pool(&reader_config);
    let public_repository = Repository::new(reader.clone());
    assert!(persistence::schema_available(&reader).await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_stories")
            .fetch_one(&reader)
            .await
            .unwrap(),
        1
    );
    expect_code(&reader, "SELECT * FROM news.story_revisions", "42501").await;
    assert!(matches!(
        public_repository.draft_head("a").await,
        Err(AppError::Forbidden)
    ));
    assert_eq!(
        public_repository.story("a").await.unwrap().title,
        "Corrected title"
    );
    expect_code(&reader, "UPDATE news.stories SET state='draft'", "42501").await;

    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('a','a2','withdrawn','Synthetic withdrawal','fixture-editor');
        UPDATE news.stories SET state='withdrawn' WHERE id='a'")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        public_repository.story("a").await,
        Err(AppError::NotFound)
    ));
    let withdrawn = public_repository.edition_entries("ed").await.unwrap();
    assert!(withdrawn[0].withdrawn && withdrawn[0].revision.is_none());

    fn draft(id: &str) -> DraftInput {
        DraftInput {
            id: id.into(),
            title: "Synthetic draft".into(),
            body: vec![],
            language: "fr".into(),
            format: ArticleFormat::Brief,
            kind: ArticleKind::Reporting,
            actor_id: "fixture-editor".into(),
        }
    }
    assert_eq!(
        owner_repository
            .create_draft(draft("new-draft"))
            .await
            .unwrap()
            .lock_version,
        1
    );
    assert!(matches!(
        owner_repository.create_draft(draft("new-draft")).await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        public_repository.create_draft(draft("reader-denied")).await,
        Err(AppError::Forbidden)
    ));
    sqlx::raw_sql("CREATE FUNCTION news.test_reject_revision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.story_id='reject-draft' THEN RAISE EXCEPTION 'synthetic test rejection' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER test_reject_revision BEFORE INSERT ON news.story_revisions FOR EACH ROW EXECUTE FUNCTION news.test_reject_revision();")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        owner_repository.create_draft(draft("reject-draft")).await,
        Err(AppError::Validation(_))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.stories WHERE id='reject-draft'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::raw_sql("DROP TRIGGER test_reject_revision ON news.story_revisions; DROP FUNCTION news.test_reject_revision();
        INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id)
        VALUES ('bad-block','new-draft',2,'Malformed stored blocks','[{\"type\":\"script\",\"text\":\"do not execute\"}]','fr','brief','reporting','fixture-editor');
        INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('new-draft','bad-block','published','Synthetic malformed content fixture','fixture-editor');
        UPDATE news.stories SET state='published',published_revision_id='bad-block' WHERE id='new-draft';")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        public_repository.story("new-draft").await,
        Err(AppError::InvalidData)
    ));
    sqlx::raw_sql("INSERT INTO news.stories(id) VALUES ('unsafe-link');
        INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,actor_id)
        VALUES ('unsafe-link-1','unsafe-link',1,'Synthetic link validation','[]','fr','brief','reporting','fixture-editor');
        INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title)
        VALUES ('unsafe-source','source','unsafe-source','https://user:synthetic-private@feed.invalid/article','Synthetic unsafe link');
        INSERT INTO news.citations(revision_id,claim_id,source_article_id) VALUES ('unsafe-link-1','ref','unsafe-source');
        INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id)
        VALUES ('unsafe-link','unsafe-link-1','published','Synthetic unsafe link fixture','fixture-editor');
        UPDATE news.stories SET state='published',published_revision_id='unsafe-link-1' WHERE id='unsafe-link';")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        public_repository.sources("unsafe-link-1").await,
        Err(AppError::InvalidData)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.public_revisions WHERE story_id='a'"
        )
        .fetch_one(&reader)
        .await
        .unwrap(),
        0
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT withdrawn FROM news.public_edition_entries WHERE edition_id='ed'"
        )
        .fetch_one(&reader)
        .await
        .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM news.public_notices WHERE action='withdrawn'"
        )
        .fetch_one(&reader)
        .await
        .unwrap(),
        1
    );

    let checksum: Vec<u8> =
        sqlx::query("SELECT checksum FROM news._sqlx_migrations WHERE version=2")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    sqlx::query("UPDATE news._sqlx_migrations SET checksum=decode('00','hex') WHERE version=2")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!persistence::schema_available(&pool).await);
    assert!(persistence::migrate(&pool).await.is_err());
    sqlx::query("UPDATE news._sqlx_migrations SET checksum=$1 WHERE version=2")
        .bind(checksum)
        .execute(&pool)
        .await
        .unwrap();
    assert!(persistence::schema_available(&pool).await);
    reader.close().await;
    pool.close().await;
}
