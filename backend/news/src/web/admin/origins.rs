//! Human article-specific evidence, separate from source-registry relationships.
use super::super::{AppState, QueryValues, path_segment, render, with_query};
use crate::{
    auth::{Permission, Role, Session},
    content::{PageSize, valid_id},
    error::AppError,
    operations::Operations,
    origins::{CurrentEvidence, OriginEvidence, OriginInput, OriginState},
};
use askama::Template;
use axum::{
    Extension, Router,
    extract::{Path, RawForm, State, rejection::RawFormRejection},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use std::collections::BTreeMap;
#[derive(Clone)]
struct Fields {
    version: i64,
    state: String,
    public_basis: String,
    private_note: String,
    origins: Vec<String>,
    after: String,
}
struct Choice {
    id: String,
    name: String,
    enabled: bool,
    checked: bool,
}
struct History {
    href: String,
    version: i64,
    state: String,
    basis: String,
    created: String,
}
#[derive(Template)]
#[template(path = "origins.html")]
struct View {
    masthead: String,
    username: String,
    csrf: String,
    title: String,
    action: String,
    article: String,
    publisher: bool,
    form: Option<Fields>,
    choices: Vec<Choice>,
    next: Option<String>,
    history: Vec<History>,
    older: Option<String>,
    message: Option<String>,
    detail_basis: Option<String>,
    detail_note: Option<String>,
    detail_origins: Vec<String>,
}
fn pool(state: &AppState) -> Result<sqlx::PgPool, AppError> {
    state.editor_pool.clone().ok_or(AppError::Unavailable)
}
fn service(state: &AppState) -> Result<OriginEvidence, AppError> {
    Ok(OriginEvidence::new(pool(state)?))
}
pub(super) fn route(id: &str) -> String {
    format!("/admin/editor{}/origins", path_segment("source", id))
}
fn label(state: &str) -> &'static str {
    match state {
        "documented" => "Origine documentée",
        "conflicting" => "Origine contestée",
        _ => "Origine inconnue",
    }
}
fn fields(current: &CurrentEvidence) -> Fields {
    Fields {
        version: current.version,
        state: current
            .assertion
            .as_ref()
            .map(|a| a.evidence.state.clone())
            .unwrap_or_else(|| "unknown".into()),
        public_basis: current
            .assertion
            .as_ref()
            .map(|a| a.evidence.public_basis.clone())
            .unwrap_or_default(),
        // A new human assertion requires its own justification, never an automatic attestation.
        private_note: String::new(),
        origins: current
            .assertion
            .as_ref()
            .map(|a| a.evidence.origins.iter().map(|o| o.id.clone()).collect())
            .unwrap_or_default(),
        after: String::new(),
    }
}
fn base(state: &AppState, session: &Session, title: String, id: &str) -> View {
    View {
        masthead: state.config.masthead.clone(),
        username: session.principal().id().into(),
        csrf: session.csrf().into(),
        title,
        action: route(id),
        article: format!("/admin{}", path_segment("articles", id)),
        publisher: session.principal().role() == Role::Publisher,
        form: None,
        choices: vec![],
        next: None,
        history: vec![],
        older: None,
        message: None,
        detail_basis: None,
        detail_note: None,
        detail_origins: vec![],
    }
}
fn fail(state: &AppState, e: AppError) -> Response {
    render(&state.config, Err(e))
}
fn response(v: View, status: StatusCode) -> Response {
    match v.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/editor/source/{id}/origins", get(show).post(save))
        .route("/admin/origin-evidence/{id}", get(detail))
}
async fn view(
    state: &AppState,
    session: &Session,
    id: &str,
    f: Fields,
    before: Option<i64>,
    message: Option<String>,
) -> Result<View, AppError> {
    let service = service(state)?;
    let article = Operations::new(pool(state)?).article(session, id).await?;
    let choices = service
        .choices(
            session,
            &f.origins,
            if f.after.is_empty() {
                None
            } else {
                Some(&f.after)
            },
        )
        .await?;
    let rows = service
        .history(session, id, before, PageSize::new(20)?)
        .await?;
    let mut v = base(
        state,
        session,
        format!("Origine du document « {} »", article.title),
        id,
    );
    v.next = choices.next;
    v.choices = choices
        .items
        .into_iter()
        .map(|c| Choice {
            checked: f.origins.contains(&c.id),
            id: c.id,
            name: c.name,
            enabled: c.enabled,
        })
        .collect();
    for id in &f.origins {
        if !v.choices.iter().any(|c| c.id == *id) {
            v.choices.push(Choice {
                id: id.clone(),
                name: format!("Origine introuvable : {id}"),
                enabled: false,
                checked: true,
            });
        }
    }
    if rows.len() == 20 {
        v.older = rows
            .last()
            .map(|r| with_query(&route(id), &[("before", &r.id.to_string())]));
    }
    let zone = state
        .config
        .editorial
        .publication_timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| AppError::InvalidData)?;
    v.history = rows
        .into_iter()
        .map(|r| History {
            href: format!(
                "/admin{}",
                path_segment("origin-evidence", &r.id.to_string())
            ),
            version: r.version,
            state: label(&r.state).into(),
            basis: r.public_basis,
            created: r
                .created_at
                .with_timezone(&zone)
                .format("%Y-%m-%d %H:%M %Z")
                .to_string(),
        })
        .collect();
    v.message = message;
    v.form = Some(f);
    Ok(v)
}
async fn show(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Extension(query): Extension<QueryValues>,
    Path(id): Path<String>,
) -> Response {
    let result = async {
        if query.0.keys().any(|k| k != "before") {
            return Err(AppError::Validation("Filtre de preuve inconnu."));
        }
        let before = query
            .0
            .get("before")
            .map(|v| {
                v.parse::<i64>()
                    .map_err(|_| AppError::Validation("Page de preuve invalide."))
            })
            .transpose()?;
        let current = service(&state)?.current(&session, &id).await?;
        view(&state, &session, &id, fields(&current), before, None).await
    }
    .await;
    match result {
        Ok(v) => response(v, StatusCode::OK),
        Err(e) => fail(&state, e),
    }
}
async fn detail(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
) -> Response {
    let result = async {
        let proof = service(&state)?.details(&session, id).await?;
        let article = Operations::new(pool(&state)?)
            .article(&session, &proof.evidence.source_article_id)
            .await?;
        let mut v = base(
            &state,
            &session,
            format!(
                "Preuve d’origine, version {} · {}",
                proof.evidence.version, article.title
            ),
            &article.id,
        );
        v.detail_basis = Some(proof.evidence.public_basis);
        v.detail_note = Some(proof.private_note);
        v.detail_origins = proof
            .evidence
            .origins
            .iter()
            .map(|o| o.name.clone())
            .collect();
        v.message = Some(label(&proof.evidence.state).into());
        Ok(v)
    }
    .await;
    match result {
        Ok(v) => response(v, StatusCode::OK),
        Err(e) => fail(&state, e),
    }
}
fn take(m: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    m.remove(key)
        .ok_or(AppError::Validation("Formulaire de preuve incomplet."))
}
fn parse(b: &[u8]) -> Result<(Fields, String), AppError> {
    if !super::editor::strict_encoding(b) {
        return Err(AppError::Validation("Encodage de preuve invalide."));
    }
    let mut m = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(b) {
        if m.len() >= 130 || k.len() > 64 || m.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation(
                "Champs de preuve répétés ou trop nombreux.",
            ));
        }
    }
    take(&mut m, "csrf")?;
    let intent = take(&mut m, "intent")?;
    let mut f = Fields {
        version: take(&mut m, "version")?
            .parse()
            .map_err(|_| AppError::Validation("Version de preuve invalide."))?,
        state: take(&mut m, "state")?,
        public_basis: take(&mut m, "public_basis")?,
        private_note: take(&mut m, "private_note")?,
        origins: vec![],
        after: take(&mut m, "after")?,
    };
    let count = take(&mut m, "origins")?
        .parse::<usize>()
        .map_err(|_| AppError::Validation("Nombre d’origines invalide."))?;
    if count > 60 {
        return Err(AppError::Validation("Trop de choix d’origines."));
    }
    for i in 0..count {
        let id = take(&mut m, &format!("origin_id_{i}"))?;
        match m.remove(&format!("origin_{i}")) {
            None => {}
            Some(v) if v == "yes" => f.origins.push(id),
            _ => return Err(AppError::Validation("Choix d’origine invalide.")),
        }
    }
    if !m.is_empty() {
        return Err(AppError::Validation("Champ de preuve inconnu."));
    }
    Ok((f, intent))
}
async fn save(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let (mut f, intent) = match raw
        .map_err(|_| AppError::Validation("Formulaire de preuve invalide."))
        .and_then(|RawForm(b)| parse(&b))
    {
        Ok(v) => v,
        Err(e) => return fail(&state, e),
    };
    let result = async {
        let s = service(&state)?;
        let current = s.current(&session, &id).await?;
        if current.version != f.version {
            return Err(AppError::Conflict);
        }
        match intent.as_str() {
            "save" => {
                s.assert(
                    &session,
                    &id,
                    f.version,
                    OriginInput {
                        state: match f.state.as_str() {
                            "unknown" => OriginState::Unknown,
                            "documented" => OriginState::Documented,
                            "conflicting" => OriginState::Conflicting,
                            _ => return Err(AppError::Validation("État d’origine inconnu.")),
                        },
                        origin_ids: f.origins.clone(),
                        public_basis: f.public_basis.clone(),
                        private_note: f.private_note.clone(),
                    },
                )
                .await?;
                Ok(true)
            }
            "first" => {
                f.after.clear();
                Ok(false)
            }
            v if v.starts_with("page:") && valid_id(&v[5..]) => {
                f.after = v[5..].into();
                Ok(false)
            }
            _ => Err(AppError::Validation("Action de preuve inconnue.")),
        }
    }
    .await;
    let (status,message)=match result {
        Ok(true)=>return Redirect::to(&route(&id)).into_response(),
        Ok(false)=>(StatusCode::OK,Some("Choix préparés sans enregistrer de preuve.".into())),
        Err(AppError::Conflict)=>(StatusCode::CONFLICT,Some("L’état de la preuve ou un travail concurrent empêche l’enregistrement. Votre saisie est conservée ; comparez-la avec la preuve actuelle avant de la reporter.".into())),
        Err(AppError::Validation(text))=>(StatusCode::BAD_REQUEST,Some(format!("Preuve non enregistrée, saisie conservée. {text}"))),
        Err(e)=>return fail(&state,e),
    };
    let Some(auth) = &state.auth else {
        return fail(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, Permission::Publish).await {
        return super::failure(&state, e);
    }
    match view(&state, &session, &id, f, None, message).await {
        Ok(v) => response(v, status),
        Err(e) => fail(&state, e),
    }
}
