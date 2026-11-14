use crate::{
    auth::AuthService,
    config::Config,
    content::{ArticleKind, ContentBlock, EditionCursor, PageSize, Revision, SourceLink, valid_id},
    error::AppError,
    persistence,
    repository::Repository,
};
use askama::Template;
use axum::{
    Extension, Json, Router,
    body::{Body, to_bytes},
    extract::{MatchedPath, Path, Request, State, rejection::PathRejection},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
};
use chrono::NaiveDate;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_BODY: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    pool: PgPool,
    request_timeout: Duration,
    auth: Option<Arc<AuthService>>,
}
#[derive(Clone)]
struct QueryValues(BTreeMap<String, String>);

#[derive(Template)]
#[template(path = "page.html")]
struct Page {
    masthead: String,
    title: String,
    canonical: Option<String>,
    message: Option<String>,
    items: Vec<Item>,
    next: Option<String>,
    article: Option<Revision>,
    sources: Vec<SourceLink>,
    legacy: bool,
}
struct Item {
    title: String,
    href: Option<String>,
    detail: String,
}
impl Page {
    fn new(config: &Config, title: impl Into<String>, path: Option<&str>) -> Self {
        Self {
            masthead: config.masthead.clone(),
            title: title.into(),
            canonical: path.map(|path| {
                // Only paths constructed by this module enter here; never trust Host/Forwarded.
                format!(
                    "{}{}",
                    config.public_url.as_str().trim_end_matches('/'),
                    path
                )
            }),
            message: None,
            items: vec![],
            next: None,
            article: None,
            sources: vec![],
            legacy: false,
        }
    }
}

pub fn router(config: Arc<Config>, pool: PgPool) -> Router {
    let editor_pool = persistence::editor_pool(&config);
    router_with_editor(config, pool, editor_pool)
}
pub fn router_with_editor(
    config: Arc<Config>,
    pool: PgPool,
    editor_pool: Option<PgPool>,
) -> Router {
    let auth = if config.admin_enabled() {
        editor_pool.map(|pool| Arc::new(AuthService::new(pool, &config.public_url)))
    } else {
        None
    };
    let state = AppState {
        config,
        pool,
        request_timeout: REQUEST_TIMEOUT,
        auth,
    };
    let admin_routes = admin::routes(state.clone());
    with_boundary(
        Router::new()
            .route("/", get(home))
            .route("/edition/{date}", get(edition))
            .route("/story/{id}", get(story))
            .route("/archive", get(archive))
            .route("/recherche", get(search))
            .route("/sujets", get(topics))
            .merge(admin_routes)
            .route("/health/live", get(live))
            .route("/health/ready", get(ready))
            .fallback(not_found)
            .method_not_allowed_fallback(method_not_allowed),
        state,
    )
}
fn with_boundary(routes: Router<AppState>, state: AppState) -> Router {
    routes
        .layer(middleware::from_fn_with_state(
            state.clone(),
            request_boundary,
        ))
        .with_state(state)
}

fn render(config: &Config, result: Result<Page, AppError>) -> Response {
    match result {
        Ok(page) => render_status(StatusCode::OK, page),
        Err(error) => match error {
            AppError::Validation(_) => message(
                config,
                StatusCode::BAD_REQUEST,
                "Requête invalide",
                "Vérifiez l’adresse et les paramètres de la requête.",
            ),
            AppError::NotFound => message(
                config,
                StatusCode::NOT_FOUND,
                "Page introuvable",
                "Cette ressource publiée est introuvable.",
            ),
            // A reader cannot fix a misconfigured DB role or malformed stored data.
            _ => message(
                config,
                StatusCode::SERVICE_UNAVAILABLE,
                "Service indisponible",
                "Les données sont temporairement indisponibles. Réessayez plus tard.",
            ),
        },
    }
}
fn render_status(status: StatusCode, page: Page) -> Response {
    match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Page indisponible.").into_response(),
    }
}
fn message(config: &Config, status: StatusCode, title: &str, text: &str) -> Response {
    let mut page = Page::new(config, title, None);
    page.message = Some(text.into());
    render_status(status, page)
}
async fn reader(state: &AppState) -> Result<Repository, AppError> {
    if !persistence::schema_available(&state.pool).await {
        return Err(AppError::Unavailable);
    }
    Ok(Repository::new(state.pool.clone()))
}

fn date(value: &str) -> Result<NaiveDate, AppError> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("Invalid edition date."))?;
    if value.len() != 10 || value < "0001-01-01" || parsed.format("%Y-%m-%d").to_string() != value {
        return Err(AppError::Validation("Invalid edition date."));
    }
    Ok(parsed)
}
fn path_segment(prefix: &str, id: &str) -> String {
    let mut url = url::Url::parse("https://route.invalid").expect("static origin");
    url.path_segments_mut()
        .expect("static base")
        .extend([prefix, id]);
    url.path().to_owned()
}
fn with_query(path: &str, pairs: &[(&str, &str)]) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().copied())
        .finish();
    format!("{path}?{query}")
}

async fn home(State(state): State<AppState>) -> Response {
    let result = async {
        let repository = reader(&state).await?;
        match repository
            .latest_edition(&state.config.editorial.publication_timezone)
            .await?
        {
            Some(edition) => edition_page(&state, &repository, edition.edition_date, "/").await,
            None => {
                let mut page = Page::new(&state.config, "Aucune édition publiée", Some("/"));
                page.message = Some("Le journal ne contient encore aucune édition publiée.".into());
                Ok(page)
            }
        }
    }
    .await;
    render(&state.config, result)
}
async fn edition(
    State(state): State<AppState>,
    path: Result<Path<String>, PathRejection>,
) -> Response {
    let result = async {
        let Path(value) = path.map_err(|_| AppError::Validation("Invalid path."))?;
        let day = date(&value)?;
        let repository = reader(&state).await?;
        edition_page(&state, &repository, day, &format!("/edition/{day}")).await
    }
    .await;
    render(&state.config, result)
}
async fn edition_page(
    state: &AppState,
    repository: &Repository,
    day: NaiveDate,
    canonical_path: &str,
) -> Result<Page, AppError> {
    let edition = repository
        .edition(day, &state.config.editorial.publication_timezone)
        .await?;
    let mut page = Page::new(
        &state.config,
        format!("Édition du {day}"),
        Some(canonical_path),
    );
    for entry in repository.edition_entries(&edition.id).await? {
        match entry.revision {
            Some(revision) => page.items.push(Item {
                title: revision.title,
                href: Some(with_query(
                    &path_segment("story", &entry.story_id),
                    &[("revision", &entry.revision_id)],
                )),
                detail: entry.section,
            }),
            None => page.items.push(Item {
                title: "Article retiré".into(),
                href: None,
                detail: entry.section,
            }),
        }
    }
    if page.items.is_empty() {
        page.message = Some("Cette édition ne contient aucun article.".into());
    }
    Ok(page)
}
async fn story(
    State(state): State<AppState>,
    Extension(query): Extension<QueryValues>,
    path: Result<Path<String>, PathRejection>,
) -> Response {
    let result = async {
        let Path(id) = path.map_err(|_| AppError::Validation("Invalid path."))?;
        if !valid_id(&id) {
            return Err(AppError::Validation("Invalid story ID."));
        }
        let revision_id = query.0.get("revision");
        if revision_id.is_some_and(|id| !valid_id(id)) {
            return Err(AppError::Validation("Invalid revision ID."));
        }
        let repository = reader(&state).await?;
        let revision = match revision_id {
            Some(id) => repository.revision(id).await?,
            None => repository.story(&id).await?,
        };
        if revision.story_id != id {
            return Err(AppError::NotFound);
        }
        let path = path_segment("story", &id);
        let canonical_path = revision_id
            .map(|id| with_query(&path, &[("revision", id)]))
            .unwrap_or(path);
        let mut page = Page::new(&state.config, revision.title.clone(), Some(&canonical_path));
        if revision_id.is_some() {
            page.message = Some("Vous consultez la révision conservée dans une édition. La version actuelle peut être différente.".into());
            page.items.push(Item { title: "Lire la version actuelle".into(), href: Some(path_segment("story", &id)), detail: String::new() });
        }
        page.sources = repository.sources(&revision.id).await?;
        page.legacy = matches!(
            revision.kind,
            ArticleKind::LegacySummary | ArticleKind::LegacySynthesis
        );
        page.article = Some(revision);
        Ok(page)
    }
    .await;
    render(&state.config, result)
}
async fn archive(
    State(state): State<AppState>,
    Extension(query): Extension<QueryValues>,
) -> Response {
    let result = async {
        let cursor = match (query.0.get("before_date"), query.0.get("before_id")) {
            (None, None) => None,
            (Some(day), Some(id)) if valid_id(id) => Some(EditionCursor {
                date: date(day)?,
                id: id.clone(),
            }),
            _ => return Err(AppError::Validation("Invalid archive cursor.")),
        };
        let path = cursor
            .as_ref()
            .map(|c| {
                with_query(
                    "/archive",
                    &[("before_date", &c.date.to_string()), ("before_id", &c.id)],
                )
            })
            .unwrap_or_else(|| "/archive".into());
        let mut page = Page::new(&state.config, "Archives", Some(&path));
        let editions = reader(&state)
            .await?
            .editions(
                &state.config.editorial.publication_timezone,
                cursor.as_ref(),
                PageSize::new(20)?,
            )
            .await?;
        page.items = editions
            .items
            .into_iter()
            .map(|edition| Item {
                title: edition.edition_date.to_string(),
                href: Some(format!("/edition/{}", edition.edition_date)),
                detail: String::new(),
            })
            .collect();
        page.next = editions.next.map(|c| {
            with_query(
                "/archive",
                &[("before_date", &c.date.to_string()), ("before_id", &c.id)],
            )
        });
        if page.items.is_empty() {
            page.message = Some("Aucune édition sur cette page d’archives.".into());
        }
        Ok(page)
    }
    .await;
    render(&state.config, result)
}
async fn search(
    State(state): State<AppState>,
    Extension(query): Extension<QueryValues>,
) -> Response {
    if query.0.get("q").is_some_and(|q| q.chars().count() > 200) {
        return render(
            &state.config,
            Err(AppError::Validation("Search query too long.")),
        );
    }
    message(
        &state.config,
        StatusCode::NOT_IMPLEMENTED,
        "Recherche en préparation",
        "La recherche dans les archives n’est pas encore disponible.",
    )
}
async fn topics(State(state): State<AppState>) -> Response {
    message(
        &state.config,
        StatusCode::NOT_IMPLEMENTED,
        "Sujets en préparation",
        "La navigation par sujet n’est pas encore disponible.",
    )
}
async fn not_found(State(state): State<AppState>) -> Response {
    render(&state.config, Err(AppError::NotFound))
}
async fn method_not_allowed(State(state): State<AppState>, request: Request) -> Response {
    let allow = match request.uri().path() {
        "/admin/login" => "GET, HEAD, POST",
        "/admin/logout" => "POST",
        _ => "GET, HEAD",
    };
    let mut response = message(
        &state.config,
        StatusCode::METHOD_NOT_ALLOWED,
        "Méthode non autorisée",
        "Cette adresse n’accepte pas la méthode utilisée.",
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}
async fn live() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    if persistence::schema_available(&state.pool).await {
        (StatusCode::OK, Json(json!({"status": "schema_ready"})))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "schema_unavailable"})),
        )
    }
}

fn valid_percent_encoding(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return false;
        }
    }
    true
}
fn parse_query(raw: Option<&str>) -> Result<QueryValues, AppError> {
    let raw = raw.unwrap_or("");
    if !valid_percent_encoding(raw) {
        return Err(AppError::Validation("Malformed query encoding."));
    }
    let mut values = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if key.len() > 64
            || value.len() > 2048
            || key.is_empty()
            || values.len() >= 32
            || key
                .chars()
                .chain(value.chars())
                .any(|c| c.is_control() || c == '\u{fffd}')
            || values
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(AppError::Validation("Invalid query parameters."));
        }
    }
    Ok(QueryValues(values))
}
fn canonical_path(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    if trimmed == path {
        return None;
    }
    if matches!(trimmed, "/archive" | "/recherche" | "/sujets") {
        return Some(trimmed);
    }
    let parts: Vec<_> = trimmed.split('/').collect();
    if parts.len() == 3 && matches!(parts[1], "story" | "edition") && !parts[2].is_empty() {
        Some(trimmed)
    } else {
        None
    }
}
async fn request_boundary(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let request_id = format!(
        "{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let start = Instant::now();
    let method = request.method().clone();
    let result = tokio::time::timeout(state.request_timeout, async {
        if request.uri().path().len() > 4096
            || request.uri().query().is_some_and(|q| q.len() > 4096)
        {
            return message(
                &state.config,
                StatusCode::URI_TOO_LONG,
                "Adresse trop longue",
                "L’adresse dépasse la limite autorisée.",
            );
        }
        if !valid_percent_encoding(request.uri().path()) {
            return render(
                &state.config,
                Err(AppError::Validation("Malformed path encoding.")),
            );
        }
        let query = match parse_query(request.uri().query()) {
            Ok(query) => query,
            Err(e) => return render(&state.config, Err(e)),
        };
        let (mut parts, body) = request.into_parts();
        if parts
            .headers
            .get(header::CONTENT_LENGTH)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|n| n > MAX_BODY as u64)
        {
            return message(
                &state.config,
                StatusCode::PAYLOAD_TOO_LARGE,
                "Requête trop volumineuse",
                "La requête dépasse la limite autorisée.",
            );
        }
        // Real streamed-body cap, including unknown routes; DefaultBodyLimit alone
        // would only apply when a handler actually consumes a body extractor.
        let body = match to_bytes(body, MAX_BODY).await {
            Ok(bytes) => Body::from(bytes),
            Err(error) => {
                use std::error::Error;
                let too_large = error
                    .source()
                    .is_some_and(|source| source.is::<http_body_util::LengthLimitError>());
                return message(
                    &state.config,
                    if too_large {
                        StatusCode::PAYLOAD_TOO_LARGE
                    } else {
                        StatusCode::BAD_REQUEST
                    },
                    "Corps de requête invalide",
                    "Le corps de la requête est invalide ou dépasse la limite autorisée.",
                );
            }
        };
        if matches!(parts.method, Method::GET | Method::HEAD)
            && let Some(path) = canonical_path(parts.uri.path())
        {
            let location = match parts.uri.query() {
                Some(query) => format!("{path}?{query}"),
                None => path.to_owned(),
            };
            return Redirect::permanent(&location).into_response();
        }
        parts.extensions.insert(query);
        next.run(Request::from_parts(parts, body)).await
    })
    .await;
    let mut response = result.unwrap_or_else(|_| {
        message(
            &state.config,
            StatusCode::SERVICE_UNAVAILABLE,
            "Délai dépassé",
            "La requête a dépassé le délai autorisé. Réessayez plus tard.",
        )
    });
    if method == Method::HEAD {
        *response.body_mut() = Body::empty();
    }
    let status = response.status();
    let headers = response.headers_mut();
    headers.insert(
        "x-request-id",
        HeaderValue::from_str(&request_id).expect("generated ASCII ID"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    // no-referrer makes Chromium send Origin:null on same-origin form POSTs.
    // Keep same-origin form provenance while withholding external referrers.
    headers.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    if status.is_client_error() || status.is_server_error() || route.starts_with("/admin") {
        headers.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    }
    if status == StatusCode::SERVICE_UNAVAILABLE {
        headers.insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    tracing::info!(
        request_id,
        route,
        method = match method {
            Method::GET => "GET",
            Method::HEAD => "HEAD",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
            Method::OPTIONS => "OPTIONS",
            _ => "OTHER",
        },
        status = status.as_u16(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "http request completed"
    );
    response
}

mod admin;
#[cfg(test)]
mod tests;
