//! Complete manual editorial transitions. Preflight observations never authorise a write.
use super::*;
use crate::{
    auth::Permission,
    editorial::{PublicState, ReviewOutcome, ReviewState},
};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Submit,
    Review,
    Publish,
    Withdraw,
}
impl Op {
    fn suffix(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Review => "review",
            Self::Publish => "publish",
            Self::Withdraw => "withdraw",
        }
    }
    fn permission(self) -> Permission {
        if self == Self::Submit {
            Permission::Edit
        } else {
            Permission::Publish
        }
    }
}
#[derive(Default)]
struct Input {
    version: i64,
    revision: String,
    outcome: String,
    internal_note: String,
    public_reason: String,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Cursor {
    revisions_before: Option<i32>,
    reviews_before: Option<i64>,
    publications_before: Option<i64>,
}
fn path(id: &str) -> String {
    format!("/admin/editor{}/workflow", path_segment("story", id))
}
fn endpoint(id: &str, op: Op) -> String {
    format!("/admin/editor{}/{}", path_segment("story", id), op.suffix())
}
fn service(state: &AppState) -> Result<EditorialService, AppError> {
    Ok(EditorialService::with_policy(
        pool(state)?,
        Arc::new(state.config.editorial.clone()),
    ))
}
fn public_state(state: PublicState) -> &'static str {
    match state {
        PublicState::Draft => "Privé",
        PublicState::InReview => "Privé, en relecture",
        PublicState::Published => "Publication accessible",
        PublicState::Withdrawn => "Retiré",
    }
}
fn review_state(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Draft => "À préparer",
        ReviewState::InReview => "En relecture",
        ReviewState::Approved => "Approuvée",
        ReviewState::ChangesRequested => "Modifications demandées",
    }
}
fn form_for(id: &str, op: Op, input: &Input) -> FormSpec {
    let mut fields = vec![
        hidden("version", input.version),
        hidden("revision", &input.revision),
    ];
    let (title, button) = match op {
        Op::Submit => (
            "Envoyer la version préparée en relecture",
            "Demander une relecture",
        ),
        Op::Review => {
            fields.push(select(
                "outcome",
                "Décision de relecture",
                &input.outcome,
                &[
                    ("approved", "Approuver la version"),
                    ("changes_requested", "Demander des modifications"),
                ],
            ));
            fields.push(field(
                "internal_note",
                "Note de relecture privée, obligatoire pour demander des modifications",
                &input.internal_note,
                "textarea",
                false,
            ));
            (
                "Décision éditoriale privée",
                "Enregistrer la décision de relecture",
            )
        }
        Op::Publish => {
            fields.push(field(
                "public_reason",
                "Motif public de publication ou de correction",
                &input.public_reason,
                "textarea",
                true,
            ));
            (
                "Publier uniquement la version approuvée préparée",
                "Publier cette version approuvée",
            )
        }
        Op::Withdraw => {
            fields.push(field(
                "public_reason",
                "Motif public du retrait",
                &input.public_reason,
                "textarea",
                true,
            ));
            (
                "Retirer la publication avec un avis public",
                "Retirer l’article avec ce motif public",
            )
        }
    };
    FormSpec {
        title: title.into(),
        action: endpoint(id, op),
        button: button.into(),
        fields,
    }
}
async fn view(
    state: &AppState,
    session: &Session,
    id: &str,
    cursor: &Cursor,
    own: Option<(Op, &Input)>,
) -> Result<View, AppError> {
    let data = service(state)?
        .workflow(
            session,
            id,
            cursor.revisions_before,
            cursor.reviews_before,
            cursor.publications_before,
        )
        .await?;
    let head = &data.head;
    let revision = &data.revision;
    let mut v = page(
        state,
        session,
        format!("Relecture et publication : {}", revision.title),
    );
    v.paragraphs.push(format!("Version privée préparée : {} · Version de contrôle lue : {}. État de publication : {}. Relecture de cette version : {}.",revision.id,head.lock_version,public_state(head.public_state),review_state(head.review_state)));
    if let Some(published) = &head.published_revision_id {
        v.paragraphs.push(format!("Révision conservée par la publication : {published}. L’aperçu ci-dessous est la candidate privée ; le retrait concerne la publication, pas seulement cette candidate."));
    }
    v.paragraphs.push("Les contrôles ci-dessous sont des observations. Chaque commande vérifie à nouveau la session, la version et ses règles. Une approbation ne suffit pas si les sources ou la politique de longueur ont changé. Les notes privées ne deviennent jamais des motifs publics.".into());
    v.paragraphs.push("Publier un article ne compose pas une édition. Une correction est une nouvelle version à préparer, sourcer, relire et publier ; l’ancienne publication reste accessible tant que la correction n’est pas publiée.".into());
    v.links.push(Link {
        label: "Brouillons et file de relecture".into(),
        href: "/admin/editor?filter=in_review".into(),
    });
    if head.public_state != PublicState::Withdrawn {
        v.links.push(Link {
            label: "Modifier la version privée (invalide sa relecture)".into(),
            href: format!("/admin/editor{}", path_segment("story", id)),
        });
        v.links.push(Link {
            label: "Attribuer les sources et justificatifs".into(),
            href: format!("/admin/editor{}/sources", path_segment("story", id)),
        });
        v.links.push(Link {
            label: "Classement et évaluation de longueur".into(),
            href: route("drafts", id),
        });
    }
    if head.public_state == PublicState::Published {
        v.links.push(Link {
            label: "Lire la publication actuelle comme un lecteur".into(),
            href: path_segment("story", id),
        });
    }
    let projection = state
        .config
        .taxonomy
        .project(&revision.topics, &revision.geography);
    let classification = crate::taxonomy::Classification {
        topics: projection.topics.clone(),
        geography: projection.geography.clone(),
    };
    v.tables.push(table(
        "Contrôles observés avant publication",
        &["Contrôle", "Observation"],
        vec![
            vec![
                cell("Sources et affirmations"),
                cell(
                    data.source_blocker
                        .as_deref()
                        .unwrap_or("Règles de sources satisfaites à la lecture"),
                ),
            ],
            vec![
                cell("Longueur et règle conservée"),
                cell(
                    data.length_blocker
                        .as_deref()
                        .unwrap_or("Règles de longueur satisfaites à la lecture"),
                ),
            ],
            vec![
                cell("Rubrique proposée"),
                cell(state.config.taxonomy.section(&classification).1),
            ],
            vec![cell("Langue originale"), cell(&revision.language)],
        ],
    ));
    if revision.topics.is_empty() || revision.geography.is_empty() {
        v.paragraphs.push("Classement à examiner : des étiquettes de sujet ou de géographie ne sont pas renseignées. La langue ne permet pas de les déduire.".into());
    }
    if projection.unmapped_topics || projection.unmapped_geography {
        v.paragraphs.push("Classement à examiner : certaines anciennes étiquettes ne sont pas reconnues. Confirmez le classement depuis le formulaire dédié ; cela crée une nouvelle version à relire.".into());
    }
    v.paragraphs.extend(data.evidence.warnings.iter().cloned());
    v.paragraphs.extend(data.length.warnings.iter().cloned());
    let ready = data.source_blocker.is_none() && data.length_blocker.is_none();
    let defaults = Input {
        version: head.lock_version,
        revision: revision.id.clone(),
        outcome: "approved".into(),
        ..Default::default()
    };
    if head.public_state != PublicState::Withdrawn {
        if matches!(
            head.review_state,
            ReviewState::Draft | ReviewState::ChangesRequested
        ) && head.published_revision_id.as_deref() != Some(&revision.id)
        {
            v.forms.push(form_for(id, Op::Submit, &defaults));
        }
        if publisher(session) && head.review_state == ReviewState::InReview {
            v.forms.push(form_for(id, Op::Review, &defaults));
        }
        if publisher(session)
            && head.review_state == ReviewState::Approved
            && head.published_revision_id.as_deref() != Some(&revision.id)
            && ready
        {
            v.forms.push(form_for(id, Op::Publish, &defaults));
        }
        if publisher(session) && head.public_state == PublicState::Published {
            v.forms.push(form_for(id, Op::Withdraw, &defaults));
        }
    }
    if let Some((op, input)) = own {
        let form = form_for(id, op, input);
        v.forms.retain(|old| old.action != form.action);
        v.forms.insert(0, form);
    }
    v.tables.push(table(
        "Historique privé des versions",
        &["Version", "Compte", "Créée le", "Titre", "Motif privé"],
        data.revisions
            .iter()
            .map(|r| {
                vec![
                    linked(
                        r.number,
                        format!(
                            "/admin/editor{}/history{}",
                            path_segment("story", id),
                            path_segment("revision", &r.id)
                        ),
                    ),
                    cell(&r.actor_id),
                    cell(stamp(state, r.created_at)),
                    cell(&r.title),
                    cell(r.change_reason.as_deref().unwrap_or("Non renseigné")),
                ]
            })
            .collect(),
    ));
    if data.revisions.len() == 20
        && let Some(r) = data.revisions.last()
    {
        v.links.push(Link {
            label: "Versions plus anciennes".into(),
            href: with_query(&path(id), &[("revisions_before", &r.number.to_string())]),
        });
    }
    v.tables.push(table(
        "Relectures privées",
        &[
            "Révision",
            "Compte",
            "Décision",
            "Note privée",
            "Enregistrée le",
        ],
        data.reviews
            .iter()
            .map(|r| {
                vec![
                    cell(&r.revision_id),
                    cell(&r.actor_id),
                    cell(if r.outcome == "approved" {
                        "Approuvée"
                    } else {
                        "Modifications demandées"
                    }),
                    cell(r.internal_note.as_deref().unwrap_or("Aucune note")),
                    cell(stamp(state, r.created_at)),
                ]
            })
            .collect(),
    ));
    if data.reviews.len() == 20
        && let Some(r) = data.reviews.last()
    {
        v.links.push(Link {
            label: "Relectures plus anciennes".into(),
            href: with_query(&path(id), &[("reviews_before", &r.id.to_string())]),
        });
    }
    v.tables.push(table(
        "Journal des publications et retraits",
        &[
            "Révision",
            "Action",
            "Compte",
            "Motif public",
            "Enregistrée le",
        ],
        data.publications
            .iter()
            .map(|r| {
                vec![
                    cell(&r.revision_id),
                    cell(match r.action.as_str() {
                        "published" => "Publication",
                        "corrected" => "Correction",
                        "withdrawn" => "Retrait",
                        "imported" => "Archive importée",
                        _ => "Action inconnue",
                    }),
                    cell(&r.actor_id),
                    cell(r.public_reason.as_deref().unwrap_or("Non renseigné")),
                    cell(stamp(state, r.created_at)),
                ]
            })
            .collect(),
    ));
    if data.publications.len() == 20
        && let Some(r) = data.publications.last()
    {
        v.links.push(Link {
            label: "Publications plus anciennes".into(),
            href: with_query(&path(id), &[("publications_before", &r.id.to_string())]),
        });
    }
    v.article = Some(data.revision);
    v.evidence = Some(data.evidence);
    v.length = Some(data.length);
    Ok(v)
}
async fn show(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    params: Result<Query<Cursor>, QueryRejection>,
) -> Response {
    let result = async {
        let cursor = query(params)?;
        view(&state, &session, &id, &cursor, None).await
    }
    .await;
    render(&state, result)
}
async fn history(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path((id, revision)): Path<(String, String)>,
) -> Response {
    let result=async{
        let service=service(&state)?;let stored=service.revision(&session,&revision).await?;if stored.story_id!=id{return Err(AppError::NotFound);}
        let evidence=crate::citations::Citations::new(pool(&state)?).private(&session,&stored).await?;
        let length=service.length_report(&session,&revision).await?;
        let mut v=page(&state,&session,format!("Historique privé : {}",stored.title));v.paragraphs.push(format!("Révision conservée : {}. Cette page est en lecture seule ; les contrôles de longueur décrivent les règles actuelles, sans réécrire le contenu historique.",stored.id));
        v.links.push(Link{label:"Retour à la version préparée et à son journal".into(),href:path(&id)});v.article=Some(stored);v.evidence=Some(evidence);v.length=Some(length);Ok(v)
    }.await;
    render(&state, result)
}
fn take(m: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    m.remove(key)
        .ok_or(AppError::Validation("Formulaire éditorial incomplet."))
}
fn parse(raw: Result<RawForm, RawFormRejection>, op: Op) -> Result<Input, AppError> {
    let RawForm(bytes) = raw.map_err(|_| AppError::Validation("Formulaire éditorial invalide."))?;
    if !super::super::editor::strict_encoding(&bytes) {
        return Err(AppError::Validation("Encodage éditorial invalide."));
    }
    let mut m = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(&bytes) {
        if m.len() >= 8 || k.len() > 64 || m.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation(
                "Champs éditoriaux répétés ou trop nombreux.",
            ));
        }
    }
    take(&mut m, "csrf")?;
    let mut f = Input {
        version: take(&mut m, "version")?
            .parse::<i64>()
            .ok()
            .filter(|v| *v >= 0)
            .ok_or(AppError::Validation("Version éditoriale invalide."))?,
        revision: take(&mut m, "revision")?,
        ..Default::default()
    };
    match op {
        Op::Review => {
            f.outcome = take(&mut m, "outcome")?;
            f.internal_note = take(&mut m, "internal_note")?;
        }
        Op::Publish | Op::Withdraw => f.public_reason = take(&mut m, "public_reason")?,
        Op::Submit => {}
    }
    if !m.is_empty() {
        return Err(AppError::Validation("Champ éditorial inconnu."));
    }
    Ok(f)
}
async fn command(
    state: AppState,
    session: Session,
    id: String,
    op: Op,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let input = match parse(raw, op) {
        Ok(input) => input,
        Err(e) => return error(&state, e),
    };
    let result = async {
        let service = service(&state)?;
        let head = service.head(&session, &id).await?;
        if head.current_revision_id != input.revision || head.lock_version != input.version {
            return Err(AppError::Conflict);
        }
        match op {
            Op::Submit => {
                service.submit(&session, &id, input.version).await?;
            }
            Op::Review => {
                let outcome = match input.outcome.as_str() {
                    "approved" => ReviewOutcome::Approved,
                    "changes_requested" => ReviewOutcome::ChangesRequested,
                    _ => return Err(AppError::Validation("Décision de relecture inconnue.")),
                };
                service
                    .review(
                        &session,
                        &id,
                        input.version,
                        outcome,
                        if input.internal_note.trim().is_empty() {
                            None
                        } else {
                            Some(&input.internal_note)
                        },
                    )
                    .await?;
            }
            Op::Publish => {
                service
                    .publish(&session, &id, input.version, &input.public_reason)
                    .await?;
            }
            Op::Withdraw => {
                service
                    .withdraw(&session, &id, input.version, &input.public_reason)
                    .await?;
            }
        }
        Ok(())
    }
    .await;
    let (status,message)=match result {Ok(())=>return Redirect::to(&path(&id)).into_response(),Err(AppError::Conflict)=>(StatusCode::CONFLICT,"L’état éditorial ou un travail concurrent empêche cette action. Votre saisie est conservée ; comparez-la avec la version préparée actuelle avant de la reporter.".into()),Err(AppError::Validation(text))=>(StatusCode::BAD_REQUEST,format!("Action non enregistrée, saisie conservée. {text}")),Err(e)=>return error(&state,e)};
    let Some(auth) = &state.auth else {
        return error(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, op.permission()).await {
        return super::super::failure(&state, e);
    }
    match view(
        &state,
        &session,
        &id,
        &Cursor::default(),
        Some((op, &input)),
    )
    .await
    {
        Ok(mut v) => {
            v.paragraphs.insert(0, message);
            match v.render() {
                Ok(html) => (status, Html(html)).into_response(),
                Err(_) => error(&state, AppError::Unavailable),
            }
        }
        Err(e) => error(&state, e),
    }
}
macro_rules! handler {
    ($name:ident,$op:ident) => {
        async fn $name(
            State(state): State<AppState>,
            Extension(session): Extension<Session>,
            Path(id): Path<String>,
            raw: Result<RawForm, RawFormRejection>,
        ) -> Response {
            command(state, session, id, Op::$op, raw).await
        }
    };
}
handler!(submit, Submit);
handler!(review, Review);
handler!(publish, Publish);
handler!(withdraw, Withdraw);
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/editor/story/{id}/workflow", get(show))
        .route(
            "/admin/editor/story/{id}/history/revision/{revision}",
            get(history),
        )
        .route("/admin/editor/story/{id}/submit", post(submit))
        .route("/admin/editor/story/{id}/review", post(review))
        .route("/admin/editor/story/{id}/publish", post(publish))
        .route("/admin/editor/story/{id}/withdraw", post(withdraw))
}
