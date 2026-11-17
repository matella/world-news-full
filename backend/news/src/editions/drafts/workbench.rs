//! Bounded private preparation. Choice metadata and preview warnings are observations only.
use super::*;
use crate::{
    content::{ArticleFormat, ArticleKind},
    editorial::PublicState,
};
#[derive(Clone, Copy)]
pub enum Filter {
    Drafts,
    All,
}
pub struct Cursor {
    pub date: NaiveDate,
    pub id: String,
}
#[derive(sqlx::FromRow)]
pub struct Card {
    pub id: String,
    pub edition_date: NaiveDate,
    pub publication_timezone: String,
    pub state: String,
    pub lock_version: i64,
    pub story_count: i64,
}
pub struct Cards {
    pub items: Vec<Card>,
    pub next: Option<Cursor>,
}
pub struct Choice {
    pub id: String,
    pub title: Option<String>,
    pub revision_id: Option<String>,
    pub state: PublicState,
    pub lock_version: i64,
    pub section: Option<String>,
    pub language: Option<String>,
    pub format: Option<ArticleFormat>,
    pub kind: Option<ArticleKind>,
}
pub struct Choices {
    pub items: Vec<Choice>,
    pub next: Option<String>,
}
#[derive(sqlx::FromRow)]
struct RawChoice {
    id: String,
    title: Option<String>,
    revision_id: Option<String>,
    state: String,
    lock_version: i64,
    language: Option<String>,
    format: Option<String>,
    kind: Option<String>,
    topics: Option<Vec<String>>,
    geography: Option<Vec<String>>,
}
const CHOICE_SQL: &str = "SELECT s.id,r.title,s.published_revision_id AS revision_id,s.state,s.lock_version,r.language,r.format,r.kind,r.topics,r.geography FROM news.stories s LEFT JOIN news.story_revisions r ON r.id=s.published_revision_id";
impl Drafts {
    pub async fn cards(
        &self,
        session: &Session,
        filter: Filter,
        cursor: Option<&Cursor>,
    ) -> Result<Cards, AppError> {
        if cursor.is_some_and(|c| !valid_id(&c.id)) {
            return Err(AppError::Validation("Invalid edition cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let mut items:Vec<Card>=sqlx::query_as("SELECT e.id,e.edition_date,e.publication_timezone,e.state,e.lock_version,(SELECT count(*) FROM news.edition_entries m WHERE m.edition_id=e.id) AS story_count FROM news.editions e WHERE ($1 OR e.state='draft') AND ($2::date IS NULL OR (e.edition_date,e.id)<($2,$3)) ORDER BY e.edition_date DESC,e.id DESC LIMIT 21")
            .bind(matches!(filter,Filter::All)).bind(cursor.map(|c|c.date)).bind(cursor.map(|c|c.id.as_str()))
            .fetch_all(&mut *tx).await.map_err(|e|AppError::database("edition_cards",e))?;
        if items.iter().any(|c| {
            !valid_id(&c.id)
                || !matches!(c.state.as_str(), "draft" | "published")
                || c.lock_version < 0
                || c.story_count < 0
                || c.publication_timezone.parse::<chrono_tz::Tz>().is_err()
        }) {
            return Err(AppError::InvalidData);
        }
        let more = items.len() > 20;
        items.truncate(20);
        let next = if more {
            items.last().map(|c| Cursor {
                date: c.edition_date,
                id: c.id.clone(),
            })
        } else {
            None
        };
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_cards_commit", e))?;
        Ok(Cards { items, next })
    }
    /// Retained selections may now be withdrawn/private. Never label them ready or attach their new version automatically.
    pub async fn story_choices(
        &self,
        session: &Session,
        after: Option<&str>,
        keep: &[String],
    ) -> Result<Choices, AppError> {
        let mut selected = BTreeSet::new();
        if after.is_some_and(|id| !valid_id(id))
            || keep.len() > MAX_STORIES
            || keep.iter().any(|id| !valid_id(id) || !selected.insert(id))
        {
            return Err(AppError::Validation("Invalid edition story choices."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let sql = format!(
            "{CHOICE_SQL} WHERE s.state='published' AND ($1::text IS NULL OR s.id>$1) ORDER BY s.id LIMIT 21"
        );
        let mut rows: Vec<RawChoice> =
            sqlx::query_as(&sql)
                .bind(after)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| AppError::database("edition_story_choices", e))?;
        let more = rows.len() > 20;
        rows.truncate(20);
        let next = if more {
            rows.last().map(|r| r.id.clone())
        } else {
            None
        };
        let sql = format!("{CHOICE_SQL} WHERE s.id=ANY($1) ORDER BY s.id LIMIT 101");
        let retained: Vec<RawChoice> = sqlx::query_as(&sql)
            .bind(keep)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::database("edition_retained_choices", e))?;
        for row in retained {
            if !rows.iter().any(|r| r.id == row.id) {
                rows.push(row);
            }
        }
        let items = rows
            .into_iter()
            .map(|r| self.choice(r))
            .collect::<Result<_, _>>()?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_choices_commit", e))?;
        Ok(Choices { items, next })
    }
    /// Nonpersistent own-input preview. Final save rechecks publication and every fence.
    pub async fn prepare(
        &self,
        session: &Session,
        entries: &[EntryInput],
        lead: Option<&str>,
    ) -> Result<Preview, AppError> {
        if entries.len() > MAX_STORIES
            || lead.is_some_and(|id| !valid_id(id))
            || entries.iter().any(|e| {
                !valid_id(&e.story_id) || !valid_id(&e.revision_id) || e.expected_story_version < 0
            })
        {
            return Err(AppError::Validation("Invalid edition preparation."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let ids: Vec<_> = entries.iter().map(|e| &e.revision_id).collect();
        let pairs: Vec<(String, String)> =
            sqlx::query_as("SELECT id,story_id FROM news.story_revisions WHERE id=ANY($1)")
                .bind(ids)
                .fetch_all(&mut *tx)
                .await
                .map_err(|e| AppError::database("edition_prepared_identities", e))?;
        if entries.iter().any(|e| {
            !pairs
                .iter()
                .any(|(revision, story)| revision == &e.revision_id && story == &e.story_id)
        }) {
            return Err(AppError::Validation(
                "An edition revision does not belong to the selected story.",
            ));
        }
        let prepared: Vec<Entry> = entries
            .iter()
            .enumerate()
            .map(|(n, e)| Entry {
                story_id: e.story_id.clone(),
                revision_id: e.revision_id.clone(),
                section: e.section.clone(),
                position: n as i32,
                is_lead: lead == Some(&e.story_id),
                prepared_story_version: Some(e.expected_story_version),
            })
            .collect();
        let mut preview = self.preview(&mut tx, &prepared).await?;
        let ids: Vec<_> = entries.iter().map(|e| &e.story_id).collect();
        let stories: Vec<Story> = sqlx::query_as(
            "SELECT id,state,published_revision_id,lock_version FROM news.stories WHERE id=ANY($1)",
        )
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::database("edition_prepared_heads", e))?;
        for entry in entries {
            let current = stories
                .iter()
                .find(|s| s.id == entry.story_id)
                .ok_or(AppError::InvalidData)?;
            if current.state != "published"
                || current.published_revision_id.as_deref() != Some(&entry.revision_id)
                || current.lock_version != entry.expected_story_version
            {
                preview.warn(WarningKind::StaleSelection,format!("Sélection privée, retirée ou devenue obsolète : {}. Les versions préparées ne sont pas actualisées automatiquement.",entry.story_id));
            }
        }
        tx.commit()
            .await
            .map_err(|e| AppError::database("edition_prepare_commit", e))?;
        Ok(preview)
    }
    fn choice(&self, r: RawChoice) -> Result<Choice, AppError> {
        if !valid_id(&r.id)
            || r.lock_version < 0
            || r.revision_id.as_deref().is_some_and(|id| !valid_id(id))
            || r.title
                .as_ref()
                .is_some_and(|t| t.trim().is_empty() || t.chars().count() > 500)
            || r.language
                .as_deref()
                .is_some_and(|s| !crate::content::valid_language(s))
        {
            return Err(AppError::InvalidData);
        }
        let state: PublicState = serde_json::from_value(serde_json::Value::String(r.state))
            .map_err(|_| AppError::InvalidData)?;
        let format = r
            .format
            .map(|f| {
                serde_json::from_value(serde_json::Value::String(f))
                    .map_err(|_| AppError::InvalidData)
            })
            .transpose()?;
        let kind = r
            .kind
            .map(|f| {
                serde_json::from_value(serde_json::Value::String(f))
                    .map_err(|_| AppError::InvalidData)
            })
            .transpose()?;
        let section = match (r.topics, r.geography) {
            (Some(topics), Some(geography)) => {
                Some(self.taxonomy.project(&topics, &geography).section)
            }
            (None, None) => None,
            _ => return Err(AppError::InvalidData),
        };
        if state == PublicState::Published
            && (r.revision_id.is_none()
                || r.title.is_none()
                || r.language.is_none()
                || format.is_none()
                || kind.is_none()
                || section.is_none())
        {
            return Err(AppError::InvalidData);
        }
        Ok(Choice {
            id: r.id,
            title: r.title,
            revision_id: r.revision_id,
            state,
            lock_version: r.lock_version,
            section,
            language: r.language,
            format,
            kind,
        })
    }
}
