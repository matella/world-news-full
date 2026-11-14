use crate::{content::*, error::AppError};
use chrono::NaiveDate;
use sqlx::{PgPool, types::Json};

#[derive(Clone)]
pub struct Repository {
    pool: PgPool,
}
impl Repository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn story(&self, id: &str) -> Result<Revision, AppError> {
        check_id(id)?;
        let raw=sqlx::query_as::<_,RawRevision>("SELECT r.* FROM news.public_revisions r JOIN news.public_stories s ON s.revision_id=r.id WHERE s.id=$1")
            .bind(id).fetch_optional(&self.pool).await.map_err(|e|AppError::database("story",e))?;
        raw.ok_or(AppError::NotFound)?.try_into()
    }
    pub async fn revision(&self, id: &str) -> Result<Revision, AppError> {
        check_id(id)?;
        let raw =
            sqlx::query_as::<_, RawRevision>("SELECT * FROM news.public_revisions WHERE id=$1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::database("revision", e))?;
        raw.ok_or(AppError::NotFound)?.try_into()
    }
    pub async fn editions(
        &self,
        timezone: &str,
        cursor: Option<&EditionCursor>,
        size: PageSize,
    ) -> Result<EditionPage, AppError> {
        check_timezone(timezone)?;
        if let Some(cursor) = cursor {
            check_id(&cursor.id)?;
        }
        let mut items=sqlx::query_as::<_,EditionSummary>("SELECT * FROM news.public_editions WHERE publication_timezone=$1 AND ($2::date IS NULL OR (edition_date,id)<($2,$3)) ORDER BY edition_date DESC,id DESC LIMIT $4")
            .bind(timezone).bind(cursor.map(|c|c.date)).bind(cursor.map(|c|c.id.as_str())).bind(i64::from(size.get())+1)
            .fetch_all(&self.pool).await.map_err(|e|AppError::database("editions",e))?;
        for item in &items {
            validate_edition(item)?;
        }
        let has_more = items.len() > size.get() as usize;
        items.truncate(size.get() as usize);
        let next = if has_more {
            items.last().map(|item| EditionCursor {
                date: item.edition_date,
                id: item.id.clone(),
            })
        } else {
            None
        };
        Ok(EditionPage { items, next })
    }
    pub async fn latest_edition(&self, timezone: &str) -> Result<Option<EditionSummary>, AppError> {
        Ok(self
            .editions(timezone, None, PageSize::new(1)?)
            .await?
            .items
            .into_iter()
            .next())
    }
    pub async fn edition(
        &self,
        date: NaiveDate,
        timezone: &str,
    ) -> Result<EditionSummary, AppError> {
        check_timezone(timezone)?;
        let item = sqlx::query_as::<_, EditionSummary>(
            "SELECT * FROM news.public_editions WHERE edition_date=$1 AND publication_timezone=$2",
        )
        .bind(date)
        .bind(timezone)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::database("edition", e))?
        .ok_or(AppError::NotFound)?;
        validate_edition(&item)?;
        Ok(item)
    }
    pub async fn edition_entries(&self, edition_id: &str) -> Result<Vec<EditionEntry>, AppError> {
        check_id(edition_id)?;
        if !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM news.public_editions WHERE id=$1)",
        )
        .bind(edition_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::database("edition_exists", e))?
        {
            return Err(AppError::NotFound);
        }
        #[derive(sqlx::FromRow)]
        struct RawEntry {
            story_id: String,
            revision_id: String,
            section: String,
            position: i32,
            is_lead: bool,
            withdrawn: bool,
            revision: Option<Json<RawRevision>>,
        }
        let rows=sqlx::query_as::<_,RawEntry>("SELECT e.*,to_jsonb(r) AS revision FROM news.public_edition_entries e LEFT JOIN news.public_revisions r ON r.id=e.revision_id WHERE e.edition_id=$1 ORDER BY e.position,e.story_id LIMIT 201")
            .bind(edition_id).fetch_all(&self.pool).await.map_err(|e|AppError::database("edition_entries",e))?;
        if rows.len() > 200 {
            return Err(AppError::InvalidData);
        }
        rows.into_iter()
            .map(|row| {
                let revision = row
                    .revision
                    .map(|raw| Revision::try_from(raw.0))
                    .transpose()?;
                if !valid_id(&row.story_id)
                    || !valid_id(&row.revision_id)
                    || row.position < 0
                    || row.section.trim().is_empty()
                    || (row.withdrawn && revision.is_some())
                    || (!row.withdrawn && revision.is_none())
                    || revision
                        .as_ref()
                        .is_some_and(|r| r.id != row.revision_id || r.story_id != row.story_id)
                {
                    return Err(AppError::InvalidData);
                }
                Ok(EditionEntry {
                    story_id: row.story_id,
                    revision_id: row.revision_id,
                    section: row.section,
                    position: row.position,
                    is_lead: row.is_lead,
                    withdrawn: row.withdrawn,
                    revision,
                })
            })
            .collect()
    }
    pub async fn sources(&self, revision_id: &str) -> Result<Vec<SourceLink>, AppError> {
        check_id(revision_id)?;
        if !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM news.public_revisions WHERE id=$1)",
        )
        .bind(revision_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::database("revision_exists", e))?
        {
            return Err(AppError::NotFound);
        }
        let rows=sqlx::query_as::<_,SourceLink>("SELECT DISTINCT source_id,source_name,original_url FROM news.public_citations WHERE revision_id=$1 ORDER BY source_id,original_url LIMIT 201")
            .bind(revision_id).fetch_all(&self.pool).await.map_err(|e|AppError::database("sources",e))?;
        if rows.len() > 200
            || rows.iter().any(|s| {
                !valid_id(&s.source_id)
                    || s.source_name.trim().is_empty()
                    || !safe_source_url(&s.original_url)
            })
        {
            return Err(AppError::InvalidData);
        }
        Ok(rows)
    }
    /// Privileged persistence API. Authentication/authorisation belongs to #15/#16.
    pub async fn draft_head(&self, id: &str) -> Result<DraftHead, AppError> {
        check_id(id)?;
        let head=sqlx::query_as::<_,DraftHead>("SELECT id,state,current_revision_id,published_revision_id,lock_version FROM news.stories WHERE id=$1")
            .bind(id).fetch_optional(&self.pool).await.map_err(|e|AppError::database("draft_head",e))?.ok_or(AppError::NotFound)?;
        if !valid_id(&head.id)
            || !matches!(
                head.state.as_str(),
                "draft" | "in_review" | "published" | "withdrawn"
            )
            || head.lock_version < 0
            || head
                .current_revision_id
                .as_ref()
                .is_some_and(|id| !valid_id(id))
            || head
                .published_revision_id
                .as_ref()
                .is_some_and(|id| !valid_id(id))
        {
            return Err(AppError::InvalidData);
        }
        Ok(head)
    }
    pub async fn create_draft(&self, input: DraftInput) -> Result<DraftHead, AppError> {
        check_id(&input.id)?;
        check_id(&input.actor_id)?;
        if input.title.trim().is_empty()
            || input.title.chars().count() > 500
            || !valid_language(&input.language)
            || !validate_blocks(&input.body)
        {
            return Err(AppError::Validation("Invalid draft content."));
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::database("draft_begin", e))?;
        sqlx::query("INSERT INTO news.stories(id) VALUES ($1)")
            .bind(&input.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("draft_story", e))?;
        let revision:String=sqlx::query_scalar("INSERT INTO news.story_revisions(story_id,number,title,body,language,format,kind,actor_id) VALUES ($1,1,$2,$3,$4,$5,$6,$7) RETURNING id")
            .bind(&input.id).bind(&input.title).bind(Json(&input.body)).bind(&input.language).bind(input.format.as_str()).bind(input.kind.as_str()).bind(&input.actor_id)
            .fetch_one(&mut *tx).await.map_err(|e|AppError::database("draft_revision",e))?;
        sqlx::query("UPDATE news.stories SET current_revision_id=$1,lock_version=1 WHERE id=$2")
            .bind(&revision)
            .bind(&input.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("draft_pointer", e))?;
        tx.commit()
            .await
            .map_err(|e| AppError::database("draft_commit", e))?;
        Ok(DraftHead {
            id: input.id,
            state: "draft".into(),
            current_revision_id: Some(revision),
            published_revision_id: None,
            lock_version: 1,
        })
    }
}

fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid identifier."))
    }
}
fn check_timezone(timezone: &str) -> Result<(), AppError> {
    timezone
        .parse::<chrono_tz::Tz>()
        .map(|_| ())
        .map_err(|_| AppError::Validation("Invalid publication timezone."))
}
fn validate_edition(item: &EditionSummary) -> Result<(), AppError> {
    if !valid_id(&item.id) || item.publication_timezone.parse::<chrono_tz::Tz>().is_err() {
        Err(AppError::InvalidData)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, persistence};

    #[tokio::test]
    async fn an_unavailable_pool_is_not_an_empty_archive() {
        let config = Config::from_lookup(|key| {
            (key == "DATABASE_URL").then(|| "postgres://fixture:synthetic@127.0.0.1:1/news".into())
        })
        .unwrap();
        let pool = persistence::pool(&config);
        pool.close().await;
        let repository = Repository::new(pool);
        assert!(matches!(
            repository.latest_edition("Europe/Brussels").await,
            Err(AppError::Unavailable)
        ));
    }
}
