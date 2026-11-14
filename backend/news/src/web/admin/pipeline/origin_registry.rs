//! Registry identities are configuration, never article-specific evidence.
use super::*;
use crate::{
    auth::Permission,
    registry::{Origin, OriginSpec},
};
use std::collections::BTreeMap;
const ROOT: &str = "/admin/editor/origins";
#[derive(Clone, Default)]
struct Fields {
    id: String,
    version: Option<i64>,
    name: String,
    source: String,
    note: String,
    enabled: bool,
}
fn path(id: &str) -> String {
    format!("/admin/editor{}", path_segment("origins", id))
}
fn fields(record: &Origin) -> Fields {
    Fields {
        id: record.id.clone(),
        version: Some(record.version),
        name: record.spec.name.clone(),
        source: record.spec.source_id.clone().unwrap_or_default(),
        note: record.spec.note.clone().unwrap_or_default(),
        enabled: record.spec.enabled,
    }
}
fn optional(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.into())
    }
}
fn form_fields(f: &Fields) -> Vec<Field> {
    let mut id = field("id", "Identifiant stable de l’origine", &f.id, "text", true);
    id.readonly = f.version.is_some();
    let mut name = field("name", "Nom de l’origine", &f.name, "text", true);
    name.readonly = f.version.is_some();
    let mut source = field(
        "source",
        "Source associée (identifiant facultatif)",
        &f.source,
        "text",
        false,
    );
    source.readonly = f.version.is_some();
    vec![
        id,
        hidden(
            "version",
            f.version.map(|n| n.to_string()).unwrap_or_default(),
        ),
        name,
        source,
        field(
            "note",
            "Note privée du registre",
            &f.note,
            "textarea",
            false,
        ),
        checkbox(
            "enabled",
            "Disponible pour de nouvelles assertions",
            f.enabled,
        ),
    ]
}
fn add_form(v: &mut View, f: &Fields) {
    v.forms.push(FormSpec {
        title: if f.version.is_some() {
            "Modifier la disponibilité ou la note"
        } else {
            "Créer une identité d’origine"
        }
        .into(),
        action: if f.version.is_some() {
            path(&f.id)
        } else {
            ROOT.into()
        },
        button: if f.version.is_some() {
            "Enregistrer la configuration"
        } else {
            "Créer l’origine"
        }
        .into(),
        fields: form_fields(f),
    });
}
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(ROOT, get(list).post(create))
        .route("/admin/editor/origins/{id}", get(show).post(save))
}
async fn list_view(
    state: &AppState,
    session: &Session,
    after: Option<&str>,
    draft: Option<Fields>,
) -> Result<View, AppError> {
    let rows = Registry::new(pool(state)?)
        .origins(session, after, size())
        .await?;
    let mut v = page(state, session, "Registre des origines de reportage");
    v.paragraphs.push("Une identité de registre ne prouve pas l’origine d’un document. Les assertions article par article restent distinctes, avec leur propre justification humaine.".into());
    v.links.push(Link {
        label: "Consulter les sources configurées et leurs identifiants".into(),
        href: "/admin/sources".into(),
    });
    v.tables.push(table(
        "Identités configurées",
        &["Identifiant", "Nom", "Source associée", "Disponibilité"],
        rows.iter()
            .map(|r| {
                vec![
                    linked(&r.id, path(&r.id)),
                    cell(&r.spec.name),
                    r.spec
                        .source_id
                        .as_deref()
                        .map(|id| linked(id, route("sources", id)))
                        .unwrap_or_else(|| cell("Aucune")),
                    cell(if r.spec.enabled {
                        "Disponible"
                    } else {
                        "Désactivée"
                    }),
                ]
            })
            .collect(),
    ));
    if rows.len() == 20 {
        v.next = rows.last().map(|r| with_query(ROOT, &[("after", &r.id)]));
    }
    if publisher(session) {
        add_form(&mut v, &draft.unwrap_or_default());
    }
    Ok(v)
}
async fn list(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    params: Result<Query<Cursor>, QueryRejection>,
) -> Response {
    let result = async {
        let q = query(params)?;
        list_view(&state, &session, q.after.as_deref(), None).await
    }
    .await;
    render(&state, result)
}
async fn detail_view(
    state: &AppState,
    session: &Session,
    id: &str,
    before: Option<i64>,
    draft: Option<Fields>,
) -> Result<View, AppError> {
    let registry = Registry::new(pool(state)?);
    let record = registry.origin(session, id).await?;
    let rows = registry.origin_history(session, id, before, size()).await?;
    let mut v = page(state, session, format!("Origine : {}", record.spec.name));
    v.links.push(Link {
        label: "Toutes les identités d’origine".into(),
        href: ROOT.into(),
    });
    v.paragraphs.push("L’identité et son association à une source sont conservées. Pour changer d’identité, créez une nouvelle entrée ; ne réécrivez pas une origine historique.".into());
    v.paragraphs.push("Désactiver empêche les nouvelles assertions de sélectionner cette origine et rend les preuves concernées à réexaminer. Les versions et attributions déjà capturées restent conservées.".into());
    v.tables.push(table(
        "Configuration actuelle",
        &["Champ", "Valeur"],
        vec![
            vec![cell("Identifiant stable"), cell(&record.id)],
            vec![cell("Version"), cell(record.version)],
            vec![
                cell("Source associée"),
                cell(record.spec.source_id.as_deref().unwrap_or("Aucune")),
            ],
            vec![
                cell("Disponibilité"),
                cell(if record.spec.enabled {
                    "Disponible"
                } else {
                    "Désactivée"
                }),
            ],
            vec![
                cell("Note privée"),
                cell(record.spec.note.as_deref().unwrap_or("Aucune")),
            ],
        ],
    ));
    if rows.len() == 20 {
        v.next = rows
            .last()
            .map(|r| with_query(&path(id), &[("before", &r.id.to_string())]));
    }
    v.tables.push(table(
        "Historique du registre",
        &[
            "Version",
            "Compte",
            "Enregistré le",
            "Disponibilité après",
            "Note privée après",
        ],
        rows.into_iter()
            .map(|r| {
                vec![
                    cell(r.version),
                    cell(r.actor_id),
                    cell(stamp(state, r.created_at)),
                    cell(if r.enabled {
                        "Disponible"
                    } else {
                        "Désactivée"
                    }),
                    cell(r.note.unwrap_or_default()),
                ]
            })
            .collect(),
    ));
    v.paragraphs.push("Les entrées historiques peuvent ne pas avoir de journal antérieur à leur import. Ce journal de configuration ne constitue pas une preuve d’origine d’article.".into());
    if publisher(session) {
        add_form(&mut v, &draft.unwrap_or_else(|| fields(&record)));
    }
    Ok(v)
}
async fn show(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    params: Result<Query<Before>, QueryRejection>,
) -> Response {
    let result = async {
        let q = query(params)?;
        detail_view(&state, &session, &id, q.before, None).await
    }
    .await;
    render(&state, result)
}
fn take(m: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    m.remove(key)
        .ok_or(AppError::Validation("Formulaire de registre incomplet."))
}
fn parse(bytes: &[u8]) -> Result<Fields, AppError> {
    if !super::super::editor::strict_encoding(bytes) {
        return Err(AppError::Validation("Encodage de registre invalide."));
    }
    let mut m = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(bytes) {
        if m.len() >= 8 || k.len() > 64 || m.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation(
                "Champs de registre répétés ou trop nombreux.",
            ));
        }
    }
    take(&mut m, "csrf")?;
    let version = take(&mut m, "version")?;
    let enabled = match m.remove("enabled") {
        None => false,
        Some(v) if v == "yes" => true,
        _ => return Err(AppError::Validation("Disponibilité invalide.")),
    };
    let f = Fields {
        id: take(&mut m, "id")?,
        version: if version.is_empty() {
            None
        } else {
            Some(
                version
                    .parse()
                    .map_err(|_| AppError::Validation("Version de registre invalide."))?,
            )
        },
        name: take(&mut m, "name")?,
        source: take(&mut m, "source")?,
        note: take(&mut m, "note")?,
        enabled,
    };
    if !m.is_empty() {
        return Err(AppError::Validation("Champ de registre inconnu."));
    }
    Ok(f)
}
async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    submit(state, session, None, raw).await
}
async fn save(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    submit(state, session, Some(id), raw).await
}
async fn submit(
    state: AppState,
    session: Session,
    id: Option<String>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let f = match raw
        .map_err(|_| AppError::Validation("Formulaire de registre invalide."))
        .and_then(|RawForm(b)| parse(&b))
    {
        Ok(f) => f,
        Err(e) => return error(&state, e),
    };
    let result = async {
        if id.as_ref().is_some_and(|id| *id != f.id) || id.is_some() != f.version.is_some() {
            return Err(AppError::Validation(
                "Identité ou version de registre incohérente.",
            ));
        }
        Registry::new(pool(&state)?)
            .save_origin(
                &session,
                &f.id,
                f.version,
                OriginSpec {
                    name: f.name.clone(),
                    source_id: optional(&f.source),
                    note: optional(&f.note),
                    enabled: f.enabled,
                },
            )
            .await?;
        Ok(path(&f.id))
    }
    .await;
    let (status, message) = match result {
        Ok(url) => return Redirect::to(&url).into_response(),
        Err(AppError::Conflict) => (
            StatusCode::CONFLICT,
            "La configuration a changé, l’identité ne peut pas être réécrite ou un travail concurrent empêche l’enregistrement. Votre saisie est conservée pour comparaison.",
        ),
        Err(AppError::Validation(_)) => (
            StatusCode::BAD_REQUEST,
            "Configuration non enregistrée. Vérifiez l’identité, les limites des champs et la source associée ; votre saisie est conservée.",
        ),
        Err(e) => return error(&state, e),
    };
    let Some(auth) = &state.auth else {
        return error(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, Permission::Publish).await {
        return super::super::failure(&state, e);
    }
    let result = match id {
        Some(id) => detail_view(&state, &session, &id, None, Some(f)).await,
        None => list_view(&state, &session, None, Some(f)).await,
    };
    match result {
        Ok(mut v) => {
            v.paragraphs.insert(0, message.into());
            let mut response = render(&state, Ok(v));
            *response.status_mut() = status;
            response
        }
        Err(e) => error(&state, e),
    }
}
