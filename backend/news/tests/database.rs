use sqlx::{PgPool, Row, migrate::Migrator};
use std::borrow::Cow;
use world_news::{
    config::Config,
    persistence::{self, MIGRATOR},
};

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
    persistence::migrate(&pool).await.unwrap();
    persistence::migrate(&pool).await.unwrap();
    assert!(persistence::schema_available(&pool).await);
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
    assert!(persistence::schema_available(&reader).await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_stories")
            .fetch_one(&reader)
            .await
            .unwrap(),
        1
    );
    expect_code(&reader, "SELECT * FROM news.story_revisions", "42501").await;
    expect_code(&reader, "UPDATE news.stories SET state='draft'", "42501").await;

    sqlx::raw_sql("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ('a','a2','withdrawn','Synthetic withdrawal','fixture-editor');
        UPDATE news.stories SET state='withdrawn' WHERE id='a'")
        .execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM news.public_revisions")
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
