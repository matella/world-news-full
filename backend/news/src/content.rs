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

/// Public unauthenticated URL fields reject known credential parameter names.
/// This cannot prove arbitrary paths/query values contain no secret.
pub(crate) fn public_url_parameters(url: &url::Url) -> bool {
    !url.query_pairs().any(|(key, _)| {
        let key = key.to_ascii_lowercase().replace('-', "_");
        key == "key"
            || key == "auth"
            || key == "sid"
            || key == "session"
            || key.ends_with("_key")
            || key == "sig"
            || [
                "token",
                "secret",
                "password",
                "authorization",
                "credential",
                "cookie",
                "signature",
                "api_key",
                "apikey",
            ]
            .iter()
            .any(|part| key.contains(part))
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SectionRole {
    Context,
    Uncertainty,
}
impl SectionRole {
    pub fn key(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Uncertainty => "uncertainty",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Context => "Contexte",
            Self::Uncertainty => "Incertitudes et limites",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SectionBlock {
    Paragraph { text: String },
    Claim { id: String, text: String },
}
impl SectionBlock {
    pub fn text(&self) -> &str {
        match self {
            Self::Paragraph { text } | Self::Claim { text, .. } => text,
        }
    }
    fn claim_id(&self) -> Option<&str> {
        match self {
            Self::Claim { id, .. } => Some(id),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContentBlock {
    Paragraph {
        text: String,
    },
    Heading {
        text: String,
        level: u8,
    },
    Claim {
        id: String,
        text: String,
    },
    Section {
        role: SectionRole,
        blocks: Vec<SectionBlock>,
    },
}
impl ContentBlock {
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        let text = match self {
            Self::Paragraph { text } | Self::Heading { text, .. } | Self::Claim { text, .. } => {
                Some(text.as_str())
            }
            Self::Section { .. } => None,
        };
        let children: &[SectionBlock] = match self {
            Self::Section { blocks, .. } => blocks,
            _ => &[],
        };
        text.into_iter()
            .chain(children.iter().map(SectionBlock::text))
    }
    pub fn has_prose(&self) -> bool {
        match self {
            Self::Heading { .. } => false,
            Self::Paragraph { text } | Self::Claim { text, .. } => !text.trim().is_empty(),
            Self::Section { blocks, .. } => blocks.iter().any(|b| !b.text().trim().is_empty()),
        }
    }
    pub fn claim_ids(&self) -> impl Iterator<Item = &str> {
        let id = match self {
            Self::Claim { id, .. } => Some(id.as_str()),
            _ => None,
        };
        let children: &[SectionBlock] = match self {
            Self::Section { blocks, .. } => blocks,
            _ => &[],
        };
        id.into_iter()
            .chain(children.iter().filter_map(SectionBlock::claim_id))
    }
}
pub fn valid_claim_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub fn validate_blocks(blocks: &[ContentBlock]) -> bool {
    let count: usize = blocks
        .iter()
        .map(|b| match b {
            ContentBlock::Section { blocks, .. } => 1 + blocks.len(),
            _ => 1,
        })
        .sum();
    let bytes: usize = blocks
        .iter()
        .flat_map(ContentBlock::texts)
        .map(str::len)
        .sum();
    let mut ids = std::collections::BTreeSet::new();
    let mut roles = std::collections::BTreeSet::new();
    count <= 2000
        && bytes <= 1_048_576
        && blocks
            .iter()
            .flat_map(ContentBlock::texts)
            .all(|t| !t.contains('\0'))
        && blocks
            .iter()
            .flat_map(ContentBlock::claim_ids)
            .all(|id| valid_claim_id(id) && ids.insert(id))
        && blocks.iter().all(|block| match block {
            ContentBlock::Paragraph { .. } => true,
            ContentBlock::Heading { text, level } => {
                (2..=3).contains(level) && !text.trim().is_empty() && text.chars().count() <= 500
            }
            ContentBlock::Claim { text, .. } => !text.trim().is_empty(),
            ContentBlock::Section { role, blocks } => {
                roles.insert(*role)
                    && !blocks.is_empty()
                    && blocks.iter().any(|b| !b.text().trim().is_empty())
                    && blocks.iter().all(|b| match b {
                        SectionBlock::Claim { text, .. } => !text.trim().is_empty(),
                        _ => true,
                    })
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Brief => "Brève",
            Self::Standard => "Article",
            Self::Analysis => "Article long",
        }
    }
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Reporting => "Information",
            Self::Analysis => "Analyse",
            Self::Opinion => "Opinion",
            Self::AutomatedSummary => "Résumé automatisé",
            Self::LegacySummary => "Résumé historique",
            Self::LegacySynthesis => "Synthèse historique",
        }
    }
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
impl Revision {
    /// Explicit authored overview only; never manufacture one by slicing full prose.
    pub fn overview(&self) -> Option<&str> {
        self.summary
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.standfirst.as_deref().filter(|s| !s.trim().is_empty()))
    }
    pub fn visible_standfirst(&self) -> Option<&str> {
        self.standfirst.as_deref().filter(|s| {
            !s.trim().is_empty()
                && !self
                    .body
                    .iter()
                    .flat_map(ContentBlock::texts)
                    .any(|t| t.trim() == s.trim())
        })
    }
    pub fn visible_summary(&self) -> Option<&str> {
        self.summary.as_deref().filter(|s| {
            !s.trim().is_empty()
                && self
                    .standfirst
                    .as_deref()
                    .is_none_or(|intro| intro.trim() != s.trim())
                && !self
                    .body
                    .iter()
                    .flat_map(ContentBlock::texts)
                    .any(|t| t.trim() == s.trim())
        })
    }
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

#[cfg(test)]
mod structured_tests {
    use super::*;
    #[test]
    fn anchors_and_sections_round_trip_without_active_markup() {
        let value = serde_json::json!([
            {"type":"claim","id":"fact-1","text":"Untrusted <script>alert(1)</script> stays text."},
            {"type":"section","role":"context","blocks":[{"type":"paragraph","text":"Explicit context."}]},
            {"type":"section","role":"uncertainty","blocks":[{"type":"claim","id":"limit-1","text":"Explicit attributed uncertainty."}]}
        ]);
        let blocks: Vec<ContentBlock> = serde_json::from_value(value.clone()).unwrap();
        assert!(validate_blocks(&blocks));
        assert_eq!(serde_json::to_value(&blocks).unwrap(), value);
        assert_eq!(
            blocks
                .iter()
                .flat_map(ContentBlock::claim_ids)
                .collect::<Vec<_>>(),
            vec!["fact-1", "limit-1"]
        );
        for bad in [
            serde_json::json!({"type":"html","html":"<script>bad()</script>"}),
            serde_json::json!({"type":"paragraph","text":"plain","style":"color:red"}),
            serde_json::json!({"type":"section","role":"context","blocks":[{"type":"section","role":"context","blocks":[]}]}),
        ] {
            assert!(serde_json::from_value::<ContentBlock>(bad).is_err());
        }
    }
    #[test]
    fn duplicate_anchors_and_ambiguous_empty_sections_are_rejected() {
        let duplicate = vec![
            ContentBlock::Claim {
                id: "fact".into(),
                text: "One".into(),
            },
            ContentBlock::Section {
                role: SectionRole::Context,
                blocks: vec![SectionBlock::Claim {
                    id: "fact".into(),
                    text: "Two".into(),
                }],
            },
        ];
        assert!(!validate_blocks(&duplicate));
        assert!(!validate_blocks(&[ContentBlock::Claim {
            id: "x\" onload=bad()".into(),
            text: "Text".into()
        }]));
        assert!(!validate_blocks(&[ContentBlock::Section {
            role: SectionRole::Uncertainty,
            blocks: vec![]
        }]));
        let section = ContentBlock::Section {
            role: SectionRole::Context,
            blocks: vec![SectionBlock::Paragraph {
                text: "Entered context".into(),
            }],
        };
        assert!(!validate_blocks(&[section.clone(), section]));
        assert!(
            !ContentBlock::Heading {
                text: "Only a heading".into(),
                level: 2
            }
            .has_prose()
        );
    }
    #[test]
    fn nested_text_and_block_limits_apply_to_the_whole_body() {
        assert!(!validate_blocks(&[ContentBlock::Section {
            role: SectionRole::Context,
            blocks: vec![SectionBlock::Paragraph {
                text: "x".repeat(1_048_577)
            }]
        }]));
        assert!(!validate_blocks(&[ContentBlock::Section {
            role: SectionRole::Context,
            blocks: vec![SectionBlock::Paragraph { text: "x".into() }; 2000]
        }]));
        let old = serde_json::json!([{"type":"paragraph","text":"# Literal legacy heading\n<script>source text</script>"}]);
        let blocks: Vec<ContentBlock> = serde_json::from_value(old.clone()).unwrap();
        assert!(validate_blocks(&blocks));
        assert_eq!(serde_json::to_value(blocks).unwrap(), old);
    }
}
