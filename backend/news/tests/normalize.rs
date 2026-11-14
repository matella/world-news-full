use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use world_news::{config::Config, error::AppError, intake::normalize::*, persistence};
fn config(url: &str) -> Config {
    Config::from_lookup(|k| (k == "DATABASE_URL").then(|| url.into())).unwrap()
}
fn entry(url: &str, title: &str) -> Value {
    json!({"parser_id":"unreliable-guid","title":title,"links":[url],"published":null,"updated":null,"language":"fr","authors":["Fixture author"]})
}
async fn batch(pool: &PgPool, entries: Vec<Value>, source: &str, feed: &str) -> i64 {
    let (sv,fv)=sqlx::query_as::<_,(i64,i64)>("SELECT s.lock_version,f.lock_version FROM news.sources s JOIN news.feeds f ON f.source_id=s.id WHERE s.id=$1 AND f.id=$2").bind(source).bind(feed).fetch_one(pool).await.unwrap();
    let id=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(source_id,feed_id,source_version,feed_version,outcome,entry_count,finished_at) VALUES($1,$2,$3,$4,'modified',$5,now()) RETURNING id").bind(source).bind(feed).bind(sv).bind(fv).bind(entries.len() as i32).fetch_one(pool).await.unwrap();
    let meta = json!({"schema":1,"source_id":source,"feed_id":feed,"source_version":sv,"feed_version":fv,"feed":{"final_url":"https://source.invalid/rss","format":"rss2","response_bytes":500,"title":"Fixture","entries":entries}});
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
fn applied(outcome: NormalizeOutcome) -> BatchResult {
    let NormalizeOutcome::Applied(r) = outcome else {
        panic!("expected applied")
    };
    r
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM news.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn normalization_preserves_provenance_conflicts_and_atomic_replays() {
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_normalize"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url));
    persistence::migrate(&owner).await.unwrap();
    sqlx::raw_sql("INSERT INTO news.sources(id,name,homepage_url,enabled) VALUES ('source-a','Outlet A','https://source.invalid',true),('source-b','Outlet B','https://other.invalid',true);
        INSERT INTO news.feeds(id,source_id,url,enabled) VALUES ('feed-a','source-a','https://source.invalid/rss',true),('feed-b','source-b','https://other.invalid/rss',true);
        CREATE ROLE fixture_normalizer LOGIN PASSWORD 'synthetic-normalizer'; GRANT USAGE ON SCHEMA news TO fixture_normalizer;
        GRANT SELECT ON news.schema_version,news.feed_batches,news.feed_fetch_attempts,news.source_articles,news.normalized_batches,news.source_observations TO fixture_normalizer;
        GRANT SELECT(id,name) ON news.sources TO fixture_normalizer;
        GRANT INSERT ON news.source_articles,news.normalized_batches,news.source_observations TO fixture_normalizer;
        GRANT EXECUTE ON FUNCTION news.lock_source_for_fetch(text),news.lock_feed_for_normalization(text) TO fixture_normalizer;
        CREATE ROLE fixture_normalizer_reader LOGIN PASSWORD 'synthetic-normalizer-reader'; GRANT USAGE ON SCHEMA news TO fixture_normalizer_reader;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_normalizer").unwrap();
    url.set_password(Some("synthetic-normalizer")).unwrap();
    let worker = persistence::pool(&config(url.as_str()));
    url.set_username("fixture_normalizer_reader").unwrap();
    url.set_password(Some("synthetic-normalizer-reader"))
        .unwrap();
    let reader = persistence::pool(&config(url.as_str()));
    let normalizer = Normalizer::new(worker.clone());
    let a = batch(
        &owner,
        vec![entry(
            "https://source.invalid/a?utm_source=first",
            "Same title",
        )],
        "source-a",
        "feed-a",
    )
    .await;
    let b = batch(
        &owner,
        vec![entry(
            "https://source.invalid/a?fbclid=second",
            "Same title",
        )],
        "source-a",
        "feed-a",
    )
    .await;
    let (left, right) = tokio::join!(normalizer.apply(a), normalizer.apply(b));
    let left = applied(left.unwrap());
    let right = applied(right.unwrap());
    assert_eq!(left.inserted + right.inserted, 1);
    assert_eq!(left.duplicate + right.duplicate, 1);
    assert_eq!(left.article_ids, right.article_ids);
    assert_eq!(applied(normalizer.apply(a).await.unwrap()), left);
    assert_eq!(count(&owner, "source_articles").await, 1);
    let first = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
        ),
    >(
        "SELECT id,title,original_url,body,fetched_url,attribution_name FROM news.source_articles",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(first.3, None);
    assert_eq!(first.4, None);
    assert_eq!(first.5, "Outlet A");
    assert!(first.2.contains("utm_source") || first.2.contains("fbclid"));
    let changed = batch(
        &owner,
        vec![entry("https://source.invalid/a", "Changed headline")],
        "source-a",
        "feed-a",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(changed).await.unwrap()).metadata_changed,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM news.source_articles WHERE id=$1")
            .bind(&first.0)
            .fetch_one(&owner)
            .await
            .unwrap(),
        "Same title"
    );
    let distinct = batch(
        &owner,
        vec![
            entry("https://source.invalid/b", "Same title"),
            entry("https://source.invalid/c", "Same title"),
        ],
        "source-a",
        "feed-a",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(distinct).await.unwrap()).inserted,
        2
    );
    let syndicated = batch(
        &owner,
        vec![entry("https://other.invalid/a", "Same title")],
        "source-b",
        "feed-b",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(syndicated).await.unwrap()).inserted,
        1
    );
    assert_eq!(count(&owner, "source_article_origins").await, 0);
    let conflict = batch(
        &owner,
        vec![entry("https://source.invalid/a", "Same title")],
        "source-b",
        "feed-b",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(conflict).await.unwrap()).quarantined,
        1
    );
    let mut missing = entry("https://source.invalid/missing", "");
    missing["title"] = Value::Null;
    let mut ambiguous = entry("https://source.invalid/ambiguous", "Valid");
    ambiguous["links"] = json!(["https://source.invalid/one", "https://source.invalid/two"]);
    let mut no_link = entry("https://source.invalid/none", "Valid");
    no_link["links"] = json!([]);
    let invalid = batch(
        &owner,
        vec![
            missing,
            ambiguous,
            no_link,
            entry("http://169.254.169.254/latest", "Valid"),
            entry("https://source.invalid/long", &"x".repeat(501)),
        ],
        "source-a",
        "feed-a",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(invalid).await.unwrap()).quarantined,
        5
    );
    // Legacy exact URL recognition does not replace its existing identity/text.
    sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title) VALUES('legacy','source-a','legacy:123','https://source.invalid/legacy','Original archive title')").execute(&owner).await.unwrap();
    let legacy = batch(
        &owner,
        vec![entry("https://source.invalid/legacy", "New metadata")],
        "source-a",
        "feed-a",
    )
    .await;
    let legacy_result = applied(normalizer.apply(legacy).await.unwrap());
    assert_eq!(legacy_result.metadata_changed, 1);
    assert_eq!(legacy_result.article_ids, ["legacy"]);
    // Two pre-existing archive identities at the same URL require human resolution.
    sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title) VALUES('legacy-two','source-a','legacy:456','https://source.invalid/legacy','Other archive title')").execute(&owner).await.unwrap();
    let ambiguous_legacy = batch(
        &owner,
        vec![entry("https://source.invalid/legacy", "New metadata")],
        "source-a",
        "feed-a",
    )
    .await;
    assert_eq!(
        applied(normalizer.apply(ambiguous_legacy).await.unwrap()).quarantined,
        1
    );
    let held = batch(
        &owner,
        vec![entry("https://source.invalid/held", "Held")],
        "source-a",
        "feed-a",
    )
    .await;
    sqlx::query("UPDATE news.sources SET enabled=false WHERE id='source-a'")
        .execute(&owner)
        .await
        .unwrap();
    let receipts = count(&owner, "normalized_batches").await;
    assert_eq!(
        normalizer.apply(held).await.unwrap(),
        NormalizeOutcome::HeldDisabled
    );
    assert_eq!(count(&owner, "normalized_batches").await, receipts);
    sqlx::query(
        "UPDATE news.sources SET enabled=true,lock_version=lock_version+1 WHERE id='source-a'",
    )
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(
        applied(normalizer.apply(held).await.unwrap()).quarantined,
        1
    );
    // Receipt failure must roll back new source articles AND their observations.
    let rollback = batch(
        &owner,
        vec![entry("https://source.invalid/rollback", "Rollback")],
        "source-a",
        "feed-a",
    )
    .await;
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture receipt failure'; END $$; CREATE TRIGGER fixture_fail BEFORE INSERT ON news.normalized_batches FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_receipt();").execute(&owner).await.unwrap();
    let articles = count(&owner, "source_articles").await;
    let observations = count(&owner, "source_observations").await;
    assert_eq!(normalizer.apply(rollback).await, Err(AppError::Unavailable));
    assert_eq!(count(&owner, "source_articles").await, articles);
    assert_eq!(count(&owner, "source_observations").await, observations);
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.normalized_batches; DROP FUNCTION news.fixture_fail_receipt();").execute(&owner).await.unwrap();
    assert_eq!(
        applied(normalizer.apply(rollback).await.unwrap()).inserted,
        1
    );
    // Same URL in another configured feed for the same outlet is a duplicate.
    sqlx::query("INSERT INTO news.feeds(id,source_id,url,enabled) VALUES('feed-a2','source-a','https://source.invalid/rss2',true)").execute(&owner).await.unwrap();
    let mut repeated = entry("https://source.invalid/a?utm_medium=other", "Same title");
    repeated["parser_id"] = json!("different-parser-id");
    let another_feed = batch(&owner, vec![repeated], "source-a", "feed-a2").await;
    assert_eq!(
        applied(normalizer.apply(another_feed).await.unwrap()).duplicate,
        1
    );
    let mut unknown_language = entry(
        "https://source.invalid/unknown-language",
        "Unknown language",
    );
    unknown_language["language"] = json!("invalid_tag");
    let unknown = batch(&owner, vec![unknown_language], "source-a", "feed-a").await;
    let unknown_id = applied(normalizer.apply(unknown).await.unwrap()).article_ids[0].clone();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT language FROM news.source_articles WHERE id=$1")
            .bind(unknown_id)
            .fetch_one(&owner)
            .await
            .unwrap(),
        "und"
    );
    let empty = batch(&owner, vec![], "source-a", "feed-a").await;
    assert_eq!(applied(normalizer.apply(empty).await.unwrap()).inserted, 0);
    let disabled_feed = batch(
        &owner,
        vec![entry(
            "https://source.invalid/feed-disabled",
            "Disabled feed",
        )],
        "source-a",
        "feed-a2",
    )
    .await;
    sqlx::query("UPDATE news.feeds SET enabled=false WHERE id='feed-a2'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        normalizer.apply(disabled_feed).await.unwrap(),
        NormalizeOutcome::HeldDisabled
    );
    // Observation errors also roll back the just-created article.
    let broken_observation = batch(
        &owner,
        vec![entry(
            "https://source.invalid/broken-observation",
            "Observation rollback",
        )],
        "source-a",
        "feed-a",
    )
    .await;
    sqlx::raw_sql("CREATE FUNCTION news.fixture_fail_observation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture observation failure'; END $$; CREATE TRIGGER fixture_fail BEFORE INSERT ON news.source_observations FOR EACH ROW EXECUTE FUNCTION news.fixture_fail_observation();").execute(&owner).await.unwrap();
    let before = count(&owner, "source_articles").await;
    assert_eq!(
        normalizer.apply(broken_observation).await,
        Err(AppError::Unavailable)
    );
    assert_eq!(count(&owner, "source_articles").await, before);
    sqlx::raw_sql("DROP TRIGGER fixture_fail ON news.source_observations; DROP FUNCTION news.fixture_fail_observation();").execute(&owner).await.unwrap();
    // A valid hash does not excuse a mismatched attempt context.
    let mismatch=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(source_id,feed_id,source_version,feed_version,outcome,entry_count,finished_at) VALUES('source-a','feed-a',1,0,'modified',0,now()) RETURNING id").fetch_one(&owner).await.unwrap();
    let wrong = json!({"schema":1,"source_id":"source-b","feed_id":"feed-a","source_version":1,"feed_version":0});
    let wrong_hash = format!("{:x}", Sha256::digest(wrong.to_string().as_bytes()));
    sqlx::query("INSERT INTO news.feed_batches(attempt_id,metadata,content_hash) VALUES($1,$2,$3)")
        .bind(mismatch)
        .bind(Json(wrong))
        .bind(wrong_hash)
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(normalizer.apply(mismatch).await, Err(AppError::InvalidData));
    // Invalid durable batch hash is not silently consumed.
    let corrupt=sqlx::query_scalar::<_,i64>("INSERT INTO news.feed_fetch_attempts(source_id,feed_id,source_version,feed_version,outcome,entry_count,finished_at) VALUES('source-a','feed-a',1,0,'modified',0,now()) RETURNING id").fetch_one(&owner).await.unwrap();
    sqlx::query(
        "INSERT INTO news.feed_batches(attempt_id,metadata,content_hash) VALUES($1,'{}',$2)",
    )
    .bind(corrupt)
    .bind("0".repeat(64))
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(normalizer.apply(corrupt).await, Err(AppError::InvalidData));
    for sql in [
        "UPDATE news.source_articles SET title='rewritten'",
        "UPDATE news.sources SET enabled=false",
        "INSERT INTO news.stories(id) VALUES('unauthorised')",
        "SELECT * FROM news.admin_users",
        "CREATE TABLE news.forbidden(id int)",
    ] {
        assert!(sqlx::query(sql).execute(&worker).await.is_err(), "{sql}");
    }
    for sql in [
        "SELECT * FROM news.source_observations",
        "SELECT * FROM news.normalized_batches",
        "SELECT * FROM news.lock_feed_for_normalization('feed-a')",
    ] {
        assert!(sqlx::query(sql).execute(&reader).await.is_err(), "{sql}");
    }
    for sql in [
        "UPDATE news.source_observations SET reason='rewrite'",
        "DELETE FROM news.normalized_batches",
    ] {
        assert!(sqlx::query(sql).execute(&owner).await.is_err(), "{sql}");
    }
    assert_eq!(count(&owner, "stories").await, 0);
    owner.close().await;
    worker.close().await;
    reader.close().await;
}
