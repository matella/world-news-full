//! Private human sourcing forms. Saving clones a candidate, never a publication.
use super::super::{AppState, MAX_EDITOR_BODY, path_segment, render};
use crate::{
    auth::{Permission, Session},
    content::{ContentBlock, valid_id},
    editorial::{EditorialService, EvidenceDraft},
    error::AppError,
    sourcing::*,
};
use askama::Template;
use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, Path, RawForm, State, rejection::RawFormRejection},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Default)]
struct Reference {
    source: String,
    claim: String,
    location: String,
    origin: String,
    refresh: bool,
    delete: bool,
}
#[derive(Clone, Default)]
struct Method {
    method: String,
    claims: Vec<String>,
    new_claim: String,
    attribution: String,
    private_note: String,
    delete: bool,
}
#[derive(Clone, Default)]
struct Dispute {
    subject: String,
    positions: Vec<Stance>,
    delete: bool,
}
#[derive(Clone, Default)]
struct Stance {
    claim: String,
    attribution: String,
    delete: bool,
}
#[derive(Clone)]
struct Fields {
    version: i64,
    revision: String,
    mode: String,
    attribution: String,
    limits: String,
    reason: String,
    references: Vec<Reference>,
    methods: Vec<Method>,
    disputes: Vec<Dispute>,
    after: String,
    claim_page: usize,
}
struct Choice {
    key: String,
    label: String,
}
#[derive(Template)]
#[template(path = "evidence.html")]
struct View {
    masthead: String,
    username: String,
    csrf: String,
    title: String,
    action: String,
    edit: String,
    details: String,
    proposed: bool,
    message: Option<String>,
    form: Fields,
    claims: Vec<Choice>,
    sources: Vec<Choice>,
    next: Option<String>,
    claim_next: Option<usize>,
}
impl View {
    fn claim_choices(&self, selected: &str) -> Vec<&Choice> {
        let mut page: Vec<_> = self
            .claims
            .iter()
            .skip(self.form.claim_page)
            .take(50)
            .collect();
        if let Some(c) = self.claims.iter().find(|c| c.key == selected)
            && !page.iter().any(|v| v.key == selected)
        {
            page.push(c);
        }
        page
    }
    fn source_choices(&self, selected: &str) -> Vec<&Choice> {
        let mut page: Vec<_> = self.sources.iter().take(50).collect();
        if let Some(c) = self.sources.iter().find(|c| c.key == selected)
            && !page.iter().any(|v| v.key == selected)
        {
            page.push(c);
        }
        page
    }

    fn claim_label(&self, id: &str) -> &str {
        self.claims
            .iter()
            .find(|c| c.key == id)
            .map(|c| c.label.as_str())
            .unwrap_or("Affirmation absente de cette version")
    }
}
fn service(state: &AppState) -> Result<EditorialService, AppError> {
    Ok(EditorialService::with_policy(
        state.editor_pool.clone().ok_or(AppError::Unavailable)?,
        Arc::new(state.config.editorial.clone()),
    ))
}
fn route(id: &str) -> String {
    format!("/admin/editor{}/sources", path_segment("story", id))
}
fn optional(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        None
    } else {
        Some(text.into())
    }
}
fn from_draft(d: &EvidenceDraft) -> Fields {
    let p = &d.package;
    Fields {
        version: d.head.lock_version,
        revision: d.revision.id.clone(),
        mode: p
            .statement
            .as_ref()
            .map(|s| match s.mode {
                SourceMode::External => "external",
                SourceMode::Original => "original",
                SourceMode::Opinion => "opinion",
                SourceMode::Historical => "historical",
            })
            .unwrap_or("")
            .into(),
        attribution: p
            .statement
            .as_ref()
            .map(|s| s.attribution.clone())
            .unwrap_or_default(),
        limits: p
            .statement
            .as_ref()
            .and_then(|s| s.limits.clone())
            .unwrap_or_default(),
        reason: String::new(),
        references: p
            .bindings
            .iter()
            .map(|b| Reference {
                source: b.source_article_id.clone(),
                claim: b.anchor_id.clone().unwrap_or_default(),
                location: b.location.clone().unwrap_or_default(),
                origin: b.origin_version.map(|v| v.to_string()).unwrap_or_default(),
                ..Default::default()
            })
            .collect(),
        methods: p
            .firsthand
            .iter()
            .map(|f| Method {
                method: match f.method {
                    FirsthandMethod::Observation => "observation",
                    FirsthandMethod::Interview => "interview",
                    FirsthandMethod::DocumentReview => "document_review",
                }
                .into(),
                claims: f.claim_ids.clone(),
                new_claim: String::new(),
                attribution: f.attribution.clone(),
                private_note: f.private_note.clone(),
                delete: false,
            })
            .collect(),
        disputes: p
            .disagreements
            .iter()
            .map(|d| Dispute {
                subject: d.subject.clone(),
                positions: d
                    .positions
                    .iter()
                    .map(|p| Stance {
                        claim: p.claim_id.clone(),
                        attribution: p.attribution.clone(),
                        delete: false,
                    })
                    .collect(),
                delete: false,
            })
            .collect(),
        after: String::new(),
        claim_page: 0,
    }
}
fn claims(body: &[ContentBlock]) -> Vec<Choice> {
    let mut result = vec![];
    for row in crate::editorial::authoring::flatten(body) {
        if !row.claim_id.is_empty() {
            result.push(Choice {
                key: row.claim_id.clone(),
                label: format!(
                    "{} · {}",
                    row.claim_id,
                    row.text.chars().take(160).collect::<String>()
                ),
            });
        }
    }
    result
}
async fn view(
    state: &AppState,
    session: &Session,
    id: &str,
    d: &EvidenceDraft,
    fields: Fields,
    message: Option<String>,
) -> Result<View, AppError> {
    let controls = 12
        + fields.references.len() * 7
        + fields.methods.len() * 7
        + fields
            .methods
            .iter()
            .map(|m| m.claims.len() * 2)
            .sum::<usize>()
        + fields
            .disputes
            .iter()
            .map(|d| 3 + d.positions.len() * 3)
            .sum::<usize>();
    if controls > 8900 {
        return Err(AppError::Validation(
            "Ce dossier dépasse les limites du formulaire privé ; aucune donnée n’a été modifiée.",
        ));
    }
    let selected: Vec<String> = fields
        .references
        .iter()
        .filter(|r| valid_id(&r.source))
        .map(|r| r.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let page = service(state)?
        .source_choices(session, &selected, optional(&fields.after).as_deref(), None)
        .await?;
    let mut sources: Vec<Choice> = page
        .items
        .into_iter()
        .map(|s| Choice {
            key: s.id,
            label: format!("{} · {}", s.attribution_name, s.title),
        })
        .collect();
    for id in selected {
        if !sources.iter().any(|c| c.key == id) {
            sources.push(Choice {
                key: id.clone(),
                label: format!("Référence introuvable : {id}"),
            });
        }
    }
    let mut claims = claims(&d.revision.body);
    // Preserve stale proposal anchors visibly until a human repairs or removes them.
    let selected_claims: Vec<&str> = fields
        .references
        .iter()
        .map(|r| r.claim.as_str())
        .chain(
            fields
                .disputes
                .iter()
                .flat_map(|d| d.positions.iter().map(|p| p.claim.as_str())),
        )
        .collect();
    for id in selected_claims
        .into_iter()
        .chain(fields.methods.iter().flat_map(|m| {
            m.claims
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(m.new_claim.as_str()))
        }))
    {
        if !id.is_empty() && !claims.iter().any(|c| c.key == id) {
            claims.push(Choice {
                key: id.into(),
                label: format!("Affirmation absente de cette version : {id}"),
            });
        }
    }
    Ok(View {
        masthead: state.config.masthead.clone(),
        username: session.principal().id().into(),
        csrf: session.csrf().into(),
        title: d.revision.title.clone(),
        action: route(id),
        edit: format!("/admin/editor{}", path_segment("story", id)),
        details: format!("/admin{}", path_segment("drafts", id)),
        proposed: d.proposed,
        message,
        claim_next: if fields.claim_page + 50 < claims.len() {
            Some(fields.claim_page + 50)
        } else {
            None
        },
        form: fields,
        claims,
        sources,
        next: page.next,
    })
}
fn respond(view: View, status: StatusCode) -> Response {
    match view.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
fn fail(state: &AppState, error: AppError) -> Response {
    render(&state.config, Err(error))
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/editor/story/{id}/sources", get(show).post(save))
        .layer(DefaultBodyLimit::max(MAX_EDITOR_BODY))
}
async fn show(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result = async {
        let d = service(&state)?.evidence_draft(&session, &id).await?;
        view(&state, &session, &id, &d, from_draft(&d), None).await
    }
    .await;
    match result {
        Ok(v) => respond(v, StatusCode::OK),
        Err(e) => fail(&state, e),
    }
}
fn take(m: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    m.remove(key)
        .ok_or(AppError::Validation("Formulaire de sources incomplet."))
}
fn count(m: &mut BTreeMap<String, String>, key: &str, max: usize) -> Result<usize, AppError> {
    let n = take(m, key)?
        .parse::<usize>()
        .map_err(|_| AppError::Validation("Nombre de champs invalide."))?;
    if n > max {
        return Err(AppError::Validation("Trop de champs de sources."));
    }
    Ok(n)
}
fn checked(m: &mut BTreeMap<String, String>, key: &str) -> Result<bool, AppError> {
    match m.remove(key) {
        None => Ok(false),
        Some(v) if v == "yes" => Ok(true),
        _ => Err(AppError::Validation("Choix invalide.")),
    }
}
fn parse(bytes: &[u8]) -> Result<(Fields, String), AppError> {
    if !super::editor::strict_encoding(bytes) {
        return Err(AppError::Validation("Encodage du formulaire invalide."));
    }
    let mut m = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(bytes) {
        if m.len() >= 9000 || k.len() > 64 || m.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation("Champs répétés ou trop nombreux."));
        }
    }
    take(&mut m, "csrf")?;
    let intent = take(&mut m, "intent")?;
    let mut f = Fields {
        version: take(&mut m, "version")?
            .parse()
            .map_err(|_| AppError::Validation("Version invalide."))?,
        revision: take(&mut m, "revision")?,
        mode: take(&mut m, "mode")?,
        attribution: take(&mut m, "attribution")?,
        limits: take(&mut m, "limits")?,
        reason: take(&mut m, "reason")?,
        after: take(&mut m, "after")?,
        claim_page: count(&mut m, "claim_page", 2000)?,
        references: vec![],
        methods: vec![],
        disputes: vec![],
    };
    for i in 0..count(&mut m, "references", 200)? {
        f.references.push(Reference {
            source: take(&mut m, &format!("source_{i}"))?,
            claim: take(&mut m, &format!("claim_{i}"))?,
            location: take(&mut m, &format!("location_{i}"))?,
            origin: take(&mut m, &format!("origin_{i}"))?,
            refresh: checked(&mut m, &format!("refresh_{i}"))?,
            delete: checked(&mut m, &format!("delete_ref_{i}"))?,
        });
    }
    for i in 0..count(&mut m, "methods", 50)? {
        let mut claims = vec![];
        for j in 0..count(&mut m, &format!("scope_count_{i}"), 2000)? {
            let id = take(&mut m, &format!("scope_id_{i}_{j}"))?;
            if checked(&mut m, &format!("scope_{i}_{j}"))? {
                claims.push(id);
            }
        }
        f.methods.push(Method {
            method: take(&mut m, &format!("method_{i}"))?,
            claims,
            new_claim: take(&mut m, &format!("new_claim_{i}"))?,
            attribution: take(&mut m, &format!("method_attribution_{i}"))?,
            private_note: take(&mut m, &format!("private_{i}"))?,
            delete: checked(&mut m, &format!("delete_method_{i}"))?,
        });
    }
    for i in 0..count(&mut m, "disputes", 20)? {
        let mut d = Dispute {
            subject: take(&mut m, &format!("subject_{i}"))?,
            delete: checked(&mut m, &format!("delete_dispute_{i}"))?,
            positions: vec![],
        };
        for j in 0..count(&mut m, &format!("positions_{i}"), 20)? {
            d.positions.push(Stance {
                claim: take(&mut m, &format!("position_claim_{i}_{j}"))?,
                attribution: take(&mut m, &format!("position_attribution_{i}_{j}"))?,
                delete: checked(&mut m, &format!("delete_position_{i}_{j}"))?,
            });
        }
        f.disputes.push(d);
    }
    if !m.is_empty() {
        return Err(AppError::Validation("Champ de sources inconnu."));
    }
    Ok((f, intent))
}
fn package(f: &Fields, draft: &EvidenceDraft) -> Result<SourcePackage, AppError> {
    let revision = &draft.revision;
    let statement = match f.mode.as_str() {
        "" if f.attribution.is_empty() && f.limits.is_empty() => None,
        "" => {
            return Err(AppError::Validation(
                "Choisissez une attribution pour conserver cette déclaration.",
            ));
        }
        key => Some(SourceStatement {
            mode: match key {
                "external" => SourceMode::External,
                "original" => SourceMode::Original,
                "opinion" => SourceMode::Opinion,
                "historical" => SourceMode::Historical,
                _ => return Err(AppError::Validation("Attribution inconnue.")),
            },
            attribution: f.attribution.clone(),
            limits: optional(&f.limits),
        }),
    };
    let mut p = SourcePackage {
        statement,
        ..Default::default()
    };
    for r in f.references.iter().filter(|r| !r.delete) {
        p.bindings.push(Binding {
            source_article_id: r.source.clone(),
            anchor_id: optional(&r.claim),
            location: optional(&r.location),
            origin_version: if r.refresh || r.origin.is_empty() {
                None
            } else {
                let version=r.origin.parse().map_err(|_|AppError::Validation("Version de preuve invalide."))?;
                if !draft.package.bindings.iter().any(|b|b.source_article_id==r.source && b.origin_version==Some(version)) {
                    return Err(AppError::Validation("Le document a changé : demandez explicitement la capture de sa preuve actuelle."));
                }
                Some(version)
            },
        });
    }
    for m in f.methods.iter().filter(|m| !m.delete) {
        p.firsthand.push(FirsthandEvidence {
            method: match m.method.as_str() {
                "observation" => FirsthandMethod::Observation,
                "interview" => FirsthandMethod::Interview,
                "document_review" => FirsthandMethod::DocumentReview,
                _ => return Err(AppError::Validation("Méthode inconnue.")),
            },
            claim_ids: m
                .claims
                .iter()
                .cloned()
                .chain(optional(&m.new_claim))
                .collect(),
            attribution: m.attribution.clone(),
            private_note: m.private_note.clone(),
        });
    }
    for d in f.disputes.iter().filter(|d| !d.delete) {
        p.disagreements.push(Disagreement {
            subject: d.subject.clone(),
            positions: d
                .positions
                .iter()
                .filter(|p| !p.delete)
                .map(|p| Position {
                    claim_id: p.claim.clone(),
                    attribution: p.attribution.clone(),
                })
                .collect(),
        });
    }
    p.validate(&revision.body, revision.kind)?;
    Ok(p)
}
async fn save(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let (mut f, intent) = match raw
        .map_err(|_| AppError::Validation("Formulaire invalide."))
        .and_then(|RawForm(b)| parse(&b))
    {
        Ok(v) => v,
        Err(e) => return fail(&state, e),
    };
    let service = match service(&state) {
        Ok(s) => s,
        Err(e) => return fail(&state, e),
    };
    let d = match service.evidence_draft(&session, &id).await {
        Ok(d) => d,
        Err(e) => return fail(&state, e),
    };
    let result = async {
        if f.version != d.head.lock_version || f.revision != d.revision.id {
            return Err(AppError::Conflict);
        }
        match intent.as_str() {
            "add_reference" if f.references.len() < 200 => f.references.push(Reference::default()),
            "add_method" if f.methods.len() < 50 => f.methods.push(Method {
                method: "observation".into(),
                ..Default::default()
            }),
            "add_dispute" if f.disputes.len() < 20 => f.disputes.push(Dispute {
                positions: vec![Stance::default(), Stance::default()],
                ..Default::default()
            }),
            "catalog" => f.after.clear(),
            v if v.starts_with("claims:") => {
                f.claim_page = v[7..]
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n <= 2000)
                    .ok_or(AppError::Validation("Page d’affirmations invalide."))?;
            }
            v if v.starts_with("add_scope:") => {
                let i = v[10..]
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| f.methods.get_mut(i))
                    .ok_or(AppError::Validation("Méthode inconnue."))?;
                if i.claims.len() >= 2000
                    || i.new_claim.is_empty()
                    || i.claims.contains(&i.new_claim)
                {
                    return Err(AppError::Validation(
                        "Choisissez une nouvelle affirmation à couvrir.",
                    ));
                }
                i.claims.push(std::mem::take(&mut i.new_claim));
            }
            "save" => {
                let p = package(&f, &d)?;
                service
                    .source_revision(&session, &id, f.version, p, &f.reason)
                    .await?;
                return Ok(true);
            }
            v if v.starts_with("page:") && valid_id(&v[5..]) => f.after = v[5..].into(),
            v if v.starts_with("add_position:") => {
                let i = v[13..]
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| f.disputes.get_mut(i))
                    .ok_or(AppError::Validation("Désaccord inconnu."))?;
                if i.positions.len() >= 20 {
                    return Err(AppError::Validation("Trop de positions."));
                }
                i.positions.push(Stance::default());
            }
            _ => {
                return Err(AppError::Validation(
                    "Action de sources inconnue ou limite atteinte.",
                ));
            }
        }
        Ok(false)
    }
    .await;
    let (status,message)=match result {
        Ok(true)=>return Redirect::to(&route(&id)).into_response(),
        Ok(false)=>(StatusCode::OK,Some("Préparation non enregistrée : vos champs sont conservés.".into())),
        Err(AppError::Conflict)=>(StatusCode::CONFLICT,Some("Cette version a changé ou son état empêche l’enregistrement. Votre saisie est conservée ; comparez-la avec la version actuelle avant de la reporter.".into())),
        Err(AppError::Validation(text))=>(StatusCode::BAD_REQUEST,Some(format!("Saisie non enregistrée, conservée ci-dessous. {text}"))),
        Err(e)=>return fail(&state,e),
    };
    let Some(auth) = &state.auth else {
        return fail(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, Permission::Edit).await {
        return super::failure(&state, e);
    }
    match view(&state, &session, &id, &d, f, message).await {
        Ok(v) => respond(v, status),
        Err(e) => fail(&state, e),
    }
}
