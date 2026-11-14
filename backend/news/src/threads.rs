//! Stable, explicitly curated subject threads. Legacy similarity links are hints only.
use crate::{
    auth::{self, Permission, Session},
    content::{PageSize, valid_id, valid_language},
    error::AppError,
    taxonomy::{Tag, Taxonomy},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Active,
    Closed,
}
impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Closed => "closed",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub story_id: String,
    pub update_at: Option<DateTime<Utc>>,
    pub added_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub title: String,
    pub state: State,
    pub merged_into: Option<String>,
    pub version: i64,
    pub members: Vec<Member>,
}
#[derive(sqlx::FromRow)]
struct Stored {
    id: String,
    title: String,
    state: String,
    merged_into: Option<String>,
    version: i64,
    members: Json<Vec<Member>>,
}
#[derive(Debug)]
pub struct Mutation {
    pub action_id: i64,
    pub threads: Vec<Snapshot>,
}
pub struct MemberInput {
    pub story_id: String,
    pub update_at: DateTime<Utc>,
}
pub struct Merge<'a> {
    pub source: &'a str,
    pub source_version: i64,
    pub target: &'a str,
    pub target_version: i64,
}
pub struct Split<'a> {
    pub source: &'a str,
    pub source_version: i64,
    pub new_id: &'a str,
    pub title: &'a str,
    pub members: &'a [String],
}
#[derive(Debug, sqlx::FromRow)]
pub struct Action {
    pub id: i64,
    pub actor_id: String,
    pub action: String,
    pub reason: String,
    pub undo_of: Option<i64>,
    pub created_at: DateTime<Utc>,
}
#[derive(Clone)]
pub struct Threads {
    pool: PgPool,
}
type Tx = Transaction<'static, Postgres>;
fn check_id(id: &str) -> Result<(), AppError> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid thread/story ID."))
    }
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}
fn reason(value: &str) -> Result<(), AppError> {
    if text(value, 2000) {
        Ok(())
    } else {
        Err(AppError::Validation("Invalid thread change reason."))
    }
}
fn next(version: i64) -> Result<i64, AppError> {
    version.checked_add(1).ok_or(AppError::Conflict)
}
fn canonical(s: &Snapshot) -> Result<(), AppError> {
    if s.merged_into.is_none() {
        Ok(())
    } else {
        Err(AppError::Conflict)
    }
}
fn expect(s: &Snapshot, version: i64) -> Result<(), AppError> {
    if s.version == version && version >= 0 {
        Ok(())
    } else {
        Err(AppError::Conflict)
    }
}
fn db(e: sqlx::Error) -> AppError {
    AppError::database("story_threads", e)
}
async fn load(tx: &mut Tx, id: &str, lock: bool) -> Result<Snapshot, AppError> {
    check_id(id)?;
    let suffix = if lock { " FOR UPDATE" } else { "" };
    let sql = format!(
        r#"SELECT id,title,state,merged_into,lock_version AS version,
        (SELECT COALESCE(jsonb_agg(jsonb_build_object('story_id',m.story_id,'update_at',m.update_at,'added_at',m.added_at) ORDER BY m.story_id),'[]'::jsonb) FROM news.thread_stories m WHERE m.thread_id=t.id) AS members
        FROM news.threads t WHERE id=$1{suffix}"#
    );
    let r: Stored = sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(AppError::NotFound)?;
    let state = match r.state.as_str() {
        "active" => State::Active,
        "closed" => State::Closed,
        _ => return Err(AppError::InvalidData),
    };
    if r.members.0.len() > 200 {
        return Err(AppError::InvalidData);
    }
    Ok(Snapshot {
        id: r.id,
        title: r.title,
        state,
        merged_into: r.merged_into,
        version: r.version,
        members: r.members.0,
    })
}
impl Threads {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    async fn begin(&self, session: &Session) -> Result<(Tx, String), AppError> {
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
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('world-news:thread-curation',0))",
        )
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        Ok((tx, actor.id().into()))
    }
    pub async fn get(&self, session: &Session, id: &str) -> Result<Snapshot, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let s = load(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(s)
    }
    pub async fn history(
        &self,
        session: &Session,
        id: &str,
        before: Option<i64>,
        size: PageSize,
    ) -> Result<Vec<Action>, AppError> {
        check_id(id)?;
        if before.is_some_and(|i| i <= 0) {
            return Err(AppError::Validation("Invalid action cursor."));
        }
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let rows=sqlx::query_as("SELECT id,actor_id,action,reason,undo_of,created_at FROM news.thread_actions WHERE after_state @> $1::jsonb AND ($2::bigint IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(Json(serde_json::json!([{"id":id}]))).bind(before).bind(i64::from(size.get())).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
    pub async fn create(
        &self,
        session: &Session,
        id: &str,
        title: &str,
        why: &str,
    ) -> Result<Mutation, AppError> {
        check_id(id)?;
        reason(why)?;
        if !text(title, 500) {
            return Err(AppError::Validation("Invalid thread title."));
        }
        let (tx, actor) = self.begin(session).await?;
        let s = Snapshot {
            id: id.into(),
            title: title.into(),
            state: State::Active,
            merged_into: None,
            version: 1,
            members: vec![],
        };
        save(tx, &actor, "created", why, vec![], vec![s], None).await
    }
    pub async fn edit(
        &self,
        session: &Session,
        id: &str,
        version: i64,
        title: &str,
        state: State,
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        if !text(title, 500) {
            return Err(AppError::Validation("Invalid thread title."));
        }
        let (mut tx, actor) = self.begin(session).await?;
        let old = load(&mut tx, id, true).await?;
        expect(&old, version)?;
        canonical(&old)?;
        let mut new = old.clone();
        new.title = title.into();
        new.state = state;
        new.version = next(old.version)?;
        save(tx, &actor, "edited", why, vec![old], vec![new], None).await
    }
    pub async fn assign(
        &self,
        session: &Session,
        id: &str,
        version: i64,
        input: MemberInput,
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        check_id(&input.story_id)?;
        let (mut tx, actor) = self.begin(session).await?;
        let old = load(&mut tx, id, true).await?;
        expect(&old, version)?;
        canonical(&old)?;
        if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM news.stories WHERE id=$1)")
            .bind(&input.story_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?
        {
            return Err(AppError::NotFound);
        }
        let owner: Option<String> =
            sqlx::query_scalar("SELECT thread_id FROM news.thread_stories WHERE story_id=$1")
                .bind(&input.story_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if owner.is_some_and(|owner| owner != id) {
            return Err(AppError::Conflict);
        }
        let mut new = old.clone();
        new.version = next(old.version)?;
        let instant = DateTime::from_timestamp_micros(input.update_at.timestamp_micros())
            .ok_or(AppError::Validation("Invalid update instant."))?;
        match new
            .members
            .iter_mut()
            .find(|m| m.story_id == input.story_id)
        {
            Some(m) => m.update_at = Some(instant),
            None => {
                let now = sqlx::query_scalar("SELECT clock_timestamp()")
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?;
                new.members.push(Member {
                    story_id: input.story_id,
                    update_at: Some(instant),
                    added_at: now,
                });
            }
        }
        save(tx, &actor, "member", why, vec![old], vec![new], None).await
    }
    pub async fn remove(
        &self,
        session: &Session,
        id: &str,
        version: i64,
        story: &str,
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        check_id(story)?;
        let (mut tx, actor) = self.begin(session).await?;
        let old = load(&mut tx, id, true).await?;
        expect(&old, version)?;
        canonical(&old)?;
        if !old.members.iter().any(|m| m.story_id == story) {
            return Err(AppError::NotFound);
        }
        let mut new = old.clone();
        new.members.retain(|m| m.story_id != story);
        new.version = next(old.version)?;
        save(tx, &actor, "removed", why, vec![old], vec![new], None).await
    }
    pub async fn merge(
        &self,
        session: &Session,
        input: Merge<'_>,
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        if input.source == input.target {
            return Err(AppError::Validation("Cannot merge a thread into itself."));
        }
        let (mut tx, actor) = self.begin(session).await?;
        let source = load(&mut tx, input.source, true).await?;
        let target = load(&mut tx, input.target, true).await?;
        expect(&source, input.source_version)?;
        expect(&target, input.target_version)?;
        canonical(&source)?;
        canonical(&target)?;
        if source.state != State::Active || target.state != State::Active {
            return Err(AppError::Conflict);
        }
        let depth:i32=sqlx::query_scalar(r#"WITH RECURSIVE incoming AS (
            SELECT id,0 AS depth FROM news.threads WHERE id=$1 UNION ALL
            SELECT t.id,i.depth+1 FROM incoming i JOIN news.threads t ON t.merged_into=i.id WHERE i.depth<20
        ) SELECT COALESCE(max(depth),0) FROM incoming"#).bind(input.source).fetch_one(&mut *tx).await.map_err(db)?;
        if depth >= 20 {
            return Err(AppError::Validation("Thread alias depth limit reached."));
        }
        let mut from = source.clone();
        let mut to = target.clone();
        to.members.append(&mut from.members);
        to.version = next(to.version)?;
        from.state = State::Closed;
        from.merged_into = Some(to.id.clone());
        from.version = next(from.version)?;
        save(
            tx,
            &actor,
            "merged",
            why,
            vec![source, target],
            vec![from, to],
            None,
        )
        .await
    }
    pub async fn split(
        &self,
        session: &Session,
        input: Split<'_>,
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        check_id(input.new_id)?;
        let ids: BTreeSet<_> = input.members.iter().collect();
        if input.source == input.new_id
            || ids.is_empty()
            || ids.len() != input.members.len()
            || ids.len() > 200
            || !text(input.title, 500)
        {
            return Err(AppError::Validation("Invalid thread split."));
        }
        let (mut tx, actor) = self.begin(session).await?;
        let old = load(&mut tx, input.source, true).await?;
        expect(&old, input.source_version)?;
        canonical(&old)?;
        if old.state != State::Active
            || ids.len() >= old.members.len()
            || ids
                .iter()
                .any(|id| !old.members.iter().any(|m| &m.story_id == *id))
        {
            return Err(AppError::Validation(
                "Split must select existing members and leave members behind.",
            ));
        }
        let mut remaining = old.clone();
        let mut moved = vec![];
        remaining.members.retain(|m| {
            if ids.contains(&m.story_id) {
                moved.push(m.clone());
                false
            } else {
                true
            }
        });
        remaining.version = next(old.version)?;
        let new = Snapshot {
            id: input.new_id.into(),
            title: input.title.into(),
            state: State::Active,
            merged_into: None,
            version: 1,
            members: moved,
        };
        save(
            tx,
            &actor,
            "split",
            why,
            vec![old],
            vec![remaining, new],
            None,
        )
        .await
    }
    pub async fn undo(
        &self,
        session: &Session,
        action_id: i64,
        versions: &[(String, i64)],
        why: &str,
    ) -> Result<Mutation, AppError> {
        reason(why)?;
        if action_id <= 0 || versions.is_empty() || versions.len() > 2 {
            return Err(AppError::Validation("Invalid inverse action request."));
        }
        if versions
            .iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            .len()
            != versions.len()
        {
            return Err(AppError::Validation("Duplicate thread inverse version."));
        }
        let versions: BTreeMap<_, _> = versions.iter().map(|(id, v)| (id.as_str(), *v)).collect();
        let (mut tx, actor) = self.begin(session).await?;
        let (Json(before), Json(after)): (Json<Vec<Snapshot>>, Json<Vec<Snapshot>>) =
            sqlx::query_as("SELECT before_state,after_state FROM news.thread_actions WHERE id=$1")
                .bind(action_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or(AppError::NotFound)?;
        if versions.len() != after.len()
            || sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM news.thread_actions WHERE undo_of=$1)",
            )
            .bind(action_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?
        {
            return Err(AppError::Conflict);
        }
        let mut current = vec![];
        let mut restored = vec![];
        for recorded in after {
            let s = load(&mut tx, &recorded.id, true).await?;
            if s != recorded || versions.get(s.id.as_str()) != Some(&s.version) {
                return Err(AppError::Conflict);
            }
            let mut old = before
                .iter()
                .find(|b| b.id == s.id)
                .cloned()
                .unwrap_or_else(|| {
                    let mut tombstone = s.clone();
                    tombstone.members.clear();
                    tombstone.state = State::Closed;
                    tombstone.merged_into = None;
                    tombstone
                });
            old.version = next(s.version)?;
            current.push(s);
            restored.push(old);
        }
        save(
            tx,
            &actor,
            "undone",
            why,
            current,
            restored,
            Some(action_id),
        )
        .await
    }
}
async fn save(
    mut tx: Tx,
    actor: &str,
    action: &str,
    why: &str,
    mut before: Vec<Snapshot>,
    mut after: Vec<Snapshot>,
    undo: Option<i64>,
) -> Result<Mutation, AppError> {
    before.sort_by(|a, b| a.id.cmp(&b.id));
    after.sort_by(|a, b| a.id.cmp(&b.id));
    let mut seen = BTreeSet::new();
    for s in &mut after {
        if s.members.len() > 200 {
            return Err(AppError::Validation("Thread membership exceeds 200."));
        }
        s.members.sort_by(|a, b| a.story_id.cmp(&b.story_id));
        if s.members.iter().any(|m| !seen.insert(m.story_id.clone())) {
            return Err(AppError::Conflict);
        }
    }
    for s in &before {
        sqlx::query("DELETE FROM news.thread_stories WHERE thread_id=$1")
            .bind(&s.id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
    }
    for s in &after {
        if let Some(old) = before.iter().find(|b| b.id == s.id) {
            let affected=sqlx::query("UPDATE news.threads SET title=$2,state=$3,merged_into=$4,lock_version=$5 WHERE id=$1 AND lock_version=$6")
                .bind(&s.id).bind(&s.title).bind(s.state.as_str()).bind(&s.merged_into).bind(s.version).bind(old.version).execute(&mut *tx).await.map_err(db)?.rows_affected();
            if affected != 1 {
                return Err(AppError::Conflict);
            }
        } else {
            sqlx::query("INSERT INTO news.threads(id,title,state,merged_into,lock_version) VALUES($1,$2,$3,$4,$5)").bind(&s.id).bind(&s.title).bind(s.state.as_str()).bind(&s.merged_into).bind(s.version).execute(&mut *tx).await.map_err(db)?;
        }
    }
    for s in &after {
        if !s.members.is_empty() {
            sqlx::query("INSERT INTO news.thread_stories(thread_id,story_id,update_at,added_at) SELECT $1,m.story_id,m.update_at,m.added_at FROM jsonb_to_recordset($2::jsonb) AS m(story_id text,update_at timestamptz,added_at timestamptz)")
                .bind(&s.id).bind(Json(&s.members)).execute(&mut *tx).await.map_err(db)?;
        }
    }
    let action_id=sqlx::query_scalar("INSERT INTO news.thread_actions(actor_id,action,reason,before_state,after_state,undo_of) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(actor).bind(action).bind(why).bind(Json(before)).bind(Json(&after)).bind(undo).fetch_one(&mut *tx).await.map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(Mutation {
        action_id,
        threads: after,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub update_at: Option<DateTime<Utc>>,
    pub id: String,
}
#[derive(Default)]
pub struct Filter {
    pub state: Option<State>,
    pub language: Option<String>,
    pub topic: Option<String>,
    pub geography: Option<String>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct Head {
    pub thread_id: String,
    pub thread_title: String,
    pub state: String,
    pub head_story_id: String,
    pub head_revision_id: String,
    pub head_title: String,
    pub update_at: Option<DateTime<Utc>>,
    pub public_story_count: i64,
    pub edition_count: i64,
    pub date_count: i64,
}
#[derive(Debug, sqlx::FromRow)]
pub struct PublicMember {
    pub thread_id: String,
    pub thread_title: String,
    pub state: String,
    pub story_id: String,
    pub revision_id: String,
    pub story_title: String,
    pub language: String,
    pub topics: Vec<String>,
    pub geography: Vec<String>,
    pub update_at: Option<DateTime<Utc>>,
}
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<Cursor>,
}
fn filter_tags(value: Option<&str>, tags: &[Tag]) -> Result<Option<Vec<String>>, AppError> {
    value
        .map(|id| {
            let t = tags
                .iter()
                .find(|t| t.id == id)
                .ok_or(AppError::Validation(
                    "Invalid thread classification filter.",
                ))?;
            let mut values = vec![t.id.clone()];
            values.extend(t.aliases.iter().cloned());
            Ok(values)
        })
        .transpose()
}
impl Threads {
    /// Public queries require only reviewed views/function grants, never editor tables.
    pub async fn resolve(&self, id: &str) -> Result<String, AppError> {
        check_id(id)?;
        let resolved: Option<String> = sqlx::query_scalar("SELECT news.resolve_public_thread($1)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        resolved.ok_or(AppError::NotFound)
    }
    pub async fn heads(
        &self,
        filter: &Filter,
        cursor: Option<&Cursor>,
        size: PageSize,
        policy: &Taxonomy,
    ) -> Result<Page<Head>, AppError> {
        if let Some(c) = cursor {
            check_id(&c.id)?;
        }
        if filter
            .language
            .as_deref()
            .is_some_and(|l| !valid_language(l))
        {
            return Err(AppError::Validation("Invalid thread language filter."));
        }
        let topics = filter_tags(filter.topic.as_deref(), &policy.topics)?;
        let geography = filter_tags(filter.geography.as_deref(), &policy.geography)?;
        let mut items:Vec<Head>=sqlx::query_as(r#"SELECT h.* FROM news.public_thread_heads h WHERE
            ($1::text IS NULL OR ($2::timestamptz IS NULL AND h.update_at IS NULL AND h.thread_id>$1)
             OR ($2::timestamptz IS NOT NULL AND (h.update_at<$2 OR h.update_at IS NULL OR (h.update_at=$2 AND h.thread_id>$1))))
            AND ($3::text IS NULL OR h.state=$3)
            AND EXISTS(SELECT 1 FROM news.public_thread_members m WHERE m.thread_id=h.thread_id
                AND ($4::text IS NULL OR lower(m.language)=lower($4))
                AND ($5::text[] IS NULL OR EXISTS(SELECT 1 FROM unnest(m.topics) tag WHERE lower(btrim(tag))=ANY($5)))
                AND ($6::text[] IS NULL OR EXISTS(SELECT 1 FROM unnest(m.geography) tag WHERE lower(btrim(tag))=ANY($6))))
            ORDER BY h.update_at DESC NULLS LAST,h.thread_id LIMIT $7"#)
            .bind(cursor.map(|c|c.id.as_str())).bind(cursor.and_then(|c|c.update_at))
            .bind(filter.state.map(State::as_str)).bind(&filter.language).bind(topics).bind(geography)
            .bind(i64::from(size.get())+1).fetch_all(&self.pool).await.map_err(db)?;
        let more = items.len() > size.get() as usize;
        items.truncate(size.get() as usize);
        let next = if more {
            items.last().map(|h| Cursor {
                update_at: h.update_at,
                id: h.thread_id.clone(),
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }
    pub async fn timeline(
        &self,
        id: &str,
        cursor: Option<&Cursor>,
        size: PageSize,
    ) -> Result<Page<PublicMember>, AppError> {
        if let Some(c) = cursor {
            check_id(&c.id)?;
        }
        check_id(id)?;
        // Resolve aliases and members from one repeatable public snapshot.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let id: Option<String> = sqlx::query_scalar("SELECT news.resolve_public_thread($1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let id = id.ok_or(AppError::NotFound)?;
        let mut items:Vec<PublicMember>=sqlx::query_as(r#"SELECT m.* FROM news.public_thread_members m WHERE m.thread_id=$1 AND
            ($2::text IS NULL OR ($3::timestamptz IS NULL AND m.update_at IS NULL AND m.story_id>$2)
             OR ($3::timestamptz IS NOT NULL AND (m.update_at<$3 OR m.update_at IS NULL OR (m.update_at=$3 AND m.story_id>$2))))
            ORDER BY m.update_at DESC NULLS LAST,m.story_id LIMIT $4"#)
            .bind(id).bind(cursor.map(|c|c.id.as_str())).bind(cursor.and_then(|c|c.update_at))
            .bind(i64::from(size.get())+1).fetch_all(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        let more = items.len() > size.get() as usize;
        items.truncate(size.get() as usize);
        let next = if more {
            items.last().map(|m| Cursor {
                update_at: m.update_at,
                id: m.story_id.clone(),
            })
        } else {
            None
        };
        Ok(Page { items, next })
    }
}
