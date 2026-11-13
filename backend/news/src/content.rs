use crate::error::AppError;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value.chars().count() <= 200
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '/' || c == '\\')
}
pub fn valid_language(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && value
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}
pub fn safe_source_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentBlock {
    Paragraph { text: String },
    Heading { text: String, level: u8 },
}
pub fn validate_blocks(blocks: &[ContentBlock]) -> bool {
    let bytes: usize = blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Paragraph { text } | ContentBlock::Heading { text, .. } => text.len(),
        })
        .sum();
    bytes <= 1_048_576
        && blocks.len() <= 2000
        && blocks.iter().all(|block| match block {
            ContentBlock::Paragraph { text } => text.len() <= 1_048_576 && !text.contains('\0'),
            ContentBlock::Heading { text, level } => {
                (2..=3).contains(level)
                    && !text.trim().is_empty()
                    && text.chars().count() <= 500
                    && !text.contains('\0')
            }
        })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArticleFormat {
    Brief,
    Standard,
    Analysis,
}
impl ArticleFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brief => "brief",
            Self::Standard => "standard",
            Self::Analysis => "analysis",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArticleKind {
    Reporting,
    Analysis,
    Opinion,
    AutomatedSummary,
    LegacySummary,
    LegacySynthesis,
}
impl ArticleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reporting => "reporting",
            Self::Analysis => "analysis",
            Self::Opinion => "opinion",
            Self::AutomatedSummary => "automated_summary",
            Self::LegacySummary => "legacy_summary",
            Self::LegacySynthesis => "legacy_synthesis",
        }
    }
}

#[derive(Debug)]
pub struct Revision {
    pub id: String,
    pub story_id: String,
    pub title: String,
    pub standfirst: Option<String>,
    pub summary: Option<String>,
    pub body: Vec<ContentBlock>,
    pub language: String,
    pub format: ArticleFormat,
    pub kind: ArticleKind,
    pub topics: Vec<String>,
    pub geography: Vec<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, sqlx::FromRow, Deserialize)]
pub(crate) struct RawRevision {
    pub id: String,
    pub story_id: String,
    pub title: String,
    pub standfirst: Option<String>,
    pub summary: Option<String>,
    pub body: serde_json::Value,
    pub language: String,
    pub format: String,
    pub kind: String,
    pub topics: Vec<String>,
    pub geography: Vec<String>,
    pub created_at: DateTime<Utc>,
}
impl TryFrom<RawRevision> for Revision {
    type Error = AppError;
    fn try_from(raw: RawRevision) -> Result<Self, Self::Error> {
        let body: Vec<ContentBlock> =
            serde_json::from_value(raw.body).map_err(|_| AppError::InvalidData)?;
        let format = serde_json::from_value(serde_json::Value::String(raw.format))
            .map_err(|_| AppError::InvalidData)?;
        let kind = serde_json::from_value(serde_json::Value::String(raw.kind))
            .map_err(|_| AppError::InvalidData)?;
        if !valid_id(&raw.id)
            || !valid_id(&raw.story_id)
            || !valid_language(&raw.language)
            || raw.title.trim().is_empty()
            || raw.title.chars().count() > 500
            || !validate_blocks(&body)
        {
            return Err(AppError::InvalidData);
        }
        Ok(Self {
            id: raw.id,
            story_id: raw.story_id,
            title: raw.title,
            standfirst: raw.standfirst,
            summary: raw.summary,
            body,
            language: raw.language,
            format,
            kind,
            topics: raw.topics,
            geography: raw.geography,
            created_at: raw.created_at,
        })
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct EditionSummary {
    pub id: String,
    pub edition_date: NaiveDate,
    pub publication_timezone: String,
    pub published_at: DateTime<Utc>,
}
#[derive(Clone, Debug)]
pub struct EditionCursor {
    pub date: NaiveDate,
    pub id: String,
}
#[derive(Debug)]
pub struct EditionPage {
    pub items: Vec<EditionSummary>,
    pub next: Option<EditionCursor>,
}
#[derive(Debug)]
pub struct EditionEntry {
    pub story_id: String,
    pub revision_id: String,
    pub section: String,
    pub position: i32,
    pub is_lead: bool,
    pub withdrawn: bool,
    pub revision: Option<Revision>,
}
#[derive(Debug, sqlx::FromRow)]
pub struct SourceLink {
    pub source_id: String,
    pub source_name: String,
    pub original_url: String,
}
#[derive(Debug, sqlx::FromRow)]
pub struct DraftHead {
    pub id: String,
    pub state: String,
    pub current_revision_id: Option<String>,
    pub published_revision_id: Option<String>,
    pub lock_version: i64,
}
pub struct DraftInput {
    pub id: String,
    pub title: String,
    pub body: Vec<ContentBlock>,
    pub language: String,
    pub format: ArticleFormat,
    pub kind: ArticleKind,
    pub actor_id: String,
}

#[derive(Clone, Copy, Debug)]
pub struct PageSize(u32);
impl PageSize {
    pub fn new(value: u32) -> Result<Self, AppError> {
        if !(1..=100).contains(&value) {
            return Err(AppError::Validation("Page size must be between 1 and 100."));
        }
        Ok(Self(value))
    }
    pub fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_active_content_shapes_and_unsafe_source_links() {
        assert!(
            serde_json::from_value::<Vec<ContentBlock>>(
                serde_json::json!([{"type":"script","text":"unsafe"}])
            )
            .is_err()
        );
        assert!(!validate_blocks(&[ContentBlock::Heading {
            text: "x".into(),
            level: 1
        }]));
        assert!(!safe_source_url("javascript:alert(1)"));
        assert!(!safe_source_url(
            "https://user:secret@source.invalid/article"
        ));
        assert!(safe_source_url(
            "https://source.invalid/article?q=1#section"
        ));
    }
    #[test]
    fn validates_opaque_ids_language_tags_and_limits() {
        assert!(valid_id("legacy-a"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id("a\nb"));
        assert!(valid_language("fr-BE"));
        assert!(!valid_language("fr_unsafe"));
        assert!(PageSize::new(0).is_err());
        assert!(PageSize::new(101).is_err());
    }
}
