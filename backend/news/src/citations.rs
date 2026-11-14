//! Bounded attribution reports shared by public reading, private review and publication.
use crate::{
    auth::{self, Permission, Session},
    content::{Revision, valid_id},
    error::AppError,
    intake::normalize::canonical_url,
    origins::OriginLabel,
    sourcing::{PublicPackage, SourceMode, SourcePackage, claim_ids},
};
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, sqlx::FromRow)]
pub struct Citation {
    pub claim_id: String,
    pub anchor_id: Option<String>,
    pub source_article_id: String,
    pub source_id: String,
    pub source_name: String,
    pub original_url: String,
    pub location: Option<String>,
    pub origin_state: String,
    pub origin_snapshot: Json<Vec<OriginLabel>>,
    pub origin_basis: Option<String>,
    pub origins_current: bool,
    #[sqlx(default)]
    pub number: i32,
}
impl Citation {
    pub fn origins(&self) -> &[OriginLabel] {
        &self.origin_snapshot.0
    }
    pub fn origin_label(&self) -> &'static str {
        match self.origin_state.as_str() {
            "documented" => "Origine attribuée et documentée",
            "conflicting" => "Origine contestée",
            _ => "Origine inconnue",
        }
    }
}
#[derive(Debug, Default)]
pub struct Report {
    pub metadata: PublicPackage,
    pub citations: Vec<Citation>,
    pub documents: usize,
    pub outlets: usize,
    pub documented_origins: usize,
    pub unknown_documents: usize,
    pub conflicting_documents: usize,
    pub missing_claims: Vec<String>,
    pub warnings: Vec<String>,
}
impl Report {
    pub fn for_claim<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Citation> + 'a {
        self.citations
            .iter()
            .filter(move |c| c.anchor_id.as_deref() == Some(id))
    }
    fn build(
        revision: &Revision,
        citations: Vec<Citation>,
        metadata: PublicPackage,
    ) -> Result<Self, AppError> {
        // Group by the existing conservative URL-v1 policy, not source-record IDs.
        let mut documents: BTreeMap<String, Vec<&Citation>> = BTreeMap::new();
        let mut outlets = BTreeSet::new();
        for c in &citations {
            let url = canonical_url(&c.original_url).ok_or(AppError::InvalidData)?;
            if !valid_id(&c.source_article_id)
                || !valid_id(&c.source_id)
                || c.source_name.trim().is_empty()
                || c.source_name.chars().count() > 300
                || !matches!(
                    c.origin_state.as_str(),
                    "unknown" | "conflicting" | "documented"
                )
                || c.origin_snapshot.0.len() > 20
                || c.origin_snapshot
                    .0
                    .iter()
                    .any(|o| !valid_id(&o.id) || o.name.trim().is_empty())
                || c.anchor_id
                    .as_deref()
                    .is_some_and(|id| !claim_ids(&revision.body).contains(id))
            {
                return Err(AppError::InvalidData);
            }
            documents.entry(url).or_default().push(c);
            outlets.insert(&c.source_id);
        }
        let mut report = Self {
            documents: documents.len(),
            outlets: outlets.len(),
            metadata,
            ..Self::default()
        };
        let mut origins = BTreeSet::new();
        for group in documents.values() {
            let known: BTreeSet<BTreeSet<&str>> = group
                .iter()
                .filter(|c| c.origin_state == "documented")
                .map(|c| c.origin_snapshot.0.iter().map(|o| o.id.as_str()).collect())
                .collect();
            if group.iter().any(|c| c.origin_state == "conflicting") || known.len() > 1 {
                report.conflicting_documents += 1;
            } else if group.iter().any(|c| c.origin_state == "unknown")
                || known.iter().any(BTreeSet::is_empty)
            {
                report.unknown_documents += 1;
            } else {
                origins.extend(known.into_iter().flatten().map(str::to_owned));
            }
        }
        report.documented_origins = origins.len();
        let covered: BTreeSet<&str> = citations
            .iter()
            .filter_map(|c| c.anchor_id.as_deref())
            .chain(
                report
                    .metadata
                    .firsthand
                    .iter()
                    .flat_map(|f| f.claim_ids.iter().map(String::as_str)),
            )
            .collect();
        report.missing_claims = claim_ids(&revision.body)
            .into_iter()
            .filter(|id| !covered.contains(id))
            .map(str::to_owned)
            .collect();
        if report.metadata.statement.is_none() {
            report.warnings.push(
                "Déclaration de provenance absente : attribution éditoriale incomplète.".into(),
            );
        }
        if !report.missing_claims.is_empty() {
            report.warnings.push(format!(
                "{} affirmation(s) sans référence ni méthode originale déclarée.",
                report.missing_claims.len()
            ));
        }
        if citations.iter().any(|c| c.anchor_id.is_none()) {
            report.warnings.push("Des références générales ne sont pas rattachées à une affirmation précise. Les anciennes correspondances ne sont pas devinées.".into());
        }
        if report.unknown_documents > 0 || report.conflicting_documents > 0 {
            report.warnings.push("L’origine de certains documents est inconnue ou contestée ; ils ne certifient pas des reportages indépendants.".into());
        }
        if citations.iter().any(|c| !c.origins_current) {
            report.warnings.push("L’attribution conservée diffère de l’état actuel du registre ; la version publiée reste figée.".into());
        }
        report.citations = citations;
        for (i, c) in report.citations.iter_mut().enumerate() {
            c.number = i as i32 + 1;
        }
        Ok(report)
    }
}
const PRIVATE_CITATIONS: &str = "SELECT c.claim_id,c.anchor_id,c.source_article_id,a.source_id,a.attribution_name AS source_name,a.original_url,c.location,c.origin_state,c.origin_snapshot,c.origin_basis,(c.origin_version=a.origin_version AND NOT EXISTS(SELECT 1 FROM news.article_origin_labels l JOIN news.reporting_origins o ON o.id=l.origin_id WHERE l.assertion_id=c.origin_assertion_id AND NOT o.enabled)) AS origins_current FROM news.citations c JOIN news.source_articles a ON a.id=c.source_article_id WHERE c.revision_id=$1 ORDER BY c.anchor_id NULLS FIRST,c.source_article_id,c.claim_id LIMIT 201";
pub(crate) async fn private_report(
    tx: &mut Transaction<'_, Postgres>,
    revision: &Revision,
) -> Result<Report, AppError> {
    let package = private_package(tx, &revision.id).await?;
    let rows = sqlx::query_as::<_, Citation>(PRIVATE_CITATIONS)
        .bind(&revision.id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db)?;
    if rows.len() > 200 {
        return Err(AppError::InvalidData);
    }
    Report::build(
        revision,
        rows,
        package.map(|p| p.public()).unwrap_or_default(),
    )
}
pub(crate) async fn private_package(
    tx: &mut Transaction<'_, Postgres>,
    revision: &str,
) -> Result<Option<SourcePackage>, AppError> {
    let value:Option<serde_json::Value>=sqlx::query_scalar("SELECT CASE WHEN provenance#>>'{sourcing,schema}'='1' THEN provenance#>'{sourcing,package}' ELSE NULL END FROM news.story_revisions WHERE id=$1")
        .bind(revision).fetch_one(&mut **tx).await.map_err(db)?;
    value
        .map(|v| serde_json::from_value(v).map_err(|_| AppError::InvalidData))
        .transpose()
}
#[derive(Clone)]
pub struct Citations {
    pool: PgPool,
}
impl Citations {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn public(&self, revision: &Revision) -> Result<Report, AppError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let value: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT sourcing FROM news.public_revisions WHERE id=$1")
                .bind(&revision.id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or(AppError::NotFound)?;
        let metadata = value
            .map(|v| serde_json::from_value(v).map_err(|_| AppError::InvalidData))
            .transpose()?
            .unwrap_or_default();
        let rows=sqlx::query_as::<_,Citation>("SELECT claim_id,anchor_id,source_article_id,source_id,source_name,original_url,location,origin_state,origin_snapshot,origin_basis,origins_current FROM news.public_citations WHERE revision_id=$1 ORDER BY anchor_id NULLS FIRST,source_article_id,claim_id LIMIT 201")
            .bind(&revision.id).fetch_all(&mut *tx).await.map_err(db)?;
        if rows.len() > 200 {
            return Err(AppError::InvalidData);
        }
        let report = Report::build(revision, rows, metadata)?;
        tx.commit().await.map_err(db)?;
        Ok(report)
    }
    pub async fn private(
        &self,
        session: &Session,
        revision: &Revision,
    ) -> Result<Report, AppError> {
        let (mut tx, _) = auth::begin_authorised(&self.pool, session, Permission::Edit).await?;
        let report = private_report(&mut tx, revision).await?;
        tx.commit().await.map_err(db)?;
        Ok(report)
    }
}
fn db(e: sqlx::Error) -> AppError {
    AppError::database("citation_report", e)
}

/// Called inside the locked publication transaction, never a cached UI decision.
pub(crate) async fn publication_check(
    tx: &mut Transaction<'_, Postgres>,
    revision: &Revision,
) -> Result<(), AppError> {
    let package = private_package(tx, &revision.id)
        .await?
        .ok_or(AppError::Validation(
            "Publication requires an explicit sourcing declaration.",
        ))?;
    package.validate(&revision.body, revision.kind)?;
    let report = private_report(tx, revision).await?;
    let statement = package.statement.as_ref().ok_or(AppError::Validation(
        "Publication requires an explicit attribution statement.",
    ))?;
    let fail = |message| Err(AppError::Validation(message));
    if report.citations.iter().any(|c| !c.origins_current) {
        return fail("Source-origin evidence changed; create and review a new sourcing revision.");
    }
    if statement.mode == SourceMode::Historical {
        let lineage:bool=sqlx::query_scalar("SELECT COALESCE(provenance->>'pipeline'='legacy_jarvis' AND kind IN ('legacy_summary','legacy_synthesis'),false) FROM news.story_revisions WHERE id=$1")
            .bind(&revision.id).fetch_one(&mut **tx).await.map_err(db)?;
        if !lineage || statement.limits.is_none() {
            return fail(
                "Historical attribution requires preserved import lineage and public limitations.",
            );
        }
        return Ok(());
    }
    if !report.missing_claims.is_empty() {
        return fail("Some factual claim anchors lack attributed evidence.");
    }
    if (revision.kind == crate::content::ArticleKind::Opinion)
        != (statement.mode == SourceMode::Opinion)
    {
        return fail("Opinion attribution must match the article kind.");
    }
    match statement.mode {
        SourceMode::Original if package.firsthand.is_empty() => {
            return fail(
                "Original reporting needs a documented human method and private evidence.",
            );
        }
        SourceMode::External
            if report.citations.is_empty()
                || !report.citations.iter().any(|c| c.anchor_id.is_some()) =>
        {
            return fail("External reporting needs explicit factual claim-to-source mappings.");
        }
        _ => {}
    }
    if !report.citations.is_empty()
        && (report.documented_origins < 2
            || report.unknown_documents > 0
            || report.conflicting_documents > 0)
        && statement.limits.is_none()
    {
        return fail(
            "Single-source or uncertain-origin reporting needs an explicit public limitations note.",
        );
    }
    Ok(())
}
