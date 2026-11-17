//! Private read models for the editor. No HTTP publication is exposed here.
use super::*;
use crate::{
    citations,
    sourcing::{Binding, SourcePackage},
};
#[derive(Clone, Copy)]
pub enum DraftFilter {
    Candidates,
    InReview,
    Published,
    Withdrawn,
    All,
}
impl DraftFilter {
    fn key(self) -> &'static str {
        match self {
            Self::Candidates => "candidates",
            Self::InReview => "in_review",
            Self::Published => "published",
            Self::Withdrawn => "withdrawn",
            Self::All => "all",
        }
    }
}
pub struct DraftCursor {
    pub updated_at: Option<DateTime<Utc>>,
    pub id: String,
}
#[derive(sqlx::FromRow)]
pub struct DraftCard {
    pub id: String,
    pub title: String,
    pub revision_id: Option<String>,
    pub state: String,
    pub review_state: String,
    pub lock_version: i64,
    pub updated_at: Option<DateTime<Utc>>,
}
pub struct DraftPage {
    pub items: Vec<DraftCard>,
    pub next: Option<DraftCursor>,
}
/// Contains private declarations; intentionally has no Debug/Serialize implementation.
pub struct EvidenceDraft {
    pub head: Head,
    pub revision: Revision,
    pub package: SourcePackage,
    pub proposed: bool,
}
impl EditorialService {
    pub async fn drafts(
        &self,
        session: &Session,
        filter: DraftFilter,
        cursor: Option<&DraftCursor>,
        size: PageSize,
    ) -> Result<DraftPage, AppError> {
        if cursor.is_some_and(|c| !valid_id(&c.id)) {
            return Err(AppError::Validation("Invalid draft cursor."));
        }
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let mut items=sqlx::query_as::<_,DraftCard>("SELECT s.id,r.title,s.current_revision_id AS revision_id,s.state,s.review_state,s.lock_version,s.updated_at FROM news.stories s LEFT JOIN news.story_revisions r ON r.id=s.current_revision_id WHERE ($1='all' OR ($1='candidates' AND s.state<>'withdrawn' AND s.current_revision_id IS DISTINCT FROM s.published_revision_id) OR ($1='in_review' AND s.review_state='in_review' AND s.state<>'withdrawn') OR ($1='published' AND s.state='published') OR ($1='withdrawn' AND s.state='withdrawn')) AND ($3::text IS NULL OR ($2::timestamptz IS NULL AND s.updated_at IS NULL AND s.id>$3) OR ($2 IS NOT NULL AND (s.updated_at<$2 OR s.updated_at IS NULL OR (s.updated_at=$2 AND s.id>$3)))) ORDER BY s.updated_at DESC NULLS LAST,s.id ASC LIMIT $4")
            .bind(filter.key()).bind(cursor.and_then(|c|c.updated_at)).bind(cursor.map(|c|c.id.as_str())).bind(i64::from(size.get())+1)
            .fetch_all(&mut *tx).await.map_err(|e|AppError::database("draft_workbench",e))?;
        if items.iter().any(|c| {
            !valid_id(&c.id)
                || c.revision_id.as_deref().is_none_or(|id| !valid_id(id))
                || c.lock_version < 0
                || c.title.trim().is_empty()
                || !matches!(
                    c.state.as_str(),
                    "draft" | "in_review" | "published" | "withdrawn"
                )
                || !matches!(
                    c.review_state.as_str(),
                    "draft" | "in_review" | "approved" | "changes_requested"
                )
        }) {
            return Err(AppError::InvalidData);
        }
        let more = items.len() > size.get() as usize;
        items.truncate(size.get() as usize);
        let next = if more {
            items.last().map(|c| DraftCursor {
                updated_at: c.updated_at,
                id: c.id.clone(),
            })
        } else {
            None
        };
        tx.commit()
            .await
            .map_err(|e| AppError::database("draft_workbench", e))?;
        Ok(DraftPage { items, next })
    }
    /// Read the actual frozen bindings, not the package's earlier auto-capture request.
    /// Previous-revision suggestions are explicitly unattached until an editor saves.
    pub async fn evidence_draft(
        &self,
        session: &Session,
        id: &str,
    ) -> Result<EvidenceDraft, AppError> {
        check_id(id)?;
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, None).await?;
        let revision = load_revision(&mut tx, &head.current_revision_id).await?;
        let current = citations::private_package(&mut tx, &revision.id).await?;
        let mut chosen = revision.id.clone();
        let mut proposed = false;
        let mut package = current;
        if package.is_none() {
            let previous:Option<String>=sqlx::query_scalar("SELECT p.id FROM news.story_revisions r JOIN news.story_revisions p ON p.id=r.provenance->>'edited_from' AND p.story_id=r.story_id WHERE r.id=$1")
                .bind(&revision.id).fetch_optional(&mut *tx).await.map_err(|e|AppError::database("evidence_proposal",e))?;
            if let Some(previous) = previous {
                chosen = previous;
                proposed = true;
                package = citations::private_package(&mut tx, &chosen).await?;
            }
        }
        let rows:Vec<(String,Option<String>,Option<String>,i64)>=sqlx::query_as("SELECT source_article_id,anchor_id,location,origin_version FROM news.citations WHERE revision_id=$1 ORDER BY claim_id,source_article_id LIMIT 201")
            .bind(&chosen).fetch_all(&mut *tx).await.map_err(|e|AppError::database("evidence_bindings",e))?;
        if rows.len() > 200 {
            return Err(AppError::InvalidData);
        }
        let mut package = package.unwrap_or_default();
        package.bindings = rows
            .into_iter()
            .map(
                |(source_article_id, anchor_id, location, version)| Binding {
                    source_article_id,
                    anchor_id,
                    location,
                    origin_version: Some(version),
                },
            )
            .collect();
        tx.commit()
            .await
            .map_err(|e| AppError::database("evidence_draft", e))?;
        Ok(EvidenceDraft {
            head,
            revision,
            package,
            proposed,
        })
    }
}

/// Bounded metadata choices plus selected records, never source text or inferred proof.
#[derive(sqlx::FromRow)]
pub struct SourceChoice {
    pub id: String,
    pub title: String,
    pub attribution_name: String,
}
pub struct SourceChoices {
    pub items: Vec<SourceChoice>,
    pub next: Option<String>,
}
impl EditorialService {
    pub async fn source_choices(
        &self,
        session: &Session,
        selected: &[String],
        after: Option<&str>,
        source: Option<&str>,
    ) -> Result<SourceChoices, AppError> {
        if selected.len() > 200
            || selected.iter().any(|id| !valid_id(id))
            || after.is_some_and(|id| !valid_id(id))
            || source.is_some_and(|id| !valid_id(id))
        {
            return Err(AppError::Validation("Invalid source selection filter."));
        }
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let mut page: Vec<SourceChoice> = sqlx::query_as("SELECT id,title,attribution_name FROM news.source_articles WHERE ($1::text IS NULL OR id>$1) AND ($2::text IS NULL OR source_id=$2) ORDER BY id LIMIT 51")
            .bind(after).bind(source).fetch_all(&mut *tx).await.map_err(|e|AppError::database("source_choices",e))?;
        let more = page.len() > 50;
        page.truncate(50);
        let next = if more {
            page.last().map(|r| r.id.clone())
        } else {
            None
        };
        let retained: Vec<SourceChoice> = sqlx::query_as("SELECT id,title,attribution_name FROM news.source_articles WHERE id=ANY($1) ORDER BY id")
            .bind(selected).fetch_all(&mut *tx).await.map_err(|e|AppError::database("selected_sources",e))?;
        for row in retained {
            if !page.iter().any(|p| p.id == row.id) {
                page.push(row);
            }
        }
        tx.commit()
            .await
            .map_err(|e| AppError::database("source_choices", e))?;
        Ok(SourceChoices { items: page, next })
    }
}

#[derive(sqlx::FromRow)]
pub struct ReviewRecord {
    pub id: i64,
    pub revision_id: String,
    pub actor_id: String,
    pub outcome: String,
    pub internal_note: Option<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(sqlx::FromRow)]
pub struct PublicationRecord {
    pub id: i64,
    pub revision_id: String,
    pub actor_id: String,
    pub action: String,
    pub public_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Private observed preparation. No ready flag is accepted by a write command.
pub struct Workflow {
    pub head: Head,
    pub revision: Revision,
    pub evidence: citations::Report,
    pub length: crate::length::Report,
    pub source_blocker: Option<String>,
    pub length_blocker: Option<String>,
    pub revisions: Vec<RevisionRecord>,
    pub reviews: Vec<ReviewRecord>,
    pub publications: Vec<PublicationRecord>,
}
fn observed_check(result: Result<(), AppError>) -> Result<Option<String>, AppError> {
    match result {
        Ok(()) => Ok(None),
        Err(AppError::Validation(text)) => Ok(Some(text.into())),
        Err(e) => Err(e),
    }
}
impl EditorialService {
    pub async fn workflow(
        &self,
        session: &Session,
        id: &str,
        before_revision: Option<i32>,
        before_review: Option<i64>,
        before_publication: Option<i64>,
    ) -> Result<Workflow, AppError> {
        check_id(id)?;
        if before_revision.is_some_and(|v| v <= 0)
            || before_review.is_some_and(|v| v <= 0)
            || before_publication.is_some_and(|v| v <= 0)
        {
            return Err(AppError::Validation("Invalid workflow cursor."));
        }
        let (mut tx, _) = self.begin(session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, None).await?;
        let revision = load_revision(&mut tx, &head.current_revision_id).await?;
        if revision.story_id != id {
            return Err(AppError::InvalidData);
        }
        let source_blocker =
            observed_check(citations::publication_check(&mut tx, &revision).await)?;
        let length_blocker = observed_check(
            crate::length::publication_check(&mut tx, &revision, &self.policy).await,
        )?;
        let evidence = citations::private_report(&mut tx, &revision).await?;
        let recorded = crate::length::private_assessment(&mut tx, &revision.id).await?;
        let length = crate::length::Report::new(&revision, &self.policy, recorded)?;
        let revisions=sqlx::query_as("SELECT id,number,actor_id,title,change_reason,created_at FROM news.story_revisions WHERE story_id=$1 AND ($2::int IS NULL OR number<$2) ORDER BY number DESC LIMIT 20")
            .bind(id).bind(before_revision).fetch_all(&mut *tx).await.map_err(|e|AppError::database("workflow_revisions",e))?;
        let reviews=sqlx::query_as("SELECT review.id,review.revision_id,review.actor_id,review.outcome,review.internal_note,review.created_at FROM news.editorial_reviews review JOIN news.story_revisions r ON r.id=review.revision_id WHERE r.story_id=$1 AND ($2::bigint IS NULL OR review.id<$2) ORDER BY review.id DESC LIMIT 20")
            .bind(id).bind(before_review).fetch_all(&mut *tx).await.map_err(|e|AppError::database("workflow_reviews",e))?;
        let publications=sqlx::query_as("SELECT id,revision_id,actor_id,action,public_reason,created_at FROM news.publication_events WHERE story_id=$1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT 20")
            .bind(id).bind(before_publication).fetch_all(&mut *tx).await.map_err(|e|AppError::database("workflow_publications",e))?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("workflow_read", e))?;
        Ok(Workflow {
            head,
            revision,
            evidence,
            length,
            source_blocker,
            length_blocker,
            revisions,
            reviews,
            publications,
        })
    }
}
