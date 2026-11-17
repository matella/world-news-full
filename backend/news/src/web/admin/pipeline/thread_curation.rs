//! Explicit human graph curation; article prose and publication pointers stay separate.
use super::*;
use crate::{
    auth::Permission,
    threads::{MemberInput, Merge, Snapshot, Split, State as ThreadState, StoryChoice, Threads},
};
use std::collections::{BTreeMap, BTreeSet};
const ROOT: &str = "/admin/editor/threads";
#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Create,
    Edit,
    Member,
    Remove,
    Split,
    Prepare,
    Merge,
}
impl Op {
    fn suffix(self) -> &'static str {
        match self {
            Self::Create => "",
            Self::Edit => "edit",
            Self::Member => "member",
            Self::Remove => "remove",
            Self::Split => "split",
            Self::Prepare => "merge-preview",
            Self::Merge => "merge",
        }
    }
}
#[derive(Clone, Default)]
struct Input {
    id: String,
    version: i64,
    title: String,
    state: String,
    story: String,
    lookup: String,
    when: String,
    unknown: bool,
    reason: String,
    target: String,
    target_version: i64,
    new_id: String,
    members: Vec<String>,
    after: String,
    intent: String,
}
#[derive(Default)]
struct UndoInput {
    fences: Vec<(String, i64)>,
    reason: String,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct HistoryCursor {
    before: Option<i64>,
}
fn path(id: &str) -> String {
    format!("/admin/editor{}", path_segment("threads", id))
}
fn endpoint(id: &str, op: Op) -> String {
    format!("{}/{}", path(id), op.suffix())
}
fn action_path(id: i64) -> String {
    format!("/admin/editor/thread-actions/{id}")
}
fn service(state: &AppState) -> Result<Threads, AppError> {
    Ok(Threads::new(pool(state)?))
}
fn why(value: &str) -> Field {
    field(
        "reason",
        "Motif privé du changement (une ligne)",
        value,
        "text",
        true,
    )
}
fn pager(label: &str, value: impl ToString) -> Field {
    field("intent", label, value, "button", false)
}
fn state_label(state: ThreadState) -> &'static str {
    match state {
        ThreadState::Active => "Actif",
        ThreadState::Closed => "Clos",
    }
}
fn action_label(action: &str) -> &str {
    match action {
        "created" => "Création",
        "edited" => "Titre ou état",
        "member" => "Rattachement ou date",
        "removed" => "Retrait d’un article",
        "merged" => "Fusion",
        "split" => "Séparation",
        "undone" => "Annulation auditée",
        other => other,
    }
}
fn story_label(rows: &[StoryChoice], id: &str) -> String {
    match rows.iter().find(|r| r.id == id) {
        Some(r) => format!(
            "{} · {}",
            r.title.as_deref().unwrap_or("Révision courante absente"),
            id
        ),
        None => format!("Article indisponible : {id}"),
    }
}
fn assignment_label(rows: &[StoryChoice], id: &str) -> String {
    let label = story_label(rows, id);
    match rows
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.thread_id.as_deref())
    {
        Some(owner) => format!("{label} · Suivi actuel : {owner}"),
        None => label,
    }
}
fn picker(name: &str, label: &str, value: &str, items: Vec<(String, String)>) -> Field {
    let mut f = field(name, label, "", "select", true);
    f.choices = std::iter::once((String::new(), "Choisir explicitement".into()))
        .chain(items)
        .map(|(key, label)| Choice {
            selected: key == value,
            value: key,
            label,
        })
        .collect();
    f
}
fn create_form(f: &Input) -> FormSpec {
    FormSpec {
        title: "Créer un sujet stable".into(),
        action: ROOT.into(),
        button: "Créer le suivi".into(),
        fields: vec![
            field("id", "Identifiant stable du suivi", &f.id, "text", true),
            field("title", "Titre du suivi", &f.title, "text", true),
            why(&f.reason),
        ],
    }
}
fn edit_form(id: &str, s: &Snapshot, f: &Input) -> FormSpec {
    FormSpec {
        title: "Titre et état".into(),
        action: endpoint(id, Op::Edit),
        button: "Enregistrer le titre et l’état".into(),
        fields: vec![
            hidden("version", f.version),
            field("title", "Titre du suivi", &f.title, "text", true),
            select(
                "state",
                "État",
                if f.state.is_empty() {
                    match s.state {
                        ThreadState::Active => "active",
                        ThreadState::Closed => "closed",
                    }
                } else {
                    &f.state
                },
                &[("active", "Actif"), ("closed", "Clos")],
            ),
            why(&f.reason),
        ],
    }
}
fn member_form(id: &str, rows: &[StoryChoice], next: Option<&str>, f: &Input) -> FormSpec {
    let mut choices: Vec<_> = rows
        .iter()
        .take(20)
        .map(|r| (r.id.clone(), assignment_label(rows, &r.id)))
        .collect();
    if !f.story.is_empty() && !choices.iter().any(|(id, _)| *id == f.story) {
        choices.push((f.story.clone(), assignment_label(rows, &f.story)));
    }
    let mut fields = vec![
        hidden("version", f.version),
        picker("story", "Article à rattacher ou à dater", &f.story, choices),
        field(
            "when",
            "Instant connu avec décalage UTC (ex. 2026-11-14T10:00:00+01:00)",
            &f.when,
            "text",
            false,
        ),
        checkbox(
            "unknown",
            "Instant d’évolution explicitement inconnu",
            f.unknown,
        ),
        why(&f.reason),
        field(
            "lookup",
            "Préparer un article par son identifiant stable",
            &f.lookup,
            "text",
            false,
        ),
        pager("Préparer cet article sans rattacher", "find"),
        hidden("after", &f.after),
        pager("Début des articles sans rattacher", "first"),
    ];
    if let Some(id) = next {
        fields.push(pager(
            "Autres articles sans rattacher",
            format!("page:{id}"),
        ));
    }
    FormSpec {
        title: "Rattacher un article ou corriger son instant d’évolution".into(),
        action: endpoint(id, Op::Member),
        button: "Enregistrer le rattachement et l’instant".into(),
        fields,
    }
}
fn remove_form(id: &str, s: &Snapshot, rows: &[StoryChoice], f: &Input) -> FormSpec {
    let mut items: Vec<_> = s
        .members
        .iter()
        .map(|m| (m.story_id.clone(), story_label(rows, &m.story_id)))
        .collect();
    if !f.story.is_empty() && !items.iter().any(|(id, _)| *id == f.story) {
        items.push((f.story.clone(), story_label(rows, &f.story)));
    }
    FormSpec {
        title: "Retirer un article du suivi".into(),
        action: endpoint(id, Op::Remove),
        button: "Retirer le rattachement".into(),
        fields: vec![
            hidden("version", f.version),
            picker("story", "Article à retirer", &f.story, items),
            why(&f.reason),
        ],
    }
}
fn split_form(id: &str, s: &Snapshot, rows: &[StoryChoice], f: &Input) -> FormSpec {
    let ids: Vec<String> = if f.version == s.version {
        s.members.iter().map(|m| m.story_id.clone()).collect()
    } else {
        f.members.clone()
    };
    let mut fields = vec![
        hidden("version", f.version),
        field(
            "new_id",
            "Identifiant stable du nouveau suivi",
            &f.new_id,
            "text",
            true,
        ),
        field("title", "Titre du nouveau suivi", &f.title, "text", true),
        hidden("members", ids.len()),
    ];
    for (i, id) in ids.iter().enumerate() {
        fields.push(hidden(&format!("member_id_{i}"), id));
        fields.push(checkbox(
            &format!("member_{i}"),
            &story_label(rows, id),
            f.members.contains(id),
        ));
    }
    fields.push(why(&f.reason));
    FormSpec {
        title: "Séparer un sous-ensemble sans vider le suivi initial".into(),
        action: endpoint(id, Op::Split),
        button: "Créer le suivi et déplacer ces rattachements".into(),
        fields,
    }
}
async fn merge_picker(
    state: &AppState,
    session: &Session,
    id: &str,
    f: &Input,
) -> Result<FormSpec, AppError> {
    let s = service(state)?;
    let cards = s
        .cards(
            session,
            if f.after.is_empty() {
                None
            } else {
                Some(&f.after)
            },
            size(),
        )
        .await?;
    let mut items: Vec<_> = cards
        .items
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                format!(
                    "{} · {} · {}",
                    c.title,
                    c.id,
                    if c.merged_into.is_some() {
                        "Redirigé"
                    } else if c.state == "active" {
                        "Actif"
                    } else {
                        "Clos"
                    }
                ),
            )
        })
        .collect();
    if !f.target.is_empty() && !items.iter().any(|(id, _)| *id == f.target) {
        match s.get(session, &f.target).await {
            Ok(t) => items.push((t.id.clone(), format!("{} · {}", t.title, t.id))),
            Err(AppError::NotFound) => items.push((
                f.target.clone(),
                format!("Suivi introuvable : {}", f.target),
            )),
            Err(e) => return Err(e),
        }
    }
    let mut fields = vec![
        hidden("version", f.version),
        picker("target", "Suivi actif de destination", &f.target, items),
        why(&f.reason),
        hidden("after", &f.after),
        pager("Début des destinations sans fusionner", "first"),
    ];
    if let Some(id) = cards.next {
        fields.push(pager(
            "Autres destinations sans fusionner",
            format!("page:{id}"),
        ));
    }
    Ok(FormSpec {
        title: "Préparer une fusion explicite".into(),
        action: endpoint(id, Op::Prepare),
        button: "Examiner la fusion avant confirmation".into(),
        fields,
    })
}
fn merge_confirm_form(id: &str, f: &Input) -> FormSpec {
    FormSpec {
        title: "Confirmation des deux versions préparées".into(),
        action: endpoint(id, Op::Merge),
        button: "Confirmer la fusion des rattachements".into(),
        fields: vec![
            hidden("version", f.version),
            hidden("target", &f.target),
            hidden("target_version", f.target_version),
            why(&f.reason),
        ],
    }
}
fn member_table(state: &AppState, s: &Snapshot, rows: &[StoryChoice]) -> Table {
    table(
        &format!("Articles du suivi {}", s.title),
        &[
            "Article courant",
            "Instant d’évolution UTC",
            "Ajout au suivi (journal)",
        ],
        s.members
            .iter()
            .map(|m| {
                vec![
                    linked(
                        story_label(rows, &m.story_id),
                        format!("/admin/editor{}", path_segment("story", &m.story_id)),
                    ),
                    cell(
                        m.update_at
                            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
                            .unwrap_or_else(|| "Inconnu".into()),
                    ),
                    cell(stamp(state, m.added_at)),
                ]
            })
            .collect(),
    )
}
async fn detail_view(
    state: &AppState,
    session: &Session,
    id: &str,
    before: Option<i64>,
    own: Option<(Op, &Input)>,
) -> Result<View, AppError> {
    let service = service(state)?;
    let s = service.get(session, id).await?;
    let mut selected: BTreeSet<String> = s.members.iter().map(|m| m.story_id.clone()).collect();
    if let Some((_, f)) = own {
        selected.extend(f.members.iter().cloned());
        if !f.story.is_empty() {
            selected.insert(f.story.clone());
        }
    }
    let after = own
        .filter(|(op, _)| *op == Op::Member)
        .map(|(_, f)| f.after.as_str())
        .filter(|s| !s.is_empty());
    let choices = service
        .story_choices(session, &selected.into_iter().collect::<Vec<_>>(), after)
        .await?;
    let history = service.history(session, id, before, size()).await?;
    let mut v = page(state, session, format!("Suivi : {}", s.title));
    v.links.push(Link {
        label: "Tous les sujets suivis".into(),
        href: ROOT.into(),
    });
    v.paragraphs.push(format!("État : {} · Version lue : {} · {} articles, y compris privés. Ces commandes modifient les relations et le titre du suivi, jamais le texte ou la publication des articles.",state_label(s.state),s.version,s.members.len()));
    v.paragraphs.push("L’instant d’évolution est distinct de la publication et de l’ajout au suivi. S’il est inconnu, déclarez-le explicitement ; aucune date n’est déduite. Un article ne peut avoir qu’un sujet principal.".into());
    if let Some(target) = &s.merged_into {
        v.links.push(Link {
            label: format!("Identité redirigée : ouvrir la destination {target}"),
            href: path(target),
        });
    }
    if let Some((Op::Member, f)) = own
        && let Some(owner) = choices
            .items
            .iter()
            .find(|r| r.id == f.story)
            .and_then(|r| r.thread_id.as_deref())
        && owner != id
    {
        v.links.push(Link {
            label: "Ouvrir le suivi actuel de l’article choisi ; aucun rattachement n’est volé"
                .into(),
            href: path(owner),
        });
    }
    v.tables.push(member_table(state, &s, &choices.items));
    if history.len() == 20 {
        v.next = history
            .last()
            .map(|r| with_query(&path(id), &[("before", &r.id.to_string())]));
    }
    v.tables.push(table(
        "Historique audité",
        &["Action", "Compte", "Enregistré le", "Motif privé"],
        history
            .into_iter()
            .map(|r| {
                vec![
                    linked(
                        format!("#{} · {}", r.id, action_label(&r.action)),
                        action_path(r.id),
                    ),
                    cell(r.actor_id),
                    cell(stamp(state, r.created_at)),
                    cell(r.reason),
                ]
            })
            .collect(),
    ));
    if publisher(session) {
        let defaults = Input {
            version: s.version,
            title: s.title.clone(),
            ..Default::default()
        };
        if s.merged_into.is_none() {
            v.forms.push(edit_form(id, &s, &defaults));
            v.forms.push(member_form(
                id,
                &choices.items,
                choices.next.as_deref(),
                &Input {
                    version: s.version,
                    ..Default::default()
                },
            ));
            v.forms.push(remove_form(
                id,
                &s,
                &choices.items,
                &Input {
                    version: s.version,
                    ..Default::default()
                },
            ));
            if s.state == ThreadState::Active {
                v.forms.push(split_form(
                    id,
                    &s,
                    &choices.items,
                    &Input {
                        version: s.version,
                        ..Default::default()
                    },
                ));
                v.forms.push(
                    merge_picker(
                        state,
                        session,
                        id,
                        &Input {
                            version: s.version,
                            ..Default::default()
                        },
                    )
                    .await?,
                );
            }
        }
        if let Some((op, f)) = own {
            let form = match op {
                Op::Edit => edit_form(id, &s, f),
                Op::Member => member_form(id, &choices.items, choices.next.as_deref(), f),
                Op::Remove => remove_form(id, &s, &choices.items, f),
                Op::Split => split_form(id, &s, &choices.items, f),
                Op::Prepare => merge_picker(state, session, id, f).await?,
                Op::Merge => merge_confirm_form(id, f),
                Op::Create => return Err(AppError::InvalidData),
            };
            v.forms.retain(|old| old.action != form.action);
            v.forms.insert(0, form);
        }
    }
    Ok(v)
}
async fn list_view(
    state: &AppState,
    session: &Session,
    after: Option<&str>,
    own: Option<&Input>,
) -> Result<View, AppError> {
    let cards = service(state)?.cards(session, after, size()).await?;
    let mut v = page(state, session, "Sujets suivis · Curation privée");
    v.paragraphs.push("Un sujet stable rassemble des évolutions datées. Les rapprochements sont des décisions humaines explicites, jamais des fusions automatiques de titres similaires ou d’anciens liens.".into());
    v.tables.push(table(
        "Identités de suivi",
        &["Suivi", "État", "Articles privés et publics", "Version"],
        cards
            .items
            .iter()
            .map(|c| {
                vec![
                    linked(format!("{} · {}", c.title, c.id), path(&c.id)),
                    cell(if c.merged_into.is_some() {
                        "Redirigé"
                    } else if c.state == "active" {
                        "Actif"
                    } else {
                        "Clos"
                    }),
                    cell(c.members),
                    cell(c.version),
                ]
            })
            .collect(),
    ));
    v.next = cards.next.map(|id| with_query(ROOT, &[("after", &id)]));
    if publisher(session) {
        v.forms.push(create_form(own.unwrap_or(&Input::default())));
    }
    Ok(v)
}
async fn confirmation(
    state: &AppState,
    session: &Session,
    id: &str,
    f: &Input,
) -> Result<View, AppError> {
    let service = service(state)?;
    let source = service.get(session, id).await?;
    let target = service.get(session, &f.target).await?;
    let ids: BTreeSet<String> = source
        .members
        .iter()
        .chain(target.members.iter())
        .map(|m| m.story_id.clone())
        .collect();
    let rows = service
        .story_choices(session, &ids.into_iter().collect::<Vec<_>>(), None)
        .await?;
    let mut v = page(state, session, "Examiner la fusion des deux suivis");
    v.paragraphs.push(format!("{} ({id}, version préparée {}) vers {} ({}, version préparée {}). La source sera conservée comme identité redirigée ; les articles ne sont pas republiés.",source.title,f.version,target.title,f.target,f.target_version));
    v.paragraphs.push("Les listes ci-dessous décrivent les versions lues pour comparaison. Le serveur recontrôlera les deux versions avant de déplacer les rattachements. Une modification concurrente bloque cette confirmation.".into());
    v.links.push(Link {
        label: "Retour au suivi, sans fusionner".into(),
        href: path(id),
    });
    v.tables.push(member_table(state, &source, &rows.items));
    v.tables.push(member_table(state, &target, &rows.items));
    v.forms.push(merge_confirm_form(id, f));
    Ok(v)
}
fn take(m: &mut BTreeMap<String, String>, key: &str) -> Result<String, AppError> {
    m.remove(key)
        .ok_or(AppError::Validation("Formulaire de suivi incomplet."))
}
fn number(m: &mut BTreeMap<String, String>, key: &str) -> Result<i64, AppError> {
    take(m, key)?
        .parse::<i64>()
        .ok()
        .filter(|v| *v >= 0)
        .ok_or(AppError::Validation("Version de suivi invalide."))
}
fn checked(m: &mut BTreeMap<String, String>, key: &str) -> Result<bool, AppError> {
    match m.remove(key) {
        None => Ok(false),
        Some(v) if v == "yes" => Ok(true),
        _ => Err(AppError::Validation("Choix de suivi invalide.")),
    }
}
fn raw_map(raw: Result<RawForm, RawFormRejection>) -> Result<BTreeMap<String, String>, AppError> {
    let RawForm(bytes) = raw.map_err(|_| AppError::Validation("Formulaire de suivi invalide."))?;
    if !super::super::editor::strict_encoding(&bytes) {
        return Err(AppError::Validation("Encodage du suivi invalide."));
    }
    let mut m = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(&bytes) {
        if m.len() >= 512 || k.len() > 64 || m.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AppError::Validation(
                "Champs de suivi répétés ou trop nombreux.",
            ));
        }
    }
    take(&mut m, "csrf")?;
    Ok(m)
}
fn parse(raw: Result<RawForm, RawFormRejection>, op: Op) -> Result<Input, AppError> {
    let mut m = raw_map(raw)?;
    let mut f = Input {
        intent: m.remove("intent").unwrap_or_else(|| "save".into()),
        reason: take(&mut m, "reason")?,
        ..Default::default()
    };
    if op == Op::Create {
        f.id = take(&mut m, "id")?;
    } else {
        f.version = number(&mut m, "version")?;
    }
    match op {
        Op::Create => f.title = take(&mut m, "title")?,
        Op::Edit => {
            f.title = take(&mut m, "title")?;
            f.state = take(&mut m, "state")?;
        }
        Op::Member => {
            f.story = take(&mut m, "story")?;
            f.lookup = m.remove("lookup").unwrap_or_default();
            f.when = take(&mut m, "when")?;
            f.unknown = checked(&mut m, "unknown")?;
            f.after = take(&mut m, "after")?;
        }
        Op::Remove => f.story = take(&mut m, "story")?,
        Op::Prepare => {
            f.target = take(&mut m, "target")?;
            f.after = take(&mut m, "after")?;
        }
        Op::Merge => {
            f.target = take(&mut m, "target")?;
            f.target_version = number(&mut m, "target_version")?;
        }
        Op::Split => {
            f.new_id = take(&mut m, "new_id")?;
            f.title = take(&mut m, "title")?;
            let n = number(&mut m, "members")?;
            if n > 200 {
                return Err(AppError::Validation("Trop de membres."));
            }
            for i in 0..n {
                let id = take(&mut m, &format!("member_id_{i}"))?;
                if checked(&mut m, &format!("member_{i}"))? {
                    f.members.push(id);
                }
            }
        }
    }
    if !m.is_empty() {
        return Err(AppError::Validation("Champ de suivi inconnu."));
    }
    Ok(f)
}
async fn submit(
    state: AppState,
    session: Session,
    id: Option<String>,
    op: Op,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let mut f = match parse(raw, op) {
        Ok(f) => f,
        Err(e) => return error(&state, e),
    };
    let result = async {
        let service = service(&state)?;
        if op == Op::Create {
            if f.intent != "save" {return Err(AppError::Validation("Action inconnue."));}
            service.create(&session, &f.id, &f.title, &f.reason).await?;
            return Ok(Some(path(&f.id)));
        }
        let id = id.as_deref().ok_or(AppError::InvalidData)?;
        let current = service.get(&session, id).await?;
        if current.version != f.version {return Err(AppError::Conflict);}
        if matches!(op, Op::Member | Op::Prepare) && f.intent != "save" {
            match f.intent.as_str() {
                "find" if op == Op::Member => {
                    let choices=service.story_choices(&session,&[f.lookup.clone()],None).await?;
                    if !choices.items.iter().any(|c|c.id==f.lookup){return Err(AppError::Validation("Article introuvable ; aucun rattachement n’a été modifié."));}
                    f.story=f.lookup.clone();
                }
                "first" => f.after.clear(),
                value if value.starts_with("page:") && crate::content::valid_id(&value[5..]) => f.after = value[5..].into(),
                _ => return Err(AppError::Validation("Page de choix inconnue.")),
            }
            return Ok(None);
        }
        if f.intent != "save" {return Err(AppError::Validation("Action inconnue."));}
        match op {
            Op::Edit => {
                let next = match f.state.as_str() {
                    "active" => ThreadState::Active,
                    "closed" => ThreadState::Closed,
                    _ => return Err(AppError::Validation("État de suivi inconnu.")),
                };
                service.edit(&session, id, f.version, &f.title, next, &f.reason).await?;
            }
            Op::Member => {
                let instant = if f.unknown {
                    if !f.when.trim().is_empty() {return Err(AppError::Validation("Effacez la date pour confirmer un instant inconnu."));}
                    None
                } else {
                    Some(chrono::DateTime::parse_from_rfc3339(f.when.trim())
                        .map_err(|_| AppError::Validation("Saisissez un instant avec décalage UTC, ou choisissez explicitement inconnu."))?
                        .with_timezone(&chrono::Utc))
                };
                service.assign(&session, id, f.version, MemberInput {story_id:f.story.clone(),update_at:instant}, &f.reason).await?;
            }
            Op::Remove => {service.remove(&session, id, f.version, &f.story, &f.reason).await?;}
            Op::Split => {
                service.split(&session, Split {source:id,source_version:f.version,new_id:&f.new_id,title:&f.title,members:&f.members}, &f.reason).await?;
            }
            Op::Prepare => {
                let target = service.get(&session, &f.target).await?;
                if target.id == current.id || target.state != ThreadState::Active || current.state != ThreadState::Active
                    || current.merged_into.is_some() || target.merged_into.is_some() {
                    return Err(AppError::Validation("Choisissez deux suivis canoniques actifs et distincts."));
                }
                if current.members.len() + target.members.len() > 200 {
                    return Err(AppError::Validation("La fusion dépasserait la limite de 200 articles."));
                }
                f.target_version = target.version;
                return Ok(None);
            }
            Op::Merge => {
                service.merge(&session, Merge {source:id,source_version:f.version,target:&f.target,target_version:f.target_version}, &f.reason).await?;
                return Ok(Some(path(&f.target)));
            }
            Op::Create => return Err(AppError::InvalidData),
        }
        Ok(Some(path(id)))
    }.await;
    let (status, message) = match result {
        Ok(Some(url)) => return Redirect::to(&url).into_response(),
        Ok(None) => (StatusCode::OK, None),
        Err(AppError::Conflict) => (
            StatusCode::CONFLICT,
            Some(
                "L’état du graphe ou un travail concurrent empêche cet enregistrement. Votre saisie est conservée pour comparaison ; les versions préparées ne sont pas remplacées.".to_owned(),
            ),
        ),
        Err(AppError::Validation(text)) => (
            StatusCode::BAD_REQUEST,
            Some(format!("Changement non enregistré, saisie conservée. {text}")),
        ),
        Err(e) => return error(&state, e),
    };
    let Some(auth) = &state.auth else {
        return error(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, Permission::Publish).await {
        return super::super::failure(&state, e);
    }
    let view = if op == Op::Create {
        list_view(&state, &session, None, Some(&f)).await
    } else if (op == Op::Prepare && status == StatusCode::OK && f.intent == "save")
        || op == Op::Merge
    {
        confirmation(&state, &session, id.as_deref().expect("existing"), &f).await
    } else {
        detail_view(
            &state,
            &session,
            id.as_deref().expect("existing"),
            None,
            Some((op, &f)),
        )
        .await
    };
    respond(&state, view, status, message.as_deref())
}
fn respond(
    state: &AppState,
    result: Result<View, AppError>,
    status: StatusCode,
    message: Option<&str>,
) -> Response {
    match result {
        Ok(mut v) => {
            if let Some(message) = message {
                v.paragraphs.insert(0, message.into());
            }
            match v.render() {
                Ok(html) => (status, Html(html)).into_response(),
                Err(_) => error(state, AppError::Unavailable),
            }
        }
        Err(e) => error(state, e),
    }
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
async fn show(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    params: Result<Query<HistoryCursor>, QueryRejection>,
) -> Response {
    let result = async {
        let q = query(params)?;
        detail_view(&state, &session, &id, q.before, None).await
    }
    .await;
    render(&state, result)
}
async fn create(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    submit(state, session, None, Op::Create, raw).await
}
macro_rules! handler {
    ($name:ident,$op:ident) => {
        async fn $name(
            State(state): State<AppState>,
            Extension(session): Extension<Session>,
            Path(id): Path<String>,
            raw: Result<RawForm, RawFormRejection>,
        ) -> Response {
            submit(state, session, Some(id), Op::$op, raw).await
        }
    };
}
handler!(edit, Edit);
handler!(member, Member);
handler!(remove, Remove);
handler!(split, Split);
handler!(prepare, Prepare);
handler!(merge, Merge);
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(ROOT, get(list).post(create))
        .route("/admin/editor/threads/{id}", get(show))
        .route("/admin/editor/threads/{id}/edit", post(edit))
        .route("/admin/editor/threads/{id}/member", post(member))
        .route("/admin/editor/threads/{id}/remove", post(remove))
        .route("/admin/editor/threads/{id}/split", post(split))
        .route("/admin/editor/threads/{id}/merge-preview", post(prepare))
        .route("/admin/editor/threads/{id}/merge", post(merge))
        .route("/admin/editor/thread-actions/{id}", get(action).post(undo))
}
async fn inverse_view(
    state: &AppState,
    session: &Session,
    id: i64,
    own: Option<&UndoInput>,
) -> Result<View, AppError> {
    let service = service(state)?;
    let preview = service.inverse_preview(session, id).await?;
    let ids: BTreeSet<String> = preview
        .before
        .iter()
        .chain(preview.after.iter())
        .flat_map(|s| s.members.iter().map(|m| m.story_id.clone()))
        .collect();
    let rows = service
        .story_choices(session, &ids.into_iter().collect::<Vec<_>>(), None)
        .await?;
    let mut v = page(
        state,
        session,
        format!("Action #{id} · {}", action_label(&preview.action.action)),
    );
    v.paragraphs.push(format!(
        "Compte : {} · Enregistrée le {} · Motif privé : {}",
        preview.action.actor_id,
        stamp(state, preview.action.created_at),
        preview.action.reason
    ));
    v.paragraphs.push("Cette annulation porte sur les identités et rattachements, sans modifier les articles. Les titres d’articles affichés sont ceux des révisions courantes. Le serveur ne restaure que si tous les états et versions correspondent encore exactement.".into());
    for (label, graphs) in [
        ("Avant", &preview.before),
        ("Après enregistré", &preview.after),
    ] {
        for s in graphs {
            v.tables.push(table(
                label,
                &["Suivi", "Version enregistrée", "État", "Destination"],
                vec![vec![
                    linked(&s.title, path(&s.id)),
                    cell(s.version),
                    cell(state_label(s.state)),
                    s.merged_into
                        .as_deref()
                        .map(|id| linked(id, path(id)))
                        .unwrap_or_else(|| cell("Aucune")),
                ]],
            ));
            v.tables.push(member_table(state, s, &rows.items));
        }
    }
    if preview.eligible && publisher(session) {
        let fences: Vec<_> = preview
            .after
            .iter()
            .map(|s| (s.id.clone(), s.version))
            .collect();
        let fences = own.map(|o| &o.fences).unwrap_or(&fences);
        let mut fields = vec![hidden("fences", fences.len())];
        for (i, (id, version)) in fences.iter().enumerate() {
            fields.push(hidden(&format!("id_{i}"), id));
            fields.push(hidden(&format!("version_{i}"), version));
        }
        fields.push(why(own.map(|o| o.reason.as_str()).unwrap_or("")));
        v.forms.push(FormSpec {
            title: "Annulation auditée préparée".into(),
            action: action_path(id),
            button: "Annuler cette action si l’état est inchangé".into(),
            fields,
        });
    } else {
        v.paragraphs.push(if preview.eligible{"Seul un responsable de publication peut annuler cette action."}else{"Un état a changé ou cette action a déjà été annulée. Inspectez les suivis actuels et prenez une nouvelle décision explicite."}.into());
        if let Some(own) = own {
            v.tables.push(table(
                "Motif privé à récupérer",
                &["Saisie conservée"],
                vec![vec![cell(&own.reason)]],
            ));
        }
    }
    Ok(v)
}
async fn action(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
) -> Response {
    let result = inverse_view(&state, &session, id, None).await;
    render(&state, result)
}
async fn undo(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
    Path(id): Path<i64>,
    raw: Result<RawForm, RawFormRejection>,
) -> Response {
    let parsed = (|| {
        let mut m = raw_map(raw)?;
        let mut input = UndoInput {
            reason: take(&mut m, "reason")?,
            ..Default::default()
        };
        let n = number(&mut m, "fences")?;
        if !(1..=2).contains(&n) {
            return Err(AppError::Validation("Nombre de versions invalide."));
        }
        for i in 0..n {
            input.fences.push((
                take(&mut m, &format!("id_{i}"))?,
                number(&mut m, &format!("version_{i}"))?,
            ));
        }
        if !m.is_empty() {
            return Err(AppError::Validation("Champ d’annulation inconnu."));
        }
        Ok(input)
    })();
    let input = match parsed {
        Ok(input) => input,
        Err(e) => return error(&state, e),
    };
    let result = async {
        service(&state)?
            .undo(&session, id, &input.fences, &input.reason)
            .await
    }
    .await;
    let (status, message) = match result {
        Ok(m) => return Redirect::to(&path(&m.threads[0].id)).into_response(),
        Err(AppError::Conflict) => (
            StatusCode::CONFLICT,
            "Les états ne correspondent plus à l’action enregistrée. Votre motif est conservé ; aucune version n’a été écrasée.",
        ),
        Err(AppError::Validation(_)) => (
            StatusCode::BAD_REQUEST,
            "Annulation non enregistrée. Vérifiez le motif et les versions ; votre saisie est conservée.",
        ),
        Err(e) => return error(&state, e),
    };
    let Some(auth) = &state.auth else {
        return error(&state, AppError::Unavailable);
    };
    if let Err(e) = auth.authorise(&session, Permission::Publish).await {
        return super::super::failure(&state, e);
    }
    respond(
        &state,
        inverse_view(&state, &session, id, Some(&input)).await,
        status,
        Some(message),
    )
}
