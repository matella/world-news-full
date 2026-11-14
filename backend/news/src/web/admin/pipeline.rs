use super::super::{AppState, message, path_segment, with_query};

use crate::{
    auth::{Role, Session},
    content::{ArticleFormat, ArticleKind, ContentBlock, PageSize},
    editorial::{EditorialService, RevisionInput},
    error::AppError,
    intake::{
        extraction_jobs::ExtractionAdmin,
        feed_admin::FeedAdmin,
        jobs::JobAdmin,
        manual::{IntakeService, ManualLink, OriginalDraft},
    },
    operations::Operations,
    registry::{DisplayPolicy, FeedSpec, Registry, RightsStatus, SourceSpec, StoragePolicy},
};

use askama::Template;

use axum::{
    Extension, Form, Router,
    extract::{
        Path, Query, State,
        rejection::{FormRejection, QueryRejection},
    },
    http::{StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};

use serde::Deserialize;

use sqlx::PgPool;

#[derive(Template)]
#[template(path = "pipeline.html")]
struct View {
    masthead: String,
    username: String,
    csrf: String,
    title: String,
    paragraphs: Vec<String>,
    links: Vec<Link>,
    tables: Vec<Table>,
    forms: Vec<FormSpec>,
    next: Option<String>,
}
struct Link {
    label: String,
    href: String,
}
struct Cell {
    label: String,
    href: Option<String>,
}
struct Table {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<Cell>>,
}
struct FormSpec {
    title: String,
    action: String,
    button: String,
    fields: Vec<Field>,
}
struct Choice {
    value: String,
    label: String,
    selected: bool,
}
struct Field {
    name: String,
    label: String,
    value: String,
    kind: &'static str,
    required: bool,
    checked: bool,
    readonly: bool,
    choices: Vec<Choice>,
}
fn field(
    name: &str,
    label: &str,
    value: impl ToString,
    kind: &'static str,
    required: bool,
) -> Field {
    Field {
        name: name.into(),
        label: label.into(),
        value: value.to_string(),
        kind,
        required,
        checked: false,
        readonly: false,
        choices: vec![],
    }
}
fn hidden(name: &str, value: impl ToString) -> Field {
    field(name, "", value, "hidden", true)
}
fn checkbox(name: &str, label: &str, checked: bool) -> Field {
    let mut f = field(name, label, "yes", "checkbox", false);

    f.checked = checked;

    f
}
fn select(name: &str, label: &str, value: &str, choices: &[(&str, &str)]) -> Field {
    let mut f = field(name, label, "", "select", true);

    f.choices = choices
        .iter()
        .map(|(v, l)| Choice {
            value: (*v).into(),
            label: (*l).into(),
            selected: *v == value,
        })
        .collect();

    f
}
fn cell(label: impl ToString) -> Cell {
    Cell {
        label: label.to_string(),
        href: None,
    }
}
fn linked(label: impl ToString, href: String) -> Cell {
    Cell {
        label: label.to_string(),
        href: Some(href),
    }
}
fn table(title: &str, headers: &[&str], rows: Vec<Vec<Cell>>) -> Table {
    Table {
        title: title.into(),
        headers: headers.iter().map(|s| s.to_string()).collect(),
        rows,
    }
}
fn route(section: &str, id: &str) -> String {
    format!("/admin{}", path_segment(section, id))
}
fn action(section: &str, id: &str, verb: &str) -> String {
    format!("{}/{verb}", route(section, id))
}
fn page(state: &AppState, session: &Session, title: impl Into<String>) -> View {
    View {
        masthead: state.config.masthead.clone(),
        username: session.principal().id().into(),
        csrf: session.csrf().into(),
        title: title.into(),
        paragraphs: vec![],
        links: vec![],
        tables: vec![],
        forms: vec![],
        next: None,
    }
}
fn publisher(session: &Session) -> bool {
    session.principal().role() == Role::Publisher
}
fn pool(state: &AppState) -> Result<PgPool, AppError> {
    state.editor_pool.clone().ok_or(AppError::Unavailable)
}
fn render(state: &AppState, result: Result<View, AppError>) -> Response {
    match result {
        Ok(v) => match v.render() {
            Ok(html) => Html(html).into_response(),
            Err(_) => error(state, AppError::Unavailable),
        },
        Err(e) => error(state, e),
    }
}
fn error(state: &AppState, error: AppError) -> Response {
    let (status, title, text) = match error {
        AppError::Validation(_) => (
            StatusCode::BAD_REQUEST,
            "Action invalide",
            "Vérifiez les champs, les droits d’usage et les délais actuels de la source.",
        ),
        AppError::Conflict => (
            StatusCode::CONFLICT,
            "Données modifiées",
            "L’état a changé. Actualisez la page avant de réessayer.",
        ),
        AppError::Forbidden => (
            StatusCode::FORBIDDEN,
            "Action refusée",
            "Votre compte ne permet pas cette action.",
        ),
        AppError::NotFound => (
            StatusCode::NOT_FOUND,
            "Élément introuvable",
            "Cet élément est introuvable.",
        ),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Administration indisponible",
            "Les données sont temporairement indisponibles. Réessayez plus tard.",
        ),
    };

    message(&state.config, status, title, text)
}
fn redirect(state: &AppState, result: Result<String, AppError>) -> Response {
    match result {
        Ok(path) => Redirect::to(&path).into_response(),
        Err(e) => error(state, e),
    }
}
fn size() -> PageSize {
    PageSize::new(20).expect("fixed page size")
}
fn stamp(state: &AppState, value: chrono::DateTime<chrono::Utc>) -> String {
    let zone = state
        .config
        .editorial
        .publication_timezone
        .parse::<chrono_tz::Tz>()
        .expect("validated publication timezone");

    value
        .with_timezone(&zone)
        .format("%d/%m/%Y %H:%M %Z")
        .to_string()
}
fn optional(state: &AppState, value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value
        .map(|value| stamp(state, value))
        .unwrap_or_else(|| "Aucun contrôle réussi".into())
}
fn status(value: &str) -> &str {
    match value {
        "metadata_only" => "Métadonnées seulement",
        "pending" | "queued" => "En attente",
        "ready" => "Texte disponible",
        "failed" => "Échec",
        "running" => "Réservé",
        "retry" => "À reprendre",
        "cancelled" => "Annulé",
        "succeeded" => "Terminé",
        "modified" => "Nouveau lot reçu",
        "not_modified" => "Source inchangée",
        "started" => "Contrôle réservé",
        "lease_expired" => "Réservation expirée",
        _ => value,
    }
}
fn detail(code: Option<&str>) -> &str {
    match code {
        None | Some("") => "",
        Some("lease_expired") => "Réservation expirée",
        Some("held_disabled") => "Source désactivée",
        Some("context_changed" | "configuration_changed") => "Document ou configuration modifié",
        Some("authority_expired") => "Autorisation expirée ou retirée",
        Some("operator_cancelled" | "operator_paused") => "Arrêt demandé par un responsable",
        Some("source_retry_wait" | "retry_after_requires_review") => "Délai demandé par la source",
        Some("retry_budget_exhausted") => "Nombre d’essais épuisé",
        Some("timeout" | "parser_timeout") => "Temps de traitement dépassé",
        Some("network" | "resolution") => "Source inaccessible",
        Some("http_status") => "Réponse HTTP en échec",
        Some("no_article" | "unsupported_document" | "invalid_feed") => {
            "Format de source non exploitable"
        }
        Some("access_restricted" | "blocked_target" | "rights_or_scope_denied") => {
            "Accès ou droits insuffisants"
        }
        Some("superseded") => "Tentative remplacée",
        Some(_) => "Échec à examiner",
    }
}
fn intake_kind(kind: &str) -> &str {
    match kind {
        "registered_link" => "Lien ajouté manuellement",
        "manual" => "Notes de rédaction",
        "extracted" => "Texte extrait",
        "failed" => "Échec d’extraction",
        "denied" => "Extraction non autorisée",
        _ => "Historique",
    }
}
fn random_id(prefix: &str) -> Result<String, AppError> {
    let mut bytes = [0u8; 16];

    getrandom::fill(&mut bytes).map_err(|_| AppError::Unavailable)?;

    Ok(format!(
        "{prefix}-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn form<T>(input: Result<Form<T>, FormRejection>) -> Result<T, AppError> {
    input
        .map(|Form(v)| v)
        .map_err(|_| AppError::Validation("Invalid form."))
}
fn query<T>(input: Result<Query<T>, QueryRejection>) -> Result<T, AppError> {
    input
        .map(|Query(v)| v)
        .map_err(|_| AppError::Validation("Invalid query."))
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Cursor {
    after: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ArticleFilter {
    after: Option<String>,
    state: Option<String>,
    source: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct JobFilter {
    before: Option<i64>,
    state: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Before {
    before: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: i64,
    reason: String,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/assets.css", get(css))
        .route("/admin/pipeline", get(overview))
        .route("/admin/sources", get(sources).post(create_source))
        .route("/admin/sources/{id}", get(source).post(update_source))
        .route("/admin/sources/{id}/confirm", post(confirm_source))
        .route("/admin/sources/{id}/disable", post(disable_source))
        .route("/admin/feeds", get(feeds).post(create_feed))
        .route("/admin/feeds/{id}", get(feed).post(update_feed))
        .route("/admin/feeds/{id}/disable", post(disable_feed))
        .route("/admin/feeds/{id}/pause", post(pause_feed))
        .route("/admin/feeds/{id}/resume", post(resume_feed))
        .route("/admin/articles", get(articles).post(register_link))
        .route("/admin/articles/{id}", get(article))
        .route("/admin/articles/{id}/notes", post(notes))
        .route("/admin/articles/{id}/draft", post(draft))
        .route("/admin/articles/{id}/extract", post(request_extraction))
        .route("/admin/articles/{id}/snapshots/{snapshot}", get(snapshot))
        .route("/admin/jobs", get(jobs))
        .route("/admin/jobs/{id}", get(job))
        .route("/admin/jobs/{id}/replay", post(replay_job))
        .route("/admin/jobs/{id}/cancel", post(cancel_job))
        .route("/admin/extractions", get(extractions))
        .route("/admin/extractions/{id}", get(extraction))
        .route("/admin/extractions/{id}/cancel", post(cancel_extraction))
        .route("/admin/batches", get(batches))
        .route("/admin/batches/{id}/enqueue", post(enqueue_batch))
        .route("/admin/drafts/{id}", get(view_draft))
}
async fn css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("admin.css"),
    )
}
async fn overview(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
) -> Response {
    let result=async{let measured=Operations::new(pool(&state)?).overview(&session).await?;

let mut v=page(&state,&session,"Suivi des sources et des traitements");
v.paragraphs.push("Les chiffres décrivent les données enregistrées ici. Une réservation ou un contrôle de flux ne vaut pas publication.".into());
v.tables.push(table("État enregistré",&["Mesure","Valeur"],vec![vec![cell("Sources configurées"),cell(measured.sources)],vec![cell("Flux activés dans la configuration"),cell(measured.active_feeds)],vec![cell("Flux en pause"),cell(measured.paused_feeds)],vec![cell("Documents sources"),cell(measured.source_articles)],vec![cell("Observations en quarantaine"),cell(measured.quarantined)],vec![cell("Normalisations en attente ou réservées"),cell(measured.normalization_waiting)],vec![cell("Extractions en attente ou réservées"),cell(measured.extraction_waiting)],vec![cell("Dernier contrôle de flux réussi"),cell(optional(&state,measured.last_feed_success))]]));
Ok(v)}.await;

    render(&state, result)
}
async fn sources(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<Cursor>, QueryRejection>,
) -> Response {
    let result=async{let q=query(params)?;

let rows=Registry::new(pool(&state)?).sources(&session,q.after.as_deref(),size()).await?;

let mut v=page(&state,&session,"Sources");
v.paragraphs.push("Les langues et les zones géographiques sont des informations distinctes. Une source configurée ne prouve pas une origine de reportage indépendante.".into());

if rows.len()==20{v.next=rows.last().map(|r|with_query("/admin/sources",&[("after",&r.id)]));
}v.tables.push(table("Sources configurées",&["Source","Activation","Stockage"],rows.iter().map(|r|vec![linked(&r.spec.name,route("sources",&r.id)),cell(if r.spec.enabled{"Activée"}else{"Désactivée"}),cell(storage(r.spec.effective_storage()))]).collect()));

if publisher(&session){v.forms.push(source_form("Ajouter une source","/admin/sources".into(),None,None));
}Ok(v)}.await;

    render(&state, result)
}
fn storage(value: StoragePolicy) -> &'static str {
    match value {
        StoragePolicy::MetadataOnly => "Métadonnées et liens",
        StoragePolicy::LicensedText => "Texte sous licence",
        StoragePolicy::Original => "Texte original",
    }
}
async fn source(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result = async {
        let registry = Registry::new(pool(&state)?);

        let row = registry.source(&session, &id).await?;

        let mut v = page(&state, &session, &row.spec.name);

        v.paragraphs.push(format!(
            "Référence : {} · Stockage effectif : {}.",
            row.id,
            storage(row.spec.effective_storage())
        ));

        v.links.push(Link {
            label: "Documents de cette source".into(),
            href: with_query("/admin/articles", &[("source", &row.id)]),
        });

        if publisher(&session) {
            v.forms.push(source_form(
                "Modifier la source",
                route("sources", &row.id),
                Some(row.version),
                Some(&row.spec),
            ));

            v.forms.push(FormSpec {
                title: "Désactiver la source".into(),
                action: action("sources", &row.id, "disable"),
                button: "Désactiver".into(),
                fields: vec![hidden("version", row.version)],
            });

            v.forms.push(feed_form(
                "Ajouter un flux",
                "/admin/feeds".into(),
                None,
                Some(&row.id),
                None,
            ));
        }
        Ok(v)
    }
    .await;

    render(&state, result)
}
async fn feeds(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<Cursor>, QueryRejection>,
) -> Response {
    let result=async {
        let q=query(params)?;
        let rows=Operations::new(pool(&state)?).feeds(&session,q.after.as_deref(),size()).await?;
        let mut v=page(&state,&session,"Flux");
        v.paragraphs.push("L’activation est une configuration. Un contrôle ancien signifie plus de deux intervalles configurés depuis le dernier contrôle réussi.".into());
        v.links.push(Link{label:"Choisir une source pour ajouter un flux".into(),href:"/admin/sources".into()});
        if rows.len()==20 {v.next=rows.last().map(|r|with_query("/admin/feeds",&[("after",&r.id)]));}
        let mut body=vec![];
        for r in rows {
            body.push(vec![linked(&r.id,route("feeds",&r.id)),linked(&r.source_id,route("sources",&r.source_id)),cell(if !r.enabled||!r.source_enabled{"Désactivé"}else if r.paused{"En pause"}else{"Activé dans la configuration"}),cell(r.last_outcome.as_deref().map(status).unwrap_or("Aucun contrôle")),cell(detail(r.last_error.as_deref())),cell(optional(&state,r.last_success_at)),cell(if r.stale{"Contrôle réussi ancien"}else{""}),cell(stamp(&state,r.next_fetch_at))]);
        }
        v.tables.push(table("Flux configurés",&["Référence","Source","Activation","Dernier résultat","Détail","Dernier contrôle réussi","À surveiller","Pas avant"],body));
        Ok(v)
    }.await;
    render(&state, result)
}

async fn feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result=async{let r=Registry::new(pool(&state)?).feed(&session,&id).await?;

let admin=FeedAdmin::new(pool(&state)?);

let s=admin.status(&session,&id).await?;

let history=admin.history(&session,&id,None,size()).await?;

let mut v=page(&state,&session,format!("Flux {}",r.id));
v.paragraphs.push(format!("Échecs consécutifs : {}. Prochain contrôle possible : {}. Les délais de la source restent applicables.",s.failure_count,stamp(&state,s.next_fetch_at)));

if let Some(reason)=s.pause_reason{v.paragraphs.push(format!("Motif de pause : {reason}"));
}v.tables.push(table("Dernières tentatives",&["Tentative","Résultat","Détail","Début","Fin"],history.into_iter().map(|h|vec![cell(h.id),cell(status(&h.outcome)),cell(detail(h.error_category.as_deref())),cell(stamp(&state,h.started_at)),cell(h.finished_at.map(|value|stamp(&state,value)).unwrap_or_else(||"Réservation sans résultat confirmé".into()))]).collect()));

if publisher(&session){v.forms.push(feed_form("Modifier le flux",route("feeds",&id),Some(r.version),Some(&r.spec.source_id),Some(&r.spec)));
v.forms.push(FormSpec{title:if s.paused{"Reprendre le suivi"}else{"Mettre le suivi en pause"}.into(),action:action("feeds",&id,if s.paused{"resume"}else{"pause"}),button:if s.paused{"Reprendre selon les délais de la source"}else{"Mettre en pause"}.into(),fields:vec![hidden("version",s.control_version),field("reason","Motif","","textarea",true)]});
v.forms.push(FormSpec{title:"Désactiver ce flux".into(),action:action("feeds",&id,"disable"),button:"Désactiver".into(),fields:vec![hidden("version",r.version)]});
}Ok(v)}.await;

    render(&state, result)
}
async fn articles(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<ArticleFilter>, QueryRejection>,
) -> Response {
    let result=async {
        let q=query(params)?;

        let rows=Operations::new(pool(&state)?).articles(&session,q.after.as_deref(),q.state.as_deref(),q.source.as_deref(),size()).await?;

        let mut v=page(&state,&session,"Documents sources");

        v.paragraphs.push("Les métadonnées et textes sources servent à la rédaction. Ils ne constituent pas des articles publiés.".into());

        if rows.len()==20 && let Some(last)=rows.last() {
            let mut pairs=vec![("after",last.id.as_str())];

            if let Some(value)=q.state.as_deref(){pairs.push(("state",value));
}
            if let Some(value)=q.source.as_deref(){pairs.push(("source",value));
}
            v.next=Some(with_query("/admin/articles",&pairs));

        }
        v.links.extend([Link{label:"Échecs d’extraction".into(),href:with_query("/admin/articles",&[("state","failed")])},Link{label:"Métadonnées seulement".into(),href:with_query("/admin/articles",&[("state","metadata_only")])},Link{label:"Tous les documents".into(),href:"/admin/articles".into()}]);

        v.tables.push(table("Documents",&["Titre","Attribution enregistrée","Langue","État","Reçu le"],rows.into_iter().map(|r|vec![linked(r.title,route("articles",&r.id)),cell(r.attribution_name),cell(r.language),cell(status(&r.extraction_state)),cell(stamp(&state,r.fetched_at))]).collect()));

        if let Some(source_id)=q.source.as_deref() {
            v.forms.push(FormSpec{title:"Ajouter un lien source manuellement".into(),action:"/admin/articles".into(),button:"Enregistrer le lien et mes notes".into(),fields:vec![hidden("source_id",source_id),field("url","Lien original","","url",true),field("title","Titre indiqué par la source","","text",true),field("language","Langue (ou und si indéterminée)","und","text",true),field("notes","Mes notes originales","","textarea",true)]});

        }else{v.links.push(Link{label:"Choisir une source pour ajouter un lien".into(),href:"/admin/sources".into()});
}
        Ok(v)
    }.await;

    render(&state, result)
}

async fn article(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result =
        async {
            let r = Operations::new(pool(&state)?)
                .article(&session, &id)
                .await?;

            let configured = Registry::new(pool(&state)?)
                .source(&session, &r.source_id)
                .await?;

            let can_extract = configured.spec.enabled
                && configured.spec.validate().is_ok()
                && configured.spec.effective_storage() != StoragePolicy::MetadataOnly
                && url::Url::parse(&r.original_url)
                    .ok()
                    .zip(url::Url::parse(&configured.spec.homepage_url).ok())
                    .is_some_and(|(a, b)| a.host_str() == b.host_str());

            let intake = IntakeService::new(pool(&state)?, &state.config.public_url)
                .map_err(|_| AppError::Unavailable)?;

            let history = intake.history(&session, &id, None, size()).await?;

            let mut v = page(&state, &session, &r.title);

            v.paragraphs.push(format!(
                "Attribution enregistrée : {} · Langue : {} · État : {}.",
                r.attribution_name,
                r.language,
                status(&r.extraction_state)
            ));

            if !crate::content::safe_source_url(&r.original_url) {
                return Err(AppError::InvalidData);
            }
            for draft in Operations::new(pool(&state)?)
                .draft_links(&session, &id)
                .await?
            {
                v.links.push(Link {
                    label: format!("Brouillon lié · {}", stamp(&state, draft.created_at)),
                    href: route("drafts", &draft.story_id),
                });
            }
            v.links.push(Link {
                label: "Ouvrir le lien original".into(),
                href: r.original_url,
            });

            v.links.push(Link {
                label: "Source configurée".into(),
                href: route("sources", &r.source_id),
            });

            v.tables.push(table(
                "Historique privé",
                &["Entrée", "Type", "Origine", "Détail", "Enregistrée le"],
                history
                    .into_iter()
                    .map(|h| {
                        vec![
                            linked(
                                h.id,
                                format!("{}/snapshots/{}", route("articles", &id), h.id),
                            ),
                            cell(intake_kind(&h.kind)),
                            cell(h.actor_id.map(|a| format!("Éditeur : {a}")).unwrap_or_else(
                                || format!("Traitement #{}", h.worker_job_id.unwrap_or_default()),
                            )),
                            cell(detail(h.error_category.as_deref())),
                            cell(stamp(&state, h.created_at)),
                        ]
                    })
                    .collect(),
            ));

            v.forms.push(FormSpec {
                title: "Ajouter mes notes".into(),
                action: action("articles", &id, "notes"),
                button: "Enregistrer mes notes".into(),
                fields: vec![
                    hidden("version", r.intake_version),
                    field("notes", "Notes rédigées par l’équipe", "", "textarea", true),
                ],
            });

            v.forms.push(FormSpec {
                title: "Créer un brouillon original lié à ce document".into(),
                action: action("articles", &id, "draft"),
                button: "Créer le brouillon privé".into(),
                fields: vec![
                    hidden("version", r.intake_version),
                    field(
                        "notes",
                        "Contexte et notes de rédaction",
                        "",
                        "textarea",
                        true,
                    ),
                    field("title", "Notre titre original", "", "text", true),
                    field("body", "Notre texte original", "", "textarea", true),
                    field("language", "Langue du brouillon", "fr", "text", true),
                    select(
                        "format",
                        "Format",
                        "standard",
                        &[
                            ("brief", "Brève"),
                            ("standard", "Article standard"),
                            ("analysis", "Format long"),
                        ],
                    ),
                ],
            });

            if publisher(&session) && can_extract {
                v.forms.push(FormSpec {
                    title: "Autoriser une extraction privée".into(),
                    action: action("articles", &id, "extract"),
                    button: "Autoriser un traitement pendant une heure".into(),
                    fields: vec![
                        hidden("version", r.intake_version),
                        field("reason", "Motif et usage prévu", "", "textarea", true),
                    ],
                });
            }
            Ok(v)
        }
        .await;

    render(&state, result)
}
async fn snapshot(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path((id, snapshot)): Path<(String, i64)>,
) -> Response {
    let result = async {
        let r = IntakeService::new(pool(&state)?, &state.config.public_url)
            .map_err(|_| AppError::Unavailable)?
            .snapshot(&session, &id, snapshot)
            .await?;

        let mut v = page(&state, &session, format!("Historique privé #{snapshot}"));

        v.links.push(Link {
            label: "Retour au document source".into(),
            href: route("articles", &id),
        });

        if let Some(notes) = r.notes {
            v.paragraphs.push(notes);
        }
        if r.text_withheld {
            v.paragraphs.push(
                "Le texte extrait a été retiré ou n’est plus accessible selon les droits actuels."
                    .into(),
            );
        }
        if let Some(text) = r.extracted {
            v.paragraphs.push(
                "Texte source non vérifié, conservé pour un usage interne de rédaction.".into(),
            );

            v.paragraphs.extend(text.paragraphs);
        }
        Ok(v)
    }
    .await;

    render(&state, result)
}
async fn jobs_page(
    state: &AppState,
    session: &Session,
    q: JobFilter,
    extraction: bool,
) -> Result<View, AppError> {
    let rows = Operations::new(pool(state)?)
        .jobs(session, extraction, q.before, q.state.as_deref(), size())
        .await?;

    let section = if extraction { "extractions" } else { "jobs" };

    let mut v = page(
        state,
        session,
        if extraction {
            "Extractions autorisées"
        } else {
            "Normalisation des lots"
        },
    );

    v.paragraphs.push(if extraction{"Une autorisation expire après une heure. Le texte extrait reste privé et ne devient pas un brouillon original."}else{"Un traitement terminé peut avoir placé des observations en quarantaine. Consultez les documents et motifs avant toute rédaction."}.into());

    v.links.extend([
        Link {
            label: "Échecs".into(),
            href: with_query(&format!("/admin/{section}"), &[("state", "failed")]),
        },
        Link {
            label: "En attente".into(),
            href: with_query(&format!("/admin/{section}"), &[("state", "queued")]),
        },
        Link {
            label: "Tous les traitements".into(),
            href: format!("/admin/{section}"),
        },
    ]);

    if rows.len() == 20
        && let Some(last) = rows.last()
    {
        let cursor = last.id.to_string();

        let mut pairs = vec![("before", cursor.as_str())];

        if let Some(s) = q.state.as_deref() {
            pairs.push(("state", s));
        }
        v.next = Some(with_query(&format!("/admin/{section}"), &pairs));
    }
    v.tables.push(table(
        "Traitements enregistrés",
        &[
            "Traitement",
            "Document ou lot",
            "État",
            "Tentatives",
            "Détail",
            "En quarantaine",
            "Créé le",
            "Pas avant",
        ],
        rows.into_iter()
            .map(|r| {
                vec![
                    linked(r.id, route(section, &r.id.to_string())),
                    if extraction {
                        linked(&r.target, route("articles", &r.target))
                    } else {
                        cell(&r.target)
                    },
                    cell(if r.expired {
                        "Réservation expirée"
                    } else {
                        status(&r.state)
                    }),
                    cell(r.attempts),
                    cell(detail(r.error.as_deref())),
                    cell(r.quarantined),
                    cell(stamp(state, r.created_at)),
                    cell(stamp(state, r.available_at)),
                ]
            })
            .collect(),
    ));

    Ok(v)
}
async fn jobs(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<JobFilter>, QueryRejection>,
) -> Response {
    let result = async { jobs_page(&state, &session, query(params)?, false).await }.await;

    render(&state, result)
}
async fn extractions(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<JobFilter>, QueryRejection>,
) -> Response {
    let result = async { jobs_page(&state, &session, query(params)?, true).await }.await;

    render(&state, result)
}
async fn job(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
) -> Response {
    let result = async {
        let admin = JobAdmin::new(pool(&state)?);

        let r = admin.status(&session, id).await?;

        let history = admin.history(&session, id, None, size()).await?;

        let mut v = page(&state, &session, format!("Normalisation #{id}"));

        v.paragraphs.push(format!(
            "Lot {} · État : {} · Tentatives du cycle : {}/{}.",
            r.batch_id,
            status(&r.state),
            r.attempt_count,
            r.max_attempts
        ));

        v.tables.push(attempt_table(&state, history));

        if publisher(&session) {
            if matches!(r.state.as_str(), "failed" | "succeeded" | "cancelled") {
                v.forms.push(job_action(
                    "Reprendre ce lot",
                    "jobs",
                    id,
                    "replay",
                    "Reprendre le traitement",
                    r.lock_version,
                ));
            } else {
                v.forms.push(job_action(
                    "Annuler ce traitement",
                    "jobs",
                    id,
                    "cancel",
                    "Annuler",
                    r.lock_version,
                ));
            }
        }
        Ok(v)
    }
    .await;

    render(&state, result)
}
fn attempt_table(state: &AppState, history: Vec<crate::intake::jobs::Attempt>) -> Table {
    table(
        "Historique des tentatives",
        &["Génération", "Résultat", "Détail", "Début", "Fin"],
        history
            .into_iter()
            .map(|h| {
                vec![
                    cell(h.generation),
                    cell(status(&h.outcome)),
                    cell(detail(h.error_category.as_deref())),
                    cell(stamp(state, h.started_at)),
                    cell(
                        h.finished_at
                            .map(|value| stamp(state, value))
                            .unwrap_or_else(|| "Résultat non confirmé".into()),
                    ),
                ]
            })
            .collect(),
    )
}
fn job_action(
    title: &str,
    section: &str,
    id: i64,
    verb: &str,
    button: &str,
    version: i64,
) -> FormSpec {
    FormSpec {
        title: title.into(),
        action: action(section, &id.to_string(), verb),
        button: button.into(),
        fields: vec![
            hidden("version", version),
            field("reason", "Motif", "", "textarea", true),
        ],
    }
}
async fn extraction(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
) -> Response {
    let result = async {
        let admin = ExtractionAdmin::new(pool(&state)?);

        let r = admin.status(&session, id).await?;

        let history = admin.history(&session, id, None, size()).await?;

        let mut v = page(&state, &session, format!("Extraction #{id}"));

        v.paragraphs.push(format!(
            "État : {} · Autorisation accordée par {} jusqu’au {}.",
            status(&r.state),
            r.requested_by,
            stamp(&state, r.authorized_until)
        ));

        v.links.push(Link {
            label: "Document source".into(),
            href: route("articles", &r.source_article_id),
        });

        v.tables.push(attempt_table(&state, history));

        if publisher(&session) && matches!(r.state.as_str(), "queued" | "running" | "retry") {
            v.forms.push(job_action(
                "Annuler cette autorisation",
                "extractions",
                id,
                "cancel",
                "Annuler",
                r.lock_version,
            ));
        }
        Ok(v)
    }
    .await;

    render(&state, result)
}
async fn batches(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<Before>, QueryRejection>,
) -> Response {
    let result = async {
        let q = query(params)?;

        let rows = Operations::new(pool(&state)?)
            .batches(&session, q.before, size())
            .await?;

        let mut v = page(&state, &session, "Lots de métadonnées reçus");

        if rows.len() == 20 {
            v.next = rows
                .last()
                .map(|r| with_query("/admin/batches", &[("before", &r.id.to_string())]));
        }
        v.tables.push(table(
            "Lots conservés",
            &["Lot", "Flux", "Résultat", "Reçu le", "Normalisation"],
            rows.iter()
                .map(|r| {
                    vec![
                        cell(r.id),
                        linked(&r.feed_id, route("feeds", &r.feed_id)),
                        cell(status(&r.outcome)),
                        cell(stamp(&state, r.created_at)),
                        r.job_id
                            .map(|id| {
                                linked(format!("Traitement #{id}"), route("jobs", &id.to_string()))
                            })
                            .unwrap_or_else(|| cell("Aucun traitement demandé")),
                    ]
                })
                .collect(),
        ));

        if publisher(&session) {
            for row in rows.iter().filter(|r| r.job_id.is_none()) {
                v.forms.push(FormSpec {
                    title: format!("Normaliser le lot {}", row.id),
                    action: action("batches", &row.id.to_string(), "enqueue"),
                    button: "Demander la normalisation".into(),
                    fields: vec![field("reason", "Motif", "", "textarea", true)],
                });
            }
        }
        Ok(v)
    }
    .await;

    render(&state, result)
}
async fn view_draft(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Response {
    let result = async {
        let service = EditorialService::new(pool(&state)?);

        let head = service.head(&session, &id).await?;

        let revision = service
            .revision(&session, &head.current_revision_id)
            .await?;

        let mut v = page(&state, &session, format!("Brouillon : {}", revision.title));

        v.paragraphs.push(format!(
            "Version privée · {} · {}. La création d’un brouillon ne le publie pas.",
            revision.language,
            revision.format.as_str()
        ));

        v.paragraphs
            .extend(revision.body.into_iter().map(|b| match b {
                ContentBlock::Paragraph { text } | ContentBlock::Heading { text, .. } => text,
            }));

        Ok(v)
    }
    .await;

    render(&state, result)
}

fn source_form(
    title: &str,
    action: String,
    version: Option<i64>,
    spec: Option<&SourceSpec>,
) -> FormSpec {
    let mut fields = vec![];

    if let Some(v) = version {
        fields.push(hidden("version", v));
    }
    fields.extend([
        field(
            "name",
            "Nom de la source",
            spec.map(|s| s.name.as_str()).unwrap_or(""),
            "text",
            true,
        ),
        field(
            "url",
            "Page d’accueil",
            spec.map(|s| s.homepage_url.as_str()).unwrap_or(""),
            "url",
            true,
        ),
        checkbox(
            "enabled",
            "Activer la source dans la configuration",
            spec.is_some_and(|s| s.enabled),
        ),
        select(
            "rights_status",
            "Droits documentés",
            spec.map(|s| enum_value(&s.rights_status))
                .as_deref()
                .unwrap_or("unknown"),
            &[
                ("unknown", "Droits inconnus"),
                ("documented", "Licence documentée"),
                ("original", "Contenu original de l’équipe"),
            ],
        ),
        select(
            "storage",
            "Conservation autorisée",
            spec.map(|s| enum_value(&s.usage_policy))
                .as_deref()
                .unwrap_or("metadata_only"),
            &[
                ("metadata_only", "Métadonnées et liens"),
                ("licensed_text", "Texte sous licence"),
                ("original", "Texte original"),
            ],
        ),
        select(
            "display",
            "Usage d’affichage autorisé",
            spec.map(|s| enum_value(&s.display_policy))
                .as_deref()
                .unwrap_or("link_only"),
            &[
                ("link_only", "Lien seulement"),
                ("summary", "Résumé"),
                ("full_text", "Texte intégral autorisé"),
            ],
        ),
        field(
            "terms_url",
            "Lien des conditions ou de la licence",
            spec.and_then(|s| s.terms_url.as_deref()).unwrap_or(""),
            "url",
            false,
        ),
        field(
            "rights_note",
            "Preuve et limites des droits",
            spec.and_then(|s| s.rights_note.as_deref()).unwrap_or(""),
            "textarea",
            false,
        ),
        field(
            "languages",
            "Langues, une par ligne",
            spec.map(|s| s.languages.join("\n")).unwrap_or_default(),
            "textarea",
            false,
        ),
        field(
            "geography",
            "Zones géographiques, une par ligne",
            spec.map(|s| s.geographic_coverage.join("\n"))
                .unwrap_or_default(),
            "textarea",
            false,
        ),
        field(
            "attribution",
            "Mention d’attribution",
            spec.and_then(|s| s.attribution.as_deref()).unwrap_or(""),
            "text",
            false,
        ),
        field(
            "ownership_note",
            "Propriété de la source",
            spec.and_then(|s| s.ownership_note.as_deref()).unwrap_or(""),
            "textarea",
            false,
        ),
        field(
            "editorial_note",
            "Note de rédaction",
            spec.and_then(|s| s.editorial_note.as_deref()).unwrap_or(""),
            "textarea",
            false,
        ),
    ]);

    FormSpec {
        title: title.into(),
        action,
        button: "Enregistrer la source".into(),
        fields,
    }
}
fn enum_value(value: &impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn feed_form(
    title: &str,
    action: String,
    version: Option<i64>,
    source: Option<&str>,
    spec: Option<&FeedSpec>,
) -> FormSpec {
    let mut fields = vec![];

    if let Some(v) = version {
        fields.push(hidden("version", v));
    }
    fields.extend([
        hidden("source_id", source.unwrap_or("")),
        field(
            "url",
            "Adresse RSS ou Atom",
            spec.map(|s| s.url.as_str()).unwrap_or(""),
            "url",
            true,
        ),
        checkbox(
            "enabled",
            "Activer ce flux dans la configuration",
            spec.is_some_and(|s| s.enabled),
        ),
        field(
            "poll_seconds",
            "Intervalle minimal entre contrôles, en secondes",
            spec.map(|s| s.poll_seconds).unwrap_or(3600),
            "number",
            true,
        ),
        hidden(
            "max_bytes",
            spec.map(|s| s.max_response_bytes).unwrap_or(2097152),
        ),
    ]);

    if version.is_some() {
        for entry in &mut fields {
            if entry.name == "url" {
                entry.readonly = true;
            }
        }
    }
    FormSpec {
        title: title.into(),
        action,
        button: "Enregistrer le flux".into(),
        fields,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: Option<i64>,
    name: String,
    url: String,
    enabled: Option<String>,
    rights_status: RightsStatus,
    storage: StoragePolicy,
    display: DisplayPolicy,
    terms_url: String,
    rights_note: String,
    languages: String,
    geography: String,
    attribution: String,
    ownership_note: String,
    editorial_note: String,
}
fn optional_text(value: String) -> Option<String> {
    let value = value.trim().to_owned();

    (!value.is_empty()).then_some(value)
}
fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
impl SourceInput {
    fn spec(self, origins: Vec<String>) -> SourceSpec {
        SourceSpec {
            name: self.name,
            homepage_url: self.url,
            enabled: self.enabled.as_deref() == Some("yes"),
            ownership_note: optional_text(self.ownership_note),
            usage_policy: self.storage,
            terms_url: optional_text(self.terms_url),
            languages: lines(&self.languages),
            geographic_coverage: lines(&self.geography),
            attribution: optional_text(self.attribution),
            editorial_note: optional_text(self.editorial_note),
            rights_status: self.rights_status,
            display_policy: self.display,
            rights_note: optional_text(self.rights_note),
            origin_ids: origins,
        }
    }
}
async fn create_source(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    input: Result<Form<SourceInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        if input.version.is_some() {
            return Err(AppError::Validation("New source has no version."));
        }
        let id = random_id("source")?;

        Registry::new(pool(&state)?)
            .save_source(&session, &id, None, input.spec(vec![]))
            .await?;

        Ok(route("sources", &id))
    }
    .await;

    redirect(&state, result)
}
async fn update_source(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<SourceInput>, FormRejection>,
) -> Response {
    let result=async{let input=form(input)?;

let expected=input.version.ok_or(AppError::Validation("Missing source version."))?;

let registry=Registry::new(pool(&state)?);

let before=registry.source(&session,&id).await?;

if before.version!=expected{return Err(AppError::Conflict);
}let spec=input.spec(before.spec.origin_ids.clone());
spec.validate()?;

if before.spec.effective_storage()!=StoragePolicy::MetadataOnly&&spec.effective_storage()==StoragePolicy::MetadataOnly{let mut v=page(&state,&session,"Confirmer le retrait du texte conservé");
v.paragraphs.push(format!("Tous les textes extraits conservés pour « {} » seront retirés. Les références, l’historique et les brouillons originaux seront conservés.",before.spec.name));

let mut check=checkbox("confirmed","Je confirme le retrait de tous les textes extraits conservés pour cette source",false);
check.required=true;
v.forms.push(FormSpec{title:"Retirer le texte et appliquer la nouvelle politique".into(),action:action("sources",&id,"confirm"),button:"Retirer le texte et enregistrer".into(),fields:vec![hidden("version",expected),hidden("payload",serde_json::to_string(&spec).map_err(|_|AppError::InvalidData)?),check]});
return Ok(Some(v));
}registry.save_source(&session,&id,Some(expected),spec).await?;
Ok(None)}.await;

    match result {
        Ok(Some(v)) => render(&state, Ok(v)),
        Ok(None) => Redirect::to(&route("sources", &id)).into_response(),
        Err(e) => error(&state, e),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: i64,
    payload: String,
    confirmed: Option<String>,
}
async fn confirm_source(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<Confirmation>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        if input.confirmed.as_deref() != Some("yes") {
            return Err(AppError::Validation("Text removal requires confirmation."));
        }
        let spec: SourceSpec = serde_json::from_str(&input.payload)
            .map_err(|_| AppError::Validation("Invalid source specification."))?;

        Registry::new(pool(&state)?)
            .save_source(&session, &id, Some(input.version), spec)
            .await?;

        Ok(route("sources", &id))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: i64,
}
async fn disable_source(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<VersionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        Registry::new(pool(&state)?)
            .disable(
                &session,
                crate::registry::Entity::Source,
                &id,
                input.version,
            )
            .await?;

        Ok(route("sources", &id))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: Option<i64>,
    source_id: String,
    url: String,
    enabled: Option<String>,
    poll_seconds: i32,
    max_bytes: i32,
}
impl FeedInput {
    fn spec(self) -> FeedSpec {
        FeedSpec {
            source_id: self.source_id,
            url: self.url,
            enabled: self.enabled.as_deref() == Some("yes"),
            poll_seconds: self.poll_seconds,
            max_response_bytes: self.max_bytes,
        }
    }
}
async fn create_feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    input: Result<Form<FeedInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        if input.version.is_some() {
            return Err(AppError::Validation("New feed has no version."));
        }
        let id = random_id("feed")?;

        Registry::new(pool(&state)?)
            .save_feed(&session, &id, None, input.spec())
            .await?;

        Ok(route("feeds", &id))
    }
    .await;

    redirect(&state, result)
}
async fn update_feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<FeedInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        let version = input
            .version
            .ok_or(AppError::Validation("Missing feed version."))?;

        Registry::new(pool(&state)?)
            .save_feed(&session, &id, Some(version), input.spec())
            .await?;

        Ok(route("feeds", &id))
    }
    .await;

    redirect(&state, result)
}
async fn disable_feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<VersionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        Registry::new(pool(&state)?)
            .disable(&session, crate::registry::Entity::Feed, &id, input.version)
            .await?;

        Ok(route("feeds", &id))
    }
    .await;

    redirect(&state, result)
}
async fn pause_feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        FeedAdmin::new(pool(&state)?)
            .pause(&session, &id, input.version, &input.reason)
            .await?;

        Ok(route("feeds", &id))
    }
    .await;

    redirect(&state, result)
}
async fn resume_feed(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        FeedAdmin::new(pool(&state)?)
            .resume(&session, &id, input.version, &input.reason)
            .await?;

        Ok(route("feeds", &id))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    source_id: String,
    url: String,
    title: String,
    language: String,
    notes: String,
}
async fn register_link(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    input: Result<Form<LinkInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        let r = IntakeService::new(pool(&state)?, &state.config.public_url)
            .map_err(|_| AppError::Unavailable)?
            .register_link(
                &session,
                ManualLink {
                    source_id: input.source_id,
                    url: input.url,
                    title: input.title,
                    language: input.language,
                    notes: input.notes,
                },
            )
            .await?;

        Ok(route("articles", &r.id))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NotesInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: i64,
    notes: String,
}
async fn notes(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<NotesInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        IntakeService::new(pool(&state)?, &state.config.public_url)
            .map_err(|_| AppError::Unavailable)?
            .manual(&session, &id, input.version, &input.notes, None)
            .await?;

        Ok(route("articles", &id))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    version: i64,
    notes: String,
    title: String,
    body: String,
    language: String,
    format: ArticleFormat,
}
async fn draft(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<DraftInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        if input.body.trim().is_empty() {
            return Err(AppError::Validation("Original draft requires text."));
        }
        let story = random_id("story")?;

        let original = OriginalDraft {
            id: story.clone(),
            input: RevisionInput {
                title: input.title,
                standfirst: None,
                summary: None,
                body: input
                    .body
                    .replace("\r\n", "\n")
                    .split("\n\n")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|text| ContentBlock::Paragraph { text: text.into() })
                    .collect(),
                language: input.language,
                format: input.format,
                kind: ArticleKind::Reporting,
                topics: vec![],
                geography: vec![],
                reason: "Création d’un brouillon original depuis les notes de rédaction.".into(),
            },
        };

        IntakeService::new(pool(&state)?, &state.config.public_url)
            .map_err(|_| AppError::Unavailable)?
            .manual(&session, &id, input.version, &input.notes, Some(original))
            .await?;

        Ok(route("drafts", &story))
    }
    .await;

    redirect(&state, result)
}
async fn request_extraction(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        let r = ExtractionAdmin::new(pool(&state)?)
            .request(&session, &id, input.version, &input.reason)
            .await?;

        Ok(route("extractions", &r.id.to_string()))
    }
    .await;

    redirect(&state, result)
}
async fn replay_job(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        JobAdmin::new(pool(&state)?)
            .replay(&session, id, input.version, &input.reason)
            .await?;

        Ok(route("jobs", &id.to_string()))
    }
    .await;

    redirect(&state, result)
}
async fn cancel_job(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        JobAdmin::new(pool(&state)?)
            .cancel(&session, id, input.version, &input.reason)
            .await?;

        Ok(route("jobs", &id.to_string()))
    }
    .await;

    redirect(&state, result)
}
async fn cancel_extraction(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
    input: Result<Form<ActionInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        ExtractionAdmin::new(pool(&state)?)
            .cancel(&session, id, input.version, &input.reason)
            .await?;

        Ok(route("extractions", &id.to_string()))
    }
    .await;

    redirect(&state, result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReasonInput {
    #[serde(rename = "csrf")]
    _csrf: String,
    reason: String,
}
async fn enqueue_batch(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
    input: Result<Form<ReasonInput>, FormRejection>,
) -> Response {
    let result = async {
        let input = form(input)?;

        let r = JobAdmin::new(pool(&state)?)
            .enqueue(&session, id, 3, &input.reason)
            .await?;

        Ok(route("jobs", &r.id.to_string()))
    }
    .await;

    redirect(&state, result)
}
