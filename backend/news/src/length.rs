//! Count-only editorial guidance. Never rewrite prose to satisfy a budget.
use crate::{
    content::{ArticleFormat, ContentBlock},
    domain::EditorialPolicy,
    error::AppError,
};
use serde::{Deserialize, Serialize};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

pub const COUNT_POLICY: &str = "word-v1";
/// Unicode letter/number runs with internal apostrophes/hyphens and numeric separators.
pub fn words(text: &str) -> u32 {
    let chars: Vec<char> = text.nfc().collect();
    let word_character = |c: char| c.is_alphanumeric() && c != 'ʼ' && !is_combining_mark(c);
    let mut count = 0;
    let mut active = false;
    for (i, c) in chars.iter().copied().enumerate() {
        if is_combining_mark(c) {
            continue;
        }
        if word_character(c) {
            if !active {
                count += 1;
            }
            active = true;
        } else {
            let next = chars.get(i + 1).copied();
            let previous = i.checked_sub(1).and_then(|n| chars.get(n)).copied();
            let lexical =
                matches!(c, '\'' | '’' | 'ʼ' | '-' | '‐' | '‑') && next.is_some_and(word_character);
            let numeric = matches!(c, '.' | ',' | '\u{a0}' | '\u{202f}')
                && previous.is_some_and(char::is_numeric)
                && next.is_some_and(char::is_numeric);
            if !active || !(lexical || numeric) {
                active = false;
            }
        }
    }
    count
}
pub fn body_words(body: &[ContentBlock]) -> u32 {
    body.iter().flat_map(ContentBlock::texts).map(words).sum()
}
pub fn supported(language: &str) -> bool {
    matches!(
        language
            .split('-')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "fr" | "en" | "nl" | "de"
    )
}
pub fn estimate_minutes(words: u32, words_per_minute: u32) -> Option<u32> {
    (words_per_minute > 0).then(|| words.div_ceil(words_per_minute))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub schema: u32,
    pub count_policy: String,
    pub policy_hash: String,
    pub words: u32,
    pub language: String,
    pub format: ArticleFormat,
    pub min_words: u32,
    pub max_words: u32,
    pub supported: bool,
    pub override_reason: Option<String>,
}
impl Assessment {
    pub fn new(
        body: &[ContentBlock],
        language: &str,
        format: ArticleFormat,
        policy: &EditorialPolicy,
        override_reason: Option<String>,
    ) -> Result<Self, AppError> {
        policy.validate().map_err(|_| AppError::InvalidData)?;
        if override_reason
            .as_ref()
            .is_some_and(|v| v.trim().is_empty() || v.chars().count() > 2000 || v.contains('\0'))
        {
            return Err(AppError::Validation(
                "A public length override needs a nonempty reason of at most 2000 characters.",
            ));
        }
        let budget = policy.budget(language, format);
        Ok(Self {
            schema: 1,
            count_policy: COUNT_POLICY.into(),
            policy_hash: policy.length_fingerprint(),
            words: body_words(body),
            language: language.into(),
            format,
            min_words: budget.min_words,
            max_words: budget.max_words,
            supported: supported(language),
            override_reason,
        })
    }
    pub fn needs_override(&self) -> bool {
        self.words < self.min_words || self.words > self.max_words || !self.supported
    }
    pub fn matches(&self, current: &Self) -> bool {
        self.schema == 1
            && self.count_policy == COUNT_POLICY
            && self.policy_hash == current.policy_hash
            && self.words == current.words
            && self.language == current.language
            && self.format == current.format
            && self.min_words == current.min_words
            && self.max_words == current.max_words
            && self.supported == current.supported
    }
}

#[derive(Debug)]
pub struct Report {
    pub current: Assessment,
    pub recorded: Option<Assessment>,
    pub reading_minutes: u32,
    pub reading_words_per_minute: u32,
    pub warnings: Vec<String>,
}
impl Report {
    pub fn new(
        revision: &crate::content::Revision,
        policy: &EditorialPolicy,
        recorded: Option<Assessment>,
    ) -> Result<Self, AppError> {
        let current = Assessment::new(
            &revision.body,
            &revision.language,
            revision.format,
            policy,
            None,
        )?;
        let mut warnings = Vec::new();
        if current.words == 0 {
            warnings.push("Aucun mot comptable dans le corps de cet article.".into());
        }
        if current.words < current.min_words {
            warnings.push("Article plus court que le repère choisi. Une brève peut rester courte si une dérogation est justifiée ; ne pas ajouter de remplissage.".into());
        }
        if current.words > current.max_words {
            warnings.push("Article plus long que le repère choisi. Revoir le format ou motiver une dérogation ; ne pas tronquer automatiquement.".into());
        }
        if !current.supported {
            warnings.push("Comptage approximatif pour cette langue : la règle a été testée pour le français, l’anglais, le néerlandais et l’allemand.".into());
        }
        if recorded.as_ref().is_some_and(|r| !r.matches(&current)) {
            warnings.push("La politique de longueur ou le contenu diffère de l’évaluation conservée. Une nouvelle validation éditoriale est nécessaire.".into());
        }
        let reading_minutes = estimate_minutes(current.words, policy.reading_words_per_minute)
            .ok_or(AppError::InvalidData)?;
        Ok(Self {
            current,
            recorded,
            reading_minutes,
            reading_words_per_minute: policy.reading_words_per_minute,
            warnings,
        })
    }
}
pub(crate) async fn private_assessment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    revision: &str,
) -> Result<Option<Assessment>, AppError> {
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT provenance->'length' FROM news.story_revisions WHERE id=$1")
            .bind(revision)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::database("length_assessment", e))?;
    value
        .filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v).map_err(|_| AppError::InvalidData))
        .transpose()
}
pub async fn public_report(
    pool: &sqlx::PgPool,
    revision: &crate::content::Revision,
    policy: &EditorialPolicy,
) -> Result<Report, AppError> {
    let value: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT length_metadata FROM news.public_revisions WHERE id=$1")
            .bind(&revision.id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::database("public_length", e))?
            .ok_or(AppError::NotFound)?;
    let recorded = value
        .filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v).map_err(|_| AppError::InvalidData))
        .transpose()?;
    Report::new(revision, policy, recorded)
}
pub(crate) async fn publication_check(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    revision: &crate::content::Revision,
    policy: &EditorialPolicy,
) -> Result<(), AppError> {
    let recorded = private_assessment(tx, &revision.id)
        .await?
        .ok_or(AppError::Validation(
            "Publication requires a reviewed length assessment.",
        ))?;
    let current = Assessment::new(
        &revision.body,
        &revision.language,
        revision.format,
        policy,
        None,
    )?;
    if !recorded.matches(&current) {
        return Err(AppError::Validation(
            "Length policy changed; create and review a new assessment.",
        ));
    }
    if current.words == 0 {
        return Err(AppError::Validation(
            "Publication requires article words, not punctuation alone.",
        ));
    }
    if policy.length_override_requires_reason
        && current.needs_override()
        && recorded
            .override_reason
            .as_ref()
            .is_none_or(|v| v.trim().is_empty())
    {
        return Err(AppError::Validation(
            "Length or counting-language guidance requires an explicit public override reason.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reading_estimates_round_up_and_reject_zero_rate() {
        for (words, minutes) in [
            (0, 0),
            (1, 1),
            (199, 1),
            (200, 1),
            (201, 2),
            (400, 2),
            (401, 3),
        ] {
            assert_eq!(estimate_minutes(words, 200), Some(minutes));
        }
        assert_eq!(estimate_minutes(60, 60), Some(1));
        assert_eq!(estimate_minutes(60, 0), None);
    }
    #[test]
    fn language_specific_ranges_resolve_exact_then_primary_then_default() {
        let mut policy = EditorialPolicy::repository();
        let mut french = policy.formats.clone();
        french.brief.min_words = 70;
        let mut belgian = french.clone();
        belgian.brief.min_words = 60;
        policy.language_formats.insert("fr".into(), french);
        policy.language_formats.insert("fr-be".into(), belgian);
        assert!(policy.validate().is_ok());
        assert_eq!(policy.budget("FR-BE", ArticleFormat::Brief).min_words, 60);
        assert_eq!(policy.budget("fr-CA", ArticleFormat::Brief).min_words, 70);
        assert_eq!(policy.budget("nl", ArticleFormat::Brief).min_words, 80);
        policy
            .language_formats
            .insert("FR".into(), policy.formats.clone());
        assert!(policy.validate().is_err());
    }
    #[test]
    fn counts_french_unicode_numbers_and_punctuation_without_altering_input() {
        assert_eq!(
            words("L’été, c'est aujourd’hui : franco-belge COVID-19."),
            5
        );
        assert_eq!(words("e\u{301}cole école q\u{301}"), 3);
        assert_eq!(words("12,5 1.234,50 1\u{202f}234 2\u{a0}500 -42 15%"), 6);
        assert_eq!(words("Bonjour—monde bonjour–monde"), 4);
        assert_eq!(words("... !!! 👋 \u{301}"), 0);
        assert_eq!(words("'l’été' a--b l''été"), 5);
        assert_eq!(words("l'été"), 1);
        assert_eq!(words("ʼ \u{05b0}"), 0);
        assert_eq!(words("lʼété"), 1);
        assert!(supported("fr-BE") && supported("nl"));
        assert!(!supported("zh") && !supported("und"));
    }
    #[test]
    fn counts_only_authored_body_text_including_section_children() {
        use crate::content::{SectionBlock, SectionRole};
        let body = vec![
            ContentBlock::Heading {
                level: 2,
                text: "Deux mots".into(),
            },
            ContentBlock::Claim {
                id: "not-counted-anchor".into(),
                text: "Trois petits mots".into(),
            },
            ContentBlock::Section {
                role: SectionRole::Uncertainty,
                blocks: vec![SectionBlock::Paragraph {
                    text: "Encore deux".into(),
                }],
            },
        ];
        assert_eq!(body_words(&body), 7);
        assert_eq!(body_words(&[]), 0);
    }
    #[test]
    fn budget_boundaries_and_overrides_are_explicit_and_policy_bound() {
        let policy = EditorialPolicy::repository();
        for (count, expected) in [
            (0, true),
            (79, true),
            (80, false),
            (150, false),
            (151, true),
        ] {
            let text = std::iter::repeat_n("mot", count)
                .collect::<Vec<_>>()
                .join(" ");
            let body = vec![ContentBlock::Paragraph { text }];
            let assessment =
                Assessment::new(&body, "fr", ArticleFormat::Brief, &policy, None).unwrap();
            assert_eq!(assessment.needs_override(), expected);
            assert_eq!(assessment.words, count as u32);
        }
        assert!(
            Assessment::new(&[], "fr", ArticleFormat::Brief, &policy, Some(" ".into())).is_err()
        );
        let a = Assessment::new(
            &[],
            "fr",
            ArticleFormat::Brief,
            &policy,
            Some("Public rationale".into()),
        )
        .unwrap();
        let mut changed = policy.clone();
        changed.formats.brief.max_words += 1;
        let b = Assessment::new(&[], "fr", ArticleFormat::Brief, &changed, None).unwrap();
        assert!(!a.matches(&b));
    }
}
