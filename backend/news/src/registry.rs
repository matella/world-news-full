//! Authenticated configuration only. Registration never performs an HTTP request.
use crate::{
    auth::{self, Permission, Principal, Session},
    content::{PageSize, safe_source_url, valid_id, valid_language},
    error::AppError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction, types::Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RightsStatus {
    Unknown,
    Documented,
    Original,
}
impl RightsStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Documented => "documented",
            Self::Original => "original",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoragePolicy {
    MetadataOnly,
    LicensedText,
    Original,
}
impl StoragePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata_only",
            Self::LicensedText => "licensed_text",
            Self::Original => "original",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayPolicy {
    LinkOnly,
    Summary,
    FullText,
}
impl DisplayPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::LinkOnly => "link_only",
            Self::Summary => "summary",
            Self::FullText => "full_text",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub name: String,
    pub homepage_url: String,
    pub enabled: bool,
    pub ownership_note: Option<String>,
    pub usage_policy: StoragePolicy,
    pub terms_url: Option<String>,
    pub languages: Vec<String>,
    pub geographic_coverage: Vec<String>,
    pub attribution: Option<String>,
    pub editorial_note: Option<String>,
    pub rights_status: RightsStatus,
    pub display_policy: DisplayPolicy,
    pub rights_note: Option<String>,
    pub origin_ids: Vec<String>,
}
impl SourceSpec {
    fn validate(&self) -> Result<(), AppError> {
        let arrays = self.languages.len() <= 20
            && self.languages.iter().all(|v| valid_language(v))
            && self.geographic_coverage.len() <= 20
            && self.geographic_coverage.iter().all(|v| text(v, 100))
            && self.origin_ids.len() <= 20
            && self.origin_ids.iter().all(|v| valid_id(v));
        let unique = self
            .origin_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == self.origin_ids.len();
        if !text(&self.name, 300)
            || !registry_url(&self.homepage_url)
            || !arrays
            || !unique
            || [
                &self.ownership_note,
                &self.editorial_note,
                &self.rights_note,
            ]
            .iter()
            .any(|v| v.as_ref().is_some_and(|s| !text(s, 10000)))
            || self.attribution.as_ref().is_some_and(|s| !text(s, 1000))
            || self.terms_url.as_ref().is_some_and(|v| !registry_url(v))
        {
            return Err(AppError::Validation("Invalid source configuration."));
        }
        let valid = match self.rights_status {
            RightsStatus::Unknown => {
                self.usage_policy == StoragePolicy::MetadataOnly
                    && self.display_policy == DisplayPolicy::LinkOnly
            }
            RightsStatus::Documented => {
                self.terms_url.is_some()
                    && self.rights_note.is_some()
                    && self.usage_policy != StoragePolicy::Original
                    && (self.display_policy != DisplayPolicy::FullText
                        || self.usage_policy == StoragePolicy::LicensedText)
            }
            RightsStatus::Original => {
                self.usage_policy == StoragePolicy::Original && self.rights_note.is_some()
            }
        };
        if !valid {
            return Err(AppError::Validation(
                "Usage needs explicit rights evidence; unknown means metadata and links only.",
            ));
        }
        Ok(())
    }
    /// Legacy declarations are retained but never silently certified as permission.
    pub fn effective_storage(&self) -> StoragePolicy {
        if self.rights_status == RightsStatus::Unknown {
            StoragePolicy::MetadataOnly
        } else {
            self.usage_policy
        }
    }
    pub fn effective_display(&self) -> DisplayPolicy {
        if self.rights_status == RightsStatus::Unknown {
            DisplayPolicy::LinkOnly
        } else {
            self.display_policy
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedSpec {
    pub source_id: String,
    pub url: String,
    pub enabled: bool,
    pub poll_seconds: i32,
    pub max_response_bytes: i32,
}
impl FeedSpec {
    fn validate(&self) -> Result<(), AppError> {
        if valid_id(&self.source_id)
            && registry_url(&self.url)
            && url::Url::parse(&self.url).is_ok_and(|u| u.fragment().is_none())
            && (60..=86400).contains(&self.poll_seconds)
            && (16384..=10485760).contains(&self.max_response_bytes)
        {
            Ok(())
        } else {
            Err(AppError::Validation("Invalid feed configuration."))
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginSpec {
    pub name: String,
    pub source_id: Option<String>,
    pub note: Option<String>,
    pub enabled: bool,
}
impl OriginSpec {
    fn validate(&self) -> Result<(), AppError> {
        if text(&self.name, 300)
            && self.source_id.as_ref().is_none_or(|v| valid_id(v))
            && self.note.as_ref().is_none_or(|v| text(v, 10000))
        {
            Ok(())
        } else {
            Err(AppError::Validation("Invalid reporting origin."))
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Record<T> {
    pub id: String,
    pub version: i64,
    pub spec: T,
}
pub type Source = Record<SourceSpec>;
pub type Feed = Record<FeedSpec>;
pub type Origin = Record<OriginSpec>;
#[derive(Clone, Copy)]
pub enum Entity {
    Source,
    Feed,
    Origin,
}
impl Entity {
    fn table(self) -> &'static str {
        match self {
            Self::Source => "sources",
            Self::Feed => "feeds",
            Self::Origin => "reporting_origins",
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Feed => "feed",
            Self::Origin => "origin",
        }
    }
}
const SOURCE_SELECT: &str = "SELECT s.id,s.lock_version,(to_jsonb(s)-'id'-'lock_version'-'metadata'-'created_at') || jsonb_build_object('origin_ids',ARRAY(SELECT origin_id FROM news.source_origin_links WHERE source_id=s.id ORDER BY origin_id)) AS spec FROM news.sources s";
const FEED_SELECT: &str = "SELECT f.id,f.lock_version,jsonb_build_object('source_id',f.source_id,'url',f.url,'enabled',f.enabled,'poll_seconds',f.poll_seconds,'max_response_bytes',f.max_response_bytes) AS spec FROM news.feeds f";
const ORIGIN_SELECT: &str = "SELECT o.id,o.lock_version,to_jsonb(o)-'id'-'lock_version' AS spec FROM news.reporting_origins o";
#[derive(Clone)]
pub struct Registry {
    pool: PgPool,
}
impl Registry {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn source(&self, session: &Session, id: &str) -> Result<Source, AppError> {
        check_id(id)?;
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let record = source(&mut tx, id).await?;
        commit(tx).await?;
        Ok(record)
    }
    pub async fn feed(&self, session: &Session, id: &str) -> Result<Feed, AppError> {
        check_id(id)?;
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let record = feed(&mut tx, id).await?;
        commit(tx).await?;
        Ok(record)
    }
    pub async fn origin(&self, session: &Session, id: &str) -> Result<Origin, AppError> {
        check_id(id)?;
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let record = origin(&mut tx, id).await?;
        commit(tx).await?;
        Ok(record)
    }
    pub async fn sources(
        &self,
        session: &Session,
        after: Option<&str>,
        size: PageSize,
    ) -> Result<Vec<Source>, AppError> {
        if let Some(id) = after {
            check_id(id)?;
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows = sqlx::query_as::<_, (String, i64, Json<SourceSpec>)>(&format!(
            "{SOURCE_SELECT} WHERE ($1::text IS NULL OR s.id>$1) ORDER BY s.id LIMIT $2"
        ))
        .bind(after)
        .bind(i64::from(size.get()))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let records = rows
            .into_iter()
            .map(record)
            .collect::<Result<Vec<_>, _>>()?;
        commit(tx).await?;
        Ok(records)
    }
    pub async fn feeds(
        &self,
        session: &Session,
        after: Option<&str>,
        size: PageSize,
        active_only: bool,
    ) -> Result<Vec<Feed>, AppError> {
        if let Some(id) = after {
            check_id(id)?;
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as::<_,(String,i64,Json<FeedSpec>)>(&format!("{FEED_SELECT} JOIN news.sources s ON s.id=f.source_id WHERE ($1::text IS NULL OR f.id>$1) AND (NOT $3 OR (f.enabled AND s.enabled)) ORDER BY f.id LIMIT $2")).bind(after).bind(i64::from(size.get())).bind(active_only).fetch_all(&mut *tx).await.map_err(db)?;
        let records = rows
            .into_iter()
            .map(record)
            .collect::<Result<Vec<_>, _>>()?;
        commit(tx).await?;
        Ok(records)
    }
    pub async fn origins(
        &self,
        session: &Session,
        after: Option<&str>,
        size: PageSize,
    ) -> Result<Vec<Origin>, AppError> {
        if let Some(id) = after {
            check_id(id)?;
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows = sqlx::query_as::<_, (String, i64, Json<OriginSpec>)>(&format!(
            "{ORIGIN_SELECT} WHERE ($1::text IS NULL OR o.id>$1) ORDER BY o.id LIMIT $2"
        ))
        .bind(after)
        .bind(i64::from(size.get()))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let records = rows
            .into_iter()
            .map(record)
            .collect::<Result<Vec<_>, _>>()?;
        commit(tx).await?;
        Ok(records)
    }
    /// expected=None creates only; Some(version) updates only. Never overwrite on conflict.
    pub async fn save_source(
        &self,
        session: &Session,
        id: &str,
        expected: Option<i64>,
        spec: SourceSpec,
    ) -> Result<Source, AppError> {
        check_id(id)?;
        spec.validate()?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let before = if let Some(expected) = expected {
            let before = source(&mut tx, id).await?;
            version(before.version, expected)?;
            Some(before)
        } else {
            None
        };
        if before.is_none() {
            sqlx::query("INSERT INTO news.sources(id,name,homepage_url) VALUES ($1,$2,$3)")
                .bind(id)
                .bind(&spec.name)
                .bind(&spec.homepage_url)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("UPDATE news.sources SET name=$2,homepage_url=$3,enabled=$4,ownership_note=$5,usage_policy=$6,terms_url=$7,languages=$8,geographic_coverage=$9,attribution=$10,editorial_note=$11,rights_status=$12,display_policy=$13,rights_note=$14,lock_version=$15 WHERE id=$1")
            .bind(id).bind(&spec.name).bind(&spec.homepage_url).bind(spec.enabled).bind(&spec.ownership_note).bind(spec.usage_policy.as_str()).bind(&spec.terms_url).bind(&spec.languages).bind(&spec.geographic_coverage).bind(&spec.attribution).bind(&spec.editorial_note).bind(spec.rights_status.as_str()).bind(spec.display_policy.as_str()).bind(&spec.rights_note).bind(next(before.as_ref().map(|r|r.version))?).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM news.source_origin_links WHERE source_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for origin in &spec.origin_ids {
            sqlx::query("INSERT INTO news.source_origin_links(source_id,origin_id) VALUES ($1,$2)")
                .bind(id)
                .bind(origin)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let after = source(&mut tx, id).await?;
        audit(&mut tx, Entity::Source, &actor, before.as_ref(), &after).await?;
        commit(tx).await?;
        Ok(after)
    }
    pub async fn save_feed(
        &self,
        session: &Session,
        id: &str,
        expected: Option<i64>,
        spec: FeedSpec,
    ) -> Result<Feed, AppError> {
        check_id(id)?;
        spec.validate()?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let before = if let Some(expected) = expected {
            let before = feed(&mut tx, id).await?;
            version(before.version, expected)?;
            if before.spec.source_id != spec.source_id || before.spec.url != spec.url {
                return Err(AppError::Conflict);
            }
            Some(before)
        } else {
            None
        };
        if before.is_none() {
            sqlx::query("INSERT INTO news.feeds(id,source_id,url,enabled) VALUES ($1,$2,$3,false)")
                .bind(id)
                .bind(&spec.source_id)
                .bind(&spec.url)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("UPDATE news.feeds SET enabled=$2,poll_seconds=$3,max_response_bytes=$4,lock_version=$5 WHERE id=$1").bind(id).bind(spec.enabled).bind(spec.poll_seconds).bind(spec.max_response_bytes).bind(next(before.as_ref().map(|r|r.version))?).execute(&mut *tx).await.map_err(db)?;
        let after = feed(&mut tx, id).await?;
        audit(&mut tx, Entity::Feed, &actor, before.as_ref(), &after).await?;
        commit(tx).await?;
        Ok(after)
    }
    pub async fn save_origin(
        &self,
        session: &Session,
        id: &str,
        expected: Option<i64>,
        spec: OriginSpec,
    ) -> Result<Origin, AppError> {
        check_id(id)?;
        spec.validate()?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let before = if let Some(expected) = expected {
            let before = origin(&mut tx, id).await?;
            version(before.version, expected)?;
            if before.spec.name != spec.name || before.spec.source_id != spec.source_id {
                return Err(AppError::Conflict);
            }
            Some(before)
        } else {
            None
        };
        if before.is_none() {
            sqlx::query("INSERT INTO news.reporting_origins(id,name,source_id) VALUES ($1,$2,$3)")
                .bind(id)
                .bind(&spec.name)
                .bind(&spec.source_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("UPDATE news.reporting_origins SET name=$2,source_id=$3,note=$4,enabled=$5,lock_version=$6 WHERE id=$1").bind(id).bind(spec.name).bind(spec.source_id).bind(spec.note).bind(spec.enabled).bind(next(before.as_ref().map(|r|r.version))?).execute(&mut *tx).await.map_err(db)?;
        let after = origin(&mut tx, id).await?;
        audit(&mut tx, Entity::Origin, &actor, before.as_ref(), &after).await?;
        commit(tx).await?;
        Ok(after)
    }
    /// Always allow safe retirement, including malformed historical configurations.
    pub async fn disable(
        &self,
        session: &Session,
        entity: Entity,
        id: &str,
        expected: i64,
    ) -> Result<i64, AppError> {
        check_id(id)?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        let before = sqlx::query_as::<_, (i64, Value)>(&format!(
            "SELECT lock_version,to_jsonb(r) FROM news.{} r WHERE id=$1 FOR UPDATE",
            entity.table()
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or(AppError::NotFound)?;
        version(before.0, expected)?;
        let updated = next(Some(before.0))?;
        sqlx::query(&format!(
            "UPDATE news.{} SET enabled=false,lock_version=$2 WHERE id=$1",
            entity.table()
        ))
        .bind(id)
        .bind(updated)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let after = sqlx::query_scalar::<_, Value>(&format!(
            "SELECT to_jsonb(r) FROM news.{} r WHERE id=$1",
            entity.table()
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        append_audit(&mut tx, entity, id, updated, &actor, Some(before.1), after).await?;
        commit(tx).await?;
        Ok(updated)
    }
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("source_registry", error)
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}
fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid registry ID."))
    }
}
fn version(actual: i64, expected: i64) -> Result<(), AppError> {
    if actual == expected && expected >= 0 {
        Ok(())
    } else {
        Err(AppError::Conflict)
    }
}
fn next(before: Option<i64>) -> Result<i64, AppError> {
    match before {
        None => Ok(0),
        Some(version) => version.checked_add(1).ok_or(AppError::InvalidData),
    }
}
fn record<T>((id, version, Json(spec)): (String, i64, Json<T>)) -> Result<Record<T>, AppError> {
    if !valid_id(&id) || version < 0 {
        return Err(AppError::InvalidData);
    }
    Ok(Record { id, version, spec })
}
async fn source(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<Source, AppError> {
    record(
        sqlx::query_as::<_, (String, i64, Json<SourceSpec>)>(&format!(
            "{SOURCE_SELECT} WHERE s.id=$1 FOR UPDATE OF s"
        ))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(AppError::NotFound)?,
    )
}
async fn feed(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<Feed, AppError> {
    record(
        sqlx::query_as::<_, (String, i64, Json<FeedSpec>)>(&format!(
            "{FEED_SELECT} WHERE f.id=$1 FOR UPDATE OF f"
        ))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(AppError::NotFound)?,
    )
}
async fn origin(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<Origin, AppError> {
    record(
        sqlx::query_as::<_, (String, i64, Json<OriginSpec>)>(&format!(
            "{ORIGIN_SELECT} WHERE o.id=$1 FOR UPDATE OF o"
        ))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(AppError::NotFound)?,
    )
}
async fn commit(tx: Transaction<'static, Postgres>) -> Result<(), AppError> {
    tx.commit().await.map_err(db)
}
async fn audit<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    entity: Entity,
    actor: &Principal,
    before: Option<&Record<T>>,
    after: &Record<T>,
) -> Result<(), AppError> {
    append_audit(
        tx,
        entity,
        &after.id,
        after.version,
        actor,
        before.map(|r| json!(r)),
        json!(after),
    )
    .await
}
async fn append_audit(
    tx: &mut Transaction<'_, Postgres>,
    entity: Entity,
    id: &str,
    version: i64,
    actor: &Principal,
    before: Option<Value>,
    after: Value,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.registry_actions(entity_type,entity_id,lock_version,actor_id,before_record,after_record) VALUES ($1,$2,$3,$4,$5,$6)").bind(entity.as_str()).bind(id).bind(version).bind(actor.id()).bind(before.map(Json)).bind(Json(after)).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
/// Syntax/configuration validation, not fetch-time DNS/redirect/SSRF enforcement.
pub fn registry_url(value: &str) -> bool {
    if value.len() > 2048 || !safe_source_url(value) {
        return false;
    }
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    let Some(url::Host::Domain(host)) = url.host() else {
        return false;
    };
    let host = host.trim_end_matches('.');
    if !host.contains('.')
        || ["localhost", "local", "internal", "lan", "home"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return false;
    }
    crate::content::public_url_parameters(&url)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_urls_reject_secrets_and_local_targets_without_doing_dns() {
        for value in [
            "https://user:password@source.invalid/rss",
            "https://source.invalid/rss?access_token=private",
            "https://source.invalid/rss?auth=private",
            "https://source.invalid/rss?rss_key=private",
            "http://127.0.0.1/feed",
            "http://[::1]/rss",
            "http://service.internal/rss",
            "file:///etc/passwd",
        ] {
            assert!(!registry_url(value));
        }
        assert!(registry_url(
            "https://source.invalid/rss?lang=fr&format=xml"
        ));
        assert!(registry_url("https://source.invalid/terms#reuse"));
    }
}
