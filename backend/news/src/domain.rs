//! Editorial settings shared by web and future worker/publishing services.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordBudget {
    pub min_words: u32,
    pub max_words: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Formats {
    pub brief: WordBudget,
    pub standard: WordBudget,
    pub analysis: WordBudget,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorialPolicy {
    pub version: u32,
    pub default_language: String,
    pub publication_timezone: String,
    pub publication_mode: String,
    pub audience: String,
    pub formats: Formats,
    pub reading_words_per_minute: u32,
    pub edition_target_minutes: u32,
    pub length_override_requires_reason: bool,
    pub ai_drafting_enabled: bool,
    pub automatic_publication_enabled: bool,
}

impl EditorialPolicy {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let policy: Self =
            serde_json::from_str(text).map_err(|_| "Invalid editorial JSON schema")?;
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
        for budget in [
            &policy.formats.brief,
            &policy.formats.standard,
            &policy.formats.analysis,
        ] {
            if budget.min_words == 0 || budget.max_words < budget.min_words {
                return Err("Invalid article word budget");
            }
        }
        Ok(policy)
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
