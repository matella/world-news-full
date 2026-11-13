use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use cookie::Cookie;
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use world_news::{
    auth::{self, AuthError, AuthService, Role},
    config::Config,
    persistence, web,
};
use zeroize::Zeroizing;

const PASSWORD: &str = "synthetic correct passphrase only";
fn config(reader: &str, writer: Option<&str>) -> Config {
    Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(reader.into()),
        "PUBLIC_URL" => Some("https://news.invalid".into()),
        "NEWS_ADMIN_ENABLED" => Some(if writer.is_some() { "true" } else { "false" }.into()),
        "EDITOR_DATABASE_URL" => writer.map(str::to_owned),
        _ => None,
    })
    .unwrap()
}
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    cookies: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookies {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    app.clone()
        .oneshot(builder.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap()
}
async fn text(response: Response) -> String {
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap()
}
struct Browser {
    cookie: String,
    csrf: String,
    token: String,
}
async fn login(app: &Router, user: &str) -> Browser {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", user)
        .append_pair("password", PASSWORD)
        .finish();
    let response = request(
        app,
        "POST",
        "/admin/login?return_to=https://evil.invalid",
        Some("__Host-worldnews_session=attacker-fixed-value"),
        Some("https://news.invalid"),
        &body,
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/admin");
    let mut parts = vec![];
    let mut token = String::new();
    let mut csrf = String::new();
    for value in response.headers().get_all(header::SET_COOKIE) {
        let value = value.to_str().unwrap();
        assert!(
            value.contains("Secure")
                && value.contains("HttpOnly")
                && value.contains("SameSite=Strict")
                && value.contains("Path=/")
                && value.contains("Max-Age=28800")
        );
        assert!(!value.contains("Domain="));
        let cookie = Cookie::parse(value).unwrap();
        if cookie.name() == "__Host-worldnews_session" {
            token = cookie.value().into();
        }
        if cookie.name() == "__Host-worldnews_csrf" {
            csrf = cookie.value().into();
        }
        parts.push(format!("{}={}", cookie.name(), cookie.value()));
    }
    assert_eq!(parts.len(), 2);
    assert_eq!(token.len(), 64);
    assert_eq!(csrf.len(), 64);
    assert_ne!(token, csrf);
    Browser {
        cookie: parts.join("; "),
        csrf,
        token,
    }
}
async fn count(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM news.admin_audit WHERE action=$1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}
#[tokio::test]
#[ignore = "requires its own disposable DB; use scripts/test_database.py"]
async fn identity_sessions_csrf_and_role_boundaries_fail_closed() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let database_url = std::env::var("WORLDNEWS_TEST_DATABASE_URL").unwrap();
    let mut url = url::Url::parse(&database_url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.path().starts_with("/worldnews_disposable_") && url.path().ends_with("_auth"));
    assert_eq!(
        std::env::var("WORLDNEWS_TEST_ALLOW_RESET").as_deref(),
        Ok("1")
    );
    let owner = persistence::pool(&config(&database_url, None));
    assert!(!persistence::schema_available(&owner).await);
    persistence::migrate(&owner).await.unwrap();
    auth::bootstrap(
        &owner,
        "test-editor",
        Role::Editor,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    auth::bootstrap(
        &owner,
        "test-publisher",
        Role::Publisher,
        Zeroizing::new(PASSWORD.into()),
    )
    .await
    .unwrap();
    assert!(
        auth::bootstrap(
            &owner,
            "test-editor",
            Role::Publisher,
            Zeroizing::new(PASSWORD.into())
        )
        .await
        .is_err()
    );
    assert!(
        auth::bootstrap(
            &owner,
            "weak-user",
            Role::Publisher,
            Zeroizing::new("short".into())
        )
        .await
        .is_err()
    );
    let hashes =
        sqlx::query_scalar::<_, String>("SELECT password_hash FROM news.admin_users ORDER BY id")
            .fetch_all(&owner)
            .await
            .unwrap();
    assert_ne!(hashes[0], hashes[1]);
    assert!(
        hashes
            .iter()
            .all(|h| h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$") && !h.contains(PASSWORD))
    );
    assert_eq!(count(&owner, "account_created").await, 2);
    assert!(
        sqlx::query("UPDATE news.admin_audit SET action='login_failed'")
            .execute(&owner)
            .await
            .is_err()
    );
    sqlx::raw_sql("CREATE ROLE fixture_auth_reader LOGIN PASSWORD 'synthetic-auth-reader';
        GRANT USAGE ON SCHEMA news TO fixture_auth_reader;
        GRANT SELECT ON news.public_stories,news.public_revisions,news.public_editions,news.public_edition_entries,news.public_citations,news.public_notices,news.public_translations,news.schema_version TO fixture_auth_reader;
        CREATE ROLE fixture_auth_writer LOGIN PASSWORD 'synthetic-auth-writer';
        GRANT USAGE ON SCHEMA news TO fixture_auth_writer;
        GRANT SELECT ON news.schema_version,news.admin_users,news.admin_sessions TO fixture_auth_writer;
        GRANT UPDATE(failed_attempts,locked_until) ON news.admin_users TO fixture_auth_writer;
        GRANT INSERT ON news.admin_sessions,news.admin_audit TO fixture_auth_writer;
        GRANT UPDATE(revoked_at) ON news.admin_sessions TO fixture_auth_writer;
        GRANT USAGE ON SEQUENCE news.admin_audit_id_seq TO fixture_auth_writer;")
        .execute(&owner).await.unwrap();
    url.set_username("fixture_auth_reader").unwrap();
    url.set_password(Some("synthetic-auth-reader")).unwrap();
    let reader_url = url.to_string();
    url.set_username("fixture_auth_writer").unwrap();
    url.set_password(Some("synthetic-auth-writer")).unwrap();
    let writer_url = url.to_string();
    let config = Arc::new(config(&reader_url, Some(&writer_url)));
    let reader = persistence::pool(&config);
    let writer = persistence::editor_pool(&config).unwrap();
    assert!(
        sqlx::query("SELECT password_hash FROM news.admin_users")
            .fetch_all(&reader)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE news.admin_users SET role='publisher' WHERE id='test-editor'")
            .execute(&writer)
            .await
            .is_err()
    );
    assert!(
        auth::bootstrap(
            &writer,
            "escalation",
            Role::Publisher,
            Zeroizing::new(PASSWORD.into())
        )
        .await
        .is_err()
    );
    let app = web::router_with_editor(config, reader.clone(), Some(writer.clone()));
    assert_eq!(
        request(&app, "GET", "/", None, None, "").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", "/admin", None, None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", "/admin/unknown", None, None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/logout",
            None,
            Some("https://news.invalid"),
            ""
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = request(
        &app,
        "GET",
        "/admin/login?return_to=https://evil.invalid",
        None,
        None,
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = text(response).await;
    assert!(html.contains("autocomplete=\"current-password\"") && !html.contains("evil.invalid"));
    let credentials = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("username", "test-editor")
        .append_pair("password", PASSWORD)
        .finish();
    for origin in [
        None,
        Some("null"),
        Some("https://evil.invalid"),
        Some("https://news.invalid.evil.invalid"),
    ] {
        assert_eq!(
            request(&app, "POST", "/admin/login", None, origin, &credentials)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(count(&owner, "login_succeeded").await, 0);
    let injected = format!("{credentials}&role=publisher");
    let response = request(
        &app,
        "POST",
        "/admin/login",
        None,
        Some("https://news.invalid"),
        &injected,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!text(response).await.contains(PASSWORD));
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/login",
            None,
            Some("https://news.invalid"),
            "username=unknown&password=not-a-real-password"
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    for _ in 0..5 {
        assert_eq!(
            request(
                &app,
                "POST",
                "/admin/login",
                None,
                Some("https://news.invalid"),
                "username=test-editor&password=not-the-right-password"
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/login",
            None,
            Some("https://news.invalid"),
            &credentials
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT locked_until>now() FROM news.admin_users WHERE id='test-editor'"
        )
        .fetch_one(&owner)
        .await
        .unwrap()
    );
    sqlx::query(
        "UPDATE news.admin_users SET locked_until=now()-interval '1 second' WHERE id='test-editor'",
    )
    .execute(&owner)
    .await
    .unwrap();
    let editor = login(&app, "test-editor").await;
    let stored = sqlx::query_as::<_, (String, String)>(
        "SELECT token_hash,csrf_hash FROM news.admin_sessions WHERE user_id='test-editor'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_ne!(stored.0, editor.token);
    assert_ne!(stored.1, editor.csrf);
    let response = request(&app, "GET", "/admin", Some(&editor.cookie), None, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let html = text(response).await;
    assert!(
        html.contains("Rôle : éditeur")
            && html.contains(&editor.csrf)
            && !html.contains(&editor.token)
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/admin/publishing",
            Some(&editor.cookie),
            None,
            ""
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let publisher = login(&app, "test-publisher").await;
    assert_ne!(editor.token, publisher.token);
    assert_eq!(
        request(
            &app,
            "GET",
            "/admin/publishing",
            Some(&publisher.cookie),
            None,
            ""
        )
        .await
        .status(),
        StatusCode::OK
    );
    let csrf_body = format!("csrf={}", editor.csrf);
    let other_session_csrf = format!("csrf={}", publisher.csrf);
    for (origin, body) in [
        (Some("https://evil.invalid"), csrf_body.as_str()),
        (None, csrf_body.as_str()),
        (Some("https://news.invalid"), "csrf=incorrect"),
        (Some("https://news.invalid"), other_session_csrf.as_str()),
        (Some("https://news.invalid"), ""),
    ] {
        assert_eq!(
            request(
                &app,
                "POST",
                "/admin/logout",
                Some(&editor.cookie),
                origin,
                body
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
    }
    let duplicate = format!("{csrf_body}&{csrf_body}");
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/logout",
            Some(&editor.cookie),
            Some("https://news.invalid"),
            &duplicate
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "GET", "/admin", Some(&editor.cookie), None, "")
            .await
            .status(),
        StatusCode::OK
    );
    assert!(count(&owner, "authorization_denied").await >= 6);
    let tampered = format!(
        "{}; __Host-worldnews_session={}",
        editor.cookie, editor.token
    );
    assert_eq!(
        request(&app, "GET", "/admin", Some(&tampered), None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/logout")
                .header(header::COOKIE, &editor.cookie)
                .header(header::ORIGIN, "https://news.invalid")
                .header("x-csrf-token", &editor.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/admin/login");
    for value in response.headers().get_all(header::SET_COOKIE) {
        assert!(value.to_str().unwrap().contains("Max-Age=0"));
    }
    assert_eq!(
        request(&app, "GET", "/admin", Some(&editor.cookie), None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let expired = login(&app, "test-editor").await;
    sqlx::query("UPDATE news.admin_sessions SET created_at=now()-interval '9 hours',expires_at=now()-interval '1 hour' WHERE user_id='test-editor' AND revoked_at IS NULL").execute(&owner).await.unwrap();
    assert_eq!(
        request(&app, "GET", "/admin", Some(&expired.cookie), None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // Every request reads the current role, not a role stored in the cookie/session.
    sqlx::query("UPDATE news.admin_users SET role='editor' WHERE id='test-publisher'")
        .execute(&owner)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            "/admin/publishing",
            Some(&publisher.cookie),
            None,
            ""
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    auth::disable(&owner, "test-publisher").await.unwrap();
    assert_eq!(
        request(&app, "GET", "/admin", Some(&publisher.cookie), None, "")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", "/", None, None, "").await.status(),
        StatusCode::OK
    );
    let service = AuthService::new(
        writer.clone(),
        &url::Url::parse("https://news.invalid").unwrap(),
    );
    for _ in 0..30 {
        let _ = service
            .login("unknown", Zeroizing::new("synthetic wrong password".into()))
            .await;
    }
    assert!(matches!(
        service
            .login("unknown", Zeroizing::new(PASSWORD.into()))
            .await,
        Err(AuthError::Throttled)
    ));
    // Runtime neither creates identities nor controls roles/passwords.
    writer.close().await;
    assert_eq!(
        request(
            &app,
            "POST",
            "/admin/login",
            None,
            Some("https://news.invalid"),
            &credentials
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        request(&app, "GET", "/", None, None, "").await.status(),
        StatusCode::OK
    );
    reader.close().await;
    owner.close().await;
}
