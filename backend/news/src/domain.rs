//! Editorial settings shared by web and future worker/publishing services.
use crate::content::ArticleFormat;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WordBudget {
    pub min_words: u32,
    pub max_words: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Formats {
    pub brief: WordBudget,
    pub standard: WordBudget,
    pub analysis: WordBudget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditorialPolicy {
    pub version: u32,
    pub default_language: String,
    pub publication_timezone: String,
    pub publication_mode: String,
    pub audience: String,
    pub formats: Formats,
    #[serde(default)]
    pub language_formats: BTreeMap<String, Formats>,
    pub reading_words_per_minute: u32,
    pub edition_target_minutes: u32,
    pub length_override_requires_reason: bool,
    pub ai_drafting_enabled: bool,
    pub automatic_publication_enabled: bool,
}

impl EditorialPolicy {
    pub fn repository() -> Self {
        Self::parse(include_str!("../../../config/editorial.json"))
            .expect("validated bundled editorial policy")
    }
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        if text.len() > 65536 {
            return Err("Editorial policy exceeds its size limit");
        }
        let policy: Self =
            serde_json::from_str(text).map_err(|_| "Invalid editorial JSON schema")?;
        policy.validate()?;
        Ok(policy)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let policy = self;
        if policy.version != 1 {
            return Err("Unsupported editorial policy version");
        }
        if policy.publication_mode != "manual"
            || policy.ai_drafting_enabled
            || policy.automatic_publication_enabled
        {
            return Err("Only manual publication without an AI provider is implemented");
        }
        if policy.default_language != "fr" || policy.audience.trim().is_empty() {
            return Err("The initial reader interface requires French and an audience description");
        }
        if policy
            .publication_timezone
            .parse::<chrono_tz::Tz>()
            .is_err()
        {
            return Err("Invalid publication timezone");
        }
        if policy.reading_words_per_minute == 0 || policy.edition_target_minutes == 0 {
            return Err("Reading and edition budgets must be positive");
        }
        if policy.language_formats.len() > 20
            || policy
                .language_formats
                .keys()
                .any(|key| !crate::content::valid_language(key) || key.to_ascii_lowercase() != *key)
        {
            return Err("Invalid language word-budget overrides");
        }
        for budget in std::iter::once(&policy.formats)
            .chain(policy.language_formats.values())
            .flat_map(|f| [&f.brief, &f.standard, &f.analysis])
        {
            if budget.min_words == 0 || budget.max_words < budget.min_words {
                return Err("Invalid article word budget");
            }
        }
        Ok(())
    }
    pub fn budget(&self, language: &str, format: ArticleFormat) -> &WordBudget {
        let language = language.to_ascii_lowercase();
        let formats = self
            .language_formats
            .get(&language)
            .or_else(|| {
                self.language_formats
                    .get(language.split('-').next().unwrap_or("und"))
            })
            .unwrap_or(&self.formats);
        match format {
            ArticleFormat::Brief => &formats.brief,
            ArticleFormat::Standard => &formats.standard,
            ArticleFormat::Analysis => &formats.analysis,
        }
    }
    pub fn length_fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let value = serde_json::json!({"count_policy":"word-v1","formats":self.formats,
            "language_formats":self.language_formats,"requires_reason":self.length_override_requires_reason,
            "reading_words_per_minute":self.reading_words_per_minute});
        format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const POLICY: &str = include_str!("../../../config/editorial.json");

    #[test]
    fn accepts_the_repository_policy() {
        let policy = EditorialPolicy::parse(POLICY).unwrap();
        assert_eq!(policy.formats.standard.max_words, 600);
    }

    #[test]
    fn rejects_unimplemented_publication_modes_and_unknown_settings() {
        assert!(
            EditorialPolicy::parse(&POLICY.replace(
                "\"ai_drafting_enabled\": false",
                "\"ai_drafting_enabled\": true"
            ))
            .is_err()
        );
        assert!(
            EditorialPolicy::parse(
                &POLICY.replace("\"version\": 1", "\"version\": 1, \"typo\": 1")
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_bad_budgets_and_timezones() {
        assert!(
            EditorialPolicy::parse(&POLICY.replace("\"min_words\": 80", "\"min_words\": 999"))
                .is_err()
        );
        assert!(EditorialPolicy::parse(&POLICY.replace("Europe/Brussels", "not/a-zone")).is_err());
        assert!(
            EditorialPolicy::parse(&POLICY.replace(
                "\"reading_words_per_minute\": 200",
                "\"reading_words_per_minute\": 0"
            ))
            .is_err()
        );
    }
}
