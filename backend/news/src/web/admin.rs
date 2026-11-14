use super::*;
use crate::auth::{AuthError, Login, Permission, Role, Session};
use axum::{Form, extract::rejection::FormRejection, http::HeaderMap, routing::post};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginPage<'a> {
    masthead: &'a str,
}
#[derive(Template)]
#[template(path = "admin.html")]
struct AdminPage<'a> {
    masthead: &'a str,
    username: &'a str,
    csrf: &'a str,
    publisher: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginInput {
    username: String,
    password: String,
}

pub(super) fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/admin", get(dashboard))
        .route("/admin/login", get(login_form).post(login))
        .route("/admin/logout", post(logout))
        .route("/admin/publishing", get(publishing))
        .route("/admin/{*rest}", get(super::not_found))
        .route_layer(middleware::from_fn_with_state(state, guard))
}
async fn guard(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let Some(auth) = &state.auth else {
        return failure(&state, AuthError::Unavailable);
    };
    let unsafe_method = !matches!(*request.method(), Method::GET | Method::HEAD);
    if request.uri().path() == "/admin/login" {
        if unsafe_method && let Err(error) = auth.verify_origin(request.headers()) {
            return failure(&state, error);
        }
        return next.run(request).await;
    }
    let session = match auth.authenticate(request.headers()).await {
        Ok(session) => session,
        Err(error) => return failure(&state, error),
    };
    let permission = if request.uri().path() == "/admin/publishing" {
        Permission::Publish
    } else {
        Permission::Edit
    };
    if let Err(error) = auth.authorise(&session, permission).await {
        return failure(&state, error);
    }
    if unsafe_method {
        let (parts, body) = request.into_parts();
        let bytes = match to_bytes(body, MAX_BODY).await {
            Ok(bytes) => bytes,
            Err(_) => return failure(&state, AuthError::InvalidInput),
        };
        let token = csrf_token(&parts.headers, &bytes);
        if token
            .as_deref()
            .map(|token| auth.verify_csrf(&parts.headers, &session, token))
            .unwrap_or(Err(AuthError::Forbidden))
            .is_err()
        {
            let error = auth
                .record_denial(&session)
                .await
                .err()
                .unwrap_or(AuthError::Forbidden);
            return failure(&state, error);
        }
        request = Request::from_parts(parts, Body::from(bytes));
    }
    request.extensions_mut().insert(session);
    next.run(request).await
}
fn csrf_token(headers: &HeaderMap, body: &[u8]) -> Option<Zeroizing<String>> {
    let mut values = headers.get_all("x-csrf-token").iter();
    if let Some(value) = values.next() {
        if values.next().is_some() {
            return None;
        }
        return value.to_str().ok().map(|v| Zeroizing::new(v.to_owned()));
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        != Some("application/x-www-form-urlencoded")
    {
        return None;
    }
    let mut token = None;
    for (key, value) in url::form_urlencoded::parse(body) {
        if key == "csrf" {
            if token.is_some() {
                return None;
            }
            token = Some(Zeroizing::new(value.into_owned()));
        }
    }
    token
}
fn cookies(auth: &AuthService, response: &mut Response, login: Option<&Login>) {
    for cookie in auth.cookies(login) {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).expect("opaque ASCII cookies"),
        );
    }
}
fn failure(state: &AppState, error: AuthError) -> Response {
    let (status, title, text) = match error {
        AuthError::Unauthenticated => (
            StatusCode::UNAUTHORIZED,
            "Connexion requise",
            "Connectez-vous pour accéder à l’administration.",
        ),
        AuthError::Forbidden => (
            StatusCode::FORBIDDEN,
            "Accès refusé",
            "Votre compte ou la vérification de cette requête ne permet pas cette action.",
        ),
        AuthError::Throttled => (
            StatusCode::TOO_MANY_REQUESTS,
            "Trop de tentatives",
            "Réessayez plus tard.",
        ),
        AuthError::InvalidInput => (
            StatusCode::BAD_REQUEST,
            "Requête invalide",
            "Vérifiez les informations du formulaire.",
        ),
        AuthError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Administration indisponible",
            "L’espace de publication est temporairement indisponible.",
        ),
    };
    let mut page = Page::new(&state.config, title, None);
    page.message = Some(text.into());
    if error == AuthError::Unauthenticated {
        page.items.push(Item {
            title: "Se connecter".into(),
            href: Some("/admin/login".into()),
            detail: String::new(),
        });
    }
    let mut response = render_status(status, page);
    if error == AuthError::Unauthenticated
        && let Some(auth) = &state.auth
    {
        cookies(auth, &mut response, None);
    }
    if error == AuthError::Throttled {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    response
}
async fn login_form(State(state): State<AppState>) -> Response {
    match (LoginPage {
        masthead: &state.config.masthead,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(_) => failure(&state, AuthError::Unavailable),
    }
}
async fn login(
    State(state): State<AppState>,
    form: Result<Form<LoginInput>, FormRejection>,
) -> Response {
    let Some(auth) = &state.auth else {
        return failure(&state, AuthError::Unavailable);
    };
    let Ok(Form(input)) = form else {
        return failure(&state, AuthError::InvalidInput);
    };
    match auth
        .login(&input.username, Zeroizing::new(input.password))
        .await
    {
        Ok(login) => {
            // Fixed redirect: never accept a return URL, Host or forwarded origin.
            let mut response = Redirect::to("/admin").into_response();
            cookies(auth, &mut response, Some(&login));
            response
        }
        Err(error) => failure(&state, error),
    }
}
async fn dashboard(
    State(state): State<AppState>,
    Extension(session): Extension<Session>,
) -> Response {
    match (AdminPage {
        masthead: &state.config.masthead,
        username: session.principal().id(),
        csrf: session.csrf(),
        publisher: session.principal().role() == Role::Publisher,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(_) => failure(&state, AuthError::Unavailable),
    }
}
async fn publishing(State(state): State<AppState>) -> Response {
    message(
        &state.config,
        StatusCode::OK,
        "Espace de publication",
        "Votre compte permet la publication. Le parcours de révision et de publication est encore en préparation.",
    )
}
async fn logout(State(state): State<AppState>, Extension(session): Extension<Session>) -> Response {
    let Some(auth) = &state.auth else {
        return failure(&state, AuthError::Unavailable);
    };
    match auth.logout(&session).await {
        Ok(()) => {
            let mut response = Redirect::to("/admin/login").into_response();
            cookies(auth, &mut response, None);
            response
        }
        Err(error) => failure(&state, error),
    }
}
