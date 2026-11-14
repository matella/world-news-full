//! Conservative metadata-only normalization. No fetching, merging by headline or publication.
use super::feed::{Entry, FeedData};
use crate::{content::valid_language, error::AppError, registry::registry_url};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::collections::BTreeSet;
use url::Url;

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
/// Version 1 removes only known tracking parameters; preserves routing fragments
/// and unknown query order, duplicates and encoding. This does not fetch a URL.
pub fn canonical_url(raw: &str) -> Option<String> {
    if !registry_url(raw) {
        return None;
    }
    let mut url = Url::parse(raw).ok()?;
    if let Some(query) = url.query() {
        let kept: Vec<_> = query
            .split('&')
            .filter(|part| {
                let key = part.split('=').next().unwrap_or("").to_ascii_lowercase();
                !key.starts_with("utm_")
                    && !["fbclid", "gclid", "dclid", "msclkid"].contains(&key.as_str())
            })
            .collect();
        let query = kept.join("&");
        url.set_query((!kept.is_empty()).then_some(query.as_str()));
    }
    Some(url.into())
}
struct Normalized {
    original: String,
    url: String,
    key: String,
    title: String,
    language: String,
    fingerprint: String,
}
fn normalize(entry: &Entry) -> Result<Normalized, &'static str> {
    let title = entry
        .title
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.chars().count() <= 500 && !v.chars().any(char::is_control))
        .ok_or("invalid_title")?;
    if entry.links.is_empty() {
        return Err("missing_link");
    }
    let mut urls = BTreeSet::new();
    for link in &entry.links {
        urls.insert(canonical_url(link).ok_or("unsafe_link")?);
    }
    if urls.len() != 1 {
        return Err("ambiguous_links");
    }
    let url = urls.into_iter().next().ok_or("missing_link")?;
    let language = entry
        .language
        .as_deref()
        .filter(|v| valid_language(v))
        .unwrap_or("und")
        .to_owned();
    let fingerprint=hash(&json!({"url":url,"title":title,"language":language,"published":entry.published,"updated":entry.updated,"authors":entry.authors}).to_string());
    Ok(Normalized {
        original: entry.links[0].clone(),
        key: format!("url-v1:{}", hash(&url)),
        url,
        title: title.into(),
        language,
        fingerprint,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BatchResult {
    pub inserted: usize,
    pub duplicate: usize,
    pub metadata_changed: usize,
    pub quarantined: usize,
    pub article_ids: Vec<String>,
    pub batch_reason: Option<String>,
}
impl BatchResult {
    fn empty() -> Self {
        Self {
            inserted: 0,
            duplicate: 0,
            metadata_changed: 0,
            quarantined: 0,
            article_ids: vec![],
            batch_reason: None,
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum NormalizeOutcome {
    HeldDisabled,
    Applied(BatchResult),
}
#[derive(Clone)]
pub struct Normalizer {
    pool: PgPool,
}
#[derive(sqlx::FromRow)]
struct Batch {
    metadata: Json<Value>,
    content_hash: String,
    source_id: String,
    feed_id: String,
    source_version: i64,
    feed_version: i64,
    finished_at: Option<DateTime<Utc>>,
    outcome: String,
    entry_count: Option<i32>,
}
impl Normalizer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn apply(&self, attempt: i64) -> Result<NormalizeOutcome, AppError> {
        if attempt <= 0 {
            return Err(AppError::Validation("Invalid batch ID."));
        }
        if !crate::persistence::schema_available(&self.pool).await {
            return Err(AppError::Unavailable);
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SET LOCAL lock_timeout='3s'")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("SET LOCAL statement_timeout='10s'")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        // Small-project initial policy: serialize batches, including different feeds.
        // Other article writers must still honour unique identity/conflict constraints.
        sqlx::query("SELECT pg_advisory_xact_lock(781642901)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if let Some(Json(result)) = sqlx::query_scalar::<_, Json<BatchResult>>(
            "SELECT result FROM news.normalized_batches WHERE attempt_id=$1",
        )
        .bind(attempt)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        {
            tx.commit().await.map_err(db)?;
            return Ok(NormalizeOutcome::Applied(result));
        }
        let batch=sqlx::query_as::<_,Batch>("SELECT b.metadata,b.content_hash,a.source_id,a.feed_id,a.source_version,a.feed_version,a.finished_at,a.outcome,a.entry_count FROM news.feed_batches b JOIN news.feed_fetch_attempts a ON a.id=b.attempt_id WHERE b.attempt_id=$1").bind(attempt).fetch_one(&mut *tx).await.map_err(db)?;
        let meta = &batch.metadata.0;
        if hash(&meta.to_string()) != batch.content_hash
            || batch.outcome != "modified"
            || meta["schema"] != 1
            || meta["source_id"] != batch.source_id
            || meta["feed_id"] != batch.feed_id
            || meta["source_version"] != batch.source_version
            || meta["feed_version"] != batch.feed_version
        {
            return Err(AppError::InvalidData);
        }
        let observed = batch.finished_at.ok_or(AppError::InvalidData)?;
        let feed: FeedData =
            serde_json::from_value(meta["feed"].clone()).map_err(|_| AppError::InvalidData)?;
        if feed.entries.len() > 500 || Some(feed.entries.len() as i32) != batch.entry_count {
            return Err(AppError::InvalidData);
        }
        let source = sqlx::query_as::<_, (bool, i64)>(
            "SELECT enabled,lock_version FROM news.lock_source_for_fetch($1)",
        )
        .bind(&batch.source_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let config = sqlx::query_as::<_, (bool, i64, String)>(
            "SELECT enabled,lock_version,source_id FROM news.lock_feed_for_normalization($1)",
        )
        .bind(&batch.feed_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if config.2 != batch.source_id {
            return Err(AppError::InvalidData);
        }
        if !source.0 || !config.0 {
            return Ok(NormalizeOutcome::HeldDisabled);
        }
        let changed = source.1 != batch.source_version || config.1 != batch.feed_version;
        let mut result = BatchResult::empty();
        result.batch_reason = changed.then(|| "configuration_changed".into());
        for (index, entry) in feed.entries.iter().enumerate() {
            let raw = serde_json::to_value(entry).map_err(|_| AppError::InvalidData)?;
            let normal = if changed {
                Err("configuration_changed")
            } else {
                normalize(entry)
            };
            let (mut article, mut reason) = (None, None);
            let (mut url, mut key, mut fingerprint) = (None, None, None);
            let disposition = match normal {
                Err(problem) => {
                    reason = Some(problem);
                    "quarantined"
                }
                Ok(n) => {
                    url = Some(n.url.clone());
                    key = Some(n.key.clone());
                    fingerprint = Some(n.fingerprint.clone());
                    let matches=sqlx::query_as::<_,(String,String,Option<String>)>("SELECT id,source_id,metadata->>'normalization_fingerprint' FROM news.source_articles WHERE canonical_key=$1 OR original_url=$2 OR original_url=$3 LIMIT 3").bind(&n.key).bind(&n.original).bind(&n.url).fetch_all(&mut *tx).await.map_err(db)?;
                    if matches.len() > 1 || matches.first().is_some_and(|m| m.1 != batch.source_id)
                    {
                        reason = Some("identity_conflict");
                        "quarantined"
                    } else if let Some(existing) = matches.first() {
                        article = Some(existing.0.clone());
                        if existing.2.as_deref() == Some(&n.fingerprint) {
                            "duplicate"
                        } else {
                            reason = Some("existing_metadata_preserved");
                            "metadata_changed"
                        }
                    } else {
                        let id = format!("feed-url-v1-{}", hash(&n.url));
                        let metadata = json!({"normalizer_version":1,"canonical_url":n.url,"normalization_fingerprint":n.fingerprint,"attribution":"configured_source_unverified","language_provenance":if entry.language.as_deref().is_some_and(valid_language){"feed_parser"}else{"undetermined"},"fetched_at_semantics":"metadata_ingestion"});
                        sqlx::query("INSERT INTO news.source_articles(id,source_id,feed_id,canonical_key,original_url,title,language,usage_policy,extraction_state,published_at,metadata) VALUES($1,$2,$3,$4,$5,$6,$7,'metadata_only','metadata_only',$8,$9)").bind(&id).bind(&batch.source_id).bind(&batch.feed_id).bind(&n.key).bind(&n.original).bind(&n.title).bind(&n.language).bind(entry.published).bind(Json(metadata)).execute(&mut *tx).await.map_err(db)?;
                        article = Some(id);
                        "inserted"
                    }
                }
            };
            match disposition {
                "inserted" => result.inserted += 1,
                "duplicate" => result.duplicate += 1,
                "metadata_changed" => result.metadata_changed += 1,
                _ => result.quarantined += 1,
            }
            if let Some(id) = &article {
                result.article_ids.push(id.clone());
            }
            sqlx::query("INSERT INTO news.source_observations(attempt_id,entry_index,source_article_id,disposition,reason,original_entry,canonical_url,canonical_key,semantic_hash,observed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(attempt).bind(index as i32).bind(article).bind(disposition).bind(reason).bind(Json(raw)).bind(url).bind(key).bind(fingerprint).bind(observed).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("INSERT INTO news.normalized_batches(attempt_id,normalizer_version,result) VALUES($1,1,$2)").bind(attempt).bind(Json(&result)).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(NormalizeOutcome::Applied(result))
    }
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("normalize_batch", error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonicalization_keeps_routing_and_unknown_query_encoding() {
        assert_eq!(
            canonical_url("https://news.invalid/story?utm_source=x&a=%2f&a=2&fbclid=x#route")
                .as_deref(),
            Some("https://news.invalid/story?a=%2f&a=2#route")
        );
        for raw in [
            "http://127.0.0.1/x",
            "https://news.invalid/?token=private",
            "https://user:pass@news.invalid/x",
            "https://localhost/x",
        ] {
            assert!(canonical_url(raw).is_none());
        }
        assert_eq!(
            canonical_url("https://news.invalid/a?"),
            Some("https://news.invalid/a?".into())
        );
        assert_ne!(
            canonical_url("https://news.invalid/a#one"),
            canonical_url("https://news.invalid/a#two")
        );
    }
}
