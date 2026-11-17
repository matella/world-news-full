//! Bounded, nonpersistent edition composition observations. No publication authority.
use crate::{
    content::{ArticleFormat, ArticleKind, Revision, valid_id},
    domain::EditorialPolicy,
    error::AppError,
    length,
    taxonomy::Taxonomy,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_STORIES: usize = 100;
/// Revisions and publication times must come from stored records, not a ready flag.
/// An approved draft can be prepared without inventing a publication instant.
pub struct Selection<'a> {
    pub revision: &'a Revision,
    pub section: &'a str,
    pub published_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WarningKind {
    Empty,
    StoryTarget,
    ReadingTarget,
    MissingLead,
    DuplicateStory,
    DuplicateRevision,
    MissingSection,
    Classification,
    SectionConcentration,
    StalePublication,
    FuturePublication,
    UnknownPublicationTime,
    ApproximateReading,
}
#[derive(Debug)]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
}
/// Content-derived metadata only; sourcing counts remain the citation report's job.
#[derive(Debug)]
pub struct Item {
    pub story_id: String,
    pub revision_id: String,
    pub section: String,
    pub is_lead: bool,
    pub format: ArticleFormat,
    pub kind: ArticleKind,
    pub words: u32,
    pub supported_count: bool,
}
#[derive(Debug)]
pub struct Preview {
    /// Preserves the editor's input order; never ranks by outlet/origin count.
    pub items: Vec<Item>,
    pub total_words: u64,
    /// Whole-edition estimate is withheld when any language has only an approximate count.
    pub estimated_minutes: Option<u64>,
    pub approximate_articles: usize,
    pub warnings: Vec<Warning>,
    pub observed_at: DateTime<Utc>,
}
impl Preview {
    pub fn new(
        selected: &[Selection<'_>],
        lead_story: Option<&str>,
        policy: &EditorialPolicy,
        taxonomy: &Taxonomy,
        at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        policy.validate().map_err(|_| AppError::InvalidData)?;
        if selected.len() > MAX_STORIES || lead_story.is_some_and(|id| !valid_id(id)) {
            return Err(AppError::Validation("Invalid edition selection or lead."));
        }
        let section_exists =
            |id: &str| taxonomy.fallback.id == id || taxonomy.sections.iter().any(|s| s.id == id);
        if policy
            .edition
            .required_sections
            .iter()
            .any(|s| !section_exists(s))
        {
            return Err(AppError::Validation(
                "Edition policy refers to an unknown section.",
            ));
        }
        let mut preview = Self {
            items: vec![],
            total_words: 0,
            estimated_minutes: None,
            approximate_articles: 0,
            warnings: vec![],
            observed_at: at,
        };
        let mut stories = BTreeSet::new();
        let mut revisions = BTreeSet::new();
        let mut sections = BTreeMap::<String, usize>::new();
        for selection in selected {
            let revision = selection.revision;
            if !valid_id(&revision.id)
                || !valid_id(&revision.story_id)
                || !section_exists(selection.section)
            {
                return Err(AppError::Validation(
                    "Invalid edition article identity or section.",
                ));
            }
            if !stories.insert(&revision.story_id) {
                preview.warn(
                    WarningKind::DuplicateStory,
                    format!(
                        "Article sélectionné plusieurs fois : {}.",
                        revision.story_id
                    ),
                );
            }
            if !revisions.insert(&revision.id) {
                preview.warn(
                    WarningKind::DuplicateRevision,
                    format!("Révision sélectionnée plusieurs fois : {}.", revision.id),
                );
            }
            let words = length::body_words(&revision.body);
            let supported_count = length::supported(&revision.language);
            preview.total_words += u64::from(words);
            preview.approximate_articles += usize::from(!supported_count);
            *sections.entry(selection.section.to_owned()).or_default() += 1;
            let projection = taxonomy.project(&revision.topics, &revision.geography);
            if projection.unmapped_topics
                || projection.unmapped_geography
                || projection.topics.is_empty()
                || projection.geography.is_empty()
            {
                preview.warn(WarningKind::Classification, format!("Classement incomplet ou ancien à examiner : {}. La langue ne définit pas la géographie.", revision.story_id));
            }
            match selection.published_at {
                None => preview.warn(WarningKind::UnknownPublicationTime, format!("Date de publication inconnue ou article encore privé : {}. Aucune fraîcheur n’est déduite de sa création.", revision.story_id)),
                Some(instant) if instant > at => preview.warn(WarningKind::FuturePublication, format!("Date de publication future à examiner : {}.", revision.story_id)),
                Some(instant) if at.signed_duration_since(instant) > Duration::hours(i64::from(policy.edition.stale_after_hours)) => preview.warn(WarningKind::StalePublication, format!("Publication de plus de {} heures : {}. Cette date ne mesure pas la fraîcheur des faits.", policy.edition.stale_after_hours, revision.story_id)),
                Some(_) => {}
            }
            preview.items.push(Item {
                story_id: revision.story_id.clone(),
                revision_id: revision.id.clone(),
                section: selection.section.to_owned(),
                is_lead: lead_story == Some(&revision.story_id),
                format: revision.format,
                kind: revision.kind,
                words,
                supported_count,
            });
        }
        if selected.is_empty() {
            preview.warn(
                WarningKind::Empty,
                "Édition vide : conserver un brouillon, sans annoncer de publication.".into(),
            );
        }
        if selected.len() < policy.edition.min_stories as usize
            || selected.len() > policy.edition.max_stories as usize
        {
            preview.warn(
                WarningKind::StoryTarget,
                format!(
                    "{} articles sélectionnés ; cible de {} à {}. Ne pas ajouter de remplissage.",
                    selected.len(),
                    policy.edition.min_stories,
                    policy.edition.max_stories
                ),
            );
        }
        if preview.items.iter().filter(|i| i.is_lead).count() != 1 {
            preview.warn(
                WarningKind::MissingLead,
                "Choisir explicitement un seul article principal parmi les articles sélectionnés."
                    .into(),
            );
        }
        for required in &policy.edition.required_sections {
            if !sections.contains_key(required) {
                preview.warn(WarningKind::MissingSection, format!("Rubrique attendue absente : {required}. Une édition partielle doit être assumée explicitement."));
            }
        }
        if preview.approximate_articles == 0 && !selected.is_empty() {
            let minutes = preview
                .total_words
                .div_ceil(u64::from(policy.reading_words_per_minute));
            preview.estimated_minutes = Some(minutes);
            if minutes > u64::from(policy.edition_target_minutes) {
                preview.warn(WarningKind::ReadingTarget, format!("Lecture estimée à {minutes} minutes ; cible de {}. Ne pas couper mécaniquement les articles.", policy.edition_target_minutes));
            }
        } else if preview.approximate_articles > 0 {
            preview.warn(WarningKind::ApproximateReading, format!("{} articles ont un décompte approximatif ; aucune estimation numérique pour l’édition entière.", preview.approximate_articles));
        }
        if selected.len() >= policy.edition.min_stories as usize {
            for (section, count) in sections {
                if count * 100 > selected.len() * policy.edition.max_section_share_percent as usize
                {
                    preview.warn(WarningKind::SectionConcentration, format!("{count}/{} articles dans la rubrique {section}, au-delà du seuil de {} %. Ce constat ne juge pas l’équilibre géopolitique.", selected.len(), policy.edition.max_section_share_percent));
                }
            }
        }
        Ok(preview)
    }
    fn warn(&mut self, kind: WarningKind, message: String) {
        self.warnings.push(Warning { kind, message });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ContentBlock;
    fn at() -> DateTime<Utc> {
        "2026-11-17T12:00:00Z".parse().unwrap()
    }
    fn revision(id: &str, words: usize, geography: &str) -> Revision {
        Revision {
            id: format!("revision-{id}"),
            story_id: id.into(),
            title: id.into(),
            standfirst: Some("Excluded overview words".into()),
            summary: None,
            body: vec![ContentBlock::Paragraph {
                text: std::iter::repeat_n("mot", words)
                    .collect::<Vec<_>>()
                    .join(" "),
            }],
            language: "fr".into(),
            format: ArticleFormat::Brief,
            kind: ArticleKind::Reporting,
            topics: vec!["society".into()],
            geography: vec![geography.into()],
            created_at: at() - Duration::days(365),
        }
    }
    fn has(preview: &Preview, kind: WarningKind) -> bool {
        preview.warnings.iter().any(|w| w.kind == kind)
    }
    #[test]
    fn deliberate_order_and_lead_use_aggregate_body_count_without_mutation() {
        let a = revision("a", 80, "be");
        let b = revision("b", 80, "world");
        let c = revision("c", 80, "eu");
        let before = serde_json::to_value(&a.body).unwrap();
        let selected = [
            Selection {
                revision: &b,
                section: "international",
                published_at: Some(at()),
            },
            Selection {
                revision: &a,
                section: "belgium",
                published_at: Some(at()),
            },
            Selection {
                revision: &c,
                section: "europe",
                published_at: Some(at()),
            },
        ];
        let preview = Preview::new(
            &selected,
            Some("a"),
            &EditorialPolicy::repository(),
            &Taxonomy::repository(),
            at(),
        )
        .unwrap();
        assert_eq!(
            preview
                .items
                .iter()
                .map(|i| i.story_id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a", "c"]
        );
        assert!(!preview.items[0].is_lead);
        assert!(preview.items[1].is_lead);
        assert_eq!(preview.total_words, 240);
        assert_eq!(preview.estimated_minutes, Some(2));
        assert!(preview.warnings.is_empty());
        assert_eq!(preview.items[1].format, ArticleFormat::Brief);
        assert_eq!(preview.items[1].kind, ArticleKind::Reporting);
        assert_eq!(serde_json::to_value(&a.body).unwrap(), before);
    }
    #[test]
    fn duplicates_missing_sections_and_ambiguous_lead_are_visible() {
        let a = revision("a", 80, "be");
        let selected = [
            Selection {
                revision: &a,
                section: "belgium",
                published_at: Some(at()),
            },
            Selection {
                revision: &a,
                section: "belgium",
                published_at: Some(at()),
            },
        ];
        let preview = Preview::new(
            &selected,
            Some("a"),
            &EditorialPolicy::repository(),
            &Taxonomy::repository(),
            at(),
        )
        .unwrap();
        for kind in [
            WarningKind::DuplicateStory,
            WarningKind::DuplicateRevision,
            WarningKind::MissingLead,
            WarningKind::MissingSection,
            WarningKind::StoryTarget,
        ] {
            assert!(has(&preview, kind));
        }
        assert_eq!(preview.items.len(), 2); // Unsaved input is retained for correction.
    }
    #[test]
    fn over_budget_concentration_and_date_uncertainty_are_observations() {
        let a = revision("a", 1000, "be");
        let b = revision("b", 1000, "be");
        let c = revision("c", 1000, "be");
        let selected = [
            Selection {
                revision: &a,
                section: "belgium",
                published_at: Some(at() - Duration::hours(49)),
            },
            Selection {
                revision: &b,
                section: "belgium",
                published_at: Some(at() + Duration::minutes(1)),
            },
            Selection {
                revision: &c,
                section: "belgium",
                published_at: None,
            },
        ];
        let preview = Preview::new(
            &selected,
            Some("a"),
            &EditorialPolicy::repository(),
            &Taxonomy::repository(),
            at(),
        )
        .unwrap();
        assert_eq!(preview.estimated_minutes, Some(15));
        assert_eq!(preview.total_words, 3000);
        for kind in [
            WarningKind::ReadingTarget,
            WarningKind::SectionConcentration,
            WarningKind::StalePublication,
            WarningKind::FuturePublication,
            WarningKind::UnknownPublicationTime,
        ] {
            assert!(has(&preview, kind));
        }
        assert_eq!(preview.observed_at, at());
    }
    #[test]
    fn approximate_language_withholds_whole_edition_minutes_and_does_not_infer_geography() {
        let mut a = revision("a", 5000, "be");
        a.language = "zh".into();
        a.geography.clear();
        let preview = Preview::new(
            &[Selection {
                revision: &a,
                section: "belgium",
                published_at: None,
            }],
            Some("a"),
            &EditorialPolicy::repository(),
            &Taxonomy::repository(),
            at(),
        )
        .unwrap();
        assert_eq!(preview.approximate_articles, 1);
        assert_eq!(preview.estimated_minutes, None);
        assert!(has(&preview, WarningKind::ApproximateReading));
        assert!(has(&preview, WarningKind::Classification));
        assert!(!has(&preview, WarningKind::ReadingTarget));
        assert!(a.geography.is_empty());
    }
    #[test]
    fn empty_invalid_and_oversized_preparations_have_explicit_boundaries() {
        let policy = EditorialPolicy::repository();
        let taxonomy = Taxonomy::repository();
        let empty = Preview::new(&[], None, &policy, &taxonomy, at()).unwrap();
        assert!(has(&empty, WarningKind::Empty));
        assert!(has(&empty, WarningKind::MissingLead));
        assert_eq!(empty.estimated_minutes, None);
        let a = revision("a", 80, "be");
        let selected = (0..101)
            .map(|_| Selection {
                revision: &a,
                section: "belgium",
                published_at: None,
            })
            .collect::<Vec<_>>();
        assert!(Preview::new(&selected, Some("a"), &policy, &taxonomy, at()).is_err());
        assert!(Preview::new(&[], Some("../invalid"), &policy, &taxonomy, at()).is_err());
        assert!(
            Preview::new(
                &[Selection {
                    revision: &a,
                    section: "invented",
                    published_at: None
                }],
                Some("a"),
                &policy,
                &taxonomy,
                at()
            )
            .is_err()
        );
        let mut bad = policy.clone();
        bad.edition.required_sections = vec!["invented".into()];
        assert!(Preview::new(&[], None, &bad, &taxonomy, at()).is_err());
    }
    #[test]
    fn guidance_is_configurable_and_does_not_invalidate_article_assessments() {
        let policy = EditorialPolicy::repository();
        let fingerprint = policy.length_fingerprint();
        let mut custom = policy.clone();
        custom.edition.min_stories = 1;
        custom.edition.max_stories = 1;
        custom.edition.required_sections.clear();
        custom.edition.max_section_share_percent = 100;
        custom.edition.stale_after_hours = 1;
        custom.edition_target_minutes = 1;
        assert_eq!(custom.length_fingerprint(), fingerprint);
        let a = revision("a", 400, "be");
        let report = Preview::new(
            &[Selection {
                revision: &a,
                section: "belgium",
                published_at: Some(at() - Duration::hours(2)),
            }],
            Some("a"),
            &custom,
            &Taxonomy::repository(),
            at(),
        )
        .unwrap();
        assert!(has(&report, WarningKind::StalePublication));
        assert!(has(&report, WarningKind::ReadingTarget));
        assert!(!has(&report, WarningKind::StoryTarget));
        assert!(!has(&report, WarningKind::SectionConcentration));
        let mut json = serde_json::to_value(&policy).unwrap();
        json.as_object_mut().unwrap().remove("edition");
        let old = EditorialPolicy::parse(&json.to_string()).unwrap();
        assert_eq!(old.edition.max_stories, 12);
        for (key, value) in [
            ("min_stories", 0),
            ("max_stories", 101),
            ("stale_after_hours", 0),
            ("max_section_share_percent", 0),
        ] {
            let mut json = serde_json::to_value(&policy).unwrap();
            json["edition"][key] = value.into();
            assert!(EditorialPolicy::parse(&json.to_string()).is_err());
        }
    }
}
