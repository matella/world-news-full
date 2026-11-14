//! Plain form blocks for server-rendered authoring; no raw HTML or publication authority.
use crate::{
    content::{ContentBlock, SectionBlock, SectionRole, validate_blocks},
    error::AppError,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Unused,
    Paragraph,
    Heading2,
    Heading3,
    Claim,
    ContextParagraph,
    ContextClaim,
    UncertaintyParagraph,
    UncertaintyClaim,
}
impl BlockKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Unused => "unused",
            Self::Paragraph => "paragraph",
            Self::Heading2 => "heading2",
            Self::Heading3 => "heading3",
            Self::Claim => "claim",
            Self::ContextParagraph => "context_paragraph",
            Self::ContextClaim => "context_claim",
            Self::UncertaintyParagraph => "uncertainty_paragraph",
            Self::UncertaintyClaim => "uncertainty_claim",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Unused => "Bloc inutilisé",
            Self::Paragraph => "Paragraphe",
            Self::Heading2 => "Intertitre",
            Self::Heading3 => "Sous-titre",
            Self::Claim => "Affirmation à documenter",
            Self::ContextParagraph => "Contexte : paragraphe",
            Self::ContextClaim => "Contexte : affirmation",
            Self::UncertaintyParagraph => "Incertitudes : paragraphe",
            Self::UncertaintyClaim => "Incertitudes : affirmation",
        }
    }
    pub fn parse(value: &str) -> Result<Self, AppError> {
        Self::all()
            .iter()
            .copied()
            .find(|k| k.key() == value)
            .ok_or(AppError::Validation("Type de bloc inconnu."))
    }
    pub fn all() -> &'static [Self] {
        &[
            Self::Unused,
            Self::Paragraph,
            Self::Heading2,
            Self::Heading3,
            Self::Claim,
            Self::ContextParagraph,
            Self::ContextClaim,
            Self::UncertaintyParagraph,
            Self::UncertaintyClaim,
        ]
    }
}
#[derive(Clone)]
pub struct AuthorBlock {
    pub kind: BlockKind,
    pub text: String,
    pub claim_id: String,
    pub delete: bool,
}
impl AuthorBlock {
    pub fn unused() -> Self {
        Self {
            kind: BlockKind::Unused,
            text: String::new(),
            claim_id: String::new(),
            delete: false,
        }
    }
}
pub fn flatten(body: &[ContentBlock]) -> Vec<AuthorBlock> {
    let mut result = Vec::new();
    for block in body {
        match block {
            ContentBlock::Paragraph { text } => result.push(row(BlockKind::Paragraph, text, "")),
            ContentBlock::Heading { level, text } => result.push(row(
                if *level == 2 {
                    BlockKind::Heading2
                } else {
                    BlockKind::Heading3
                },
                text,
                "",
            )),
            ContentBlock::Claim { id, text } => result.push(row(BlockKind::Claim, text, id)),
            ContentBlock::Section { role, blocks } => {
                for child in blocks {
                    let (kind, text, id) = match (role, child) {
                        (SectionRole::Context, SectionBlock::Paragraph { text }) => {
                            (BlockKind::ContextParagraph, text, "")
                        }
                        (SectionRole::Context, SectionBlock::Claim { id, text }) => {
                            (BlockKind::ContextClaim, text, id.as_str())
                        }
                        (SectionRole::Uncertainty, SectionBlock::Paragraph { text }) => {
                            (BlockKind::UncertaintyParagraph, text, "")
                        }
                        (SectionRole::Uncertainty, SectionBlock::Claim { id, text }) => {
                            (BlockKind::UncertaintyClaim, text, id.as_str())
                        }
                    };
                    result.push(row(kind, text, id));
                }
            }
        }
    }
    result
}
fn row(kind: BlockKind, text: &str, id: &str) -> AuthorBlock {
    AuthorBlock {
        kind,
        text: text.into(),
        claim_id: id.into(),
        delete: false,
    }
}
pub fn fresh_id(prefix: &str) -> Result<String, AppError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;
    Ok(format!(
        "{prefix}-{}",
        bytes.iter().map(|v| format!("{v:02x}")).collect::<String>()
    ))
}
pub fn compose(rows: &[AuthorBlock]) -> Result<Vec<ContentBlock>, AppError> {
    if rows.len() > 2001 {
        return Err(AppError::Validation("Trop de blocs dans ce formulaire."));
    }
    let mut result = Vec::new();
    let mut section: Option<(SectionRole, Vec<SectionBlock>)> = None;
    let mut seen = std::collections::BTreeSet::new();
    for r in rows {
        if r.delete {
            continue;
        }
        if r.kind == BlockKind::Unused {
            if !r.text.is_empty() || !r.claim_id.is_empty() {
                return Err(AppError::Validation(
                    "Un bloc contenant du texte doit avoir un type ou être supprimé explicitement.",
                ));
            }
            continue;
        }
        let role = match r.kind {
            BlockKind::ContextParagraph | BlockKind::ContextClaim => Some(SectionRole::Context),
            BlockKind::UncertaintyParagraph | BlockKind::UncertaintyClaim => {
                Some(SectionRole::Uncertainty)
            }
            _ => None,
        };
        if section.as_ref().map(|s| s.0) != role {
            flush(&mut section, &mut result);
            if let Some(role) = role {
                if !seen.insert(role) {
                    return Err(AppError::Validation(
                        "Regroupez les blocs de contexte ou d’incertitude dans un seul encadré chacun.",
                    ));
                }
                section = Some((role, Vec::new()));
            }
        }
        let id = if matches!(
            r.kind,
            BlockKind::Claim | BlockKind::ContextClaim | BlockKind::UncertaintyClaim
        ) {
            if r.claim_id.is_empty() {
                fresh_id("c")?
            } else {
                r.claim_id.clone()
            }
        } else {
            String::new()
        };
        if let Some((_, children)) = &mut section {
            children.push(
                if matches!(
                    r.kind,
                    BlockKind::ContextClaim | BlockKind::UncertaintyClaim
                ) {
                    SectionBlock::Claim {
                        id,
                        text: r.text.clone(),
                    }
                } else {
                    SectionBlock::Paragraph {
                        text: r.text.clone(),
                    }
                },
            );
        } else {
            result.push(match r.kind {
                BlockKind::Heading2 => ContentBlock::Heading {
                    level: 2,
                    text: r.text.clone(),
                },
                BlockKind::Heading3 => ContentBlock::Heading {
                    level: 3,
                    text: r.text.clone(),
                },
                BlockKind::Claim => ContentBlock::Claim {
                    id,
                    text: r.text.clone(),
                },
                _ => ContentBlock::Paragraph {
                    text: r.text.clone(),
                },
            });
        }
    }
    flush(&mut section, &mut result);
    if !validate_blocks(&result) {
        return Err(AppError::Validation(
            "Vérifiez les textes, les identifiants d’affirmation et les limites des blocs.",
        ));
    }
    Ok(result)
}
fn flush(section: &mut Option<(SectionRole, Vec<SectionBlock>)>, result: &mut Vec<ContentBlock>) {
    if let Some((role, blocks)) = section.take() {
        result.push(ContentBlock::Section { role, blocks });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn form_round_trip_preserves_plain_original_prose_and_claim_identity() {
        let body = vec![
            ContentBlock::Paragraph {
                text: " Original e\u{301}cole\r\n<script>plain</script> ".into(),
            },
            ContentBlock::Heading {
                level: 3,
                text: "Sous-titre".into(),
            },
            ContentBlock::Section {
                role: SectionRole::Context,
                blocks: vec![SectionBlock::Claim {
                    id: "known".into(),
                    text: "Attribution à vérifier".into(),
                }],
            },
        ];
        assert_eq!(compose(&flatten(&body)).unwrap(), body);
        let mut rows = flatten(&body);
        rows.push(AuthorBlock::unused());
        assert_eq!(compose(&rows).unwrap(), body);
    }
    #[test]
    fn deleting_is_explicit_and_inactive_rows_cannot_silently_lose_text() {
        let mut rows = vec![row(BlockKind::Unused, "Must survive validation", "")];
        assert!(compose(&rows).is_err());
        rows[0].delete = true;
        assert!(compose(&rows).unwrap().is_empty());
        rows[0] = row(BlockKind::Paragraph, "", "");
        assert_eq!(
            compose(&rows).unwrap(),
            vec![ContentBlock::Paragraph {
                text: String::new()
            }]
        );
    }
    #[test]
    fn claims_receive_safe_ids_and_split_sections_are_not_silently_reordered() {
        let rows = vec![row(BlockKind::Claim, "A factual assertion", "")];
        let body = compose(&rows).unwrap();
        let id = body[0].claim_ids().next().unwrap();
        assert!(crate::content::valid_claim_id(id));
        assert_eq!(compose(&flatten(&body)).unwrap(), body);
        let rows = vec![
            row(BlockKind::ContextParagraph, "First context", ""),
            row(BlockKind::Paragraph, "Intervening text", ""),
            row(BlockKind::ContextParagraph, "Second context", ""),
        ];
        assert!(compose(&rows).is_err());
        assert!(BlockKind::parse("raw_html").is_err());
    }
}
