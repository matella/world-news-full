//! Bounded, offline planning and explicit transactional legacy import.
use crate::{content::*, error::AppError, persistence};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, types::Json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct RecordIssue {
    pub row: usize,
    pub id: Option<String>,
    pub code: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct Mapping {
    pub row: usize,
    pub id: String,
    pub revision_id: String,
    pub disposition: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub dry_run: bool,
    pub database_checked: bool,
    pub input_hash: String,
    pub publication_timezone: String,
    pub input_rows: usize,
    pub validated_rows: usize,
    pub duplicates: usize,
    pub inserted: usize,
    pub already_imported: usize,
    pub editions_created: usize,
    pub quarantined: Vec<RecordIssue>,
    pub warnings: Vec<RecordIssue>,
    pub mappings: Vec<Mapping>,
}
pub struct ImportPlan {
    pub report: ImportReport,
    stories: Vec<PlannedStory>,
}
struct PlannedStory {
    row: usize,
    id: String,
    title: String,
    body: Vec<ContentBlock>,
    language: String,
    topics: Vec<String>,
    date: NaiveDate,
    updated_at: DateTime<Utc>,
    origins: Option<u32>,
    kind: ArticleKind,
    format: ArticleFormat,
    hash: String,
    revision_id: String,
    payload: Value,
    sources: Vec<LegacySource>,
    related: Vec<String>,
    translation: Option<LegacyTranslation>,
}
struct LegacySource {
    name: String,
    url: String,
    homepage: String,
    source_id: String,
    article_id: String,
    key: String,
}
struct LegacyTranslation {
    title: String,
    body: Vec<ContentBlock>,
    disagreements: Value,
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort_unstable();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&map[key])))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        other => other.clone(),
    }
}
fn contains_nul(value: &Value) -> bool {
    match value {
        Value::String(text) => text.contains('\0'),
        Value::Array(values) => values.iter().any(contains_nul),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.contains('\0') || contains_nul(value)),
        _ => false,
    }
}
fn unsupported_number(value: &Value) -> bool {
    match value {
        Value::Number(number) => {
            let text = number.to_string();
            text.len() > 512
                || text.split_once(['e', 'E']).is_some_and(|(_, exponent)| {
                    exponent
                        .parse::<i32>()
                        .map_or(true, |n| n.unsigned_abs() > 308)
                })
        }
        Value::Array(values) => values.iter().any(unsupported_number),
        Value::Object(values) => values.values().any(unsupported_number),
        _ => false,
    }
}
fn hash(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(canonical(value).to_string().as_bytes())
    )
}
fn text_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn issue(row: usize, id: Option<&str>, code: &'static str) -> RecordIssue {
    RecordIssue {
        row,
        id: id.map(str::to_owned),
        code,
    }
}
fn optional_text<'a>(row: &'a Value, key: &str) -> Result<Option<&'a str>, &'static str> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        _ => Err("invalid_text_field"),
    }
}
fn optional_count(row: &Value, key: &str) -> Result<Option<u32>, &'static str> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or("invalid_source_count"),
    }
}
fn blocks(text: &str) -> Vec<ContentBlock> {
    text.split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|text| ContentBlock::Paragraph {
            text: text.to_owned(),
        })
        .collect()
}
fn array<'a>(row: &'a Value, key: &str) -> Result<&'a [Value], &'static str> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        _ => Err("invalid_array_field"),
    }
}

pub fn plan_file(path: &Path, timezone: &str) -> Result<ImportPlan, AppError> {
    let file = std::fs::File::open(path)
        .map_err(|_| AppError::Validation("Cannot read legacy export."))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::Validation("Cannot read legacy export."))?;
    plan_bytes(&bytes, timezone)
}

pub fn plan_bytes(bytes: &[u8], timezone: &str) -> Result<ImportPlan, AppError> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(AppError::Validation(
            "Legacy export exceeds 32 MiB; split by complete editions.",
        ));
    }
    if timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(AppError::Validation("Invalid import timezone."));
    }
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::Validation("Malformed legacy JSON export."))?;
    let rows = root
        .get("stories")
        .and_then(Value::as_array)
        .ok_or(AppError::Validation(
            "Legacy export requires a stories array.",
        ))?;
    if rows.len() > 10_000 {
        return Err(AppError::Validation(
            "Legacy export exceeds 10000 rows; split by complete editions.",
        ));
    }
    let corrections = array(&root, "corrections")
        .map_err(|_| AppError::Validation("Invalid corrections metadata."))?;
    let mut report = ImportReport {
        dry_run: true,
        database_checked: false,
        input_hash: hash(&root),
        publication_timezone: timezone.into(),
        input_rows: rows.len(),
        validated_rows: 0,
        duplicates: 0,
        inserted: 0,
        already_imported: 0,
        editions_created: 0,
        quarantined: vec![],
        warnings: vec![],
        mappings: vec![],
    };
    let mut grouped: BTreeMap<String, Vec<PlannedStory>> = BTreeMap::new();
    let mut hashes_by_id: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        if let Some(id) = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        {
            hashes_by_id.entry(id).or_default().insert(hash(row));
        }
    }
    for (index, row) in rows.iter().enumerate() {
        let number = index + 1;
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id));
        if id.is_some_and(|id| hashes_by_id[id].len() > 1) {
            report
                .quarantined
                .push(issue(number, id, "conflicting_duplicate_id"));
            continue;
        }
        let metadata: Vec<_> = corrections
            .iter()
            .filter(|c| c.get("story_id").and_then(Value::as_str) == id && id.is_some())
            .cloned()
            .collect();
        match parse_row(number, row, &metadata) {
            Ok((story, warnings)) => {
                report.warnings.extend(
                    warnings
                        .into_iter()
                        .map(|code| issue(number, Some(&story.id), code)),
                );
                grouped.entry(story.id.clone()).or_default().push(story);
            }
            Err(code) => report.quarantined.push(issue(number, id, code)),
        }
    }
    let mut stories = vec![];
    for (_, mut group) in grouped {
        if group.iter().any(|row| row.hash != group[0].hash) {
            report.quarantined.extend(
                group
                    .iter()
                    .map(|row| issue(row.row, Some(&row.id), "conflicting_duplicate_id")),
            );
        } else {
            report.duplicates += group.len() - 1;
            stories.push(group.remove(0));
        }
    }
    let mut per_date: BTreeMap<NaiveDate, usize> = BTreeMap::new();
    for story in &stories {
        *per_date.entry(story.date).or_default() += 1;
    }
    stories.retain(|story| {
        if per_date[&story.date] > 200 {
            report
                .quarantined
                .push(issue(story.row, Some(&story.id), "edition_exceeds_limit"));
            false
        } else {
            true
        }
    });
    let ids: BTreeSet<_> = stories.iter().map(|story| story.id.as_str()).collect();
    for correction in corrections {
        if let Some(id) = correction
            .get("story_id")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        {
            if !ids.contains(id) {
                report
                    .warnings
                    .push(issue(0, Some(id), "correction_not_in_export"));
            }
        } else {
            report
                .warnings
                .push(issue(0, None, "unmapped_correction_metadata"));
        }
    }
    for story in &stories {
        if story.related.iter().any(|id| !ids.contains(id.as_str())) {
            report.warnings.push(issue(
                story.row,
                Some(&story.id),
                "related_id_not_in_export",
            ));
        }
        report.mappings.push(Mapping {
            row: story.row,
            id: story.id.clone(),
            revision_id: story.revision_id.clone(),
            disposition: "planned",
        });
    }
    report.validated_rows = stories.len();
    Ok(ImportPlan { report, stories })
}

fn parse_row(
    number: usize,
    row: &Value,
    corrections: &[Value],
) -> Result<(PlannedStory, BTreeSet<&'static str>), &'static str> {
    if contains_nul(row) || corrections.iter().any(contains_nul) {
        return Err("unsupported_nul_character");
    }
    if unsupported_number(row) || corrections.iter().any(unsupported_number) {
        return Err("unsupported_numeric_range");
    }
    let mut warnings = BTreeSet::new();
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or("invalid_id")?
        .to_owned();
    let title = row
        .get("title")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty() && v.chars().count() <= 500)
        .ok_or("invalid_title")?
        .to_owned();
    let body_text = row
        .get("body")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or("missing_body")?;
    let body = blocks(body_text);
    if !validate_blocks(&body) {
        return Err("body_exceeds_limit");
    }
    let language = row
        .get("lang")
        .and_then(Value::as_str)
        .filter(|v| valid_language(v))
        .ok_or("invalid_language")?
        .to_owned();
    let date_text = row
        .get("edition_date")
        .and_then(Value::as_str)
        .ok_or("missing_edition_date")?;
    let date =
        NaiveDate::parse_from_str(date_text, "%Y-%m-%d").map_err(|_| "invalid_edition_date")?;
    if date_text.len() != 10
        || date.format("%Y-%m-%d").to_string() != date_text
        || date_text < "0001-01-01"
    {
        return Err("invalid_edition_date");
    }
    let updated_at = DateTime::parse_from_rfc3339(
        row.get("updated_at")
            .and_then(Value::as_str)
            .ok_or("missing_update_time")?,
    )
    .map_err(|_| "invalid_update_time")?
    .with_timezone(&Utc);
    let origins = optional_count(row, "origin_count")?;
    let count = optional_count(row, "source_count")?;
    if origins
        .zip(count)
        .is_some_and(|(origins, count)| origins > count)
    {
        warnings.insert("inconsistent_legacy_counts");
    }
    let synthesized = match row.get("synthesized") {
        Some(Value::Bool(flag)) => *flag,
        None | Some(Value::Null) => {
            warnings.insert("unknown_synthesis_state");
            false
        }
        _ => return Err("invalid_synthesis_state"),
    };
    let topics = match optional_text(row, "topic")? {
        Some(v) if v.chars().count() > 100 => return Err("invalid_topic"),
        Some(v) if !v.trim().is_empty() => vec![v.to_owned()],
        _ => vec![],
    };
    let mut sources = vec![];
    let mut seen = BTreeSet::new();
    if row.get("sources_json").is_some_and(|v| !v.is_null())
        && row.get("sources").is_some_and(|v| !v.is_null())
        && row["sources_json"] != row["sources"]
    {
        return Err("ambiguous_source_fields");
    }
    let source_key = if row.get("sources_json").is_some_and(|v| !v.is_null()) {
        "sources_json"
    } else {
        "sources"
    };
    let source_values = array(row, source_key)?;
    if source_values.len() > 200 {
        return Err("too_many_sources");
    }
    for value in source_values {
        let link = value
            .get("url")
            .and_then(Value::as_str)
            .filter(|v| safe_source_url(v))
            .ok_or("unsafe_source_url")?;
        let parsed = url::Url::parse(link).map_err(|_| "unsafe_source_url")?;
        let host = parsed.host_str().ok_or("unsafe_source_url")?;
        let name = value
            .get("name")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| {
                warnings.insert("source_name_inferred_from_host");
                host
            });
        if name.chars().count() > 300 {
            return Err("invalid_source_name");
        }
        let key = text_hash(parsed.as_str());
        if !seen.insert(key.clone()) {
            warnings.insert("duplicate_source_reference");
            continue;
        }
        sources.push(LegacySource {
            name: name.into(),
            url: link.into(),
            homepage: parsed.origin().ascii_serialization(),
            source_id: format!("legacy-source-{}", text_hash(host)),
            article_id: format!("legacy-article-{key}"),
            key: format!("legacy-url:{key}"),
        });
    }
    if sources.is_empty() {
        warnings.insert("missing_source_links");
    }
    if count.is_some_and(|count| count as usize != sources.len()) {
        warnings.insert("legacy_count_differs_from_links");
    }
    if !array(row, "claims_json")?.is_empty() {
        warnings.insert("legacy_claims_require_mapping");
    }
    if !array(row, "disagreements_json")?.is_empty() {
        warnings.insert("legacy_disagreements_retained_unverified");
    }
    let mut related = vec![];
    let links = array(row, "related_ids")?;
    if links.len() > 1000 {
        return Err("too_many_related_ids");
    }
    for value in links {
        if let Some(link) = value.as_str().filter(|v| valid_id(v)) {
            if link != id {
                related.push(link.to_owned());
            }
        } else {
            warnings.insert("invalid_related_id_retained_in_original");
        }
    }
    related.sort();
    related.dedup();
    let title_fr = optional_text(row, "title_fr")?.unwrap_or("");
    let body_fr = optional_text(row, "body_fr")?.unwrap_or("");
    let disagreements = array(row, "disagreements_fr")?;
    let valid_disagreements = disagreements.iter().all(|d| {
        d.get("point").and_then(Value::as_str).is_some()
            && d.get("positions")
                .and_then(Value::as_array)
                .is_some_and(|v| v.iter().all(Value::is_string))
    });
    let translation = if !title_fr.is_empty() || !body_fr.is_empty() || !disagreements.is_empty() {
        let translated_body = blocks(body_fr);
        if !validate_blocks(&translated_body) || title_fr.chars().count() > 500 {
            return Err("invalid_translation");
        }
        if title_fr.trim().is_empty() || translated_body.is_empty() {
            warnings.insert("partial_french_translation");
        }
        if !valid_disagreements {
            warnings.insert("unmapped_translated_disagreements");
        }
        if language.eq_ignore_ascii_case("fr") {
            warnings.insert("legacy_french_display_variant");
        }
        Some(LegacyTranslation {
            title: title_fr.into(),
            body: translated_body,
            disagreements: if valid_disagreements {
                Value::Array(disagreements.to_vec())
            } else {
                json!([])
            },
        })
    } else {
        None
    };
    if !corrections.is_empty() {
        warnings.insert("legacy_correction_notes_unverified");
    }
    warnings.insert("legacy_format_inferred");
    let payload = json!({"story":row,"corrections":corrections});
    let fingerprint = hash(&payload);
    Ok((
        PlannedStory {
            row: number,
            id,
            title,
            body,
            language,
            topics,
            date,
            updated_at,
            origins,
            kind: if synthesized {
                ArticleKind::LegacySynthesis
            } else {
                ArticleKind::LegacySummary
            },
            format: if synthesized {
                ArticleFormat::Standard
            } else {
                ArticleFormat::Brief
            },
            revision_id: format!("legacy-{fingerprint}"),
            hash: fingerprint,
            payload,
            sources,
            related,
            translation,
        },
        warnings,
    ))
}

pub async fn apply(pool: &PgPool, plan: &ImportPlan) -> Result<ImportReport, AppError> {
    if !persistence::schema_available(pool).await {
        return Err(AppError::Unavailable);
    }
    let mut report = plan.report.clone();
    report.dry_run = false;
    report.database_checked = true;
    report.mappings.clear();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AppError::database("import_begin", e))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('world-news:legacy-import',0))")
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::database("import_lock", e))?;
    let mut eligible = vec![];
    for story in &plan.stories {
        let existing=sqlx::query_as::<_,(String,String)>("SELECT story_id,revision_id FROM news.legacy_records WHERE legacy_id=$1 AND content_hash=$2")
            .bind(&story.id).bind(&story.hash).fetch_optional(&mut *tx).await.map_err(|e|AppError::database("import_existing",e))?;
        if let Some((id, revision_id)) = existing {
            report.already_imported += 1;
            report.mappings.push(Mapping {
                row: story.row,
                id,
                revision_id,
                disposition: "already_imported",
            });
            continue;
        }
        if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM news.stories WHERE id=$1)")
            .bind(&story.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::database("import_identity", e))?
        {
            report
                .quarantined
                .push(issue(story.row, Some(&story.id), "existing_story_conflict"));
            continue;
        }
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM news.editions WHERE edition_date=$1 AND publication_timezone=$2)")
            .bind(story.date).bind(&report.publication_timezone).fetch_one(&mut *tx).await.map_err(|e|AppError::database("import_edition_conflict",e))? {
            report.quarantined.push(issue(story.row,Some(&story.id),"existing_edition_conflict"));continue;
        }
        eligible.push(story);
    }
    for story in &eligible {
        sqlx::query("INSERT INTO news.stories(id,updated_at) VALUES ($1,$2)")
            .bind(&story.id)
            .bind(story.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("import_story", e))?;
        sqlx::query("INSERT INTO news.story_revisions(id,story_id,number,title,body,language,format,kind,topics,actor_id,provenance,created_at) VALUES ($1,$2,1,$3,$4,$5,$6,$7,$8,'legacy-import',$9,$10)")
            .bind(&story.revision_id).bind(&story.id).bind(&story.title).bind(Json(&story.body)).bind(&story.language).bind(story.format.as_str()).bind(story.kind.as_str()).bind(&story.topics)
            .bind(Json(json!({"pipeline":"legacy_jarvis","review":"unverified","timestamp_semantics":"legacy_updated_at","record_hash":story.hash}))).bind(story.updated_at)
            .execute(&mut *tx).await.map_err(|e|AppError::database("import_revision",e))?;
        for (index, source) in story.sources.iter().enumerate() {
            sqlx::query("INSERT INTO news.sources(id,name,homepage_url) VALUES ($1,$2,$3) ON CONFLICT(id) DO NOTHING")
                .bind(&source.source_id).bind(&source.name).bind(&source.homepage).execute(&mut *tx).await.map_err(|e|AppError::database("import_source",e))?;
            sqlx::query("INSERT INTO news.source_articles(id,source_id,canonical_key,original_url,title,extraction_state) VALUES ($1,$2,$3,$4,'','metadata_only') ON CONFLICT(id) DO NOTHING")
                .bind(&source.article_id).bind(&source.source_id).bind(&source.key).bind(&source.url).execute(&mut *tx).await.map_err(|e|AppError::database("import_source_article",e))?;
            sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id) VALUES ($1,$2,$3)")
                .bind(&story.revision_id).bind(format!("legacy-reference-{index}")).bind(&source.article_id).execute(&mut *tx).await.map_err(|e|AppError::database("import_reference",e))?;
        }
        if let Some(translation) = &story.translation {
            sqlx::query("INSERT INTO news.translations(source_revision_id,language,number,title,body,disagreements,method,review_state,actor_id) VALUES ($1,'fr',1,$2,$3,$4,'legacy','legacy_unverified','legacy-import')")
                .bind(&story.revision_id).bind(&translation.title).bind(Json(&translation.body)).bind(Json(&translation.disagreements))
                .execute(&mut *tx).await.map_err(|e|AppError::database("import_translation",e))?;
        }
        for link in &story.related {
            sqlx::query("INSERT INTO news.legacy_links(story_id,related_legacy_id) VALUES ($1,$2)")
                .bind(&story.id)
                .bind(link)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::database("import_relation", e))?;
        }
        sqlx::query("INSERT INTO news.publication_events(story_id,revision_id,action,public_reason,actor_id) VALUES ($1,$2,'imported','Historical publication imported; original generation/review state unverified.','legacy-import')")
            .bind(&story.id).bind(&story.revision_id).execute(&mut *tx).await.map_err(|e|AppError::database("import_event",e))?;
        sqlx::query("UPDATE news.stories SET state='published',current_revision_id=$1,published_revision_id=$1,lock_version=1 WHERE id=$2")
            .bind(&story.revision_id).bind(&story.id).execute(&mut *tx).await.map_err(|e|AppError::database("import_publication",e))?;
        sqlx::query("INSERT INTO news.legacy_records(legacy_id,content_hash,story_id,revision_id,original) VALUES ($1,$2,$1,$3,$4)")
            .bind(&story.id).bind(&story.hash).bind(&story.revision_id).bind(Json(&story.payload)).execute(&mut *tx).await.map_err(|e|AppError::database("import_record",e))?;
        report.inserted += 1;
        report.mappings.push(Mapping {
            row: story.row,
            id: story.id.clone(),
            revision_id: story.revision_id.clone(),
            disposition: "inserted",
        });
    }
    let mut editions: BTreeMap<NaiveDate, Vec<&PlannedStory>> = BTreeMap::new();
    for story in eligible {
        editions.entry(story.date).or_default().push(story);
    }
    for (date, mut stories) in editions {
        stories.sort_by(|a, b| {
            b.origins
                .unwrap_or(0)
                .cmp(&a.origins.unwrap_or(0))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        if stories.len() > 200 {
            return Err(AppError::Validation(
                "Legacy edition exceeds 200 entries; review before import.",
            ));
        }
        let edition_id = format!(
            "legacy-edition-{date}-{}",
            &text_hash(&report.publication_timezone)[..16]
        );
        sqlx::query(
            "INSERT INTO news.editions(id,edition_date,publication_timezone) VALUES ($1,$2,$3)",
        )
        .bind(&edition_id)
        .bind(date)
        .bind(&report.publication_timezone)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::database("import_edition", e))?;
        for (index, story) in stories.iter().enumerate() {
            sqlx::query("INSERT INTO news.edition_entries(edition_id,story_id,revision_id,section,position,is_lead) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(&edition_id).bind(&story.id).bind(&story.revision_id).bind(story.topics.first().map(String::as_str).unwrap_or("Non classé"))
                .bind(index as i32).bind(index==0).execute(&mut *tx).await.map_err(|e|AppError::database("import_entry",e))?;
        }
        sqlx::query("UPDATE news.editions SET state='published',published_at=now() WHERE id=$1")
            .bind(&edition_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::database("import_snapshot", e))?;
        report.editions_created += 1;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::database("import_commit", e))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &[u8] = include_bytes!("../../../fixtures/legacy-stories.json");

    #[test]
    fn plans_database_incompatible_values_and_oversized_editions_as_quarantine() {
        let mut root: Value = serde_json::from_slice(FIXTURE).unwrap();
        root["stories"][0]["body"] = json!("Text with \0 unsupported code point");
        let plan = plan_bytes(&serde_json::to_vec(&root).unwrap(), "Europe/Brussels").unwrap();
        assert!(
            plan.report
                .quarantined
                .iter()
                .any(|i| i.code == "unsupported_nul_character")
        );
        let mut root: Value = serde_json::from_slice(FIXTURE).unwrap();
        let first = root["stories"][0].clone();
        root["stories"] = Value::Array(
            (0..201)
                .map(|index| {
                    let mut row = first.clone();
                    row["id"] = json!(format!("many-{index}"));
                    row
                })
                .collect(),
        );
        let plan = plan_bytes(&serde_json::to_vec(&root).unwrap(), "Europe/Brussels").unwrap();
        assert_eq!(plan.report.validated_rows, 0);
        assert_eq!(
            plan.report
                .quarantined
                .iter()
                .filter(|i| i.code == "edition_exceeds_limit")
                .count(),
            201
        );
    }
    #[test]
    fn fixture_is_planned_offline_without_rewriting_text_or_leaking_it_in_report() {
        let plan = plan_bytes(FIXTURE, "Europe/Brussels").unwrap();
        assert_eq!(plan.report.validated_rows, 3);
        assert_eq!(plan.report.quarantined.len(), 1);
        assert_eq!(plan.stories[0].id, "legacy-a");
        let report = serde_json::to_string(&plan.report).unwrap();
        assert!(!report.contains("Invented river project"));
        assert!(!report.contains("dispatch.invalid"));
        assert!(
            plan.report
                .warnings
                .iter()
                .any(|i| i.code == "legacy_claims_require_mapping")
        );
    }
    #[test]
    fn rejects_credentials_and_conflicting_duplicate_ids_without_dumping_payloads() {
        let mut root: Value = serde_json::from_slice(FIXTURE).unwrap();
        root["stories"][0]["sources_json"][0]["url"] =
            json!("https://user:synthetic-private@source.invalid");
        let plan = plan_bytes(&serde_json::to_vec(&root).unwrap(), "Europe/Brussels").unwrap();
        assert!(
            plan.report
                .quarantined
                .iter()
                .any(|i| i.code == "unsafe_source_url")
        );
        assert!(
            !serde_json::to_string(&plan.report)
                .unwrap()
                .contains("synthetic-private")
        );
        let mut root: Value = serde_json::from_slice(FIXTURE).unwrap();
        let mut duplicate = root["stories"][0].clone();
        duplicate["title"] = json!("Different title");
        root["stories"].as_array_mut().unwrap().push(duplicate);
        let plan = plan_bytes(&serde_json::to_vec(&root).unwrap(), "Europe/Brussels").unwrap();
        assert_eq!(
            plan.report
                .quarantined
                .iter()
                .filter(|i| i.code == "conflicting_duplicate_id")
                .count(),
            2
        );
    }
    #[test]
    fn canonical_hash_ignores_object_key_order_but_not_changes() {
        let first: Value = serde_json::from_str("{\"a\":1,\"b\":2}").unwrap();
        let second: Value = serde_json::from_str("{\"b\":2,\"a\":1}").unwrap();
        assert_eq!(hash(&first), hash(&second));
        assert_ne!(hash(&first), hash(&json!({"a":2,"b":2})));
        assert!(plan_bytes(b"{}", "Europe/Brussels").is_err());
        assert!(plan_bytes(FIXTURE, "invalid/timezone").is_err());
        let number: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
        assert_eq!(number.to_string(), "123456789012345678901234567890");
        assert!(unsupported_number(
            &serde_json::from_str::<Value>("1e99999").unwrap()
        ));
    }
}
