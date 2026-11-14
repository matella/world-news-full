//! Session-authorized private intake. Extracted evidence is never automatically a draft.
use super::{
    article::{ArticleClient, Extracted, ExtractionError},
    http::{FetchError, SafeHttp},
    normalize::canonical_url,
};
use crate::{
    auth::{self, Permission, Principal, Session},
    content::{PageSize, valid_id, valid_language},
    editorial::{self, RevisionInput},
    error::AppError,
    registry::{SOURCE_SELECT, SourceSpec, StoragePolicy},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use url::Url;

pub type ExtractFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Extracted, ExtractionError>> + Send + 'a>>;
/// Trusted policy-aware adapters only. Normal construction uses shared SafeHttp.
pub trait ArticleTransport: Send + Sync {
    fn extract<'a>(&'a self, url: &'a str) -> ExtractFuture<'a>;
}
impl ArticleTransport for ArticleClient {
    fn extract<'a>(&'a self, url: &'a str) -> ExtractFuture<'a> {
        Box::pin(ArticleClient::extract(self, url))
    }
}
#[derive(Clone)]
pub struct IntakeService {
    pool: PgPool,
    transport: Arc<dyn ArticleTransport>,
}
pub struct ManualLink {
    pub source_id: String,
    pub url: String,
    pub title: String,
    pub language: String,
    pub notes: String,
}
pub struct OriginalDraft {
    pub id: String,
    pub input: RevisionInput,
}
#[derive(Debug, sqlx::FromRow)]
pub struct IntakeHead {
    pub id: String,
    pub intake_version: i64,
    pub extraction_state: String,
    pub current_intake_id: Option<i64>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct IntakeRecord {
    pub id: i64,
    pub version: i64,
    pub kind: String,
    pub error_category: Option<String>,
    pub actor_id: Option<String>,
    pub worker_job_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct Snapshot {
    pub record: IntakeRecord,
    pub notes: Option<String>,
    pub extracted: Option<Extracted>,
    pub text_withheld: bool,
}
pub(crate) struct Context {
    pub(crate) source_id: String,
    pub(crate) source_version: i64,
    spec: SourceSpec,
    pub(crate) url: String,
    pub(crate) version: i64,
}
impl Context {
    fn storage_allowed(&self) -> bool {
        self.spec.validate().is_ok() && self.spec.effective_storage() != StoragePolicy::MetadataOnly
    }
    pub(crate) fn may_fetch(&self) -> bool {
        self.spec.enabled
            && self.storage_allowed()
            && crate::registry::registry_url(&self.url)
            && Url::parse(&self.url)
                .ok()
                .zip(Url::parse(&self.spec.homepage_url).ok())
                .is_some_and(|(a, b)| a.host_str() == b.host_str())
    }
    fn policy(&self) -> Value {
        json!({"rights_status":self.spec.rights_status,"storage":self.spec.effective_storage(),"display":self.spec.effective_display(),"terms_url":self.spec.terms_url,"rights_note":self.spec.rights_note,"source_version":self.source_version,"input_trust":"untrusted_data","extractor":"plain-article-v1"})
    }
}
impl IntakeService {
    pub fn new(pool: PgPool, public_url: &Url) -> Result<Self, FetchError> {
        Ok(Self {
            pool,
            transport: Arc::new(ArticleClient::new(SafeHttp::new(public_url)?)),
        })
    }
    pub fn with_transport(pool: PgPool, transport: Arc<dyn ArticleTransport>) -> Self {
        Self { pool, transport }
    }
    pub async fn head(&self, session: &Session, id: &str) -> Result<IntakeHead, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        check_id(id)?;
        let head = load_head(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(head)
    }
    pub async fn extract(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
    ) -> Result<IntakeHead, AppError> {
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let context = context(&mut tx, id, Some(expected)).await?;
        let rights_allowed = context.may_fetch();
        if !rights_allowed || !retry_allowed(&mut tx, &context.source_id).await? {
            save(
                &mut tx,
                &actor,
                id,
                &context,
                "denied",
                Some(if rights_allowed {
                    "source_retry_wait"
                } else {
                    "rights_or_scope_denied"
                }),
                None,
                None,
            )
            .await?;
            let head = load_head(&mut tx, id).await?;
            tx.commit().await.map_err(db)?;
            return Ok(head);
        }
        tx.commit().await.map_err(db)?; // Never retain identity/config/row locks over HTTP.
        let mut extracted = tokio::time::timeout(
            Duration::from_secs(20),
            self.transport.extract(&context.url),
        )
        .await
        .unwrap_or(Err(ExtractionError::Timeout));
        if let Ok(data) = &extracted
            && (!data.valid()
                || Url::parse(&data.final_url).ok().map(|u| u.origin())
                    != Url::parse(&context.url).ok().map(|u| u.origin()))
        {
            extracted = Err(ExtractionError::UnsupportedDocument);
        }
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let current = context_for_completion(&mut tx, id, expected, &context).await?;
        let hint = match &extracted {
            Err(ExtractionError::Fetch(FetchError::HttpStatus(_, hint))) => *hint,
            _ => None,
        };
        let (kind, error, data) = match extracted {
            Ok(data) => ("extracted", None, Some(data)),
            Err(e) => ("failed", Some(e.category()), None),
        };
        let snapshot = save(
            &mut tx,
            &actor,
            id,
            &current,
            kind,
            error,
            data.as_ref(),
            None,
        )
        .await?;
        if let Some(seconds) = hint {
            record_retry(&mut tx, &current.source_id, None, Some(snapshot), seconds).await?;
        }
        let head = load_head(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(head)
    }
    /// Editor-authored notes and a separately authored revision; no implicit copy
    /// from extraction. Draft creation, link, note and intake version are atomic.
    pub async fn manual(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        notes: &str,
        draft: Option<OriginalDraft>,
    ) -> Result<IntakeHead, AppError> {
        if !valid_notes(notes) {
            return Err(AppError::Validation("Invalid manual notes."));
        }
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let context = context(&mut tx, id, Some(expected)).await?;
        let snapshot = save(
            &mut tx,
            &actor,
            id,
            &context,
            "manual",
            None,
            None,
            Some(notes),
        )
        .await?;
        if let Some(draft) = draft {
            let head = editorial::create_in_transaction(
                &mut tx,
                &actor,
                &draft.id,
                draft.input,
                &crate::domain::EditorialPolicy::repository(),
            )
            .await?;
            sqlx::query("INSERT INTO news.intake_draft_links(snapshot_id,story_id,initial_revision_id) VALUES($1,$2,$3)").bind(snapshot).bind(head.id).bind(head.current_revision_id).execute(&mut *tx).await.map_err(db)?;
        }
        let head = load_head(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(head)
    }
    pub async fn register_link(
        &self,
        session: &Session,
        input: ManualLink,
    ) -> Result<IntakeHead, AppError> {
        check_id(&input.source_id)?;
        let canonical =
            canonical_url(&input.url).ok_or(AppError::Validation("Invalid source link."))?;
        if input.title.trim().is_empty()
            || input.title.chars().count() > 500
            || input.title.chars().any(char::is_control)
            || !valid_language(&input.language)
            || !valid_notes(&input.notes)
        {
            return Err(AppError::Validation("Invalid manual link metadata."));
        }
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let (source_version, spec) = source(&mut tx, &input.source_id).await?;
        let digest = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        let key = format!("url-v1:{digest}");
        let id = format!("feed-url-v1-{digest}");
        sqlx::query("SELECT pg_advisory_xact_lock(781642901)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM news.source_articles WHERE canonical_key=$1 OR original_url=$2 OR original_url=$3)").bind(&key).bind(&input.url).bind(&canonical).fetch_one(&mut *tx).await.map_err(db)? {return Err(AppError::Conflict);}
        let metadata = json!({"normalizer_version":1,"canonical_url":canonical,"input_trust":"untrusted_data","origin":"manual_link_metadata","fetched_at_semantics":"metadata_ingestion"});
        sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,language,usage_policy,extraction_state,metadata) VALUES($1,$2,$3,$4,$5,$6,'metadata_only','metadata_only',$7)").bind(&id).bind(&input.source_id).bind(key).bind(&input.url).bind(input.title.trim()).bind(&input.language).bind(Json(metadata)).execute(&mut *tx).await.map_err(db)?;
        let ctx = Context {
            source_id: input.source_id,
            source_version,
            spec,
            url: input.url,
            version: 0,
        };
        save(
            &mut tx,
            &actor,
            &id,
            &ctx,
            "registered_link",
            None,
            None,
            Some(&input.notes),
        )
        .await?;
        let head = load_head(&mut tx, &id).await?;
        tx.commit().await.map_err(db)?;
        Ok(head)
    }
    pub async fn history(
        &self,
        session: &Session,
        id: &str,
        before: Option<i64>,
        limit: PageSize,
    ) -> Result<Vec<IntakeRecord>, AppError> {
        check_id(id)?;
        if before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid history cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        load_head(&mut tx, id).await?;
        let rows=sqlx::query_as::<_,IntakeRecord>("SELECT id,version,kind,error_category,actor_id,worker_job_id,created_at FROM news.intake_snapshots WHERE source_article_id=$1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT $3").bind(id).bind(before).bind(i64::from(limit.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn snapshot(
        &self,
        session: &Session,
        id: &str,
        snapshot: i64,
    ) -> Result<Snapshot, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let ctx = context(&mut tx, id, None).await?;
        let record=sqlx::query_as::<_,IntakeRecord>("SELECT id,version,kind,error_category,actor_id,worker_job_id,created_at FROM news.intake_snapshots WHERE source_article_id=$1 AND id=$2").bind(id).bind(snapshot).fetch_one(&mut *tx).await.map_err(db)?;
        // Do not even select retained publisher text when current rights deny it.
        let (notes,extracted)=sqlx::query_as::<_,(Option<String>,Option<Json<Extracted>>)>("SELECT i.notes,CASE WHEN $3 THEN t.extracted ELSE NULL END FROM news.intake_snapshots i LEFT JOIN news.intake_text t ON t.snapshot_id=i.id WHERE i.source_article_id=$1 AND i.id=$2").bind(id).bind(snapshot).bind(ctx.storage_allowed()).fetch_one(&mut *tx).await.map_err(db)?;
        let text_withheld = record.kind == "extracted" && extracted.is_none();
        tx.commit().await.map_err(db)?;
        Ok(Snapshot {
            record,
            notes,
            extracted: extracted.map(|v| v.0),
            text_withheld,
        })
    }
}
fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid article/source ID."))
    }
}
fn valid_notes(notes: &str) -> bool {
    !notes.trim().is_empty() && notes.chars().count() <= 20000 && !notes.contains('\0')
}
async fn load_head(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<IntakeHead, AppError> {
    sqlx::query_as("SELECT id,intake_version,extraction_state,current_intake_id FROM news.source_articles WHERE id=$1").bind(id).fetch_one(&mut **tx).await.map_err(db)
}
async fn source(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
) -> Result<(i64, SourceSpec), AppError> {
    let (_, version) = sqlx::query_as::<_, (bool, i64)>(
        "SELECT enabled,lock_version FROM news.lock_source_for_fetch($1)",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    let (_, stored_version, Json(spec)) = sqlx::query_as::<_, (String, i64, Json<SourceSpec>)>(
        &format!("{SOURCE_SELECT} WHERE s.id=$1"),
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    if stored_version != version {
        return Err(AppError::Conflict);
    }
    Ok((version, spec))
}
pub(crate) async fn context(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    expected: Option<i64>,
) -> Result<Context, AppError> {
    check_id(id)?;
    if expected.is_some_and(|v| v < 0) {
        return Err(AppError::Validation("Invalid intake version."));
    }
    let source_id =
        sqlx::query_scalar::<_, String>("SELECT source_id FROM news.source_articles WHERE id=$1")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
    let (source_version, spec) = source(tx, &source_id).await?;
    let (url, version) = sqlx::query_as::<_, (String, i64)>(
        "SELECT original_url,intake_version FROM news.source_articles WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    if expected.is_some_and(|v| v != version) {
        return Err(AppError::Conflict);
    }
    Ok(Context {
        source_id,
        source_version,
        spec,
        url,
        version,
    })
}
async fn context_for_completion(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    expected: i64,
    old: &Context,
) -> Result<Context, AppError> {
    let current = context(tx, id, Some(expected)).await?;
    if current.source_id != old.source_id
        || current.source_version != old.source_version
        || current.url != old.url
        || !current.may_fetch()
    {
        return Err(AppError::Conflict);
    }
    Ok(current)
}
#[allow(clippy::too_many_arguments)]
async fn save(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Principal,
    id: &str,
    ctx: &Context,
    kind: &str,
    error: Option<&str>,
    extracted: Option<&Extracted>,
    notes: Option<&str>,
) -> Result<i64, AppError> {
    save_intake(
        tx,
        Some(actor.id()),
        None,
        id,
        ctx,
        kind,
        error,
        extracted,
        notes,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_worker(
    tx: &mut Transaction<'_, Postgres>,
    job: i64,
    id: &str,
    ctx: &Context,
    kind: &str,
    error: Option<&str>,
    extracted: Option<&Extracted>,
) -> Result<i64, AppError> {
    save_intake(tx, None, Some(job), id, ctx, kind, error, extracted, None).await
}
pub(crate) async fn retry_allowed(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
) -> Result<bool, AppError> {
    sqlx::query_scalar("SELECT news.extraction_retry_allowed($1)")
        .bind(source)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)
}
#[allow(clippy::too_many_arguments)]
async fn save_intake(
    tx: &mut Transaction<'_, Postgres>,
    actor: Option<&str>,
    worker_job: Option<i64>,
    id: &str,
    ctx: &Context,
    kind: &str,
    error: Option<&str>,
    extracted: Option<&Extracted>,
    notes: Option<&str>,
) -> Result<i64, AppError> {
    let version = ctx.version.checked_add(1).ok_or(AppError::Conflict)?;
    let extraction_metadata = extracted.map(|data| {
        let mut value = serde_json::to_value(data).expect("serializable extraction");
        let digest = format!("{:x}", Sha256::digest(value.to_string().as_bytes()));
        value
            .as_object_mut()
            .expect("extraction object")
            .remove("paragraphs");
        value["paragraph_count"] = json!(data.paragraphs.len());
        value["text_hash"] = json!(digest);
        Json(value)
    });
    let snapshot = if let Some(job) = worker_job {
        sqlx::query_scalar::<_,i64>("INSERT INTO news.intake_snapshots(source_article_id,version,source_version,worker_job_id,kind,error_category,policy,extraction_metadata) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id").bind(id).bind(version).bind(ctx.source_version).bind(job).bind(kind).bind(error).bind(Json(ctx.policy())).bind(extraction_metadata).fetch_one(&mut **tx).await.map_err(db)?
    } else {
        sqlx::query_scalar::<_,i64>("INSERT INTO news.intake_snapshots(source_article_id,version,source_version,actor_id,kind,error_category,policy,extraction_metadata,notes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id").bind(id).bind(version).bind(ctx.source_version).bind(actor).bind(kind).bind(error).bind(Json(ctx.policy())).bind(extraction_metadata).bind(notes).fetch_one(&mut **tx).await.map_err(db)?
    };
    if let Some(data) = extracted {
        sqlx::query("INSERT INTO news.intake_text(snapshot_id,extracted) VALUES($1,$2)")
            .bind(snapshot)
            .bind(Json(data))
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    }
    let changed=sqlx::query("UPDATE news.source_articles SET intake_version=$2,current_intake_id=$3,extraction_state=CASE WHEN $4='extracted' THEN 'ready' WHEN $4='failed' THEN 'failed' WHEN $4='denied' THEN 'metadata_only' ELSE extraction_state END WHERE id=$1 AND intake_version=$5").bind(id).bind(version).bind(snapshot).bind(kind).bind(ctx.version).execute(&mut **tx).await.map_err(db)?;
    if changed.rows_affected() != 1 {
        return Err(AppError::Conflict);
    }
    Ok(snapshot)
}
pub(crate) async fn record_retry(
    tx: &mut Transaction<'_, Postgres>,
    source: &str,
    job: Option<i64>,
    manual_snapshot: Option<i64>,
    seconds: u64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.source_extraction_limits(source_id,retry_after_seconds,job_id,manual_snapshot_id) VALUES($1,$2,$3,$4) ON CONFLICT(source_id) DO UPDATE SET retry_after_seconds=EXCLUDED.retry_after_seconds,observed_at=EXCLUDED.observed_at,job_id=EXCLUDED.job_id,manual_snapshot_id=EXCLUDED.manual_snapshot_id WHERE news.source_extraction_limits.retry_after_seconds::numeric-extract(epoch FROM clock_timestamp()-news.source_extraction_limits.observed_at)<EXCLUDED.retry_after_seconds::numeric").bind(source).bind(seconds.to_string()).bind(job).bind(manual_snapshot).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
fn db(error: sqlx::Error) -> AppError {
    AppError::database("article_intake", error)
}
