//! Private versioned draft selection only. Publication is a separate command/gate.
use super::*;
use crate::{
    auth::{self, Permission, Session},
    content::RawRevision,
};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::sync::Arc;
type Tx = Transaction<'static, Postgres>;
const MAX_PREVIEW_BYTES: i64 = 8 * 1024 * 1024;
#[derive(Clone)]
pub struct Drafts {
    pool: PgPool,
    policy: Arc<EditorialPolicy>,
    taxonomy: Arc<Taxonomy>,
}
#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Head {
    pub id: String,
    pub edition_date: NaiveDate,
    pub publication_timezone: String,
    pub state: String,
    pub lock_version: i64,
}
#[derive(Clone)]
pub struct EntryInput {
    pub story_id: String,
    pub revision_id: String,
    pub section: String,
    pub expected_story_version: i64,
}
#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Entry {
    pub story_id: String,
    pub revision_id: String,
    pub section: String,
    pub position: i32,
    pub is_lead: bool,
    pub prepared_story_version: Option<i64>,
}
pub struct Draft {
    pub head: Head,
    pub entries: Vec<Entry>,
    pub preview: Preview,
}
#[derive(sqlx::FromRow)]
pub struct Action {
    pub id: i64,
    pub actor_id: String,
    pub action: String,
    pub reason: String,
    pub lock_version: i64,
    pub created_at: DateTime<Utc>,
}
#[derive(sqlx::FromRow)]
struct Story {
    id: String,
    state: String,
    published_revision_id: Option<String>,
    lock_version: i64,
}
impl Drafts {
    pub fn new(pool: PgPool, policy: Arc<EditorialPolicy>, taxonomy: Arc<Taxonomy>) -> Self {
        Self {
            pool,
            policy,
            taxonomy,
        }
    }
    pub async fn create(
        &self,
        session: &Session,
        day: NaiveDate,
        reason: &str,
    ) -> Result<Head, AppError> {
        reason_check(reason)?;
        if !(1..=9999).contains(&day.year()) {
            return Err(AppError::Validation(
                "Edition date must have a public-route-compatible year.",
            ));
        }
        // Validate direct service construction too, even without an HTTP Config.
        Preview::new(&[], None, &self.policy, &self.taxonomy, Utc::now())?;
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let head: Head=sqlx::query_as("INSERT INTO news.editions(edition_date,publication_timezone,lock_version) VALUES ($1,$2,1) RETURNING id,edition_date,publication_timezone,state,lock_version")
            .bind(day).bind(&self.policy.publication_timezone).fetch_one(&mut *tx).await.map_err(|e|AppError::database("edition_create",e))?;
        audit(&mut tx, &head, actor.id(), "created", reason, None, &[]).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_create_commit", e))?;
        Ok(head)
    }
    /// Only actual current publications can be persisted in this first service.
    /// Article publication is always a separate explicit editorial action.
    pub async fn save(
        &self,
        session: &Session,
        id: &str,
        expected: i64,
        entries: &[EntryInput],
        lead: Option<&str>,
        reason: &str,
    ) -> Result<Head, AppError> {
        reason_check(reason)?;
        let mut identities = BTreeSet::new();
        if expected < 0
            || entries.len() > MAX_STORIES
            || lead.is_some_and(|id| !valid_id(id) || !entries.iter().any(|e| e.story_id == id))
            || entries.iter().any(|e| {
                !valid_id(&e.story_id)
                    || !valid_id(&e.revision_id)
                    || e.expected_story_version < 0
                    || !identities.insert(e.story_id.as_str())
            })
        {
            return Err(AppError::Validation(
                "Invalid or duplicate edition selection.",
            ));
        }
        let (mut tx, actor) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let mut head = load_head(&mut tx, id, true).await?;
        if head.state != "draft" || head.lock_version != expected {
            return Err(AppError::Conflict);
        }
        if head.publication_timezone != self.policy.publication_timezone {
            return Err(AppError::Validation(
                "Edition timezone differs from configured publication identity.",
            ));
        }
        let before = load_entries(&mut tx, id).await?;
        let ids: Vec<_> = identities.into_iter().collect();
        let stories:Vec<Story>=sqlx::query_as("SELECT id,state,published_revision_id,lock_version FROM news.stories WHERE id=ANY($1) ORDER BY id FOR SHARE")
            .bind(&ids).fetch_all(&mut *tx).await.map_err(|e|AppError::database("edition_selection_locks",e))?;
        for input in entries {
            let story = stories
                .iter()
                .find(|s| s.id == input.story_id)
                .ok_or(AppError::NotFound)?;
            if story.state != "published"
                || story.published_revision_id.as_deref() != Some(&input.revision_id)
                || story.lock_version != input.expected_story_version
            {
                return Err(AppError::Conflict);
            }
        }
        let next: Vec<Entry> = entries
            .iter()
            .enumerate()
            .map(|(position, e)| Entry {
                story_id: e.story_id.clone(),
                revision_id: e.revision_id.clone(),
                section: e.section.clone(),
                position: position as i32,
                is_lead: lead == Some(&e.story_id),
                prepared_story_version: Some(e.expected_story_version),
            })
            .collect();
        // Check content/section/byte bounds before deleting any previous selection.
        self.preview(&mut tx, &next).await?;
        let previous = serde_json::json!({"head":head,"entries":before});
        sqlx::query("DELETE FROM news.edition_entries WHERE edition_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("edition_replace", e))?;
        for e in &next {
            sqlx::query("INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead,prepared_story_version) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(id).bind(&e.story_id).bind(&e.revision_id).bind(&e.section).bind(e.position).bind(e.is_lead).bind(e.prepared_story_version)
                .execute(&mut *tx).await.map_err(|e|AppError::database("edition_entry",e))?;
        }
        head.lock_version = head
            .lock_version
            .checked_add(1)
            .ok_or(AppError::InvalidData)?;
        sqlx::query("UPDATE news.editions SET lock_version=$2 WHERE id=$1")
            .bind(id)
            .bind(head.lock_version)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("edition_version", e))?;
        audit(
            &mut tx,
            &head,
            actor.id(),
            "saved",
            reason,
            Some(previous),
            &next,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_save_commit", e))?;
        Ok(head)
    }
    pub async fn get(&self, session: &Session, id: &str) -> Result<Draft, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let head = load_head(&mut tx, id, false).await?;
        let entries = load_entries(&mut tx, id).await?;
        let mut preview = self.preview(&mut tx, &entries).await?;
        let ids: Vec<_> = entries.iter().map(|e| &e.story_id).collect();
        let current: Vec<Story> = sqlx::query_as(
            "SELECT id,state,published_revision_id,lock_version FROM news.stories WHERE id=ANY($1)",
        )
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::database("edition_current_selection", e))?;
        for entry in &entries {
            let story = current
                .iter()
                .find(|s| s.id == entry.story_id)
                .ok_or(AppError::InvalidData)?;
            if story.state != "published"
                || story.published_revision_id.as_deref() != Some(&entry.revision_id)
                || entry.prepared_story_version != Some(story.lock_version)
            {
                preview.warn(WarningKind::StaleSelection,format!("Sélection à confirmer explicitement : {}. La publication ou sa version de contrôle a changé, ou l’ancienne archive n’a pas de version préparée.",entry.story_id));
            }
        }
        if head.publication_timezone != self.policy.publication_timezone {
            preview.warn(WarningKind::PublicationTimezone,"L’identité conservée utilise un autre fuseau que la politique actuelle ; aucune date n’a été réécrite.".into());
        }
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_read_commit", e))?;
        Ok(Draft {
            head,
            entries,
            preview,
        })
    }
    pub async fn history(
        &self,
        session: &Session,
        id: &str,
        before: Option<i64>,
    ) -> Result<Vec<Action>, AppError> {
        if before.is_some_and(|n| n <= 0) {
            return Err(AppError::Validation("Invalid edition history cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        load_head(&mut tx, id, false).await?;
        let actions=sqlx::query_as("SELECT id,actor_id,action,reason,lock_version,created_at FROM news.edition_actions WHERE edition_id=$1 AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT 20")
            .bind(id).bind(before).fetch_all(&mut *tx).await.map_err(|e|AppError::database("edition_history",e))?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_history_commit", e))?;
        Ok(actions)
    }
    async fn preview(&self, tx: &mut Tx, entries: &[Entry]) -> Result<Preview, AppError> {
        let ids: Vec<_> = entries.iter().map(|e| &e.revision_id).collect();
        let bytes:i64=sqlx::query_scalar("SELECT COALESCE(sum(octet_length(body::text)),0)::bigint FROM news.story_revisions WHERE id=ANY($1)")
            .bind(&ids).fetch_one(&mut **tx).await.map_err(|e|AppError::database("edition_preview_bound",e))?;
        if bytes > MAX_PREVIEW_BYTES {
            return Err(AppError::Validation(
                "Edition preview exceeds its body-byte bound.",
            ));
        }
        let raw: Vec<RawRevision> =
            sqlx::query_as("SELECT * FROM news.story_revisions WHERE id=ANY($1)")
                .bind(&ids)
                .fetch_all(&mut **tx)
                .await
                .map_err(|e| AppError::database("edition_preview_revisions", e))?;
        let revisions: Vec<Revision> = raw
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;
        let instants:Vec<(String,Option<DateTime<Utc>>)>=sqlx::query_as("SELECT revision_id,min(created_at) FROM news.publication_events WHERE revision_id=ANY($1) AND action IN ('published','corrected') GROUP BY revision_id")
            .bind(&ids).fetch_all(&mut **tx).await.map_err(|e|AppError::database("edition_publication_times",e))?;
        let selected: Vec<_> = entries
            .iter()
            .map(|e| {
                let revision = revisions
                    .iter()
                    .find(|r| r.id == e.revision_id && r.story_id == e.story_id)
                    .ok_or(AppError::InvalidData)?;
                let published_at = instants
                    .iter()
                    .find(|(id, _)| id == &e.revision_id)
                    .and_then(|(_, at)| *at);
                Ok(Selection {
                    revision,
                    section: e.section.as_str(),
                    published_at,
                })
            })
            .collect::<Result<_, AppError>>()?;
        let at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::database("edition_observation_time", e))?;
        let lead = entries
            .iter()
            .find(|e| e.is_lead)
            .map(|e| e.story_id.as_str());
        Preview::new(&selected, lead, &self.policy, &self.taxonomy, at)
    }
}
fn reason_check(reason: &str) -> Result<(), AppError> {
    if reason.trim().is_empty() || reason.chars().count() > 2000 || reason.contains('\0') {
        return Err(AppError::Validation(
            "An edition change needs a bounded private reason.",
        ));
    }
    Ok(())
}
async fn load_head(tx: &mut Tx, id: &str, write: bool) -> Result<Head, AppError> {
    if !valid_id(id) {
        return Err(AppError::Validation("Invalid edition identity."));
    }
    let sql = if write {
        "SELECT id,edition_date,publication_timezone,state,lock_version FROM news.editions WHERE id=$1 FOR UPDATE"
    } else {
        "SELECT id,edition_date,publication_timezone,state,lock_version FROM news.editions WHERE id=$1 FOR SHARE"
    };
    let head: Head = sqlx::query_as(sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::database("edition_head", e))?
        .ok_or(AppError::NotFound)?;
    if head.lock_version < 0
        || !matches!(head.state.as_str(), "draft" | "published")
        || head.publication_timezone.parse::<chrono_tz::Tz>().is_err()
    {
        return Err(AppError::InvalidData);
    }
    Ok(head)
}
async fn load_entries(tx: &mut Tx, id: &str) -> Result<Vec<Entry>, AppError> {
    let entries:Vec<Entry>=sqlx::query_as("SELECT story_id,revision_id,section,position,is_lead,prepared_story_version FROM news.edition_entries WHERE edition_id=$1 ORDER BY position LIMIT 101")
        .bind(id).fetch_all(&mut **tx).await.map_err(|e|AppError::database("edition_entries_private",e))?;
    if entries.len() > MAX_STORIES {
        return Err(AppError::Validation(
            "Edition exceeds the private workbench bound.",
        ));
    }
    Ok(entries)
}
async fn audit(
    tx: &mut Tx,
    head: &Head,
    actor: &str,
    action: &str,
    reason: &str,
    before: Option<serde_json::Value>,
    entries: &[Entry],
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO news.edition_actions(edition_id,actor_id,action,reason,lock_version,before_state,after_state) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(&head.id).bind(actor).bind(action).bind(reason).bind(head.lock_version).bind(before.map(Json)).bind(Json(serde_json::json!({"head":head,"entries":entries})))
        .execute(&mut **tx).await.map_err(|e|AppError::database("edition_audit",e))?;
    Ok(())
}
