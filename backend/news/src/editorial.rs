//! Explicit, session-authorised editorial transactions. No automatic publishing.
use crate::{
    auth::{self, Permission, Principal, Session},
    content::*,
    error::AppError,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction, types::Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicState {
    Draft,
    InReview,
    Published,
    Withdrawn,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Draft,
    InReview,
    Approved,
    ChangesRequested,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewOutcome {
    Approved,
    ChangesRequested,
}
impl ReviewOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
        }
    }
}
#[derive(Debug)]
pub struct Head {
    pub id: String,
    pub public_state: PublicState,
    pub review_state: ReviewState,
    pub current_revision_id: String,
    pub published_revision_id: Option<String>,
    pub lock_version: i64,
}
#[derive(sqlx::FromRow)]
struct RawHead {
    id: String,
    state: String,
    review_state: String,
    current_revision_id: Option<String>,
    published_revision_id: Option<String>,
    lock_version: i64,
}
impl TryFrom<RawHead> for Head {
    type Error = AppError;
    fn try_from(raw: RawHead) -> Result<Self, AppError> {
        let parse = |value: String| serde_json::Value::String(value);
        let public_state: PublicState =
            serde_json::from_value(parse(raw.state)).map_err(|_| AppError::InvalidData)?;
        let review_state =
            serde_json::from_value(parse(raw.review_state)).map_err(|_| AppError::InvalidData)?;
        let current_revision_id = raw
            .current_revision_id
            .filter(|id| valid_id(id))
            .ok_or(AppError::InvalidData)?;
        if !valid_id(&raw.id)
            || raw.lock_version < 0
            || raw
                .published_revision_id
                .as_ref()
                .is_some_and(|id| !valid_id(id))
            || matches!(
                public_state,
                PublicState::Published | PublicState::Withdrawn
            ) != raw.published_revision_id.is_some()
        {
            return Err(AppError::InvalidData);
        }
        Ok(Self {
            id: raw.id,
            public_state,
            review_state,
            current_revision_id,
            published_revision_id: raw.published_revision_id,
            lock_version: raw.lock_version,
        })
    }
}
#[derive(Clone)]
pub struct RevisionInput {
    pub title: String,
    pub standfirst: Option<String>,
    pub summary: Option<String>,
    pub body: Vec<ContentBlock>,
    pub language: String,
    pub format: ArticleFormat,
    pub kind: ArticleKind,
    pub topics: Vec<String>,
    pub geography: Vec<String>,
    pub reason: String,
    pub evidence: Option<crate::sourcing::SourcePackage>,
    pub length_override: Option<String>,
}
impl RevisionInput {
    fn validate(&self) -> Result<(), AppError> {
        self.validate_content(false)
    }
    fn validate_content(&self, historical: bool) -> Result<(), AppError> {
        let metadata =
            |values: &[String]| values.len() <= 20 && values.iter().all(|v| text(v, 100));
        if !text(&self.title, 500)
            || self.title.chars().any(char::is_control)
            || !valid_language(&self.language)
            || !validate_blocks(&self.body)
            || self.standfirst.as_ref().is_some_and(|s| !text(s, 1000))
            || self.summary.as_ref().is_some_and(|s| !text(s, 3000))
            || !metadata(&self.topics)
            || !metadata(&self.geography)
            || !text(&self.reason, 2000)
            || (!historical
                && matches!(
                    self.kind,
                    ArticleKind::LegacySummary | ArticleKind::LegacySynthesis
                ))
        {
            return Err(AppError::Validation(
                "Invalid revision content or change reason.",
            ));
        }
        if let Some(package) = &self.evidence {
            package.validate(&self.body, self.kind)?;
        }
        Ok(())
    }
}
#[derive(Debug, sqlx::FromRow)]
pub struct RevisionRecord {
    pub id: String,
    pub number: i32,
    pub actor_id: String,
    pub title: String,
    pub change_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone)]
pub struct EditorialService {
    pool: PgPool,
    policy: std::sync::Arc<crate::domain::EditorialPolicy>,
}
impl EditorialService {
    pub fn new(pool: PgPool) -> Self {
        Self::with_policy(
            pool,
            std::sync::Arc::new(crate::domain::EditorialPolicy::repository()),
        )
    }
    pub fn with_policy(
        pool: PgPool,
        policy: std::sync::Arc<crate::domain::EditorialPolicy>,
    ) -> Self {
        Self { pool, policy }
    }
    async fn begin(
        &self,
        session: &Session,
        permission: Permission,
    ) -> Result<(Transaction<'static, Postgres>, Principal), AppError> {
        auth::begin_authorised(&self.pool, session, permission).await
    }

    pub async fn create(
        &self,
        session: &Session,
        id: &str,
        input: RevisionInput,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        input.validate()?;
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = create_in_transaction(&mut tx, &actor, id, input, &self.policy).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("editorial_commit", e))?;
        Ok(head)
    }
    pub async fn revise(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        input: RevisionInput,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        input.validate_content(true)?;
        let reason = input.reason.clone();
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn {
            return Err(AppError::Conflict);
        }
        let old = load_revision(&mut tx, &head.current_revision_id).await?;
        let mut provenance: serde_json::Value =
            sqlx::query_scalar("SELECT provenance FROM news.story_revisions WHERE id=$1")
                .bind(&old.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppError::database("revision_lineage", e))?;
        let historical = matches!(
            old.kind,
            ArticleKind::LegacySummary | ArticleKind::LegacySynthesis
        );
        if (historical && (input.kind != old.kind || provenance["pipeline"] != "legacy_jarvis"))
            || (!historical
                && matches!(
                    input.kind,
                    ArticleKind::LegacySummary | ArticleKind::LegacySynthesis
                ))
        {
            return Err(AppError::Validation(
                "Historical edits must preserve genuine archive lineage and kind.",
            ));
        }
        if input.topics != old.topics || input.geography != old.geography {
            provenance
                .as_object_mut()
                .ok_or(AppError::InvalidData)?
                .remove("classification");
        }
        provenance["edited_from"] = serde_json::json!(old.id);
        let revision =
            insert_revision(&mut tx, id, &actor, input, &self.policy, Some(provenance)).await?;
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,review_state='draft',state=CASE WHEN published_revision_id IS NULL THEN 'draft' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).bind(&revision).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_revise",e))?;
        finish(tx, &head, &revision, &actor, "revised", &reason).await
    }
    /// Classification-only edit: retain content, citations and original provenance.
    /// Review is invalidated and the published revision remains unchanged.
    pub async fn reclassify(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        classification: crate::taxonomy::Classification,
        reason: &str,
        policy: &crate::taxonomy::Taxonomy,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        if !policy.validate(&classification) || !text(reason, 2000) {
            return Err(AppError::Validation("Invalid classification or reason."));
        }
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn {
            return Err(AppError::Conflict);
        }
        let old = load_revision(&mut tx, &head.current_revision_id).await?;
        if old.topics == classification.topics && old.geography == classification.geography {
            return Err(AppError::Validation("Classification is unchanged."));
        }
        let provenance = serde_json::json!({"basis":"editor_override","policy_hash":policy.fingerprint(),"policy_snapshot":policy,
            "previous_revision":old.id,"section":policy.section(&classification).0});
        let revision: String=sqlx::query_scalar("INSERT INTO news.story_revisions(story_id,number,title,standfirst,summary,body,language,format,kind,topics,geography,actor_id,change_reason,provenance) SELECT story_id,number+1,title,standfirst,summary,body,language,format,kind,$3,$4,$5,$6,provenance || jsonb_build_object('classification',$7::jsonb) FROM news.story_revisions WHERE story_id=$1 AND id=$2 RETURNING id")
            .bind(id).bind(&old.id).bind(&classification.topics).bind(&classification.geography)
            .bind(actor.id()).bind(reason).bind(Json(provenance)).fetch_one(&mut *tx).await
            .map_err(|e|AppError::database("classification_revision",e))?;
        sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,location,anchor_id,origin_version) SELECT $1,claim_id,source_article_id,location,anchor_id,origin_version FROM news.citations WHERE revision_id=$2")
            .bind(&revision).bind(&old.id).execute(&mut *tx).await.map_err(|e|AppError::database("classification_citations",e))?;
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,review_state='draft',state=CASE WHEN published_revision_id IS NULL THEN 'draft' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).bind(&revision).execute(&mut *tx).await.map_err(|e|AppError::database("classification_head",e))?;
        finish(tx, &head, &revision, &actor, "revised", reason).await
    }

    /// Source-only changes retain authored prose/provenance and create a fresh review candidate.
    pub async fn source_revision(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        package: crate::sourcing::SourcePackage,
        reason: &str,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        check_reason(reason)?;
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn {
            return Err(AppError::Conflict);
        }
        let old = load_revision(&mut tx, &head.current_revision_id).await?;
        package.validate(&old.body, old.kind)?;
        let length = crate::length::private_assessment(&mut tx, &old.id)
            .await?
            .unwrap_or(crate::length::Assessment::new(
                &old.body,
                &old.language,
                old.format,
                &self.policy,
                None,
            )?);
        let sourcing = serde_json::json!({"schema":1,"package":package});
        let revision:String=sqlx::query_scalar("INSERT INTO news.story_revisions(story_id,number,title,standfirst,summary,body,language,format,kind,topics,geography,actor_id,change_reason,provenance) SELECT story_id,number+1,title,standfirst,summary,body,language,format,kind,topics,geography,$3,$4,provenance || jsonb_build_object('sourcing',$5::jsonb,'sourcing_previous_revision',id,'length',$6::jsonb) FROM news.story_revisions WHERE story_id=$1 AND id=$2 RETURNING id")
            .bind(id).bind(&old.id).bind(actor.id()).bind(reason).bind(Json(sourcing)).bind(Json(length)).fetch_one(&mut *tx).await.map_err(|e|AppError::database("sourcing_revision",e))?;
        crate::sourcing::attach(&mut tx, &revision, &package).await?;
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,review_state='draft',state=CASE WHEN published_revision_id IS NULL THEN 'draft' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).bind(&revision).execute(&mut *tx).await.map_err(|e|AppError::database("sourcing_head",e))?;
        finish(tx, &head, &revision, &actor, "revised", reason).await
    }
    pub async fn length_revision(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        override_reason: Option<String>,
        reason: &str,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        check_reason(reason)?;
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn {
            return Err(AppError::Conflict);
        }
        let old = load_revision(&mut tx, &head.current_revision_id).await?;
        let length = crate::length::Assessment::new(
            &old.body,
            &old.language,
            old.format,
            &self.policy,
            override_reason,
        )?;
        let revision:String=sqlx::query_scalar("INSERT INTO news.story_revisions(story_id,number,title,standfirst,summary,body,language,format,kind,topics,geography,actor_id,change_reason,provenance) SELECT story_id,number+1,title,standfirst,summary,body,language,format,kind,topics,geography,$3,$4,provenance || jsonb_build_object('length',$5::jsonb,'length_previous_revision',id) FROM news.story_revisions WHERE story_id=$1 AND id=$2 RETURNING id")
            .bind(id).bind(&old.id).bind(actor.id()).bind(reason).bind(Json(length)).fetch_one(&mut *tx).await.map_err(|e|AppError::database("length_revision",e))?;
        sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,location,anchor_id,origin_version) SELECT $1,claim_id,source_article_id,location,anchor_id,origin_version FROM news.citations WHERE revision_id=$2")
            .bind(&revision).bind(&old.id).execute(&mut *tx).await.map_err(|e|AppError::database("length_citations",e))?;
        sqlx::query("UPDATE news.stories SET current_revision_id=$2,review_state='draft',state=CASE WHEN published_revision_id IS NULL THEN 'draft' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).bind(&revision).execute(&mut *tx).await.map_err(|e|AppError::database("length_head",e))?;
        finish(tx, &head, &revision, &actor, "revised", reason).await
    }
    pub async fn length_report(
        &self,
        session: &Session,
        revision: &str,
    ) -> Result<crate::length::Report, AppError> {
        check_id(revision)?;
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let value = load_revision(&mut tx, revision).await?;
        let recorded = crate::length::private_assessment(&mut tx, revision).await?;
        let report = crate::length::Report::new(&value, &self.policy, recorded)?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("length_report", e))?;
        Ok(report)
    }
    pub async fn submit(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        let (mut tx, actor) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn
            || !matches!(
                head.review_state,
                ReviewState::Draft | ReviewState::ChangesRequested
            )
            || head.published_revision_id.as_deref() == Some(&head.current_revision_id)
        {
            return Err(AppError::Conflict);
        }
        let revision = load_revision(&mut tx, &head.current_revision_id).await?;
        if !revision.body.iter().any(ContentBlock::has_prose) {
            return Err(AppError::Validation(
                "Review requires nonempty article prose.",
            ));
        }
        sqlx::query("UPDATE news.stories SET review_state='in_review',state=CASE WHEN published_revision_id IS NULL THEN 'in_review' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_submit",e))?;
        finish(
            tx,
            &head,
            &head.current_revision_id,
            &actor,
            "submitted",
            "Submitted for editorial review.",
        )
        .await
    }
    pub async fn review(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        outcome: ReviewOutcome,
        note: Option<&str>,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        if note.is_some_and(|v| !text(v, 10000))
            || (outcome == ReviewOutcome::ChangesRequested && note.is_none())
        {
            return Err(AppError::Validation(
                "A valid internal review note is required for requested changes.",
            ));
        }
        let (mut tx, actor) = self.begin(session, Permission::Publish).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state == PublicState::Withdrawn || head.review_state != ReviewState::InReview
        {
            return Err(AppError::Conflict);
        }
        sqlx::query("INSERT INTO news.editorial_reviews(revision_id,actor_id,outcome,internal_note) VALUES ($1,$2,$3,$4)")
            .bind(&head.current_revision_id).bind(actor.id()).bind(outcome.as_str()).bind(note).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_review",e))?;
        sqlx::query("UPDATE news.stories SET review_state=$2,state=CASE WHEN published_revision_id IS NULL AND $2='changes_requested' THEN 'draft' ELSE state END,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).bind(outcome.as_str()).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_review_head",e))?;
        finish(
            tx,
            &head,
            &head.current_revision_id,
            &actor,
            outcome.as_str(),
            "Editorial review recorded; see private review note.",
        )
        .await
    }
    pub async fn publish(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        public_reason: &str,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        check_reason(public_reason)?;
        let (mut tx, actor) = self.begin(session, Permission::Publish).await?;
        sqlx::query("SET LOCAL lock_timeout='1s'")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("publication_lock_timeout", e))?;
        sqlx::query("SET LOCAL statement_timeout='5s'")
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("publication_timeout", e))?;
        let candidate: Option<String> =
            sqlx::query_scalar("SELECT current_revision_id FROM news.stories WHERE id=$1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AppError::database("publication_candidate", e))?
                .flatten();
        let candidate = candidate.ok_or(AppError::NotFound)?;
        sqlx::query("SELECT news.lock_citation_evidence($1)")
            .bind(&candidate)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("publication_evidence_lock", e))?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.current_revision_id != candidate {
            return Err(AppError::Conflict);
        }

        if head.public_state == PublicState::Withdrawn
            || head.review_state != ReviewState::Approved
            || head.published_revision_id.as_deref() == Some(&head.current_revision_id)
        {
            return Err(AppError::Conflict);
        }
        let approved=sqlx::query_scalar::<_,String>("SELECT outcome FROM news.editorial_reviews WHERE revision_id=$1 ORDER BY id DESC LIMIT 1")
            .bind(&head.current_revision_id).fetch_optional(&mut *tx).await.map_err(|e|AppError::database("editorial_approval",e))?;
        if approved.as_deref() != Some("approved") {
            return Err(AppError::Conflict);
        }
        let revision = load_revision(&mut tx, &head.current_revision_id).await?;
        crate::citations::publication_check(&mut tx, &revision).await?;
        crate::length::publication_check(&mut tx, &revision, &self.policy).await?;
        let action = if head.published_revision_id.is_some() {
            "corrected"
        } else {
            "published"
        };
        publication_event(
            &mut tx,
            &head.id,
            &head.current_revision_id,
            &actor,
            action,
            public_reason,
        )
        .await?;
        sqlx::query("UPDATE news.stories SET state='published',published_revision_id=current_revision_id,lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_publish",e))?;
        finish(
            tx,
            &head,
            &head.current_revision_id,
            &actor,
            action,
            public_reason,
        )
        .await
    }
    pub async fn withdraw(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        public_reason: &str,
    ) -> Result<Head, AppError> {
        check_id(id)?;
        check_reason(public_reason)?;
        let (mut tx, actor) = self.begin(session, Permission::Publish).await?;
        let head = load_head(&mut tx, id, Some(expected)).await?;
        if head.public_state != PublicState::Published {
            return Err(AppError::Conflict);
        }
        let revision = head
            .published_revision_id
            .as_ref()
            .ok_or(AppError::InvalidData)?;
        publication_event(&mut tx, id, revision, &actor, "withdrawn", public_reason).await?;
        sqlx::query("UPDATE news.stories SET state='withdrawn',lock_version=lock_version+1,updated_at=now() WHERE id=$1")
            .bind(id).execute(&mut *tx).await.map_err(|e|AppError::database("editorial_withdraw",e))?;
        finish(tx, &head, revision, &actor, "withdrawn", public_reason).await
    }
    pub async fn head(&self, session: &Session, id: &str) -> Result<Head, AppError> {
        check_id(id)?;
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, None).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("editorial_read", e))?;
        Ok(head)
    }
    pub async fn revision(&self, session: &Session, id: &str) -> Result<Revision, AppError> {
        check_id(id)?;
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let revision = load_revision(&mut tx, id).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("editorial_read", e))?;
        Ok(revision)
    }
    /// Metadata only, at most 20 revisions. before_number is an exclusive cursor.
    pub async fn history(
        &self,
        session: &Session,
        id: &str,
        before_number: Option<i32>,
    ) -> Result<Vec<RevisionRecord>, AppError> {
        check_id(id)?;
        if before_number.is_some_and(|n| n <= 0) {
            return Err(AppError::Validation("Invalid history cursor."));
        }
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let _ = load_head(&mut tx, id, None).await?;
        let rows=sqlx::query_as::<_,RevisionRecord>("SELECT id,number,actor_id,title,change_reason,created_at FROM news.story_revisions WHERE story_id=$1 AND ($2::int IS NULL OR number<$2) ORDER BY number DESC LIMIT 20")
            .bind(id).bind(before_number).fetch_all(&mut *tx).await.map_err(|e|AppError::database("editorial_history",e))?;
        if rows.iter().any(|r| {
            !valid_id(&r.id) || !valid_id(&r.actor_id) || r.number <= 0 || !text(&r.title, 500)
        }) {
            return Err(AppError::InvalidData);
        }
        tx.commit()
            .await
            .map_err(|e| AppError::database("editorial_read", e))?;
        Ok(rows)
    }
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}
fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid story or revision ID."))
    }
}
fn check_reason(reason: &str) -> Result<(), AppError> {
    if text(reason, 2000) {
        Ok(())
    } else {
        Err(AppError::Validation(
            "A nonempty public reason of at most 2000 characters is required.",
        ))
    }
}
/// Shared creation for atomic manual intake. Caller must hold live identity/session
/// locks from begin_authorised; this never changes publication state.
pub(crate) async fn create_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Principal,
    id: &str,
    input: RevisionInput,
    policy: &crate::domain::EditorialPolicy,
) -> Result<Head, AppError> {
    check_id(id)?;
    input.validate()?;
    sqlx::query("INSERT INTO news.stories(id) VALUES ($1)")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::database("editorial_create", e))?;
    let revision = insert_revision(tx, id, actor, input, policy, None).await?;
    sqlx::query("UPDATE news.stories SET current_revision_id=$2,lock_version=1,updated_at=now() WHERE id=$1").bind(id).bind(&revision).execute(&mut **tx).await.map_err(|e|AppError::database("editorial_create_head",e))?;
    action(
        tx,
        id,
        &revision,
        actor,
        "created",
        "Initial draft created.",
        1,
    )
    .await?;
    let head = load_head(tx, id, None).await?;
    Ok(head)
}
async fn load_head(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    expected: Option<i64>,
) -> Result<Head, AppError> {
    let head:Head=sqlx::query_as::<_,RawHead>("SELECT id,state,review_state,current_revision_id,published_revision_id,lock_version FROM news.stories WHERE id=$1 FOR UPDATE")
        .bind(id).fetch_optional(&mut **tx).await.map_err(|e|AppError::database("editorial_head",e))?.ok_or(AppError::NotFound)?.try_into()?;
    if expected.is_some_and(|version| version != head.lock_version) {
        return Err(AppError::Conflict);
    }
    Ok(head)
}
async fn load_revision(tx: &mut Transaction<'_, Postgres>, id: &str) -> Result<Revision, AppError> {
    sqlx::query_as::<_, RawRevision>("SELECT * FROM news.story_revisions WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::database("editorial_revision", e))?
        .ok_or(AppError::NotFound)?
        .try_into()
}
async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    actor: &Principal,
    input: RevisionInput,
    policy: &crate::domain::EditorialPolicy,
    previous_provenance: Option<serde_json::Value>,
) -> Result<String, AppError> {
    let length = crate::length::Assessment::new(
        &input.body,
        &input.language,
        input.format,
        policy,
        input.length_override,
    )?;
    let evidence = input.evidence;
    let mut provenance = previous_provenance.unwrap_or_else(
        || serde_json::json!({"workflow":"editorial","origin":"manual_submission"}),
    );
    provenance["body_schema"] = serde_json::json!(2);
    provenance["length"] = serde_json::to_value(length).map_err(|_| AppError::InvalidData)?;
    provenance["sourcing"] = evidence
        .as_ref()
        .map(|p| serde_json::json!({"schema":1,"package":p}))
        .unwrap_or(serde_json::Value::Null);
    let revision:String=sqlx::query_scalar("INSERT INTO news.story_revisions(story_id,number,title,standfirst,summary,body,language,format,kind,topics,geography,actor_id,change_reason,provenance) SELECT $1,COALESCE(MAX(number),0)+1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13 FROM news.story_revisions WHERE story_id=$1 RETURNING id")
        .bind(id).bind(input.title).bind(input.standfirst).bind(input.summary).bind(Json(input.body)).bind(input.language).bind(input.format.as_str()).bind(input.kind.as_str()).bind(input.topics).bind(input.geography).bind(actor.id()).bind(input.reason).bind(Json(provenance))
        .fetch_one(&mut **tx).await.map_err(|e|AppError::database("editorial_save_revision",e))?;
    if let Some(package) = evidence {
        crate::sourcing::attach(tx, &revision, &package).await?;
    }
    Ok(revision)
}
async fn action(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    revision: &str,
    actor: &Principal,
    action: &str,
    reason: &str,
    version: i64,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.editorial_actions(story_id,revision_id,actor_id,action,reason,lock_version) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(revision).bind(actor.id()).bind(action).bind(reason).bind(version).execute(&mut **tx).await.map_err(|e|AppError::database("editorial_action",e))?;
    Ok(())
}
async fn publication_event(
    tx: &mut Transaction<'_, Postgres>,
    id: &str,
    revision: &str,
    actor: &Principal,
    action: &str,
    reason: &str,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ($1,$2,$3,$4,$5)")
        .bind(id).bind(revision).bind(action).bind(reason).bind(actor.id()).execute(&mut **tx).await.map_err(|e|AppError::database("editorial_publication_event",e))?;
    Ok(())
}
async fn finish(
    mut tx: Transaction<'static, Postgres>,
    head: &Head,
    revision: &str,
    actor: &Principal,
    verb: &str,
    reason: &str,
) -> Result<Head, AppError> {
    let updated = load_head(&mut tx, &head.id, None).await?;
    action(
        &mut tx,
        &head.id,
        revision,
        actor,
        verb,
        reason,
        updated.lock_version,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|e| AppError::database("editorial_commit", e))?;
    Ok(updated)
}

mod workbench;
pub use workbench::{DraftCard, DraftCursor, DraftFilter, DraftPage, EvidenceDraft};
