//! Explicit editorial dimensions and deterministic section selection; never infer geography.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tag {
    pub id: String,
    pub label: String,
    pub aliases: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: String,
    pub label: String,
    pub any_topics: Vec<String>,
    pub any_geography: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SectionName {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Taxonomy {
    pub version: u32,
    pub topics: Vec<Tag>,
    pub geography: Vec<Tag>,
    pub sections: Vec<Section>,
    pub fallback: SectionName,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Classification {
    pub topics: Vec<String>,
    pub geography: Vec<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Projection {
    pub topics: Vec<String>,
    pub geography: Vec<String>,
    pub section: String,
    pub unknown_geography: bool,
    pub unmapped_topics: bool,
    pub unmapped_geography: bool,
}
fn key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}
fn tags(values: &[Tag]) -> bool {
    if values.is_empty() || values.len() > 40 {
        return false;
    }
    let mut keys = BTreeSet::new();
    values.iter().all(|v| {
        key(&v.id)
            && label(&v.label)
            && v.aliases.len() <= 20
            && std::iter::once(&v.id)
                .chain(v.aliases.iter())
                .all(|id| key(id) && keys.insert(id))
    })
}
fn exact(values: &[String], tags: &[Tag]) -> bool {
    let mut seen = BTreeSet::new();
    values.len() <= 20
        && values
            .iter()
            .all(|v| tags.iter().any(|t| t.id == *v) && seen.insert(v))
}
fn project(values: &[String], tags: &[Tag]) -> (Vec<String>, bool) {
    let mut known = BTreeSet::new();
    let mut unmapped = false;
    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        match tags
            .iter()
            .find(|t| t.id == normalized || t.aliases.contains(&normalized))
        {
            Some(t) => {
                known.insert(t.id.clone());
            }
            None => unmapped = true,
        }
    }
    (known.into_iter().collect(), unmapped)
}
impl Taxonomy {
    pub fn repository() -> Self {
        Self::parse(include_str!("../../../config/taxonomy.json"))
            .expect("valid repository taxonomy")
    }
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        if text.len() > 65536 {
            return Err("Taxonomy exceeds 64 KiB");
        }
        let p: Self = serde_json::from_str(text).map_err(|_| "Invalid taxonomy JSON schema")?;
        let mut section_ids = BTreeSet::new();
        if p.version != 1
            || !tags(&p.topics)
            || !tags(&p.geography)
            || p.sections.is_empty()
            || p.sections.len() > 20
            || !key(&p.fallback.id)
            || !label(&p.fallback.label)
            || !p.sections.iter().all(|s| {
                key(&s.id)
                    && label(&s.label)
                    && s.id != p.fallback.id
                    && section_ids.insert(&s.id)
                    && !(s.any_topics.is_empty() && s.any_geography.is_empty())
                    && exact(&s.any_topics, &p.topics)
                    && exact(&s.any_geography, &p.geography)
            })
        {
            return Err("Invalid taxonomy definitions or section rules");
        }
        Ok(p)
    }
    pub fn fingerprint(&self) -> String {
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).expect("serializable taxonomy"))
        )
    }
    pub fn validate(&self, value: &Classification) -> bool {
        exact(&value.topics, &self.topics) && exact(&value.geography, &self.geography)
    }
    pub fn section<'a>(&'a self, value: &Classification) -> (&'a str, &'a str) {
        self.sections
            .iter()
            .find(|s| {
                (s.any_topics.is_empty() || s.any_topics.iter().any(|t| value.topics.contains(t)))
                    && (s.any_geography.is_empty()
                        || s.any_geography.iter().any(|g| value.geography.contains(g)))
            })
            .map(|s| (s.id.as_str(), s.label.as_str()))
            .unwrap_or((&self.fallback.id, &self.fallback.label))
    }
    /// Read projection for historical/free-text metadata. Unknowns stay explicit;
    /// raw data is retained in immutable revisions/import payloads, not overwritten.
    pub fn project(&self, topics: &[String], geography: &[String]) -> Projection {
        let (topics, unmapped_topics) = project(topics, &self.topics);
        let (geography, unmapped_geography) = project(geography, &self.geography);
        let c = Classification { topics, geography };
        Projection {
            section: self.section(&c).0.into(),
            unknown_geography: c.geography.is_empty(),
            topics: c.topics,
            geography: c.geography,
            unmapped_topics,
            unmapped_geography,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(topics: &[&str], geography: &[&str]) -> Classification {
        Classification {
            topics: topics.iter().map(|s| (*s).into()).collect(),
            geography: geography.iter().map(|s| (*s).into()).collect(),
        }
    }
    #[test]
    fn requested_editorial_cases_keep_language_separate() {
        let p = Taxonomy::repository();
        for (_language, value, expected) in [
            ("fr", c(&["science"], &["world"]), "science-tech-economy"),
            ("nl", c(&["politics"], &["be"]), "belgium"),
            ("es", c(&["society"], &["world"]), "international"),
            ("fr", c(&["science", "sport"], &["be", "world"]), "sport"),
            ("fr", c(&["politics"], &[]), "unclassified"),
        ] {
            assert!(p.validate(&value));
            assert_eq!(p.section(&value).0, expected);
        }
    }
    #[test]
    fn legacy_mapping_never_turns_language_or_world_labels_into_countries() {
        let p = Taxonomy::repository();
        let mapped = p.project(&["tech".into()], &[]);
        assert_eq!(mapped.topics, vec!["technology"]);
        assert!(mapped.unknown_geography);
        assert_eq!(p.project(&["Monde".into()], &[]).section, "unclassified");
        assert!(
            p.project(&["unknown-private-label".into()], &[])
                .unmapped_topics
        );
    }
    #[test]
    fn strict_keys_rules_and_priority_are_configurable() {
        let p = Taxonomy::repository();
        assert!(!p.validate(&c(&["tech"], &[])));
        assert!(!p.validate(&c(&["science", "science"], &[])));
        assert!(!p.validate(&c(&[], &["fr"])));
        let mut reordered = p.clone();
        reordered.sections.swap(0, 2);
        assert_eq!(reordered.section(&c(&["sport"], &["be"])).0, "belgium");
        assert_ne!(p.fingerprint(), reordered.fingerprint());
        let mut bad = serde_json::to_value(&p).unwrap();
        bad["sections"][0]["language"] = serde_json::json!("fr");
        assert!(Taxonomy::parse(&bad.to_string()).is_err());
        bad = serde_json::to_value(&p).unwrap();
        bad["sections"][0]["any_topics"] = serde_json::json!(["missing"]);
        assert!(Taxonomy::parse(&bad.to_string()).is_err());
        bad = serde_json::to_value(&p).unwrap();
        bad["topics"][0]["aliases"] = serde_json::json!(["science"]);
        assert!(Taxonomy::parse(&bad.to_string()).is_err());
        assert!(Taxonomy::parse(&" ".repeat(65537)).is_err());
    }
}
