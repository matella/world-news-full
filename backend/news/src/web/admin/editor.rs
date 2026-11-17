use super::super::{AppState, MAX_EDITOR_BODY, QueryValues, path_segment, render, with_query};
use crate::{
    auth::{Permission, Session},
    content::{ArticleFormat, ArticleKind, ContentBlock, PageSize, Revision, SectionBlock},
    editorial::{
        DraftCursor, DraftFilter, EditorialService, RevisionInput,
        authoring::{self, AuthorBlock, BlockKind},
    },
    error::AppError,
};
use askama::Template;
use axum::{
    Extension, Router,
    extract::{DefaultBodyLimit, Path, RawForm, State, rejection::RawFormRejection},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
struct Fields {
    story: Option<String>,
    revision: String,
    version: i64,
    title: String,
    standfirst: String,
    summary: String,
    language: String,
    format: String,
    kind: String,
    reason: String,
    override_reason: String,
    rows: Vec<AuthorBlock>,
}
struct Card {
    title: String,
    href: String,
    status: String,
}
struct Choice {
    key: &'static str,
    label: &'static str,
    selected: bool,
}
#[derive(Template)]
#[template(path = "editor.html")]
struct View {
    masthead: String,
    username: String,
    csrf: String,
    title: String,
    message: Option<String>,
    cards: Vec<Card>,
    next: Option<String>,
    form: Option<Fields>,
    action: String,
    formats: Vec<Choice>,
    kinds: Vec<Choice>,
    block_kinds: &'static [BlockKind],
    details: Option<String>,
    sources: Option<String>,
    workflow: Option<String>,
    article: Option<Revision>,
    evidence: Option<crate::citations::Report>,
    length: Option<crate::length::Report>,
    length_private: bool,
}
fn service(state: &AppState) -> Result<EditorialService, AppError> {
    Ok(EditorialService::with_policy(
        state.editor_pool.clone().ok_or(AppError::Unavailable)?,
        Arc::new(state.config.editorial.clone()),
    ))
}
fn route(id: &str) -> String {
    format!("/admin/editor{}", path_segment("story", id))
}
fn view(state: &AppState, session: &Session, title: &str) -> View {
    View {
        masthead: state.config.masthead.clone(),
        username: session.principal().id().into(),
        csrf: session.csrf().into(),
        title: title.into(),
        message: None,
        cards: vec![],
        next: None,
        form: None,
        action: "/admin/editor".into(),
        formats: vec![],
        kinds: vec![],
        block_kinds: BlockKind::all(),
        details: None,
        sources: None,
        workflow: None,
        article: None,
        evidence: None,
        length: None,
        length_private: true,
    }
}
fn response(view: View, status: StatusCode) -> Response {
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
        .route("/admin/editor", get(board).post(create))
        .route("/admin/editor/new", get(new))
        .route("/admin/editor/story/{id}", get(edit).post(save))
        .layer(DefaultBodyLimit::max(MAX_EDITOR_BODY))
}

async fn board(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Extension(query): Extension<QueryValues>,
) -> Response {
    let result = async {
        if query
            .0
            .keys()
            .any(|k| !matches!(k.as_str(), "filter" | "after" | "at"))
        {
            return Err(AppError::Validation("Filtre de rédaction inconnu."));
        }
        let filter_key = query
            .0
            .get("filter")
            .map(String::as_str)
            .unwrap_or("candidates");
        let filter = match filter_key {
            "candidates" => DraftFilter::Candidates,
            "in_review" => DraftFilter::InReview,
            "published" => DraftFilter::Published,
            "withdrawn" => DraftFilter::Withdrawn,
            "all" => DraftFilter::All,
            _ => return Err(AppError::Validation("Filtre de rédaction inconnu.")),
        };
        let cursor = match query.0.get("after") {
            Some(id) => Some(DraftCursor {
                id: id.clone(),
                updated_at: query
                    .0
                    .get("at")
                    .map(|v| {
                        chrono::DateTime::parse_from_rfc3339(v)
                            .map(|t| t.with_timezone(&chrono::Utc))
                            .map_err(|_| AppError::Validation("Date de pagination invalide."))
                    })
                    .transpose()?,
            }),
            None => {
                if query.0.contains_key("at") {
                    return Err(AppError::Validation("Pagination incomplète."));
                }
                None
            }
        };
        let page = service(&state)?
            .drafts(&session, filter, cursor.as_ref(), PageSize::new(20)?)
            .await?;
        let mut v = view(&state, &session, "Rédaction");
        v.cards = page
            .items
            .into_iter()
            .map(|c| Card {
                title: c.title,
                href: route(&c.id),
                status: format!(
                    "{} · {}",
                    state_label(&c.state),
                    review_label(&c.review_state)
                ),
            })
            .collect();
        v.next = page.next.map(|c| {
            let mut pairs = vec![("filter", filter_key.to_owned()), ("after", c.id)];
            if let Some(t) = c.updated_at {
                pairs.push(("at", t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)));
            }
            with_query(
                "/admin/editor",
                &pairs
                    .iter()
                    .map(|(k, v)| (*k, v.as_str()))
                    .collect::<Vec<_>>(),
            )
        });
        Ok(v)
    }
    .await;
    match result {
        Ok(v) => response(v, StatusCode::OK),
        Err(e) => fail(&state, e),
    }
}
fn state_label(s: &str) -> &'static str {
    match s {
        "published" => "Publié",
        "in_review" => "En relecture",
        "withdrawn" => "Retiré",
        _ => "Privé",
    }
}
fn review_label(s: &str) -> &'static str {
    match s {
        "approved" => "Relecture validée",
        "in_review" => "À relire",
        "changes_requested" => "Corrections demandées",
        _ => "Brouillon",
    }
}
fn with_form(mut v: View, f: Fields) -> View {
    v.action = f
        .story
        .as_deref()
        .map(route)
        .unwrap_or_else(|| "/admin/editor".into());
    v.formats = [
        ArticleFormat::Brief,
        ArticleFormat::Standard,
        ArticleFormat::Analysis,
    ]
    .into_iter()
    .map(|k| Choice {
        key: k.as_str(),
        label: k.label(),
        selected: k.as_str() == f.format,
    })
    .collect();
    v.kinds = [
        ArticleKind::Reporting,
        ArticleKind::Analysis,
        ArticleKind::Opinion,
        ArticleKind::AutomatedSummary,
        ArticleKind::LegacySummary,
        ArticleKind::LegacySynthesis,
    ]
    .into_iter()
    .filter(|k| {
        if matches!(
            f.kind.as_str(),
            "legacy_summary" | "legacy_synthesis" | "automated_summary"
        ) {
            k.as_str() == f.kind
        } else {
            !matches!(
                k,
                ArticleKind::LegacySummary
                    | ArticleKind::LegacySynthesis
                    | ArticleKind::AutomatedSummary
            )
        }
    })
    .map(|k| Choice {
        key: k.as_str(),
        label: k.label(),
        selected: k.as_str() == f.kind,
    })
    .collect();
    v.details = f
        .story
        .as_deref()
        .map(|id| format!("/admin{}", path_segment("drafts", id)));
    v.sources = f
        .story
        .as_deref()
        .map(|id| format!("{}/sources", route(id)));
    v.workflow = f
        .story
        .as_deref()
        .map(|id| format!("{}/workflow", route(id)));
    v.form = Some(f);
    v
}
async fn new(State(state): State<AppState>, Extension(session): Extension<Session>) -> Response {
    let f = Fields {
        story: None,
        revision: String::new(),
        version: 0,
        title: String::new(),
        standfirst: String::new(),
        summary: String::new(),
        language: state.config.editorial.default_language.clone(),
        format: "brief".into(),
        kind: "reporting".into(),
        reason: "Création d’un nouveau brouillon.".into(),
        override_reason: String::new(),
        rows: vec![AuthorBlock::unused()],
    };
    response(
        with_form(view(&state, &session, "Nouveau brouillon"), f),
        StatusCode::OK,
    )
}
async fn edit(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result = async {
        let s = service(&state)?;
        let head = s.head(&session, &id).await?;
        let r = s.revision(&session, &head.current_revision_id).await?;
        let mut rows = authoring::flatten(&r.body);
        if rows.len() < 2001 {
            rows.push(AuthorBlock::unused());
        }
        let f = Fields {
            story: Some(id),
            revision: r.id,
            version: head.lock_version,
            title: r.title,
            standfirst: r.standfirst.unwrap_or_default(),
            summary: r.summary.unwrap_or_default(),
            language: r.language,
            format: r.format.as_str().into(),
            kind: r.kind.as_str().into(),
            reason: String::new(),
            override_reason: String::new(),
            rows,
        };
        Ok(with_form(
            view(&state, &session, "Modifier le brouillon"),
            f,
        ))
    }
    .await;
    match result {
        Ok(v) => response(v, StatusCode::OK),
        Err(e) => fail(&state, e),
    }
}
fn take(fields: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    fields
        .remove(key)
        .ok_or(AppError::Validation("Formulaire incomplet."))
}
// Validate decoded UTF-8 before the URL-form library's intentionally lossy decoding.
// This preserves an intentionally entered U+FFFD while rejecting malformed byte sequences.
pub(super) fn strict_encoding(bytes: &[u8]) -> bool {
    let hex = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    for pair in bytes.split(|b| *b == b'&') {
        for component in pair.splitn(2, |b| *b == b'=') {
            let mut decoded = Vec::with_capacity(component.len());
            let mut i = 0;
            while i < component.len() {
                match component[i] {
                    b'%' => {
                        if i + 2 >= component.len() {
                            return false;
                        }
                        let (Some(a), Some(b)) = (hex(component[i + 1]), hex(component[i + 2]))
                        else {
                            return false;
                        };
                        decoded.push(a * 16 + b);
                        i += 3;
                    }
                    b'+' => {
                        decoded.push(b' ');
                        i += 1;
                    }
                    b => {
                        decoded.push(b);
                        i += 1;
                    }
                }
            }
            if std::str::from_utf8(&decoded).is_err() {
                return false;
            }
        }
    }
    true
}
fn parse(bytes: &[u8], story: Option<String>) -> Result<(Fields, String), AppError> {
    if !strict_encoding(bytes) {
        return Err(AppError::Validation(
            "Encodage du formulaire invalide : aucune donnée n’a été remplacée.",
        ));
    }
    let mut values = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(bytes) {
        if values.len() >= 9000 || k.len() > 64 {
            return Err(AppError::Validation(
                "Trop de champs ou nom de champ trop long.",
            ));
        }
        if values.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation("Champ répété."));
        }
    }
    take(&mut values, "csrf")?;
    let intent = take(&mut values, "intent")?;
    let version = take(&mut values, "version")?
        .parse()
        .map_err(|_| AppError::Validation("Version invalide."))?;
    let count: usize = take(&mut values, "blocks")?
        .parse()
        .map_err(|_| AppError::Validation("Nombre de blocs invalide."))?;
    if count > 2001 {
        return Err(AppError::Validation("Trop de blocs."));
    }
    let mut f = Fields {
        story,
        version,
        revision: take(&mut values, "revision")?,
        title: take(&mut values, "title")?,
        standfirst: take(&mut values, "standfirst")?,
        summary: take(&mut values, "summary")?,
        language: take(&mut values, "language")?,
        format: take(&mut values, "format")?,
        kind: take(&mut values, "kind")?,
        reason: take(&mut values, "reason")?,
        override_reason: take(&mut values, "override_reason")?,
        rows: vec![],
    };
    for i in 0..count {
        let delete = match values.remove(&format!("delete_{i}")) {
            None => false,
            Some(v) if v == "yes" => true,
            _ => return Err(AppError::Validation("Suppression de bloc invalide.")),
        };
        f.rows.push(AuthorBlock {
            kind: BlockKind::parse(&take(&mut values, &format!("kind_{i}"))?)?,
            text: take(&mut values, &format!("text_{i}"))?,
            claim_id: take(&mut values, &format!("claim_{i}"))?,
            delete,
        });
    }
    if !values.is_empty() {
        return Err(AppError::Validation("Champ de formulaire inconnu."));
    }
    Ok((f, intent))
}
async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    input: Result<RawForm, RawFormRejection>,
) -> Response {
    submit(state, session, None, input).await
}
async fn save(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<RawForm, RawFormRejection>,
) -> Response {
    submit(state, session, Some(id), input).await
}
async fn submit(
    state: AppState,
    session: Session,
    story: Option<String>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let bytes = match raw {
        Ok(RawForm(b)) => b,
        Err(_) => return fail(&state, AppError::Validation("Formulaire illisible.")),
    };
    let (mut fields, intent) = match parse(&bytes, story) {
        Ok(v) => v,
        Err(e) => return fail(&state, e),
    };
    let result = async {
        let s = service(&state)?;
        let old = if let Some(id) = &fields.story {
            let head = s.head(&session, id).await?;
            if head.lock_version != fields.version || head.current_revision_id != fields.revision {
                return Err(AppError::Conflict);
            }
            Some(s.revision(&session, &fields.revision).await?)
        } else {
            if fields.version != 0 || !fields.revision.is_empty() {
                return Err(AppError::Validation(
                    "Identité de nouveau brouillon invalide.",
                ));
            }
            None
        };
        if intent == "add" {
            if fields.rows.len() >= 2001 {
                return Err(AppError::Validation("Limite de blocs atteinte."));
            }
            fields.rows.push(AuthorBlock::unused());
            return Ok(None);
        }
        if !matches!(intent.as_str(), "save" | "preview") {
            return Err(AppError::Validation("Action inconnue."));
        }
        // Browser form encoding uses CRLF; retain exact stored text when only line endings differ.
        if let Some(old) = &old {
            let original = authoring::flatten(&old.body);
            for (i, row) in fields.rows.iter_mut().enumerate() {
                if let Some(previous) = original.get(i)
                    && row.text.replace("\r\n", "\n") == previous.text.replace("\r\n", "\n")
                {
                    row.text = previous.text.clone();
                }
            }
        }
        let body = authoring::compose(&fields.rows)?;
        let format = match fields.format.as_str() {
            "brief" => ArticleFormat::Brief,
            "standard" => ArticleFormat::Standard,
            "analysis" => ArticleFormat::Analysis,
            _ => return Err(AppError::Validation("Format inconnu.")),
        };
        let kind = match fields.kind.as_str() {
            "reporting" => ArticleKind::Reporting,
            "analysis" => ArticleKind::Analysis,
            "opinion" => ArticleKind::Opinion,
            "automated_summary"
                if old
                    .as_ref()
                    .is_some_and(|r| r.kind == ArticleKind::AutomatedSummary) =>
            {
                ArticleKind::AutomatedSummary
            }
            "legacy_summary"
                if old
                    .as_ref()
                    .is_some_and(|r| r.kind == ArticleKind::LegacySummary) =>
            {
                ArticleKind::LegacySummary
            }
            "legacy_synthesis"
                if old
                    .as_ref()
                    .is_some_and(|r| r.kind == ArticleKind::LegacySynthesis) =>
            {
                ArticleKind::LegacySynthesis
            }
            _ => return Err(AppError::Validation("Type d’article invalide.")),
        };
        if old.as_ref().is_some_and(|r| {
            matches!(
                r.kind,
                ArticleKind::LegacySummary
                    | ArticleKind::LegacySynthesis
                    | ArticleKind::AutomatedSummary
            ) && r.kind != kind
        }) {
            return Err(AppError::Validation(
                "Une archive conserve son identité historique.",
            ));
        }
        let nonempty = |s: &String| (!s.is_empty()).then(|| s.clone());
        let input = RevisionInput {
            title: fields.title.clone(),
            standfirst: nonempty(&fields.standfirst),
            summary: nonempty(&fields.summary),
            body,
            language: fields.language.clone(),
            format,
            kind,
            topics: old.as_ref().map(|r| r.topics.clone()).unwrap_or_default(),
            geography: old
                .as_ref()
                .map(|r| r.geography.clone())
                .unwrap_or_default(),
            reason: if intent == "preview" && fields.reason.is_empty() {
                "Prévisualisation privée non enregistrée.".into()
            } else {
                fields.reason.clone()
            },
            evidence: None,
            length_override: nonempty(&fields.override_reason),
        };
        input.validate_content(old.is_some())?;
        if intent == "preview" {
            let assessment = crate::length::Assessment::new(
                &input.body,
                &input.language,
                input.format,
                &state.config.editorial,
                input.length_override.clone(),
            )?;
            let revision = Revision {
                id: fields.revision.clone(),
                story_id: fields.story.clone().unwrap_or_else(|| "new-preview".into()),
                title: input.title,
                standfirst: input.standfirst,
                summary: input.summary,
                body: input.body,
                language: input.language,
                format: input.format,
                kind: input.kind,
                topics: input.topics,
                geography: input.geography,
                created_at: chrono::Utc::now(),
            };
            fields.rows = authoring::flatten(&revision.body);
            if fields.rows.len() < 2001 {
                fields.rows.push(AuthorBlock::unused());
            }
            return Ok(Some((revision, assessment)));
        }
        let head = match &fields.story {
            Some(id) => s.revise(&session, id, fields.version, input).await?,
            None => {
                s.create(&session, &authoring::fresh_id("story")?, input)
                    .await?
            }
        };
        fields.story = Some(head.id);
        Ok(None)
    }
    .await;
    match result {
        Ok(preview) => {
            if intent == "save" {
                return Redirect::to(&route(fields.story.as_deref().expect("saved story")))
                    .into_response();
            }
            let mut v = with_form(view(&state, &session, "Écriture du brouillon"), fields);
            v.message=Some("Préparation privée non enregistrée. Les sources devront être rattachées et la nouvelle version relue.".into());
            if let Some((revision, assessment)) = preview {
                v.length = match crate::length::Report::new(
                    &revision,
                    &state.config.editorial,
                    Some(assessment),
                ) {
                    Ok(r) => Some(r),
                    Err(e) => return fail(&state, e),
                };
                v.evidence = match crate::citations::Report::uncited(&revision) {
                    Ok(r) => Some(r),
                    Err(e) => return fail(&state, e),
                };
                v.article = Some(revision);
            }
            response(v, StatusCode::OK)
        }
        Err(error) => {
            let status = match &error {
                AppError::Conflict => StatusCode::CONFLICT,
                AppError::Validation(_) => StatusCode::BAD_REQUEST,
                _ => return fail(&state, error),
            };
            let Some(auth) = &state.auth else {
                return fail(&state, AppError::Unavailable);
            };
            if let Err(error) = auth.authorise(&session, Permission::Edit).await {
                return super::failure(&state, error);
            }
            let mut v = with_form(view(&state, &session, "Saisie à récupérer"), fields);
            v.message = Some(if status == StatusCode::CONFLICT {
                "L’état actuel de l’article ou un travail concurrent empêche cet enregistrement. Votre saisie est conservée ci-dessous ; comparez-la avec la dernière version avant de la reporter.".into()
            } else {
                format!("Saisie non enregistrée, conservée ci-dessous. {error}")
            });
            response(v, status)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn form_encoding_never_silently_substitutes_invalid_text() {
        assert!(strict_encoding(b"text=%C3%A9cole+%26+%EF%BF%BD"));
        assert!(strict_encoding("text=école".as_bytes()));
        assert!(!strict_encoding(b"text=%FF"));
        assert!(!strict_encoding(b"text=%C3%28"));
        assert!(!strict_encoding(b"text=%z0"));
        assert!(!strict_encoding(b"text=abc%"));
    }
}
