//! Human article-specific reporting-origin evidence; registry hints are not proof.
use crate::{
    auth::{self, Permission, Session},
    content::{PageSize, valid_id},
    error::AppError,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, types::Json};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginState {
    Unknown,
    Documented,
    Conflicting,
}
impl OriginState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Documented => "documented",
            Self::Conflicting => "conflicting",
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OriginInput {
    pub state: OriginState,
    pub origin_ids: Vec<String>,
    pub public_basis: String,
    pub private_note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OriginLabel {
    pub id: String,
    pub name: String,
}
#[derive(Debug, sqlx::FromRow)]
pub struct Evidence {
    pub id: i64,
    pub source_article_id: String,
    pub version: i64,
    pub source_version: i64,
    pub state: String,
    pub public_basis: String,
    pub actor_id: String,
    pub created_at: DateTime<Utc>,
    pub origins: Json<Vec<OriginLabel>>,
}
pub struct PrivateEvidence {
    pub evidence: Evidence,
    pub private_note: String,
}
#[derive(Clone)]
pub struct OriginEvidence {
    pool: PgPool,
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}
impl OriginInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.origin_ids.len() > 20
            || self.origin_ids.iter().any(|id| !valid_id(id))
            || self.origin_ids.iter().collect::<BTreeSet<_>>().len() != self.origin_ids.len()
            || (self.state == OriginState::Documented && self.origin_ids.is_empty())
            || (self.state == OriginState::Unknown && !self.origin_ids.is_empty())
            || !text(&self.public_basis, 2000)
            || !text(&self.private_note, 10000)
        {
            return Err(AppError::Validation("Invalid article origin evidence."));
        }
        Ok(())
    }
}
fn db(e: sqlx::Error) -> AppError {
    AppError::database("article_origin_evidence", e)
}
impl OriginEvidence {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn assert(
        &self,
        session: &Session,
        article: &str,
        expected: i64,
        input: OriginInput,
    ) -> Result<Evidence, AppError> {
        if !valid_id(article) || expected < 0 {
            return Err(AppError::Validation("Invalid article origin version."));
        }
        input.validate()?;
        let (mut tx, actor) =
            auth::begin_authorised(&self.pool, session, Permission::Publish).await?;
        sqlx::query("SET LOCAL lock_timeout='1s'")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("SET LOCAL statement_timeout='5s'")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let source: String =
            sqlx::query_scalar("SELECT source_id FROM news.source_articles WHERE id=$1")
                .bind(article)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or(AppError::NotFound)?;
        // Shared registry lock order: source -> selected origins -> article.
        // Disabled sources can still have historical attribution corrected; no fetch.
        let source_version: i64 =
            sqlx::query_scalar("SELECT lock_version FROM news.lock_source_for_fetch($1)")
                .bind(&source)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let rows: Vec<(String, String, bool)> = sqlx::query_as(
            "SELECT id,name,enabled FROM news.lock_reporting_origins_for_evidence($1)",
        )
        .bind(&input.origin_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        if rows.len() != input.origin_ids.len() {
            return Err(AppError::NotFound);
        }
        if rows.iter().any(|r| !r.2) {
            return Err(AppError::Validation(
                "Disabled origins cannot support new assertions.",
            ));
        }
        let version: i64 = sqlx::query_scalar(
            "SELECT origin_version FROM news.source_articles WHERE id=$1 FOR UPDATE",
        )
        .bind(article)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if version != expected {
            return Err(AppError::Conflict);
        }
        let version = version.checked_add(1).ok_or(AppError::Conflict)?;
        let id:i64=sqlx::query_scalar("INSERT INTO news.article_origin_assertions(source_article_id,version,source_version,state,public_basis,private_note,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
            .bind(article).bind(version).bind(source_version).bind(input.state.as_str()).bind(&input.public_basis).bind(&input.private_note).bind(actor.id()).fetch_one(&mut *tx).await.map_err(db)?;
        for (id_value, name, _) in &rows {
            sqlx::query("INSERT INTO news.article_origin_labels(assertion_id,origin_id,captured_name) VALUES($1,$2,$3)").bind(id).bind(id_value).bind(name).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE news.source_articles SET origin_version=$2,current_origin_assertion_id=$3 WHERE id=$1").bind(article).bind(version).bind(id).execute(&mut *tx).await.map_err(db)?;
        let result = read_one(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(result)
    }
    pub async fn details(&self, session: &Session, id: i64) -> Result<PrivateEvidence, AppError> {
        if id <= 0 {
            return Err(AppError::Validation("Invalid origin evidence ID."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let evidence = read_one(&mut tx, id).await?;
        let private_note = sqlx::query_scalar(
            "SELECT private_note FROM news.article_origin_assertions WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(PrivateEvidence {
            evidence,
            private_note,
        })
    }
    pub async fn history(
        &self,
        session: &Session,
        article: &str,
        before: Option<i64>,
        size: PageSize,
    ) -> Result<Vec<Evidence>, AppError> {
        if !valid_id(article) || before.is_some_and(|v| v <= 0) {
            return Err(AppError::Validation("Invalid origin evidence cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as(&format!("{SELECT_EVIDENCE} WHERE a.source_article_id=$1 AND ($2::bigint IS NULL OR a.id<$2) ORDER BY a.id DESC LIMIT $3"))
            .bind(article).bind(before).bind(i64::from(size.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
}
const SELECT_EVIDENCE: &str = "SELECT a.id,a.source_article_id,a.version,a.source_version,a.state,a.public_basis,a.actor_id,a.created_at,(SELECT COALESCE(jsonb_agg(jsonb_build_object('id',l.origin_id,'name',l.captured_name) ORDER BY l.origin_id),'[]'::jsonb) FROM news.article_origin_labels l WHERE l.assertion_id=a.id) AS origins FROM news.article_origin_assertions a";
async fn read_one(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
) -> Result<Evidence, AppError> {
    sqlx::query_as(&format!("{SELECT_EVIDENCE} WHERE a.id=$1"))
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(db)
}
