//! Versioned human attribution declarations. Evidence establishes attribution, not truth.
use crate::{
    content::{ArticleKind, ContentBlock, valid_id},
    error::AppError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    External,
    Original,
    Opinion,
    Historical,
}
impl SourceMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::External => "Sources externes attribuées",
            Self::Original => "Travail original déclaré par la rédaction",
            Self::Opinion => "Opinion : faits et références attribués séparément",
            Self::Historical => "Archive historique non vérifiée",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStatement {
    pub mode: SourceMode,
    pub attribution: String,
    pub limits: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirsthandMethod {
    Observation,
    Interview,
    DocumentReview,
}
impl FirsthandMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Observation => "Observation",
            Self::Interview => "Entretien",
            Self::DocumentReview => "Examen de documents",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirsthandEvidence {
    pub method: FirsthandMethod,
    pub claim_ids: Vec<String>,
    pub attribution: String,
    pub private_note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFirsthand {
    pub method: FirsthandMethod,
    pub claim_ids: Vec<String>,
    pub attribution: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source_article_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_version: Option<i64>,
    pub anchor_id: Option<String>,
    pub location: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub claim_id: String,
    pub attribution: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disagreement {
    pub subject: String,
    pub positions: Vec<Position>,
}
/// Stored privately in immutable revision provenance; never serialize into a public response.
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourcePackage {
    pub statement: Option<SourceStatement>,
    pub bindings: Vec<Binding>,
    pub firsthand: Vec<FirsthandEvidence>,
    pub disagreements: Vec<Disagreement>,
}
/// Explicit allowlist, deliberately excluding private evidence notes and actors.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PublicPackage {
    pub statement: Option<SourceStatement>,
    pub firsthand: Vec<PublicFirsthand>,
    pub disagreements: Vec<Disagreement>,
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}
pub fn claim_ids(body: &[ContentBlock]) -> BTreeSet<&str> {
    body.iter().flat_map(ContentBlock::claim_ids).collect()
}
impl SourcePackage {
    pub fn public(&self) -> PublicPackage {
        PublicPackage {
            statement: self.statement.clone(),
            firsthand: self
                .firsthand
                .iter()
                .map(|v| PublicFirsthand {
                    method: v.method,
                    claim_ids: v.claim_ids.clone(),
                    attribution: v.attribution.clone(),
                })
                .collect(),
            disagreements: self.disagreements.clone(),
        }
    }
    /// Incomplete sourcing is valid draft data. Invalid mappings or invented positions are not.
    pub fn validate(&self, body: &[ContentBlock], kind: ArticleKind) -> Result<(), AppError> {
        let claims = claim_ids(body);
        let mut bindings = BTreeSet::new();
        let invalid = self.bindings.len() > 200
            || self.firsthand.len() > 50
            || self.disagreements.len() > 20
            || self.statement.as_ref().is_some_and(|s| {
                !text(&s.attribution, 2000)
                    || s.limits.as_ref().is_some_and(|v| !text(v, 2000))
                    || (s.mode == SourceMode::Opinion && kind != ArticleKind::Opinion)
                    || (matches!(
                        kind,
                        ArticleKind::LegacySummary | ArticleKind::LegacySynthesis
                    ) && s.mode != SourceMode::Historical)
            })
            || self.bindings.iter().any(|b| {
                b.origin_version.is_some_and(|v| v < 0)
                    || !valid_id(&b.source_article_id)
                    || b.anchor_id
                        .as_deref()
                        .is_some_and(|id| !claims.contains(id))
                    || b.location.as_ref().is_some_and(|v| !text(v, 2000))
                    || !bindings.insert((&b.source_article_id, &b.anchor_id))
            })
            || self.firsthand.iter().any(|f| {
                !text(&f.attribution, 2000)
                    || !text(&f.private_note, 10000)
                    || f.claim_ids.len() > 2000
                    || (f.claim_ids.is_empty() && !claims.is_empty())
                    || f.claim_ids.iter().collect::<BTreeSet<_>>().len() != f.claim_ids.len()
                    || f.claim_ids.iter().any(|id| !claims.contains(id.as_str()))
            })
            || self.disagreements.iter().any(|d| {
                !text(&d.subject, 1000)
                    || !(2..=20).contains(&d.positions.len())
                    || d.positions
                        .iter()
                        .map(|p| &p.claim_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        != d.positions.len()
                    || d.positions.iter().any(|p| {
                        !text(&p.attribution, 1000)
                            || !claims.contains(p.claim_id.as_str())
                            || (!self
                                .bindings
                                .iter()
                                .any(|b| b.anchor_id.as_deref() == Some(&p.claim_id))
                                && !self
                                    .firsthand
                                    .iter()
                                    .any(|f| f.claim_ids.contains(&p.claim_id)))
                    })
            });
        if invalid || serde_json::to_vec(self).map_or(true, |v| v.len() > 256 * 1024) {
            return Err(AppError::Validation(
                "Invalid sourcing declarations or claim mappings.",
            ));
        }
        Ok(())
    }
}

pub(crate) async fn attach(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    revision: &str,
    package: &SourcePackage,
) -> Result<(), AppError> {
    for binding in &package.bindings {
        let key = binding
            .anchor_id
            .as_ref()
            .map(|id| format!("claim:{id}"))
            .unwrap_or_else(|| "general".into());
        sqlx::query("INSERT INTO news.citations(revision_id,claim_id,source_article_id,anchor_id,location,origin_version) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(revision).bind(key).bind(&binding.source_article_id).bind(&binding.anchor_id).bind(&binding.location).bind(binding.origin_version)
            .execute(&mut **tx).await.map_err(|e|AppError::database("sourcing_bindings",e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> Vec<ContentBlock> {
        vec![
            ContentBlock::Claim {
                id: "one".into(),
                text: "First attributed claim".into(),
            },
            ContentBlock::Claim {
                id: "two".into(),
                text: "Second attributed claim".into(),
            },
        ]
    }
    #[test]
    fn packages_reject_missing_anchors_and_unattributed_disagreements() {
        let mut p = SourcePackage::default();
        assert!(p.validate(&body(), ArticleKind::Reporting).is_ok());
        p.bindings.push(Binding {
            origin_version: None,
            source_article_id: "source".into(),
            anchor_id: Some("missing".into()),
            location: None,
        });
        assert!(p.validate(&body(), ArticleKind::Reporting).is_err());
        p.bindings[0].anchor_id = Some("one".into());
        p.disagreements.push(Disagreement {
            subject: "Conflicting accounts".into(),
            positions: vec![
                Position {
                    claim_id: "one".into(),
                    attribution: "Source one".into(),
                },
                Position {
                    claim_id: "two".into(),
                    attribution: "Source two".into(),
                },
            ],
        });
        assert!(p.validate(&body(), ArticleKind::Reporting).is_err());
        p.bindings.push(Binding {
            origin_version: None,
            source_article_id: "source-two".into(),
            anchor_id: Some("two".into()),
            location: Some("Page 2".into()),
        });
        assert!(p.validate(&body(), ArticleKind::Reporting).is_ok());
        p.bindings[0].origin_version = Some(-1);
        assert!(p.validate(&body(), ArticleKind::Reporting).is_err());
        p.bindings[0].origin_version = None;
        p.bindings.push(p.bindings[0].clone());
        assert!(p.validate(&body(), ArticleKind::Reporting).is_err());
    }
    #[test]
    fn public_projection_cannot_contain_private_firsthand_notes() {
        let p = SourcePackage {
            firsthand: vec![FirsthandEvidence {
                method: FirsthandMethod::Interview,
                claim_ids: vec!["one".into(), "two".into()],
                attribution: "Named synthetic interview".into(),
                private_note: "PRIVATE-MARKER".into(),
            }],
            ..Default::default()
        };
        assert!(p.validate(&body(), ArticleKind::Reporting).is_ok());
        assert!(
            !serde_json::to_string(&p.public())
                .unwrap()
                .contains("PRIVATE-MARKER")
        );
        let mut historical = p.clone();
        historical.statement = Some(SourceStatement {
            mode: SourceMode::Original,
            attribution: "Cannot relabel an archive as newly original".into(),
            limits: Some("Historical archive".into()),
        });
        assert!(
            historical
                .validate(&body(), ArticleKind::LegacySummary)
                .is_err()
        );
        let mut bad = p.clone();
        bad.firsthand[0].claim_ids.clear();
        assert!(bad.validate(&body(), ArticleKind::Reporting).is_err());
        bad.firsthand[0].claim_ids = vec!["one".into()];
        bad.firsthand[0].private_note.clear();
        assert!(bad.validate(&body(), ArticleKind::Reporting).is_err());
    }
}
